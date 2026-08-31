//! Target-scoped application service and local server.
//!
//! The workspace dependency contract lets this crate depend on every inward
//! product adapter except the command line. This commit declares the crate's module families as
//! documentation-only roots.

pub mod artifact_transfer;
pub mod author_agent_operation_executor;
pub mod diagnostics;
pub mod local_server;
pub mod operation;
pub mod operation_maintenance;
pub mod operation_queries;
pub mod operation_recovery;
pub mod operation_scheduler;
pub mod operation_submission;
pub mod operation_wait;
pub mod ownership;
pub mod platform_runtime;
pub mod process_checkpoint;
pub mod request_dispatch;
pub mod runtime_namespace;
pub mod service;
pub mod shutdown;
pub mod startup;
pub mod unavailable_operation_executor;
