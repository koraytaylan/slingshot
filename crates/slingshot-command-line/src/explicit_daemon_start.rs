//! Explicit daemon start as a convergence protocol.
//!
//! Every caller of `daemon start` either reaches the daemon that already owns
//! the target or waits for the one client that was elected to create it. The
//! elected client holds the startup-election lock through a responsive ping or
//! a terminal failure; it never lends that lock to the child it spawns, and the
//! lock is never ownership. Existing-only `daemon ping` takes no part in this:
//! it probes, reports, and changes nothing.

use std::path::{Path, PathBuf};
use std::time::Duration;

use serde::{Deserialize, Serialize};
use slingshot_daemon::platform_runtime::endpoint::{self, EndpointAddress};
use slingshot_daemon::platform_runtime::failure::PlatformFailure;
use slingshot_daemon::platform_runtime::locks::{OwnerLock, StartupElectionLock};
use slingshot_daemon::runtime_namespace::{NamespaceFailure, RuntimeNamespace};
use slingshot_local_protocol::foundation_contract::FoundationContract;
use slingshot_local_protocol::ping::{PingResult, nonce_is_well_formed};
use tokio::time::Instant;

use crate::daemon_connection::{self, ExchangeFailure};

/// Internal subcommand a spawned daemon child is started with.
pub const DAEMON_SERVE_COMMAND: &str = "serve";

/// How a start caller reached the daemon that owns its target.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StartDisposition {
    /// A daemon already owned the target, so nothing was created.
    Joined,
    /// This caller was elected and created the daemon.
    Started,
}

/// The structured result of one explicit start.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StartReport {
    /// How this caller reached the daemon.
    pub disposition: StartDisposition,
    /// Profile half of the target.
    pub profile: String,
    /// Environment half of the target.
    pub environment: String,
    /// Process identifier of the daemon, as a diagnostic and never as authority.
    pub process_identifier: u32,
    /// Live readiness nonce the daemon reported.
    pub readiness_nonce: String,
}

/// The structured result of one existing-owner probe.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PingReport {
    /// Whether a daemon owns the target right now.
    pub running: bool,
    /// Profile half of the target.
    pub profile: String,
    /// Environment half of the target.
    pub environment: String,
    /// Process identifier of the daemon, when one is running.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub process_identifier: Option<u32>,
    /// Live readiness nonce, when a daemon is running.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub readiness_nonce: Option<String>,
}

/// Reason an explicit start or an existing-owner probe could not finish.
#[derive(Debug, thiserror::Error)]
pub enum StartFailure {
    /// The target does not name a runtime namespace.
    #[error("the target does not name a runtime namespace: {0}")]
    Target(#[from] NamespaceFailure),
    /// The runtime state could not be prepared or read.
    #[error("the runtime state could not be used: {0}")]
    Runtime(#[from] PlatformFailure),
    /// The daemon could not be started.
    #[error("the daemon could not be started: {0}")]
    Unstartable(String),
    /// The start did not converge inside the contract's total deadline.
    #[error("no daemon became responsive within {0:?}")]
    DeadlineElapsed(Duration),
    /// The daemon reported a readiness nonce that is not well formed.
    #[error("the daemon reported the readiness nonce {0:?}, which is not well formed")]
    InvalidReadinessNonce(String),
    /// The daemon refused the probe.
    #[error("the daemon refused the probe: {0}")]
    Refused(String),
}

/// Everything one explicit start or probe needs to reach its target.
#[derive(Debug, Clone)]
pub struct TargetRuntime {
    /// Runtime root the namespace's objects live in.
    pub runtime_root: PathBuf,
    /// Profile half of the target.
    pub profile: String,
    /// Environment half of the target.
    pub environment: String,
}

/// Resolves the namespace and endpoint of one target.
fn resolve(
    contract: &FoundationContract,
    target: &TargetRuntime,
) -> Result<(RuntimeNamespace, EndpointAddress), StartFailure> {
    let namespace = RuntimeNamespace::name(
        contract,
        &target.runtime_root,
        &target.profile,
        &target.environment,
    )?;
    let address = endpoint::endpoint_address(contract, &target.runtime_root, namespace.digest())?;
    Ok((namespace, address))
}

/// Probes the endpoint for a daemon that is already serving.
async fn probe(
    contract: &FoundationContract,
    address: &EndpointAddress,
    request_identifier: &str,
) -> Result<Option<PingResult>, StartFailure> {
    match daemon_connection::ping(contract, address, request_identifier).await {
        Ok(result) => {
            if nonce_is_well_formed(contract, &result.readiness_nonce) {
                Ok(Some(result))
            } else {
                Err(StartFailure::InvalidReadinessNonce(result.readiness_nonce))
            }
        }
        Err(ExchangeFailure::Absent(_)) => Ok(None),
        Err(ExchangeFailure::Refused { code, message }) => {
            Err(StartFailure::Refused(format!("{code}: {message}")))
        }
        Err(other) => Err(StartFailure::Refused(other.to_string())),
    }
}

/// Reports whether no process holds the owner lock of a namespace.
///
/// The probe takes the lock and releases it at once. Holding it is the only
/// proof that nobody else does, and releasing it immediately leaves the daemon
/// this caller is about to create free to take it.
fn owner_is_absent(namespace: &RuntimeNamespace) -> Result<bool, PlatformFailure> {
    let probed = OwnerLock::acquire(namespace.runtime_root(), namespace.digest())?;
    Ok(probed.is_some())
}

/// Starts the daemon child that will own one target.
fn spawn_daemon(executable: &Path, target: &TargetRuntime) -> Result<(), StartFailure> {
    let arguments = vec![
        "--runtime-root".to_owned(),
        target.runtime_root.display().to_string(),
        "--profile".to_owned(),
        target.profile.clone(),
        "--environment".to_owned(),
        target.environment.clone(),
        "daemon".to_owned(),
        DAEMON_SERVE_COMMAND.to_owned(),
    ];
    crate::platform_runtime::detached_child::spawn_detached(executable, &arguments)
        .map(|_detached| ())
        .map_err(|failure| StartFailure::Unstartable(failure.to_string()))
}

/// Waits for a daemon to become responsive, holding whatever this caller holds.
async fn await_responsive(
    contract: &FoundationContract,
    address: &EndpointAddress,
    request_identifier: &str,
    deadline: Instant,
) -> Result<Option<PingResult>, StartFailure> {
    loop {
        if let Some(result) = probe(contract, address, request_identifier).await? {
            return Ok(Some(result));
        }
        if Instant::now() >= deadline {
            return Ok(None);
        }
        tokio::time::sleep(contract.startup.start_retry_maximum_delay()).await;
    }
}

/// Reaches the daemon that owns one target, creating it if nobody has.
///
/// The caller prepares the runtime root, connects first, then contends for the
/// startup-election lock, then rechecks, and only the elected caller spawns, and
/// only once, and only after the owner lock proves absence. Every caller returns
/// the same live nonce.
///
/// # Errors
///
/// Returns [`StartFailure`] when the target does not name a namespace, the
/// runtime state cannot be used, the daemon cannot be started, no daemon
/// becomes responsive inside the contract's total deadline, or the daemon
/// reports a readiness nonce that is not well formed.
pub async fn explicit_start(
    contract: &FoundationContract,
    target: &TargetRuntime,
    executable: &Path,
    request_identifier: &str,
) -> Result<StartReport, StartFailure> {
    slingshot_daemon::platform_runtime::current_user::create_owner_only_directory(
        &target.runtime_root,
    )?;
    let (namespace, address) = resolve(contract, target)?;
    let deadline = Instant::now() + contract.startup.explicit_start_total();
    loop {
        if let Some(result) = probe(contract, &address, request_identifier).await? {
            return Ok(report(StartDisposition::Joined, target, &result));
        }
        let elected = StartupElectionLock::acquire(namespace.runtime_root(), namespace.digest())?;
        let Some(election) = elected else {
            if Instant::now() >= deadline {
                return Err(StartFailure::DeadlineElapsed(contract.startup.explicit_start_total()));
            }
            tokio::time::sleep(contract.startup.start_retry_maximum_delay()).await;
            continue;
        };
        if let Some(result) = probe(contract, &address, request_identifier).await? {
            drop(election);
            return Ok(report(StartDisposition::Joined, target, &result));
        }
        if owner_is_absent(&namespace)? {
            spawn_daemon(executable, target)?;
        }
        let observed = await_responsive(contract, &address, request_identifier, deadline).await?;
        drop(election);
        return match observed {
            Some(result) => Ok(report(StartDisposition::Started, target, &result)),
            None => Err(StartFailure::DeadlineElapsed(contract.startup.explicit_start_total())),
        };
    }
}

/// Builds the structured result of one explicit start.
fn report(
    disposition: StartDisposition,
    target: &TargetRuntime,
    result: &PingResult,
) -> StartReport {
    StartReport {
        disposition,
        profile: target.profile.clone(),
        environment: target.environment.clone(),
        process_identifier: result.process_identifier,
        readiness_nonce: result.readiness_nonce.clone(),
    }
}

/// Reports whether a daemon already owns one target.
///
/// The probe never contends for the startup-election lock, never spawns, and
/// never waits for readiness. Absence and a record a departed owner left behind
/// are both reported as not running, and neither changes any runtime state.
///
/// # Errors
///
/// Returns [`StartFailure`] when the target does not name a namespace, the
/// endpoint cannot be named, or a listening daemon refuses the probe.
pub async fn existing_only_ping(
    contract: &FoundationContract,
    target: &TargetRuntime,
    request_identifier: &str,
) -> Result<PingReport, StartFailure> {
    let (_, address) = resolve(contract, target)?;
    let observed = probe(contract, &address, request_identifier).await?;
    Ok(PingReport {
        running: observed.is_some(),
        profile: target.profile.clone(),
        environment: target.environment.clone(),
        process_identifier: observed.as_ref().map(|result| result.process_identifier),
        readiness_nonce: observed.map(|result| result.readiness_nonce),
    })
}
