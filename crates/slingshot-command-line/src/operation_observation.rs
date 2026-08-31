//! Watching one operation without holding it open.
//!
//! Observation is a read. Nothing here changes an operation, so a caller who
//! stops watching changes nothing about what is running - which is the whole
//! reason detaching is safe and the reason an interrupt is not a cancellation.
//!
//! # History may be read and never acted on
//!
//! A caller may name a historical target partition to look at work that
//! finished under an identity this client no longer serves. That is a read of
//! something settled. Waiting on it, resuming it, or reading work that has not
//! ended would be acting in a partition the caller is only allowed to look at,
//! so those combinations are refused rather than quietly aimed at the current
//! one.
//!
//! # A resume names what it saw
//!
//! Releasing paused work requires the revision the caller observed and the
//! category they observed it in. Without both, a resume written against one
//! state would apply to whatever the operation had since become, which is the
//! one thing a person reviewing a paused operation is trying to avoid.

/// What one observation asks for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Observation {
    /// What state it is in now.
    Status,
    /// What state it is in, waiting until it ends.
    Wait,
    /// What it produced.
    Result,
    /// One of the artifacts it produced.
    Artifact,
    /// Release it from a pause a person is reviewing.
    Resume,
}

impl Observation {
    /// Returns whether this observation may name a historical partition.
    ///
    /// Reading settled work may; acting on it may not. Waiting and resuming are
    /// actions in all but name - one holds the caller against work that is
    /// still moving, the other schedules more of it.
    #[must_use]
    pub fn permits_history(self) -> bool {
        matches!(self, Self::Status | Self::Result | Self::Artifact)
    }

    /// Returns whether this observation changes anything.
    #[must_use]
    pub fn changes_anything(self) -> bool {
        matches!(self, Self::Resume)
    }
}

/// Which partition an observation reads.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Partition {
    /// The one this client currently serves.
    Current {
        /// Its digest.
        author_target_identity_digest: String,
    },
    /// One it served before, read only.
    Historical {
        /// Its digest.
        author_target_identity_digest: String,
    },
}

impl Partition {
    /// Returns the digest either way.
    #[must_use]
    pub fn digest(&self) -> &str {
        match self {
            Self::Current { author_target_identity_digest }
            | Self::Historical { author_target_identity_digest } => author_target_identity_digest,
        }
    }

    /// Returns whether this is the partition the client currently serves.
    #[must_use]
    pub fn is_current(&self) -> bool {
        matches!(self, Self::Current { .. })
    }
}

/// Why one observation may not be made.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ObservationRefusal {
    /// A historical partition was named for something that acts.
    #[error("a historical target may be read and not acted on")]
    HistoryNotActionable,
    /// The operation has not ended and a historical partition was named.
    #[error("a historical target holds settled work, and this operation has not ended")]
    HistoryNotSettled,
    /// The resume names a revision the operation is no longer at.
    #[error("this operation is at revision {stored}, and the resume names {observed}")]
    RevisionMoved {
        /// What the caller observed.
        observed: u64,
        /// What is stored.
        stored: u64,
    },
    /// The resume names a category the operation is not paused in.
    #[error("this operation is not paused in the category the resume names")]
    CategoryChanged,
    /// The operation is not paused at all.
    #[error("this operation is not waiting for anybody, so there is nothing to release")]
    NotPaused,
}

/// What one observation is about to read.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ObservationRequest {
    /// Which runtime contract this client was built against.
    pub daemon_runtime_contract_digest: String,
    /// What it asks for.
    pub observation: Observation,
    /// Which operation.
    pub operation_identifier: String,
    /// Which partition.
    pub partition: Partition,
}

/// Requires one observation to be one the partition it names permits.
///
/// # Errors
///
/// Returns [`ObservationRefusal::HistoryNotActionable`] or
/// [`ObservationRefusal::HistoryNotSettled`].
pub fn require_permitted(
    request: &ObservationRequest,
    has_ended: bool,
) -> Result<(), ObservationRefusal> {
    if request.partition.is_current() {
        return Ok(());
    }
    if !request.observation.permits_history() {
        return Err(ObservationRefusal::HistoryNotActionable);
    }
    if !has_ended {
        return Err(ObservationRefusal::HistoryNotSettled);
    }
    Ok(())
}

/// What one resume was written against.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResumePreconditions {
    /// The category the caller observed the pause in.
    pub observed_category: String,
    /// The revision the caller observed.
    pub observed_revision: u64,
}

/// What the operation is, as the daemon holds it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PausedOperation {
    /// The category it is paused in, when it is paused.
    pub paused_category: Option<String>,
    /// The revision it stands at.
    pub revision: u64,
}

/// What consuming one resume did.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResumeOutcome {
    /// It was fresh, so exactly one recovery was scheduled.
    Scheduled,
    /// It has been applied before, so nothing was scheduled again.
    Replayed,
}

/// Requires one resume to be about the pause the caller actually saw.
///
/// Both the revision and the category, because either alone would let a resume
/// written against one state apply to whatever the operation had since become -
/// which is precisely what a person reviewing a paused operation is trying to
/// avoid.
///
/// # Errors
///
/// Returns [`ObservationRefusal::NotPaused`],
/// [`ObservationRefusal::RevisionMoved`], or
/// [`ObservationRefusal::CategoryChanged`].
pub fn require_resumable(
    preconditions: &ResumePreconditions,
    held: &PausedOperation,
) -> Result<(), ObservationRefusal> {
    let Some(category) = &held.paused_category else {
        return Err(ObservationRefusal::NotPaused);
    };
    if held.revision != preconditions.observed_revision {
        return Err(ObservationRefusal::RevisionMoved {
            observed: preconditions.observed_revision,
            stored: held.revision,
        });
    }
    if category != &preconditions.observed_category {
        return Err(ObservationRefusal::CategoryChanged);
    }
    Ok(())
}

/// Returns what applying one resume receipt does.
///
/// A receipt already consumed schedules nothing, whatever the operation has
/// become since - including after it has ended. Replaying a resume into a
/// finished operation would schedule recovery for work that is over.
#[must_use]
pub fn apply_receipt(consumed_receipts: &[String], receipt_identifier: &str) -> ResumeOutcome {
    if consumed_receipts.iter().any(|held| held == receipt_identifier) {
        ResumeOutcome::Replayed
    } else {
        ResumeOutcome::Scheduled
    }
}
