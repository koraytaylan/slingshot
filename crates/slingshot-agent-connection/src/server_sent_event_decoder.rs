//! Reading the author's event stream without letting it decide how much memory
//! this daemon spends.
//!
//! A stream is bytes arriving from somewhere else for as long as somewhere else
//! feels like sending them. Every quantity in it - the length of a line, the
//! size of an event, the length of an identifier, how long the whole thing goes
//! on - is chosen by the far side. So each one is bounded here by name, checked
//! as the bytes arrive rather than after they have been collected, because a
//! bound applied to a buffer that is already full is a bound on nothing.
//!
//! Decoding is incremental for the same reason it is bounded: the transport
//! delivers chunks that have nothing to do with line or event boundaries, and a
//! decoder that waited for a whole event before parsing would be holding an
//! unbounded amount of somebody else's data while it waited.
//!
//! Nothing is inferred from anything else. The subscription and the generation
//! are compared against what the request asked for, the stream cursor is the
//! `id` field and never a sequence number, the per-job sequence is the
//! document's and never a cursor, and a terminal event's contract correlation
//! is authenticated in full before the event is exposed. An event that names
//! another subscription, another generation, or another submission is not this
//! daemon's event, whatever else is right about it.

use serde::Deserialize;
use slingshot_agent_protocol::identity::DocumentProvenance;
use slingshot_agent_protocol::job_contract::JobEvent;
use slingshot_agent_protocol::job_contract::JobEventKind;
use slingshot_agent_protocol::wire_contract::{ExpectedProvenance, WireRefusal};
use slingshot_domain::author_agent_transport_contract::AuthorAgentTransportContract;

use crate::author_hypertext_transfer_protocol_policy::{ResponseHead, ResponseRefusal};

/// The media type a live event stream is, and the only one accepted.
pub const EVENT_STREAM_MEDIA_TYPE: &str = "text/event-stream";

/// Parameter lists the media type may carry, spelled in lowercase.
///
/// Absent or an explicit UTF-8, and nothing else. A stream announcing another
/// character set is announcing bytes this decoder would have to transcode, and
/// transcoding somebody else's stream is one more thing that can be wrong.
pub const PERMITTED_MEDIA_PARAMETERS: &[&str] = &["", "charset=utf-8"];

/// The field carrying an event's payload.
pub const DATA_FIELD: &str = "data";

/// The field naming what kind of event this is.
pub const EVENT_FIELD: &str = "event";

/// The field carrying the stream cursor.
pub const IDENTIFIER_FIELD: &str = "id";

/// The field a server uses to suggest a reconnection delay.
pub const RETRY_FIELD: &str = "retry";

/// The character that begins a comment, and separates a field from its value.
const FIELD_SEPARATOR: char = ':';

/// The character separating a media type from its parameters.
const MEDIA_PARAMETER_SEPARATOR: char = ';';

/// The byte a line ends with.
const LINE_FEED: u8 = b'\n';

/// The byte that may precede a line feed, or end a line on its own.
const CARRIAGE_RETURN: u8 = b'\r';

/// The character a data buffer's parts are joined with.
const LINE_FEED_CHARACTER: char = '\n';

/// The bounds one stream is decoded under, read from the transport contract.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DecoderBounds {
    /// Bytes one event's field lines may come to together.
    pub event_bytes: u64,
    /// Bytes one stream cursor may occupy.
    pub identifier_bytes: u64,
    /// Bytes one line may occupy.
    pub line_bytes: u64,
}

impl DecoderBounds {
    /// Returns the bounds the transport contract names.
    #[must_use]
    pub fn embedded() -> Self {
        let contract = AuthorAgentTransportContract::embedded();
        Self {
            event_bytes: contract.limit("maximum_server_sent_event_bytes"),
            identifier_bytes: contract.limit("maximum_agent_operation_identifier_bytes"),
            line_bytes: contract.limit("maximum_server_sent_event_line_bytes"),
        }
    }
}

/// Why a stream cannot be attached to, or cannot be read any further.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum StreamRefusal {
    /// The response head is one the shared policy refuses.
    #[error(transparent)]
    Head(#[from] ResponseRefusal),
    /// The response is not a single event stream.
    #[error("a live stream is exactly one {EVENT_STREAM_MEDIA_TYPE}, and this announced {named}")]
    MediaType {
        /// What it announced.
        named: String,
    },
    /// The media type carries parameters this decoder does not accept.
    #[error(
        "a live stream is UTF-8 or says nothing about its character set, and this said {named}"
    )]
    MediaParameters {
        /// What it said.
        named: String,
    },
    /// A trailer section arrived that the head never declared.
    #[error("a trailer nobody declared is a lost connection, not a fact about this stream")]
    UndeclaredTrailer,
    /// One line is longer than a line may be.
    #[error("one line holds at most {allowed} bytes, and this reached {actual}")]
    LineTooLong {
        /// How long one may be.
        allowed: u64,
        /// How long this reached.
        actual: usize,
    },
    /// One event's fields come to more than an event may.
    #[error("one event holds at most {allowed} bytes, and this reached {actual}")]
    EventTooLarge {
        /// How large one may be.
        allowed: u64,
        /// How large this reached.
        actual: usize,
    },
    /// One cursor is longer than a cursor may be.
    #[error("one stream cursor holds at most {allowed} bytes, and this holds {actual}")]
    IdentifierTooLong {
        /// How long one may be.
        allowed: u64,
        /// How long this is.
        actual: usize,
    },
    /// The bytes are not text.
    #[error("an event stream is text, and these bytes are not")]
    NotUnicode,
    /// The payload is not one valid event document.
    #[error("this event's {field} is not something this build can read")]
    Malformed {
        /// Which part could not be read.
        field: &'static str,
    },
    /// The event belongs to a subscription this stream did not ask for.
    #[error("this stream asked for one subscription, and this event names another")]
    AnotherSubscription,
    /// The event belongs to another incarnation of the event store.
    #[error("this stream asked for generation {expected}, and this event names {named}")]
    AnotherGeneration {
        /// Which generation the request named.
        expected: u64,
        /// Which generation the event names.
        named: u64,
    },
    /// A terminal event correlates to another submission.
    #[error("this terminal event names a submission this stream did not make")]
    AnotherSubmission,
    /// A terminal event carries no contract correlation at all.
    #[error("an ending is the one event worth authenticating, and this carries no correlation")]
    TerminalWithoutCorrelation,
    /// A correlation arrived on an event that does not end anything.
    #[error("a correlation on a non-terminal event correlates nothing")]
    CorrelationOnNonTerminal,
    /// The correlation names contracts this build does not have.
    #[error(transparent)]
    Provenance(#[from] WireRefusal),
}

/// Where one event sits in the stream, as the stream itself counts.
///
/// A separate value from a job's sequence, and deliberately opaque. The cursor
/// orders one subscription's whole stream; a sequence orders one job's events.
/// Deriving either from the other would make a reconnection resume at a
/// position the agent never issued.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct EventStreamCursor {
    /// The bytes the agent issued, unread.
    spelling: String,
}

impl EventStreamCursor {
    /// Returns the cursor `spelling` names, if it fits.
    ///
    /// # Errors
    ///
    /// Returns [`StreamRefusal::IdentifierTooLong`].
    pub fn new(spelling: &str, allowed: u64) -> Result<Self, StreamRefusal> {
        if u64::try_from(spelling.len()).unwrap_or(u64::MAX) > allowed {
            return Err(StreamRefusal::IdentifierTooLong { allowed, actual: spelling.len() });
        }
        Ok(Self { spelling: spelling.to_owned() })
    }

    /// Returns this cursor's bytes.
    #[must_use]
    pub fn as_text(&self) -> &str {
        &self.spelling
    }
}

/// What a terminal event says about which submission it ends.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TerminalCorrelation {
    /// Which contracts the ending was produced under.
    pub provenance: DocumentProvenance,
    /// Which submission it ends.
    pub submitted_command_digest: String,
}

/// What the request this stream answers asked for.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StreamExpectation {
    /// Which incarnation of the store the request named.
    pub agent_event_store_generation: u64,
    /// Which subscription the request named.
    pub daemon_subscription_identifier: String,
    /// Which contracts this build has.
    pub expected_provenance: ExpectedProvenance,
    /// Which submission this stream is about.
    pub submitted_command_digest: String,
}

/// One event document, exactly as the agent writes it.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
struct JobEventDocument {
    /// Which incarnation of the store it came from.
    agent_event_store_generation: u64,
    /// Which operation it is about.
    agent_operation_identifier: String,
    /// Which subscription delivered it.
    daemon_subscription_identifier: String,
    /// What happened.
    kind: JobEventKind,
    /// Where it sits in that operation's own sequence.
    sequence: u64,
    /// What it says about the submission it ends, when it ends one.
    #[serde(default)]
    terminal: Option<TerminalCorrelation>,
}

/// One authenticated event, with everything the stream said around it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DecodedEvent {
    /// Where this event sits in the stream, when the agent said.
    pub cursor: Option<EventStreamCursor>,
    /// What happened to which operation, in that operation's own order.
    pub event: JobEvent,
    /// What the agent called this event.
    pub name: String,
    /// What it says about the submission it ends, when it ends one.
    pub terminal: Option<TerminalCorrelation>,
}

/// What one complete unit of the stream turned out to be.
///
/// The event is held behind an indirection because the two units are nothing
/// like the same size: a heartbeat is the absence of news, and every item in
/// every batch would otherwise be as large as a fully correlated ending.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StreamItem {
    /// A comment, which says only that the connection is alive.
    Heartbeat,
    /// One authenticated event about one job.
    Event(Box<DecodedEvent>),
}

/// One event stream being read, a byte at a time.
#[derive(Debug)]
pub struct ServerSentEventDecoder {
    /// Whether the previous byte was a carriage return.
    after_carriage_return: bool,
    /// The bounds this stream is held to.
    bounds: DecoderBounds,
    /// The data field's parts, joined as they arrive.
    data: String,
    /// What the current event is called.
    event_name: String,
    /// How many bytes the current event's field lines come to.
    event_bytes: usize,
    /// What the request asked for.
    expectation: StreamExpectation,
    /// The cursor the current event carries.
    identifier: Option<String>,
    /// Bytes of the line being read.
    pending: Vec<u8>,
    /// Whether any field line has been seen since the last blank line.
    saw_field: bool,
}

impl ServerSentEventDecoder {
    /// Returns a decoder attached to a response this policy accepts.
    ///
    /// The head is settled before a byte of body is read. A stream this daemon
    /// would refuse after decoding it is a stream it has already spent memory
    /// on, and refusing it then would be refusing it too late.
    ///
    /// # Errors
    ///
    /// Returns [`StreamRefusal::Head`], [`StreamRefusal::MediaType`], or
    /// [`StreamRefusal::MediaParameters`].
    pub fn attached(
        head: &ResponseHead,
        media_type: &str,
        bounds: DecoderBounds,
        expectation: StreamExpectation,
    ) -> Result<Self, StreamRefusal> {
        head.require_acceptable()?;
        require_event_stream(media_type)?;
        Ok(Self {
            after_carriage_return: false,
            bounds,
            data: String::new(),
            event_name: String::new(),
            event_bytes: 0,
            expectation,
            identifier: None,
            pending: Vec::new(),
            saw_field: false,
        })
    }

    /// Returns everything `chunk` completes.
    ///
    /// Byte-wise, so the answer does not depend on how the transport happened
    /// to split the stream. A refusal discards every partial line and event
    /// this decoder was holding, because state accumulated before a protocol
    /// error is state whose meaning nobody can vouch for.
    ///
    /// # Errors
    ///
    /// Returns the first [`StreamRefusal`] the bytes produce.
    pub fn push(&mut self, chunk: &[u8]) -> Result<Vec<StreamItem>, StreamRefusal> {
        let mut items = Vec::new();
        for byte in chunk {
            match self.absorb(*byte) {
                Ok(Some(item)) => items.push(item),
                Ok(None) => {}
                Err(refusal) => {
                    self.discard();
                    return Err(refusal);
                }
            }
        }
        Ok(items)
    }

    /// Returns whether bytes remain that never completed an event.
    ///
    /// An event is what arrives before a blank line. Bytes without one are a
    /// connection that ended mid-sentence, and emitting them would be inventing
    /// the rest of the sentence.
    #[must_use]
    pub fn has_partial_event(&self) -> bool {
        !self.pending.is_empty() || self.saw_field
    }

    /// Returns what a trailer section nobody declared means for this stream.
    ///
    /// A transport failure, handled by reconnecting. It is not a field, and it
    /// carries no cursor: treating it as either would let the framing layer
    /// write into the stream's own record of where it has got to.
    #[must_use]
    pub fn undeclared_trailer() -> StreamRefusal {
        StreamRefusal::UndeclaredTrailer
    }

    /// Returns what one more byte completes.
    fn absorb(&mut self, byte: u8) -> Result<Option<StreamItem>, StreamRefusal> {
        if byte == CARRIAGE_RETURN {
            self.after_carriage_return = true;
            return self.complete_line();
        }
        if byte == LINE_FEED {
            if std::mem::take(&mut self.after_carriage_return) {
                return Ok(None);
            }
            return self.complete_line();
        }
        self.after_carriage_return = false;
        self.pending.push(byte);
        let reached = self.pending.len();
        if u64::try_from(reached).unwrap_or(u64::MAX) > self.bounds.line_bytes {
            return Err(StreamRefusal::LineTooLong {
                allowed: self.bounds.line_bytes,
                actual: reached,
            });
        }
        Ok(None)
    }

    /// Returns what the line that just ended completes.
    fn complete_line(&mut self) -> Result<Option<StreamItem>, StreamRefusal> {
        let bytes = std::mem::take(&mut self.pending);
        let line = String::from_utf8(bytes).map_err(|_| StreamRefusal::NotUnicode)?;
        if line.is_empty() {
            return self.dispatch();
        }
        if line.starts_with(FIELD_SEPARATOR) {
            return Ok(Some(StreamItem::Heartbeat));
        }
        self.event_bytes += line.len();
        if u64::try_from(self.event_bytes).unwrap_or(u64::MAX) > self.bounds.event_bytes {
            return Err(StreamRefusal::EventTooLarge {
                allowed: self.bounds.event_bytes,
                actual: self.event_bytes,
            });
        }
        self.absorb_field(&line)?;
        Ok(None)
    }

    /// Records one field line.
    ///
    /// [`RETRY_FIELD`] and every unknown field are ignored: a suggested
    /// reconnection delay is the reconnection policy's business, and a field
    /// this build does not know is one the protocol says to skip rather than
    /// one to refuse a stream over.
    fn absorb_field(&mut self, line: &str) -> Result<(), StreamRefusal> {
        let (name, value) = match line.split_once(FIELD_SEPARATOR) {
            Some((name, raw)) => (name, raw.strip_prefix(' ').unwrap_or(raw)),
            None => (line, ""),
        };
        match name {
            DATA_FIELD => {
                self.data.push_str(value);
                self.data.push(LINE_FEED_CHARACTER);
            }
            EVENT_FIELD => self.event_name = value.to_owned(),
            IDENTIFIER_FIELD => {
                EventStreamCursor::new(value, self.bounds.identifier_bytes)?;
                self.identifier = Some(value.to_owned());
            }
            _ => return Ok(()),
        }
        self.saw_field = true;
        Ok(())
    }

    /// Returns the event a blank line ends, when it ends one.
    fn dispatch(&mut self) -> Result<Option<StreamItem>, StreamRefusal> {
        if !self.saw_field {
            self.discard();
            return Ok(None);
        }
        let payload = self.data.strip_suffix(LINE_FEED_CHARACTER).unwrap_or(&self.data).to_owned();
        let name = std::mem::take(&mut self.event_name);
        let identifier = self.identifier.take();
        self.discard();
        let document: JobEventDocument = serde_json::from_str(&payload)
            .map_err(|_| StreamRefusal::Malformed { field: "payload" })?;
        self.require_requested(&document)?;
        self.require_correlated(&document)?;
        let cursor = match identifier {
            Some(spelling) => {
                Some(EventStreamCursor::new(&spelling, self.bounds.identifier_bytes)?)
            }
            None => None,
        };
        Ok(Some(StreamItem::Event(Box::new(DecodedEvent {
            cursor,
            event: JobEvent {
                agent_event_store_generation: document.agent_event_store_generation,
                agent_operation_identifier: document.agent_operation_identifier,
                kind: document.kind,
                sequence: document.sequence,
            },
            name,
            terminal: document.terminal,
        }))))
    }

    /// Requires one document to belong to the stream that was asked for.
    fn require_requested(&self, document: &JobEventDocument) -> Result<(), StreamRefusal> {
        if document.daemon_subscription_identifier
            != self.expectation.daemon_subscription_identifier
        {
            return Err(StreamRefusal::AnotherSubscription);
        }
        if document.agent_event_store_generation != self.expectation.agent_event_store_generation {
            return Err(StreamRefusal::AnotherGeneration {
                expected: self.expectation.agent_event_store_generation,
                named: document.agent_event_store_generation,
            });
        }
        Ok(())
    }

    /// Requires an ending to authenticate itself, and nothing else to try.
    fn require_correlated(&self, document: &JobEventDocument) -> Result<(), StreamRefusal> {
        match (&document.terminal, document.kind.is_terminal()) {
            (Some(terminal), true) => {
                self.expectation.expected_provenance.require_matching(&terminal.provenance)?;
                if terminal.submitted_command_digest != self.expectation.submitted_command_digest {
                    return Err(StreamRefusal::AnotherSubmission);
                }
                Ok(())
            }
            (None, false) => Ok(()),
            (None, true) => Err(StreamRefusal::TerminalWithoutCorrelation),
            (Some(_), false) => Err(StreamRefusal::CorrelationOnNonTerminal),
        }
    }

    /// Forgets every part of an event that is no longer being assembled.
    fn discard(&mut self) {
        self.data.clear();
        self.event_bytes = 0;
        self.event_name.clear();
        self.identifier = None;
        self.pending.clear();
        self.saw_field = false;
    }
}

/// Requires one response to announce exactly one event stream.
///
/// Exactly one: a header naming the type twice, or naming it beside something
/// else, is a server that has not decided what it is sending, and a decoder
/// that picked one reading would be deciding for it.
///
/// # Errors
///
/// Returns [`StreamRefusal::MediaType`] or [`StreamRefusal::MediaParameters`].
pub fn require_event_stream(named: &str) -> Result<(), StreamRefusal> {
    let lowered = named.trim().to_ascii_lowercase();
    let (essence, parameters) = match lowered.split_once(MEDIA_PARAMETER_SEPARATOR) {
        Some((essence, rest)) => (essence.trim(), rest.trim()),
        None => (lowered.as_str(), ""),
    };
    if essence != EVENT_STREAM_MEDIA_TYPE {
        return Err(StreamRefusal::MediaType { named: named.to_owned() });
    }
    if !PERMITTED_MEDIA_PARAMETERS.contains(&parameters) {
        return Err(StreamRefusal::MediaParameters { named: named.to_owned() });
    }
    Ok(())
}
