//! Capability probes owned by the domain crate.
//!
//! Each module exercises the public behavior one inventory row promises, so a
//! candidate that compiles but does not expose the required interface fails
//! here rather than in the first feature task that needs it.

mod property_tests;
mod typed_errors;
mod unicode_normalization;
