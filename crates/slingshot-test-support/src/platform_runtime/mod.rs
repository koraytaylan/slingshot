//! Test-support platform-runtime family root.
//!
//! The module map assigns this family the deterministic platform fakes a test
//! evaluates a supported row against. The supervision channel a detached test
//! daemon is cleaned up through is a top-level module of this crate, because a
//! test reaches for it directly rather than through a platform family.
