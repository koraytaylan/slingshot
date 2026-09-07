//! The internal daemon process entry point.
//!
//! The entry point receives its runtime root and its target, takes ownership of
//! that namespace, reads one verified configuration generation, freezes trust,
//! establishes and recovers its durable runtime, then binds the endpoint and
//! publishes readiness. The service retains that runtime until every connection
//! closes, and serves until a stop carrying the live nonce arrives.
//! A process that finds the namespace already owned exits without binding, so a
//! second daemon for one target can never exist.

use std::path::{Path, PathBuf};

use slingshot_agent_connection::authentication::runtime_snapshot::build_runtime_snapshot;
#[cfg(feature = "runtime-test-host")]
use slingshot_configuration::platform_trust::ProviderRecord;
use slingshot_configuration::{
    platform_trust::{OperatingSystemTrustSource, PlatformTrustSource},
    profile_loader::LoadedProfiles,
    profile_selection::RequestedSelection,
};
use slingshot_daemon::local_server::{self, LocalListener};
use slingshot_daemon::ownership::{Acquisition, DaemonOwnership};
use slingshot_daemon::platform_runtime::endpoint;
use slingshot_daemon::runtime_builder::RuntimeBuilder;
use slingshot_daemon::runtime_namespace::RuntimeNamespace;
use slingshot_daemon::service::DaemonService;
use slingshot_domain::{
    daemon_runtime_contract::DaemonRuntimeContract,
    profile::{EnvironmentName, ProfileName},
};
use slingshot_local_protocol::foundation_contract::FoundationContract;
use slingshot_storage::database::RequiredSettings;
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
    /// Configuration could not be read without exposing source material.
    #[error("the daemon configuration could not be established")]
    Configuration,
    /// The selected configuration or trust snapshot was refused.
    #[error(transparent)]
    Snapshot(
        #[from]
        slingshot_agent_connection::authentication::runtime_snapshot::RuntimeSnapshotRefusal,
    ),
    /// Durable startup refused before endpoint publication.
    #[error(transparent)]
    Startup(#[from] slingshot_daemon::runtime_builder::RuntimeBuildRefusal),
    /// This account has no usable persistent state location.
    #[error("the daemon persistent state root could not be resolved")]
    StateRoot,
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
    run_with_sources(contract, arguments, shutdown, &OperatingSystemTrustSource, || {
        let loaded = crate::command_line::loaded_profiles()
            .map_err(|_| DaemonEntryFailure::Configuration)?;
        let directories = directories::ProjectDirs::from("", "", "slingshot")
            .ok_or(DaemonEntryFailure::StateRoot)?;
        Ok((loaded, directories.data_local_dir().join("state")))
    })
    .await
}

/// Runs the same startup with an explicit configuration tree in a test host.
/// Neither the product executable nor its argument parser can select this path.
#[cfg(feature = "runtime-test-host")]
pub async fn run_daemon_entry_for_test(
    contract: &FoundationContract,
    arguments: &DaemonEntryArguments,
    shutdown: CancellationToken,
    root: slingshot_configuration::configuration_root::ConfigurationRoot,
    state_root: PathBuf,
) -> Result<DaemonEntryOutcome, DaemonEntryFailure> {
    run_with_sources(contract, arguments, shutdown, &TestPlatformTrustSource, || {
        #[cfg(unix)]
        let authority =
            slingshot_configuration::credential_filesystem::UnixConfigurationFilesystem::new(root);
        #[cfg(windows)]
        let authority =
            slingshot_configuration::credential_filesystem::WindowsConfigurationFilesystem::new(
                root,
            );
        let authority = authority.map_err(|_| DaemonEntryFailure::Configuration)?;
        let loaded = slingshot_configuration::profile_loader::load_profiles(authority)
            .map_err(|_| DaemonEntryFailure::Configuration)?;
        Ok((loaded, state_root))
    })
    .await
}

async fn run_with_sources(
    contract: &FoundationContract,
    arguments: &DaemonEntryArguments,
    shutdown: CancellationToken,
    platform: &dyn PlatformTrustSource,
    sources: impl FnOnce() -> Result<(LoadedProfiles, PathBuf), DaemonEntryFailure>,
) -> Result<DaemonEntryOutcome, DaemonEntryFailure> {
    let namespace = RuntimeNamespace::name(
        contract,
        &arguments.runtime_root,
        &arguments.profile,
        &arguments.environment,
    )?;
    namespace.create_runtime_directory()?;
    let owned = match DaemonOwnership::acquire(contract, namespace)? {
        Acquisition::AlreadyOwned(_) => return Ok(DaemonEntryOutcome::AlreadyOwned),
        Acquisition::Owned(owned) => *owned,
    };
    let address =
        endpoint::endpoint_address(contract, &arguments.runtime_root, owned.namespace().digest())?;
    let (loaded, state_root) = sources()?;
    let requested = RequestedSelection {
        profile: Some(
            ProfileName::parse(&arguments.profile)
                .map_err(|_| DaemonEntryFailure::Configuration)?,
        ),
        environment: Some(
            EnvironmentName::parse(&arguments.environment)
                .map_err(|_| DaemonEntryFailure::Configuration)?,
        ),
    };
    let snapshot = build_runtime_snapshot(loaded, &requested, platform)?;
    let limits = DaemonRuntimeContract::embedded();
    let runtime = RuntimeBuilder::new(
        snapshot,
        owned,
        state_root,
        RequiredSettings {
            page_bytes: limits.limit("sqlite_page_bytes"),
            database_pages: limits.limit("maximum_sqlite_database_pages"),
            busy_timeout_milliseconds: limits.limit("database_busy_timeout_milliseconds"),
        },
    )?
    .establish_durable()?;
    if shutdown.is_cancelled() {
        return Ok(DaemonEntryOutcome::Served);
    }
    // Declare the service before the listener, so every error/cancellation drops
    // the endpoint before releasing the service's namespace ownership.
    let mut service = std::sync::Arc::new(DaemonService::from_runtime(contract.clone(), runtime));
    let mut listener = LocalListener::bind(&address)?;
    std::sync::Arc::get_mut(&mut service)
        .expect("the service is not shared before readiness")
        .ownership_mut()
        .publish_readiness(contract, &address.display())?;
    let served =
        local_server::serve(std::sync::Arc::clone(&service), &mut listener, shutdown).await;
    let withdrawn = std::sync::Arc::get_mut(&mut service)
        .expect("the server drains every connection before returning")
        .ownership_mut()
        .withdraw_readiness();
    listener.remove();
    drop(service);
    served?;
    withdrawn?;
    Ok(DaemonEntryOutcome::Served)
}

/// Deterministic trust source for the compiled runtime test host.
///
/// The host must not inherit the machine's CA store: its contents are
/// platform- and image-dependent, and a single restricted root would make a
/// fixture that is otherwise local fail before it can bind its endpoint.
#[cfg(feature = "runtime-test-host")]
struct TestPlatformTrustSource;

#[cfg(feature = "runtime-test-host")]
impl PlatformTrustSource for TestPlatformTrustSource {
    fn records(
        &self,
    ) -> Result<Vec<ProviderRecord>, slingshot_configuration::profile_loader::ConfigurationDiagnostic>
    {
        Ok(Vec::new())
    }
}
