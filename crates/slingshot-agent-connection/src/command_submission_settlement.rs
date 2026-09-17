//! What one whole submission exchange means after the request has left.

use slingshot_domain::author_agent_transport_contract::AuthorAgentTransportContract;

use crate::author_hypertext_transfer_protocol_policy::retry_delay_milliseconds;

use super::{
    Checkpoint, Exchange, SUBMISSION_MEDIA_TYPE, StatusClass, Submission,
    SubmissionAcknowledgement, SubmissionOutcome, UnknownCause, classify_status,
};

impl Submission {
    /// Returns what one whole exchange means for this submission.
    ///
    /// Transport settles before content and content before identity: an answer
    /// read off a message this daemon cannot frame is not an answer.
    #[must_use]
    pub fn interpret(&self, exchange: &Exchange) -> SubmissionOutcome {
        if let Err(cause) = require_transport_clean(exchange) {
            return SubmissionOutcome::SubmissionUnknown { cause };
        }
        let class = classify_status(exchange.status);
        match class {
            StatusClass::Retryable => {
                return SubmissionOutcome::RetryAfter {
                    milliseconds: retry_delay_milliseconds(exchange.retry_after_milliseconds),
                };
            }
            StatusClass::Unvalidated => {
                return SubmissionOutcome::SubmissionUnknown {
                    cause: UnknownCause::UnvalidatedStatus,
                };
            }
            StatusClass::Answered | StatusClass::Rejected | StatusClass::Conflict => {}
        }
        let Some(acknowledgement) = &exchange.acknowledgement else {
            return SubmissionOutcome::SubmissionUnknown { cause: UnknownCause::Body };
        };
        if let Err(cause) = self.require_echoes(acknowledgement) {
            return SubmissionOutcome::SubmissionUnknown { cause };
        }
        if matches!(class, StatusClass::Conflict) {
            return SubmissionOutcome::Conflict;
        }
        self.settle(class, acknowledgement, exchange.elapsed_milliseconds)
    }

    /// Returns what a failed exchange means, given where it failed.
    ///
    /// Before the first request byte this is a proof; after it, it is not.
    #[must_use]
    pub fn transport_failure(checkpoint: Checkpoint) -> SubmissionOutcome {
        if checkpoint.bytes_may_have_reached_author() {
            SubmissionOutcome::SubmissionUnknown { cause: UnknownCause::Deadline(checkpoint) }
        } else {
            SubmissionOutcome::ConfirmedNotExecuted { checkpoint }
        }
    }

    /// Requires every echoed field to be the one that was sent.
    fn require_echoes(
        &self,
        acknowledgement: &SubmissionAcknowledgement,
    ) -> Result<(), UnknownCause> {
        if acknowledgement.provenance != self.provenance {
            return Err(UnknownCause::Provenance);
        }
        if acknowledgement.selected_environment_revision
            != self.operation.selected_environment_revision
        {
            return Err(UnknownCause::Revision);
        }
        if acknowledgement.agent_operation_identifier != self.operation.agent_operation_identifier {
            return Err(UnknownCause::Identity);
        }
        if acknowledgement.agent_event_store_generation
            != self.operation.agent_event_store_generation
        {
            return Err(UnknownCause::Generation);
        }
        if acknowledgement.author_target_identity_digest
            != self.operation.author_target_identity_digest
        {
            return Err(UnknownCause::Partition);
        }
        if acknowledgement.submitted_command_digest != self.submitted_command_digest {
            return Err(UnknownCause::Digest);
        }
        if acknowledgement.daemon_subscription_identifier != self.daemon_subscription_identifier {
            return Err(UnknownCause::Registration);
        }
        Ok(())
    }

    /// Returns the outcome of an answer whose echoes have all been checked.
    fn settle(
        &self,
        class: StatusClass,
        acknowledgement: &SubmissionAcknowledgement,
        elapsed_milliseconds: u64,
    ) -> SubmissionOutcome {
        if let Some(outcome) = Self::settle_non_execution(class, acknowledgement) {
            return outcome;
        }
        if matches!(class, StatusClass::Rejected) {
            return SubmissionOutcome::SubmissionUnknown { cause: UnknownCause::Body };
        }
        if acknowledgement.retired {
            return if acknowledgement.physical_sling_job_identifiers.is_empty() {
                SubmissionOutcome::RecoveryWindowExpired
            } else {
                SubmissionOutcome::SubmissionUnknown { cause: UnknownCause::Body }
            };
        }
        if let Err(cause) = require_bounded_job_set(&acknowledgement.physical_sling_job_identifiers)
        {
            return SubmissionOutcome::SubmissionUnknown { cause };
        }
        let Some(remaining_retention_milliseconds) = remaining_retention_milliseconds(
            acknowledgement.granted_retention_milliseconds,
            elapsed_milliseconds,
        ) else {
            return SubmissionOutcome::SubmissionUnknown { cause: UnknownCause::Retention };
        };
        let physical_sling_job_identifiers = acknowledgement.physical_sling_job_identifiers.clone();
        if acknowledgement.already_accepted {
            SubmissionOutcome::Duplicate {
                physical_sling_job_identifiers,
                remaining_retention_milliseconds,
            }
        } else {
            SubmissionOutcome::Accepted {
                physical_sling_job_identifiers,
                remaining_retention_milliseconds,
            }
        }
    }

    /// Returns the outcome of an acknowledgement that named a closed refusal.
    fn settle_non_execution(
        class: StatusClass,
        acknowledgement: &SubmissionAcknowledgement,
    ) -> Option<SubmissionOutcome> {
        let non_execution = acknowledgement.non_execution?;
        if acknowledgement.already_accepted
            || acknowledgement.retired
            || !acknowledgement.physical_sling_job_identifiers.is_empty()
        {
            return Some(SubmissionOutcome::SubmissionUnknown { cause: UnknownCause::Body });
        }
        Some(if matches!(class, StatusClass::Rejected) {
            SubmissionOutcome::AuthoritativeNonExecution { non_execution }
        } else {
            SubmissionOutcome::SubmissionUnknown { cause: UnknownCause::UnvalidatedStatus }
        })
    }
}

/// Requires one message to be framed and typed the way an answer must be.
fn require_transport_clean(exchange: &Exchange) -> Result<(), UnknownCause> {
    exchange.head.require_acceptable().map_err(UnknownCause::from)?;
    if exchange.trailer_section_present {
        return Err(UnknownCause::TrailerSection);
    }
    if exchange.framing_ambiguous {
        return Err(UnknownCause::Framing);
    }
    if exchange.trailing_bytes {
        return Err(UnknownCause::TrailingBytes);
    }
    if exchange.media_type != SUBMISSION_MEDIA_TYPE {
        return Err(UnknownCause::Media);
    }
    let allowed =
        AuthorAgentTransportContract::embedded().limit("maximum_finite_response_body_bytes");
    if exchange.body_bytes > allowed {
        return Err(UnknownCause::Body);
    }
    if exchange.unknown_fields {
        return Err(UnknownCause::UnknownField);
    }
    Ok(())
}

/// Requires an acknowledged job set to be one this daemon can act on.
///
/// Non-empty, distinct, sorted, bounded. Sortedness is checked rather than
/// imposed: the set is compared across recoveries, and sorting hides that the
/// agent answered differently.
fn require_bounded_job_set(identifiers: &[String]) -> Result<(), UnknownCause> {
    let contract = AuthorAgentTransportContract::embedded();
    let allowed_matches = contract.limit("maximum_physical_sling_job_matches");
    let allowed_bytes = contract.limit("maximum_sling_job_identifier_bytes");
    if identifiers.is_empty() {
        return Err(UnknownCause::EmptyIdentifier);
    }
    if u64::try_from(identifiers.len()).unwrap_or(u64::MAX) > allowed_matches {
        return Err(UnknownCause::Body);
    }
    if !identifiers.is_sorted_by(|earlier, later| earlier < later) {
        return Err(UnknownCause::Body);
    }
    if identifiers.iter().any(|identifier| {
        identifier.is_empty() || u64::try_from(identifier.len()).unwrap_or(u64::MAX) > allowed_bytes
    }) {
        return Err(UnknownCause::EmptyIdentifier);
    }
    Ok(())
}

/// Returns how much of a granted retention is left, counted from request start.
///
/// Counted from before any network work, because a slow answer spends the
/// retention it describes. An exactly exhausted retention is expired: zero
/// remaining time promises nothing, and a caller would hunt vanished results.
#[must_use]
pub fn remaining_retention_milliseconds(
    granted_milliseconds: u64,
    elapsed_since_request_start_milliseconds: u64,
) -> Option<u64> {
    let cap = AuthorAgentTransportContract::embedded()
        .limit("maximum_persisted_remaining_retention_milliseconds");
    if elapsed_since_request_start_milliseconds >= granted_milliseconds {
        return None;
    }
    Some((granted_milliseconds - elapsed_since_request_start_milliseconds).min(cap))
}
