//! Terminal evidence for retained selected-author recovery.

use super::*;

/// Records authoritative remote success while the result itself is still
/// unavailable. Repeated proof cannot reset an existing acquisition schedule.
pub(super) fn record_snapshot_success(
    operations: &OperationRepository,
    retained: &slingshot_storage::agent_job_repository::AgentSubmission,
    identity: &ExecutionIdentity,
    submission: &Submission,
    expected_revision: u64,
    now_unix_milliseconds: u64,
) -> Result<OperationSummary, DurableLookupRefusal> {
    let held = retained_command(operations, identity, submission)?;
    if held.record.revision != expected_revision
        || held.record.lifecycle_state.is_terminal()
        || held.selected_environment_revision != identity.selected_environment_revision
    {
        return Err(DurableLookupRefusal);
    }
    if held
        .record
        .outstanding_recovery
        .as_ref()
        .is_some_and(|fact| fact.evidence == RecoveryExecutionEvidence::AuthoritativeRemoteSuccess)
    {
        return Ok(held);
    }
    operations
        .apply_for_retained_agent(
            retained,
            expected_revision,
            &OperationFact::Recovery {
                recovery: RecoveryFact {
                    attempt_count: 0,
                    category: RecoveryCategory::ResultAcquisition,
                    detail: "remote work succeeded; result acquisition remains pending".to_owned(),
                    evidence: RecoveryExecutionEvidence::AuthoritativeRemoteSuccess,
                    manual_resume_eligible: false,
                    retry_delay_milliseconds: 0,
                    retry_observed_at_unix_milliseconds: now_unix_milliseconds,
                },
            },
            now_unix_milliseconds,
        )
        .map_err(|_| DurableLookupRefusal)
}

/// Persists an identity-checked tombstone without converting uncertainty into
/// nonexecution or retracting a previously established remote success.
///
/// # Errors
/// Returns `DurableLookupRefusal` when the retained bytes, execution identity,
/// or tombstone echo disagree; the local command or revision is stale or
/// terminal; or the repository cannot read or atomically persist the result.
pub fn record_retired_lookup(
    operations: &OperationRepository,
    retained: &slingshot_storage::agent_job_repository::AgentSubmission,
    identity: &ExecutionIdentity,
    submission: &Submission,
    echo: &slingshot_agent_connection::job_snapshot_reconciliation::SnapshotEcho,
    expected_revision: u64,
    now_unix_milliseconds: u64,
) -> Result<OperationSummary, DurableLookupRefusal> {
    use slingshot_domain::operation::{
        TerminalFailure, TerminalFailureDisposition, TerminalFailureKind,
    };
    slingshot_agent_connection::selected_author_submission::require_submission_derivation(
        identity, submission,
    )
    .map_err(|_| DurableLookupRefusal)?;
    require_retained_identity(retained, identity, submission)?;
    require_retired_echo(echo, identity, submission)?;
    let held = retained_command(operations, identity, submission)?;
    if held.record.revision != expected_revision
        || held.record.lifecycle_state.is_terminal()
        || held.selected_environment_revision != identity.selected_environment_revision
    {
        return Err(DurableLookupRefusal);
    }
    let proven_success = held.record.outstanding_recovery.as_ref().is_some_and(|recovery| {
        recovery.evidence == RecoveryExecutionEvidence::AuthoritativeRemoteSuccess
    });
    let failure = if proven_success {
        TerminalFailure {
            kind: TerminalFailureKind::ResultUnavailable,
            disposition: TerminalFailureDisposition::AuthoritativeRemoteSuccess,
            metadata: None,
        }
    } else {
        TerminalFailure {
            kind: TerminalFailureKind::RecoveryWindowExpired,
            disposition: TerminalFailureDisposition::FailClosedIndeterminate {
                certainty: OperationExecutionCertainty::RemoteOutcomeUnknown,
            },
            metadata: None,
        }
    };
    operations
        .apply_for_retained_agent(
            retained,
            expected_revision,
            &OperationFact::Terminal { failure },
            now_unix_milliseconds,
        )
        .map_err(|_| DurableLookupRefusal)
}

fn require_retained_identity(
    retained: &slingshot_storage::agent_job_repository::AgentSubmission,
    identity: &ExecutionIdentity,
    submission: &Submission,
) -> Result<(), DurableLookupRefusal> {
    if retained.identity.operation_identifier != identity.operation_identifier
        || retained.identity.author_target_identity_digest != identity.author_target_identity_digest
        || retained.identity.selected_environment_revision != identity.selected_environment_revision
        || retained.canonical_submission.as_bytes()
            != submission.wire_body().map_err(|_| DurableLookupRefusal)?
    {
        return Err(DurableLookupRefusal);
    }
    Ok(())
}

fn require_retired_echo(
    echo: &slingshot_agent_connection::job_snapshot_reconciliation::SnapshotEcho,
    identity: &ExecutionIdentity,
    submission: &Submission,
) -> Result<(), DurableLookupRefusal> {
    if echo.provenance != submission.provenance
        || echo.agent_event_store_generation != submission.operation.agent_event_store_generation
        || echo.agent_operation_identifier != submission.operation.agent_operation_identifier
        || echo.author_target_identity_digest != identity.author_target_identity_digest
        || echo.selected_environment_revision != identity.selected_environment_revision
        || echo.daemon_subscription_identifier != submission.daemon_subscription_identifier
        || echo.submitted_command_digest != submission.submitted_command_digest
    {
        return Err(DurableLookupRefusal);
    }
    Ok(())
}
