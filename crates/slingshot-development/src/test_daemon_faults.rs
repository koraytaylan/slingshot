//! Driving a daemon through an interruption at every boundary that has one.
//!
//! A restart-safety suite is only as good as the moments it stops at. This
//! composes the fault injector with the test daemon so a suite can walk every
//! named checkpoint, stop there, restart, and assert the one thing that has to
//! be true afterwards: whatever is on disk is a coherent fact rather than a
//! half-written one.

use slingshot_test_support::operation_fault_injection::{
    Checkpoint, EVERY_CHECKPOINT, FaultInjector, Instruction,
};

/// What one interrupted run left behind.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InterruptedRun {
    /// Where it stopped.
    pub interrupted_at: Checkpoint,
    /// Every checkpoint it reached before stopping, in order.
    pub reached: Vec<Checkpoint>,
}

/// One daemon run that can be interrupted at a named point.
#[derive(Debug)]
pub struct FaultedRun {
    /// The injector deciding where it stops.
    injector: FaultInjector,
}

impl FaultedRun {
    /// Returns a run that stops at `checkpoint` and nowhere else.
    #[must_use]
    pub fn stopping_at(checkpoint: Checkpoint) -> Self {
        let injector = FaultInjector::passive();
        injector.arm(checkpoint);
        Self { injector }
    }

    /// Returns a run that stops nowhere.
    #[must_use]
    pub fn uninterrupted() -> Self {
        Self { injector: FaultInjector::passive() }
    }

    /// Returns the injector, so a daemon under test can reach its checkpoints.
    #[must_use]
    pub fn injector(&self) -> &FaultInjector {
        &self.injector
    }

    /// Walks every checkpoint in order, stopping at the armed one.
    ///
    /// Returns where it stopped, or nothing when it got all the way through.
    /// The walk is the daemon's own order, so a suite reading the result sees
    /// the sequence the daemon actually performs rather than a list this module
    /// invented.
    pub fn walk(&self) -> Option<InterruptedRun> {
        for checkpoint in EVERY_CHECKPOINT {
            if self.injector.reach(*checkpoint) == Instruction::Interrupt {
                return Some(InterruptedRun {
                    interrupted_at: *checkpoint,
                    reached: self.injector.reached(),
                });
            }
        }
        None
    }
}
