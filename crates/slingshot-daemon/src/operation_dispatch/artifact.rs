//! Target-qualified artifact resolution and bounded, terminally verified frames.

use base64::Engine as _;
use slingshot_domain::installation::InstallationIdentifier;
use slingshot_domain::operation::OperationLifecycleState;
use slingshot_local_protocol::message::{
    ChunkBody, OperationRequest, OperationResponse, digest_is_canonical,
};
use slingshot_storage::artifact_store::{ArtifactAssociations, ArtifactIdentifier, ArtifactStore};
use slingshot_storage::operation_repository::OperationRepository;

use super::{BoundRequest, internal_failure, malformed};
use crate::artifact_transfer::{ArtifactTransfer, TransferFailure, TransferRequest};

/// One verified handle and at most one decoded chunk are retained per reader.
/// Dropping the stream cancels observation only; it never alters the operation.
#[derive(Debug)]
pub struct ArtifactResponseStream {
    start: Option<OperationResponse>,
    transfer: Option<ArtifactTransfer>,
    offset: u64,
}

impl Iterator for ArtifactResponseStream {
    type Item = OperationResponse;

    fn next(&mut self) -> Option<Self::Item> {
        if let Some(start) = self.start.take() {
            return Some(start);
        }
        let transfer = self.transfer.as_mut()?;
        match transfer.next_chunk() {
            Ok(Some(bytes)) => {
                let starting_byte_offset = self.offset;
                self.offset += bytes.len() as u64;
                Some(OperationResponse::ArtifactChunk {
                    body: ChunkBody {
                        encoded_bytes: base64::engine::general_purpose::STANDARD.encode(bytes),
                        starting_byte_offset,
                    },
                })
            }
            Ok(None) => {
                let transfer = self.transfer.take()?;
                Some(match transfer.finish() {
                    Ok(()) => OperationResponse::ArtifactEnd,
                    Err(_) => read_failure(),
                })
            }
            Err(_) => {
                self.transfer = None;
                Some(read_failure())
            }
        }
    }
}

impl BoundRequest {
    /// Resolves and verifies a retained artifact before returning its stream.
    /// No command or remote-author request is issued by this read.
    ///
    /// # Errors
    /// Returns a bounded protocol refusal. `Ok(None)` belongs to another method.
    pub fn artifact(
        &self,
        repository: &OperationRepository,
        installation: &InstallationIdentifier,
        store: &ArtifactStore,
    ) -> Result<Option<ArtifactResponseStream>, OperationResponse> {
        let OperationRequest::ArtifactRead {
            artifact_identifier,
            expected_content_digest,
            operation_identifier,
            preferred_chunk_bytes,
            starting_byte_offset,
        } = self.request()
        else {
            return Ok(None);
        };
        if operation_identifier.is_empty()
            || operation_identifier.contains('\0')
            || !digest_is_canonical(expected_content_digest)
        {
            return Err(malformed());
        }
        let identifier = ArtifactIdentifier::parse(artifact_identifier).map_err(|_| malformed())?;
        let target = &self.envelope.author_target_identity_digest;
        let summary = repository
            .read(target, operation_identifier)
            .map_err(|_| internal_failure())?
            .ok_or_else(|| OperationResponse::MissingOperation {
                operation_identifier: operation_identifier.clone(),
            })?;
        if summary.record.lifecycle_state != OperationLifecycleState::Succeeded {
            return Err(OperationResponse::InvalidTransition {
                lifecycle_state: serde_json::to_value(summary.record.lifecycle_state)
                    .ok()
                    .and_then(|value| value.as_str().map(str::to_owned))
                    .ok_or_else(internal_failure)?,
                operation_identifier: operation_identifier.clone(),
            });
        }
        let associations = ArtifactAssociations::new(repository.database());
        let metadata = match associations
            .read_identifier(target, operation_identifier, &identifier)
            .map_err(|_| read_failure())?
        {
            Some(found) if found.artifact_identifier == identifier => found,
            Some(_) => return Err(read_failure()),
            None => {
                result_named(&associations, target, operation_identifier, expected_content_digest)?
            }
        };
        if !admissible(&metadata, installation, target, operation_identifier) {
            return Err(read_failure());
        }
        let (transfer, _) = ArtifactTransfer::open(
            store,
            &metadata,
            &TransferRequest {
                expected_content_digest: expected_content_digest.clone(),
                starting_offset: *starting_byte_offset,
                preferred_chunk_bytes: u64::from(*preferred_chunk_bytes),
            },
        )
        .map_err(|failure| match failure {
            TransferFailure::DigestMismatch { .. } | TransferFailure::OffsetPastEnd { .. } => {
                malformed()
            }
            TransferFailure::Artifact(_) => read_failure(),
        })?;
        Ok(Some(ArtifactResponseStream {
            // The start echoes the name the caller used, which is the one the
            // operation's result gave it, whichever of the two names it is.
            start: Some(OperationResponse::ArtifactStart {
                artifact_identifier: identifier.as_text().to_owned(),
                byte_length: metadata.byte_length,
                content_digest: metadata.content_digest,
                media_type: metadata.media_type,
            }),
            transfer: Some(transfer),
            offset: *starting_byte_offset,
        }))
    }
}

/// Returns whether one retained association is one this installation made for
/// this operation, with a slot, media type and length inside their bounds.
fn admissible(
    metadata: &slingshot_storage::artifact_store::ArtifactMetadata,
    installation: &InstallationIdentifier,
    target: &str,
    operation_identifier: &str,
) -> bool {
    ArtifactIdentifier::derive(installation, target, operation_identifier, &metadata.artifact_slot)
        == metadata.artifact_identifier
        && !metadata.artifact_slot.is_empty()
        && metadata.artifact_slot.len()
            <= slingshot_storage::artifact_store::maximum_artifact_slot_bytes()
        && !metadata.media_type.is_empty()
        && metadata.media_type.len()
            <= slingshot_storage::artifact_store::maximum_media_type_bytes()
        && metadata.byte_length
            <= slingshot_domain::daemon_runtime_contract::DaemonRuntimeContract::embedded()
                .formula("maximum_individual_artifact_bytes")
}

/// Resolves an artifact the operation's own result names by the agent's
/// identifier rather than this installation's.
///
/// A command whose result carries an artifact descriptor names the artifact as
/// the agent published it, and the daemon retains the fetched bytes in the slot
/// the descriptor fills under an identifier of its own. The name alone is
/// therefore not enough to find it; the declared slots of this one operation
/// are, and exactly one of them must hold content with the digest the caller
/// quoted - which the caller took from that same result. Nothing outside the
/// addressed operation and target is consulted.
fn result_named(
    associations: &ArtifactAssociations<'_>,
    target: &str,
    operation_identifier: &str,
    expected_content_digest: &str,
) -> Result<slingshot_storage::artifact_store::ArtifactMetadata, OperationResponse> {
    let mut held = Vec::new();
    for declaration in slingshot_domain::command::artifact::ArtifactSlotDeclaration::declared() {
        if let Some(found) = associations
            .read(target, operation_identifier, declaration.slot.as_text())
            .map_err(|_| read_failure())?
            && found.content_digest == expected_content_digest
        {
            held.push(found);
        }
    }
    match <[_; 1]>::try_from(held) {
        Ok([found]) => Ok(found),
        Err(_) => Err(read_failure()),
    }
}

fn read_failure() -> OperationResponse {
    OperationResponse::InternalFailure {
        detail: "the retained artifact could not be verified or read".to_owned(),
    }
}
