//! Stopping a daemon without abandoning what it was doing.
//!
//! Stopping has three parts in one order, and the order is what makes it
//! graceful rather than abrupt. New work is refused first, so nothing arrives
//! that will not be finished. Work already running is left to finish and
//! waiters are left attached, because a client that asked to stop a daemon did
//! not ask to fail the operations it was running. Only then does the daemon
//! withdraw readiness, close its listener, and go.
//!
//! Stopping is authorized by the current instance nonce alone. A process
//! identifier proves nothing - the operating system reuses them - and a nonce
//! a previous instance published proves only that a previous instance existed.
//! So a stale nonce cannot stop a replacement even when everything else about
//! the two looks identical, which is exactly the situation that arises when a
//! daemon is restarted while an old client is still holding what it learned.

use std::collections::BTreeSet;

/// How far a stop has got.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum ShutdownPhase {
    /// Serving normally.
    Running,
    /// Taking no new work; what is running continues.
    Draining,
    /// Nothing is left running, and readiness has been withdrawn.
    Withdrawn,
    /// The listener is closed and the process may go.
    Closed,
}

/// Why a stop was refused.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum StopRefusal {
    /// The nonce is not this instance's.
    #[error("that nonce belongs to another daemon instance, and this one is still running")]
    StaleInstance,
}

/// One daemon's progress towards stopping.
#[derive(Debug)]
pub struct Shutdown {
    /// The nonce that authorizes stopping this instance.
    instance_nonce: String,
    /// What is still running.
    outstanding: BTreeSet<String>,
    /// How far the stop has got.
    phase: ShutdownPhase,
}

impl Shutdown {
    /// Returns a daemon that is running and has stopped nothing.
    #[must_use]
    pub fn running(instance_nonce: &str) -> Self {
        Self {
            instance_nonce: instance_nonce.to_owned(),
            outstanding: BTreeSet::new(),
            phase: ShutdownPhase::Running,
        }
    }

    /// Returns how far the stop has got.
    #[must_use]
    pub fn phase(&self) -> ShutdownPhase {
        self.phase
    }

    /// Returns how much work is still running.
    #[must_use]
    pub fn outstanding(&self) -> usize {
        self.outstanding.len()
    }

    /// Records that one operation has started running.
    pub fn started(&mut self, operation_identifier: &str) {
        self.outstanding.insert(operation_identifier.to_owned());
    }

    /// Records that one operation has finished, and advances if that was the last.
    pub fn finished(&mut self, operation_identifier: &str) {
        self.outstanding.remove(operation_identifier);
        if self.phase == ShutdownPhase::Draining && self.outstanding.is_empty() {
            self.phase = ShutdownPhase::Withdrawn;
        }
    }

    /// Begins stopping, if `supplied_nonce` authorizes it.
    ///
    /// Acknowledged before anything is torn down, so a client learns its
    /// request was accepted rather than losing its connection and having to
    /// guess. Work already running is untouched: stopping is not cancelling.
    ///
    /// # Errors
    ///
    /// Returns [`StopRefusal::StaleInstance`] for any nonce but this
    /// instance's, and changes nothing.
    pub fn begin(&mut self, supplied_nonce: &str) -> Result<ShutdownPhase, StopRefusal> {
        if supplied_nonce != self.instance_nonce {
            return Err(StopRefusal::StaleInstance);
        }
        if self.phase == ShutdownPhase::Running {
            self.phase = if self.outstanding.is_empty() {
                ShutdownPhase::Withdrawn
            } else {
                ShutdownPhase::Draining
            };
        }
        Ok(self.phase)
    }

    /// Closes the listener, once nothing is left running.
    ///
    /// Returns whether it closed. A daemon still draining is not refused here -
    /// it is simply not finished, and the caller comes back.
    pub fn close(&mut self) -> bool {
        if self.phase == ShutdownPhase::Withdrawn {
            self.phase = ShutdownPhase::Closed;
            return true;
        }
        false
    }

    /// Returns whether this daemon still takes new work.
    #[must_use]
    pub fn takes_new_work(&self) -> bool {
        self.phase == ShutdownPhase::Running
    }
}
