//! Closed reset decoding after a complete selected-author finite response.
//! This evidence requests reconciliation; it cannot install a cursor or change
//! a job. Snapshot/high-water recovery and durable CAS remain caller-owned.

use crate::selected_author_exchange::SelectedAuthorFiniteResponse;
use crate::server_sent_event_decoder::{DecoderBounds, EventStreamCursor};
use slingshot_agent_protocol::event_stream_reset::{EventStreamResetRequired, ResetReason};
use slingshot_domain::author_agent_transport_contract::AuthorAgentTransportContract;

/// The route on which a reset response arrived.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResetRoute {
    /// Filtered event attachment; generation and cursor resets are possible.
    Events,
    /// Authenticated high-water capture; only a generation reset is possible.
    HighWater,
}

/// Immutable request context, supplied independently of the response.
pub struct ResetRequest<'a> {
    /// Route actually requested.
    pub route: ResetRoute,
    /// Selected filtered subscription.
    pub subscription: &'a str,
    /// Generation supplied by retained state.
    pub generation: u64,
    /// Committed Last-Event-ID, never an uncommitted observation.
    pub committed_cursor: Option<&'a str>,
}

/// Verified reset request, not proof that recovery has completed.
pub struct ValidatedEventReset {
    subscription: String,
    requested_generation: u64,
    requested_cursor: Option<String>,
    generation: u64,
    cursor: EventStreamCursor,
    reason: ResetReason,
}

impl core::fmt::Debug for ValidatedEventReset {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str("ValidatedEventReset([redacted])")
    }
}

impl ValidatedEventReset {
    /// The subscription whose request was echoed.
    pub fn subscription(&self) -> &str {
        &self.subscription
    }
    /// The old generation supplied by the request.
    pub fn requested_generation(&self) -> u64 {
        self.requested_generation
    }
    /// The exact committed cursor echoed by the response.
    pub fn requested_cursor(&self) -> Option<&str> {
        self.requested_cursor.as_deref()
    }
    /// The generation in which the response captured its position.
    pub fn generation(&self) -> u64 {
        self.generation
    }
    /// Captured position; this is not authority to install it before reconciliation.
    pub fn captured_cursor(&self) -> &EventStreamCursor {
        &self.cursor
    }
    /// Validated reason to enter subscription reset recovery.
    pub fn reason(&self) -> crate::event_stream_reconnection::ResetReason {
        match self.reason {
            ResetReason::GenerationChanged => {
                crate::event_stream_reconnection::ResetReason::GenerationChanged
            }
            ResetReason::CursorExpired => {
                crate::event_stream_reconnection::ResetReason::CursorExpired
            }
        }
    }
}

/// Bounded refusal containing no remote body, cursor or identity values.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[error("the author event reset response is not verified")]
pub struct ResetRefusal;

/// Decodes a complete framed response against independent request context.
/// A successful result requests reconciliation but performs no mutation.
pub fn decode_event_reset(
    response: &SelectedAuthorFiniteResponse,
    request: ResetRequest<'_>,
) -> Result<ValidatedEventReset, ResetRefusal> {
    let contract = AuthorAgentTransportContract::embedded();
    if request.subscription.is_empty()
        || request.subscription.len() as u64
            > contract.limit("maximum_daemon_subscription_identifier_bytes")
        || request.generation == 0
        || request.route == ResetRoute::HighWater && request.committed_cursor.is_some()
        || response.body.len() as u64 > contract.limit("maximum_agent_protocol_document_bytes")
        || !matches!(response.status, 409 | 410)
        || response.head.location.is_some()
        || !crate::selected_author_submission::json_media_type(
            response.content_type.as_deref().unwrap_or(""),
        )
    {
        return Err(ResetRefusal);
    }
    if let Some(cursor) = request.committed_cursor {
        require_cursor(cursor)?;
    }
    response.head.require_acceptable().map_err(|_| ResetRefusal)?;
    let document: EventStreamResetRequired =
        serde_json::from_slice(&response.body).map_err(|_| ResetRefusal)?;
    if document.format != slingshot_agent_protocol::identity::AGENT_FORMAT
        || document.transport_contract_digest != AuthorAgentTransportContract::embedded_digest()
        || document.daemon_subscription_identifier != request.subscription
        || document.requested_agent_event_store_generation != request.generation
        || document.requested_last_event_identifier.as_deref() != request.committed_cursor
        || document.agent_event_store_generation == 0
    {
        return Err(ResetRefusal);
    }
    require_cursor(&document.high_water_cursor)?;
    match (response.status, document.reason) {
        (409, ResetReason::GenerationChanged)
            if document.agent_event_store_generation != request.generation => {}
        (410, ResetReason::CursorExpired)
            if request.route == ResetRoute::Events
                && request.committed_cursor.is_some()
                && document.agent_event_store_generation == request.generation => {}
        _ => return Err(ResetRefusal),
    }
    Ok(ValidatedEventReset {
        subscription: document.daemon_subscription_identifier,
        requested_generation: request.generation,
        requested_cursor: document.requested_last_event_identifier,
        generation: document.agent_event_store_generation,
        cursor: EventStreamCursor::new(
            &document.high_water_cursor,
            DecoderBounds::embedded().identifier_bytes,
        )
        .map_err(|_| ResetRefusal)?,
        reason: document.reason,
    })
}

pub(crate) fn require_cursor(cursor: &str) -> Result<(), ResetRefusal> {
    if cursor.is_empty()
        || cursor.len() as u64 > DecoderBounds::embedded().identifier_bytes
        || cursor.as_bytes().first().is_some_and(|b| matches!(b, b' ' | b'\t'))
        || cursor.as_bytes().last().is_some_and(|b| matches!(b, b' ' | b'\t'))
        || http::HeaderValue::from_str(cursor).is_err()
    {
        return Err(ResetRefusal);
    }
    Ok(())
}
