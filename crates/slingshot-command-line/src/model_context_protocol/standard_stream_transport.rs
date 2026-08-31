//! Reading and writing framed messages on the standard streams.
//!
//! Bounded, and byte-clean. A message larger than the transport admits is
//! refused rather than partially read, and nothing but a message is ever
//! written to standard output.
//!
//! # One writer, and complete lines only
//!
//! Every response and notification goes through one queue and one writer, so
//! two producers cannot interleave halves of two messages into one unparseable
//! line. A line is serialized in full before it is queued, which means the
//! writer never discovers halfway through that it cannot finish.
//!
//! # Failing stops, and stops once
//!
//! Standard output closing, filling, or timing out is not a message that
//! failed; it is the end of this server's ability to answer anything. So the
//! first
//! such failure stops intake, rejects producers, discards what has not started
//! being written, and asks the application to detach every local waiter. It
//! happens once: a second failure finds the transition already made.
//!
//! At most one line can be left unterminated, and only at the end. A sink that
//! accepted a prefix and then failed leaves that suffix invalid and every
//! completed line before it parseable, which is what lets a client read
//! everything that did arrive.

use std::time::Duration;

use serde_json::Value;
use slingshot_local_protocol::foundation_contract::FoundationContract;

/// Every protocol revision this build serves, in the order it prefers them.
///
/// One list, borrowed by both era handlers. A handler with a copy of its own is
/// a handler that answers a discovery request with something the other one does
/// not offer, and the two disagreeing about what this build supports is exactly
/// the failure a client cannot diagnose.
pub const SUPPORTED_REVISIONS: &[&str] = &["2026-07-28", "2025-06-18"];

/// Returns the largest line this transport reads or writes, in bytes.
///
/// The workspace's one wire bound, read rather than restated. A transport with
/// a number of its own would eventually admit a message the rest of the product
/// refuses, or refuse one it admits.
#[must_use]
pub fn maximum_line_bytes() -> usize {
    usize::try_from(FoundationContract::embedded().framing.maximum_payload_bytes)
        .unwrap_or(usize::MAX)
}

/// Returns the deepest container nesting a message may reach.
#[must_use]
pub fn maximum_nesting_depth() -> usize {
    usize::try_from(FoundationContract::embedded().framing.maximum_nesting_depth)
        .unwrap_or(usize::MAX)
}

/// How many lines the output queue holds before a producer is refused.
pub const MAXIMUM_QUEUED_MESSAGES: usize = 256;

/// How many bytes the output queue holds before a producer is refused.
pub const MAXIMUM_QUEUED_BYTES: usize = 8_388_608;

/// How long a producer waits for room in the queue before the transport fails.
pub const QUEUE_PRESSURE_DEADLINE: Duration = Duration::from_secs(30);

/// How long one line may take to be written before the transport fails.
pub const WRITE_DEADLINE: Duration = Duration::from_secs(30);

/// How long cleanup may take after an output failure.
pub const SHUTDOWN_DEADLINE: Duration = Duration::from_secs(10);

/// Which revision a peer speaks.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum ProtocolRevision {
    /// The revision this build prefers, whose clients carry their own state.
    Current,
    /// The older revision, whose clients establish a session first.
    Legacy,
}

impl ProtocolRevision {
    /// Returns the revision this build offers first.
    #[must_use]
    pub fn preferred() -> Self {
        Self::Current
    }

    /// Returns the exact text this revision is named by.
    #[must_use]
    pub fn as_text(self) -> &'static str {
        match self {
            Self::Current => SUPPORTED_REVISIONS[0],
            Self::Legacy => SUPPORTED_REVISIONS[1],
        }
    }

    /// Returns the revision one exact name means.
    #[must_use]
    pub fn named(text: &str) -> Option<Self> {
        [Self::Current, Self::Legacy].into_iter().find(|held| held.as_text() == text)
    }

    /// Returns the best revision a peer offering `named` can be served with.
    ///
    /// This build's preference decides, not the peer's order. A peer that lists
    /// its own favourite first would otherwise choose for both of them, and a
    /// server that let it would serve two eras differently depending on who
    /// asked.
    #[must_use]
    pub fn negotiated(named: &[String]) -> Option<Self> {
        SUPPORTED_REVISIONS
            .iter()
            .find(|offered| named.iter().any(|held| held == *offered))
            .and_then(|offered| Self::named(offered))
    }
}

/// What one accepted line carries.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Message {
    /// A request, which is answered exactly once.
    Request {
        /// What the answer is correlated by.
        identifier: String,
        /// What is being asked.
        method: String,
        /// What it is being asked with.
        parameters: Value,
    },
    /// A notification, which is answered never.
    Notification {
        /// What is being said.
        method: String,
        /// What it is being said with.
        parameters: Value,
    },
}

impl Message {
    /// Returns the identifier this message is answered under, when it has one.
    #[must_use]
    pub fn identifier(&self) -> Option<&str> {
        match self {
            Self::Request { identifier, .. } => Some(identifier),
            Self::Notification { .. } => None,
        }
    }
}

/// Why one line is not a message this transport accepts.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum MessageRefusal {
    /// The line is longer than the transport reads.
    #[error("a message is at most {allowed} bytes, and this is {observed}", allowed = maximum_line_bytes(), observed = .0)]
    LineTooLong(usize),
    /// The bytes are not text.
    #[error("a message is text, and these bytes are not")]
    EncodingInvalid,
    /// The text is not one JavaScript Object Notation value.
    #[error("a message is one object, and this is not readable as one")]
    NotReadable,
    /// The value is not an object.
    #[error("a message is an object, and this is another kind of value")]
    NotAnObject,
    /// A member is given twice.
    #[error("{0} is given twice, and which one was meant is not knowable")]
    DuplicateMember(String),
    /// The containers nest deeper than the transport reads.
    #[error("a message nests at most {} deep", maximum_nesting_depth())]
    TooDeep,
    /// The object is neither a request nor a notification.
    #[error("a message names a method, and a request also names an identifier")]
    UnknownDirection,
}

/// Returns the message one line carries.
///
/// # Errors
///
/// Returns [`MessageRefusal`] naming the first rule the line breaks. The bounds
/// are checked before the content, because a line that is too long or too deep
/// must not be parsed at all: parsing it is the cost the bound exists to avoid.
pub fn read_message(line: &[u8]) -> Result<Message, MessageRefusal> {
    if line.len() > maximum_line_bytes() {
        return Err(MessageRefusal::LineTooLong(line.len()));
    }
    let text = core::str::from_utf8(line).map_err(|_| MessageRefusal::EncodingInvalid)?;
    require_no_duplicate_member(text)?;
    let value: Value = serde_json::from_str(text).map_err(|_| MessageRefusal::NotReadable)?;
    require_within_depth(&value, maximum_nesting_depth())?;
    let object = value.as_object().ok_or(MessageRefusal::NotAnObject)?;
    let method = object
        .get("method")
        .and_then(Value::as_str)
        .ok_or(MessageRefusal::UnknownDirection)?
        .to_owned();
    let parameters = object.get("params").cloned().unwrap_or(Value::Null);
    match object.get("id") {
        None => Ok(Message::Notification { method, parameters }),
        Some(Value::String(identifier)) => {
            Ok(Message::Request { identifier: identifier.clone(), method, parameters })
        }
        Some(Value::Number(identifier)) => {
            Ok(Message::Request { identifier: identifier.to_string(), method, parameters })
        }
        Some(_) => Err(MessageRefusal::UnknownDirection),
    }
}

/// Requires no object in one message to give a member twice.
///
/// Read from the text rather than from the parsed value, because a parsed
/// value has already chosen which of two identical members won. Which one a
/// client meant is not knowable, and choosing silently is how two peers end up
/// disagreeing about what a message said.
fn require_no_duplicate_member(text: &str) -> Result<(), MessageRefusal> {
    let mut scanner = MemberScanner::new();
    for character in text.chars() {
        scanner.read(character)?;
    }
    Ok(())
}

/// Which members each open object has named so far.
#[derive(Debug, Default)]
struct MemberScanner {
    /// One set of names per open object.
    open: Vec<std::collections::BTreeSet<String>>,
    /// The string being read, when one is.
    reading: Option<String>,
    /// Whether the last character was an escape.
    escaped: bool,
    /// The string that was read most recently.
    held: Option<String>,
}

impl MemberScanner {
    /// Returns a scanner that has read nothing.
    fn new() -> Self {
        Self::default()
    }

    /// Reads one character.
    ///
    /// # Errors
    ///
    /// Returns [`MessageRefusal::DuplicateMember`] the first time an open
    /// object names one member twice.
    fn read(&mut self, character: char) -> Result<(), MessageRefusal> {
        if self.reading.is_some() {
            self.read_inside_string(character);
            return Ok(());
        }
        match character {
            '"' => self.reading = Some(String::new()),
            '{' => self.open.push(std::collections::BTreeSet::new()),
            '}' => {
                self.open.pop();
                self.held = None;
            }
            ':' => return self.name_a_member(),
            ',' => self.held = None,
            _ => {}
        }
        Ok(())
    }

    /// Reads one character while inside a string.
    fn read_inside_string(&mut self, character: char) {
        if self.escaped {
            if let Some(reading) = self.reading.as_mut() {
                reading.push(character);
            }
            self.escaped = false;
            return;
        }
        match character {
            '\\' => self.escaped = true,
            '"' => self.held = self.reading.take(),
            other => {
                if let Some(reading) = self.reading.as_mut() {
                    reading.push(other);
                }
            }
        }
    }

    /// Records the string just read as a member of the innermost object.
    fn name_a_member(&mut self) -> Result<(), MessageRefusal> {
        let Some(name) = self.held.take() else {
            return Ok(());
        };
        let Some(members) = self.open.last_mut() else {
            return Ok(());
        };
        if members.insert(name.clone()) {
            Ok(())
        } else {
            Err(MessageRefusal::DuplicateMember(name))
        }
    }
}

/// Requires one value to nest no deeper than `allowed`.
fn require_within_depth(value: &Value, allowed: usize) -> Result<(), MessageRefusal> {
    if allowed == 0 {
        return Err(MessageRefusal::TooDeep);
    }
    match value {
        Value::Array(held) => {
            for member in held {
                require_within_depth(member, allowed - 1)?;
            }
            Ok(())
        }
        Value::Object(held) => {
            for member in held.values() {
                require_within_depth(member, allowed - 1)?;
            }
            Ok(())
        }
        _ => Ok(()),
    }
}

/// Where a written line went, or why it did not go.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Written {
    /// Every byte of the line reached the sink.
    Complete,
    /// Some bytes reached it and then it failed.
    Prefix(usize),
    /// It accepted nothing.
    Refused,
}

/// Whatever this server writes complete lines to.
pub trait LineSink {
    /// Writes one line, and returns how much of it arrived.
    fn write_line(&mut self, line: &str) -> Written;
}

/// Why a producer's line was not taken.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum QueueRefusal {
    /// The queue holds as much as it may.
    #[error(
        "the output queue holds {MAXIMUM_QUEUED_MESSAGES} messages or {MAXIMUM_QUEUED_BYTES} bytes"
    )]
    Full,
    /// Output has failed, so nothing more is written.
    #[error("output has failed, and this server writes nothing further")]
    Stopped,
    /// The line is longer than the transport writes.
    #[error("a message is at most {allowed} bytes, and this is {observed}", allowed = maximum_line_bytes(), observed = .0)]
    TooLong(usize),
}

/// Why the transport stopped writing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OutputFailure {
    /// A producer waited longer than the pressure deadline for room.
    PressureExpired,
    /// A line took longer than the write deadline.
    WriteExpired,
    /// The sink refused or failed partway.
    SinkFailed,
}

/// The one queue and the one writer every answer goes through.
#[derive(Debug, Default)]
pub struct OutputQueue {
    /// Lines waiting to be written, in the order they were taken.
    waiting: std::collections::VecDeque<String>,
    /// How many bytes are waiting.
    waiting_bytes: usize,
    /// Why writing stopped, once it has.
    failure: Option<OutputFailure>,
    /// Identifiers whose lines have been written in full.
    acknowledged: Vec<String>,
}

impl OutputQueue {
    /// Returns an empty queue that has not failed.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Returns why output stopped, once it has.
    #[must_use]
    pub fn failure(&self) -> Option<OutputFailure> {
        self.failure
    }

    /// Returns how many lines are waiting.
    #[must_use]
    pub fn waiting(&self) -> usize {
        self.waiting.len()
    }

    /// Returns how many bytes are waiting.
    #[must_use]
    pub fn waiting_bytes(&self) -> usize {
        self.waiting_bytes
    }

    /// Returns the identifiers whose lines were written in full, in order.
    #[must_use]
    pub fn acknowledged(&self) -> &[String] {
        &self.acknowledged
    }

    /// Takes one complete line from a producer.
    ///
    /// # Errors
    ///
    /// Returns [`QueueRefusal`] when output has failed, the queue is full, or
    /// the line is longer than this transport writes.
    pub fn enqueue(&mut self, line: &str) -> Result<(), QueueRefusal> {
        if self.failure.is_some() {
            return Err(QueueRefusal::Stopped);
        }
        if line.len() > maximum_line_bytes() {
            return Err(QueueRefusal::TooLong(line.len()));
        }
        if self.waiting.len() >= MAXIMUM_QUEUED_MESSAGES
            || self.waiting_bytes + line.len() > MAXIMUM_QUEUED_BYTES
        {
            return Err(QueueRefusal::Full);
        }
        self.waiting_bytes += line.len();
        self.waiting.push_back(line.to_owned());
        Ok(())
    }

    /// Records that a producer waited `waited` for room.
    ///
    /// A wait past the deadline is an output failure rather than one refused
    /// message: a queue that has not drained in that long is not going to.
    pub fn waited_for_room(&mut self, waited: Duration) {
        if waited >= QUEUE_PRESSURE_DEADLINE {
            self.fail(OutputFailure::PressureExpired);
        }
    }

    /// Writes everything waiting, stopping at the first failure.
    ///
    /// Returns how many complete lines reached the sink. A line that arrived in
    /// part is left unterminated and nothing is written after it, so a reader
    /// gets every completed line and one invalid suffix at the end.
    pub fn write_waiting(&mut self, sink: &mut dyn LineSink, each_took: Duration) -> usize {
        let mut written = 0;
        while let Some(line) = self.waiting.pop_front() {
            self.waiting_bytes -= line.len();
            if each_took >= WRITE_DEADLINE {
                self.fail(OutputFailure::WriteExpired);
                return written;
            }
            match sink.write_line(&line) {
                Written::Complete => {
                    written += 1;
                    self.acknowledged.push(line);
                }
                Written::Prefix(_) | Written::Refused => {
                    self.fail(OutputFailure::SinkFailed);
                    return written;
                }
            }
        }
        written
    }

    /// Makes the one output-failure transition, if it has not been made.
    ///
    /// Idempotent by construction: the first reason wins, everything unstarted
    /// is discarded, and a later failure finds the transition already made.
    pub fn fail(&mut self, reason: OutputFailure) {
        if self.failure.is_some() {
            return;
        }
        self.failure = Some(reason);
        self.waiting.clear();
        self.waiting_bytes = 0;
    }

    /// Reports whether this transport still accepts anything.
    #[must_use]
    pub fn accepts_more(&self) -> bool {
        self.failure.is_none()
    }
}
