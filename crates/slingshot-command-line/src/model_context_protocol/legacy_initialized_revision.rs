//! Answering clients that negotiate the older initialized revision.
//!
//! An older client establishes a session before it does anything else, and
//! speaks a different set of shapes afterwards. Keeping that apart from the
//! current revision is what stops one era's rules from quietly deciding the
//! other's behaviour - and the shapes really do differ, so a single handler
//! trying to serve both would decorate one era's results with the other's
//! members.
//!
//! # Nothing runs before the client says it is ready
//!
//! Initialization is a handshake with two halves: the client asks, the server
//! answers with the revision it will speak, and the client says it is
//! initialized. Only then may work be dispatched. A server that ran a tool call
//! arriving between the two halves would be acting on a session neither side
//! had agreed on yet.
//!
//! # A revision this era does not know is answered, not refused
//!
//! This era's clients predate the revision this build prefers, and one asking
//! for something else is not making a mistake - it is asking whether there is
//! common ground. So the answer offers this revision rather than an error, and
//! the client decides. That is the difference between the two eras' handling of
//! the same situation, and the reason the fsm executor can talk to this build.

use serde_json::{Value, json};

use crate::model_context_protocol::current_stateless_revision::{
    CACHE_SCOPE_MEMBER, COMPLETE_MEMBER, EVERY_REQUEST, LIFETIME_MEMBER, METHOD_NOT_FOUND_ERROR,
};
use crate::model_context_protocol::standard_stream_transport::ProtocolRevision;

/// The request that begins a session.
pub const INITIALIZE_REQUEST: &str = "initialize";

/// The notification that ends the handshake.
pub const INITIALIZED_NOTIFICATION: &str = "notifications/initialized";

/// The error a request arriving before the handshake ends receives.
pub const NOT_INITIALIZED_ERROR: i64 = -32_002;

/// Members this era's results never carry.
pub const MODERN_ONLY_MEMBERS: &[&str] = &[COMPLETE_MEMBER, LIFETIME_MEMBER, CACHE_SCOPE_MEMBER];

/// How far one session has got.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Lifecycle {
    /// Nothing has been asked yet.
    #[default]
    Fresh,
    /// The server answered an initialize and awaits the notification.
    Offered,
    /// The client said it is initialized, and work may be dispatched.
    Ready,
}

/// Why one legacy message is not acted on.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LegacyRefusal {
    /// The session has not finished its handshake.
    NotInitialized {
        /// What was asked for too early.
        named: String,
    },
    /// The request names something this server does not offer.
    MethodUnavailable {
        /// Exactly what it asked for.
        named: String,
    },
}

impl LegacyRefusal {
    /// Returns the code a client receives for this refusal.
    #[must_use]
    pub fn code(&self) -> i64 {
        match self {
            Self::NotInitialized { .. } => NOT_INITIALIZED_ERROR,
            Self::MethodUnavailable { .. } => METHOD_NOT_FOUND_ERROR,
        }
    }

    /// Returns the error object a client receives, exactly.
    #[must_use]
    pub fn rendered(&self) -> Value {
        let message = match self {
            Self::NotInitialized { named } => {
                format!("{named} waits until this session says it is initialized")
            }
            Self::MethodUnavailable { named } => format!("this server offers no {named}"),
        };
        json!({ "code": self.code(), "message": message })
    }
}

/// One legacy session, and what it has agreed so far.
#[derive(Debug, Clone, Copy, Default)]
pub struct LegacySession {
    /// How far the handshake has got.
    lifecycle: Lifecycle,
    /// Whether the client asked for the revision it was offered.
    echoed: bool,
}

impl LegacySession {
    /// Returns a session that has agreed nothing.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Returns how far this session has got.
    #[must_use]
    pub fn lifecycle(&self) -> Lifecycle {
        self.lifecycle
    }

    /// Reports whether the client asked for the revision it was offered.
    ///
    /// Kept on the session rather than put in the answer. The answer says which
    /// revision this session speaks, which is all a client needs; a member saying
    /// whether that matched the request would be one more thing for two
    /// implementations to disagree about.
    #[must_use]
    pub fn echoed_the_request(&self) -> bool {
        self.echoed
    }

    /// Answers one initialize, whatever revision it asked for.
    ///
    /// The requested revision is echoed when this era serves it and replaced by
    /// this era's own otherwise. Either way the answer is a successful one: a
    /// client from this era asking for something else is asking whether there
    /// is common ground, and there is.
    pub fn initialize(&mut self, requested: &str) -> Value {
        self.lifecycle = Lifecycle::Offered;
        self.echoed = ProtocolRevision::named(requested) == Some(ProtocolRevision::Legacy);
        json!({
            "protocolVersion": ProtocolRevision::Legacy.as_text(),
            "capabilities": { "tools": {}, "resources": {} },
            "serverInfo": { "name": SERVER_NAME, "version": env!("CARGO_PKG_VERSION") },
        })
    }

    /// Records that the client says it is initialized.
    pub fn initialized(&mut self) -> bool {
        if self.lifecycle != Lifecycle::Offered {
            return false;
        }
        self.lifecycle = Lifecycle::Ready;
        true
    }

    /// Requires one request to be one this session may act on now.
    ///
    /// # Errors
    ///
    /// Returns [`LegacyRefusal::NotInitialized`] before the handshake ends and
    /// [`LegacyRefusal::MethodUnavailable`] for anything this server does not
    /// offer.
    pub fn require_actionable(&self, method: &str) -> Result<(), LegacyRefusal> {
        if method == INITIALIZE_REQUEST {
            return Ok(());
        }
        if !EVERY_REQUEST.contains(&method) {
            return Err(LegacyRefusal::MethodUnavailable { named: method.to_owned() });
        }
        if self.lifecycle != Lifecycle::Ready {
            return Err(LegacyRefusal::NotInitialized { named: method.to_owned() });
        }
        Ok(())
    }
}

/// What this server calls itself when a client asks.
const SERVER_NAME: &str = "slingshot";

/// Returns one semantic payload as this era carries it.
///
/// Undecorated. This era has no member saying a result is whole and no cache
/// fields, and adding them because the other era has them would produce a
/// message this era's clients are entitled to reject.
#[must_use]
pub fn undecorated(payload: Value) -> Value {
    let mut result = payload;
    if let Some(object) = result.as_object_mut() {
        for member in MODERN_ONLY_MEMBERS {
            object.remove(*member);
        }
    }
    result
}
