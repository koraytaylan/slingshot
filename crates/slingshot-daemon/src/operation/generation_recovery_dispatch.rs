//! Single-use scheduled dispatch of one retained generation-loss recovery.
use super::{
    generation_loss_probe::{PhysicalRecoveryStatus, probe_generation_loss_with_authentication},
    author_authentication::AuthorAuthentication,
    subscription_reset::ResetTransport,
};
use slingshot_agent_connection::{
    authentication::environment_provider::RequestAuthentication,
    event_stream_reset::ValidatedEventReset, selected_author_lookup::OperationLookupReceipt,
    selected_author_transport::SelectedAuthorTransport,
};
use slingshot_domain::{
    operation::RecoveryExecutionEvidence, operation_executor::ExecutionIdentity,
};
use slingshot_storage::{
    agent_job_repository::AgentJobRepository,
    agent_subscription_ledger::AgentSubscriptionLedger,
    operation_repository::{OperationRepository, OperationSummary},
};

/// One dispatched pass; no variant authorizes replacement work.
pub enum GenerationDispatchOutcome {
    /// Coherent found evidence went through guarded snapshot/result reconciliation.
    Reconciled(OperationLookupReceipt),
    /// Unanswered requests recorded one bounded, nonterminal recovery attempt.
    Deferred(OperationSummary),
    /// Unavailable prior state produced a guarded local fail-closed disposition.
    Settled(OperationSummary),
    /// Already-known success requires independent result acquisition, not loss.
    NeedsResultAcquisition,
    /// Conflicting snapshots require integrity recovery, not a missing-state claim.
    NeedsIntegrityRecovery,
}
impl core::fmt::Debug for GenerationDispatchOutcome {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str("GenerationDispatchOutcome([redacted])")
    }
}

/// Refused or moved retained context; never remote absence.
#[derive(Debug, Clone, Copy, thiserror::Error)]
#[error("the scheduled generation recovery is not current")]
pub struct GenerationDispatchRefusal;

/// A single scheduled pass bound to the original owner, selection and revision.
/// Dropping it before or during its wait sends no requests and changes no facts.
/// Rescheduling is an explicit new read of durable state, not an implicit retry.
pub struct ScheduledGenerationRecovery<'runtime> {
    ledger: &'runtime AgentSubscriptionLedger,
    operations: &'runtime OperationRepository,
    repository: &'runtime AgentJobRepository,
    transport: &'runtime SelectedAuthorTransport,
    authentication: AuthorAuthentication<'runtime>,
    reset: &'runtime ValidatedEventReset,
    identity: ExecutionIdentity,
    revision: u64,
    success_known: bool,
    ready_at: tokio::time::Instant,
    started: tokio::time::Instant,
    observed_at: u64,
}
impl core::fmt::Debug for ScheduledGenerationRecovery<'_> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str("ScheduledGenerationRecovery([redacted])")
    }
}
impl<'runtime> ScheduledGenerationRecovery<'runtime> {
    /// Reconstructs a bounded residual wait once from persisted scheduling facts.
    /// A forward wall-clock jump makes work due; a backward jump cannot extend
    /// the original chosen wait. Subsequent waiting uses only monotonic time.
    pub fn new(
        ledger: &'runtime AgentSubscriptionLedger,
        operations: &'runtime OperationRepository,
        repository: &'runtime AgentJobRepository,
        transport: &'runtime SelectedAuthorTransport,
        authentication: &'runtime RequestAuthentication,
        reset: &'runtime ValidatedEventReset,
        identity: &ExecutionIdentity,
        protocol: ResetTransport,
        now_unix_milliseconds: u64,
    ) -> Result<Self, GenerationDispatchRefusal> {
        Self::new_with_authentication(ledger, operations, repository, transport,
            AuthorAuthentication::Fixed { authentication, protocol }, reset, identity, now_unix_milliseconds)
    }

    /// Captures one scheduled pass using a provider policy without freezing its
    /// bearer token. Waiting and the complete durable-view fences are unchanged.
    pub fn new_with_authentication(
        ledger: &'runtime AgentSubscriptionLedger,
        operations: &'runtime OperationRepository,
        repository: &'runtime AgentJobRepository,
        transport: &'runtime SelectedAuthorTransport,
        authentication: AuthorAuthentication<'runtime>,
        reset: &'runtime ValidatedEventReset,
        identity: &ExecutionIdentity,
        now_unix_milliseconds: u64,
    ) -> Result<Self, GenerationDispatchRefusal> {
        let started = tokio::time::Instant::now();
        if !ledger.database().shares_database_with(operations.database())
            || !repository.database().shares_database_with(operations.database())
            || now_unix_milliseconds > i64::MAX as u64
        {
            return Err(GenerationDispatchRefusal);
        }
        transport.require_execution(identity).map_err(|_| GenerationDispatchRefusal)?;
        authentication.require_execution(identity).map_err(|_| GenerationDispatchRefusal)?;
        let view = ledger
            .read_recovery_view(&identity.author_target_identity_digest, reset.subscription())
            .map_err(|_| GenerationDispatchRefusal)?;
        if view.ledger().agent_event_store_generation != reset.requested_generation()
            || reset.generation() == reset.requested_generation()
            || reset
                .requested_cursor()
                .is_some_and(|cursor| view.ledger().cursor.as_deref() != Some(cursor))
        {
            return Err(GenerationDispatchRefusal);
        }
        let member = view
            .members()
            .iter()
            .find(|member| member.identity.operation_identifier == identity.operation_identifier)
            .ok_or(GenerationDispatchRefusal)?;
        let submission =
            super::subscription_reset::restore(member).map_err(|_| GenerationDispatchRefusal)?;
        transport
            .require_submission(identity, &submission)
            .map_err(|_| GenerationDispatchRefusal)?;
        let local =
            super::durable_author_lookup::retained_command(operations, identity, &submission)
                .map_err(|_| GenerationDispatchRefusal)?;
        let recovery = local.record.outstanding_recovery.as_ref();
        if local.record.lifecycle_state.is_terminal()
            || recovery.is_some_and(super::durable_author_lookup::automatic_recovery_paused)
        {
            return Err(GenerationDispatchRefusal);
        }
        let delay = recovery.map_or(0, |held| {
            residual_wait(
                held.retry_delay_milliseconds,
                held.retry_observed_at_unix_milliseconds,
                now_unix_milliseconds,
            )
        });
        let maximum = slingshot_domain::author_agent_transport_contract::AuthorAgentTransportContract::embedded()
            .limit("retry_jitter_cap_milliseconds");
        if recovery.is_some_and(|held| held.retry_delay_milliseconds > maximum) {
            return Err(GenerationDispatchRefusal);
        }
        // A backwards wall clock may defer at most the chosen wait, and must
        // not make the subsequent durable timestamp regress and refuse forever.
        let observed_at = now_unix_milliseconds
            .max(member.recorded_at_unix_milliseconds)
            .max(recovery.map_or(0, |held| held.retry_observed_at_unix_milliseconds));
        if observed_at > i64::MAX as u64 {
            return Err(GenerationDispatchRefusal);
        }
        let ready_at = started
            .checked_add(std::time::Duration::from_millis(delay))
            .ok_or(GenerationDispatchRefusal)?;
        Ok(Self {
            ledger,
            operations,
            repository,
            transport,
            authentication,
            reset,
            identity: identity.clone(),
            revision: local.record.revision,
            success_known: recovery.is_some_and(|held| {
                held.evidence == RecoveryExecutionEvidence::AuthoritativeRemoteSuccess
            }),
            ready_at,
            started,
            observed_at,
        })
    }

    /// Waits until due, rechecks the local revision, then dispatches exactly one
    /// bounded physical probe. A new durable delay requires a new scheduled pass.
    /// Cancellation during probing retains no partial settlement or retry charge.
    pub async fn run(
        self,
        completion: Option<(
            &slingshot_storage::artifact_store::ArtifactStore,
            &slingshot_storage::persistent_capacity::PersistentCapacityAccount<'_>,
        )>,
    ) -> Result<GenerationDispatchOutcome, GenerationDispatchRefusal> {
        tokio::time::sleep_until(self.ready_at).await;
        let local = self
            .operations
            .read(&self.identity.author_target_identity_digest, &self.identity.operation_identifier)
            .map_err(|_| GenerationDispatchRefusal)?
            .ok_or(GenerationDispatchRefusal)?;
        if local.record.revision != self.revision {
            return Err(GenerationDispatchRefusal);
        }
        let report = probe_generation_loss_with_authentication(
            self.ledger,
            self.operations,
            self.transport,
            &self.identity,
            self.reset,
            self.authentication,
        )
        .await
        .map_err(|_| GenerationDispatchRefusal)?;
        let current = self
            .operations
            .read(&self.identity.author_target_identity_digest, &self.identity.operation_identifier)
            .map_err(|_| GenerationDispatchRefusal)?
            .ok_or(GenerationDispatchRefusal)?;
        if current.record.revision != self.revision {
            return Err(GenerationDispatchRefusal);
        }
        let elapsed = u64::try_from(self.started.elapsed().as_nanos().div_ceil(1_000_000))
            .map_err(|_| GenerationDispatchRefusal)?;
        let now = self
            .observed_at
            .checked_add(elapsed)
            .filter(|now| *now <= i64::MAX as u64)
            .ok_or(GenerationDispatchRefusal)?;
        match report.status() {
            PhysicalRecoveryStatus::Recovered => report
                .reconcile_saved(
                    self.repository,
                    self.transport,
                    now,
                    completion,
                )
                .await
                .map(GenerationDispatchOutcome::Reconciled)
                .map_err(|_| GenerationDispatchRefusal),
            PhysicalRecoveryStatus::Unanswered => report
                .defer_unanswered(self.transport, now)
                .map(GenerationDispatchOutcome::Deferred)
                .map_err(|_| GenerationDispatchRefusal),
            PhysicalRecoveryStatus::AllMissing | PhysicalRecoveryStatus::NoPhysicalJobs
                if self.success_known =>
            {
                Ok(GenerationDispatchOutcome::NeedsResultAcquisition)
            }
            PhysicalRecoveryStatus::AllMissing | PhysicalRecoveryStatus::NoPhysicalJobs => report
                .settle_unavailable(self.transport, now)
                .map(GenerationDispatchOutcome::Settled)
                .map_err(|_| GenerationDispatchRefusal),
            PhysicalRecoveryStatus::ConflictingSnapshots => {
                Ok(GenerationDispatchOutcome::NeedsIntegrityRecovery)
            }
        }
    }
}

fn residual_wait(chosen: u64, observed: u64, now: u64) -> u64 {
    chosen.saturating_sub(now.saturating_sub(observed))
}

#[cfg(test)]
mod tests {
    use super::residual_wait;
    #[test]
    fn restart_wait_is_bounded_without_addition_overflow() {
        for (chosen, observed, now, expected) in [
            (50, 1000, 1000, 50),
            (50, 1000, 1020, 30),
            (50, 1000, 1050, 0),
            (50, 1000, 5000, 0),
            (50, 1000, 0, 50),
            (50, u64::MAX - 10, u64::MAX, 40),
            (0, u64::MAX, 0, 0),
        ] {
            assert_eq!(residual_wait(chosen, observed, now), expected);
        }
    }
}
