//! Fake services, virtual time, temporary roots, path-only executable values,
//! and reusable operating-system process harnesses.
//!
//! The workspace dependency contract lets this crate depend normally only on
//! `slingshot-domain`, `slingshot-agent-protocol`, `slingshot-local-protocol`,
//! and `slingshot-storage`, and forbids a product crate from reaching it
//! through a normal or build dependency. The reusable process,
//! temporary-runtime, and supervision harnesses are implemented here, and the
//! identity-management server leaf is documentation-only structure.
//!
//! The process harness holds every child it starts by a handle bound to that
//! one instance, taken at spawn and kept until the child is waited for. Signals
//! go through it, cleanup goes through it, and a numeric process identifier is
//! recorded only so output can be correlated. Children run under an environment
//! the scenario builds rather than inherits, against pipes the harness drains
//! or a pseudo-terminal it holds the controlling end of, and inside a deadline.

pub mod daemon_fault_checkpoints;
pub mod daemon_process;
pub mod fake_author;
pub mod fake_operation_executor;
pub mod finite_state_machine_executable;
pub mod identity_management_server;
pub mod network_fault_script;
pub mod operation_fault_injection;
pub mod platform_runtime;
pub mod process_barrier;
pub mod process_harness;
pub mod runtime_harness;
pub mod supervised_child;
