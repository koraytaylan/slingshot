//! Failed snapshots retain uncertainty unless command-specific evidence proves effects.

use super::*;
use slingshot_agent_connection::selected_author_lookup::SnapshotLookupReceipt;
use slingshot_storage::agent_job_repository::AgentSubmission;
pub(super) fn reconcile_failed_snapshot(
    repository: &AgentJobRepository,
    operations: &OperationRepository,
    identity: &ExecutionIdentity,
    submission: &Submission,
    local: &OperationSummary,
    retained: &AgentSubmission,
    found: &SnapshotLookupReceipt,
    receipt_was_captured: bool,
    expected_operation_revision: u64,
    now_unix_milliseconds: u64,
    started: std::time::Instant,
    failed_exchange: &impl Fn() -> DurableLookupRefusal,
) -> Result<(), DurableLookupRefusal> {
    use slingshot_domain::command::catalog::Command;
    let contract = &submission.provenance.command_contract;
    require_failed_snapshot(
        repository,
        identity,
        submission,
        local,
        retained,
        found,
        receipt_was_captured,
        failed_exchange,
    )?;
    let mut arguments: serde_json::Value =
        serde_json::from_str(&submission.canonical_arguments).map_err(|_| failed_exchange())?;
    if arguments
        .as_object_mut()
        .ok_or_else(failed_exchange)?
        .insert("command".to_owned(), contract.command_wire_name.clone().into())
        .is_some()
    {
        return Err(failed_exchange());
    }
    let command: Command = serde_json::from_value(arguments).map_err(|_| failed_exchange())?;
    let document = found.snapshot.terminal_failure.as_ref().ok_or_else(failed_exchange)?;
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
    let (no_effect, category, diagnosis, partial_admission) =
        classify_failure(&command, &body, &expectation, failed_exchange)?;
    let elapsed = u64::try_from(started.elapsed().as_nanos().div_ceil(NANOSECONDS_PER_MILLISECOND))
        .unwrap_or(u64::MAX);
    let now = now_unix_milliseconds.saturating_add(elapsed);
    if no_effect || partial_admission {
        let snapshot = slingshot_storage::agent_job_repository::FailedAgentSnapshot {
            observation: RemoteJobObservation {
                state: found.snapshot.described_state(),
                applied_sequence: found.snapshot.sequence,
                attempt: found.snapshot.attempt,
                progress: found.snapshot.progress,
            },
            physical_sling_job_identifiers: found.snapshot.physical_sling_job_identifiers.clone(),
            remaining_retention_milliseconds: found.remaining_retention_milliseconds,
        };
        if partial_admission {
            operations.settle_partial_admission_snapshot(
                retained,
                expected_operation_revision,
                &snapshot,
                category,
                now,
            )
        } else {
            operations.settle_rejected_agent_snapshot(
                retained,
                expected_operation_revision,
                &snapshot,
                diagnosis,
                category,
                now,
            )
        }
        .map_err(|_| DurableLookupRefusal)?;
    } else {
        // Leave the remote child open for same-operation reconciliation;
        // never convert the failed wire state into known effect evidence.
        record_lookup_recovery(operations, retained, expected_operation_revision, now, true)?;
    }
    Ok(())
}

fn require_failed_snapshot(
    repository: &AgentJobRepository,
    identity: &ExecutionIdentity,
    submission: &Submission,
    local: &OperationSummary,
    retained: &AgentSubmission,
    found: &SnapshotLookupReceipt,
    receipt_was_captured: bool,
    failed_exchange: &impl Fn() -> DurableLookupRefusal,
) -> Result<(), DurableLookupRefusal> {
    if local
        .record
        .outstanding_recovery
        .as_ref()
        .is_some_and(|fact| fact.evidence == RecoveryExecutionEvidence::AuthoritativeRemoteSuccess)
        || found.snapshot.sequence < retained.snapshot_watermark
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
    if held_jobs
        .iter()
        .any(|name| found.snapshot.physical_sling_job_identifiers.binary_search(name).is_err())
    {
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
    Ok(())
}

fn classify_failure(
    command: &slingshot_domain::command::catalog::Command,
    body: &[u8],
    expectation: &slingshot_agent_connection::structured_job_result::ResultExpectation,
    failed_exchange: &impl Fn() -> DurableLookupRefusal,
) -> Result<
    (
        bool,
        Option<String>,
        Option<slingshot_storage::agent_job_repository::RejectedAgentDiagnosis>,
        bool,
    ),
    DurableLookupRefusal,
> {
    use slingshot_agent_connection::terminal_failure::{
        decode_configuration_failure, decode_creation_failure, decode_load_failure,
        decode_package_failure,
    };
    use slingshot_domain::command::catalog::Command;
    // Plan 0005's local mapping, never a wire-selected disposition.
    let (no_effect, category, diagnosis, partial_admission) = match command {
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
            let failure = slingshot_agent_connection::terminal_failure::decode_read_failure(
                body,
                expectation,
                command,
            )
            .map_err(|_| failed_exchange())?;
            (true, Some(failure.category().to_owned()), None, false)
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
            let failure = slingshot_agent_connection::terminal_failure::decode_mutation_failure(
                body,
                expectation,
                command,
            )
            .map_err(|_| failed_exchange())?;
            (failure.proves_no_effect(), Some(failure.category().to_owned()), None, false)
        }
        Command::ReplicateContent(_) => {
            use slingshot_agent_connection::terminal_failure::{
                ReplicationFailureEffect, decode_replication_failure,
            };
            let failure = decode_replication_failure(body, expectation, command)
                .map_err(|_| failed_exchange())?;
            let effect = failure.effect();
            (
                effect == ReplicationFailureEffect::NoAdmission,
                Some(failure.category().to_owned()),
                None,
                effect == ReplicationFailureEffect::PartialAdmission,
            )
        }
        Command::CreatePage(_) | Command::AddComponent(_) => {
            let failure = decode_creation_failure(body, expectation, command)
                .map_err(|_| failed_exchange())?;
            (failure.proves_no_effect(), Some(failure.category().to_owned()), None, false)
        }
        Command::LoadContentAsJson(_) => {
            let failure =
                decode_load_failure(body, expectation, command).map_err(|_| failed_exchange())?;
            (true, Some(failure.category().to_owned()), None, false)
        }
        Command::InspectOpenServiceGatewayInitiativeConfiguration(_) => {
            let failure = decode_configuration_failure(body, expectation, command)
                .map_err(|_| failed_exchange())?;
            (true, Some(failure.category().to_owned()), None, false)
        }
        Command::QueryPaths(_)
        | Command::FindPagesByTemplate(_)
        | Command::FindPagesContainingPhrase(_)
        | Command::FindPagesUsingComponents(_)
        | Command::FindAssetsByMetadata(_)
        | Command::FindAssetsReferencedByPage(_) => {
            let failure = slingshot_agent_connection::terminal_failure::decode_discovery_failure(
                body,
                expectation,
                command,
            )
            .map_err(|_| failed_exchange())?;
            (true, Some(failure.category().to_owned()), None, false)
        }
        Command::DownloadContentPackage(_) => {
            let failure = decode_package_failure(body, expectation, command)
                .map_err(|_| failed_exchange())?;
            let diagnosis = if failure.category() == "staging_cleanup_failed" {
                Some(slingshot_storage::agent_job_repository::RejectedAgentDiagnosis::PackageStagingCleanupRequired)
            } else {
                None
            };
            (failure.proves_no_publication(), Some(failure.category().to_owned()), diagnosis, false)
        }
    };
    Ok((no_effect, category, diagnosis, partial_admission))
}
