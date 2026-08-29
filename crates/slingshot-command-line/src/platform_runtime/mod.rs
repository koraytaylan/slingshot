//! Command-line platform-runtime family root.
//!
//! The module map assigns this family the endpoints, startup-election lock,
//! and detached spawn behavior a client needs. The endpoints and the two lock
//! identities belong to the daemon crate, which the command line depends on;
//! this family owns the detached child a client creates.

pub mod detached_child;
