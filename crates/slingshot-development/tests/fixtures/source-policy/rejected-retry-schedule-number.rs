//! A retry schedule nobody named.

/// Returns how long to wait before attempt `attempt`.
#[must_use]
pub fn backoff_milliseconds(attempt: u32) -> u64 {
    u64::from(attempt) * 500
}
