//! Interrupting a daemon exactly where it would hurt most.
//!
//! Restart safety is not something a test can establish by restarting at
//! convenient moments. What matters is the boundaries: the instant between a
//! row being written and its receipt, between a receipt and the bytes it
//! accounts for, between a rename and the directory synchronize that makes it
//! durable. A fault injector names those points so a test can stop at each one
//! deliberately rather than hoping to land on one.
//!
//! Each checkpoint fires at most once. A retry after an interruption has to be
//! able to get past the point that interrupted it, or the test would prove only
//! that the daemon can fail repeatedly.

use std::collections::BTreeSet;
use std::sync::Mutex;

/// A point at which a daemon can be interrupted.
///
/// Named rather than numbered, because what a test wants to say is "stop after
/// the rows are gone and before the bytes are" - and a number could not be
/// reviewed against the code it is describing.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Checkpoint {
    /// After an operation row is written, before its acknowledgement.
    AfterAdmissionCommit,
    /// After a lifecycle fact commits, before anything reads it back.
    AfterLifecycleCommit,
    /// After a resume receipt commits, before its revision is published.
    AfterResumeReceiptCommit,
    /// After artifact bytes are written, before the rename that publishes them.
    BeforeArtifactPublication,
    /// After the rename, before the directory synchronize that makes it durable.
    BeforeDirectorySynchronize,
    /// After maintenance removes rows, before it deletes unreferenced content.
    AfterMaintenanceRowsRemoved,
    /// After the installation record is staged, before it is published.
    BeforeInstallationPublication,
}

/// Every checkpoint, so a suite can walk them all rather than remember them.
pub const EVERY_CHECKPOINT: &[Checkpoint] = &[
    Checkpoint::AfterAdmissionCommit,
    Checkpoint::AfterLifecycleCommit,
    Checkpoint::AfterResumeReceiptCommit,
    Checkpoint::BeforeArtifactPublication,
    Checkpoint::BeforeDirectorySynchronize,
    Checkpoint::AfterMaintenanceRowsRemoved,
    Checkpoint::BeforeInstallationPublication,
];

/// What a checkpoint told its caller to do.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Instruction {
    /// Carry on.
    Continue,
    /// Stop here, as an interrupted process would.
    Interrupt,
}

/// Which checkpoints are armed, and which have fired.
#[derive(Debug, Default)]
pub struct FaultInjector {
    /// Checkpoints that interrupt the next time they are reached.
    armed: Mutex<BTreeSet<Checkpoint>>,
    /// Checkpoints that have been reached, in the order they were.
    reached: Mutex<Vec<Checkpoint>>,
}

impl FaultInjector {
    /// Returns an injector that interrupts nothing.
    #[must_use]
    pub fn passive() -> Self {
        Self::default()
    }

    /// Arms one checkpoint to interrupt the next time it is reached.
    pub fn arm(&self, checkpoint: Checkpoint) {
        if let Ok(mut armed) = self.armed.lock() {
            armed.insert(checkpoint);
        }
    }

    /// Returns what to do at `checkpoint`, and disarms it if it fires.
    ///
    /// Disarming is what lets a retry get past the point that stopped it. An
    /// injector that kept firing would prove only that a daemon can fail the
    /// same way twice.
    pub fn reach(&self, checkpoint: Checkpoint) -> Instruction {
        if let Ok(mut reached) = self.reached.lock() {
            reached.push(checkpoint);
        }
        let fires = self.armed.lock().map(|mut armed| armed.remove(&checkpoint)).unwrap_or(false);
        if fires { Instruction::Interrupt } else { Instruction::Continue }
    }

    /// Returns every checkpoint reached so far, in order.
    #[must_use]
    pub fn reached(&self) -> Vec<Checkpoint> {
        self.reached.lock().map(|held| held.clone()).unwrap_or_default()
    }

    /// Returns whether `checkpoint` has been reached at least once.
    #[must_use]
    pub fn has_reached(&self, checkpoint: Checkpoint) -> bool {
        self.reached().contains(&checkpoint)
    }
}
