//! Explicit daemon start as a convergence protocol.
//!
//! Every caller of `daemon start` either reaches the daemon that already owns
//! the target or waits for the one client that was elected to create it. The
//! elected client holds the startup-election lock through a responsive ping or
//! a terminal failure; it never lends that lock to the child it spawns, and the
//! lock is never ownership. Existing-only `daemon ping` takes no part in this:
//! it probes, reports, and changes nothing.

use std::io::{Read as _, Seek as _, SeekFrom};
use std::path::{Path, PathBuf};
use std::process::Child;
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

/// File-name suffix of the log a started daemon's diagnostic stream goes to.
///
/// The child's diagnostic stream is the only place it can say why it did not
/// come up, and it has no terminal. The elected client truncates this file,
/// hands it to the child, and quotes its end when the start fails.
pub const STARTUP_LOG_SUFFIX: &str = ".startup.log";

/// How many bytes from the end of a startup log a failure quotes.
pub const QUOTED_STARTUP_LOG_BYTES: u64 = 4096;

/// Permission bits of a startup log only its owner may read or write.
#[cfg(unix)]
const OWNER_ONLY_FILE_MODE: u32 = 0o600;

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
    /// The runtime root is not one a daemon may own a namespace in.
    #[error("the runtime root cannot be used: {0}")]
    RuntimeRoot(NamespaceFailure),
    /// The daemon could not be started.
    #[error("the daemon could not be started: {0}")]
    Unstartable(String),
    /// The daemon this caller started exited before it became responsive.
    #[error(
        "the daemon exited before it became responsive ({status}){}",
        quoted(.diagnostic, .log)
    )]
    DaemonExited {
        /// How it exited, as the operating system reports it.
        status: String,
        /// The end of what it wrote to its diagnostic stream.
        diagnostic: String,
        /// Where the whole of what it wrote is kept.
        log: PathBuf,
    },
    /// The start did not converge inside the contract's total deadline.
    #[error("no daemon became responsive within {deadline:?}{}", quoted(.diagnostic, .log))]
    DeadlineElapsed {
        /// The contract's total deadline.
        deadline: Duration,
        /// The end of what the starting daemon wrote to its diagnostic stream.
        diagnostic: String,
        /// Where the whole of what it wrote is kept.
        log: PathBuf,
    },
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

/// Renders what a daemon wrote while starting, and where to read the rest.
fn quoted(diagnostic: &str, log: &Path) -> String {
    let log = log.display();
    if diagnostic.is_empty() {
        format!(
            "; it wrote nothing to {log}. Run the same command line with `daemon serve` in \
             place of `daemon start` to watch it start in the foreground"
        )
    } else {
        format!("; it wrote: {diagnostic} (the whole startup log is {log})")
    }
}

/// Returns where the startup log of one runtime namespace lives.
#[must_use]
pub fn startup_log_path(runtime_root: &Path, namespace_digest: &str) -> PathBuf {
    runtime_root.join(format!("{namespace_digest}{STARTUP_LOG_SUFFIX}"))
}

/// Returns the end of a startup log, or nothing when there is none to read.
///
/// Only the end is quoted: a failure is the last thing a daemon says, and a
/// diagnostic line holding a whole log would bury it.
#[must_use]
pub fn startup_log_tail(log: &Path) -> String {
    let Ok(mut file) = std::fs::File::open(log) else {
        return String::new();
    };
    let length = file.metadata().map(|metadata| metadata.len()).unwrap_or_default();
    let start = length.saturating_sub(QUOTED_STARTUP_LOG_BYTES);
    let mut bytes = Vec::new();
    if file.seek(SeekFrom::Start(start)).is_err() || file.read_to_end(&mut bytes).is_err() {
        return String::new();
    }
    String::from_utf8_lossy(&bytes).trim().to_owned()
}

/// Opens a fresh startup log that only this user can read.
fn fresh_startup_log(log: &Path) -> Result<std::fs::File, StartFailure> {
    let mut options = std::fs::OpenOptions::new();
    options.write(true).create(true).truncate(true);
    #[cfg(unix)]
    std::os::unix::fs::OpenOptionsExt::mode(&mut options, OWNER_ONLY_FILE_MODE);
    options.open(log).map_err(|failure| {
        StartFailure::Unstartable(format!(
            "its startup log {} could not be opened: {failure}",
            log.display()
        ))
    })
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

/// Starts the daemon child that will own one target, writing to `log`.
///
/// The handle is returned so the caller can notice the child exiting before it
/// became responsive, which is the only way a failure it cannot otherwise
/// report ever reaches the person who asked for the start.
fn spawn_daemon(
    executable: &Path,
    target: &TargetRuntime,
    log: &Path,
) -> Result<Child, StartFailure> {
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
    let diagnostics = fresh_startup_log(log)?;
    crate::platform_runtime::detached_child::spawn_detached(executable, &arguments, diagnostics)
        .map_err(|failure| {
            StartFailure::Unstartable(format!(
                "{} could not be run: {failure}",
                executable.display()
            ))
        })
}

/// Returns why a started child is gone, when it is gone for a reason.
///
/// A child that exits because another daemon already owns the namespace has
/// lost a race rather than failed, so waiting continues for the owner that won
/// it and this reports nothing.
fn departure(child: &mut Child, log: &Path) -> Result<Option<StartFailure>, StartFailure> {
    let exited = child.try_wait().map_err(|failure| {
        StartFailure::Unstartable(format!("the started daemon could not be observed: {failure}"))
    })?;
    Ok(exited
        .filter(|status| status.code() != Some(i32::from(crate::command_line::EXIT_ALREADY_OWNED)))
        .map(|status| StartFailure::DaemonExited {
            status: status.to_string(),
            diagnostic: startup_log_tail(log),
            log: log.to_path_buf(),
        }))
}

/// Returns the failure a start that ran out of time reports.
fn deadline_elapsed(contract: &FoundationContract, log: &Path) -> StartFailure {
    StartFailure::DeadlineElapsed {
        deadline: contract.startup.explicit_start_total(),
        diagnostic: startup_log_tail(log),
        log: log.to_path_buf(),
    }
}

/// Waits for a daemon to become responsive, holding whatever this caller holds.
///
/// A child this caller started is watched while it waits, so one that exits is
/// reported at once with what it wrote rather than after the whole deadline.
async fn await_responsive(
    contract: &FoundationContract,
    address: &EndpointAddress,
    request_identifier: &str,
    deadline: Instant,
    mut started: Option<(Child, &Path)>,
) -> Result<Option<PingResult>, StartFailure> {
    loop {
        if let Some(result) = probe(contract, address, request_identifier).await? {
            return Ok(Some(result));
        }
        if let Some((child, log)) = started.as_mut()
            && let Some(failure) = departure(child, log)?
        {
            return Err(failure);
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
/// The runtime root is refused before anything is contended for when it is not
/// a directory this user alone owns, because the daemon refuses to own a
/// namespace in one and the refusal would otherwise happen where nobody sees it.
///
/// # Errors
///
/// Returns [`StartFailure`] when the target does not name a namespace, the
/// runtime root is not private to this user, the runtime state cannot be used,
/// the daemon cannot be started or exits before it becomes responsive, no
/// daemon becomes responsive inside the contract's total deadline, or the
/// daemon reports a readiness nonce that is not well formed.
pub async fn explicit_start(
    contract: &FoundationContract,
    target: &TargetRuntime,
    executable: &Path,
    request_identifier: &str,
) -> Result<StartReport, StartFailure> {
    let (namespace, address) = resolve(contract, target)?;
    namespace.create_runtime_directory().map_err(StartFailure::RuntimeRoot)?;
    let log = startup_log_path(namespace.runtime_root(), namespace.digest());
    let deadline = Instant::now() + contract.startup.explicit_start_total();
    loop {
        if let Some(result) = probe(contract, &address, request_identifier).await? {
            return Ok(report(StartDisposition::Joined, target, &result));
        }
        let elected = StartupElectionLock::acquire(namespace.runtime_root(), namespace.digest())?;
        let Some(election) = elected else {
            if Instant::now() >= deadline {
                return Err(deadline_elapsed(contract, &log));
            }
            tokio::time::sleep(contract.startup.start_retry_maximum_delay()).await;
            continue;
        };
        if let Some(result) = probe(contract, &address, request_identifier).await? {
            drop(election);
            return Ok(report(StartDisposition::Joined, target, &result));
        }
        let started = if owner_is_absent(&namespace)? {
            Some((spawn_daemon(executable, target, &log)?, log.as_path()))
        } else {
            None
        };
        let observed =
            await_responsive(contract, &address, request_identifier, deadline, started).await;
        drop(election);
        return match observed? {
            Some(result) => Ok(report(StartDisposition::Started, target, &result)),
            None => Err(deadline_elapsed(contract, &log)),
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
