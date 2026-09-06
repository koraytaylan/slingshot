//! One selected, read-only pass over every retained physical job of an operation.
use super::subscription_reset::ResetTransport;
use slingshot_agent_connection::{
    authentication::environment_provider::RequestAuthentication,
    event_stream_reset::ValidatedEventReset,
    selected_author_lookup::{PhysicalLookupReceipt, SnapshotLookupRefusal},
    selected_author_transport::SelectedAuthorTransport,
};
use slingshot_domain::operation_executor::ExecutionIdentity;
use slingshot_storage::{
    agent_subscription_ledger::AgentSubscriptionLedger, operation_repository::OperationRepository,
};

/// What a complete physical probe established, never permission to resubmit.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PhysicalRecoveryStatus {
    /// At least one identity-bound snapshot recovered the logical operation.
    Recovered,
    /// Every retained physical identifier independently answered missing.
    AllMissing,
    /// At least one request supplied neither a snapshot nor validated absence.
    Unanswered,
    /// The retained operation has no physical association to query.
    NoPhysicalJobs,
    /// Returned snapshots disagree about the same sequence or regress job facts.
    ConflictingSnapshots,
}

/// One queried physical identity and its result. A refused request is not missing.
pub struct PhysicalProbeResult {
    identifier: String,
    result: Result<PhysicalLookupReceipt, SnapshotLookupRefusal>,
    received: std::time::Instant,
}
impl core::fmt::Debug for PhysicalProbeResult {
    fn fmt(&self, formatter: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        formatter.write_str("PhysicalProbeResult([redacted])")
    }
}
impl PhysicalProbeResult {
    /// Exact retained physical identifier queried.
    pub fn identifier(&self) -> &str {
        &self.identifier
    }
    /// Consumes the result, conservatively debiting time since its receipt.
    /// Zero retention preserves terminal truth but supplies no acquisition time.
    pub fn into_result(mut self) -> Result<PhysicalLookupReceipt, SnapshotLookupRefusal> {
        if let Ok(PhysicalLookupReceipt::Found(receipt)) = &mut self.result {
            let elapsed = u64::try_from(self.received.elapsed().as_nanos().div_ceil(1_000_000))
                .unwrap_or(u64::MAX);
            receipt.remaining_retention_milliseconds =
                receipt.remaining_retention_milliseconds.saturating_sub(elapsed);
        }
        self.result
    }
}

/// Complete bounded probe evidence tied to its original local operation owner.
/// Probing is read-only; consuming recovered evidence uses guarded persistence
/// without changing the subscription's generation, cursor or incident.
pub struct PhysicalRecoveryReport<'runtime> {
    authentication: super::author_authentication::AuthorAuthentication<'runtime>,
    status: PhysicalRecoveryStatus,
    results: Vec<PhysicalProbeResult>,
    operations: &'runtime OperationRepository,
    identity: ExecutionIdentity,
    submission: slingshot_agent_connection::command_submission::Submission,
    expected_revision: u64,
    ledger: &'runtime AgentSubscriptionLedger,
    view: slingshot_storage::agent_subscription_ledger::SubscriptionRecoveryView<'runtime>,
    current_generation: u64,
}
impl core::fmt::Debug for PhysicalRecoveryReport<'_> {
    fn fmt(&self, formatter: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        formatter.write_str("PhysicalRecoveryReport([redacted])")
    }
}
impl PhysicalRecoveryReport<'_> {
    /// Classification after every retained physical identity has been queried.
    pub fn status(&self) -> PhysicalRecoveryStatus {
        self.status
    }
    /// Consumes all bounded results in canonical physical-identifier order.
    pub fn into_results(self) -> Vec<PhysicalProbeResult> {
        self.results
    }

    /// Charges one unsuccessful full probe, preserving execution evidence and
    /// using the shared bounded lookup retry policy. Exhaustion stays nonterminal
    /// and becomes manually resumable. No job, cursor or generation is changed.
    pub fn defer_unanswered(
        self,
        transport: &SelectedAuthorTransport,
        now_unix_milliseconds: u64,
    ) -> Result<slingshot_storage::operation_repository::OperationSummary, PhysicalRecoveryRefusal>
    {
        use slingshot_domain::operation::{OperationExecutionCertainty, RecoveryExecutionEvidence};
        if self.status != PhysicalRecoveryStatus::Unanswered {
            return Err(PhysicalRecoveryRefusal);
        }
        transport
            .require_submission(&self.identity, &self.submission)
            .map_err(|_| PhysicalRecoveryRefusal)?;
        let local = super::durable_author_lookup::retained_command(
            self.operations,
            &self.identity,
            &self.submission,
        )
        .map_err(|_| PhysicalRecoveryRefusal)?;
        let previous = local.record.outstanding_recovery.as_ref();
        if local.record.revision != self.expected_revision
            || previous.is_some_and(super::durable_author_lookup::automatic_recovery_paused)
        {
            return Err(PhysicalRecoveryRefusal);
        }
        let evidence = previous.map_or(
            RecoveryExecutionEvidence::ExecutionCertainty {
                certainty: OperationExecutionCertainty::RemoteOutcomeUnknown,
            },
            |held| held.evidence,
        );
        let recovery = super::durable_author_lookup::next_lookup_recovery(
            previous,
            evidence,
            now_unix_milliseconds,
        );
        self.operations
            .record_subscription_probe_recovery(
                self.ledger,
                &self.view,
                &self.submission.operation.agent_operation_identifier,
                self.expected_revision,
                recovery,
                now_unix_milliseconds,
            )
            .map_err(|_| PhysicalRecoveryRefusal)
    }

    /// Consumes complete missing/no-association evidence as a guarded local
    /// fail-closed disposition, never authoritative nonexecution or a retry.
    /// Known success remains on its independent result-acquisition path.
    /// Transport failures and conflicting snapshots cannot enter this method.
    pub fn settle_unavailable(
        self,
        transport: &SelectedAuthorTransport,
        now_unix_milliseconds: u64,
    ) -> Result<slingshot_storage::operation_repository::OperationSummary, PhysicalRecoveryRefusal>
    {
        if !matches!(
            self.status,
            PhysicalRecoveryStatus::AllMissing | PhysicalRecoveryStatus::NoPhysicalJobs
        ) {
            return Err(PhysicalRecoveryRefusal);
        }
        transport
            .require_submission(&self.identity, &self.submission)
            .map_err(|_| PhysicalRecoveryRefusal)?;
        self.operations
            .settle_unavailable_generation(
                self.ledger,
                &self.view,
                &self.submission.operation.agent_operation_identifier,
                self.current_generation,
                self.expected_revision,
                now_unix_milliseconds,
            )
            .map_err(|_| PhysicalRecoveryRefusal)
    }

    /// Consumes coherent found evidence through the existing durable snapshot
    /// and terminal-result reconciliation. The highest job sequence wins, not
    /// the last response received. No logical lookup or submission is repeated.
    /// Absence, conflicts and unanswered reports cannot enter this path.
    /// The subscription generation, cursor and incident remain unchanged.
    pub async fn reconcile_recovered(
        self,
        repository: &slingshot_storage::agent_job_repository::AgentJobRepository,
        transport: &SelectedAuthorTransport,
        authentication: &RequestAuthentication,
        now_unix_milliseconds: u64,
        completion: Option<(
            &slingshot_storage::artifact_store::ArtifactStore,
            &slingshot_storage::persistent_capacity::PersistentCapacityAccount<'_>,
        )>,
    ) -> Result<
        slingshot_agent_connection::selected_author_lookup::OperationLookupReceipt,
        PhysicalRecoveryRefusal,
    > {
        use super::author_authentication::AuthorAuthentication;
        let policy = match self.authentication {
            AuthorAuthentication::Fixed { protocol, .. } => {
                AuthorAuthentication::Fixed { authentication, protocol }
            }
            policy @ (AuthorAuthentication::Provider { .. }
            | AuthorAuthentication::AsyncProvider { .. }) => policy,
        };
        self.reconcile_using(repository, transport, policy, now_unix_milliseconds, completion).await
    }

    /// Completes recovered evidence with its captured policy, without another
    /// physical/logical lookup or a new submission permission.
    pub async fn reconcile_saved(
        self,
        repository: &slingshot_storage::agent_job_repository::AgentJobRepository,
        transport: &SelectedAuthorTransport,
        now: u64,
        completion: Option<(
            &slingshot_storage::artifact_store::ArtifactStore,
            &slingshot_storage::persistent_capacity::PersistentCapacityAccount<'_>,
        )>,
    ) -> Result<
        slingshot_agent_connection::selected_author_lookup::OperationLookupReceipt,
        PhysicalRecoveryRefusal,
    > {
        let authentication = self.authentication;
        self.reconcile_using(repository, transport, authentication, now, completion).await
    }

    async fn reconcile_using(
        self,
        repository: &slingshot_storage::agent_job_repository::AgentJobRepository,
        transport: &SelectedAuthorTransport,
        authentication: super::author_authentication::AuthorAuthentication<'_>,
        now_unix_milliseconds: u64,
        completion: Option<(
            &slingshot_storage::artifact_store::ArtifactStore,
            &slingshot_storage::persistent_capacity::PersistentCapacityAccount<'_>,
        )>,
    ) -> Result<
        slingshot_agent_connection::selected_author_lookup::OperationLookupReceipt,
        PhysicalRecoveryRefusal,
    > {
        if self.status != PhysicalRecoveryStatus::Recovered {
            return Err(PhysicalRecoveryRefusal);
        }
        let result = self
            .results
            .into_iter()
            .filter(|result| matches!(&result.result, Ok(PhysicalLookupReceipt::Found(_))))
            .max_by_key(|result| match &result.result {
                Ok(PhysicalLookupReceipt::Found(found)) => found.snapshot.sequence,
                _ => unreachable!("filtered found receipts"),
            })
            .ok_or(PhysicalRecoveryRefusal)?;
        let PhysicalLookupReceipt::Found(receipt) =
            result.into_result().map_err(|_| PhysicalRecoveryRefusal)?
        else {
            return Err(PhysicalRecoveryRefusal);
        };
        super::durable_author_lookup::reconcile_captured_physical_snapshot(
            repository,
            self.operations,
            self.expected_revision,
            transport,
            &self.identity,
            &self.submission,
            authentication,
            now_unix_milliseconds,
            completion,
            receipt,
        )
        .await
        .map_err(|_| PhysicalRecoveryRefusal)
    }
}

/// The retained context moved or did not authorize a generation-loss probe.
#[derive(Debug, Clone, Copy, thiserror::Error)]
#[error("the retained physical recovery context is not current")]
pub struct PhysicalRecoveryRefusal;

/// Queries every persisted physical job after a validated generation change.
/// Requests are read-only, selected-origin and never retried or turned into POSTs.
/// A single failure does not stop the remaining independent queries or count as
/// absence; a moved local record stops the attempt before its next request.
pub async fn probe_generation_loss<'runtime>(
    ledger: &'runtime AgentSubscriptionLedger,
    operations: &'runtime OperationRepository,
    transport: &SelectedAuthorTransport,
    identity: &ExecutionIdentity,
    reset: &ValidatedEventReset,
    authentication: &'runtime RequestAuthentication,
    protocol: ResetTransport,
) -> Result<PhysicalRecoveryReport<'runtime>, PhysicalRecoveryRefusal> {
    probe_generation_loss_with_authentication(
        ledger,
        operations,
        transport,
        identity,
        reset,
        super::author_authentication::AuthorAuthentication::Fixed { authentication, protocol },
    )
    .await
}

/// Queries the complete retained physical set through one invocation policy.
/// Authentication failure remains unanswered evidence, never physical absence.
pub async fn probe_generation_loss_with_authentication<'runtime>(
    ledger: &'runtime AgentSubscriptionLedger,
    operations: &'runtime OperationRepository,
    transport: &SelectedAuthorTransport,
    identity: &ExecutionIdentity,
    reset: &ValidatedEventReset,
    authentication: super::author_authentication::AuthorAuthentication<'runtime>,
) -> Result<PhysicalRecoveryReport<'runtime>, PhysicalRecoveryRefusal> {
    if !ledger.database().shares_database_with(operations.database()) {
        return Err(PhysicalRecoveryRefusal);
    }
    transport.require_execution(identity).map_err(|_| PhysicalRecoveryRefusal)?;
    authentication.require_execution(identity).map_err(|_| PhysicalRecoveryRefusal)?;
    let view = ledger
        .read_recovery_view(&identity.author_target_identity_digest, reset.subscription())
        .map_err(|_| PhysicalRecoveryRefusal)?;
    if view.ledger().agent_event_store_generation != reset.requested_generation()
        || reset.generation() == reset.requested_generation()
        || reset
            .requested_cursor()
            .is_some_and(|cursor| view.ledger().cursor.as_deref() != Some(cursor))
    {
        return Err(PhysicalRecoveryRefusal);
    }
    let member = view
        .members()
        .iter()
        .find(|member| member.identity.operation_identifier == identity.operation_identifier)
        .ok_or(PhysicalRecoveryRefusal)?;
    let submission =
        super::subscription_reset::restore(member).map_err(|_| PhysicalRecoveryRefusal)?;
    transport.require_submission(identity, &submission).map_err(|_| PhysicalRecoveryRefusal)?;
    let initial = super::durable_author_lookup::retained_command(operations, identity, &submission)
        .map_err(|_| PhysicalRecoveryRefusal)?;
    if initial.record.lifecycle_state.is_terminal()
        || initial
            .record
            .outstanding_recovery
            .as_ref()
            .is_some_and(super::durable_author_lookup::automatic_recovery_paused)
    {
        return Err(PhysicalRecoveryRefusal);
    }
    let identifiers = view
        .physical_jobs_for(&member.identity.agent_operation_identifier)
        .ok_or(PhysicalRecoveryRefusal)?;
    let maximum =
        slingshot_domain::author_agent_transport_contract::AuthorAgentTransportContract::embedded()
            .limit("maximum_physical_sling_job_matches");
    if identifiers.len() as u64 > maximum || identifiers.windows(2).any(|pair| pair[0] >= pair[1]) {
        return Err(PhysicalRecoveryRefusal);
    }
    let mut results = Vec::new();
    for identifier in identifiers {
        let current =
            super::durable_author_lookup::retained_command(operations, identity, &submission)
                .map_err(|_| PhysicalRecoveryRefusal)?;
        if current != initial {
            return Err(PhysicalRecoveryRefusal);
        }
        let result = authentication
            .physical_lookup(transport, identity, &submission, identifier, reset.generation())
            .await;
        results.push(PhysicalProbeResult {
            identifier: identifier.clone(),
            result,
            received: std::time::Instant::now(),
        });
    }
    let current = ledger
        .read_recovery_view(&identity.author_target_identity_digest, reset.subscription())
        .map_err(|_| PhysicalRecoveryRefusal)?;
    if current.ledger() != view.ledger()
        || current.members() != view.members()
        || current.physical_jobs_for(&member.identity.agent_operation_identifier)
            != Some(identifiers)
        || super::durable_author_lookup::retained_command(operations, identity, &submission)
            .map_err(|_| PhysicalRecoveryRefusal)?
            != initial
    {
        return Err(PhysicalRecoveryRefusal);
    }
    Ok(PhysicalRecoveryReport {
        authentication,
        status: classify(&results, &member.observation, identifiers),
        results,
        operations,
        identity: identity.clone(),
        submission,
        expected_revision: initial.record.revision,
        ledger,
        view,
        current_generation: reset.generation(),
    })
}

fn classify(
    results: &[PhysicalProbeResult],
    retained: &slingshot_domain::remote_job::RemoteJobObservation,
    retained_identifiers: &[String],
) -> PhysicalRecoveryStatus {
    if results.is_empty() {
        return PhysicalRecoveryStatus::NoPhysicalJobs;
    }
    let mut snapshots: Vec<_> = results
        .iter()
        .filter_map(|result| match &result.result {
            Ok(PhysicalLookupReceipt::Found(receipt)) => Some(&receipt.snapshot),
            _ => None,
        })
        .collect();
    if snapshots.is_empty() {
        return if results
            .iter()
            .all(|result| matches!(result.result, Ok(PhysicalLookupReceipt::Missing(_))))
        {
            PhysicalRecoveryStatus::AllMissing
        } else {
            PhysicalRecoveryStatus::Unanswered
        };
    }
    if snapshots.iter().any(|snapshot| {
        snapshot.sequence < retained.applied_sequence
            || (snapshot.sequence == retained.applied_sequence
                && (snapshot.described_state() != retained.state
                    || snapshot.attempt != retained.attempt
                    || snapshot.progress != retained.progress))
            || retained
                .require_advanceable(
                    snapshot.described_state(),
                    snapshot.attempt,
                    snapshot.progress,
                )
                .is_err()
            || retained_identifiers
                .iter()
                .any(|identifier| !snapshot.physical_sling_job_identifiers.contains(identifier))
    }) {
        return PhysicalRecoveryStatus::ConflictingSnapshots;
    }
    snapshots.sort_by_key(|snapshot| snapshot.sequence);
    for pair in snapshots.windows(2) {
        let previous = pair[0];
        let next = pair[1];
        let observation = slingshot_domain::remote_job::RemoteJobObservation {
            state: previous.described_state(),
            applied_sequence: previous.sequence,
            attempt: previous.attempt,
            progress: previous.progress,
        };
        let same_sequence_conflict = previous.sequence == next.sequence
            && (previous.kind != next.kind
                || previous.attempt != next.attempt
                || previous.progress != next.progress
                || previous.terminal_result != next.terminal_result
                || previous.terminal_failure != next.terminal_failure
                || previous.physical_sling_job_identifiers != next.physical_sling_job_identifiers);
        let terminal_conflict = previous.described_state().is_terminal()
            && (previous.terminal_result != next.terminal_result
                || previous.terminal_failure != next.terminal_failure);
        if same_sequence_conflict
            || terminal_conflict
            || observation
                .advanced(next.described_state(), next.sequence, next.attempt, next.progress)
                .is_err()
            || previous
                .physical_sling_job_identifiers
                .iter()
                .any(|identifier| !next.physical_sling_job_identifiers.contains(identifier))
        {
            return PhysicalRecoveryStatus::ConflictingSnapshots;
        }
    }
    PhysicalRecoveryStatus::Recovered
}
