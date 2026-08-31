//! Stopping a job that is retrying against something that will never work.
//!
//! A job with a wrong payload will fail, wait, and fail again until its retries
//! run out, and until now there was no way to stop it. Cancelling is destructive
//! in this registry's sense: work that was queued stops being queued.
//!
//! A job that has already succeeded, been cancelled, or been dropped cannot be
//! cancelled, and that is a refusal rather than a success - so a caller can tell
//! "I stopped it" from "it had already stopped".

use serde::{Deserialize, Serialize};

use crate::command::process_identity::{SlingJobIdentifier, SlingJobState};
use crate::command::resource_mutation::MutationResultFailure;

/// One request to cancel a Sling job.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CancelSlingJobCommand {
    /// Job to cancel.
    pub job_identifier: SlingJobIdentifier,
}

/// Why a job was not cancelled.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CancelSlingJobFailure {
    /// No job answers to that identifier.
    JobNotFound,
    /// The job is in a state it cannot be cancelled from.
    JobNotCancellable,
    /// The author refused to cancel it.
    PlatformControlRejected,
    /// Nobody can tell whether it was cancelled.
    PlatformControlOutcomeUnknown,
}

/// One refused cancellation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CancelSlingJobRefusal {
    /// Why it was refused.
    pub failure: CancelSlingJobFailure,
    /// Job this request named.
    pub job_identifier: SlingJobIdentifier,
}

impl CancelSlingJobRefusal {
    /// Returns whether this refusal proves the operation changed nothing.
    #[must_use]
    pub fn proves_no_effect(&self) -> bool {
        !matches!(self.failure, CancelSlingJobFailure::PlatformControlOutcomeUnknown)
    }

    /// Requires this refusal to answer `command`.
    ///
    /// # Errors
    ///
    /// Returns [`MutationResultFailure::NotThisRequest`] when it names another
    /// request's job.
    pub fn require_answers(
        &self,
        command: &CancelSlingJobCommand,
    ) -> Result<(), MutationResultFailure> {
        if self.job_identifier == command.job_identifier {
            Ok(())
        } else {
            Err(MutationResultFailure::NotThisRequest)
        }
    }
}

/// What the author observed after the cancellation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CancelSlingJobResult {
    /// Job that was acted on.
    pub job_identifier: SlingJobIdentifier,
    /// The state it was in afterwards.
    pub observed_state: SlingJobState,
}

impl CancelSlingJobResult {
    /// Requires this result to answer `command`.
    ///
    /// # Errors
    ///
    /// Returns [`MutationResultFailure::NotThisRequest`] when it names another
    /// request's job, or reports the job as still cancellable - a cancellation
    /// that succeeded and left the job queued is two answers at once.
    pub fn require_answers(
        &self,
        command: &CancelSlingJobCommand,
    ) -> Result<(), MutationResultFailure> {
        if self.job_identifier != command.job_identifier || self.observed_state.is_cancellable() {
            return Err(MutationResultFailure::NotThisRequest);
        }
        Ok(())
    }
}
