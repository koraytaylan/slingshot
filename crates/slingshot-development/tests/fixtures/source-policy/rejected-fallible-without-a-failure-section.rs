//! A file whose fallible interface does not say what makes it fail.

/// Reads a value.
pub fn read() -> Result<usize, String> {
    Ok(0)
}
