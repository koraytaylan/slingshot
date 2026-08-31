//! Command-line adapter for the `slingshot` product executable.
//!
//! The workspace dependency contract lets this crate depend on
//! `slingshot-local-protocol`, `slingshot-configuration`, and
//! `slingshot-daemon`. The process entry point stays thin and delegates here,
//! so command behavior stays testable without spawning a process. The process entry point stays thin and delegates to the
//! argument surface, so command behavior stays testable without spawning a
//! process.

pub mod application;
pub mod artifact_access;
pub mod artifact_download;
pub mod artifact_staging_lock;
pub mod artifact_staging_metadata;
pub mod command_line;
pub mod commands;
pub mod configuration_check;
pub mod daemon_answer;
pub mod daemon_connection;
pub mod daemon_request;
pub mod daemon_entry;
pub mod daemon_process;
pub mod exit_classification;
pub mod explicit_daemon_start;
pub mod human_renderer;
pub mod interrupt;
pub mod invocation;
pub mod machine_outcome_envelope;
pub mod machine_readable_renderer;
pub mod model_context_protocol;
pub mod operation_maintenance;
pub mod operation_observation;
pub mod operation_submission;
pub mod platform_runtime;
pub mod predicate_arguments;
pub mod progress_renderer;
pub mod property_document;
pub mod target_selection;
