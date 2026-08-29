//! Innermost contract layer of the Slingshot workspace.
//!
//! The workspace dependency contract gives this crate the value objects, the
//! operation and durable agent-job vocabulary, the execution ports, the error
//! types, and the shared limits, and forbids any dependency on another
//! workspace crate. This commit declares the crate's module families and the
//! profile, secret, and selected-environment leaves as documentation-only
//! structure.

pub mod command;
pub mod configuration_snapshot;
pub mod profile;
pub mod profile_authentication_contract;
pub mod remote_job;
pub mod secret_value;
pub mod selected_environment_revision;
