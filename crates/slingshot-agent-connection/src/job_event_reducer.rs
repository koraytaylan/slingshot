//! What one event does to one job, and what it is not allowed to do.
//!
//! Pure, and separate from the subscription's fold. The two answer different
//! questions and must be able to disagree: an event about work this daemon does
//! not hold moves the stream on and moves no job at all, and an event that
//! cannot be believed about a job must not move the stream either.
//!
//! # An event is believed only when everything about it is
//!
//! Before an associated job advances, the event's revision, contracts, and
//! submitted digest are all required to be the ones this daemon submitted
//! under. A result that is validly shaped, correctly sequenced, and produced by
//! the same command with different arguments is exactly the case a shape check
//! would let through, so the digest is checked and not the shape.
//!
//! # Gaps are visible, and going backwards is not a failure
//!
//! An event that follows the applied sequence without being next means
//! something in between was missed. The reducer does not guess what: it asks
//! for a snapshot and changes nothing, because filling a gap from the event
//! after it is inventing the events inside it. An event from before the applied
//! sequence is history, and history moves the cursor and nothing else.
//!
//! # A conflict is not an ending
//!
//! Two accounts of one position leave both folds exactly as they were and ask
//! for reconciliation. Nothing here promotes a disagreement into a terminal
//! disposition, because a job whose accounts disagree is a job whose ending
//! nobody knows.

use slingshot_agent_protocol::job_contract::JobEventKind;
use slingshot_agent_protocol::wire_contract::{ExpectedProvenance, WireRefusal};
use slingshot_domain::remote_job::{
    AgentJobState, JobEventSequence, RemoteJobFailure, RemoteJobObservation,
};

use crate::server_sent_event_decoder::TerminalCorrelation;

/// What one event did to one job.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum JobDisposition {
    /// The job's durable state moved.
    Applied,
    /// The same sequence with the same account, so nothing moved.
    ExactReplay,
    /// Nothing about the job moved, and only the stream's position did.
    StaleCursorOnly,
    /// Something in between was missed, and only a snapshot can say what.
    NeedsSnapshot,
    /// Two accounts of one sequence, which nothing here can settle.
    IntegrityConflictNeedsReconciliation,
}

impl JobDisposition {
    /// Returns whether the job's durable state changed.
    #[must_use]
    pub fn changed_state(self) -> bool {
        matches!(self, Self::Applied)
    }

    /// Returns whether settling this needs something other than more events.
    #[must_use]
    pub fn needs_authority(self) -> bool {
        matches!(self, Self::NeedsSnapshot | Self::IntegrityConflictNeedsReconciliation)
    }
}

/// Why one event cannot be reduced against this job at all.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ReducerRefusal {
    /// The event was produced under another environment revision.
    #[error("this operation was submitted under {held}, and this event names {named}")]
    AnotherRevision {
        /// Which revision the operation was submitted under.
        held: String,
        /// Which revision the event names.
        named: String,
    },
    /// The event ends a submission this daemon did not make.
    #[error("this event ends a submission this daemon did not make")]
    AnotherSubmission,
    /// The event names contracts this build does not have.
    #[error(transparent)]
    Provenance(#[from] WireRefusal),
    /// An ending carries no correlation, or a correlation ends nothing.
    #[error("an ending carries its correlation, and nothing else carries one")]
    CorrelationMisplaced,
    /// The event describes a job transition the domain does not allow.
    #[error(transparent)]
    Job(#[from] RemoteJobFailure),
}

/// What must agree before an associated job's state may move.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AssociationBinding {
    /// Which contracts this build submitted under.
    pub expected_provenance: ExpectedProvenance,
    /// Which environment revision it submitted under.
    pub selected_environment_revision: String,
    /// Which submission the job is carrying out.
    pub submitted_command_digest: String,
}

/// One validated event, as the reducer reads it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ObservedJobEvent {
    /// How many physical attempts the agent says have carried this.
    pub attempt: u64,
    /// What an ending says about which submission it ends.
    pub correlation: Option<TerminalCorrelation>,
    /// What happened.
    pub kind: JobEventKind,
    /// How far the agent says it has got.
    pub progress: u64,
    /// Which environment revision it was produced under.
    pub selected_environment_revision: String,
    /// Where it sits in this job's own sequence.
    pub sequence: JobEventSequence,
}

impl ObservedJobEvent {
    /// Returns the durable state this event describes.
    ///
    /// Progress and a fresh start both mean running. A physical requeue arrives
    /// as another start, and reading that as a return to the queue would make
    /// Sling's at-least-once delivery look like work that stopped.
    #[must_use]
    pub fn described_state(&self) -> AgentJobState {
        match self.kind {
            JobEventKind::Accepted => AgentJobState::Queued,
            JobEventKind::Started | JobEventKind::Progress => AgentJobState::Running,
            JobEventKind::Succeeded => AgentJobState::Succeeded,
            JobEventKind::Failed => AgentJobState::Failed,
        }
    }
}

/// What this daemon already holds about one job.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RetainedJob {
    /// What is known about it.
    pub observation: RemoteJobObservation,
    /// The sequence a snapshot has already accounted for.
    ///
    /// Everything at or below this has been settled by authority rather than by
    /// events, so an event from down there needs no retained row to compare
    /// against and no digest to agree with. It is simply old news.
    pub snapshot_watermark: JobEventSequence,
}

/// Returns what one event does to a job, and what the job becomes.
///
/// A job that is not held produces [`JobDisposition::StaleCursorOnly`] and no
/// state at all: the stream's position advances, and the event is left for
/// whichever lookup or snapshot eventually associates the operation. Inventing
/// a local row from a stream event would mean holding work this daemon never
/// submitted.
///
/// # Errors
///
/// Returns [`ReducerRefusal`] naming the first thing that does not agree. Every
/// refusal leaves the job exactly as it was, which is what makes the caller
/// safe to record one bounded incident and move nothing.
pub fn reduce(
    retained: Option<&RetainedJob>,
    binding: &AssociationBinding,
    event: &ObservedJobEvent,
) -> Result<(JobDisposition, Option<RemoteJobObservation>), ReducerRefusal> {
    let Some(retained) = retained else {
        return Ok((JobDisposition::StaleCursorOnly, None));
    };
    require_bound(binding, event)?;
    let applied = retained.observation.applied_sequence;
    if event.sequence == applied {
        return Ok((replay_disposition(retained, event), None));
    }
    if !event.sequence.follows(applied) {
        return Ok((JobDisposition::StaleCursorOnly, None));
    }
    if !event.sequence.immediately_follows(applied) {
        return Ok((JobDisposition::NeedsSnapshot, None));
    }
    let advanced = retained.observation.advanced(
        event.described_state(),
        event.sequence,
        event.attempt,
        event.progress,
    )?;
    Ok((JobDisposition::Applied, Some(advanced)))
}

/// Returns whether a repeated sequence is the same account or another one.
fn replay_disposition(retained: &RetainedJob, event: &ObservedJobEvent) -> JobDisposition {
    let held = &retained.observation;
    let same = held.state == event.described_state()
        && held.attempt == event.attempt
        && held.progress == event.progress;
    if same {
        JobDisposition::ExactReplay
    } else {
        JobDisposition::IntegrityConflictNeedsReconciliation
    }
}

/// Requires one event to be about the submission this job is carrying out.
///
/// # Errors
///
/// Returns [`ReducerRefusal::AnotherRevision`],
/// [`ReducerRefusal::CorrelationMisplaced`], [`ReducerRefusal::Provenance`], or
/// [`ReducerRefusal::AnotherSubmission`].
pub fn require_bound(
    binding: &AssociationBinding,
    event: &ObservedJobEvent,
) -> Result<(), ReducerRefusal> {
    if event.selected_environment_revision != binding.selected_environment_revision {
        return Err(ReducerRefusal::AnotherRevision {
            held: binding.selected_environment_revision.clone(),
            named: event.selected_environment_revision.clone(),
        });
    }
    match (&event.correlation, event.kind.is_terminal()) {
        (Some(correlation), true) => {
            binding.expected_provenance.require_matching(&correlation.provenance)?;
            if correlation.submitted_command_digest != binding.submitted_command_digest {
                return Err(ReducerRefusal::AnotherSubmission);
            }
            Ok(())
        }
        (None, false) => Ok(()),
        (None, true) | (Some(_), false) => Err(ReducerRefusal::CorrelationMisplaced),
    }
}
