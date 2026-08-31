//! Answering clients that speak the revision this build prefers.
//!
//! A stateless client sends what it needs with every request and expects
//! discovery, invocation, and errors in this revision's own shapes. There is no
//! session to establish and nothing to remember between requests, which is the
//! property the whole era is built around: a client that reconnects mid-stream
//! resumes by asking again rather than by re-establishing anything.
//!
//! # Eligibility is decided per request, from that request alone
//!
//! Every request carries the revision it speaks. Nothing earlier in the stream
//! changes how a later request is read, because a server whose answers depended
//! on request order would answer the same request differently depending on what
//! a client happened to send first - and neither side could say which answer
//! was right.
//!
//! # This server sends no requests
//!
//! It answers requests and emits progress notifications. A server-initiated
//! request would need a client that answers one, and a stateless client is not
//! obliged to be listening for anything it did not ask for.

use serde_json::{Value, json};

use crate::model_context_protocol::standard_stream_transport::{
    ProtocolRevision, SUPPORTED_REVISIONS,
};

/// Every request this revision answers.
pub const EVERY_REQUEST: &[&str] = &[
    "server/discover",
    "ping",
    "tools/list",
    "tools/call",
    "resources/list",
    "resources/templates/list",
    "resources/read",
];

/// Every notification this revision accepts from a client.
pub const EVERY_INBOUND_NOTIFICATION: &[&str] = &["notifications/cancelled"];

/// Every notification this revision sends to a client.
pub const EVERY_OUTBOUND_NOTIFICATION: &[&str] = &["notifications/progress"];

/// The capabilities this server advertises, and it advertises no others.
pub const EVERY_CAPABILITY: &[&str] = &["tools", "resources"];

/// The member every request carries to say which revision it speaks.
pub const REVISION_MEMBER: &str = "protocolVersion";

/// The member every successful result carries to say it is whole.
pub const COMPLETE_MEMBER: &str = "resultType";

/// What that member says.
pub const COMPLETE_VALUE: &str = "complete";

/// The member a listing carries to say how long it stays usable.
pub const LIFETIME_MEMBER: &str = "ttlMs";

/// The member a listing carries to say who may hold it.
pub const CACHE_SCOPE_MEMBER: &str = "cacheScope";

/// How long a listing stays usable, in milliseconds.
pub const LISTING_LIFETIME_MILLISECONDS: u64 = 60_000;

/// Who may hold a listing.
pub const LISTING_CACHE_SCOPE: &str = "session";

/// The error a line that is not readable receives.
pub const PARSE_ERROR: i64 = -32_700;

/// The error a readable line that is not a request receives.
pub const INVALID_REQUEST_ERROR: i64 = -32_600;

/// The error a request naming something this server does not offer receives.
pub const METHOD_NOT_FOUND_ERROR: i64 = -32_601;

/// The error a request whose arguments are unusable receives.
pub const INVALID_PARAMETERS_ERROR: i64 = -32_602;

/// The error a request this server failed to answer receives.
pub const INTERNAL_ERROR: i64 = -32_603;

/// The error a request naming a revision this build does not serve receives.
pub const UNSUPPORTED_REVISION_ERROR: i64 = -32_022;

/// Every error a client may receive over the standard streams.
pub const EVERY_ERROR: &[i64] = &[
    PARSE_ERROR,
    INVALID_REQUEST_ERROR,
    METHOD_NOT_FOUND_ERROR,
    INVALID_PARAMETERS_ERROR,
    INTERNAL_ERROR,
    UNSUPPORTED_REVISION_ERROR,
];

/// Why one request is not answered.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Refusal {
    /// The request names a revision this build does not serve.
    RevisionUnsupported {
        /// Exactly what it asked for.
        requested: String,
    },
    /// The request names something this server does not offer.
    MethodUnavailable {
        /// Exactly what it asked for.
        named: String,
    },
    /// The request is missing something it cannot be answered without.
    ParametersUnusable {
        /// What is wrong with them.
        detail: String,
    },
}

impl Refusal {
    /// Returns the code a client receives for this refusal.
    #[must_use]
    pub fn code(&self) -> i64 {
        match self {
            Self::RevisionUnsupported { .. } => UNSUPPORTED_REVISION_ERROR,
            Self::MethodUnavailable { .. } => METHOD_NOT_FOUND_ERROR,
            Self::ParametersUnusable { .. } => INVALID_PARAMETERS_ERROR,
        }
    }

    /// Returns the error object a client receives, exactly.
    ///
    /// An unsupported revision carries the same ordered list discovery carries,
    /// so a client told no can see what to ask for instead without a second
    /// round trip.
    #[must_use]
    pub fn rendered(&self) -> Value {
        match self {
            Self::RevisionUnsupported { requested } => json!({
                "code": self.code(),
                "message": "this build serves neither of the revisions this request names",
                "data": { "requested": requested, "supported": SUPPORTED_REVISIONS },
            }),
            Self::MethodUnavailable { named } => json!({
                "code": self.code(),
                "message": format!("this server offers no {named}"),
            }),
            Self::ParametersUnusable { detail } => json!({
                "code": self.code(),
                "message": detail,
            }),
        }
    }
}

/// Requires one request to be one this server answers, in a revision it serves.
///
/// The revision is checked before the method, because a client speaking a
/// revision this build does not serve is told that rather than told its method
/// is unknown - which would be true only by accident.
///
/// # Errors
///
/// Returns [`Refusal`] naming the first thing that stops the request.
pub fn require_answerable(method: &str, requested_revision: &str) -> Result<(), Refusal> {
    if ProtocolRevision::named(requested_revision) != Some(ProtocolRevision::Current) {
        return Err(Refusal::RevisionUnsupported { requested: requested_revision.to_owned() });
    }
    if !EVERY_REQUEST.contains(&method) {
        return Err(Refusal::MethodUnavailable { named: method.to_owned() });
    }
    Ok(())
}

/// Returns what this server says about itself.
#[must_use]
pub fn discovery() -> Value {
    json!({
        "supportedVersions": SUPPORTED_REVISIONS,
        "capabilities": { "tools": {}, "resources": {} },
    })
}

/// Returns one semantic payload decorated as this revision requires.
///
/// Every successful result says it is whole, including a tool call whose own
/// answer reports a failure: the call completed and the failure is its result,
/// and a client that read those two facts as one would retry work that ran.
#[must_use]
pub fn decorated(method: &str, payload: Value) -> Value {
    let mut result = payload;
    let Some(object) = result.as_object_mut() else {
        return json!({ COMPLETE_MEMBER: COMPLETE_VALUE, "value": result });
    };
    object.insert(COMPLETE_MEMBER.to_owned(), json!(COMPLETE_VALUE));
    if LISTING_METHODS.contains(&method) {
        object.insert(LIFETIME_MEMBER.to_owned(), json!(LISTING_LIFETIME_MILLISECONDS));
        object.insert(CACHE_SCOPE_MEMBER.to_owned(), json!(LISTING_CACHE_SCOPE));
    }
    result
}

/// The requests whose results a client may hold for a while.
const LISTING_METHODS: &[&str] =
    &["tools/list", "resources/list", "resources/templates/list", "resources/read"];
