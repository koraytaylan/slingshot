//! A project-owned contract that borrows the exempt spelling.

/// A contract this workspace declares itself.
pub trait Renderable {
    /// Renders the value.
    fn fmt(&self) -> String;
}

/// A value that renders itself.
pub struct Rendered;

impl Renderable for Rendered {
    fn fmt(&self) -> String {
        "rendered".to_owned()
    }
}
