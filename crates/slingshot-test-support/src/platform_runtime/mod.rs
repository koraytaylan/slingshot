//! Test-support platform-runtime family root.
//!
//! The module map assigns this family the deterministic platform fakes and
//! the supervision channel that test daemons are cleaned up through. This
//! commit owns the supervision channel; the deterministic policy evaluation
//! belongs to the outermost tooling crate, which can reach every product
//! contract.

pub mod supervision;
