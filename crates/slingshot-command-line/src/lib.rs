//! Command-line adapter for the `slingshot` product executable.
//!
//! The workspace dependency contract lets this crate depend on
//! `slingshot-local-protocol`, `slingshot-configuration`, and
//! `slingshot-daemon`. The process entry point stays thin and delegates here,
//! so command behavior stays testable without spawning a process. The process entry point stays thin and delegates to the
//! argument surface, so command behavior stays testable without spawning a
//! process.

pub mod command_line;
pub mod commands;
pub mod daemon_connection;
pub mod daemon_entry;
pub mod explicit_daemon_start;
pub mod model_context_protocol;
pub mod platform_runtime;
