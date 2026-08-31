//! Reading one daemon answer as one outcome.
//!
//! The daemon speaks a versioned operation vocabulary and a caller reads a
//! closed outcome envelope, and this is the one place the first becomes the
//! second. Keeping it apart from the routing means a new service cannot invent
//! its own reading of an answer its neighbour already interprets: the shared
//! refusals are read once, here, and every service defers to them.
//!
//! # An answer outside the vocabulary is a defect, not an outcome
//!
//! The envelope vocabulary is closed and describes what happened to an
//! operation. A daemon that answers a listing with an artifact chunk has said
//! something the protocol does not let it say there, and rendering that as an
//! outcome would let a consumer parse a protocol violation as an answer. It
//! becomes a local refusal instead, which exits as a local failure and writes
//! no envelope at all.

use slingshot_local_protocol::message::{OperationResponse, TerminalFailureDisposition};

use crate::application::{Answer, Completion, RunRefusal};
use crate::exit_classification::{self, TerminalDisposition};
use crate::machine_outcome_envelope::{
    ArtifactAccess, MachineOutcomeEnvelope, MaintenanceResultAccess,
};
use crate::machine_outcome_envelope::{artifact_uri, maintenance_result_uri};

/// The member of a preview manifest that counts what it would release.
const RELEASED_ROWS_MEMBER: &str = "released_operation_rows";

/// What is said when no executor is installed for a target.
const NO_EXECUTOR: &str = "this daemon has no executor installed for that target";

/// What is said when a daemon serves something this caller is not addressing.
const SERVES_SOMETHING_ELSE: &str =
    "that daemon serves another target, revision, contract, or protocol version";

/// What an applied maintenance receipt releases.
const MAINTENANCE_APPLIED_CATEGORY: &str = "terminal-maintenance";

/// What is said when a daemon answers something the protocol does not allow.
const UNEXPECTED_ANSWER: &str = "that daemon answered with something this request cannot receive";

/// Returns what one submission's answer means.
///
/// # Errors
///
/// Returns the refusal a daemon's own answer describes.
pub fn submitted(response: &OperationResponse) -> Result<Submission, RunRefusal> {
    match response {
        OperationResponse::Accepted { operation_identifier } => {
            Ok(Submission::Admitted(Admitted {
                operation_identifier: operation_identifier.clone(),
                replayed: false,
            }))
        }
        OperationResponse::Replayed { operation_identifier } => {
            Ok(Submission::Admitted(Admitted {
                operation_identifier: operation_identifier.clone(),
                replayed: true,
            }))
        }
        other => shared(other).map(|completion| Submission::Ended(Box::new(completion))),
    }
}

/// What one submission's answer was.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Submission {
    /// The daemon took the work and named it.
    Admitted(Admitted),
    /// It had already finished, and this is what that means.
    Ended(Box<Completion>),
}

/// What a daemon admitted, before its revision is known.
///
/// The wire receipt names the operation and says whether it is new work. It
/// carries no revision, because admission is the moment before the operation
/// has a history; the revision comes from the status this client then reads,
/// which is also the first thing a caller could have read for themselves.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Admitted {
    /// Which operation.
    pub operation_identifier: String,
    /// Whether the daemon already held it.
    pub replayed: bool,
}

/// Where something a caller may fetch can be found.
///
/// Carried into the mapping rather than read from the answer, because a daemon
/// says what it holds and this client says where, in its own namespace, that
/// thing is addressed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AccessContext {
    /// Which partition.
    pub author_target_identity_digest: String,
    /// Which environment.
    pub environment: String,
    /// Which operation, when one is in play.
    pub operation_identifier: String,
    /// Which profile.
    pub profile: String,
}

/// Returns what one observation's answer means.
///
/// # Errors
///
/// Returns the refusal a daemon's own answer describes.
pub fn observed(
    response: &OperationResponse,
    context: &AccessContext,
) -> Result<Completion, RunRefusal> {
    let envelope = match response {
        OperationResponse::Status { lifecycle_state, operation_revision, .. } => {
            MachineOutcomeEnvelope::OperationStatus {
                revision: *operation_revision,
                state: lifecycle_state.clone(),
            }
        }
        OperationResponse::Progress { detail, operation_revision, .. } => {
            MachineOutcomeEnvelope::OperationStatus {
                revision: *operation_revision,
                state: detail.clone(),
            }
        }
        OperationResponse::RecoveryResumeApplied { current_lifecycle_state, .. } => {
            MachineOutcomeEnvelope::OperationResumeReceipt {
                category: current_lifecycle_state.clone(),
                replayed: false,
            }
        }
        OperationResponse::RecoveryResumeReplayed { current_lifecycle_state, .. } => {
            MachineOutcomeEnvelope::OperationResumeReceipt {
                category: current_lifecycle_state.clone(),
                replayed: true,
            }
        }
        OperationResponse::ArtifactStart {
            artifact_identifier,
            byte_length,
            content_digest,
            media_type,
        } => MachineOutcomeEnvelope::StructuredResultArtifactAccess {
            artifact: ArtifactAccess {
                artifact_identifier: artifact_identifier.clone(),
                author_target_identity_digest: context.author_target_identity_digest.clone(),
                byte_length: *byte_length,
                content_digest: content_digest.clone(),
                media_type: media_type.clone(),
                operation_identifier: context.operation_identifier.clone(),
                uri: artifact_uri(
                    &context.profile,
                    &context.environment,
                    &context.author_target_identity_digest,
                    &context.operation_identifier,
                    artifact_identifier,
                ),
            },
        },
        other => return shared(other),
    };
    Ok(succeeded(envelope))
}

/// Returns what one maintenance answer means.
///
/// # Errors
///
/// Returns the refusal a daemon's own answer describes.
pub fn maintained(
    response: &OperationResponse,
    context: &AccessContext,
) -> Result<Completion, RunRefusal> {
    let envelope = match response {
        OperationResponse::ListPage { next_cursor, operations } => {
            MachineOutcomeEnvelope::OperationListPage {
                continuation_token: next_cursor.clone(),
                operations: operations.clone(),
            }
        }
        OperationResponse::MaintenancePreview { manifest, reviewed_manifest_digest } => {
            MachineOutcomeEnvelope::MaintenancePreview {
                released_operation_rows: released_rows(manifest),
                reviewed_digest: reviewed_manifest_digest.clone(),
            }
        }
        OperationResponse::MaintenanceResultMetadata { description }
        | OperationResponse::MaintenanceResultStart { description } => {
            MachineOutcomeEnvelope::MaintenanceResultAccess {
                access: MaintenanceResultAccess {
                    association_revision: description.association_revision,
                    author_target_identity_digest: description
                        .author_target_identity_digest
                        .clone(),
                    byte_length: description.byte_length,
                    content_digest: description.content_digest.clone(),
                    kind: description.kind.clone(),
                    maintenance_result_identifier: description
                        .maintenance_result_identifier
                        .clone(),
                    media_type: description.media_type.clone(),
                    reviewed_source_digest: description.reviewed_source_digest.clone(),
                    uri: maintenance_result_uri(
                        &context.profile,
                        &context.environment,
                        &description.author_target_identity_digest,
                        &description.maintenance_result_identifier,
                    ),
                },
            }
        }
        OperationResponse::MaintenanceApplied { .. }
        | OperationResponse::MaintenanceReplayed { .. } => {
            MachineOutcomeEnvelope::OperationResumeReceipt {
                category: MAINTENANCE_APPLIED_CATEGORY.to_owned(),
                replayed: matches!(response, OperationResponse::MaintenanceReplayed { .. }),
            }
        }
        other => return shared(other),
    };
    Ok(succeeded(envelope))
}

/// Returns how many rows one preview manifest says it would release.
fn released_rows(manifest: &serde_json::Value) -> u64 {
    manifest[RELEASED_ROWS_MEMBER].as_u64().unwrap_or_default()
}

/// Returns the completion one successful envelope makes.
fn succeeded(envelope: MachineOutcomeEnvelope) -> Completion {
    Completion { answer: Answer::Envelope(Box::new(envelope)), exit: exit_classification::SUCCESS }
}

/// Returns what an answer every service may receive means.
///
/// One place, so a service cannot answer a shared refusal differently from its
/// neighbour. A response outside both this set and the service's own is a
/// daemon saying something the protocol does not let it say here, which is a
/// local defect rather than an outcome to render.
fn shared(response: &OperationResponse) -> Result<Completion, RunRefusal> {
    if let Some(completion) = ended(response) {
        return Ok(completion);
    }
    let reason = unavailable_reason(response)
        .or_else(|| identity_reason(response))
        .unwrap_or_else(|| UNEXPECTED_ANSWER.to_owned());
    Err(RunRefusal::Unavailable(reason))
}

/// Returns the completion one ended operation produces.
fn ended(response: &OperationResponse) -> Option<Completion> {
    match response {
        OperationResponse::TerminalFailure { disposition, kind, metadata, .. } => {
            let ending = classify_ending(*disposition);
            Some(Completion {
                answer: Answer::Envelope(Box::new(
                    MachineOutcomeEnvelope::OperationTerminalError {
                        disposition: format!("{disposition:?}"),
                        failure: serde_json::json!({ "metadata": metadata }),
                        kind: format!("{kind:?}"),
                    },
                )),
                exit: exit_classification::exit_for(ending),
            })
        }
        _ => None,
    }
}

/// Returns the operation, category, and evidence one recovery answer names.
///
/// The answer carries no revision, and a recovery report without one would be
/// unusable: quoting the expected revision is exactly what releasing a recovery
/// requires. So this reports the facts and leaves the caller to read the
/// revision, which is the same read a person would do next anyway.
#[must_use]
pub fn recovery_facts(response: &OperationResponse) -> Option<(String, String, String)> {
    match response {
        OperationResponse::RecoveryRequired { category, evidence, operation_identifier } => {
            Some((category.clone(), format!("{evidence:?}"), operation_identifier.clone()))
        }
        _ => None,
    }
}

/// Returns what one operation waiting in recovery reports.
#[must_use]
pub fn recovering(category: String, evidence: String, revision: u64) -> Completion {
    Completion {
        answer: Answer::Envelope(Box::new(MachineOutcomeEnvelope::OperationRecoveryRequired {
            category,
            evidence,
            revision,
        })),
        exit: exit_classification::INDETERMINATE,
    }
}

/// Returns what a terminal disposition means for this process's exit.
fn classify_ending(disposition: TerminalFailureDisposition) -> TerminalDisposition {
    match disposition {
        TerminalFailureDisposition::AuthoritativeNonExecution { .. } => {
            TerminalDisposition::AuthoritativeNonExecution
        }
        TerminalFailureDisposition::AuthoritativeRemoteFailure => {
            TerminalDisposition::AuthoritativeRemoteFailure
        }
        TerminalFailureDisposition::AuthoritativeRemoteSuccess => {
            TerminalDisposition::AuthoritativeRemoteSuccess
        }
        TerminalFailureDisposition::FailClosedIndeterminate { .. } => {
            TerminalDisposition::FailClosedIndeterminate
        }
    }
}

/// Returns why a daemon could not take this work, when that is the answer.
fn unavailable_reason(response: &OperationResponse) -> Option<String> {
    match response {
        OperationResponse::ExecutorUnavailable => Some(NO_EXECUTOR.to_owned()),
        OperationResponse::SchedulerCapacityExhausted { guidance }
        | OperationResponse::PersistentCapacityExhausted { guidance, .. }
        | OperationResponse::PersistentStorageBackpressure { guidance, .. } => {
            Some(guidance.clone())
        }
        OperationResponse::MalformedFrame { detail }
        | OperationResponse::InternalFailure { detail } => Some(detail.clone()),
        _ => None,
    }
}

/// Returns why a daemon refused this caller's identity, when that is the answer.
fn identity_reason(response: &OperationResponse) -> Option<String> {
    match response {
        OperationResponse::MissingOperation { operation_identifier } => {
            Some(format!("no operation named {operation_identifier} is held here"))
        }
        OperationResponse::IdentifierConflict { operation_identifier } => {
            Some(format!("{operation_identifier} already names other work here"))
        }
        OperationResponse::InvalidTransition { lifecycle_state, operation_identifier } => {
            Some(format!("{operation_identifier} is {lifecycle_state}, which does not permit this"))
        }
        OperationResponse::TargetMismatch { .. }
        | OperationResponse::RevisionMismatch { .. }
        | OperationResponse::RuntimeContractDigestMismatch { .. }
        | OperationResponse::IncompatibleOperationProtocol { .. } => {
            Some(SERVES_SOMETHING_ELSE.to_owned())
        }
        _ => None,
    }
}
