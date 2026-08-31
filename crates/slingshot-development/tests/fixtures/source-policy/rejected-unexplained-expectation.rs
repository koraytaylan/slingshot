//! An expectation that does not say why it is there.

/// Returns nothing.
#[expect(clippy::unused_unit)]
pub fn deliberately_empty() -> () {}
