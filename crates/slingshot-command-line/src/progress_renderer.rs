//! Showing that something is happening, without pretending to know more.
//!
//! Progress is what the daemon reported and nothing else. An estimated
//! percentage would be a claim about a remote system this process cannot see,
//! and the estimate that matters - how much longer - is exactly the one nothing
//! here can make honestly.
//!
//! # It goes to standard error
//!
//! A pipeline reading standard output must see the answer and only the answer.
//! Progress on that stream would corrupt every consumer that did not expect it,
//! and the consumers that did would be parsing around it forever.

use crate::machine_readable_renderer::Stream;

/// Where progress is written.
pub const PROGRESS_STREAM: Stream = Stream::StandardError;

/// How long one progress line may be.
pub const MAXIMUM_PROGRESS_LINE_BYTES: usize = 1024;

/// One thing the daemon said while work was running.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProgressNote {
    /// What it said.
    pub detail: String,
    /// Which operation it was about.
    pub operation_identifier: String,
}

/// Returns the line one note prints, bounded.
///
/// Truncated rather than refused, because a progress line is advisory: losing
/// the tail of a long note costs a reader nothing, and refusing to print it
/// would lose the whole note over its length.
#[must_use]
pub fn render(note: &ProgressNote) -> String {
    let mut line = format!("{}: {}", note.operation_identifier, note.detail);
    if line.len() > MAXIMUM_PROGRESS_LINE_BYTES {
        line.truncate(bounded_boundary(&line));
    }
    line
}

/// Returns the largest character boundary at or below the bound.
fn bounded_boundary(line: &str) -> usize {
    let mut at = MAXIMUM_PROGRESS_LINE_BYTES;
    while at > 0 && !line.is_char_boundary(at) {
        at -= 1;
    }
    at
}

/// Returns whether progress is shown at all.
///
/// Only when a person is reading. A machine-readable run writes exactly one
/// envelope, and progress lines beside it would make a consumer read two
/// streams to find the one answer.
#[must_use]
pub fn is_shown(machine_readable: bool) -> bool {
    !machine_readable
}
