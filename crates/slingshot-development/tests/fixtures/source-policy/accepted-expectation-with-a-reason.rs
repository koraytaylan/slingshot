//! An expectation the compiler keeps honest.
//!
//! An expectation is reported when the lint it names stops firing, so it cannot
//! outlive the situation it was written for. That is what makes it different
//! from switching a rule off, and why it is admitted when it says why.

/// Returns nothing, deliberately.
#[expect(clippy::unused_unit, reason = "the empty return is what this sample is about")]
pub fn deliberately_empty() -> () {}
