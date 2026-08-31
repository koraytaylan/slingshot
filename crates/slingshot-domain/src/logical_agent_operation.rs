//! One logical command, and the physical records a queue makes of it.
//!
//! Sling delivers at least once. One submitted command can become several job
//! records and several attempts, and no amount of care on this side prevents
//! that - it is a property of the queue, not a defect in the caller. What must
//! never happen twice is the effect, and that is what this models.
//!
//! The design turns on one transition. Exactly one compare-and-set crosses from
//! not-started to started, and whoever wins it owns the work. A holder that
//! then loses its lease does not release the work: it simply stops being able
//! to record anything. The tempting alternative - letting a new holder take
//! over unfinished work - is precisely how one logical command becomes two
//! remote effects, because the first holder may still be running.
//!
//! An attempt count is bounded, and the bound is not an optimisation. An
//! unbounded retry against a remote system that is failing is a way of turning
//! one problem into a larger one.

use crate::author_agent_transport_contract::AuthorAgentTransportContract;

/// How far one logical operation has got.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum LogicalExecutionState {
    /// Recorded, and nobody has begun.
    ExecutionNotStarted,
    /// Somebody won the transition and owns the work.
    ExecutionStarted,
    /// The effect happened, exactly once.
    Effected,
}

/// Why a logical operation refused a transition.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum LogicalExecutionFailure {
    /// Somebody already crossed the transition.
    #[error("this operation has already started, and starting is not a thing that happens twice")]
    AlreadyStarted,
    /// The caller is not the holder.
    #[error("this caller does not hold this operation; the work is still the previous holder's")]
    NotTheHolder,
    /// The operation has not started.
    #[error("this operation has not started, so there is nothing to record against it")]
    NotStarted,
    /// The attempts allowed have all been made.
    #[error("this operation has made the {allowed} attempts it may")]
    AttemptsExhausted {
        /// How many it may make.
        allowed: u64,
    },
    /// More physical records than one operation may have.
    #[error("this operation has {found} physical records, and at most {allowed} are matched")]
    TooManyRecords {
        /// How many may be matched.
        allowed: u64,
        /// How many were found.
        found: usize,
    },
}

/// One logical operation, as the daemon holds it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LogicalAgentOperation {
    /// How many attempts have been made.
    pub attempts: u64,
    /// Which lease holds the work, once one does.
    pub holder: Option<u64>,
    /// The physical Sling job records that belong to it, in a fixed order.
    pub physical_records: Vec<String>,
    /// How far it has got.
    pub state: LogicalExecutionState,
}

impl LogicalAgentOperation {
    /// Returns an operation nobody has begun.
    #[must_use]
    pub fn recorded() -> Self {
        Self {
            attempts: 0,
            holder: None,
            physical_records: Vec::new(),
            state: LogicalExecutionState::ExecutionNotStarted,
        }
    }

    /// Returns how many attempts one operation may make.
    #[must_use]
    pub fn maximum_attempts() -> u64 {
        AuthorAgentTransportContract::embedded().limit("maximum_logical_outbox_attempts")
    }

    /// Returns how many physical records one operation may have matched.
    #[must_use]
    pub fn maximum_records() -> u64 {
        AuthorAgentTransportContract::embedded().limit("maximum_physical_sling_job_matches")
    }

    /// Records one physical Sling job, ignoring one already recorded.
    ///
    /// Duplicates are the normal case rather than an error, so the set is what
    /// matters and a repeated delivery changes nothing.
    ///
    /// # Errors
    ///
    /// Returns [`LogicalExecutionFailure::TooManyRecords`] past the bound.
    pub fn physical_record(
        &mut self,
        sling_job_identifier: &str,
    ) -> Result<(), LogicalExecutionFailure> {
        if self.physical_records.iter().any(|held| held == sling_job_identifier) {
            return Ok(());
        }
        let allowed = Self::maximum_records();
        if u64::try_from(self.physical_records.len()).unwrap_or(u64::MAX) >= allowed {
            return Err(LogicalExecutionFailure::TooManyRecords {
                allowed,
                found: self.physical_records.len() + 1,
            });
        }
        self.physical_records.push(sling_job_identifier.to_owned());
        self.physical_records.sort();
        Ok(())
    }

    /// Crosses from not-started to started, if nobody has.
    ///
    /// # Errors
    ///
    /// Returns [`LogicalExecutionFailure::AlreadyStarted`] or
    /// [`LogicalExecutionFailure::AttemptsExhausted`].
    pub fn start(&mut self, lease: u64) -> Result<(), LogicalExecutionFailure> {
        if self.state != LogicalExecutionState::ExecutionNotStarted {
            return Err(LogicalExecutionFailure::AlreadyStarted);
        }
        let allowed = Self::maximum_attempts();
        if self.attempts >= allowed {
            return Err(LogicalExecutionFailure::AttemptsExhausted { allowed });
        }
        self.attempts += 1;
        self.holder = Some(lease);
        self.state = LogicalExecutionState::ExecutionStarted;
        Ok(())
    }

    /// Records the effect, if this caller is the holder.
    ///
    /// # Errors
    ///
    /// Returns [`LogicalExecutionFailure::NotTheHolder`] or
    /// [`LogicalExecutionFailure::NotStarted`].
    pub fn effect(&mut self, lease: u64) -> Result<(), LogicalExecutionFailure> {
        if self.state != LogicalExecutionState::ExecutionStarted {
            return Err(LogicalExecutionFailure::NotStarted);
        }
        if self.holder != Some(lease) {
            return Err(LogicalExecutionFailure::NotTheHolder);
        }
        self.state = LogicalExecutionState::Effected;
        Ok(())
    }

    /// Returns whether a crash here leaves an operation something can act on.
    ///
    /// Every state does, which is the point of there being three of them.
    /// Not-started may be started by anyone; started belongs to its holder and
    /// to nobody else, whatever happened to that holder; effected is finished.
    /// There is no fourth state in which the right thing to do is unclear.
    #[must_use]
    pub fn is_recoverable(&self) -> bool {
        match self.state {
            LogicalExecutionState::ExecutionNotStarted => self.holder.is_none(),
            LogicalExecutionState::ExecutionStarted | LogicalExecutionState::Effected => {
                self.holder.is_some()
            }
        }
    }
}
