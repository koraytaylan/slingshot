//! Waiting on one operation, without a waiter ever affecting the work.
//!
//! A wait is a subscription to persisted revisions, and the rule that shapes
//! everything else is that watching must be free. A slow waiter, a waiter that
//! stopped reading, a waiter that disconnected, and the maximum number of
//! waiters at once all have to leave the operation exactly as it would have
//! been with nobody watching. Anything else would make a client able to affect
//! work by observing it.
//!
//! A waiter registers with the revision it last saw and is told immediately if
//! the row has already moved past it. That removes the worst race in the whole
//! interface: a client that read a status, decided to wait, and would otherwise
//! block forever because the thing it was waiting for happened in between.
//!
//! Each waiter has its own bounded queue. Progress can be coalesced when it is
//! superseded, because a note nobody read yet is worth less than the current
//! one; a recovery, a resume, and a terminal fact never are, because those are
//! the things a client is waiting to hear. So a queue under pressure loses
//! detail rather than losing the answer.
//!
//! Time detaches nobody. A wait has no read deadline once it is attached, and
//! advancing a clock cannot turn pending work into failed work: an application
//! deciding it has waited long enough is a fact about the client, and a daemon
//! that converted it into an operation failure would be inventing an outcome.

use std::collections::VecDeque;

use slingshot_domain::daemon_runtime_contract::DaemonRuntimeContract;

/// What a waiter is told about the operation it is watching.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WaitUpdate {
    /// A bounded description of what is happening.
    Progress {
        /// What the note says.
        detail: String,
        /// The revision it was recorded at.
        revision: u64,
    },
    /// The operation is waiting on something.
    RecoveryRequired {
        /// The revision it was recorded at.
        revision: u64,
    },
    /// A person resumed it.
    Resumed {
        /// The revision the resume committed.
        revision: u64,
    },
    /// The operation ended.
    Terminal {
        /// The revision it ended at.
        revision: u64,
    },
}

impl WaitUpdate {
    /// Returns the revision this update describes.
    #[must_use]
    pub fn revision(&self) -> u64 {
        match self {
            Self::Progress { revision, .. }
            | Self::RecoveryRequired { revision }
            | Self::Resumed { revision }
            | Self::Terminal { revision } => *revision,
        }
    }

    /// Returns whether a later update may replace this one in a full queue.
    ///
    /// Progress may: a note nobody has read is worth less than the current one.
    /// Nothing else may, because a recovery, a resume, and an ending are what a
    /// client is waiting to hear, and dropping one would answer a different
    /// question than the one it asked.
    #[must_use]
    pub fn is_supersedable(&self) -> bool {
        matches!(self, Self::Progress { .. })
    }

    /// Returns whether this update ends the wait.
    #[must_use]
    pub fn ends_the_wait(&self) -> bool {
        matches!(self, Self::Terminal { .. })
    }
}

/// Why a waiter could not be attached, or was detached.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum WaitRefusal {
    /// This operation already has every waiter it may.
    #[error("this operation has {held} waiters and may have {limit}")]
    WaitersExhausted {
        /// How many it has.
        held: u64,
        /// How many it may have.
        limit: u64,
    },
}

/// Why one waiter stopped waiting.
///
/// Every reason is about the waiter. None of them is about the operation,
/// which is the property this enum exists to make visible.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Detachment {
    /// The client asked to stop.
    Cancelled,
    /// The transport went away.
    Disconnected,
    /// The daemon is stopping.
    DaemonStopping,
    /// The client stopped reading and the write deadline elapsed.
    WriteDeadlineElapsed,
}

/// One client waiting on one operation.
#[derive(Debug)]
pub struct Waiter {
    /// Which waiter this is, so a caller can detach exactly one.
    ticket: u64,
    /// Updates queued for it, oldest first.
    queued: VecDeque<WaitUpdate>,
    /// The highest revision it has been given.
    delivered_revision: u64,
}

impl Waiter {
    /// Returns which waiter this is.
    #[must_use]
    pub fn ticket(&self) -> u64 {
        self.ticket
    }

    /// Returns how many updates are queued for this waiter.
    #[must_use]
    pub fn queued(&self) -> usize {
        self.queued.len()
    }

    /// Takes the next update this waiter should be given.
    pub fn take(&mut self) -> Option<WaitUpdate> {
        let update = self.queued.pop_front()?;
        self.delivered_revision = self.delivered_revision.max(update.revision());
        Some(update)
    }

    /// Returns the highest revision this waiter has been given.
    #[must_use]
    pub fn delivered_revision(&self) -> u64 {
        self.delivered_revision
    }
}

/// The bounds one operation's waiters are held to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WaitBounds {
    /// Updates one waiter's queue may hold.
    pub queue_updates: u64,
    /// Waiters one operation may have.
    pub waiters_per_operation: u64,
}

impl WaitBounds {
    /// Returns the bounds the embedded runtime contract names.
    #[must_use]
    pub fn embedded() -> Self {
        let contract = DaemonRuntimeContract::embedded();
        Self {
            queue_updates: contract.limit("maximum_waiter_queue_updates"),
            waiters_per_operation: contract.limit("maximum_waiters_per_operation"),
        }
    }
}

/// Everyone waiting on one operation.
#[derive(Debug)]
pub struct WaiterRegistry {
    /// The bounds this registry is held to.
    bounds: WaitBounds,
    /// The next waiter's ticket.
    next_ticket: u64,
    /// The highest revision this registry has broadcast.
    published_revision: u64,
    /// Everyone attached, in the order they attached.
    waiters: Vec<Waiter>,
}

impl WaiterRegistry {
    /// Returns an empty registry for an operation at `revision`.
    #[must_use]
    pub fn new(bounds: WaitBounds, revision: u64) -> Self {
        Self { bounds, next_ticket: 1, published_revision: revision, waiters: Vec::new() }
    }

    /// Returns how many waiters are attached.
    #[must_use]
    pub fn attached(&self) -> usize {
        self.waiters.len()
    }

    /// Returns the highest revision this registry has broadcast.
    #[must_use]
    pub fn published_revision(&self) -> u64 {
        self.published_revision
    }

    /// Attaches one waiter that last saw `observed_revision`.
    ///
    /// A waiter that is already behind is given the current state immediately
    /// rather than made to wait for the next event. That closes the race a
    /// client cannot otherwise avoid: reading a status, deciding to wait, and
    /// blocking forever because what it was waiting for happened in between.
    ///
    /// # Errors
    ///
    /// Returns [`WaitRefusal::WaitersExhausted`], which changes nothing about
    /// the operation - a client that cannot watch has not affected the work.
    pub fn attach(
        &mut self,
        observed_revision: u64,
        current: Option<WaitUpdate>,
    ) -> Result<u64, WaitRefusal> {
        let held = u64::try_from(self.waiters.len()).unwrap_or(u64::MAX);
        if held >= self.bounds.waiters_per_operation {
            return Err(WaitRefusal::WaitersExhausted {
                held,
                limit: self.bounds.waiters_per_operation,
            });
        }
        let ticket = self.next_ticket;
        self.next_ticket = self.next_ticket.saturating_add(1);
        let mut queued = VecDeque::new();
        if let Some(update) = current
            && update.revision() > observed_revision
        {
            queued.push_back(update);
        }
        self.waiters.push(Waiter { ticket, queued, delivered_revision: observed_revision });
        Ok(ticket)
    }

    /// Broadcasts one update to everyone attached.
    ///
    /// A revision that is not newer than the last one broadcast is dropped, so
    /// what every waiter sees is strictly increasing however the caller stitches
    /// its own reads together.
    pub fn publish(&mut self, update: &WaitUpdate) {
        if update.revision() <= self.published_revision {
            return;
        }
        self.published_revision = update.revision();
        let allowed = usize::try_from(self.bounds.queue_updates).unwrap_or(usize::MAX);
        for waiter in &mut self.waiters {
            enqueue(waiter, update.clone(), allowed);
        }
    }

    /// Detaches one waiter, and reports whether it was there.
    ///
    /// Nothing about the operation changes. That is the whole contract of this
    /// method, and the reason the reason for detaching is not consulted here.
    pub fn detach(&mut self, ticket: u64, _reason: Detachment) -> bool {
        let before = self.waiters.len();
        self.waiters.retain(|waiter| waiter.ticket != ticket);
        self.waiters.len() != before
    }

    /// Returns one attached waiter.
    #[must_use]
    pub fn waiter(&self, ticket: u64) -> Option<&Waiter> {
        self.waiters.iter().find(|waiter| waiter.ticket == ticket)
    }

    /// Takes the next update one waiter should be given.
    pub fn take(&mut self, ticket: u64) -> Option<WaitUpdate> {
        self.waiters.iter_mut().find(|waiter| waiter.ticket == ticket).and_then(Waiter::take)
    }
}

/// Queues one update for one waiter, within its bound.
///
/// A full queue drops the oldest superseded progress note to make room. When
/// nothing is superseded - every queued update is a recovery, a resume, or an
/// ending - the new update is dropped instead, because those are the ones a
/// client is waiting to hear and the oldest of them is not the least useful.
fn enqueue(waiter: &mut Waiter, update: WaitUpdate, allowed: usize) {
    if waiter.queued.len() < allowed {
        waiter.queued.push_back(update);
        return;
    }
    let supersedable = waiter.queued.iter().position(WaitUpdate::is_supersedable);
    match supersedable {
        Some(index) => {
            waiter.queued.remove(index);
            waiter.queued.push_back(update);
        }
        None if update.ends_the_wait() => {
            waiter.queued.pop_front();
            waiter.queued.push_back(update);
        }
        None => (),
    }
}
