//! A file that declares a foreign block.

unsafe extern "C" {
    /// A function this workspace did not write.
    pub fn read() -> usize;
}
