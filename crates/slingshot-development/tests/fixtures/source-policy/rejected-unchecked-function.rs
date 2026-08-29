//! A file that declares an unchecked function.

/// Reads a value without checking.
///
/// # Safety
///
/// The caller upholds nothing at all.
pub unsafe fn read() -> usize {
    0
}
