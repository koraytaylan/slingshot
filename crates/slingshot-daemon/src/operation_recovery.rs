//! Resuming one paused operation, and never doing anything else.
//!
//! An operation waiting on a recovery is work that is not finished and not
//! failed. Resuming it means one thing only: making the durable row eligible
//! for the scheduler again. This service allocates no identifier, invokes no
//! executor, submits nothing to a remote system, and leaves the command
//! fingerprint exactly as it was - so resuming retries whatever local half was
//! outstanding and never repeats the remote half, which may well have already
//! happened.
//!
//! Every resume is keyed by a source fingerprint and answered from a durable
//! receipt. That is what makes a repeat safe to send: whether a resume took
//! effect cannot be reconstructed from the operation's current state, because
//! later progress, another recovery cycle, and terminal settlement all look the
//! same from outside. The receipt says so directly, and keeps saying so
//! afterwards.
//!
//! The preconditions are exact and all of them are checked before anything is
//! written. The operation must be in this daemon's partition at this daemon's
//! revision, still nonterminal, waiting on the category the caller named, and
//! at the revision the caller last saw. A resume that guessed at any of these
//! would be a person authorizing something other than what they looked at.

use slingshot_domain::operation::{RecoveryCategory, RecoveryResumeReceipt};
use slingshot_storage::operation_repository::{
    OperationRepository, RepositoryFailure, ResumeOutcome,
};

/// What a person asked to be resumed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResumeRequest {
    /// The partition the operation is in.
    pub author_target_identity_digest: String,
    /// The category the caller believes it is waiting on.
    pub expected_recovery_category: RecoveryCategory,
    /// The revision the caller last saw.
    pub expected_revision: u64,
    /// The operation to resume.
    pub operation_identifier: String,
    /// The environment revision the caller selected.
    pub selected_environment_revision: String,
}

/// Why a resume was refused.
///
/// Every one of these leaves the operation exactly as it was. A resume is a
/// person's decision about a specific situation, so a situation that turns out
/// to be different is a refusal rather than a resume of something else.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ResumeRefusal {
    /// There is no such operation in that partition.
    #[error("no operation named {identifier} in that target partition")]
    NoSuchOperation {
        /// The identifier the caller named.
        identifier: String,
    },
    /// The operation was admitted under another environment revision.
    #[error("that operation belongs to another environment revision, and was not resumed")]
    RevisionMismatch,
    /// The operation has already ended.
    #[error("that operation has already ended, and an ending is not something to resume")]
    AlreadyTerminal,
    /// The operation is not waiting on anything.
    #[error("that operation is not waiting on anything, so there is nothing to resume")]
    NotWaiting,
    /// The operation is waiting on something else.
    #[error("that operation is waiting on {holding:?}, and the request named {named:?}")]
    CategoryMismatch {
        /// What it is actually waiting on.
        holding: RecoveryCategory,
        /// What the caller believed.
        named: RecoveryCategory,
    },
    /// A person may not resume this kind of wait.
    #[error("that recovery is not one a person resumes; the daemon retries it on its own")]
    NotManuallyResumable,
    /// The caller's revision is not the stored one.
    #[error("the operation moved on: expected revision {expected}, stored {stored}")]
    RevisionMoved {
        /// What the caller last saw.
        expected: u64,
        /// What the row holds.
        stored: u64,
    },
}

/// What resuming did.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ResumeResponse {
    /// The operation became eligible, and this is the proof.
    Applied(Box<RecoveryResumeReceipt>),
    /// This exact source had already been resumed, and this is that proof.
    Replayed(Box<RecoveryResumeReceipt>),
    /// Nothing was resumed, and this says why.
    Refused(ResumeRefusal),
}

/// Reason a resume could not be attempted at all.
#[derive(Debug, thiserror::Error)]
pub enum ResumeFailure {
    /// The database refused.
    #[error(transparent)]
    Repository(#[from] RepositoryFailure),
}

/// Returns the source fingerprint one resume request is keyed by.
///
/// The operation's own fingerprint, the revision being resumed from, and the
/// category, so two resumes of the same operation at different points are
/// different sources - and the same resume sent twice is one.
#[must_use]
pub fn source_fingerprint(
    command_fingerprint: &str,
    expected_revision: u64,
    category: RecoveryCategory,
) -> String {
    format!("{command_fingerprint}:{expected_revision}:{category:?}")
}

/// Resumes one paused operation, or says why it did not.
///
/// The receipt is looked for first, so a repeat is answered from what was
/// committed rather than re-checked against a situation that has since moved
/// on. That is the point of the receipt: an exact repeat after later progress,
/// another recovery cycle, terminal settlement, or a restart still replays.
///
/// Everything else is checked before anything is written, and a refusal leaves
/// the operation byte for byte as it was.
///
/// # Errors
///
/// Returns [`ResumeFailure`] when the database refuses. A refusal of the
/// request is a response rather than an error, because "that operation has
/// already ended" is a fact the caller asked for.
pub fn resume(
    repository: &OperationRepository,
    request: &ResumeRequest,
    now_unix_milliseconds: u64,
) -> Result<ResumeResponse, ResumeFailure> {
    let Some(summary) =
        repository.read(&request.author_target_identity_digest, &request.operation_identifier)?
    else {
        return Ok(ResumeResponse::Refused(ResumeRefusal::NoSuchOperation {
            identifier: request.operation_identifier.clone(),
        }));
    };
    let source = source_fingerprint(
        summary.command_fingerprint.as_text(),
        request.expected_revision,
        request.expected_recovery_category,
    );
    if let Some(held) =
        repository.read_resume_receipt(&request.author_target_identity_digest, &source)?
    {
        return Ok(ResumeResponse::Replayed(Box::new(held)));
    }
    if let Err(refusal) = require_resumable(&summary, request) {
        return Ok(ResumeResponse::Refused(refusal));
    }

    let outcome = repository.record_resume_receipt(
        &request.author_target_identity_digest,
        &request.operation_identifier,
        &source,
        &request.selected_environment_revision,
        summary.record.revision,
        now_unix_milliseconds,
    )?;
    Ok(match outcome {
        ResumeOutcome::Applied(receipt) => ResumeResponse::Applied(receipt),
        ResumeOutcome::Replayed(receipt) => ResumeResponse::Replayed(receipt),
    })
}

/// Requires the operation to be the one the caller looked at.
fn require_resumable(
    summary: &slingshot_storage::operation_repository::OperationSummary,
    request: &ResumeRequest,
) -> Result<(), ResumeRefusal> {
    if summary.selected_environment_revision != request.selected_environment_revision {
        return Err(ResumeRefusal::RevisionMismatch);
    }
    if summary.record.lifecycle_state.is_terminal() {
        return Err(ResumeRefusal::AlreadyTerminal);
    }
    let Some(recovery) = &summary.record.outstanding_recovery else {
        return Err(ResumeRefusal::NotWaiting);
    };
    if recovery.category != request.expected_recovery_category {
        return Err(ResumeRefusal::CategoryMismatch {
            holding: recovery.category,
            named: request.expected_recovery_category,
        });
    }
    if !recovery.manual_resume_eligible {
        return Err(ResumeRefusal::NotManuallyResumable);
    }
    if summary.record.revision != request.expected_revision {
        return Err(ResumeRefusal::RevisionMoved {
            expected: request.expected_revision,
            stored: summary.record.revision,
        });
    }
    Ok(())
}
