//! Ownership-bound inputs for complete product startup.
//!
//! This initial stage deliberately has no readiness publication or service
//! conversion. Durable installation, audit and recovery must establish the
//! later ready stage before the product may expose an operation endpoint.

const INSTALLATION_RANDOM_BYTES: usize = 32;

use std::path::{Path, PathBuf};

use crate::diagnostics::{DiagnosticBounds, DiagnosticSink};
use slingshot_agent_connection::{
    authentication::environment_provider::{
        AsyncEnvironmentAuthenticationProvider, SelectedEnvironmentSnapshot,
    },
    selected_author_transport::SelectedAuthorTransport,
};
use slingshot_domain::daemon_runtime_contract::DaemonRuntimeContract;
use slingshot_domain::installation::{
    InstallationIdentifier, InstallationRecord, TargetRegistration,
};
use slingshot_domain::persistent_capacity::PersistentCapacityPolicy;
use slingshot_storage::database::{OperationDatabase, RequiredSettings, StartupDatabaseBinding};
use slingshot_storage::installation_state::{InstallationState, InstallationStateFailure};
use slingshot_storage::{
    agent_job_repository::AgentJobRepository, agent_subscription_ledger::AgentSubscriptionLedger,
    artifact_store::ArtifactStore, operation_repository::OperationRepository,
    persistent_capacity::PersistentCapacityAccount,
};
use tokio_util::sync::CancellationToken;

mod publication_recovery;
pub use publication_recovery::RecoveredPublication;

use crate::{
    ownership::DaemonOwnership,
    runtime_namespace::{PersistentTargetPaths, RuntimeNamespace},
    startup::SelectedTarget,
};

/// A startup context that owns its selection and operating-system namespace lock.
/// No caller-selected target/revision strings or replacement authentication
/// policy can be installed after construction.
pub struct RuntimeBuilder {
    authentication: AsyncEnvironmentAuthenticationProvider,
    transport: SelectedAuthorTransport,
    target: SelectedTarget,
    state_root: PathBuf,
    settings: RequiredSettings,
    clock: RuntimeClock,
    // The namespace lock outlives all preceding owned runtime fields.
    ownership: DaemonOwnership,
}

struct RuntimeClock(std::time::Instant);

impl slingshot_agent_connection::authentication::identity_management_exchange::MonotonicClock
    for RuntimeClock
{
    fn reading_milliseconds(&self) -> u64 {
        u64::try_from(self.0.elapsed().as_millis()).unwrap_or(u64::MAX)
    }
}

impl slingshot_agent_connection::authentication::token_assertion::CoordinatedUniversalTimeClock
    for RuntimeClock
{
    fn sample(&self) -> Option<u64> {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .ok()
            .map(|value| value.as_secs())
    }
}

/// A runtime invocation could not be bound or was locally cancelled.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum RuntimeExecutionRefusal {
    /// The retained operation, submission and selected runtime disagree.
    #[error("the runtime invocation could not be bound")]
    Binding,
    /// Local polling stopped; this makes no assertion about remote execution.
    #[error("the runtime invocation was cancelled locally")]
    Cancelled,
    /// The local scheduler fence is absent, stale, or already past its
    /// no-return checkpoint.
    #[error("the runtime invocation does not hold the live scheduler fence")]
    Claim,
}

impl core::fmt::Debug for RuntimeBuilder {
    fn fmt(&self, formatter: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        formatter.write_str("RuntimeBuilder([redacted])")
    }
}

/// Startup did not establish an owned, selected runtime context.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum RuntimeBuildRefusal {
    /// The acquired namespace is not the namespace of the resolved snapshot.
    #[error("runtime ownership does not match the selected configuration")]
    OwnershipMismatch,
    /// The selected provider could not initialize its owned state.
    #[error("the selected runtime authentication could not be initialized")]
    Authentication,
    /// The immutable selected-author transport could not be initialized.
    #[error("the selected runtime transport could not be initialized")]
    Transport,
    /// Durable paths or the installation ledger could not be safely established.
    #[error("the selected runtime installation could not be established")]
    Installation,
    /// The database failed its installation/partition audit or durable open.
    #[error("the selected runtime database could not be established")]
    Database,
    /// A required local runtime resource could not be initialized or verified.
    #[error("the selected runtime resources could not be established")]
    Resources,
}

/// Audited durable state retaining the configuration and namespace ownership.
/// Durable establishment installs selected execution and reconstructs retained
/// recovery evidence. This stage is not advertised until a service retains it,
/// binds its listener and publishes exactly that service's installed versions.
pub struct DurableRuntime {
    operations: OperationRepository,
    remote: AgentJobRepository,
    subscriptions: AgentSubscriptionLedger,
    waiters: crate::operation_wait::runtime::RuntimeWaiters,
    artifacts: ArtifactStore,
    diagnostics: DiagnosticSink,
    cancellation: CancellationToken,
    maintenance_recovery: Vec<slingshot_storage::maintenance::ApplicationReceipt>,
    recovered_operations: Vec<RecoveredOperation>,
    recovered_stages: u64,
    pending_publications: Vec<RecoveredPublication>,
    maintenance_publication_recovery:
        Option<slingshot_storage::persistent_capacity::MaintenancePublicationRecovery>,
    installation: InstallationIdentifier,
    paths: PersistentTargetPaths,
    // Database and its resources close before namespace ownership is released.
    builder: RuntimeBuilder,
}

impl core::fmt::Debug for DurableRuntime {
    fn fmt(&self, formatter: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        formatter.write_str("DurableRuntime([redacted])")
    }
}

/// Retained startup input, not an execution lease or a submission permit.
#[derive(Debug)]
pub struct RecoveredOperation {
    /// Exact admitted command and retained lifecycle/recovery facts.
    pub input: slingshot_storage::operation_repository::RetainedExecutionInput,
    /// Existing outbox evidence requires lookup-first recovery, never a new POST.
    pub remote: Option<slingshot_storage::agent_job_repository::AgentSubmission>,
}

impl DurableRuntime {
    /// Executes only while the supplied local scheduler fence is still live.
    /// This is the production handoff; the compatibility method below remains
    /// for the pre-scheduler author-port tests.
    pub async fn execute_retained_with_claim(
        &self,
        identity: &slingshot_domain::operation_executor::ExecutionIdentity,
        submission: slingshot_agent_connection::command_submission::Submission,
        progress: &dyn slingshot_domain::operation_executor::ProgressPort,
        fence: u64,
        now_unix_milliseconds: u64,
    ) -> Result<slingshot_domain::operation_executor::OperationExecutorOutcome, RuntimeExecutionRefusal> {
        let facts = slingshot_storage::operation::scheduler_claim::facts(
            self.database(), &identity.author_target_identity_digest, &identity.operation_identifier,
        ).map_err(|_| RuntimeExecutionRefusal::Claim)?.ok_or(RuntimeExecutionRefusal::Claim)?;
        if facts.scheduler_fence != Some(fence) || facts.checkpoint.is_some()
            || facts.lease_expires_at_unix_milliseconds.is_some_and(|expiry| expiry < now_unix_milliseconds)
        {
            return Err(RuntimeExecutionRefusal::Claim);
        }
        if !slingshot_storage::operation::scheduler_claim::checkpoint(
            self.database(), &identity.author_target_identity_digest,
            &identity.operation_identifier, fence, "executor-started",
        ).map_err(|_| RuntimeExecutionRefusal::Claim)? {
            return Err(RuntimeExecutionRefusal::Claim);
        }
        self.execute_retained(identity, submission, progress).await
    }

    pub(crate) fn ownership(&self) -> &DaemonOwnership {
        &self.builder.ownership
    }

    pub(crate) fn ownership_mut(&mut self) -> &mut DaemonOwnership {
        &mut self.builder.ownership
    }

    /// Complete pending producer evidence, preserved for owner-bound result recovery.
    pub fn pending_publications(&self) -> &[RecoveredPublication] {
        &self.pending_publications
    }
    /// Startup producer reconciliation; this never grants maintenance approval.
    pub fn maintenance_publication_recovery(
        &self,
    ) -> Option<slingshot_storage::persistent_capacity::MaintenancePublicationRecovery> {
        self.maintenance_publication_recovery
    }
    /// Private abandoned stages removed before any artifact producer could run.
    pub fn recovered_stages(&self) -> u64 {
        self.recovered_stages
    }
    /// Runs the concrete retained protocol over this runtime's resources.
    /// The caller owns scheduling/settlement and must already hold its execution
    /// authority. This method neither grants a scheduler lease nor re-admits work.
    pub async fn execute_retained(
        &self,
        identity: &slingshot_domain::operation_executor::ExecutionIdentity,
        submission: slingshot_agent_connection::command_submission::Submission,
        progress: &dyn slingshot_domain::operation_executor::ProgressPort,
    ) -> Result<
        slingshot_domain::operation_executor::OperationExecutorOutcome,
        RuntimeExecutionRefusal,
    > {
        use slingshot_domain::operation_executor::OperationExecutor as _;
        if self.cancellation.is_cancelled() {
            return Err(RuntimeExecutionRefusal::Cancelled);
        }
        self.builder
            .transport
            .require_execution(identity)
            .map_err(|_| RuntimeExecutionRefusal::Binding)?;
        let local = self
            .operations
            .read(&identity.author_target_identity_digest, &identity.operation_identifier)
            .map_err(|_| RuntimeExecutionRefusal::Binding)?
            .ok_or(RuntimeExecutionRefusal::Binding)?;
        if local.installation_identifier != self.installation {
            return Err(RuntimeExecutionRefusal::Binding);
        }
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .ok()
            .and_then(|value| u64::try_from(value.as_millis()).ok())
            .ok_or(RuntimeExecutionRefusal::Binding)?;
        let capacity = self.capacity();
        let protocol =
            crate::retained_author_protocol::RetainedAuthorProtocol::new_with_authentication(
                &self.operations,
                &self.remote,
                &self.artifacts,
                &capacity,
                crate::operation::author_authentication::AuthorAuthentication::AsyncProvider {
                    provider: &self.builder.authentication,
                    clock: &self.builder.clock,
                    utc: &self.builder.clock,
                },
                identity.clone(),
                submission,
                now,
            )
            .map_err(|_| RuntimeExecutionRefusal::Binding)?;
        let ports = crate::author_agent_operation_executor::ProductAuthorPorts::over_transport(
            &self.builder.transport,
            &protocol,
        );
        let executor =
            crate::author_agent_operation_executor::AuthorAgentOperationExecutor::over(&ports);
        tokio::select! {
            biased;
            _ = self.cancellation.cancelled() => Err(RuntimeExecutionRefusal::Cancelled),
            outcome = executor.execute(identity, protocol.command(), progress) => Ok(outcome),
        }
    }

    /// Stops owned local polling without claiming cancellation of remote work.
    pub fn request_shutdown(&self) {
        self.cancellation.cancel();
    }
    /// Nonterminal local inputs and their retained remote evidence in enqueue order.
    pub fn recovered_operations(&self) -> &[RecoveredOperation] {
        &self.recovered_operations
    }
    /// Audited database owned by this startup stage.
    pub fn database(&self) -> &OperationDatabase {
        self.operations.database()
    }
    /// Local operation repository on the audited database.
    pub fn operations(&self) -> &OperationRepository {
        &self.operations
    }
    /// Retained author outbox on the same database object.
    pub fn remote(&self) -> &AgentJobRepository {
        &self.remote
    }
    /// Shared author subscription ledger on the same database object.
    pub fn subscriptions(&self) -> &AgentSubscriptionLedger {
        &self.subscriptions
    }
    /// Bounded local observers sharing this runtime's shutdown scope.
    pub fn waiters(&self) -> &crate::operation_wait::runtime::RuntimeWaiters {
        &self.waiters
    }
    /// Artifact store rooted in the validated private content directory.
    pub fn artifacts(&self) -> &ArtifactStore {
        &self.artifacts
    }
    /// Bounded diagnostic sink owned by this runtime.
    pub fn diagnostics(&self) -> &DiagnosticSink {
        &self.diagnostics
    }
    /// Capacity accounting always uses this runtime's database and embedded policy.
    pub fn capacity(&self) -> PersistentCapacityAccount<'_> {
        PersistentCapacityAccount::new(self.database(), PersistentCapacityPolicy::embedded())
    }
    /// A child cancellation scope that cannot cancel its owning runtime.
    pub fn cancellation_scope(&self) -> CancellationToken {
        self.cancellation.child_token()
    }
    /// Receipts revisited during startup, including referenced cleanup still pending.
    pub fn maintenance_recovery(&self) -> &[slingshot_storage::maintenance::ApplicationReceipt] {
        &self.maintenance_recovery
    }
    /// Stable identity retained from the locked installation ledger.
    pub fn installation(&self) -> &InstallationIdentifier {
        &self.installation
    }
    /// Validated persistent target paths.
    pub fn paths(&self) -> &PersistentTargetPaths {
        &self.paths
    }
    /// Immutable selected runtime context, still holding namespace ownership.
    pub fn context(&self) -> &RuntimeBuilder {
        &self.builder
    }

    /// Acquires the durable local scheduler fence for one retained operation.
    /// Selection and the compare-and-set claim happen in one SQLite transaction;
    /// a stale lifecycle/revision or worker loses without executor authority.
    pub fn claim_scheduled_operation(
        &self,
        operation_identifier: &str,
        expected_lifecycle: &str,
        expected_revision: u64,
        fence: u64,
        lease_expires_at_unix_milliseconds: u64,
        now_unix_milliseconds: u64,
    ) -> Result<slingshot_storage::operation::scheduler_claim::ClaimOutcome, slingshot_storage::operation_repository::RepositoryFailure> {
        slingshot_storage::operation::scheduler_claim::claim(
            self.database(),
            &self.context().target().author_target_identity_digest,
            operation_identifier,
            expected_lifecycle,
            expected_revision,
            fence,
            lease_expires_at_unix_milliseconds,
            now_unix_milliseconds,
        )
    }
}

impl Drop for DurableRuntime {
    fn drop(&mut self) {
        self.cancellation.cancel();
    }
}

impl RuntimeBuilder {
    /// Establishes durable identity and database under namespace ownership and
    /// one uninterrupted installation-ledger transaction. A fresh identity is
    /// generated only in an empty state root. Interrupted staged intent remains
    /// durable on failure; existing missing or foreign identities are never repaired.
    /// No endpoint or readiness record is published by this transition.
    pub fn establish_durable(self) -> Result<DurableRuntime, RuntimeBuildRefusal> {
        use rand::RngExt as _;
        let installation_failure = |_| RuntimeBuildRefusal::Installation;
        crate::runtime_namespace::create_private_directory(&self.state_root)
            .map_err(installation_failure)?;
        let state = InstallationState::at(&self.state_root);
        let mut transaction = state.transaction().map_err(|_| RuntimeBuildRefusal::Installation)?;
        let mut record = match transaction.read() {
            Ok(record) => record,
            Err(InstallationStateFailure::Absent) if !transaction.state_root_occupied() => {
                let bytes: [u8; INSTALLATION_RANDOM_BYTES] = rand::rng().random();
                InstallationRecord::new(
                    InstallationIdentifier::parse(&hex::encode(bytes))
                        .map_err(|_| RuntimeBuildRefusal::Installation)?,
                )
            }
            Err(_) => return Err(RuntimeBuildRefusal::Installation),
        };
        let paths = self.namespace().beneath(&self.state_root);
        let namespace = self.namespace().key();
        let present = match std::fs::symlink_metadata(paths.database_path()) {
            Ok(_) => true,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => false,
            Err(_) => return Err(RuntimeBuildRefusal::Installation),
        };
        let registration = record.registration(&namespace);
        if (registration.is_none() && present)
            || (registration == Some(TargetRegistration::Registered) && !present)
        {
            return Err(RuntimeBuildRefusal::Installation);
        }
        let database = if present {
            OperationDatabase::reopen_bound(
                &paths.database_path(),
                self.settings,
                StartupDatabaseBinding {
                    installation: &record.installation_identifier,
                    target: &self.target.author_target_identity_digest,
                    revision: &self.target.selected_environment_revision,
                    runtime_contract: &self.target.daemon_runtime_contract_digest,
                },
            )
            .map_err(|_| RuntimeBuildRefusal::Database)?
        } else {
            record = record.stage(&namespace).map_err(|_| RuntimeBuildRefusal::Installation)?;
            transaction.replace(&record).map_err(|_| RuntimeBuildRefusal::Installation)?;
            paths.create().map_err(installation_failure)?;
            let database = OperationDatabase::open(&paths.database_path(), self.settings)
                .map_err(|_| RuntimeBuildRefusal::Database)?;
            let recorded_at = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .ok()
                .and_then(|value| i64::try_from(value.as_millis()).ok())
                .ok_or(RuntimeBuildRefusal::Installation)?;
            database
                .record_installation_identifier(&record.installation_identifier, recorded_at)
                .map_err(|_| RuntimeBuildRefusal::Database)?;
            database
        };
        paths.create().map_err(installation_failure)?;
        crate::runtime_namespace::create_private_directory(
            &paths.artifact_root().join(slingshot_storage::artifact_store::CONTENT_DIRECTORY),
        )
        .map_err(|_| RuntimeBuildRefusal::Resources)?;
        let artifacts = ArtifactStore::open(&paths.artifact_root())
            .map_err(|_| RuntimeBuildRefusal::Resources)?;
        let recovered_stages =
            artifacts.recover_abandoned_stages().map_err(|_| RuntimeBuildRefusal::Resources)?;
        let diagnostics =
            DiagnosticSink::open(&paths.diagnostic_root(), DiagnosticBounds::embedded())
                .map_err(|_| RuntimeBuildRefusal::Resources)?;
        diagnostics.health().map_err(|_| RuntimeBuildRefusal::Resources)?;
        let remote_database = OperationDatabase::open_live(&paths.database_path(), self.settings)
            .map_err(|_| RuntimeBuildRefusal::Database)?;
        let subscription_database =
            OperationDatabase::open_live(&paths.database_path(), self.settings)
                .map_err(|_| RuntimeBuildRefusal::Database)?;
        if !database.shares_database_with(&remote_database)
            || !database.shares_database_with(&subscription_database)
        {
            return Err(RuntimeBuildRefusal::Database);
        }
        PersistentCapacityAccount::new(&database, PersistentCapacityPolicy::embedded())
            .usage()
            .map_err(|_| RuntimeBuildRefusal::Resources)?;
        let (pending_publications, pending_maintenance_publication) =
            PersistentCapacityAccount::new(&database, PersistentCapacityPolicy::embedded())
                .reconstruct_target_publications(&self.target.author_target_identity_digest)
                .map_err(|_| RuntimeBuildRefusal::Resources)?;
        let maintenance_recovery = slingshot_storage::maintenance::recover_pending_cleanup(
            &database,
            &artifacts,
            &self.target.author_target_identity_digest,
        )
        .map_err(|_| RuntimeBuildRefusal::Resources)?;
        slingshot_storage::maintenance_results::cleanup_superseded_preview(
            &database,
            &artifacts,
            &self.target.author_target_identity_digest,
        )
        .map_err(|_| RuntimeBuildRefusal::Resources)?;
        if registration != Some(TargetRegistration::Registered) {
            record = record.register(&namespace).map_err(|_| RuntimeBuildRefusal::Installation)?;
            transaction.replace(&record).map_err(|_| RuntimeBuildRefusal::Installation)?;
        }
        let installation = record.installation_identifier;
        drop(transaction);
        let operations = OperationRepository::new(database);
        let remote = AgentJobRepository::new(remote_database);
        let mut recovered_operations = Vec::new();
        let mut publication_owners = Vec::new();
        for summary in operations
            .reconstruct(&self.target.author_target_identity_digest)
            .map_err(|_| RuntimeBuildRefusal::Resources)?
        {
            if summary.installation_identifier == installation {
                publication_owners.push((
                    summary.operation_identifier.clone(),
                    summary.command_wire_name.clone(),
                ));
            }
            if summary.record.lifecycle_state.is_terminal() {
                continue;
            }
            let input = operations
                .read_execution_input(
                    &self.target.author_target_identity_digest,
                    &summary.operation_identifier,
                )
                .map_err(|_| RuntimeBuildRefusal::Resources)?
                .ok_or(RuntimeBuildRefusal::Resources)?;
            if input.summary != summary
                || summary.installation_identifier != installation
                || summary.selected_environment_revision
                    != self.target.selected_environment_revision
                || input.daemon_runtime_contract_digest
                    != self.target.daemon_runtime_contract_digest
            {
                return Err(RuntimeBuildRefusal::Resources);
            }
            let child = remote
                .read_for_local_operation(
                    &self.target.author_target_identity_digest,
                    &summary.operation_identifier,
                )
                .map_err(|_| RuntimeBuildRefusal::Resources)?;
            if child.as_ref().is_some_and(|child| {
                child.identity.selected_environment_revision
                    != self.target.selected_environment_revision
            }) {
                return Err(RuntimeBuildRefusal::Resources);
            }
            recovered_operations.push(RecoveredOperation { input, remote: child });
        }
        let pending_publications = publication_recovery::bind_publications(
            &installation,
            &self.target.author_target_identity_digest,
            publication_owners
                .iter()
                .map(|(operation, command)| (operation.as_str(), command.as_str())),
            pending_publications,
        )
        .map_err(|_| RuntimeBuildRefusal::Resources)?;
        let cancellation = CancellationToken::new();
        let maintenance_publication_recovery = pending_maintenance_publication
            .as_ref()
            .map(|pending| {
                PersistentCapacityAccount::new(
                    operations.database(),
                    PersistentCapacityPolicy::embedded(),
                )
                .reconcile_maintenance_publication(
                    &artifacts,
                    &self.target.author_target_identity_digest,
                    pending.hold(),
                )
                .map_err(|_| RuntimeBuildRefusal::Resources)
            })
            .transpose()?;
        Ok(DurableRuntime {
            operations,
            remote,
            subscriptions: AgentSubscriptionLedger::new(subscription_database),
            waiters: crate::operation_wait::runtime::RuntimeWaiters::new(
                cancellation.child_token(),
            ),
            artifacts,
            diagnostics,
            cancellation,
            maintenance_recovery,
            recovered_operations,
            recovered_stages,
            pending_publications,
            maintenance_publication_recovery,
            installation,
            paths,
            builder: self,
        })
    }

    /// Consumes one resolved snapshot and one acquired namespace ownership.
    /// Failure releases ownership without creating durable state, opening a
    /// network connection or publishing readiness. Normal provider construction
    /// uses process randomness and the fixed IMS endpoint, never a test seam.
    pub fn new(
        snapshot: SelectedEnvironmentSnapshot,
        ownership: DaemonOwnership,
        state_root: PathBuf,
        settings: RequiredSettings,
    ) -> Result<Self, RuntimeBuildRefusal> {
        if ownership.namespace().profile() != snapshot.profile_name().as_text()
            || ownership.namespace().environment() != snapshot.environment_name().as_text()
        {
            return Err(RuntimeBuildRefusal::OwnershipMismatch);
        }
        let target = SelectedTarget {
            author_target_identity_digest: snapshot.target().to_string(),
            selected_environment_revision: snapshot.revision().to_string(),
            daemon_runtime_contract_digest: DaemonRuntimeContract::embedded_digest()
                .as_text()
                .to_owned(),
        };
        let transport = SelectedAuthorTransport::new(snapshot.author_connection())
            .map_err(|_| RuntimeBuildRefusal::Transport)?;
        let authentication = AsyncEnvironmentAuthenticationProvider::new_async(snapshot)
            .map_err(|_| RuntimeBuildRefusal::Authentication)?;
        Ok(Self {
            authentication,
            transport,
            target,
            state_root,
            settings,
            clock: RuntimeClock(std::time::Instant::now()),
            ownership,
        })
    }

    /// The selected target derives only from the frozen snapshot and this build.
    pub fn target(&self) -> &SelectedTarget {
        &self.target
    }

    /// The namespace whose lock this context keeps alive.
    pub fn namespace(&self) -> &RuntimeNamespace {
        self.ownership.namespace()
    }

    /// The selected state root, not yet opened by this initial stage.
    pub fn state_root(&self) -> &Path {
        &self.state_root
    }

    /// Database settings to verify during durable establishment.
    pub fn settings(&self) -> RequiredSettings {
        self.settings
    }

    /// The one runtime-owned provider, never an invocation-long bearer token.
    pub fn authentication(&self) -> &AsyncEnvironmentAuthenticationProvider {
        &self.authentication
    }

    /// The one immutable direct author transport.
    pub fn transport(&self) -> &SelectedAuthorTransport {
        &self.transport
    }
}
