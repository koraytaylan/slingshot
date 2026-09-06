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
    /// This connection's decoder previously failed and must not be resumed.
    #[error("the event stream decoder is closed after a refusal")]
    Closed,
    /// The consumer could not accept one independently validated stream item.
    #[error("the event stream consumer refused an item")]
    Consumer,
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

pub use slingshot_agent_protocol::job_event_document::TerminalCorrelation;
use slingshot_agent_protocol::job_event_document::{JobEventDocument, JobEventState};

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

/// Independently retained correlation for one operation, resolved by its key.
/// The decoder checks the returned key as well as every provenance/digest field.
pub struct OperationStreamExpectation {
    /// The retained subscription, checked independently of the operation key.
    pub daemon_subscription_identifier: String,
    /// The retained generation, never inferred from another identity.
    pub agent_event_store_generation: u64,
    /// The operation actually read from retained storage.
    pub agent_operation_identifier: String,
    /// Its independently retained/installed command provenance.
    pub expected_provenance: ExpectedProvenance,
    /// Its unchanged submitted-command digest.
    pub submitted_command_digest: String,
}

impl core::fmt::Debug for OperationStreamExpectation {
    fn fmt(&self, formatter: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        formatter.write_str("OperationStreamExpectation([redacted])")
    }
}

/// Resolves terminal correlation from retained state, not from the event's
/// claimed contract or digest. Failure stops delivery and cursor advancement.
pub trait TerminalExpectationResolver {
    /// Read the named operation's immutable correlation, or refuse resolution.
    fn resolve(&mut self, operation: &str) -> Result<OperationStreamExpectation, StreamRefusal>;
}

impl<F: FnMut(&str) -> Result<OperationStreamExpectation, StreamRefusal>>
    TerminalExpectationResolver for F
{
    fn resolve(&mut self, operation: &str) -> Result<OperationStreamExpectation, StreamRefusal> {
        self(operation)
    }
}

/// Fixed correlation retained for the legacy single-command decoder constructor.
/// Product subscription streams must use per-operation resolution instead.
pub struct FixedTerminalExpectation {
    subscription: String,
    generation: u64,
    provenance: ExpectedProvenance,
    digest: String,
}

impl TerminalExpectationResolver for FixedTerminalExpectation {
    fn resolve(&mut self, operation: &str) -> Result<OperationStreamExpectation, StreamRefusal> {
        Ok(OperationStreamExpectation {
            daemon_subscription_identifier: self.subscription.clone(),
            agent_event_store_generation: self.generation,
            agent_operation_identifier: operation.to_owned(),
            expected_provenance: self.provenance.clone(),
            submitted_command_digest: self.digest.clone(),
        })
    }
}

/// One authenticated event, with everything the stream said around it.
#[derive(Clone, PartialEq, Eq)]
pub struct DecodedEvent {
    /// Subscription authenticated by the decoder, retained for durable binding.
    pub daemon_subscription_identifier: String,
    /// Digest of the complete canonical event document, excluding SSE framing.
    pub canonical_digest: String,
    /// UTF-8 byte count of that complete canonical document for ledger accounting.
    pub canonical_bytes: u64,
    /// Bounded physical Sling job reporting this event.
    pub sling_job_identifier: String,
    /// Explicit, kind-consistent logical state.
    pub state: JobEventState,
    /// Optional monotonic remote attempt; absence is not zero.
    pub attempt: Option<u64>,
    /// Optional monotonic logical progress; absence is not zero.
    pub progress: Option<u64>,
    /// Where this event sits in the stream, when the agent said.
    pub cursor: Option<EventStreamCursor>,
    /// What happened to which operation, in that operation's own order.
    pub event: JobEvent,
    /// What the agent called this event.
    pub name: String,
    /// What it says about the submission it ends, when it ends one.
    pub terminal: Option<TerminalCorrelation>,
}

impl core::fmt::Debug for DecodedEvent {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str("DecodedEvent([redacted])")
    }
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
pub struct ServerSentEventDecoder<R = FixedTerminalExpectation> {
    /// Failed decoding or delivery closes this connection permanently.
    poisoned: bool,
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
    subscription: String,
    generation: u64,
    resolver: R,
    /// The cursor the current event carries.
    identifier: Option<String>,
    /// Bytes of the line being read.
    pending: Vec<u8>,
    /// Whether any field line has been seen since the last blank line.
    saw_field: bool,
}

impl<R> core::fmt::Debug for ServerSentEventDecoder<R> {
    fn fmt(&self, formatter: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        formatter.write_str("ServerSentEventDecoder([redacted])")
    }
}

impl ServerSentEventDecoder {
    /// An actual undeclared trailer is a transport failure, never an event or
    /// cursor fact. Previously committed valid events are not retracted.
    pub fn undeclared_trailer() -> StreamRefusal {
        StreamRefusal::UndeclaredTrailer
    }
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
        let resolver = FixedTerminalExpectation {
            subscription: expectation.daemon_subscription_identifier.clone(),
            generation: expectation.agent_event_store_generation,
            provenance: expectation.expected_provenance,
            digest: expectation.submitted_command_digest,
        };
        Self::attached_subscription(
            head,
            media_type,
            bounds,
            expectation.daemon_subscription_identifier,
            expectation.agent_event_store_generation,
            resolver,
        )
    }
}

impl<R: TerminalExpectationResolver> ServerSentEventDecoder<R> {
    /// Attaches one filtered subscription with independently resolved terminal
    /// expectations for every operation. Resolution happens only after closed
    /// event decoding and subscription/generation checks, and before delivery.
    pub fn attached_subscription(
        head: &ResponseHead,
        media_type: &str,
        bounds: DecoderBounds,
        subscription: String,
        generation: u64,
        resolver: R,
    ) -> Result<Self, StreamRefusal> {
        head.require_acceptable()?;
        require_event_stream(media_type)?;
        Ok(Self {
            poisoned: false,
            after_carriage_return: false,
            bounds,
            data: String::new(),
            event_name: String::new(),
            event_bytes: 0,
            subscription,
            generation,
            resolver,
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
        self.push_each(chunk, |item| {
            items.push(item);
            Ok(())
        })?;
        Ok(items)
    }

    /// Delivers each complete validated item immediately, without accumulating
    /// a batch. Live transports use this boundary so a later malformed item
    /// cannot discard independently committed earlier items in the same chunk.
    /// A failed callback stops before any later byte is consumed. The decoder
    /// is permanently closed on either decoding or consumer refusal; reopening
    /// requires a new connection and the caller's last durably committed cursor.
    pub fn push_each(
        &mut self,
        chunk: &[u8],
        mut consume: impl FnMut(StreamItem) -> Result<(), StreamRefusal>,
    ) -> Result<(), StreamRefusal> {
        if self.poisoned {
            return Err(StreamRefusal::Closed);
        }
        // Keep a decoder closed even if a consumer unwinds and its caller
        // catches the panic. Only a completely successful push reopens it.
        self.poisoned = true;
        for byte in chunk {
            match self.absorb(*byte) {
                Ok(Some(item)) => {
                    if let Err(refusal) = consume(item) {
                        self.discard();
                        self.poisoned = true;
                        return Err(refusal);
                    }
                }
                Ok(None) => {}
                Err(refusal) => {
                    self.discard();
                    self.poisoned = true;
                    return Err(refusal);
                }
            }
        }
        self.poisoned = false;
        Ok(())
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
        let reached = self.pending.len().saturating_add(1);
        if u64::try_from(reached).unwrap_or(u64::MAX) > self.bounds.line_bytes {
            return Err(StreamRefusal::LineTooLong {
                allowed: self.bounds.line_bytes,
                actual: reached,
            });
        }
        self.pending.push(byte);
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
        // Hash the full envelope before projecting it into job-specific fields.
        // Optional counters stay omitted: absence and explicit zero are distinct
        // accounts of an event even when they yield the same job observation.
        let value = serde_json::to_value(&document)
            .map_err(|_| StreamRefusal::Malformed { field: "payload" })?;
        let canonical = slingshot_domain::command::canonical_json::write_canonical(&value)
            .map_err(|_| StreamRefusal::Malformed { field: "payload" })?;
        let canonical_digest =
            slingshot_domain::command::canonical_json::canonical_digest(&canonical);
        let canonical_bytes = u64::try_from(canonical.len())
            .map_err(|_| StreamRefusal::Malformed { field: "payload" })?;
        let cursor = match identifier {
            Some(spelling) => {
                Some(EventStreamCursor::new(&spelling, self.bounds.identifier_bytes)?)
            }
            None => None,
        };
        Ok(Some(StreamItem::Event(Box::new(DecodedEvent {
            daemon_subscription_identifier: document.daemon_subscription_identifier,
            canonical_digest,
            canonical_bytes,
            sling_job_identifier: document.sling_job_identifier,
            state: document.state,
            attempt: document.attempt,
            progress: document.progress,
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
        slingshot_domain::remote_job::AgentJobIdentifier::new(&document.sling_job_identifier)
            .map_err(|_| StreamRefusal::Malformed { field: "sling_job_identifier" })?;
        let state = match document.kind {
            JobEventKind::Accepted => JobEventState::Queued,
            JobEventKind::Started | JobEventKind::Progress => JobEventState::Running,
            JobEventKind::Succeeded => JobEventState::Succeeded,
            JobEventKind::Failed => JobEventState::Failed,
        };
        if document.state != state {
            return Err(StreamRefusal::Malformed { field: "state" });
        }
        if document.daemon_subscription_identifier != self.subscription {
            return Err(StreamRefusal::AnotherSubscription);
        }
        if document.agent_event_store_generation != self.generation {
            return Err(StreamRefusal::AnotherGeneration {
                expected: self.generation,
                named: document.agent_event_store_generation,
            });
        }
        if document.agent_operation_identifier.len() != 64
            || !document
                .agent_operation_identifier
                .bytes()
                .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
        {
            return Err(StreamRefusal::Malformed { field: "agent_operation_identifier" });
        }
        Ok(())
    }

    /// Requires an ending to authenticate itself, and nothing else to try.
    fn require_correlated(&mut self, document: &JobEventDocument) -> Result<(), StreamRefusal> {
        match (&document.terminal, document.kind.is_terminal()) {
            (Some(terminal), true) => {
                let expected = self.resolver.resolve(&document.agent_operation_identifier)?;
                if expected.daemon_subscription_identifier != self.subscription {
                    return Err(StreamRefusal::AnotherSubscription);
                }
                if expected.agent_event_store_generation != self.generation {
                    return Err(StreamRefusal::AnotherGeneration {
                        expected: self.generation,
                        named: expected.agent_event_store_generation,
                    });
                }
                if expected.agent_operation_identifier != document.agent_operation_identifier {
                    return Err(StreamRefusal::AnotherSubmission);
                }
                let installed = ExpectedProvenance {
                    command_contract: slingshot_domain::selected_command_contract_identity::SelectedCommandContractIdentity::installed(
                        &expected.expected_provenance.command_contract.command_wire_name
                    ).map_err(|_| StreamRefusal::AnotherSubmission)?,
                    canonical_json_contract_digest: slingshot_domain::command::schema::canonical_contract_digest(),
                    transport_contract_digest: AuthorAgentTransportContract::embedded_digest(),
                };
                installed.require_matching(&expected.expected_provenance.provenance())?;
                if expected.submitted_command_digest.len() != 64
                    || !expected
                        .submitted_command_digest
                        .bytes()
                        .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
                {
                    return Err(StreamRefusal::AnotherSubmission);
                }
                expected.expected_provenance.require_matching(&terminal.provenance)?;
                if terminal.submitted_command_digest != expected.submitted_command_digest {
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
