//! What the agent says about a job, and what a daemon may conclude from it.
//!
//! An event stream is not a record. Events can be missed, replayed, or arrive
//! after the thing they describe has moved on, so a daemon that treated the
//! last event it saw as the truth would be treating its own connection quality
//! as a fact about the remote system. A snapshot is the record; events are a
//! way of learning sooner.
//!
//! So every event carries the generation and the operation it belongs to, and a
//! daemon reconciles rather than accumulates. An event from a generation the
//! store has since rebuilt describes something that no longer exists, and an
//! event about an operation this daemon does not hold is not evidence of
//! anything it should act on.

use serde::{Deserialize, Serialize};

/// What one job event says happened.
///
/// Closed, because a daemon that met an event kind it did not know would have
/// to guess whether it mattered - and both answers are wrong: ignoring it may
/// miss an ending, and acting on it may invent one.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum JobEventKind {
    /// The agent accepted the submission.
    Accepted,
    /// The agent began running it.
    Started,
    /// Something worth reporting happened.
    Progress,
    /// It finished, and produced a result.
    Succeeded,
    /// It finished without succeeding.
    Failed,
}

impl JobEventKind {
    /// Returns whether this kind ends the job.
    #[must_use]
    pub fn is_terminal(self) -> bool {
        matches!(self, Self::Succeeded | Self::Failed)
    }
}

/// One thing the agent says happened to one job.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct JobEvent {
    /// Which incarnation of the store it came from.
    pub agent_event_store_generation: u64,
    /// Which operation it is about.
    pub agent_operation_identifier: String,
    /// What happened.
    pub kind: JobEventKind,
    /// Where this event sits in the operation's own sequence.
    pub sequence: u64,
}

/// What a daemon may conclude from one event.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EventVerdict {
    /// It describes this operation now, and may be applied.
    Applies,
    /// It is older than what this daemon already has.
    Superseded,
    /// It describes an incarnation of the store that no longer exists.
    AnotherGeneration,
    /// It describes an operation this daemon does not hold.
    AnotherOperation,
}

/// What a daemon already knows about one operation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ObservedJob {
    /// Which incarnation of the store this daemon is following.
    pub agent_event_store_generation: u64,
    /// Which operation it is following.
    pub agent_operation_identifier: String,
    /// The highest sequence it has applied.
    pub applied_sequence: u64,
}

impl ObservedJob {
    /// Returns what this daemon may conclude from `event`.
    ///
    /// Generation before operation before sequence. A daemon that compared
    /// sequences first could apply an event from a rebuilt store because its
    /// number happened to be higher, which is how a stream from a store that no
    /// longer exists gets treated as news.
    #[must_use]
    pub fn verdict(&self, event: &JobEvent) -> EventVerdict {
        if event.agent_event_store_generation != self.agent_event_store_generation {
            return EventVerdict::AnotherGeneration;
        }
        if event.agent_operation_identifier != self.agent_operation_identifier {
            return EventVerdict::AnotherOperation;
        }
        if event.sequence <= self.applied_sequence {
            return EventVerdict::Superseded;
        }
        EventVerdict::Applies
    }

    /// Returns this observation with `event` applied, when it applies.
    #[must_use]
    pub fn applying(&self, event: &JobEvent) -> Self {
        if self.verdict(event) == EventVerdict::Applies {
            Self { applied_sequence: event.sequence, ..self.clone() }
        } else {
            self.clone()
        }
    }
}

/// What the agent's snapshot says, which is the record rather than the news.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct JobSnapshot {
    /// Which incarnation of the store it came from.
    pub agent_event_store_generation: u64,
    /// Which operation it is about.
    pub agent_operation_identifier: String,
    /// The last thing that happened, as the store holds it.
    pub kind: JobEventKind,
    /// The highest sequence the store holds.
    pub sequence: u64,
}

impl JobSnapshot {
    /// Returns whether this snapshot reconciles with what a daemon observed.
    ///
    /// A snapshot ahead of the daemon is ordinary: events were missed. A
    /// snapshot behind it is not, and is worth refusing rather than
    /// rationalising - it means the daemon applied something the store does not
    /// have, which is a disagreement about what happened rather than a gap.
    #[must_use]
    pub fn reconciles_with(&self, observed: &ObservedJob) -> bool {
        self.agent_event_store_generation == observed.agent_event_store_generation
            && self.agent_operation_identifier == observed.agent_operation_identifier
            && self.sequence >= observed.applied_sequence
    }
}
