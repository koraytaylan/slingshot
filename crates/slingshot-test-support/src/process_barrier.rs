//! Making several clients arrive at one moment, on purpose.
//!
//! A concurrency test that starts its clients in a loop is not testing
//! concurrency: the first one has usually finished before the last one starts,
//! and the interesting interleavings never happen. A barrier makes every
//! participant wait until all of them are ready, so they contend for real.
//!
//! Waiting is bounded. A barrier that could block forever would turn one stuck
//! participant into a suite that never finishes and never says why, so the wait
//! has a deadline and reports that it elapsed rather than hanging.

use std::sync::{Arc, Condvar, Mutex};
use std::time::{Duration, Instant};

/// What waiting at a barrier produced.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Arrival {
    /// Everyone arrived, and this participant may go.
    Released,
    /// Somebody did not arrive in time.
    DeadlineElapsed,
}

/// How many participants have arrived, and how many are expected.
#[derive(Debug)]
struct Attendance {
    /// How many have arrived.
    arrived: usize,
    /// How many are expected.
    expected: usize,
}

/// A point several participants reach before any of them goes on.
#[derive(Debug, Clone)]
pub struct ProcessBarrier {
    /// The attendance, and the signal that it changed.
    attendance: Arc<(Mutex<Attendance>, Condvar)>,
}

impl ProcessBarrier {
    /// Returns a barrier that releases once `expected` participants arrive.
    #[must_use]
    pub fn expecting(expected: usize) -> Self {
        Self {
            attendance: Arc::new((Mutex::new(Attendance { arrived: 0, expected }), Condvar::new())),
        }
    }

    /// Waits until everyone has arrived, or until `deadline` elapses.
    ///
    /// The deadline is measured from this participant's own arrival, so a late
    /// arrival does not get less time than an early one - which would make the
    /// test's outcome depend on scheduling, exactly what a barrier exists to
    /// remove.
    pub fn arrive(&self, deadline: Duration) -> Arrival {
        let (attendance, changed) = &*self.attendance;
        let started = Instant::now();
        let Ok(mut held) = attendance.lock() else {
            return Arrival::DeadlineElapsed;
        };
        held.arrived += 1;
        if held.arrived >= held.expected {
            changed.notify_all();
            return Arrival::Released;
        }
        while held.arrived < held.expected {
            let remaining = deadline.saturating_sub(started.elapsed());
            if remaining.is_zero() {
                return Arrival::DeadlineElapsed;
            }
            let Ok((waited, timed_out)) = changed.wait_timeout(held, remaining) else {
                return Arrival::DeadlineElapsed;
            };
            held = waited;
            if timed_out.timed_out() && held.arrived < held.expected {
                return Arrival::DeadlineElapsed;
            }
        }
        Arrival::Released
    }

    /// Returns how many participants have arrived.
    #[must_use]
    pub fn arrived(&self) -> usize {
        let (attendance, _) = &*self.attendance;
        attendance.lock().map(|held| held.arrived).unwrap_or_default()
    }
}
