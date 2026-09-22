//! Lookup-first recovery over an existing, byte-identical remote child.

mod reconciliation;
use reconciliation::reconcile_retained_operation;
mod terminal;
pub use terminal::record_retired_lookup;
use terminal::record_snapshot_success;

const NANOSECONDS_PER_MILLISECOND: u128 = 1_000_000;

use super::author_authentication::AuthorAuthentication;
use slingshot_agent_connection::authentication::environment_provider::RequestAuthentication;
use slingshot_agent_connection::command_submission::Submission;
use slingshot_agent_connection::selected_author_lookup::OperationLookupReceipt;
use slingshot_agent_connection::selected_author_transport::SelectedAuthorTransport;
use slingshot_domain::operation::{
    OperationExecutionCertainty, OperationFact, RecoveryCategory, RecoveryExecutionEvidence,
    RecoveryFact, TerminalFailure, TerminalFailureDisposition, TerminalFailureKind,
};
use slingshot_domain::operation_executor::ExecutionIdentity;
use slingshot_domain::remote_job::RemoteJobObservation;
use slingshot_storage::agent_job_repository::AgentJobRepository;
use slingshot_storage::operation_repository::{OperationRepository, OperationSummary};

pub(crate) fn automatic_recovery_paused(fact: &RecoveryFact) -> bool {
    fact.manual_resume_eligible
        && (fact.category == RecoveryCategory::PersistentCapacityUnavailable
            || u64::from(fact.attempt_count)
                >= crate::operation::recovery_and_event_supervisor::automatic_attempt_cap())
}

#[cfg(test)]
mod pause_tests;

/// Recovery could not establish and persist a consistent selected-author fact.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[error("the retained author lookup could not be reconciled")]
pub struct DurableLookupRefusal;

/// Activates one admitted resume against the retained command and remote child.
/// A returned revision may enter the selected recovery category; this grants no
/// submission permit or scheduler lease. Receipt replay returns no activation.
///
/// # Errors
/// Returns `DurableLookupRefusal` for mismatched selection, command bytes,
/// resume fingerprint or retained child, or a repository read/activation failure.
pub fn activate_retained_resume(
    repository: &AgentJobRepository,
    operations: &OperationRepository,
    transport: &SelectedAuthorTransport,
    identity: &ExecutionIdentity,
    submission: &Submission,
    receipt: &slingshot_domain::operation::RecoveryResumeReceipt,
    category: RecoveryCategory,
    now_unix_milliseconds: u64,
) -> Result<Option<OperationSummary>, DurableLookupRefusal> {
    transport.require_submission(identity, submission).map_err(|_| DurableLookupRefusal)?;
    let local = retained_command(operations, identity, submission)?;
    let expected_source = crate::operation_recovery::source_fingerprint(
        &identity.operation_identifier,
        local.command_fingerprint.as_text(),
        receipt.applied_operation_revision,
        category,
    );
    if receipt.source_fingerprint != expected_source {
        return Err(DurableLookupRefusal);
    }
    let retained = repository
        .read(
            &identity.author_target_identity_digest,
            &submission.operation.agent_operation_identifier,
        )
        .map_err(|_| DurableLookupRefusal)?
        .ok_or(DurableLookupRefusal)?;
    if retained.canonical_submission.as_bytes()
        != submission.wire_body().map_err(|_| DurableLookupRefusal)?
    {
        return Err(DurableLookupRefusal);
    }
    operations
        .activate_retained_recovery(&retained, receipt, category, now_unix_milliseconds)
        .map_err(|_| DurableLookupRefusal)
}

/// Binds remote submission bytes to the independently admitted local command.
pub(crate) fn retained_command(
    operations: &OperationRepository,
    identity: &ExecutionIdentity,
    submission: &Submission,
) -> Result<OperationSummary, DurableLookupRefusal> {
    use slingshot_domain::command_fingerprint::{CommandFingerprint, FingerprintInput};
    let input = operations
        .read_execution_input(
            &identity.author_target_identity_digest,
            &identity.operation_identifier,
        )
        .map_err(|_| DurableLookupRefusal)?
        .ok_or(DurableLookupRefusal)?;
    let fingerprint = CommandFingerprint::derive(&FingerprintInput {
        author_target_identity_digest: identity.author_target_identity_digest.clone(),
        selected_environment_revision: identity.selected_environment_revision.clone(),
        canonical_command: submission.canonical_arguments.clone(),
        command_wire_name: submission.provenance.command_contract.command_wire_name.clone(),
        command_semantic_contract_version: submission
            .provenance
            .command_contract
            .command_semantic_contract_version
            .clone(),
    })
    .map_err(|_| DurableLookupRefusal)?;
    if input.canonical_command != submission.canonical_arguments
        || input.summary.command_wire_name
            != submission.provenance.command_contract.command_wire_name
        || input.summary.command_fingerprint != fingerprint
        || input.daemon_runtime_contract_digest
            != slingshot_domain::daemon_runtime_contract::DaemonRuntimeContract::embedded_digest()
                .as_text()
    {
        return Err(DurableLookupRefusal);
    }
    Ok(input.summary)
}

/// Persists a missing-operation wait under the local operation's revision CAS.
/// Grace is anchored to the saved request start, never renewed by a restart.
/// Exhaustion pauses uncertain work and never supplies a resend permission.
///
/// # Errors
/// Returns `DurableLookupRefusal` for stale or terminal local state, invalid
/// timing, conflicting success evidence, or a repository read/update failure.
pub fn record_missing_lookup(
    operations: &OperationRepository,
    identity: &ExecutionIdentity,
    expected_revision: u64,
    request_start_unix_milliseconds: u64,
    now_unix_milliseconds: u64,
) -> Result<OperationSummary, DurableLookupRefusal> {
    use crate::operation::recovery_and_event_supervisor::{
        automatic_attempt_cap, jitter_ceiling_milliseconds,
    };
    use slingshot_agent_connection::job_snapshot_reconciliation::missing_grace_milliseconds;
    let held = operations
        .read(&identity.author_target_identity_digest, &identity.operation_identifier)
        .map_err(|_| DurableLookupRefusal)?
        .ok_or(DurableLookupRefusal)?;
    if held.record.revision != expected_revision
        || held.selected_environment_revision != identity.selected_environment_revision
        || held.record.lifecycle_state.is_terminal()
        || now_unix_milliseconds < request_start_unix_milliseconds
    {
        return Err(DurableLookupRefusal);
    }
    let previous = held.record.outstanding_recovery.as_ref();
    if previous
        .is_some_and(|fact| fact.evidence == RecoveryExecutionEvidence::AuthoritativeRemoteSuccess)
    {
        return Err(DurableLookupRefusal);
    }
    if previous.is_some_and(automatic_recovery_paused) {
        return Ok(held);
    }
    let attempt_count = previous.map_or(1, |fact| fact.attempt_count.saturating_add(1));
    if u64::from(attempt_count) >= automatic_attempt_cap() {
        return apply_terminal(
            operations,
            &identity.author_target_identity_digest,
            &identity.operation_identifier,
            expected_revision,
            exhausted_without_an_answer(),
            now_unix_milliseconds,
        );
    }
    let grace_end = request_start_unix_milliseconds
        .checked_add(missing_grace_milliseconds())
        .ok_or(DurableLookupRefusal)?;
    let grace_remaining = grace_end.saturating_sub(now_unix_milliseconds);
    let recovery = RecoveryFact {
        attempt_count,
        category: RecoveryCategory::OperationLookup,
        detail: if grace_remaining > 0 {
            "waiting for the saved missing-operation grace interval"
        } else {
            "operation remains missing; lookup is required before any resend"
        }
        .to_owned(),
        evidence: previous.map_or(
            RecoveryExecutionEvidence::ExecutionCertainty {
                certainty: OperationExecutionCertainty::SubmissionUnknown,
            },
            |fact| fact.evidence,
        ),
        manual_resume_eligible: false,
        retry_delay_milliseconds: {
            use rand::RngExt;
            let ceiling = jitter_ceiling_milliseconds(u64::from(attempt_count));
            grace_remaining.max(rand::rng().random_range(0..=ceiling))
        },
        retry_observed_at_unix_milliseconds: now_unix_milliseconds,
    };
    operations
        .apply(
            &identity.author_target_identity_digest,
            &identity.operation_identifier,
            expected_revision,
            &OperationFact::Recovery { recovery },
            now_unix_milliseconds,
        )
        .map_err(|_| DurableLookupRefusal)
}

/// Charges one detached attempt against the operation's automatic budget.
///
/// An attempt the scheduler detached produced no outcome this daemon may
/// publish, so the operation must not be reported as anything at all. What it
/// must do is count: without a durable charge the same work would be detached
/// and reclaimed forever, and the automatic budget that exists to ask a person
/// would never be reached. The evidence stays whatever it already was, because
/// a local timeout says nothing about the remote system, and exhaustion pauses
/// the work for manual recovery rather than deciding it.
///
/// # Errors
/// Returns `DurableLookupRefusal` for stale or terminal state, an established
/// remote success whose local half is what is outstanding, or a repository
/// refusal.
pub fn record_detached_attempt(
    operations: &OperationRepository,
    identity: &ExecutionIdentity,
    expected_revision: u64,
    now_unix_milliseconds: u64,
) -> Result<OperationSummary, DurableLookupRefusal> {
    use crate::operation::recovery_and_event_supervisor::{
        automatic_attempt_cap, jitter_ceiling_milliseconds,
    };
    let held = operations
        .read(&identity.author_target_identity_digest, &identity.operation_identifier)
        .map_err(|_| DurableLookupRefusal)?
        .ok_or(DurableLookupRefusal)?;
    if held.record.revision != expected_revision
        || held.record.lifecycle_state.is_terminal()
        || held.selected_environment_revision != identity.selected_environment_revision
        || now_unix_milliseconds < held.recorded_at_unix_milliseconds
    {
        return Err(DurableLookupRefusal);
    }
    let previous = held.record.outstanding_recovery.as_ref();
    if previous.is_some_and(automatic_recovery_paused) {
        return Ok(held);
    }
    let evidence = previous.map_or(
        RecoveryExecutionEvidence::ExecutionCertainty {
            certainty: OperationExecutionCertainty::RemoteOutcomeUnknown,
        },
        |fact| fact.evidence,
    );
    let attempt_count = previous.map_or(1, |fact| fact.attempt_count.saturating_add(1));
    if u64::from(attempt_count) >= automatic_attempt_cap() {
        return apply_terminal(
            operations,
            &identity.author_target_identity_digest,
            &identity.operation_identifier,
            expected_revision,
            failure_when_the_budget_ends(&evidence),
            now_unix_milliseconds,
        );
    }
    let recovery = RecoveryFact {
        attempt_count,
        category: if evidence == RecoveryExecutionEvidence::AuthoritativeRemoteSuccess {
            RecoveryCategory::ResultAcquisition
        } else {
            RecoveryCategory::OperationLookup
        },
        detail: "the attempt was detached locally and the operation remains outstanding".to_owned(),
        evidence,
        manual_resume_eligible: false,
        retry_delay_milliseconds: {
            use rand::RngExt;
            rand::rng().random_range(0..=jitter_ceiling_milliseconds(u64::from(attempt_count)))
        },
        retry_observed_at_unix_milliseconds: now_unix_milliseconds,
    };
    operations
        .apply(
            &identity.author_target_identity_digest,
            &identity.operation_identifier,
            expected_revision,
            &OperationFact::Recovery { recovery },
            now_unix_milliseconds,
        )
        .map_err(|_| DurableLookupRefusal)
}

/// Charges one failed exchange only after the full retained binding passed.
fn record_lookup_failure(
    operations: &OperationRepository,
    retained: &slingshot_storage::agent_job_repository::AgentSubmission,
    expected_revision: u64,
    now: u64,
) -> Result<OperationSummary, DurableLookupRefusal> {
    record_lookup_recovery(operations, retained, expected_revision, now, false)
}

fn record_lookup_recovery(
    operations: &OperationRepository,
    retained: &slingshot_storage::agent_job_repository::AgentSubmission,
    expected_revision: u64,
    now: u64,
    authoritative_unknown: bool,
) -> Result<OperationSummary, DurableLookupRefusal> {
    let identity = &retained.identity;
    let held = operations
        .read(&identity.author_target_identity_digest, &identity.operation_identifier)
        .map_err(|_| DurableLookupRefusal)?
        .ok_or(DurableLookupRefusal)?;
    if held.record.revision != expected_revision
        || held.record.lifecycle_state.is_terminal()
        || held.selected_environment_revision != identity.selected_environment_revision
    {
        return Err(DurableLookupRefusal);
    }
    let previous = held.record.outstanding_recovery.as_ref();
    if previous.is_some_and(automatic_recovery_paused) {
        return Ok(held);
    }
    let evidence = if authoritative_unknown {
        if previous.is_some_and(|fact| {
            fact.evidence == RecoveryExecutionEvidence::AuthoritativeRemoteSuccess
        }) {
            return Err(DurableLookupRefusal);
        }
        RecoveryExecutionEvidence::ExecutionCertainty {
            certainty: OperationExecutionCertainty::RemoteOutcomeUnknown,
        }
    } else {
        previous.map_or(
            RecoveryExecutionEvidence::ExecutionCertainty {
                certainty: OperationExecutionCertainty::RemoteOutcomeUnknown,
            },
            |fact| fact.evidence,
        )
    };
    let attempt_count = previous.map_or(1, |fact| fact.attempt_count.saturating_add(1));
    if u64::from(attempt_count)
        >= crate::operation::recovery_and_event_supervisor::automatic_attempt_cap()
    {
        return apply_terminal_retained(
            operations,
            retained,
            expected_revision,
            failure_when_the_budget_ends(&evidence),
            now,
        );
    }
    operations
        .apply_for_retained_agent(
            retained,
            expected_revision,
            &OperationFact::Recovery { recovery: next_lookup_recovery(previous, evidence, now) },
            now,
        )
        .map_err(|_| DurableLookupRefusal)
}

/// Ends one operation the author has already answered, or whose retry budget ended.
fn failure_when_the_budget_ends(evidence: &RecoveryExecutionEvidence) -> TerminalFailure {
    if *evidence == RecoveryExecutionEvidence::AuthoritativeRemoteSuccess {
        TerminalFailure {
            kind: TerminalFailureKind::ResultUnavailable,
            disposition: TerminalFailureDisposition::AuthoritativeRemoteSuccess,
            metadata: Some("the author answered and the result was not obtained".to_owned()),
        }
    } else {
        exhausted_without_an_answer()
    }
}

/// Ends one operation whose host never produced an answer inside the retry budget.
fn exhausted_without_an_answer() -> TerminalFailure {
    TerminalFailure {
        kind: TerminalFailureKind::RetryPolicyExhausted,
        disposition: TerminalFailureDisposition::FailClosedIndeterminate {
            certainty: OperationExecutionCertainty::RemoteOutcomeUnknown,
        },
        metadata: Some("the author did not answer before the retry budget ended".to_owned()),
    }
}

fn apply_terminal(
    operations: &OperationRepository,
    author_target_identity_digest: &str,
    operation_identifier: &str,
    expected_revision: u64,
    failure: TerminalFailure,
    now: u64,
) -> Result<OperationSummary, DurableLookupRefusal> {
    operations
        .apply(
            author_target_identity_digest,
            operation_identifier,
            expected_revision,
            &OperationFact::Terminal { failure },
            now,
        )
        .map_err(|_| DurableLookupRefusal)
}

fn apply_terminal_retained(
    operations: &OperationRepository,
    retained: &slingshot_storage::agent_job_repository::AgentSubmission,
    expected_revision: u64,
    failure: TerminalFailure,
    now: u64,
) -> Result<OperationSummary, DurableLookupRefusal> {
    operations
        .apply_for_retained_agent(
            retained,
            expected_revision,
            &OperationFact::Terminal { failure },
            now,
        )
        .map_err(|_| DurableLookupRefusal)
}

/// One bounded attempt for either a logical lookup or a complete physical probe.
pub(super) fn next_lookup_recovery(
    previous: Option<&RecoveryFact>,
    evidence: RecoveryExecutionEvidence,
    now: u64,
) -> RecoveryFact {
    use crate::operation::recovery_and_event_supervisor::{
        automatic_attempt_cap, jitter_ceiling_milliseconds,
    };
    use rand::RngExt;
    let attempt_count = previous.map_or(1, |fact| fact.attempt_count.saturating_add(1));
    let paused = u64::from(attempt_count) >= automatic_attempt_cap();
    RecoveryFact {
        category: if evidence == RecoveryExecutionEvidence::AuthoritativeRemoteSuccess {
            RecoveryCategory::ResultAcquisition
        } else {
            RecoveryCategory::OperationLookup
        },
        evidence,
        attempt_count,
        detail: "author lookup did not produce a usable observation".to_owned(),
        manual_resume_eligible: paused,
        retry_delay_milliseconds: if paused {
            0
        } else {
            rand::rng().random_range(0..=jitter_ceiling_milliseconds(u64::from(attempt_count)))
        },
        retry_observed_at_unix_milliseconds: now,
    }
}

/// Looks up an existing child, never inserting it or sending a replacement.
/// Active snapshots are persisted atomically, missing receipts record grace,
/// and validated retirement settles local recovery. Successful snapshots carrying
/// a valid inline result publish through the guarded completion coordinator;
/// absent results and artifact-backed results leave acquisition pending. Typed
/// configuration/discovery/load/package/creation failures settle only their
/// locally derived no-effect branch. Replication additionally distinguishes
/// partial admission; ambiguous effects remain lookup recovery without replacement sends.
///
/// # Errors
/// Returns `DurableLookupRefusal` for invalid retained identity or evidence,
/// failed author exchange, stale state, or a failed durable update/completion.
pub async fn lookup_retained_operation(
    repository: &AgentJobRepository,
    operations: &OperationRepository,
    expected_operation_revision: u64,
    transport: &SelectedAuthorTransport,
    identity: &ExecutionIdentity,
    submission: &Submission,
    authentication: &RequestAuthentication,
    now_unix_milliseconds: u64,
) -> Result<OperationLookupReceipt, DurableLookupRefusal> {
    lookup_retained_operation_with_completion(
        repository,
        operations,
        expected_operation_revision,
        transport,
        identity,
        submission,
        authentication,
        now_unix_milliseconds,
        None,
    )
    .await
}

/// Lookup with runtime-owned resources for local structured-result externalization.
/// Without these resources, over-inline results remain acquisition-pending.
///
/// # Errors
/// Returns `DurableLookupRefusal` for invalid retained identity or evidence,
/// failed author exchange, stale state, or refused result storage/publication.
pub async fn lookup_retained_operation_with_completion(
    repository: &AgentJobRepository,
    operations: &OperationRepository,
    expected_operation_revision: u64,
    transport: &SelectedAuthorTransport,
    identity: &ExecutionIdentity,
    submission: &Submission,
    authentication: &RequestAuthentication,
    now_unix_milliseconds: u64,
    completion: Option<(
        &slingshot_storage::artifact_store::ArtifactStore,
        &slingshot_storage::persistent_capacity::PersistentCapacityAccount<'_>,
    )>,
) -> Result<OperationLookupReceipt, DurableLookupRefusal> {
    reconcile_retained_operation(
        repository,
        operations,
        expected_operation_revision,
        transport,
        identity,
        submission,
        AuthorAuthentication::Fixed {
            authentication,
            protocol: super::subscription_reset::ResetTransport::Http1,
        },
        now_unix_milliseconds,
        completion,
        None,
    )
    .await
}

/// Lookup and result acquisition over one explicit HTTP mode, without fallback.
/// Capability discovery, snapshot lookup and any artifact download share it.
///
/// # Errors
/// Returns `DurableLookupRefusal` for identity/evidence mismatches, a failed
/// exchange in the selected mode, or refused durable reconciliation/completion.
pub async fn lookup_retained_operation_over(
    repository: &AgentJobRepository,
    operations: &OperationRepository,
    expected_operation_revision: u64,
    transport: &SelectedAuthorTransport,
    identity: &ExecutionIdentity,
    submission: &Submission,
    authentication: &RequestAuthentication,
    now: u64,
    completion: Option<(
        &slingshot_storage::artifact_store::ArtifactStore,
        &slingshot_storage::persistent_capacity::PersistentCapacityAccount<'_>,
    )>,
    protocol: super::subscription_reset::ResetTransport,
) -> Result<OperationLookupReceipt, DurableLookupRefusal> {
    reconcile_retained_operation(
        repository,
        operations,
        expected_operation_revision,
        transport,
        identity,
        submission,
        AuthorAuthentication::Fixed { authentication, protocol },
        now,
        completion,
        None,
    )
    .await
}

/// Looks up and completes retained work using request-scoped provider or fixed
/// authentication. All durable recovery accounting remains in the shared fold.
///
/// # Errors
/// Returns `DurableLookupRefusal` for invalid selection, authentication or
/// evidence, failed author exchange, or refused durable reconciliation/completion.
pub async fn lookup_retained_operation_with_authentication(
    repository: &AgentJobRepository,
    operations: &OperationRepository,
    expected_operation_revision: u64,
    transport: &SelectedAuthorTransport,
    identity: &ExecutionIdentity,
    submission: &Submission,
    authentication: AuthorAuthentication<'_>,
    now: u64,
    completion: Option<(
        &slingshot_storage::artifact_store::ArtifactStore,
        &slingshot_storage::persistent_capacity::PersistentCapacityAccount<'_>,
    )>,
) -> Result<OperationLookupReceipt, DurableLookupRefusal> {
    reconcile_retained_operation(
        repository,
        operations,
        expected_operation_revision,
        transport,
        identity,
        submission,
        authentication,
        now,
        completion,
        None,
    )
    .await
}

/// Consumes sealed terminal evidence under the capturing invocation's policy.
pub(crate) async fn reconcile_captured_lookup_with_authentication(
    repository: &AgentJobRepository,
    operations: &OperationRepository,
    expected_operation_revision: u64,
    transport: &SelectedAuthorTransport,
    identity: &ExecutionIdentity,
    submission: &Submission,
    authentication: AuthorAuthentication<'_>,
    now_unix_milliseconds: u64,
    completion: Option<(
        &slingshot_storage::artifact_store::ArtifactStore,
        &slingshot_storage::persistent_capacity::PersistentCapacityAccount<'_>,
    )>,
    receipt: OperationLookupReceipt,
) -> Result<OperationLookupReceipt, DurableLookupRefusal> {
    if matches!(&receipt, OperationLookupReceipt::Found(found) if !found.snapshot.kind.is_terminal())
    {
        return Err(DurableLookupRefusal);
    }
    reconcile_retained_operation(
        repository,
        operations,
        expected_operation_revision,
        transport,
        identity,
        submission,
        authentication,
        now_unix_milliseconds,
        completion,
        Some(receipt),
    )
    .await
}

/// Only the sealed generation-loss report may pass a physical snapshot here.
/// Logical absence is deliberately not representable on this entry point.
pub(super) async fn reconcile_captured_physical_snapshot(
    repository: &AgentJobRepository,
    operations: &OperationRepository,
    expected_operation_revision: u64,
    transport: &SelectedAuthorTransport,
    identity: &ExecutionIdentity,
    submission: &Submission,
    authentication: AuthorAuthentication<'_>,
    now_unix_milliseconds: u64,
    completion: Option<(
        &slingshot_storage::artifact_store::ArtifactStore,
        &slingshot_storage::persistent_capacity::PersistentCapacityAccount<'_>,
    )>,
    receipt: slingshot_agent_connection::selected_author_lookup::SnapshotLookupReceipt,
) -> Result<OperationLookupReceipt, DurableLookupRefusal> {
    reconcile_retained_operation(
        repository,
        operations,
        expected_operation_revision,
        transport,
        identity,
        submission,
        authentication,
        now_unix_milliseconds,
        completion,
        Some(OperationLookupReceipt::Found(receipt)),
    )
    .await
}
