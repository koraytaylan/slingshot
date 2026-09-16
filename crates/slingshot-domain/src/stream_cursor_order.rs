//! Shared ordering for retained and wire subscription cursors.
//!
//! The agent emits canonical decimal `generation:sequence` positions. Compare
//! those numerically without rewriting the bytes needed for replay. Other
//! historical cursor encodings retain their text ordering in a separate class;
//! that class sorts before decimal positions so mixed comparisons remain a
//! transitive total order. Ordering never authorizes a generation transition.

use core::cmp::Ordering;

/// Compares cursor positions without altering their replay identifiers.
///
/// Canonical decimal pairs compare numerically. Nondecimal spellings compare
/// bytewise with each other and sort before decimal pairs. Callers must still
/// validate subscription and generation identity independently.
#[must_use]
pub fn compare(left: &str, right: &str) -> Ordering {
    match (decimal_position(left), decimal_position(right)) {
        (Some(left_position), Some(right_position)) => left_position.cmp(&right_position),
        (None, None) => left.cmp(right),
        (None, Some(_)) => Ordering::Less,
        (Some(_), None) => Ordering::Greater,
    }
}

fn decimal_position(cursor: &str) -> Option<(u64, u64)> {
    let (generation, sequence) = cursor.split_once(':')?;
    Some((canonical_number(generation)?, canonical_number(sequence)?))
}

fn canonical_number(spelling: &str) -> Option<u64> {
    let number = spelling.parse::<u64>().ok()?;
    (number.to_string() == spelling).then_some(number)
}
