//! A wait nobody named.

/// Waits for the stream to settle.
pub fn settle() {
    std::thread::sleep(std::time::Duration::from_millis(250));
}
