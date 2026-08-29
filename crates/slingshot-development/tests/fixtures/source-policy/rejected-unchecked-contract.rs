//! A file that declares an unchecked contract.

/// A contract whose implementers uphold something unstated.
pub unsafe trait Upheld {
    /// Reads a value.
    fn read(&self) -> usize;
}
