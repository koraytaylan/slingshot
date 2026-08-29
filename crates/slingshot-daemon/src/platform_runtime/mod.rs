//! Daemon platform-runtime family root.
//!
//! The module map assigns this family the endpoints, owner and election
//! locks, readiness publication, and detached process creation as the daemon
//! uses them. This commit declares the endpoint identities, the two distinct
//! lock identities, the atomic readiness record, and the owner-only runtime
//! directory.

pub mod current_user;
pub mod endpoint;
pub mod failure;
pub mod locks;
pub mod readiness;
