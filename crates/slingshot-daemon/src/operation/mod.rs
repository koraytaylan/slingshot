//! Daemon operation family root.
//!
//! The module map assigns this family the operation lifecycle, the scheduler,
//! and the executor boundary the daemon composes. This commit declares the
//! family root, its reconciliation, supervision, and remote-submission leaves.

pub mod job_reconciliation;
pub mod recovery_and_event_supervisor;
pub mod remote_submission;
