//! The internal daemon process entry point.
//!
//! The entry point receives its runtime root and its target, takes ownership of
//! that namespace, binds the endpoint, publishes readiness only once the
//! endpoint can answer, and serves until a stop carrying the live nonce arrives.
//! A process that finds the namespace already owned exits without binding, so a
//! second daemon for one target can never exist.

use std::path::{Path, PathBuf};

use slingshot_daemon::local_server::{self, LocalListener};
use slingshot_daemon::ownership::{Acquisition, DaemonOwnership};
use slingshot_daemon::platform_runtime::{current_user, endpoint};
use slingshot_daemon::runtime_namespace::RuntimeNamespace;
use slingshot_daemon::service::DaemonService;
use slingshot_local_protocol::foundation_contract::FoundationContract;
use tokio_util::sync::CancellationToken;

/// Why a daemon process ended.
#[derive(Debug, PartialEq, Eq)]
pub enum DaemonEntryOutcome {
    /// The daemon served its namespace and then stopped in order.
    Served,
    /// Another live daemon already owned the namespace.
    AlreadyOwned,
}

/// Why a daemon process could not start.
#[derive(Debug, thiserror::Error)]
pub enum DaemonEntryFailure {
    /// The target does not name a runtime namespace.
    #[error("the target does not name a runtime namespace: {0}")]
    Target(#[from] slingshot_daemon::runtime_namespace::NamespaceFailure),
    /// The runtime state could not be prepared.
    #[error("the runtime state could not be prepared: {0}")]
    Runtime(#[from] slingshot_daemon::platform_runtime::failure::PlatformFailure),
    /// The endpoint could not be bound or served.
    #[error("the endpoint could not be served: {0}")]
    Endpoint(#[from] slingshot_daemon::local_server::ServerFailure),
}

/// What a daemon process was asked to serve.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DaemonEntryArguments {
    /// Runtime root the namespace's objects live in.
    pub runtime_root: PathBuf,
    /// Profile half of the target.
    pub profile: String,
    /// Environment half of the target.
    pub environment: String,
}

impl DaemonEntryArguments {
    /// Names one daemon process entry.
    #[must_use]
    pub fn new(runtime_root: &Path, profile: &str, environment: &str) -> Self {
        Self {
            runtime_root: runtime_root.to_path_buf(),
            profile: profile.to_owned(),
            environment: environment.to_owned(),
        }
    }
}

/// Runs one daemon process until it stops in order.
///
/// Readiness is published only after the endpoint is bound, so a client that
/// reads readiness and then connects always reaches a daemon that can answer.
/// Orderly shutdown removes the endpoint object and this owner's readiness
/// record, and leaves the persistent lock file in place for the next owner to
/// contend for.
///
/// # Errors
///
/// Returns [`DaemonEntryFailure`] when the target does not name a namespace,
/// the runtime state cannot be prepared, or the endpoint cannot be served.
pub async fn run_daemon_entry(
    contract: &FoundationContract,
    arguments: &DaemonEntryArguments,
    shutdown: CancellationToken,
) -> Result<DaemonEntryOutcome, DaemonEntryFailure> {
    current_user::create_owner_only_directory(&arguments.runtime_root)?;
    let namespace = RuntimeNamespace::name(
        contract,
        &arguments.runtime_root,
        &arguments.profile,
        &arguments.environment,
    )?;
    let owned = match DaemonOwnership::acquire(contract, namespace)? {
        Acquisition::AlreadyOwned(_) => return Ok(DaemonEntryOutcome::AlreadyOwned),
        Acquisition::Owned(owned) => *owned,
    };
    let address =
        endpoint::endpoint_address(contract, &arguments.runtime_root, owned.namespace().digest())?;
    let mut listener = LocalListener::bind(&address)?;
    let mut service = DaemonService::new(contract.clone(), owned);
    service.ownership_mut().publish_readiness(contract, &address.display())?;
    let service = std::sync::Arc::new(service);
    local_server::serve(std::sync::Arc::clone(&service), &mut listener, shutdown).await?;
    listener.remove();
    drop(service);
    Ok(DaemonEntryOutcome::Served)
}
