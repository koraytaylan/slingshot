//! A file whose operational value carries no name.

/// Waits for the declared span.
#[must_use]
pub fn deadline() -> std::time::Duration {
    std::time::Duration::from_millis(5_000)
}
