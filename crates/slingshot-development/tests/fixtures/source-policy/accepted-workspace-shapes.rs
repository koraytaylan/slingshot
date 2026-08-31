//! Shapes the complete workspace introduced, all of them acceptable.
//!
//! One sample per family the later plans added - a command, a configuration
//! document, an authentication method, a daemon phase, an agent transport
//! frame, a storage row, a command-line leaf, a protocol tool, and a release
//! input - written the way the policy asks: names spelled in full, quantities
//! named, exported items documented, fallible interfaces saying what makes them
//! fail, and generic parameters and lifetimes that are words rather than
//! initials.

/// How many milliseconds a heartbeat may be late before a stream is stale.
pub const HEARTBEAT_TOLERANCE_MILLISECONDS: u64 = 5_000;

/// How many rows one page of a listing holds.
pub const LISTING_PAGE_ROWS: usize = 50;

/// The status a refused submission answers with.
pub const REFUSED_STATUS: u16 = 409;

/// One command a caller may submit.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SubmittedCommand {
    /// The name it answers to on the wire.
    pub wire_name: String,
    /// The repository path it acts on.
    pub repository_path: String,
}

/// How an environment authenticates to its author.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AuthenticationMethod {
    /// A user name and a password.
    BasicCredentials,
    /// A service credentials document.
    ServiceCredentials,
}

/// A phase a daemon passes through while it starts.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StartupPhase {
    /// Reading configuration.
    ReadingConfiguration,
    /// Opening storage.
    OpeningStorage,
    /// Listening for callers.
    Listening,
}

/// Something a protocol tool can be asked for.
pub trait Answerable<Question, Answer> {
    /// Returns the answer to one question.
    ///
    /// # Errors
    ///
    /// Returns the refusal text when the question is one this cannot answer.
    fn answer(&self, question: &Question) -> Result<Answer, String>;
}

/// One borrowed row of a listing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ListingRow<'listing> {
    /// The identifier the row carries.
    pub identifier: &'listing str,
}

/// Returns how many pages `rows` fills.
#[must_use]
pub fn pages_for(rows: usize) -> usize {
    rows.div_ceil(LISTING_PAGE_ROWS)
}

/// Returns whether a heartbeat that arrived after `elapsed` is still in time.
#[must_use]
pub fn heartbeat_is_in_time(elapsed: u64) -> bool {
    elapsed <= HEARTBEAT_TOLERANCE_MILLISECONDS
}

/// Returns the command one wire name and path describe.
///
/// # Errors
///
/// Returns the refusal text when the path is not absolute, because a relative
/// path names a different node depending on where it is read from.
pub fn submitted(wire_name: &str, repository_path: &str) -> Result<SubmittedCommand, String> {
    if !repository_path.starts_with('/') {
        return Err(format!("{repository_path} is not an absolute repository path"));
    }
    Ok(SubmittedCommand {
        wire_name: wire_name.to_owned(),
        repository_path: repository_path.to_owned(),
    })
}
