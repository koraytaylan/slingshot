//! Fake services, virtual time, temporary roots, path-only executable values,
//! and reusable operating-system process harnesses.
//!
//! The workspace dependency contract lets this crate depend normally only on
//! `slingshot-domain`, `slingshot-agent-protocol`, `slingshot-local-protocol`,
//! and `slingshot-storage`, and forbids a product crate from reaching it
//! through a normal or build dependency. This commit declares the crate's
//! module families as documentation-only roots.

pub mod daemon_process;
pub mod finite_state_machine_executable;
pub mod platform_runtime;
pub mod process_harness;
pub mod runtime_harness;
pub mod supervised_child;
