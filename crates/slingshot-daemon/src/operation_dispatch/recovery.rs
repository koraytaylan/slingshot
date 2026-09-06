//! Project durable recovery receipts and current operation facts onto the wire.

use sha2::{Digest as _, Sha256};
use slingshot_local_protocol::message::{OperationRequest, OperationResponse};
use slingshot_storage::operation_repository::{OperationRepository, RepositoryFailure};

use super::{BoundRequest, internal_failure, malformed};
use crate::operation_recovery::{
    self, ResumeFailure, ResumeRefusal, ResumeRequest, ResumeResponse,
};

impl BoundRequest {
    /// Applies or replays an exact recovery resume and reports the current state.
    /// This grants eligibility only; it never runs an executor or sends work.
    ///
    /// # Errors
    ///
    /// Returns the repository refusal for the caller's bounded wire mapping.
    /// `Ok(None)` means another handler owns this request.
    pub fn resume(
        &self,
        repository: &OperationRepository,
        now_unix_milliseconds: u64,
    ) -> Result<Option<OperationResponse>, RepositoryFailure> {
        let OperationRequest::ResumeOperationRecovery {
            expected_operation_revision,
            expected_recovery_category,
            operation_identifier,
        } = self.request()
        else {
            return Ok(None);
        };
        if operation_identifier.is_empty() || operation_identifier.contains('\0') {
            return Ok(Some(malformed()));
        }
        let Ok(category) =
            serde_json::from_value(serde_json::Value::String(expected_recovery_category.clone()))
        else {
            return Ok(Some(malformed()));
        };
        let target = &self.envelope.author_target_identity_digest;
        let outcome = operation_recovery::resume(
            repository,
            &ResumeRequest {
                author_target_identity_digest: target.clone(),
                expected_recovery_category: category,
                expected_revision: *expected_operation_revision,
                operation_identifier: operation_identifier.clone(),
                selected_environment_revision: self.envelope.selected_environment_revision.clone(),
            },
            now_unix_milliseconds,
        )
        .map_err(|ResumeFailure::Repository(failure)| failure)?;
        let Some(current) = repository.read(target, operation_identifier)? else {
            return Ok(Some(OperationResponse::MissingOperation {
                operation_identifier: operation_identifier.clone(),
            }));
        };
        let Ok(serde_json::Value::String(lifecycle_state)) =
            serde_json::to_value(current.record.lifecycle_state)
        else {
            return Ok(Some(internal_failure()));
        };
        let response = match outcome {
            ResumeResponse::Applied(ref receipt) | ResumeResponse::Replayed(ref receipt) => {
                // Length-delimited JSON avoids concatenation ambiguity. Only
                // immutable receipt-source bindings participate, not current
                // progress or timestamps observed while rendering a replay.
                let identity = serde_json::to_vec(&(
                    "slingshot.recovery-resume-receipt/1",
                    target,
                    &receipt.selected_environment_revision,
                    &receipt.source_fingerprint,
                ))
                .expect("receipt identity strings serialize");
                let identifier = hex::encode(Sha256::digest(identity));
                if matches!(outcome, ResumeResponse::Applied(_)) {
                    OperationResponse::RecoveryResumeApplied {
                        current_lifecycle_state: lifecycle_state,
                        operation_identifier: operation_identifier.clone(),
                        resume_receipt_identifier: identifier,
                    }
                } else {
                    OperationResponse::RecoveryResumeReplayed {
                        current_lifecycle_state: lifecycle_state,
                        operation_identifier: operation_identifier.clone(),
                        resume_receipt_identifier: identifier,
                    }
                }
            }
            ResumeResponse::Refused(ResumeRefusal::NoSuchOperation { .. }) => {
                OperationResponse::MissingOperation {
                    operation_identifier: operation_identifier.clone(),
                }
            }
            ResumeResponse::Refused(ResumeRefusal::RevisionMismatch) => {
                OperationResponse::RevisionMismatch {
                    selected_environment_revision: current.selected_environment_revision,
                }
            }
            ResumeResponse::Refused(_) => OperationResponse::InvalidTransition {
                lifecycle_state,
                operation_identifier: operation_identifier.clone(),
            },
        };
        Ok(Some(response))
    }
}
