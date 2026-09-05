//! Daemon operation family root.
//!
//! The module map assigns this family the operation lifecycle, the scheduler,
//! and the executor boundary the daemon composes. This commit declares the
//! family root, its completion, reconciliation, supervision, settlement, and remote-submission leaves.

pub mod artifact_completion;
pub mod durable_author_submission;
pub mod author_authentication;
pub mod durable_author_lookup;
pub mod durable_author_event;
pub mod selected_event_attachment;
pub mod terminal_event_recovery;
pub mod job_reconciliation;
pub mod recovery_and_event_supervisor;
pub mod remote_result_settlement;
pub mod remote_submission;
pub mod subscription_reset;
pub mod generation_loss_probe;
pub mod generation_recovery_dispatch;
