//! Ordered reconciliation of retained selected-author lookup evidence.

use super::*;
use slingshot_agent_connection::selected_author_lookup::SnapshotLookupReceipt;
use slingshot_storage::agent_job_repository::AgentSubmission;

mod failure;
use failure::reconcile_failed_snapshot;

pub(super) async fn reconcile_retained_operation(
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
    require_local_revision(&local, identity, expected_operation_revision)?;
    let retained = repository
        .read(
            &identity.author_target_identity_digest,
            &submission.operation.agent_operation_identifier,
        )
        .map_err(|_| DurableLookupRefusal)?
        .ok_or(DurableLookupRefusal)?;
    require_retained_submission(&retained, identity, submission)?;
    let failed_exchange = || {
        let elapsed =
            u64::try_from(started.elapsed().as_nanos().div_ceil(NANOSECONDS_PER_MILLISECOND))
                .unwrap_or(u64::MAX);
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
            let elapsed =
                u64::try_from(started.elapsed().as_nanos().div_ceil(NANOSECONDS_PER_MILLISECOND))
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
            reconcile_failed_snapshot(
                repository,
                operations,
                identity,
                submission,
                &local,
                &retained,
                found,
                receipt_was_captured,
                expected_operation_revision,
                now_unix_milliseconds,
                started,
                &failed_exchange,
            )?;
        }
        if found.snapshot.described_state()
            == slingshot_domain::remote_job::AgentJobState::Succeeded
        {
            reconcile_successful_snapshot(
                operations,
                identity,
                submission,
                &retained,
                found,
                expected_operation_revision,
                now_unix_milliseconds,
                started,
                completion,
                transport,
                authentication,
            )
            .await?;
        }
        if !found.snapshot.kind.is_terminal() {
            reconcile_active_snapshot(
                repository,
                &local,
                &retained,
                found,
                expected_operation_revision,
                now_unix_milliseconds,
                started,
            )?;
        }
    }
    Ok(receipt)
}

fn require_local_revision(
    local: &OperationSummary,
    identity: &ExecutionIdentity,
    expected_operation_revision: u64,
) -> Result<(), DurableLookupRefusal> {
    if local.record.revision != expected_operation_revision
        || local.selected_environment_revision != identity.selected_environment_revision
        || local.record.lifecycle_state.is_terminal()
        || local.record.outstanding_recovery.as_ref().is_some_and(automatic_recovery_paused)
    {
        return Err(DurableLookupRefusal);
    }
    Ok(())
}

fn require_retained_submission(
    retained: &AgentSubmission,
    identity: &ExecutionIdentity,
    submission: &Submission,
) -> Result<(), DurableLookupRefusal> {
    if retained.identity.operation_identifier != identity.operation_identifier
        || retained.identity.selected_environment_revision != identity.selected_environment_revision
        || retained.identity.agent_event_store_generation
            != submission.operation.agent_event_store_generation
        || retained.identity.daemon_subscription_identifier
            != submission.daemon_subscription_identifier
        || retained.canonical_submission.as_bytes()
            != submission.wire_body().map_err(|_| DurableLookupRefusal)?
    {
        return Err(DurableLookupRefusal);
    }
    require_retained_contracts(retained, submission)?;
    if retained.terminal_disposition.is_some() {
        return Err(DurableLookupRefusal);
    }
    Ok(())
}

fn require_retained_contracts(
    retained: &AgentSubmission,
    submission: &Submission,
) -> Result<(), DurableLookupRefusal> {
    let contract = &submission.provenance.command_contract;
    let stored = &retained.contracts;
    if stored.submitted_command_digest != submission.submitted_command_digest
        || stored.argument_schema_digest != contract.argument_schema_digest
        || stored.result_schema_digest != contract.result_schema_digest
        || stored.command_contract_limits_digest != contract.command_contract_limits_digest
        || stored.command_semantic_contract_version != contract.command_semantic_contract_version
        || stored.command_wire_name != contract.command_wire_name
        || stored.author_agent_transport_contract_digest
            != submission.provenance.transport_contract_digest
        || stored.command_canonical_json_contract_digest
            != submission.provenance.canonical_json_contract_digest
    {
        return Err(DurableLookupRefusal);
    }
    Ok(())
}

async fn reconcile_successful_snapshot(
    operations: &OperationRepository,
    identity: &ExecutionIdentity,
    submission: &Submission,
    retained: &AgentSubmission,
    found: &SnapshotLookupReceipt,
    expected_operation_revision: u64,
    now_unix_milliseconds: u64,
    started: std::time::Instant,
    completion: Option<(
        &slingshot_storage::artifact_store::ArtifactStore,
        &slingshot_storage::persistent_capacity::PersistentCapacityAccount<'_>,
    )>,
    transport: &SelectedAuthorTransport,
    authentication: AuthorAuthentication<'_>,
) -> Result<(), DurableLookupRefusal> {
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
        retained,
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
            physical_sling_job_identifiers: found.snapshot.physical_sling_job_identifiers.clone(),
            remaining_retention_milliseconds: found.remaining_retention_milliseconds,
        };
        if let Some((store, capacity)) = completion {
            crate::operation::artifact_completion::complete_retained_snapshot_result_with_authentication(
                operations,
                retained,
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
                retained,
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
            retained,
            success.record.revision,
            now_unix_milliseconds.saturating_add(
                u64::try_from(started.elapsed().as_nanos().div_ceil(NANOSECONDS_PER_MILLISECOND))
                    .unwrap_or(u64::MAX),
            ),
        )?;
    }
    Ok(())
}

fn reconcile_active_snapshot(
    repository: &AgentJobRepository,
    local: &OperationSummary,
    retained: &AgentSubmission,
    found: &mut SnapshotLookupReceipt,
    expected_operation_revision: u64,
    now_unix_milliseconds: u64,
    started: std::time::Instant,
) -> Result<(), DurableLookupRefusal> {
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
            retained,
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
    Ok(())
}
