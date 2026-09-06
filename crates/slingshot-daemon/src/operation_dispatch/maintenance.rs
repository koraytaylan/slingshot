//! Operation-free maintenance reads through the validated target boundary.

use slingshot_domain::daemon_runtime_contract::{
    MaintenanceResultIdentifier, MaintenanceResultKind,
};
use slingshot_local_protocol::message::{
    MaintenanceResultDescription, OperationRequest, OperationResponse,
};
use slingshot_storage::database::OperationDatabase;
use slingshot_storage::maintenance_results::{self, RetentionOwner};

use super::{BoundRequest, malformed};

/// A bounded operation-free transfer, ending successfully only after verification.
#[derive(Debug)]
pub struct MaintenanceResponseStream {
    start: Option<OperationResponse>,
    transfer: Option<crate::artifact_transfer::ArtifactTransfer>,
    offset: u64,
}

impl Iterator for MaintenanceResponseStream {
    type Item = OperationResponse;
    fn next(&mut self) -> Option<Self::Item> {
        use base64::Engine as _;
        if let Some(start) = self.start.take() {
            return Some(start);
        }
        match self.transfer.as_mut()?.next_chunk() {
            Ok(Some(bytes)) => {
                let starting_byte_offset = self.offset;
                self.offset += bytes.len() as u64;
                Some(OperationResponse::MaintenanceResultChunk {
                    body: slingshot_local_protocol::message::ChunkBody {
                        encoded_bytes: base64::engine::general_purpose::STANDARD.encode(bytes),
                        starting_byte_offset,
                    },
                })
            }
            Ok(None) => Some(match self.transfer.take()?.finish() {
                Ok(()) => OperationResponse::MaintenanceResultEnd,
                Err(_) => read_failure(),
            }),
            Err(_) => {
                self.transfer = None;
                Some(read_failure())
            }
        }
    }
}

fn read_failure() -> OperationResponse {
    OperationResponse::InternalFailure {
        detail: "the retained maintenance result could not be verified or read".to_owned(),
    }
}

impl BoundRequest {
    /// Opens a verified maintenance document without an operation/artifact address.
    ///
    /// # Errors
    /// Returns a bounded validation, missing-result, or read refusal. `Ok(None)`
    /// belongs to another handler. Dropping the stream changes no retained state.
    pub fn maintenance_read(
        &self,
        database: &OperationDatabase,
        store: &slingshot_storage::artifact_store::ArtifactStore,
    ) -> Result<Option<MaintenanceResponseStream>, OperationResponse> {
        let OperationRequest::MaintenanceResultRead {
            maintenance_result_identifier,
            expected_content_digest,
            preferred_chunk_bytes,
            starting_byte_offset,
            ..
        } = self.request()
        else {
            return Ok(None);
        };
        let identifier = MaintenanceResultIdentifier::parse(maintenance_result_identifier)
            .map_err(|_| malformed())?;
        if !slingshot_local_protocol::message::digest_is_canonical(expected_content_digest) {
            return Err(malformed());
        }
        let target = &self.envelope.author_target_identity_digest;
        let metadata = maintenance_results::read(database, target, &identifier)
            .map_err(|_| read_failure())?
            .ok_or_else(|| OperationResponse::MissingMaintenanceResult {
                maintenance_result_identifier: identifier.as_text().to_owned(),
            })?;
        if metadata.content_digest != *expected_content_digest
            || *starting_byte_offset > metadata.byte_length
        {
            return Err(malformed());
        }
        let reader = store.open_maintenance_result(&metadata).map_err(|_| read_failure())?;
        let transfer = crate::artifact_transfer::ArtifactTransfer::from_verified(
            reader,
            metadata.byte_length,
            *starting_byte_offset,
            u64::from(*preferred_chunk_bytes),
        )
        .map_err(|_| read_failure())?;
        let description = MaintenanceResultDescription {
            association_revision: metadata.association_revision,
            author_target_identity_digest: target.clone(),
            byte_length: metadata.byte_length,
            content_digest: metadata.content_digest,
            kind: match metadata.kind {
                MaintenanceResultKind::Preview => "preview",
                MaintenanceResultKind::Application => "application",
            }
            .to_owned(),
            maintenance_result_identifier: identifier.as_text().to_owned(),
            media_type: "application/json".to_owned(),
            retention_owner: match metadata.owner {
                RetentionOwner::CurrentPreview => "current_preview",
                RetentionOwner::ApplicationReceipt(_) => "application_receipt",
            }
            .to_owned(),
            reviewed_source_digest: metadata.reviewed_source_digest,
        };
        Ok(Some(MaintenanceResponseStream {
            start: Some(OperationResponse::MaintenanceResultStart { description }),
            transfer: Some(transfer),
            offset: *starting_byte_offset,
        }))
    }
    /// Describes a retained result without reading an operation or file bytes.
    /// `None` means another handler owns this request.
    #[must_use]
    pub fn maintenance_metadata(&self, database: &OperationDatabase) -> Option<OperationResponse> {
        let OperationRequest::MaintenanceResultMetadata { maintenance_result_identifier, .. } =
            self.request()
        else {
            return None;
        };
        let identifier = match MaintenanceResultIdentifier::parse(maintenance_result_identifier) {
            Ok(identifier) => identifier,
            Err(_) => return Some(malformed()),
        };
        let target = &self.envelope.author_target_identity_digest;
        Some(match maintenance_results::read(database, target, &identifier) {
            Ok(Some(metadata)) => OperationResponse::MaintenanceResultMetadata {
                description: MaintenanceResultDescription {
                    association_revision: metadata.association_revision,
                    author_target_identity_digest: target.clone(),
                    byte_length: metadata.byte_length,
                    content_digest: metadata.content_digest,
                    kind: match metadata.kind {
                        MaintenanceResultKind::Preview => "preview",
                        MaintenanceResultKind::Application => "application",
                    }
                    .to_owned(),
                    maintenance_result_identifier: metadata.identifier.as_text().to_owned(),
                    media_type: "application/json".to_owned(),
                    retention_owner: match metadata.owner {
                        RetentionOwner::CurrentPreview => "current_preview",
                        RetentionOwner::ApplicationReceipt(_) => "application_receipt",
                    }
                    .to_owned(),
                    reviewed_source_digest: metadata.reviewed_source_digest,
                },
            },
            Ok(None) => OperationResponse::MissingMaintenanceResult {
                maintenance_result_identifier: identifier.as_text().to_owned(),
            },
            Err(_) => OperationResponse::InternalFailure {
                detail: "the retained maintenance result could not be validated or read".to_owned(),
            },
        })
    }
}
