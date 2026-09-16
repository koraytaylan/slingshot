//! Independent live connections for author execution while local requests continue.
//!
//! The scheduler retains the owning runtime until this worker is dropped. It
//! shares the selected provider and transport; it neither repeats startup
//! recovery nor acquires another namespace or authentication cache.

use super::{DurableRuntime, RuntimeClock, RuntimeExecutionRefusal};
use slingshot_agent_connection::{
    authentication::environment_provider::AsyncEnvironmentAuthenticationProvider,
    selected_author_transport::SelectedAuthorTransport,
};
use slingshot_domain::{
    installation::InstallationIdentifier, persistent_capacity::PersistentCapacityPolicy,
};
use slingshot_storage::{
    agent_job_repository::AgentJobRepository, artifact_store::ArtifactStore,
    database::OperationDatabase, operation_repository::OperationRepository,
    persistent_capacity::PersistentCapacityAccount,
};
use std::sync::Arc;
use tokio_util::sync::CancellationToken;

/// Worker-owned connections, bound to the still-owned runtime's database.
pub(crate) struct RuntimeExecution {
    operations: OperationRepository,
    remote: AgentJobRepository,
    artifacts: ArtifactStore,
    authentication: Arc<AsyncEnvironmentAuthenticationProvider>,
    transport: Arc<SelectedAuthorTransport>,
    clock: RuntimeClock,
    cancellation: CancellationToken,
    installation: InstallationIdentifier,
}

impl RuntimeExecution {
    /// Opens only live connections and verifies they share the owner's database.
    pub(super) fn open(runtime: &DurableRuntime) -> Result<Self, RuntimeExecutionRefusal> {
        let open = || {
            OperationDatabase::open_live(&runtime.paths.database_path(), runtime.builder.settings)
                .map_err(|_| RuntimeExecutionRefusal::Binding)
        };
        let operations = open()?;
        let remote = open()?;
        if !runtime.database().shares_database_with(&operations)
            || !runtime.database().shares_database_with(&remote)
        {
            return Err(RuntimeExecutionRefusal::Binding);
        }
        Ok(Self {
            operations: OperationRepository::new(operations),
            remote: AgentJobRepository::new(remote),
            artifacts: runtime.artifacts.clone(),
            authentication: Arc::clone(&runtime.builder.authentication),
            transport: Arc::clone(&runtime.builder.transport),
            clock: runtime.builder.clock,
            cancellation: runtime.cancellation.clone(),
            installation: runtime.installation.clone(),
        })
    }

    fn database(&self) -> &OperationDatabase {
        self.operations.database()
    }

    fn capacity(&self) -> PersistentCapacityAccount<'_> {
        PersistentCapacityAccount::new(self.database(), PersistentCapacityPolicy::embedded())
    }

    /// Executes only while the supplied local scheduler fence is still live.
    /// This is the production handoff; the compatibility method below remains
    /// for the pre-scheduler author-port tests.
    pub(crate) async fn execute_retained_with_claim(
        &self,
        identity: &slingshot_domain::operation_executor::ExecutionIdentity,
        submission: slingshot_agent_connection::command_submission::Submission,
        progress: &dyn slingshot_domain::operation_executor::ProgressPort,
        fence: u64,
        now_unix_milliseconds: u64,
    ) -> Result<
        slingshot_domain::operation_executor::OperationExecutorOutcome,
        RuntimeExecutionRefusal,
    > {
        let facts = slingshot_storage::operation::scheduler_claim::facts(
            self.database(),
            &identity.author_target_identity_digest,
            &identity.operation_identifier,
        )
        .map_err(|_| RuntimeExecutionRefusal::Claim)?
        .ok_or(RuntimeExecutionRefusal::Claim)?;
        if facts.scheduler_fence != Some(fence)
            || facts.checkpoint.is_some()
            || facts
                .lease_expires_at_unix_milliseconds
                .is_some_and(|expiry| expiry < now_unix_milliseconds)
        {
            return Err(RuntimeExecutionRefusal::Claim);
        }
        if !slingshot_storage::operation::scheduler_claim::checkpoint(
            self.database(),
            &identity.author_target_identity_digest,
            &identity.operation_identifier,
            fence,
            "executor-started",
        )
        .map_err(|_| RuntimeExecutionRefusal::Claim)?
        {
            return Err(RuntimeExecutionRefusal::Claim);
        }
        self.execute_retained(identity, submission, progress).await
    }

    /// Runs the concrete retained protocol over this runtime's resources.
    /// The caller owns scheduling/settlement and must already hold its execution
    /// authority. This method neither grants a scheduler lease nor re-admits work.
    pub(crate) async fn execute_retained(
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
        self.transport.require_execution(identity).map_err(|_| RuntimeExecutionRefusal::Binding)?;
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
                    provider: &self.authentication,
                    clock: &self.clock,
                    utc: &self.clock,
                },
                identity.clone(),
                submission,
                now,
            )
            .map_err(|_| RuntimeExecutionRefusal::Binding)?;
        let ports = crate::author_agent_operation_executor::ProductAuthorPorts::over_transport(
            &self.transport,
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
}
