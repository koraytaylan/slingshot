//! Single-use recovery of a terminal event through independent snapshot evidence.

use super::{durable_author_event::DurableEventRefusal, author_authentication::AuthorAuthentication};

fn remaining_delay(chosen: u64, observed: u64, now: u64) -> u64 {
    chosen.saturating_sub(now.saturating_sub(observed))
}
use slingshot_agent_connection::{
    authentication::environment_provider::RequestAuthentication, command_submission::Submission,
    selected_author_lookup::OperationLookupReceipt,
    selected_author_transport::SelectedAuthorTransport, server_sent_event_decoder::DecodedEvent,
};
use slingshot_domain::operation_executor::ExecutionIdentity;
use slingshot_storage::{
    agent_job_repository::{AgentJobRepository, AgentSubmission},
    agent_subscription_ledger::{AgentSubscriptionLedger, SubscriptionRecoveryView},
    operation_repository::OperationRepository,
};

/// Sealed terminal-event correlation and its original retained owner/revision.
/// The event is not itself a result or a failure-disposition proof.
pub struct CapturedTerminalEvent<'runtime> {
    ledger: &'runtime AgentSubscriptionLedger,
    operations: &'runtime OperationRepository,
    view: SubscriptionRecoveryView<'runtime>,
    retained: AgentSubmission,
    identity: ExecutionIdentity,
    submission: Submission,
    revision: u64,
    event: DecodedEvent,
    authentication: AuthorAuthentication<'runtime>,
    observed: tokio::time::Instant,
    ready_at: tokio::time::Instant,
    now: u64,
}
impl core::fmt::Debug for CapturedTerminalEvent<'_> {
    fn fmt(&self, formatter: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        formatter.write_str("CapturedTerminalEvent([redacted])")
    }
}
impl<'runtime> CapturedTerminalEvent<'runtime> {
    pub(super) fn capture(
        ledger: &'runtime AgentSubscriptionLedger,
        operations: &'runtime OperationRepository,
        transport: &SelectedAuthorTransport,
        selection: &ExecutionIdentity,
        event: DecodedEvent,
        authentication: AuthorAuthentication<'runtime>,
        now: u64,
    ) -> Result<Self, DurableEventRefusal> {
        authentication.require_execution(selection).map_err(|_| DurableEventRefusal)?;
        if !event.event.kind.is_terminal()
            || event.terminal.is_none()
            || event.cursor.is_none()
            || now > i64::MAX as u64
            || !ledger.database().shares_database_with(operations.database())
        {
            return Err(DurableEventRefusal);
        }
        let observed = tokio::time::Instant::now();
        let view = ledger
            .read_recovery_view(
                &selection.author_target_identity_digest,
                &event.daemon_subscription_identifier,
            )
            .map_err(|_| DurableEventRefusal)?;
        let retained = view
            .members()
            .iter()
            .find(|member| {
                member.identity.agent_operation_identifier == event.event.agent_operation_identifier
            })
            .ok_or(DurableEventRefusal)?
            .clone();
        let submission =
            super::subscription_reset::restore(&retained).map_err(|_| DurableEventRefusal)?;
        let terminal = event.terminal.as_ref().ok_or(DurableEventRefusal)?;
        if terminal.provenance != submission.provenance
            || terminal.submitted_command_digest != submission.submitted_command_digest
            || !slingshot_domain::remote_job::JobEventSequence::of(event.event.sequence)
                .immediately_follows(retained.observation.applied_sequence)
            || now < retained.recorded_at_unix_milliseconds
        {
            return Err(DurableEventRefusal);
        }
        let identity = ExecutionIdentity {
            attempt: selection.attempt,
            author_target_identity_digest: retained.identity.author_target_identity_digest.clone(),
            selected_environment_revision: retained.identity.selected_environment_revision.clone(),
            operation_identifier: retained.identity.operation_identifier.clone(),
        };
        transport.require_submission(&identity, &submission).map_err(|_| DurableEventRefusal)?;
        let local =
            super::durable_author_lookup::retained_command(operations, &identity, &submission)
                .map_err(|_| DurableEventRefusal)?;
        if local.record.lifecycle_state.is_terminal()
            || local
                .record
                .outstanding_recovery
                .as_ref()
                .is_some_and(super::durable_author_lookup::automatic_recovery_paused)
            || view.ledger().unresolved_incident.is_some()
            || view.ledger().agent_event_store_generation
                != event.event.agent_event_store_generation
            || retained.identity.agent_event_store_generation
                != event.event.agent_event_store_generation
        {
            return Err(DurableEventRefusal);
        }
        let retry = local.record.outstanding_recovery.as_ref();
        let chosen = retry.map_or(0, |retry| retry.retry_delay_milliseconds);
        if chosen > slingshot_domain::author_agent_transport_contract::AuthorAgentTransportContract::embedded().limit("retry_jitter_cap_milliseconds") {
            return Err(DurableEventRefusal);
        }
        let remaining = remaining_delay(
            chosen,
            retry.map_or(now, |retry| retry.retry_observed_at_unix_milliseconds),
            now,
        );
        let ready_at = observed
            .checked_add(tokio::time::Duration::from_millis(remaining))
            .ok_or(DurableEventRefusal)?;
        let now = now.max(retry.map_or(0, |retry| retry.retry_observed_at_unix_milliseconds));
        if now > i64::MAX as u64 {
            return Err(DurableEventRefusal);
        }
        Ok(Self {
            ledger,
            operations,
            view,
            retained,
            identity,
            submission,
            revision: local.record.revision,
            event,
            authentication,
            observed,
            ready_at,
            now,
        })
    }

    /// Remote operation named by the authenticated event and retained submission.
    pub fn agent_operation_identifier(&self) -> &str {
        &self.retained.identity.agent_operation_identifier
    }

    fn require_current(&self) -> Result<(), DurableEventRefusal> {
        let current = self
            .ledger
            .read_recovery_view(
                &self.identity.author_target_identity_digest,
                &self.event.daemon_subscription_identifier,
            )
            .map_err(|_| DurableEventRefusal)?;
        if current.ledger() != self.view.ledger()
            || current.members() != self.view.members()
            || self.view.members().iter().any(|member| {
                current.physical_jobs_for(&member.identity.agent_operation_identifier)
                    != self.view.physical_jobs_for(&member.identity.agent_operation_identifier)
            })
        {
            return Err(DurableEventRefusal);
        }
        let local = super::durable_author_lookup::retained_command(
            self.operations,
            &self.identity,
            &self.submission,
        )
        .map_err(|_| DurableEventRefusal)?;
        if local.record.revision != self.revision || local.record.lifecycle_state.is_terminal() {
            return Err(DurableEventRefusal);
        }
        Ok(())
    }

    fn record_exchange_failure(&self) -> Result<(), DurableEventRefusal> {
        use slingshot_domain::operation::{OperationExecutionCertainty, RecoveryExecutionEvidence};
        self.require_current()?;
        let local = super::durable_author_lookup::retained_command(
            self.operations,
            &self.identity,
            &self.submission,
        )
        .map_err(|_| DurableEventRefusal)?;
        let previous = local.record.outstanding_recovery.as_ref();
        let evidence = previous.map_or(
            RecoveryExecutionEvidence::ExecutionCertainty {
                certainty: OperationExecutionCertainty::RemoteOutcomeUnknown,
            },
            |fact| fact.evidence,
        );
        let now = self
            .now
            .checked_add(
                u64::try_from(self.observed.elapsed().as_nanos().div_ceil(1_000_000))
                    .map_err(|_| DurableEventRefusal)?,
            )
            .filter(|now| *now <= i64::MAX as u64)
            .ok_or(DurableEventRefusal)?;
        let retry = super::durable_author_lookup::next_lookup_recovery(previous, evidence, now);
        self.operations
            .record_subscription_probe_recovery(
                self.ledger,
                &self.view,
                self.agent_operation_identifier(),
                self.revision,
                retry,
                now,
            )
            .map_err(|_| DurableEventRefusal)?;
        Ok(())
    }

    /// Looks up independent terminal evidence and consumes it through the guarded
    /// typed-result/failure reconciler. Waits any retained retry delay, makes one
    /// attempt and durably charges failed exchanges without advancing the cursor.
    /// No POST, cursor installation or implicit repeated request.
    /// Missing, active, older or contradictory snapshots never settle the event.
    /// Dropping this future leaves the event cursor available for later replay.
    ///
    /// # Errors
    /// Refuses changed ownership, failed exchanges or snapshots that do not cover
    /// the complete terminal event. The cursor remains at the committed prefix.
    pub async fn reconcile(
        self,
        repository: &AgentJobRepository,
        transport: &SelectedAuthorTransport,
        authentication: &RequestAuthentication,
        completion: Option<(
            &slingshot_storage::artifact_store::ArtifactStore,
            &slingshot_storage::persistent_capacity::PersistentCapacityAccount<'_>,
        )>,
    ) -> Result<OperationLookupReceipt, DurableEventRefusal> {
        let policy = match self.authentication {
            AuthorAuthentication::Fixed { protocol, .. } => AuthorAuthentication::Fixed { authentication, protocol },
            policy @ (AuthorAuthentication::Provider { .. } | AuthorAuthentication::AsyncProvider { .. }) => policy,
        };
        self.reconcile_using(repository, transport, policy, completion).await
    }

    /// Consumes the ticket using its captured policy after the persisted wait.
    /// No invocation-long bearer value or replacement submission is required.
    pub async fn reconcile_saved(
        self,
        repository: &AgentJobRepository,
        transport: &SelectedAuthorTransport,
        completion: Option<(&slingshot_storage::artifact_store::ArtifactStore,
            &slingshot_storage::persistent_capacity::PersistentCapacityAccount<'_>)>,
    ) -> Result<OperationLookupReceipt, DurableEventRefusal> {
        let authentication = self.authentication;
        self.reconcile_using(repository, transport, authentication, completion).await
    }

    async fn reconcile_using(
        self,
        repository: &AgentJobRepository,
        transport: &SelectedAuthorTransport,
        authentication: AuthorAuthentication<'_>,
        completion: Option<(&slingshot_storage::artifact_store::ArtifactStore,
            &slingshot_storage::persistent_capacity::PersistentCapacityAccount<'_>)>,
    ) -> Result<OperationLookupReceipt, DurableEventRefusal> {
        authentication.require_execution(&self.identity).map_err(|_| DurableEventRefusal)?;
        if !repository.database().shares_database_with(self.operations.database()) {
            return Err(DurableEventRefusal);
        }
        transport
            .require_submission(&self.identity, &self.submission)
            .map_err(|_| DurableEventRefusal)?;
        self.require_current()?;
        tokio::time::sleep_until(self.ready_at).await;
        self.require_current()?;
        let capabilities = authentication.discover(transport, &self.identity, &self.submission).await;
        if capabilities.is_err() {
            self.record_exchange_failure()?;
            return Err(DurableEventRefusal);
        }
        self.require_current()?;
        let receipt = authentication.lookup(transport, &self.identity, &self.submission).await;
        let mut receipt = match receipt {
            Ok(receipt) => receipt,
            Err(_) => {
                self.record_exchange_failure()?;
                return Err(DurableEventRefusal);
            }
        };
        let received = tokio::time::Instant::now();
        self.require_current()?;
        let OperationLookupReceipt::Found(found) = &receipt else {
            self.record_exchange_failure()?;
            return Err(DurableEventRefusal);
        };
        let snapshot = &found.snapshot;
        if snapshot.kind != self.event.event.kind
            || snapshot.sequence.value() < self.event.event.sequence
            || snapshot.subscription_watermark.as_text()
                < self.event.cursor.as_ref().ok_or(DurableEventRefusal)?.as_text()
            || snapshot.attempt < self.event.attempt.unwrap_or(self.retained.observation.attempt)
            || snapshot.progress < self.event.progress.unwrap_or(self.retained.observation.progress)
            || !snapshot.physical_sling_job_identifiers.contains(&self.event.sling_job_identifier)
            || self
                .view
                .physical_jobs_for(self.agent_operation_identifier())
                .ok_or(DurableEventRefusal)?
                .iter()
                .any(|name| !snapshot.physical_sling_job_identifiers.contains(name))
        {
            self.record_exchange_failure()?;
            return Err(DurableEventRefusal);
        }
        let now = self
            .now
            .checked_add(
                u64::try_from(self.observed.elapsed().as_nanos().div_ceil(1_000_000))
                    .map_err(|_| DurableEventRefusal)?,
            )
            .filter(|now| *now <= i64::MAX as u64)
            .ok_or(DurableEventRefusal)?;
        if let OperationLookupReceipt::Found(found) = &mut receipt {
            found.remaining_retention_milliseconds =
                found.remaining_retention_milliseconds.saturating_sub(
                    u64::try_from(received.elapsed().as_nanos().div_ceil(1_000_000))
                        .map_err(|_| DurableEventRefusal)?,
                );
        }
        super::durable_author_lookup::reconcile_captured_lookup_with_authentication(
            repository,
            self.operations,
            self.revision,
            transport,
            &self.identity,
            &self.submission,
            authentication,
            now,
            completion,
            receipt,
        )
        .await
        .map_err(|_| DurableEventRefusal)
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn persisted_retry_delay_uses_only_unelapsed_time_and_survives_clock_regression() {
        for (chosen, observed, now, remaining) in [
            (50, 100, 100, 50),
            (50, 100, 120, 30),
            (50, 100, 150, 0),
            (50, 100, 200, 0),
            (50, 100, 90, 50),
            (0, 100, 90, 0),
            (50, 0, u64::MAX, 0),
        ] {
            assert_eq!(super::remaining_delay(chosen, observed, now), remaining);
        }
    }
}
