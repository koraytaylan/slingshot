//! A file whose operational value carries a name.

/// Milliseconds a caller waits before it gives up.
const WAIT_MILLISECONDS: u64 = 5_000;

/// Waits for the declared span.
#[must_use]
pub fn deadline() -> std::time::Duration {
    std::time::Duration::from_millis(WAIT_MILLISECONDS)
}
