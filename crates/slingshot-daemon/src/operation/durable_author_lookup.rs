//! Lookup-first recovery over an existing, byte-identical remote child.

use super::author_authentication::AuthorAuthentication;
use slingshot_agent_connection::authentication::environment_provider::RequestAuthentication;
use slingshot_agent_connection::command_submission::Submission;
use slingshot_agent_connection::selected_author_lookup::OperationLookupReceipt;
use slingshot_agent_connection::selected_author_transport::SelectedAuthorTransport;
use slingshot_domain::operation::{
    OperationExecutionCertainty, OperationFact, RecoveryCategory, RecoveryExecutionEvidence,
    RecoveryFact,
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
mod pause_tests {
    use super::*;

    #[test]
    fn capacity_pauses_immediately_but_eligibility_alone_does_not_pause_other_retries() {
        let mut fact = RecoveryFact {
            attempt_count: 0,
            category: RecoveryCategory::PersistentCapacityUnavailable,
            detail: "capacity unavailable".to_owned(),
            evidence: RecoveryExecutionEvidence::AuthoritativeRemoteSuccess,
            manual_resume_eligible: true,
            retry_delay_milliseconds: 0,
            retry_observed_at_unix_milliseconds: 1,
        };
        assert!(automatic_recovery_paused(&fact));
        fact.category = RecoveryCategory::ResultAcquisition;
        assert!(!automatic_recovery_paused(&fact));
        fact.attempt_count =
            u32::try_from(crate::operation::recovery_and_event_supervisor::automatic_attempt_cap())
                .unwrap();
        assert!(automatic_recovery_paused(&fact));
        fact.manual_resume_eligible = false;
        assert!(!automatic_recovery_paused(&fact));
    }
}

/// Recovery could not establish and persist a consistent selected-author fact.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[error("the retained author lookup could not be reconciled")]
pub struct DurableLookupRefusal;

/// Activates one admitted resume against the retained command and remote child.
/// A returned revision may enter the selected recovery category; this grants no
/// submission permit or scheduler lease. Receipt replay returns no activation.
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
    let grace_end = request_start_unix_milliseconds
        .checked_add(missing_grace_milliseconds())
        .ok_or(DurableLookupRefusal)?;
    let grace_remaining = grace_end.saturating_sub(now_unix_milliseconds);
    let paused = u64::from(attempt_count) >= automatic_attempt_cap();
    let recovery = RecoveryFact {
        attempt_count,
        category: RecoveryCategory::OperationLookup,
        detail: if paused {
            "operation lookup requires manual recovery"
        } else if grace_remaining > 0 {
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
        manual_resume_eligible: paused,
        retry_delay_milliseconds: if paused {
            0
        } else {
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
    operations
        .apply_for_retained_agent(
            retained,
            expected_revision,
            &OperationFact::Recovery { recovery: next_lookup_recovery(previous, evidence, now) },
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

async fn reconcile_retained_operation(
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
    captured: Option<OperationLookupReceipt>,
) -> Result<OperationLookupReceipt, DurableLookupRefusal> {
    let started = std::time::Instant::now();
    authentication.require_execution(identity).map_err(|_| DurableLookupRefusal)?;
    if !repository.database().shares_database_with(operations.database()) {
        return Err(DurableLookupRefusal);
    }
    transport.require_submission(identity, submission).map_err(|_| DurableLookupRefusal)?;
    let local = retained_command(operations, identity, submission)?;
    if local.record.revision != expected_operation_revision
        || local.selected_environment_revision != identity.selected_environment_revision
        || local.record.lifecycle_state.is_terminal()
        || local.record.outstanding_recovery.as_ref().is_some_and(automatic_recovery_paused)
    {
        return Err(DurableLookupRefusal);
    }
    let retained = repository
        .read(
            &identity.author_target_identity_digest,
            &submission.operation.agent_operation_identifier,
        )
        .map_err(|_| DurableLookupRefusal)?
        .ok_or(DurableLookupRefusal)?;
    let contract = &submission.provenance.command_contract;
    let stored = &retained.contracts;
    if retained.identity.operation_identifier != identity.operation_identifier
        || retained.identity.selected_environment_revision != identity.selected_environment_revision
        || retained.identity.agent_event_store_generation
            != submission.operation.agent_event_store_generation
        || retained.identity.daemon_subscription_identifier
            != submission.daemon_subscription_identifier
        || retained.canonical_submission.as_bytes()
            != submission.wire_body().map_err(|_| DurableLookupRefusal)?
        || stored.submitted_command_digest != submission.submitted_command_digest
        || stored.argument_schema_digest != contract.argument_schema_digest
        || stored.result_schema_digest != contract.result_schema_digest
        || stored.command_contract_limits_digest != contract.command_contract_limits_digest
        || stored.command_semantic_contract_version != contract.command_semantic_contract_version
        || stored.command_wire_name != contract.command_wire_name
        || stored.author_agent_transport_contract_digest
            != submission.provenance.transport_contract_digest
        || stored.command_canonical_json_contract_digest
            != submission.provenance.canonical_json_contract_digest
        || retained.terminal_disposition.is_some()
    {
        return Err(DurableLookupRefusal);
    }
    let failed_exchange = || {
        let elapsed =
            u64::try_from(started.elapsed().as_nanos().div_ceil(1_000_000)).unwrap_or(u64::MAX);
        let _ = record_lookup_failure(
            operations,
            &retained,
            expected_operation_revision,
            now_unix_milliseconds.saturating_add(elapsed),
        );
        DurableLookupRefusal
    };
    let receipt_was_captured = captured.is_some();
    let mut receipt = if let Some(mut receipt) = captured {
        if let OperationLookupReceipt::Found(found) = &mut receipt {
            let elapsed = u64::try_from(started.elapsed().as_nanos().div_ceil(1_000_000))
                .map_err(|_| failed_exchange())?;
            // Expiry removes remote acquisition time, not already validated
            // terminal truth. Complete retained terminal bodies can still settle.
            found.remaining_retention_milliseconds =
                found.remaining_retention_milliseconds.saturating_sub(elapsed);
        }
        receipt
    } else {
        let capabilities = authentication.discover(transport, identity, submission).await;
        capabilities.map_err(|_| failed_exchange())?;
        authentication
            .lookup(transport, identity, submission)
            .await
            .map_err(|_| failed_exchange())?
    };
    if matches!(
        &receipt,
        OperationLookupReceipt::Absent(
            slingshot_agent_connection::job_snapshot_reconciliation::LookupAnswer::Missing
        )
    ) {
        let elapsed = u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX);
        record_missing_lookup(
            operations,
            identity,
            expected_operation_revision,
            retained.request_start_unix_milliseconds,
            now_unix_milliseconds.saturating_add(elapsed),
        )?;
    }
    if let OperationLookupReceipt::Absent(
        slingshot_agent_connection::job_snapshot_reconciliation::LookupAnswer::Retired(echo),
    ) = &receipt
    {
        let elapsed = u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX);
        record_retired_lookup(
            operations,
            &retained,
            identity,
            submission,
            echo,
            expected_operation_revision,
            now_unix_milliseconds.saturating_add(elapsed),
        )?;
    }
    if let OperationLookupReceipt::Found(found) = &mut receipt {
        if found.snapshot.described_state() == slingshot_domain::remote_job::AgentJobState::Failed {
            use slingshot_agent_connection::terminal_failure::{
                decode_configuration_failure, decode_creation_failure, decode_load_failure,
                decode_package_failure,
            };
            use slingshot_domain::command::catalog::Command;
            if local.record.outstanding_recovery.as_ref().is_some_and(|fact| {
                fact.evidence == RecoveryExecutionEvidence::AuthoritativeRemoteSuccess
            }) || found.snapshot.sequence < retained.snapshot_watermark
                || (found.remaining_retention_milliseconds == 0 && !receipt_was_captured)
                || (found.snapshot.sequence == retained.observation.applied_sequence
                    && RemoteJobObservation {
                        state: found.snapshot.described_state(),
                        applied_sequence: found.snapshot.sequence,
                        attempt: found.snapshot.attempt,
                        progress: found.snapshot.progress,
                    } != retained.observation)
            {
                return Err(failed_exchange());
            }
            let held_jobs = repository
                .physical_jobs(
                    &identity.author_target_identity_digest,
                    &submission.operation.agent_operation_identifier,
                )
                .map_err(|_| failed_exchange())?;
            if held_jobs.iter().any(|name| {
                found.snapshot.physical_sling_job_identifiers.binary_search(name).is_err()
            }) {
                return Err(failed_exchange());
            }
            retained
                .observation
                .advanced(
                    found.snapshot.described_state(),
                    found.snapshot.sequence,
                    found.snapshot.attempt,
                    found.snapshot.progress,
                )
                .map_err(|_| failed_exchange())?;
            let mut arguments: serde_json::Value =
                serde_json::from_str(&submission.canonical_arguments)
                    .map_err(|_| failed_exchange())?;
            if arguments
                .as_object_mut()
                .ok_or_else(&failed_exchange)?
                .insert("command".to_owned(), contract.command_wire_name.clone().into())
                .is_some()
            {
                return Err(failed_exchange());
            }
            let command: Command =
                serde_json::from_value(arguments).map_err(|_| failed_exchange())?;
            let document = found.snapshot.terminal_failure.as_ref().ok_or_else(&failed_exchange)?;
            let body = serde_json::to_vec(document).map_err(|_| failed_exchange())?;
            let expectation = slingshot_agent_connection::structured_job_result::ResultExpectation {
                operation: submission.operation.clone(),
                daemon_subscription_identifier: submission.daemon_subscription_identifier.clone(),
                expected_provenance: slingshot_agent_protocol::wire_contract::ExpectedProvenance {
                    command_contract: slingshot_domain::selected_command_contract_identity::SelectedCommandContractIdentity::installed(command.wire_name()).map_err(|_| failed_exchange())?,
                    canonical_json_contract_digest: submission.provenance.canonical_json_contract_digest.clone(),
                    transport_contract_digest: submission.provenance.transport_contract_digest.clone(),
                },
                submitted_command_digest: submission.submitted_command_digest.clone(),
                wire_name: contract.command_wire_name.clone(),
            };
            // Plan 0005's local mapping, never a wire-selected disposition.
            let mut diagnosis = None;
            let mut partial_admission = false;
            let no_effect = match &command {
                Command::InspectSlingJob(_)
                | Command::InspectWorkflowInstance(_)
                | Command::InspectReplicationAgent(_)
                | Command::ResolveResourcePath(_)
                | Command::MapResourcePath(_)
                | Command::ListChildPages(_)
                | Command::ListGroupMembers(_)
                | Command::ListAssetRenditions(_)
                | Command::InspectReplicationQueue(_)
                | Command::ReadContentFragment(_)
                | Command::FindOpenServiceGatewayInitiativeConfigurations(_)
                | Command::FindSlingJobs(_)
                | Command::FindWorkflowInstances(_)
                | Command::ListOpenServiceGatewayInitiativeBundles(_)
                | Command::ListOpenServiceGatewayInitiativeComponents(_)
                | Command::ListReplicationAgents(_)
                | Command::ListResourceMappings(_)
                | Command::ListSlingJobQueues(_)
                | Command::ListWorkflowModels(_) => {
                    slingshot_agent_connection::terminal_failure::decode_read_failure(
                        &body,
                        &expectation,
                        &command,
                    )
                    .map_err(|_| failed_exchange())?;
                    true
                }
                Command::UpdatePage(_)
                | Command::MovePage(_)
                | Command::DeletePage(_)
                | Command::UpdateComponent(_)
                | Command::DeleteComponent(_)
                | Command::ReorderComponent(_)
                | Command::CreateAsset(_)
                | Command::CreateAssetFolder(_)
                | Command::MoveAsset(_)
                | Command::DeleteAsset(_)
                | Command::UpdateAssetMetadata(_)
                | Command::CreateContentFragment(_)
                | Command::UpdateContentFragment(_)
                | Command::DeleteContentFragment(_)
                | Command::CreateExperienceFragment(_)
                | Command::UpdateExperienceFragment(_)
                | Command::DeleteExperienceFragment(_)
                | Command::CreateUser(_)
                | Command::CreateGroup(_)
                | Command::DeleteAuthorizable(_)
                | Command::UpdateUserProfile(_)
                | Command::SetUserDisabled(_)
                | Command::AddGroupMember(_)
                | Command::RemoveGroupMember(_)
                | Command::CancelSlingJob(_)
                | Command::StartWorkflow(_)
                | Command::TerminateWorkflowInstance(_)
                | Command::SetWorkflowInstanceSuspension(_)
                | Command::FlushReplicationQueue(_)
                | Command::RetryReplicationQueueEntry(_)
                | Command::UpdateOpenServiceGatewayInitiativeConfiguration(_)
                | Command::DeleteOpenServiceGatewayInitiativeConfiguration(_)
                | Command::SetOpenServiceGatewayInitiativeBundleState(_) => {
                    slingshot_agent_connection::terminal_failure::decode_mutation_failure(
                        &body,
                        &expectation,
                        &command,
                    )
                    .map_err(|_| failed_exchange())?
                    .proves_no_effect()
                }
                Command::ReplicateContent(_) => {
                    use slingshot_agent_connection::terminal_failure::{
                        ReplicationFailureEffect, decode_replication_failure,
                    };
                    let effect = decode_replication_failure(&body, &expectation, &command)
                        .map_err(|_| failed_exchange())?
                        .effect();
                    partial_admission = effect == ReplicationFailureEffect::PartialAdmission;
                    effect == ReplicationFailureEffect::NoAdmission
                }
                Command::CreatePage(_) | Command::AddComponent(_) => {
                    decode_creation_failure(&body, &expectation, &command)
                        .map_err(|_| failed_exchange())?
                        .proves_no_effect()
                }
                Command::LoadContentAsJson(_) => {
                    decode_load_failure(&body, &expectation, &command)
                        .map_err(|_| failed_exchange())?;
                    true
                }
                Command::InspectOpenServiceGatewayInitiativeConfiguration(_) => {
                    decode_configuration_failure(&body, &expectation, &command)
                        .map_err(|_| failed_exchange())?;
                    true
                }
                Command::QueryPaths(_)
                | Command::FindPagesByTemplate(_)
                | Command::FindPagesContainingPhrase(_)
                | Command::FindPagesUsingComponents(_)
                | Command::FindAssetsByMetadata(_)
                | Command::FindAssetsReferencedByPage(_) => {
                    slingshot_agent_connection::terminal_failure::decode_discovery_failure(
                        &body,
                        &expectation,
                        &command,
                    )
                    .map_err(|_| failed_exchange())?;
                    true
                }
                Command::DownloadContentPackage(_) => {
                    let failure = decode_package_failure(&body, &expectation, &command)
                        .map_err(|_| failed_exchange())?;
                    if failure.category() == "staging_cleanup_failed" {
                        diagnosis = Some(slingshot_storage::agent_job_repository::RejectedAgentDiagnosis::PackageStagingCleanupRequired);
                    }
                    failure.proves_no_publication()
                }
            };
            let elapsed =
                u64::try_from(started.elapsed().as_nanos().div_ceil(1_000_000)).unwrap_or(u64::MAX);
            let now = now_unix_milliseconds.saturating_add(elapsed);
            if no_effect || partial_admission {
                let snapshot = slingshot_storage::agent_job_repository::FailedAgentSnapshot {
                    observation: RemoteJobObservation {
                        state: found.snapshot.described_state(),
                        applied_sequence: found.snapshot.sequence,
                        attempt: found.snapshot.attempt,
                        progress: found.snapshot.progress,
                    },
                    physical_sling_job_identifiers: found
                        .snapshot
                        .physical_sling_job_identifiers
                        .clone(),
                    remaining_retention_milliseconds: found.remaining_retention_milliseconds,
                };
                if partial_admission {
                    operations.settle_partial_admission_snapshot(
                        &retained,
                        expected_operation_revision,
                        &snapshot,
                        now,
                    )
                } else {
                    operations.settle_rejected_agent_snapshot(
                        &retained,
                        expected_operation_revision,
                        &snapshot,
                        diagnosis,
                        now,
                    )
                }
                .map_err(|_| DurableLookupRefusal)?;
            } else {
                // Leave the remote child open for same-operation reconciliation;
                // never convert the failed wire state into known effect evidence.
                record_lookup_recovery(
                    operations,
                    &retained,
                    expected_operation_revision,
                    now,
                    true,
                )?;
            }
        }
        if found.snapshot.described_state()
            == slingshot_domain::remote_job::AgentJobState::Succeeded
        {
            if found.snapshot.sequence < retained.snapshot_watermark {
                return Err(DurableLookupRefusal);
            }
            retained
                .observation
                .advanced(
                    found.snapshot.described_state(),
                    found.snapshot.sequence,
                    found.snapshot.attempt,
                    found.snapshot.progress,
                )
                .map_err(|_| DurableLookupRefusal)?;
            let elapsed = u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX);
            let success = record_snapshot_success(
                operations,
                &retained,
                identity,
                submission,
                expected_operation_revision,
                now_unix_milliseconds.saturating_add(elapsed),
            )?;
            if let Some(result) = &found.snapshot.terminal_result {
                let body = serde_json::to_vec(result).map_err(|_| DurableLookupRefusal)?;
                // Persist remote success first. A refused/oversized result must
                // not turn local acquisition trouble into remote nonexecution.
                let snapshot = slingshot_storage::agent_job_repository::SuccessfulAgentSnapshot {
                    observation: RemoteJobObservation {
                        state: found.snapshot.described_state(),
                        applied_sequence: found.snapshot.sequence,
                        attempt: found.snapshot.attempt,
                        progress: found.snapshot.progress,
                    },
                    physical_sling_job_identifiers: found
                        .snapshot
                        .physical_sling_job_identifiers
                        .clone(),
                    remaining_retention_milliseconds: found.remaining_retention_milliseconds,
                };
                if let Some((store, capacity)) = completion {
                    crate::operation::artifact_completion::complete_retained_snapshot_result_with_authentication(
                        operations,
                        &retained,
                        success.record.revision,
                        identity,
                        submission,
                        &body,
                        now_unix_milliseconds.saturating_add(elapsed),
                        &snapshot,
                        store,
                        capacity,
                        transport,
                        authentication,
                    )
                    .await
                    .map_err(|_| DurableLookupRefusal)?;
                } else {
                    crate::operation::artifact_completion::publish_retained_inline_snapshot(
                        operations,
                        &retained,
                        success.record.revision,
                        identity,
                        submission,
                        &body,
                        now_unix_milliseconds.saturating_add(elapsed),
                        &snapshot,
                    )
                    .map_err(|_| DurableLookupRefusal)?;
                }
            } else {
                // A valid ending without its result is still an incomplete
                // acquisition attempt. Preserve authoritative success, but do
                // not let repeated identical snapshots bypass retry exhaustion.
                record_lookup_failure(
                    operations,
                    &retained,
                    success.record.revision,
                    now_unix_milliseconds.saturating_add(
                        u64::try_from(started.elapsed().as_nanos().div_ceil(1_000_000))
                            .unwrap_or(u64::MAX),
                    ),
                )?;
            }
        }
        if !found.snapshot.kind.is_terminal() {
            if local.record.outstanding_recovery.as_ref().is_some_and(|recovery| {
                recovery.evidence == RecoveryExecutionEvidence::AuthoritativeRemoteSuccess
            }) {
                return Err(DurableLookupRefusal);
            }
            let observation = RemoteJobObservation {
                applied_sequence: found.snapshot.sequence,
                attempt: found.snapshot.attempt,
                progress: found.snapshot.progress,
                state: found.snapshot.described_state(),
            };
            let elapsed = u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX);
            found.remaining_retention_milliseconds = repository
                .reconcile_active_snapshot_for_operation(
                    &retained,
                    expected_operation_revision,
                    &found.snapshot.physical_sling_job_identifiers,
                    observation,
                    found.remaining_retention_milliseconds,
                    now_unix_milliseconds.saturating_add(elapsed),
                )
                .map_err(|_| DurableLookupRefusal)?;
            if found.remaining_retention_milliseconds == 0 {
                return Err(DurableLookupRefusal);
            }
        }
    }
    Ok(receipt)
}

/// Records authoritative remote success while the result itself is still
/// unavailable. Repeated proof cannot reset an existing acquisition schedule.
fn record_snapshot_success(
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
    if retained.identity.operation_identifier != identity.operation_identifier
        || retained.identity.author_target_identity_digest != identity.author_target_identity_digest
        || retained.identity.selected_environment_revision != identity.selected_environment_revision
        || retained.canonical_submission.as_bytes()
            != submission.wire_body().map_err(|_| DurableLookupRefusal)?
        || echo.provenance != submission.provenance
        || echo.agent_event_store_generation != submission.operation.agent_event_store_generation
        || echo.agent_operation_identifier != submission.operation.agent_operation_identifier
        || echo.author_target_identity_digest != identity.author_target_identity_digest
        || echo.selected_environment_revision != identity.selected_environment_revision
        || echo.daemon_subscription_identifier != submission.daemon_subscription_identifier
        || echo.submitted_command_digest != submission.submitted_command_digest
    {
        return Err(DurableLookupRefusal);
    }
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
