//! Daemon operation family root.
//!
//! The module map assigns this family the operation lifecycle, the scheduler,
//! and the executor boundary the daemon composes. This commit declares the
//! family root, its reconciliation leaf, and its remote-submission leaf.

pub mod job_reconciliation;
pub mod remote_submission;
