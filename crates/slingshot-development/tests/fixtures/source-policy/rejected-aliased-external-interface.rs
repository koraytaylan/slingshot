//! An implementation that reaches the interface through an alias.

use core::fmt;

/// A value that renders itself.
pub struct Rendered;

impl fmt::Display for Rendered {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "rendered")
    }
}
