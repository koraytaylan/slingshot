//! One logical command, however many physical records Sling makes of it.
//!
//! Sling delivers at least once, so one submitted command can become several
//! job records and several attempts. That is not a bug to be prevented; it is
//! the property the whole design has to survive. What must never happen twice
//! is the effect.
//!
//! One compare-and-set crosses from not-started to started, and only the caller
//! that wins it may act. A lease lost after that transition never permits a
//! second effect: the work is already someone's, and the fact that the winner
//! stopped answering does not make it available again. The alternative -
//! letting a new holder take over unfinished work - is exactly how one logical
//! command becomes two remote effects.

use std::collections::BTreeSet;
use std::sync::Mutex;

/// How far one logical operation has got.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum ExecutionState {
    /// Recorded, and nobody has begun.
    NotStarted,
    /// Somebody won the transition and is acting.
    Started,
    /// The effect happened.
    Effected,
}

/// Why a transition was refused.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum OutboxRefusal {
    /// Somebody else already crossed this transition.
    #[error("this operation has already started, and starting is not a thing that happens twice")]
    AlreadyStarted,
    /// The caller's lease was taken.
    #[error("this caller's lease was taken; the work is still the previous holder's")]
    Fenced,
    /// The physical records do not match what the postcheck expected.
    #[error("this operation has {found} physical records and the postcheck expected {expected}")]
    RecordsDisagree {
        /// How many the postcheck expected.
        expected: usize,
        /// How many were found.
        found: usize,
    },
}

/// One logical operation and the physical records Sling made of it.
#[derive(Debug)]
pub struct LogicalOperation {
    /// The physical job records that belong to it.
    records: Mutex<BTreeSet<String>>,
    /// How far it has got.
    state: Mutex<ExecutionState>,
    /// Which lease holds it, once one does.
    holder: Mutex<Option<u64>>,
}

impl LogicalOperation {
    /// Returns an operation nobody has begun.
    #[must_use]
    pub fn recorded() -> Self {
        Self {
            records: Mutex::new(BTreeSet::new()),
            state: Mutex::new(ExecutionState::NotStarted),
            holder: Mutex::new(None),
        }
    }

    /// Records one physical Sling job for this logical operation.
    ///
    /// Several are expected. A duplicate delivery is the normal case, not an
    /// error, and the set is what the postcheck later reconciles.
    pub fn physical_record(&self, sling_job_identifier: &str) {
        if let Ok(mut held) = self.records.lock() {
            held.insert(sling_job_identifier.to_owned());
        }
    }

    /// Returns how many physical records this operation has.
    #[must_use]
    pub fn physical_records(&self) -> usize {
        self.records.lock().map(|held| held.len()).unwrap_or_default()
    }

    /// Returns how far this operation has got.
    #[must_use]
    pub fn state(&self) -> ExecutionState {
        self.state.lock().map(|held| *held).unwrap_or(ExecutionState::NotStarted)
    }

    /// Crosses from not-started to started, if nobody has.
    ///
    /// The one transition that decides who may act. Everything after it is that
    /// caller's, and a caller that loses its lease afterwards does not release
    /// the work - it simply stops being able to write.
    ///
    /// # Errors
    ///
    /// Returns [`OutboxRefusal::AlreadyStarted`].
    pub fn start(&self, lease: u64) -> Result<(), OutboxRefusal> {
        let mut state = self.state.lock().map_err(|_| OutboxRefusal::AlreadyStarted)?;
        if *state != ExecutionState::NotStarted {
            return Err(OutboxRefusal::AlreadyStarted);
        }
        *state = ExecutionState::Started;
        if let Ok(mut holder) = self.holder.lock() {
            *holder = Some(lease);
        }
        Ok(())
    }

    /// Records the effect, if this caller is the one that started.
    ///
    /// # Errors
    ///
    /// Returns [`OutboxRefusal::Fenced`] for any caller but the holder, and
    /// [`OutboxRefusal::AlreadyStarted`] when nobody started at all.
    pub fn effect(&self, lease: u64) -> Result<(), OutboxRefusal> {
        let holder = self.holder.lock().map_err(|_| OutboxRefusal::Fenced)?;
        if *holder != Some(lease) {
            return Err(OutboxRefusal::Fenced);
        }
        let mut state = self.state.lock().map_err(|_| OutboxRefusal::Fenced)?;
        if *state != ExecutionState::Started {
            return Err(OutboxRefusal::AlreadyStarted);
        }
        *state = ExecutionState::Effected;
        Ok(())
    }

    /// Requires the physical records to be the number the postcheck expected.
    ///
    /// Fails closed on disagreement rather than guessing. More records than
    /// expected may mean a delivery this daemon does not know about, and fewer
    /// may mean one it recorded and lost - neither is a state to act on.
    ///
    /// # Errors
    ///
    /// Returns [`OutboxRefusal::RecordsDisagree`].
    pub fn require_records(&self, expected: usize) -> Result<(), OutboxRefusal> {
        let found = self.physical_records();
        if found == expected {
            Ok(())
        } else {
            Err(OutboxRefusal::RecordsDisagree { expected, found })
        }
    }
}
