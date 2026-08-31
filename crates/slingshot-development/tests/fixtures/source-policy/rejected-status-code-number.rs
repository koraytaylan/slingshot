//! A status nobody named.

/// Returns whether the author refused this submission.
#[must_use]
pub fn was_refused(status: u16) -> bool {
    status == 409
}
