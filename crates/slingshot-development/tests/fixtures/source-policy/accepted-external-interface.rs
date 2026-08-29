//! An implementation of an external interface named in full.

/// A value that renders itself.
pub struct Rendered;

impl ::core::fmt::Display for Rendered {
    fn fmt(&self, formatter: &mut ::core::fmt::Formatter<'_>) -> ::core::fmt::Result {
        write!(formatter, "rendered")
    }
}
