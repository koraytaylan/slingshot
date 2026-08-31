//! Numbers that are structure rather than quantity.
//!
//! Zero and one are identity and index values. A position in a sequence is a
//! position. A version grammar, a protocol syntax, and a fixture parsed from
//! outside are all data rather than decisions somebody made. None of them is a
//! number a reader would have to guess the meaning of, so none of them needs a
//! name to stand beside it.

/// Every window a listing offers, laid out as data.
pub const WINDOWS: &[usize] = &[10, 25, 50, 100];

/// How a version is written.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VersionField {
    /// The first field.
    Major = 0,
    /// The second.
    Minor = 1,
    /// The third.
    Patch = 2,
}

/// Returns the first window a listing offers.
#[must_use]
pub fn narrowest_window() -> usize {
    WINDOWS[0]
}

/// Returns the second window, which is the one a caller gets by default.
#[must_use]
pub fn default_window() -> usize {
    WINDOWS[1]
}

/// Returns whether `spelling` has the three fields a version has.
#[must_use]
pub fn version_is_well_formed(spelling: &str) -> bool {
    spelling.split('.').count() == WINDOWS.len() - 1
}

/// Returns the payload one framed message carries.
#[must_use]
pub fn payload_of(frame: &str) -> &str {
    match frame.split_once(':') {
        Some((_, rest)) => rest,
        None => "",
    }
}

/// Returns how many rows one parsed fixture declares.
#[must_use]
pub fn declared_rows(fixture: &str) -> usize {
    fixture.lines().filter(|line| !line.is_empty()).count()
}
