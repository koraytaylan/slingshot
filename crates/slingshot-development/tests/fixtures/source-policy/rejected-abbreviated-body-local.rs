//! An exempt implementation whose body declares a shortened name.

/// A value that renders itself.
pub struct Rendered;

impl ::core::fmt::Display for Rendered {
    fn fmt(&self, formatter: &mut ::core::fmt::Formatter<'_>) -> ::core::fmt::Result {
        let cfg = "rendered";
        write!(formatter, "{cfg}")
    }
}
