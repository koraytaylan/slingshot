//! A file that implements an unchecked contract.

/// A contract whose implementers uphold something unstated.
pub unsafe trait Upheld {
    /// Reads a value.
    fn read(&self) -> usize;
}

/// A value that upholds it.
pub struct Value;

unsafe impl Upheld for Value {
    fn read(&self) -> usize {
        0
    }
}
