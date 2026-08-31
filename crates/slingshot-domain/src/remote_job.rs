//! What this daemon durably knows about work running somewhere else.
//!
//! The vocabulary sits inward of both the wire and storage on purpose. Storage
//! persists these values and the protocols convert into them, so the question
//! "what state is this job in" has exactly one answer that neither a wire
//! document nor a table row gets to redefine.
//!
//! # Physically many, logically one
//!
//! Sling delivers at least once, so one logical operation can be carried by
//! several physical jobs: a requeue, a retry after a node went away, a second
//! record for the same submission. Every one of those is the same work, and
//! none of them is a reason to say the work went back to being queued. So the
//! transition table admits no path out of Running except an ending, and a
//! physical retry shows up as monotonic attempt and progress metadata beside a
//! state that does not move.
//!
//! # Endings are final
//!
//! Succeeded and Failed accept nothing but themselves. An event that would move
//! a job out of an ending is either a replay from before it ended or an account
//! this daemon cannot reconcile, and both are better handled by refusing than
//! by letting the most recently delivered packet win.

use crate::author_agent_transport_contract::AuthorAgentTransportContract;

/// Where one job's own events start.
pub const FIRST_SEQUENCE: u64 = 1;

/// What one job's attempt and progress counters start at.
pub const NO_ATTEMPTS_YET: u64 = 0;

/// Why one remote-job value cannot be built or advanced.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum RemoteJobFailure {
    /// A job identifier with nothing in it names no job.
    #[error("a job identifier names one job, and an empty one names none")]
    IdentifierEmpty,
    /// The identifier is longer than the contract allows.
    #[error("a job identifier holds at most {allowed} bytes, and this holds {actual}")]
    IdentifierTooLong {
        /// How long one may be.
        allowed: u64,
        /// How long this is.
        actual: usize,
    },
    /// The cursor is longer than the contract allows.
    #[error("a stream cursor holds at most {allowed} bytes, and this holds {actual}")]
    CursorTooLong {
        /// How long one may be.
        allowed: u64,
        /// How long this is.
        actual: usize,
    },
    /// Something tried to move a job out of an ending.
    #[error("a job that {from} stays {from}, and this account says {to}")]
    EndingIsFinal {
        /// What it ended as.
        from: AgentJobState,
        /// What the account says.
        to: AgentJobState,
    },
    /// Something tried to put a running job back in the queue.
    #[error("a physical retry is the same work running, not work waiting to run again")]
    RunningCannotRequeue,
    /// An attempt count went backwards.
    #[error("attempts only ever increase, and this went from {held} to {named}")]
    AttemptRegressed {
        /// What was held.
        held: u64,
        /// What arrived.
        named: u64,
    },
    /// A progress reading went backwards.
    #[error("progress only ever increases, and this went from {held} to {named}")]
    ProgressRegressed {
        /// What was held.
        held: u64,
        /// What arrived.
        named: u64,
    },
}

/// One physical Sling job, as it is named durably.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct AgentJobIdentifier {
    /// The name the agent gave it.
    spelling: String,
}

impl AgentJobIdentifier {
    /// Returns the identifier `spelling` names, if it is one.
    ///
    /// # Errors
    ///
    /// Returns [`RemoteJobFailure::IdentifierEmpty`] or
    /// [`RemoteJobFailure::IdentifierTooLong`].
    pub fn new(spelling: &str) -> Result<Self, RemoteJobFailure> {
        if spelling.is_empty() {
            return Err(RemoteJobFailure::IdentifierEmpty);
        }
        let allowed =
            AuthorAgentTransportContract::embedded().limit("maximum_sling_job_identifier_bytes");
        if u64::try_from(spelling.len()).unwrap_or(u64::MAX) > allowed {
            return Err(RemoteJobFailure::IdentifierTooLong { allowed, actual: spelling.len() });
        }
        Ok(Self { spelling: spelling.to_owned() })
    }

    /// Returns this identifier's spelling.
    #[must_use]
    pub fn as_text(&self) -> &str {
        &self.spelling
    }
}

/// Where one subscription's stream has durably got to.
///
/// A different value from the one the wire carries, and deliberately so: a wire
/// cursor is something an agent said, and this is something this daemon wrote
/// down. Only the second may be resumed from.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct EventStreamCursor {
    /// The bytes the agent issued, unread.
    spelling: String,
}

impl EventStreamCursor {
    /// Returns the cursor `spelling` names, if it fits.
    ///
    /// # Errors
    ///
    /// Returns [`RemoteJobFailure::CursorTooLong`].
    pub fn new(spelling: &str) -> Result<Self, RemoteJobFailure> {
        let allowed = AuthorAgentTransportContract::embedded()
            .limit("maximum_agent_operation_identifier_bytes");
        if u64::try_from(spelling.len()).unwrap_or(u64::MAX) > allowed {
            return Err(RemoteJobFailure::CursorTooLong { allowed, actual: spelling.len() });
        }
        Ok(Self { spelling: spelling.to_owned() })
    }

    /// Returns this cursor's bytes.
    #[must_use]
    pub fn as_text(&self) -> &str {
        &self.spelling
    }
}

/// Where one job's own events have got to.
///
/// Per job, not per subscription. One stream carries several jobs, so a
/// subscription's cursor and a job's sequence advance at different rates and
/// deriving either from the other would resume at a position nobody issued.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct JobEventSequence {
    /// Which event this is, in that job's own count.
    value: u64,
}

impl JobEventSequence {
    /// Returns the sequence one job's first event carries.
    #[must_use]
    pub fn first() -> Self {
        Self { value: FIRST_SEQUENCE }
    }

    /// Returns the sequence `value` names.
    #[must_use]
    pub fn of(value: u64) -> Self {
        Self { value }
    }

    /// Returns this sequence as a number.
    #[must_use]
    pub fn value(self) -> u64 {
        self.value
    }

    /// Returns whether this sequence comes after `held`.
    #[must_use]
    pub fn follows(self, held: Self) -> bool {
        self.value > held.value
    }

    /// Returns whether this sequence is the very next one after `held`.
    ///
    /// The distinction from [`Self::follows`] is the whole reason gaps are
    /// visible: an event that follows but is not next means something in
    /// between was missed, and the only honest response is to go and look.
    #[must_use]
    pub fn immediately_follows(self, held: Self) -> bool {
        self.value == held.value.saturating_add(FIRST_SEQUENCE)
    }
}

/// What one remote job is, as far as durable state goes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AgentJobState {
    /// Accepted, and not started.
    Queued,
    /// Started, however many physical jobs have carried it.
    Running,
    /// Finished, with a result.
    Succeeded,
    /// Finished, without one.
    Failed,
}

impl ::core::fmt::Display for AgentJobState {
    fn fmt(&self, formatter: &mut ::core::fmt::Formatter<'_>) -> ::core::fmt::Result {
        formatter.write_str(match self {
            Self::Queued => "queued",
            Self::Running => "running",
            Self::Succeeded => "succeeded",
            Self::Failed => "failed",
        })
    }
}

impl AgentJobState {
    /// Every state a job can be in, in the order it can reach them.
    pub const ALL: &'static [Self] = &[Self::Queued, Self::Running, Self::Succeeded, Self::Failed];

    /// Returns whether this state ends the job.
    #[must_use]
    pub fn is_terminal(self) -> bool {
        matches!(self, Self::Succeeded | Self::Failed)
    }

    /// Returns whether a job in this state may be said to be in `next`.
    ///
    /// The whole allowed table, in one place. Repeating a state is always
    /// allowed because replay is ordinary; leaving an ending never is; and
    /// Running does not return to Queued, because a physical requeue is the
    /// same work being carried again rather than work that stopped.
    #[must_use]
    pub fn may_become(self, next: Self) -> bool {
        match self {
            Self::Queued => true,
            Self::Running => !matches!(next, Self::Queued),
            Self::Succeeded | Self::Failed => self == next,
        }
    }
}

/// What this daemon durably knows about one remote job.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RemoteJobObservation {
    /// How far into that job's own events this has been applied.
    pub applied_sequence: JobEventSequence,
    /// How many physical attempts have carried it.
    pub attempt: u64,
    /// How far the agent last said it had got.
    pub progress: u64,
    /// What it is.
    pub state: AgentJobState,
}

impl RemoteJobObservation {
    /// Returns what is known about a job that has only just been accepted.
    #[must_use]
    pub fn accepted() -> Self {
        Self {
            applied_sequence: JobEventSequence::first(),
            attempt: NO_ATTEMPTS_YET,
            progress: NO_ATTEMPTS_YET,
            state: AgentJobState::Queued,
        }
    }

    /// Requires one account of this job to be one that can be believed.
    ///
    /// Only the parts an account is allowed to change. Whether the account is
    /// timely, or about the right job at all, is somebody else's question:
    /// this answers whether the values themselves are consistent with what is
    /// already held.
    ///
    /// # Errors
    ///
    /// Returns [`RemoteJobFailure::EndingIsFinal`],
    /// [`RemoteJobFailure::RunningCannotRequeue`],
    /// [`RemoteJobFailure::AttemptRegressed`], or
    /// [`RemoteJobFailure::ProgressRegressed`].
    pub fn require_advanceable(
        &self,
        state: AgentJobState,
        attempt: u64,
        progress: u64,
    ) -> Result<(), RemoteJobFailure> {
        if self.state.is_terminal() && self.state != state {
            return Err(RemoteJobFailure::EndingIsFinal { from: self.state, to: state });
        }
        if !self.state.may_become(state) {
            return Err(RemoteJobFailure::RunningCannotRequeue);
        }
        if attempt < self.attempt {
            return Err(RemoteJobFailure::AttemptRegressed { held: self.attempt, named: attempt });
        }
        if progress < self.progress {
            return Err(RemoteJobFailure::ProgressRegressed {
                held: self.progress,
                named: progress,
            });
        }
        Ok(())
    }

    /// Returns what this becomes when one believable account is applied.
    ///
    /// # Errors
    ///
    /// Returns whatever [`Self::require_advanceable`] returns.
    pub fn advanced(
        &self,
        state: AgentJobState,
        sequence: JobEventSequence,
        attempt: u64,
        progress: u64,
    ) -> Result<Self, RemoteJobFailure> {
        self.require_advanceable(state, attempt, progress)?;
        Ok(Self { applied_sequence: sequence, attempt, progress, state })
    }
}
