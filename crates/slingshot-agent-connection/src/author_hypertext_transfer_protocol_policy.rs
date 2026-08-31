//! What this daemon will and will not accept from an author, at the wire.
//!
//! Everything here is a refusal, and the refusals share a shape: they happen
//! before a response can influence anything. A header limit checked after the
//! headers were collected is not a limit; a protocol check made after a body
//! arrived is not a check. So the bounds are enforced incrementally, as input
//! arrives, and a response is rejected the moment the next field would cross
//! one rather than once the whole head is in hand.
//!
//! Two protocol versions, and no way to change to another mid-connection. An
//! upgrade, an alternative-service migration, or an informational head that
//! precedes a real one are all ways for a server to move a conversation
//! somewhere the policy was not applied, so all of them are refused rather than
//! followed.
//!
//! Redirects are disabled everywhere, including same-origin ones. A redirect is
//! a server telling a client to ask somewhere else, and the whole point of
//! selecting one author origin is that nothing else gets asked.

use slingshot_domain::author_agent_transport_contract::AuthorAgentTransportContract;

/// The protocol versions this daemon speaks to an author.
pub const PERMITTED_PROTOCOL_VERSIONS: &[&str] = &["HTTP/1.1", "HTTP/2"];

/// Content codings this daemon accepts, which is the one that means none.
///
/// Automatic decompression is disabled, so a coding the client did not ask for
/// is a body of unknown decoded length - and a bound on an unknown length is
/// not a bound.
pub const PERMITTED_CONTENT_CODINGS: &[&str] = &["identity"];

/// Why a response was refused before it could influence anything.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ResponseRefusal {
    /// The response arrived over a protocol this daemon does not speak.
    #[error("this daemon speaks {PERMITTED_PROTOCOL_VERSIONS:?}, and this arrived over {named}")]
    ProtocolVersion {
        /// What it arrived over.
        named: String,
    },
    /// The server tried to move the conversation elsewhere.
    #[error("this response tries to move the conversation with {mechanism}, which is not followed")]
    MigrationAttempted {
        /// Which mechanism it used.
        mechanism: &'static str,
    },
    /// One header field is beyond its bound.
    #[error("one header field holds at most {allowed} bytes, and this holds {actual}")]
    FieldTooLong {
        /// How long it may be.
        allowed: u64,
        /// How long it was.
        actual: usize,
    },
    /// There are more header fields than the bound allows.
    #[error("a response head carries at most {allowed} fields, and this carries {actual}")]
    TooManyFields {
        /// How many it may carry.
        allowed: u64,
        /// How many it carries.
        actual: usize,
    },
    /// The whole head is beyond its bound.
    #[error("a response head holds at most {allowed} bytes, and this holds {actual}")]
    HeadTooLong {
        /// How long it may be.
        allowed: u64,
        /// How long it was.
        actual: usize,
    },
    /// The body carries a coding this daemon did not ask for.
    #[error(
        "this response is encoded as {named}, and a body of unknown decoded length has no bound"
    )]
    UnexpectedContentCoding {
        /// What it is encoded as.
        named: String,
    },
    /// The response declares trailers.
    #[error("this response declares trailers, which arrive after a body this daemon has acted on")]
    TrailersDeclared,
    /// A redirect was offered.
    #[error(
        "this response redirects to {location}, and selecting one author origin means asking nowhere else"
    )]
    RedirectOffered {
        /// Where it points.
        location: String,
    },
}

/// The bounds one response head is held to, read from the transport contract.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HeadBounds {
    /// Bytes one decoded field name and value may occupy together.
    pub field_bytes: u64,
    /// Fields one head may carry.
    pub field_count: u64,
    /// Bytes the whole head may occupy.
    pub head_bytes: u64,
}

impl HeadBounds {
    /// Returns the bounds the transport contract names.
    #[must_use]
    pub fn embedded() -> Self {
        let contract = AuthorAgentTransportContract::embedded();
        Self {
            field_bytes: contract.limit("maximum_author_response_header_bytes"),
            field_count: contract.limit("maximum_author_response_header_count"),
            head_bytes: contract.limit("maximum_author_response_head_bytes"),
        }
    }
}

/// One response head being read, field by field.
///
/// Incremental on purpose. A limit applied to a collection that has already
/// been built is a limit on nothing: the memory was spent before it was
/// checked, which is exactly what the bound exists to prevent.
#[derive(Debug)]
pub struct HeadReader {
    /// The bounds this head is held to.
    bounds: HeadBounds,
    /// How many bytes have arrived.
    bytes: usize,
    /// How many fields have arrived.
    fields: usize,
}

impl HeadReader {
    /// Returns a reader held to `bounds`, with nothing read.
    #[must_use]
    pub fn new(bounds: HeadBounds) -> Self {
        Self { bounds, bytes: 0, fields: 0 }
    }

    /// Reads one field, or refuses because it would cross a bound.
    ///
    /// # Errors
    ///
    /// Returns [`ResponseRefusal::FieldTooLong`],
    /// [`ResponseRefusal::TooManyFields`], or [`ResponseRefusal::HeadTooLong`],
    /// whichever this field crosses first.
    pub fn read_field(&mut self, name: &str, value: &str) -> Result<(), ResponseRefusal> {
        let field = name.len() + value.len();
        if u64::try_from(field).unwrap_or(u64::MAX) > self.bounds.field_bytes {
            return Err(ResponseRefusal::FieldTooLong {
                allowed: self.bounds.field_bytes,
                actual: field,
            });
        }
        let fields = self.fields + 1;
        if u64::try_from(fields).unwrap_or(u64::MAX) > self.bounds.field_count {
            return Err(ResponseRefusal::TooManyFields {
                allowed: self.bounds.field_count,
                actual: fields,
            });
        }
        let bytes = self.bytes + field;
        if u64::try_from(bytes).unwrap_or(u64::MAX) > self.bounds.head_bytes {
            return Err(ResponseRefusal::HeadTooLong {
                allowed: self.bounds.head_bytes,
                actual: bytes,
            });
        }
        self.fields = fields;
        self.bytes = bytes;
        Ok(())
    }

    /// Returns how many fields have been read.
    #[must_use]
    pub fn fields(&self) -> usize {
        self.fields
    }

    /// Returns how many bytes have been read.
    #[must_use]
    pub fn bytes(&self) -> usize {
        self.bytes
    }
}

/// What one response says about itself, before its body is read.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResponseHead {
    /// The content coding it declares, if any.
    pub content_coding: Option<String>,
    /// Where it redirects, if it does.
    pub location: Option<String>,
    /// The protocol it arrived over.
    pub protocol_version: String,
    /// Whether it declares trailers.
    pub trailers_declared: bool,
    /// Whether it offers an alternative service.
    pub alternative_service_offered: bool,
    /// Whether it is an informational head preceding a real one.
    pub informational: bool,
}

impl ResponseHead {
    /// Requires this response to be one the policy accepts.
    ///
    /// Ordered so a caller learns the most fundamental thing that is wrong. A
    /// response over an unspoken protocol is refused before its headers are
    /// considered at all, because nothing about a conversation this daemon is
    /// not having is worth interpreting.
    ///
    /// # Errors
    ///
    /// Returns [`ResponseRefusal`] naming the first thing that is wrong.
    pub fn require_acceptable(&self) -> Result<(), ResponseRefusal> {
        if !PERMITTED_PROTOCOL_VERSIONS.contains(&self.protocol_version.as_str()) {
            return Err(ResponseRefusal::ProtocolVersion { named: self.protocol_version.clone() });
        }
        if self.informational {
            return Err(ResponseRefusal::MigrationAttempted { mechanism: "an informational head" });
        }
        if self.alternative_service_offered {
            return Err(ResponseRefusal::MigrationAttempted {
                mechanism: "an alternative service",
            });
        }
        if self.trailers_declared {
            return Err(ResponseRefusal::TrailersDeclared);
        }
        if let Some(coding) = &self.content_coding
            && !PERMITTED_CONTENT_CODINGS.contains(&coding.as_str())
        {
            return Err(ResponseRefusal::UnexpectedContentCoding { named: coding.clone() });
        }
        if let Some(location) = &self.location {
            return Err(ResponseRefusal::RedirectOffered { location: location.clone() });
        }
        Ok(())
    }
}

/// The deadlines one author exchange is held to, each for its own phase.
///
/// Separate rather than one total, because they answer different questions. A
/// name that will not resolve is a different problem from a handshake that
/// stalls, and both are different from a server that accepted a request and
/// then said nothing. One deadline covering all of them would be long enough
/// that the fast failures took as long as the slow ones.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ExchangeDeadlines {
    /// Establishing the connection.
    pub connect_milliseconds: u64,
    /// Completing the security handshake.
    pub transport_layer_security_milliseconds: u64,
    /// Sending the request body.
    pub request_body_milliseconds: u64,
    /// Waiting for the response head.
    pub response_header_milliseconds: u64,
    /// Reading a finite response, in total.
    pub finite_total_milliseconds: u64,
    /// Waiting between bytes of a finite response.
    pub finite_idle_milliseconds: u64,
}

impl ExchangeDeadlines {
    /// Returns the deadlines the transport contract names.
    #[must_use]
    pub fn embedded() -> Self {
        let contract = AuthorAgentTransportContract::embedded();
        Self {
            connect_milliseconds: contract.limit("author_connect_timeout_milliseconds"),
            transport_layer_security_milliseconds: contract
                .limit("author_tls_timeout_milliseconds"),
            request_body_milliseconds: contract.limit("author_request_body_timeout_milliseconds"),
            response_header_milliseconds: contract
                .limit("author_response_header_timeout_milliseconds"),
            finite_total_milliseconds: contract.limit("finite_response_total_timeout_milliseconds"),
            finite_idle_milliseconds: contract.limit("finite_response_idle_timeout_milliseconds"),
        }
    }
}

/// Returns how long to wait before retrying, honouring a server's request.
///
/// A server asking for longer is honoured up to a cap. Without one, a server
/// could park a client indefinitely by asking it to; with one, the worst a
/// server can do is delay a retry by a bounded amount.
#[must_use]
pub fn retry_delay_milliseconds(requested: Option<u64>) -> u64 {
    let contract = AuthorAgentTransportContract::embedded();
    let cap = contract.limit("retry_after_cap_milliseconds");
    match requested {
        Some(asked) => asked.min(cap),
        None => contract.limit("retry_base_milliseconds"),
    }
}
