//! A collection bound nobody named.

/// Returns whether this listing fits one page.
#[must_use]
pub fn fits_one_page(rows: usize) -> bool {
    rows <= 50
}
