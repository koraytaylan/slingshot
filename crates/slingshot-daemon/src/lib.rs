//! Target-scoped application service and local server.
//!
//! The workspace dependency contract lets this crate depend on every inward
//! product adapter except the command line. This commit declares the crate's module families as
//! documentation-only roots.

pub mod local_server;
pub mod operation;
pub mod ownership;
pub mod platform_runtime;
pub mod process_checkpoint;
pub mod runtime_namespace;
pub mod service;
