//! Reserving before fetching, and publishing only what was proved.
//!
//! Capacity is reserved before the request is issued and before a staging file
//! exists, because a transfer that discovers there is no room has already spent
//! the disk and the bandwidth it was meant to protect. A refusal therefore
//! reads no body, creates no file, publishes nothing, and schedules nothing
//! automatic: it records that the work succeeded remotely and waits for a
//! person to make room.
//!
//! # A failed attempt keeps its names
//!
//! The partial file goes and the uncommitted reservation is released, but the
//! mapping from operation and slot to local artifact stays. So a retry - live
//! or after a restart - asks for the same artifact under the same identifiers
//! rather than allocating new ones, and a second, different body for an
//! identifier already verified is an integrity conflict that preserves the
//! original rather than replacing it.

/// Structural guard for the streamed canonical result document.
const STRUCTURED_RESULT_NESTING_DEPTH: usize = 128;

use slingshot_agent_connection::artifact_download::{
    ArtifactTransfer, DownloadRefusal, ExpectedArtifact, TransferEnd,
};

fn age_success_snapshot(
    snapshot: &slingshot_storage::agent_job_repository::SuccessfulAgentSnapshot,
    now: u64,
    elapsed: std::time::Duration,
) -> (slingshot_storage::agent_job_repository::SuccessfulAgentSnapshot, u64) {
    let milliseconds = u64::try_from(elapsed.as_nanos().div_ceil(1_000_000)).unwrap_or(u64::MAX);
    let mut snapshot = snapshot.clone();
    snapshot.remaining_retention_milliseconds =
        snapshot.remaining_retention_milliseconds.saturating_sub(milliseconds);
    (snapshot, now.saturating_add(milliseconds))
}

#[cfg(test)]
mod completion_age_tests {
    use super::age_success_snapshot;
    use std::time::Duration;

    const SNAPSHOT_RECORDED_AT: u64 = 100;

    #[test]
    fn elapsed_completion_time_rounds_up_without_refreshing_retention() {
        let snapshot = slingshot_storage::agent_job_repository::SuccessfulAgentSnapshot {
            observation: slingshot_domain::remote_job::RemoteJobObservation::accepted(),
            physical_sling_job_identifiers: vec!["job-one".to_owned()],
            remaining_retention_milliseconds: 2,
        };
        for (elapsed, remaining, now) in [
            (Duration::ZERO, 2, SNAPSHOT_RECORDED_AT),
            (Duration::from_nanos(1), 1, 101),
            (Duration::from_millis(1), 1, 101),
            (Duration::from_nanos(1_000_001), 0, 102),
            (Duration::from_secs(1), 0, 1100),
            (Duration::from_secs(u64::MAX), 0, u64::MAX),
        ] {
            let (aged, recorded_at) =
                age_success_snapshot(&snapshot, SNAPSHOT_RECORDED_AT, elapsed);
            assert_eq!(aged.remaining_retention_milliseconds, remaining);
            assert_eq!(recorded_at, now);
            assert_eq!(aged.observation, snapshot.observation);
            assert_eq!(
                aged.physical_sling_job_identifiers,
                snapshot.physical_sling_job_identifiers
            );
        }
        assert_eq!(snapshot.remaining_retention_milliseconds, 2);
    }
}

/// A remote transfer failed without publishing bytes or changing remote truth.
#[derive(Debug, thiserror::Error)]
pub enum RemoteStageRefusal {
    /// Capacity must be admitted before staging or contacting the author.
    #[error("remote artifact capacity was refused")]
    Capacity,
    /// Selected identity, transfer or private staging verification failed.
    #[error("remote artifact staging failed")]
    Verification,
    /// Complete identity-checked upstream refusal. Staging and reservation are
    /// abandoned; durable grace/terminal classification belongs to the owner.
    #[error("remote artifact is unavailable")]
    Unavailable {
        /// Verified identity-bearing evidence, never a bare status.
        evidence: slingshot_agent_connection::artifact_download::ValidatedArtifactUnavailable,
        /// Time spent on this request before receiving the verified refusal.
        elapsed_milliseconds: u64,
    },
}

/// Transfers into private staging and hands its charge to a durable publication
/// hold only after the transport receipt and storage digest checks both pass.
/// The caller must derive `request` and `expected` from a validated retained
/// terminal result. Loaded JSON passes incremental canonical and closed typed
/// document validation against the selected submission. This does not publish
/// or settle work, or replace the caller's retained terminal-manifest gate.
pub async fn stage_remote_artifact<'store>(
    transport: &slingshot_agent_connection::selected_author_transport::SelectedAuthorTransport,
    identity: &slingshot_domain::operation_executor::ExecutionIdentity,
    submission: &slingshot_agent_connection::command_submission::Submission,
    authentication: &slingshot_agent_connection::authentication::environment_provider::RequestAuthentication,
    expected: &ExpectedArtifact,
    request: &slingshot_storage::artifact_store::InstallationRequest,
    store: &'store slingshot_storage::artifact_store::ArtifactStore,
    capacity: &slingshot_storage::persistent_capacity::PersistentCapacityAccount<'_>,
    now_unix_milliseconds: u64,
) -> Result<
    (
        slingshot_storage::artifact_store::StagedArtifact<'store>,
        slingshot_storage::persistent_capacity::ArtifactPublication,
    ),
    RemoteStageRefusal,
> {
    stage_remote_artifact_started(
        transport,
        identity,
        submission,
        super::author_authentication::AuthorAuthentication::Fixed {
            authentication,
            protocol: super::subscription_reset::ResetTransport::Http1,
        },
        expected,
        request,
        store,
        capacity,
        now_unix_milliseconds,
        || Ok(()),
    )
    .await
}

/// Stages a remote artifact over an explicitly selected HTTP mode, without fallback.
/// All capacity, digest, typed-document and private-publication gates are shared.
pub async fn stage_remote_artifact_over<'store>(
    transport: &slingshot_agent_connection::selected_author_transport::SelectedAuthorTransport,
    identity: &slingshot_domain::operation_executor::ExecutionIdentity,
    submission: &slingshot_agent_connection::command_submission::Submission,
    authentication: &slingshot_agent_connection::authentication::environment_provider::RequestAuthentication,
    expected: &ExpectedArtifact,
    request: &slingshot_storage::artifact_store::InstallationRequest,
    store: &'store slingshot_storage::artifact_store::ArtifactStore,
    capacity: &slingshot_storage::persistent_capacity::PersistentCapacityAccount<'_>,
    now: u64,
    protocol: super::subscription_reset::ResetTransport,
) -> Result<
    (
        slingshot_storage::artifact_store::StagedArtifact<'store>,
        slingshot_storage::persistent_capacity::ArtifactPublication,
    ),
    RemoteStageRefusal,
> {
    stage_remote_artifact_with_authentication(
        transport,
        identity,
        submission,
        super::author_authentication::AuthorAuthentication::Fixed { authentication, protocol },
        expected,
        request,
        store,
        capacity,
        now,
    )
    .await
}

/// Stages through the runtime authentication policy without bypassing capacity,
/// private publication or artifact verification. Refresh never retries a partial transfer.
pub async fn stage_remote_artifact_with_authentication<'store>(
    transport: &slingshot_agent_connection::selected_author_transport::SelectedAuthorTransport,
    identity: &slingshot_domain::operation_executor::ExecutionIdentity,
    submission: &slingshot_agent_connection::command_submission::Submission,
    authentication: super::author_authentication::AuthorAuthentication<'_>,
    expected: &ExpectedArtifact,
    request: &slingshot_storage::artifact_store::InstallationRequest,
    store: &'store slingshot_storage::artifact_store::ArtifactStore,
    capacity: &slingshot_storage::persistent_capacity::PersistentCapacityAccount<'_>,
    now: u64,
) -> Result<
    (
        slingshot_storage::artifact_store::StagedArtifact<'store>,
        slingshot_storage::persistent_capacity::ArtifactPublication,
    ),
    RemoteStageRefusal,
> {
    stage_remote_artifact_started(
        transport,
        identity,
        submission,
        authentication,
        expected,
        request,
        store,
        capacity,
        now,
        || Ok(()),
    )
    .await
}

async fn stage_remote_artifact_started<'store>(
    transport: &slingshot_agent_connection::selected_author_transport::SelectedAuthorTransport,
    identity: &slingshot_domain::operation_executor::ExecutionIdentity,
    submission: &slingshot_agent_connection::command_submission::Submission,
    authentication: super::author_authentication::AuthorAuthentication<'_>,
    expected: &ExpectedArtifact,
    request: &slingshot_storage::artifact_store::InstallationRequest,
    store: &'store slingshot_storage::artifact_store::ArtifactStore,
    capacity: &slingshot_storage::persistent_capacity::PersistentCapacityAccount<'_>,
    now_unix_milliseconds: u64,
    before_request: impl FnOnce() -> Result<(), RemoteStageRefusal>,
) -> Result<
    (
        slingshot_storage::artifact_store::StagedArtifact<'store>,
        slingshot_storage::persistent_capacity::ArtifactPublication,
    ),
    RemoteStageRefusal,
> {
    use slingshot_agent_connection::selected_author_http::FiniteHttpFailure;
    use slingshot_storage::persistent_capacity::AccountingFailure;
    transport
        .require_submission(identity, submission)
        .map_err(|_| RemoteStageRefusal::Verification)?;
    if request.author_target_identity_digest != identity.author_target_identity_digest
        || request.operation_identifier != identity.operation_identifier
        || request.artifact_slot != expected.artifact_slot
        || request.media_type != expected.media_type
        || slingshot_agent_connection::artifact_download::require_remote_slot(
            &expected.artifact_slot,
        )
        .map_err(|_| RemoteStageRefusal::Verification)?
            != expected.media_type
    {
        return Err(RemoteStageRefusal::Verification);
    }
    if expected.artifact_slot == slingshot_domain::command::artifact::LOADED_CONTENT_SLOT
        && expected.byte_length > slingshot_domain::command::load_content_as_javascript_object_notation::maximum_load_document_bytes()
    {
        return Err(RemoteStageRefusal::Verification);
    }
    let reservation = capacity
        .reserve_artifact(Some(&expected.artifact_digest), expected.byte_length)
        .map_err(|error| match error {
            AccountingFailure::Refused(_) => RemoteStageRefusal::Capacity,
            _ => RemoteStageRefusal::Verification,
        })?;
    let mut writer = store
        .begin_verified(request, expected.byte_length, &expected.artifact_digest)
        .map_err(|_| RemoteStageRefusal::Verification)?;
    before_request()?;
    let artifact_identifier = slingshot_storage::artifact_store::ArtifactIdentifier::derive(
        &request.installation_identifier,
        &request.author_target_identity_digest,
        &request.operation_identifier,
        &request.artifact_slot,
    );
    let consume = |bytes: &[u8]| writer.write_chunk(bytes).map_err(|_| FiniteHttpFailure::Body);
    let outcome = authentication
        .artifact(transport, identity, submission, expected, artifact_identifier.as_text(), consume)
        .await
        .map_err(|_| RemoteStageRefusal::Verification)?;
    let receipt = match outcome {
        slingshot_agent_connection::selected_author_http::ArtifactHttpOutcome::Unauthorized => {
            return Err(RemoteStageRefusal::Verification);
        }
        slingshot_agent_connection::selected_author_http::ArtifactHttpOutcome::Transferred(
            receipt,
        ) => receipt,
        slingshot_agent_connection::selected_author_http::ArtifactHttpOutcome::Unavailable {
            evidence,
            elapsed_milliseconds,
        } => return Err(RemoteStageRefusal::Unavailable { evidence, elapsed_milliseconds }),
    };
    if receipt.byte_length() != expected.byte_length {
        return Err(RemoteStageRefusal::Verification);
    }
    let stage = writer.finish().map_err(|_| RemoteStageRefusal::Verification)?;
    if expected.artifact_slot == slingshot_domain::command::artifact::LOADED_CONTENT_SLOT {
        use slingshot_domain::command::canonical_json_reader::{Bounds, require_canonical_reader};
        let maximum = slingshot_domain::command::load_content_as_javascript_object_notation::maximum_load_document_bytes();
        let mut reader = stage.open_verified().map_err(|_| RemoteStageRefusal::Verification)?;
        require_canonical_reader(
            &mut reader,
            Bounds {
                bytes: expected.byte_length,
                token_bytes: usize::try_from(maximum)
                    .map_err(|_| RemoteStageRefusal::Verification)?,
                depth: STRUCTURED_RESULT_NESTING_DEPTH,
            },
        )
        .map_err(|_| RemoteStageRefusal::Verification)?;
        reader.finish().map_err(|_| RemoteStageRefusal::Verification)?;
        if submission.provenance.command_contract.command_wire_name != "load_content_as_json" {
            return Err(RemoteStageRefusal::Verification);
        }
        let command = serde_json::from_str(&submission.canonical_arguments)
            .map_err(|_| RemoteStageRefusal::Verification)?;
        let mut reader = stage.open_verified().map_err(|_| RemoteStageRefusal::Verification)?;
        slingshot_domain::command::loaded_document_reader::require_loaded_document_reader(
            &mut reader,
            &command,
        )
        .map_err(|_| RemoteStageRefusal::Verification)?;
        reader.finish().map_err(|_| RemoteStageRefusal::Verification)?;
    }
    let publication = match capacity
        .recover_publication(stage.metadata())
        .map_err(|_| RemoteStageRefusal::Verification)?
    {
        Some(publication) => {
            drop(reservation);
            publication
        }
        None => capacity
            .retain_staged_publication(
                &stage,
                reservation,
                now_unix_milliseconds.saturating_add(receipt.elapsed_milliseconds()),
            )
            .map_err(|_| RemoteStageRefusal::Verification)?,
    };
    Ok((stage, publication))
}

/// Completes the inline/no-artifact branch after persisted authoritative remote
/// success. Returns `None` for a validated result needing artifact completion
/// or local externalization, without publishing or discarding its recovery.
///
/// # Errors
///
/// Refuses missing success evidence, drifted retained facts, invalid result
/// bytes or a publication race. No retry or command resubmission is performed.
pub fn publish_retained_inline_result(
    operations: &slingshot_storage::operation_repository::OperationRepository,
    retained: &slingshot_storage::agent_job_repository::AgentSubmission,
    expected_revision: u64,
    identity: &slingshot_domain::operation_executor::ExecutionIdentity,
    submission: &slingshot_agent_connection::command_submission::Submission,
    body: &[u8],
    now_unix_milliseconds: u64,
) -> Result<
    Option<slingshot_storage::operation_repository::OperationSummary>,
    slingshot_agent_connection::structured_job_result::TerminalResultDecodeRefusal,
> {
    publish_inline(
        operations,
        retained,
        expected_revision,
        identity,
        submission,
        body,
        now_unix_milliseconds,
        None,
    )
}

/// Publishes inline completion and its validated successful snapshot atomically.
pub fn publish_retained_inline_snapshot(
    operations: &slingshot_storage::operation_repository::OperationRepository,
    retained: &slingshot_storage::agent_job_repository::AgentSubmission,
    expected_revision: u64,
    identity: &slingshot_domain::operation_executor::ExecutionIdentity,
    submission: &slingshot_agent_connection::command_submission::Submission,
    body: &[u8],
    now_unix_milliseconds: u64,
    snapshot: &slingshot_storage::agent_job_repository::SuccessfulAgentSnapshot,
) -> Result<
    Option<slingshot_storage::operation_repository::OperationSummary>,
    slingshot_agent_connection::structured_job_result::TerminalResultDecodeRefusal,
> {
    publish_inline(
        operations,
        retained,
        expected_revision,
        identity,
        submission,
        body,
        now_unix_milliseconds,
        Some(snapshot),
    )
}

/// Completes an accepted inline logical result, externalizing its exact bytes
/// when the local envelope cannot carry them. Remote artifact transfer remains
/// pending. Resources must belong to the selected runtime namespace.
pub fn publish_retained_snapshot_result(
    operations: &slingshot_storage::operation_repository::OperationRepository,
    retained: &slingshot_storage::agent_job_repository::AgentSubmission,
    expected_revision: u64,
    identity: &slingshot_domain::operation_executor::ExecutionIdentity,
    submission: &slingshot_agent_connection::command_submission::Submission,
    body: &[u8],
    now_unix_milliseconds: u64,
    snapshot: &slingshot_storage::agent_job_repository::SuccessfulAgentSnapshot,
    store: &slingshot_storage::artifact_store::ArtifactStore,
    capacity: &slingshot_storage::persistent_capacity::PersistentCapacityAccount<'_>,
) -> Result<
    Option<slingshot_storage::operation_repository::OperationSummary>,
    slingshot_agent_connection::structured_job_result::TerminalResultDecodeRefusal,
> {
    use sha2::Digest as _;
    let started = std::time::Instant::now();
    use slingshot_agent_connection::structured_job_result::{
        STRUCTURED_RESULT_MEDIA_TYPE, STRUCTURED_RESULT_SLOT, TerminalResultDecodeRefusal,
    };
    use slingshot_domain::operation::{
        OperationFact, ProducedArtifact, RecoveryCategory, RecoveryExecutionEvidence, RecoveryFact,
        SuccessfulSettlement,
    };
    if let Some(completed) = publish_retained_inline_snapshot(
        operations,
        retained,
        expected_revision,
        identity,
        submission,
        body,
        now_unix_milliseconds,
        snapshot,
    )? {
        return Ok(Some(completed));
    }
    let result = decode_retained_result(operations, expected_revision, identity, submission, body)?;
    if result.remote_artifact.is_some() {
        return Ok(None);
    }
    let held = super::durable_author_lookup::retained_command(operations, identity, submission)
        .map_err(|_| TerminalResultDecodeRefusal)?;
    let bytes = result.canonical_result.as_bytes();
    let digest = hex::encode(sha2::Sha256::digest(bytes));
    let reservation = match capacity.reserve_artifact(Some(&digest), bytes.len() as u64) {
        Ok(reservation) => reservation,
        Err(slingshot_storage::persistent_capacity::AccountingFailure::Refused(_)) => {
            operations
                .apply_for_retained_agent(
                    retained,
                    expected_revision,
                    &OperationFact::Recovery {
                        recovery: RecoveryFact {
                            category: RecoveryCategory::PersistentCapacityUnavailable,
                            evidence: RecoveryExecutionEvidence::AuthoritativeRemoteSuccess,
                            attempt_count: 0,
                            detail: "result externalization requires persistent capacity"
                                .to_owned(),
                            manual_resume_eligible: true,
                            retry_delay_milliseconds: 0,
                            retry_observed_at_unix_milliseconds: now_unix_milliseconds,
                        },
                    },
                    now_unix_milliseconds,
                )
                .map_err(|_| TerminalResultDecodeRefusal)?;
            return Ok(None);
        }
        Err(_) => return Err(TerminalResultDecodeRefusal),
    };
    let request = slingshot_storage::artifact_store::InstallationRequest {
        artifact_slot: STRUCTURED_RESULT_SLOT.to_owned(),
        author_target_identity_digest: identity.author_target_identity_digest.clone(),
        descriptor: None,
        installation_identifier: held.installation_identifier,
        media_type: STRUCTURED_RESULT_MEDIA_TYPE.to_owned(),
        operation_identifier: identity.operation_identifier.clone(),
    };
    let mut source = bytes;
    let stage = store
        .stage_verified(&request, &mut source, bytes.len() as u64, &digest)
        .map_err(|_| TerminalResultDecodeRefusal)?;
    // The hold survives any subsequent filesystem/SQL failure or process exit.
    let publication = match capacity
        .recover_publication(stage.metadata())
        .map_err(|_| TerminalResultDecodeRefusal)?
    {
        Some(publication) => {
            // A retry reuses the same charged content and producer hold. The
            // validated bytes are still re-staged and verified before publication.
            drop(reservation);
            publication
        }
        None => capacity
            .retain_staged_publication(&stage, reservation, now_unix_milliseconds)
            .map_err(|_| TerminalResultDecodeRefusal)?,
    };
    let artifact = stage.publish().map_err(|_| TerminalResultDecodeRefusal)?;
    let (snapshot, settled_at) =
        age_success_snapshot(snapshot, now_unix_milliseconds, started.elapsed());
    operations
        .settle_success_for_publications(
            retained,
            &SuccessfulSettlement {
                artifacts: vec![ProducedArtifact {
                    artifact_identifier: artifact.artifact_identifier.as_text().to_owned(),
                    artifact_slot: artifact.artifact_slot,
                    byte_length: artifact.byte_length,
                    content_digest: artifact.content_digest,
                    media_type: artifact.media_type,
                }],
                inline_result: None,
                expected_lifecycle_state: held.record.lifecycle_state,
                expected_revision,
                settled_at_unix_milliseconds: settled_at,
            },
            &snapshot,
            &[publication],
        )
        .map(Some)
        .map_err(|_| TerminalResultDecodeRefusal)
}

/// Completes a retained successful snapshot through the selected connection,
/// including remote artifacts and optional local result externalization.
/// All resources must belong to the selected runtime namespace.
pub async fn complete_retained_snapshot_result(
    operations: &slingshot_storage::operation_repository::OperationRepository,
    retained: &slingshot_storage::agent_job_repository::AgentSubmission,
    expected_revision: u64,
    identity: &slingshot_domain::operation_executor::ExecutionIdentity,
    submission: &slingshot_agent_connection::command_submission::Submission,
    body: &[u8],
    now: u64,
    snapshot: &slingshot_storage::agent_job_repository::SuccessfulAgentSnapshot,
    store: &slingshot_storage::artifact_store::ArtifactStore,
    capacity: &slingshot_storage::persistent_capacity::PersistentCapacityAccount<'_>,
    transport: &slingshot_agent_connection::selected_author_transport::SelectedAuthorTransport,
    authentication: &slingshot_agent_connection::authentication::environment_provider::RequestAuthentication,
) -> Result<
    Option<slingshot_storage::operation_repository::OperationSummary>,
    slingshot_agent_connection::structured_job_result::TerminalResultDecodeRefusal,
> {
    complete_retained_snapshot_result_over(
        operations,
        retained,
        expected_revision,
        identity,
        submission,
        body,
        now,
        snapshot,
        store,
        capacity,
        transport,
        authentication,
        super::subscription_reset::ResetTransport::Http1,
    )
    .await
}

/// Completes a retained result using one explicit artifact transport mode.
/// Publication and settlement remain guarded by the same retained revision.
pub async fn complete_retained_snapshot_result_over(
    operations: &slingshot_storage::operation_repository::OperationRepository,
    retained: &slingshot_storage::agent_job_repository::AgentSubmission,
    expected_revision: u64,
    identity: &slingshot_domain::operation_executor::ExecutionIdentity,
    submission: &slingshot_agent_connection::command_submission::Submission,
    body: &[u8],
    now: u64,
    snapshot: &slingshot_storage::agent_job_repository::SuccessfulAgentSnapshot,
    store: &slingshot_storage::artifact_store::ArtifactStore,
    capacity: &slingshot_storage::persistent_capacity::PersistentCapacityAccount<'_>,
    transport: &slingshot_agent_connection::selected_author_transport::SelectedAuthorTransport,
    authentication: &slingshot_agent_connection::authentication::environment_provider::RequestAuthentication,
    protocol: super::subscription_reset::ResetTransport,
) -> Result<
    Option<slingshot_storage::operation_repository::OperationSummary>,
    slingshot_agent_connection::structured_job_result::TerminalResultDecodeRefusal,
> {
    complete_retained_snapshot_result_with_authentication(
        operations,
        retained,
        expected_revision,
        identity,
        submission,
        body,
        now,
        snapshot,
        store,
        capacity,
        transport,
        super::author_authentication::AuthorAuthentication::Fixed { authentication, protocol },
    )
    .await
}

/// Completes one retained result through the same invocation authentication
/// policy as its snapshot. Capacity, acquisition anchors and settlement remain
/// guarded by the original retained revision across credential refresh.
pub async fn complete_retained_snapshot_result_with_authentication(
    operations: &slingshot_storage::operation_repository::OperationRepository,
    retained: &slingshot_storage::agent_job_repository::AgentSubmission,
    expected_revision: u64,
    identity: &slingshot_domain::operation_executor::ExecutionIdentity,
    submission: &slingshot_agent_connection::command_submission::Submission,
    body: &[u8],
    now: u64,
    snapshot: &slingshot_storage::agent_job_repository::SuccessfulAgentSnapshot,
    store: &slingshot_storage::artifact_store::ArtifactStore,
    capacity: &slingshot_storage::persistent_capacity::PersistentCapacityAccount<'_>,
    transport: &slingshot_agent_connection::selected_author_transport::SelectedAuthorTransport,
    authentication: super::author_authentication::AuthorAuthentication<'_>,
) -> Result<
    Option<slingshot_storage::operation_repository::OperationSummary>,
    slingshot_agent_connection::structured_job_result::TerminalResultDecodeRefusal,
> {
    use sha2::Digest as _;
    let started = std::time::Instant::now();
    use slingshot_agent_connection::structured_job_result::{
        LocalDisposition, STRUCTURED_RESULT_MEDIA_TYPE, STRUCTURED_RESULT_SLOT,
        TerminalResultDecodeRefusal,
    };
    use slingshot_domain::operation::{
        OperationFact, ProducedArtifact, RecoveryCategory, RecoveryExecutionEvidence, RecoveryFact,
        SuccessfulSettlement,
    };
    transport.require_submission(identity, submission).map_err(|_| TerminalResultDecodeRefusal)?;
    let result = decode_retained_result(operations, expected_revision, identity, submission, body)?;
    let Some(descriptor) = &result.remote_artifact else {
        let (snapshot, now) = age_success_snapshot(snapshot, now, started.elapsed());
        return publish_retained_snapshot_result(
            operations,
            retained,
            expected_revision,
            identity,
            submission,
            body,
            now,
            &snapshot,
            store,
            capacity,
        );
    };
    // This shared gate checks the exact retained child, revision and persisted
    // authoritative success before any artifact capacity or network operation.
    if let Some(completed) = publish_retained_inline_snapshot(
        operations,
        retained,
        expected_revision,
        identity,
        submission,
        body,
        now,
        snapshot,
    )? {
        return Ok(Some(completed));
    }
    let held = super::durable_author_lookup::retained_command(operations, identity, submission)
        .map_err(|_| TerminalResultDecodeRefusal)?;
    let pause = || -> Result<Option<slingshot_storage::operation_repository::OperationSummary>, TerminalResultDecodeRefusal> {
        operations.apply_for_retained_agent(retained, expected_revision, &OperationFact::Recovery {
            recovery: RecoveryFact {
                category: RecoveryCategory::PersistentCapacityUnavailable,
                evidence: RecoveryExecutionEvidence::AuthoritativeRemoteSuccess,
                attempt_count: 0, detail: "artifact completion requires persistent capacity".to_owned(),
                manual_resume_eligible: true, retry_delay_milliseconds: 0,
                retry_observed_at_unix_milliseconds: now,
            },
        }, now).map_err(|_| TerminalResultDecodeRefusal)?;
        Ok(None)
    };
    let expected = ExpectedArtifact {
        artifact_digest: descriptor.digest.as_text().to_owned(),
        artifact_slot: descriptor.slot.as_text().to_owned(),
        byte_length: descriptor.byte_length,
        media_type: descriptor.media_type.as_text().to_owned(),
    };
    let request = slingshot_storage::artifact_store::InstallationRequest {
        artifact_slot: expected.artifact_slot.clone(),
        author_target_identity_digest: identity.author_target_identity_digest.clone(),
        descriptor: None,
        installation_identifier: held.installation_identifier.clone(),
        media_type: expected.media_type.clone(),
        operation_identifier: identity.operation_identifier.clone(),
    };
    // Reserve optional local result space before fetching remote content, too.
    let local_bytes = result.canonical_result.as_bytes();
    let local_digest = hex::encode(sha2::Sha256::digest(local_bytes));
    let external = result.disposition != LocalDisposition::Inline;
    let local_reservation = if external {
        match capacity.reserve_artifact(Some(&local_digest), local_bytes.len() as u64) {
            Ok(reservation) => reservation,
            Err(slingshot_storage::persistent_capacity::AccountingFailure::Refused(_)) => {
                return pause();
            }
            Err(_) => return Err(TerminalResultDecodeRefusal),
        }
    } else {
        None
    };
    let acquisition_started = std::cell::Cell::new(None);
    let (stage, remote_publication) = match stage_remote_artifact_started(
        transport,
        identity,
        submission,
        authentication,
        &expected,
        &request,
        store,
        capacity,
        now,
        || {
            let elapsed =
                u64::try_from(started.elapsed().as_nanos().div_ceil(1_000_000)).unwrap_or(u64::MAX);
            let anchor = operations
                .begin_artifact_acquisition(
                    retained,
                    expected_revision,
                    descriptor.identifier.as_text(),
                    descriptor.slot.as_text(),
                    descriptor.digest.as_text(),
                    now.saturating_add(elapsed),
                )
                .map_err(|_| RemoteStageRefusal::Verification)?;
            acquisition_started.set(Some(anchor));
            Ok(())
        },
    )
    .await
    {
        Ok(staged) => staged,
        Err(RemoteStageRefusal::Capacity) => return pause(),
        Err(RemoteStageRefusal::Verification) => return Err(TerminalResultDecodeRefusal),
        Err(RemoteStageRefusal::Unavailable { evidence, .. }) => {
            let elapsed =
                u64::try_from(started.elapsed().as_nanos().div_ceil(1_000_000)).unwrap_or(u64::MAX);
            let observed_at = now.saturating_add(elapsed);
            let grace_end =
                acquisition_started.get().ok_or(TerminalResultDecodeRefusal)?.saturating_add(
                    slingshot_agent_connection::artifact_download::missing_grace_milliseconds(),
                );
            if evidence.reason() == slingshot_agent_protocol::artifact_unavailable::UnavailableReason::RetentionExpired || observed_at >= grace_end {
                use slingshot_domain::operation::{TerminalFailure, TerminalFailureDisposition, TerminalFailureKind};
                return operations.apply_for_retained_agent(retained, expected_revision, &OperationFact::Terminal {
                    failure: TerminalFailure {
                        kind: TerminalFailureKind::ResultUnavailable,
                        disposition: TerminalFailureDisposition::AuthoritativeRemoteSuccess,
                        metadata: None,
                    },
                }, observed_at).map(Some).map_err(|_| TerminalResultDecodeRefusal);
            }
            use rand::RngExt;
            let attempt = held
                .record
                .outstanding_recovery
                .as_ref()
                .map_or(1, |fact| fact.attempt_count.saturating_add(1));
            let paused =
                u64::from(attempt) >= super::recovery_and_event_supervisor::automatic_attempt_cap();
            let ceiling = super::recovery_and_event_supervisor::jitter_ceiling_milliseconds(
                u64::from(attempt),
            )
            .min(grace_end.saturating_sub(observed_at));
            return operations
                .apply_for_retained_agent(
                    retained,
                    expected_revision,
                    &OperationFact::Recovery {
                        recovery: RecoveryFact {
                            category: RecoveryCategory::ResultAcquisition,
                            evidence: RecoveryExecutionEvidence::AuthoritativeRemoteSuccess,
                            attempt_count: attempt,
                            detail: "waiting for the saved missing-artifact grace interval"
                                .to_owned(),
                            manual_resume_eligible: paused,
                            retry_delay_milliseconds: if paused {
                                0
                            } else {
                                rand::rng().random_range(0..=ceiling)
                            },
                            retry_observed_at_unix_milliseconds: observed_at,
                        },
                    },
                    observed_at,
                )
                .map(Some)
                .map_err(|_| TerminalResultDecodeRefusal);
        }
    };
    let mut stages = vec![stage];
    let mut publications = vec![remote_publication];
    if external {
        let request = slingshot_storage::artifact_store::InstallationRequest {
            artifact_slot: STRUCTURED_RESULT_SLOT.to_owned(),
            author_target_identity_digest: identity.author_target_identity_digest.clone(),
            descriptor: None,
            installation_identifier: held.installation_identifier,
            media_type: STRUCTURED_RESULT_MEDIA_TYPE.to_owned(),
            operation_identifier: identity.operation_identifier.clone(),
        };
        let stage = store
            .stage_verified(
                &request,
                &mut &local_bytes[..],
                local_bytes.len() as u64,
                &local_digest,
            )
            .map_err(|_| TerminalResultDecodeRefusal)?;
        let publication = match capacity
            .recover_publication(stage.metadata())
            .map_err(|_| TerminalResultDecodeRefusal)?
        {
            Some(publication) => {
                drop(local_reservation);
                publication
            }
            None => capacity
                .retain_staged_publication(&stage, local_reservation, now)
                .map_err(|_| TerminalResultDecodeRefusal)?,
        };
        stages.push(stage);
        publications.push(publication);
    }
    let mut artifacts = Vec::new();
    for stage in stages {
        let artifact = stage.publish().map_err(|_| TerminalResultDecodeRefusal)?;
        artifacts.push(ProducedArtifact {
            artifact_identifier: artifact.artifact_identifier.as_text().to_owned(),
            artifact_slot: artifact.artifact_slot,
            byte_length: artifact.byte_length,
            content_digest: artifact.content_digest,
            media_type: artifact.media_type,
        });
    }
    let (snapshot, settled_at) = age_success_snapshot(snapshot, now, started.elapsed());
    operations
        .settle_success_for_publications(
            retained,
            &SuccessfulSettlement {
                artifacts,
                inline_result: if external { None } else { Some(result.canonical_result) },
                expected_lifecycle_state: held.record.lifecycle_state,
                expected_revision,
                settled_at_unix_milliseconds: settled_at,
            },
            &snapshot,
            &publications,
        )
        .map(Some)
        .map_err(|_| TerminalResultDecodeRefusal)
}

fn publish_inline(
    operations: &slingshot_storage::operation_repository::OperationRepository,
    retained: &slingshot_storage::agent_job_repository::AgentSubmission,
    expected_revision: u64,
    identity: &slingshot_domain::operation_executor::ExecutionIdentity,
    submission: &slingshot_agent_connection::command_submission::Submission,
    body: &[u8],
    now_unix_milliseconds: u64,
    snapshot: Option<&slingshot_storage::agent_job_repository::SuccessfulAgentSnapshot>,
) -> Result<
    Option<slingshot_storage::operation_repository::OperationSummary>,
    slingshot_agent_connection::structured_job_result::TerminalResultDecodeRefusal,
> {
    let started = std::time::Instant::now();
    use slingshot_agent_connection::structured_job_result::{
        LocalDisposition, TerminalResultDecodeRefusal,
    };
    use slingshot_domain::operation::{RecoveryExecutionEvidence, SuccessfulSettlement};
    let held = super::durable_author_lookup::retained_command(operations, identity, submission)
        .map_err(|_| TerminalResultDecodeRefusal)?;
    if held.record.revision != expected_revision
        || held.record.lifecycle_state.is_terminal()
        || held
            .record
            .outstanding_recovery
            .as_ref()
            .is_some_and(super::durable_author_lookup::automatic_recovery_paused)
        || !held.record.outstanding_recovery.as_ref().is_some_and(|recovery| {
            recovery.evidence == RecoveryExecutionEvidence::AuthoritativeRemoteSuccess
        })
    {
        return Err(TerminalResultDecodeRefusal);
    }
    let contract = &submission.provenance.command_contract;
    let stored = &retained.contracts;
    if retained.identity.author_target_identity_digest != identity.author_target_identity_digest
        || retained.identity.operation_identifier != identity.operation_identifier
        || retained.identity.selected_environment_revision != identity.selected_environment_revision
        || retained.identity.agent_operation_identifier
            != submission.operation.agent_operation_identifier
        || retained.identity.agent_event_store_generation
            != submission.operation.agent_event_store_generation
        || retained.identity.daemon_subscription_identifier
            != submission.daemon_subscription_identifier
        || retained.canonical_submission.as_bytes()
            != submission.wire_body().map_err(|_| TerminalResultDecodeRefusal)?
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
        return Err(TerminalResultDecodeRefusal);
    }
    let result = decode_retained_result(operations, expected_revision, identity, submission, body)?;
    if result.remote_artifact.is_some() || result.disposition != LocalDisposition::Inline {
        return Ok(None);
    }
    let aged = snapshot
        .map(|snapshot| age_success_snapshot(snapshot, now_unix_milliseconds, started.elapsed()));
    let settlement = SuccessfulSettlement {
        artifacts: Vec::new(),
        inline_result: Some(result.canonical_result),
        expected_lifecycle_state: held.record.lifecycle_state,
        expected_revision,
        settled_at_unix_milliseconds: aged.as_ref().map_or(now_unix_milliseconds, |(_, now)| *now),
    };
    match aged.as_ref() {
        Some((snapshot, _)) => {
            operations.settle_success_for_agent_snapshot(retained, &settlement, snapshot)
        }
        None => operations.settle_success_for_retained_agent(retained, &settlement),
    }
    .map(Some)
    .map_err(|_| TerminalResultDecodeRefusal)
}

/// Decodes using the retained local command and installation, with recomputed
/// submission provenance and digest. Refuses a stale or terminal local owner.
/// This read-only gate is not a settlement lease: the remote child and local
/// revision must still be compared atomically when publishing a result.
///
/// # Errors
///
/// Returns an opaque refusal without changing the operation or any artifacts.
pub fn decode_retained_result(
    operations: &slingshot_storage::operation_repository::OperationRepository,
    expected_revision: u64,
    identity: &slingshot_domain::operation_executor::ExecutionIdentity,
    submission: &slingshot_agent_connection::command_submission::Submission,
    body: &[u8],
) -> Result<
    slingshot_agent_connection::structured_job_result::ValidatedResult,
    slingshot_agent_connection::structured_job_result::TerminalResultDecodeRefusal,
> {
    use slingshot_agent_connection::structured_job_result::{
        ResultExpectation, TerminalResultDecodeRefusal,
    };
    use slingshot_agent_protocol::wire_contract::ExpectedProvenance;
    use slingshot_domain::command::canonical_json::require_canonical_bytes;
    slingshot_agent_connection::selected_author_submission::require_submission_derivation(
        identity, submission,
    )
    .map_err(|_| TerminalResultDecodeRefusal)?;
    let held = super::durable_author_lookup::retained_command(operations, identity, submission)
        .map_err(|_| TerminalResultDecodeRefusal)?;
    if held.record.revision != expected_revision
        || held.record.lifecycle_state.is_terminal()
        || held.selected_environment_revision != identity.selected_environment_revision
    {
        return Err(TerminalResultDecodeRefusal);
    }
    let mut arguments = require_canonical_bytes(submission.canonical_arguments.as_bytes())
        .map_err(|_| TerminalResultDecodeRefusal)?;
    let object = arguments.as_object_mut().ok_or(TerminalResultDecodeRefusal)?;
    if object.insert("command".to_owned(), held.command_wire_name.clone().into()).is_some() {
        return Err(TerminalResultDecodeRefusal);
    }
    let command = serde_json::from_value(arguments).map_err(|_| TerminalResultDecodeRefusal)?;
    let expectation = ResultExpectation {
        operation: submission.operation.clone(),
        daemon_subscription_identifier: submission.daemon_subscription_identifier.clone(),
        expected_provenance: ExpectedProvenance {
            canonical_json_contract_digest: submission.provenance.canonical_json_contract_digest.clone(),
            transport_contract_digest: submission.provenance.transport_contract_digest.clone(),
            command_contract: slingshot_domain::selected_command_contract_identity::SelectedCommandContractIdentity::installed(&held.command_wire_name)
                .map_err(|_| TerminalResultDecodeRefusal)?,
        },
        submitted_command_digest: submission.submitted_command_digest.clone(),
        wire_name: held.command_wire_name,
    };
    decode_bound_result(body, &expectation, &command, &held.installation_identifier, identity)
}

/// Validates actual result bytes and binds any remote artifact to the same
/// deterministic identity used by the local artifact store. `command` and the
/// expected submission digest must come from the retained operation, not the
/// remote response. This function performs no transfer or state mutation.
///
/// # Errors
///
/// Refuses malformed results and artifacts belonging to another installation,
/// target, operation or slot. Diagnostics never contain remote payloads.
pub fn decode_bound_result(
    body: &[u8],
    expectation: &slingshot_agent_connection::structured_job_result::ResultExpectation,
    command: &slingshot_domain::command::catalog::Command,
    installation: &slingshot_domain::installation::InstallationIdentifier,
    identity: &slingshot_domain::operation_executor::ExecutionIdentity,
) -> Result<
    slingshot_agent_connection::structured_job_result::ValidatedResult,
    slingshot_agent_connection::structured_job_result::TerminalResultDecodeRefusal,
> {
    use slingshot_agent_connection::structured_job_result::{
        TerminalResultDecodeRefusal, decode_result_for_command,
    };
    let expected_operation = slingshot_agent_protocol::identity::WireOperationIdentity::of(
        &identity.author_target_identity_digest,
        &identity.selected_environment_revision,
        &identity.operation_identifier,
        slingshot_domain::agent_identity::AgentEventStoreGeneration::of(
            expectation.operation.agent_event_store_generation,
        ),
    );
    if expectation.operation != expected_operation {
        return Err(TerminalResultDecodeRefusal);
    }
    let result = decode_result_for_command(body, expectation, command)?;
    if let Some(artifact) = &result.remote_artifact {
        let expected = slingshot_storage::artifact_store::ArtifactIdentifier::derive(
            installation,
            &identity.author_target_identity_digest,
            &identity.operation_identifier,
            artifact.slot.as_text(),
        );
        if artifact.identifier.as_text() != expected.as_text() {
            return Err(TerminalResultDecodeRefusal);
        }
    }
    Ok(result)
}

/// Where one artifact lives locally, once it is mapped.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ArtifactMapping {
    /// What the operation is called at the agent.
    pub agent_operation_identifier: String,
    /// Which partition it belongs to.
    pub author_target_identity_digest: String,
    /// What the local artifact is called.
    pub local_artifact_identifier: String,
    /// Which slot it fills.
    pub artifact_slot: String,
}

/// What one completion attempt produced.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CompletionOutcome {
    /// The artifact is published, atomically, under its mapping.
    Published(Box<ArtifactMapping>),
    /// There is no room, and the remote success is recorded instead.
    PersistentCapacityUnavailable {
        /// How long the agent will still hold it.
        remaining_retention_milliseconds: u64,
    },
    /// The attempt failed and the mapping is kept for the next one.
    Retryable {
        /// What went wrong.
        refusal: Box<DownloadRefusal>,
    },
    /// A second, different body arrived for an identifier already verified.
    IntegrityConflict,
}

impl CompletionOutcome {
    /// Returns whether anything was written where a reader could see it.
    #[must_use]
    pub fn published_anything(&self) -> bool {
        matches!(self, Self::Published(_))
    }

    /// Returns whether this attempt may be made again without asking a person.
    #[must_use]
    pub fn permits_automatic_retry(&self) -> bool {
        matches!(self, Self::Retryable { .. })
    }
}

/// Where an artifact is reserved, staged, and published.
pub trait ArtifactSink: ::core::fmt::Debug {
    /// Reserves exactly `bytes`, before any request is issued.
    fn reserve(&self, bytes: u64) -> bool;

    /// Releases a reservation that was never committed.
    fn release(&self, bytes: u64);

    /// Removes whatever a failed attempt left behind.
    fn discard_partial(&self, mapping: &ArtifactMapping);

    /// Publishes the staged bytes atomically, under `mapping`.
    fn publish(&self, mapping: &ArtifactMapping, digest: &str);

    /// Returns what was already published under `mapping`, when anything was.
    fn published_digest(&self, mapping: &ArtifactMapping) -> Option<String>;
}

/// Returns what one completion attempt produces.
///
/// The order is the design: the reservation, then the transfer, then the proof,
/// then the publication. Nothing later can rescue a step that was skipped, and
/// nothing earlier is allowed to write where a reader could see it.
#[must_use]
pub fn complete(
    sink: &dyn ArtifactSink,
    mapping: &ArtifactMapping,
    expected: &ExpectedArtifact,
    transfer: &ArtifactTransfer,
    proof: (TransferEnd, &str, u64),
) -> CompletionOutcome {
    let (end, observed_digest, remaining_retention_milliseconds) = proof;
    if let Some(published) = sink.published_digest(mapping) {
        return if published == observed_digest {
            CompletionOutcome::Published(Box::new(mapping.clone()))
        } else {
            CompletionOutcome::IntegrityConflict
        };
    }
    if !sink.reserve(expected.byte_length) {
        return CompletionOutcome::PersistentCapacityUnavailable {
            remaining_retention_milliseconds,
        };
    }
    if let Err(refusal) = transfer.require_publishable(end, observed_digest) {
        sink.discard_partial(mapping);
        sink.release(expected.byte_length);
        return CompletionOutcome::Retryable { refusal: Box::new(refusal) };
    }
    sink.publish(mapping, observed_digest);
    CompletionOutcome::Published(Box::new(mapping.clone()))
}
