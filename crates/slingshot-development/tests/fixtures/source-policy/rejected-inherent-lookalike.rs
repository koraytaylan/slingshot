//! An inherent method that borrows the exempt spelling.

/// A value that renders itself.
pub struct Rendered;

impl Rendered {
    /// Renders this value.
    pub fn fmt(&self) -> String {
        "rendered".to_owned()
    }
}
