//! Where this server says anything that is not a protocol message.
//!
//! Standard output carries protocol messages and nothing else, whatever
//! happens to the diagnostic stream, because a diagnostic on standard output
//! corrupts every client parsing it.
//!
//! # A diagnostic never delays an answer
//!
//! The diagnostic stream can be full, closed, or attached to something that
//! has stopped reading. None of that is a reason for a request to wait, so
//! recording never blocks: what does not fit is dropped and counted, and the
//! count is itself a diagnostic worth having, because it says how much was
//! lost rather than pretending nothing was.
//!
//! Nothing here is written from a failing or panicking path. A process on its
//! way out has no business waiting on a stream that is already not draining.

/// How many records this sink holds before it drops what it is given.
pub const MAXIMUM_HELD_RECORDS: usize = 256;

/// How many bytes this sink holds before it drops what it is given.
pub const MAXIMUM_HELD_BYTES: usize = 65_536;

/// What happened to one record.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Recorded {
    /// It was kept, for the stream to take when it takes anything.
    Kept,
    /// It was dropped, because keeping it would have meant waiting.
    Dropped,
}

/// Where diagnostics go while the streams are working, and when they are not.
#[derive(Debug, Default)]
pub struct ProtocolDiagnosticSink {
    /// What is held, in the order it was recorded.
    held: Vec<String>,
    /// How many bytes are held.
    held_bytes: usize,
    /// How many records were dropped, saturating rather than wrapping.
    dropped: usize,
    /// Whether the stream this writes to has stopped taking anything.
    closed: bool,
}

impl ProtocolDiagnosticSink {
    /// Returns a sink holding nothing.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Returns a sink whose stream takes nothing at all.
    #[must_use]
    pub fn closed() -> Self {
        Self { closed: true, ..Self::default() }
    }

    /// Records one diagnostic without waiting for anything.
    pub fn record(&mut self, message: &str) -> Recorded {
        if self.closed
            || self.held.len() >= MAXIMUM_HELD_RECORDS
            || self.held_bytes + message.len() > MAXIMUM_HELD_BYTES
        {
            self.dropped = self.dropped.saturating_add(1);
            return Recorded::Dropped;
        }
        self.held_bytes += message.len();
        self.held.push(message.to_owned());
        Recorded::Kept
    }

    /// Returns what is held, in order.
    #[must_use]
    pub fn held(&self) -> &[String] {
        &self.held
    }

    /// Returns how many records were dropped.
    #[must_use]
    pub fn dropped(&self) -> usize {
        self.dropped
    }
}
