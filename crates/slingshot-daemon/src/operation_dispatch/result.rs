//! Read-only result projection from one retained operation observation.

use slingshot_domain::command::canonical_json::require_canonical_bytes;
use slingshot_domain::daemon_runtime_contract::DaemonRuntimeContract;
use slingshot_domain::installation::InstallationIdentifier;
use slingshot_domain::operation as domain;
use slingshot_local_protocol::message::{self as wire, OperationRequest, OperationResponse};
use slingshot_storage::artifact_store::{
    ArtifactAssociations, ArtifactIdentifier, CANONICAL_JSON_MEDIA_TYPE, STRUCTURED_RESULT_SLOT,
};
use slingshot_storage::operation_repository::OperationRepository;

use super::{BoundRequest, internal_failure, malformed};
use crate::operation_queries::{OperationResult, result_of};

impl BoundRequest {
    /// Reads a result without scheduling work or changing its settled outcome.
    /// Returns `None` only for a request owned by another handler.
    #[must_use]
    pub fn result(
        &self,
        repository: &OperationRepository,
        installation: &InstallationIdentifier,
    ) -> Option<OperationResponse> {
        let OperationRequest::Result { operation_identifier } = self.request() else {
            return None;
        };
        if operation_identifier.is_empty() || operation_identifier.contains('\0') {
            return Some(malformed());
        }
        let target = &self.envelope.author_target_identity_digest;
        let summary = match repository.read(target, operation_identifier) {
            Ok(Some(summary)) => summary,
            Ok(None) => {
                return Some(OperationResponse::MissingOperation {
                    operation_identifier: operation_identifier.clone(),
                });
            }
            Err(_) => return Some(internal_failure()),
        };
        // Every response describes this single observation, not a second row
        // read that could race settlement. Artifact associations are immutable
        // after settlement; concurrent maintenance can only make the read fail.
        let response = (|| {
            let limits = DaemonRuntimeContract::embedded();
            let operation_identifier = operation_identifier.clone();
            match result_of(&summary).ok()? {
                OperationResult::Pending { lifecycle_state } => {
                    if lifecycle_state.is_terminal() {
                        return None;
                    }
                    Some(OperationResponse::Status {
                        lifecycle_state: word(lifecycle_state)?,
                        operation_identifier,
                        operation_revision: summary.record.revision,
                    })
                }
                OperationResult::RecoveryRequired { recovery } => {
                    if summary.record.lifecycle_state.is_terminal()
                        || !recovery.category.admits(recovery.evidence)
                    {
                        return None;
                    }
                    Some(OperationResponse::RecoveryRequired {
                        category: word(recovery.category)?,
                        evidence: match recovery.evidence {
                            domain::RecoveryExecutionEvidence::ExecutionCertainty { certainty } => {
                                wire::RecoveryExecutionEvidence::ExecutionCertainty {
                                    certainty: certainty_on_wire(certainty),
                                }
                            }
                            domain::RecoveryExecutionEvidence::AuthoritativeRemoteSuccess => {
                                wire::RecoveryExecutionEvidence::AuthoritativeRemoteSuccess
                            }
                        },
                        operation_identifier,
                    })
                }
                OperationResult::Failed { failure } => {
                    if summary.record.lifecycle_state != domain::OperationLifecycleState::Failed
                        || failure.metadata.as_ref().is_some_and(|metadata| {
                            metadata.len() as u64
                                > limits.limit("maximum_terminal_failure_metadata_bytes")
                        })
                    {
                        return None;
                    }
                    // result_of checked the domain kind/disposition pairing.
                    Some(OperationResponse::TerminalFailure {
                        kind: kind_on_wire(failure.kind),
                        disposition: disposition_on_wire(failure.disposition),
                        metadata: failure.metadata,
                        operation_identifier,
                    })
                }
                OperationResult::Succeeded {
                    disposition: domain::ResultDisposition::Inline,
                    inline_result,
                } => {
                    let text = inline_result?;
                    if text.len() as u64 > limits.limit("maximum_inline_machine_result_bytes") {
                        return None;
                    }
                    let result = require_canonical_bytes(text.as_bytes()).ok()?;
                    Some(OperationResponse::ResultInline { operation_identifier, result })
                }
                OperationResult::Succeeded {
                    disposition: domain::ResultDisposition::Artifact,
                    inline_result,
                } => {
                    if inline_result.is_some() {
                        return None;
                    }
                    let metadata = ArtifactAssociations::new(repository.database())
                        .read(target, &operation_identifier, STRUCTURED_RESULT_SLOT)
                        .ok()??;
                    let expected = ArtifactIdentifier::derive(
                        installation,
                        target,
                        &operation_identifier,
                        STRUCTURED_RESULT_SLOT,
                    );
                    if metadata.artifact_identifier != expected
                        || !wire::digest_is_canonical(&metadata.content_digest)
                        || metadata.media_type != CANONICAL_JSON_MEDIA_TYPE
                        || metadata.byte_length == 0
                        || metadata.byte_length
                            > limits.limit("maximum_canonical_structured_result_bytes")
                    {
                        return None;
                    }
                    Some(OperationResponse::ResultArtifact {
                        artifact_identifier: metadata.artifact_identifier.as_text().to_owned(),
                        byte_length: metadata.byte_length,
                        content_digest: metadata.content_digest,
                        media_type: metadata.media_type,
                        operation_identifier,
                    })
                }
            }
        })();
        Some(response.unwrap_or_else(internal_failure))
    }
}

fn word(value: impl serde::Serialize) -> Option<String> {
    match serde_json::to_value(value).ok()? {
        serde_json::Value::String(word) => Some(word),
        _ => None,
    }
}

fn certainty_on_wire(
    value: domain::OperationExecutionCertainty,
) -> wire::OperationExecutionCertainty {
    match value {
        domain::OperationExecutionCertainty::ConfirmedNotExecuted => {
            wire::OperationExecutionCertainty::ConfirmedNotExecuted
        }
        domain::OperationExecutionCertainty::SubmissionUnknown => {
            wire::OperationExecutionCertainty::SubmissionUnknown
        }
        domain::OperationExecutionCertainty::RemoteOutcomeUnknown => {
            wire::OperationExecutionCertainty::RemoteOutcomeUnknown
        }
    }
}

fn kind_on_wire(value: domain::TerminalFailureKind) -> wire::TerminalFailureKind {
    match value {
        domain::TerminalFailureKind::Rejected => wire::TerminalFailureKind::Rejected,
        domain::TerminalFailureKind::RemoteFailed => wire::TerminalFailureKind::RemoteFailed,
        domain::TerminalFailureKind::ResultUnavailable => {
            wire::TerminalFailureKind::ResultUnavailable
        }
        domain::TerminalFailureKind::RecoveryWindowExpired => {
            wire::TerminalFailureKind::RecoveryWindowExpired
        }
        domain::TerminalFailureKind::RemoteStateLost => wire::TerminalFailureKind::RemoteStateLost,
        domain::TerminalFailureKind::IntegrityFailure => {
            wire::TerminalFailureKind::IntegrityFailure
        }
        domain::TerminalFailureKind::RetryPolicyExhausted => {
            wire::TerminalFailureKind::RetryPolicyExhausted
        }
    }
}

fn disposition_on_wire(
    value: domain::TerminalFailureDisposition,
) -> wire::TerminalFailureDisposition {
    match value {
        domain::TerminalFailureDisposition::AuthoritativeNonExecution { certainty } => {
            wire::TerminalFailureDisposition::AuthoritativeNonExecution {
                certainty: certainty_on_wire(certainty),
            }
        }
        domain::TerminalFailureDisposition::AuthoritativeRemoteFailure => {
            wire::TerminalFailureDisposition::AuthoritativeRemoteFailure
        }
        domain::TerminalFailureDisposition::AuthoritativeRemoteSuccess => {
            wire::TerminalFailureDisposition::AuthoritativeRemoteSuccess
        }
        domain::TerminalFailureDisposition::FailClosedIndeterminate { certainty } => {
            wire::TerminalFailureDisposition::FailClosedIndeterminate {
                certainty: certainty_on_wire(certainty),
            }
        }
    }
}
