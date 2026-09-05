//! Shared selected-author event request, item delivery and liveness policy.
use crate::author_hypertext_transfer_protocol_policy::ResponseHead;
use crate::event_stream_heartbeat::EventStreamHeartbeat;
use crate::selected_author_http::FiniteHttpFailure;
use crate::selected_author_transport::SelectedAuthorTransport;
use crate::server_sent_event_decoder::{
    DecoderBounds, EventStreamCursor, ServerSentEventDecoder, StreamItem, StreamRefusal,
    TerminalExpectationResolver,
};
use http::{HeaderMap, HeaderValue};
use tokio::time::{Duration, Instant};

/// Attachment outcome, never terminal operation evidence.
#[derive(Debug)]
pub enum EventHttpOutcome {
    /// Complete framed stream and clean EOF; reconnection may still be needed.
    Closed,
    /// Validated reset request; snapshot/high-water reconciliation must still
    /// finish before any captured cursor is installed.
    Reset(crate::event_stream_reset::ValidatedEventReset),
    /// Complete bounded JSON requiring route-specific reset/identity validation.
    /// Status alone grants no reset, authentication-refresh or retry authority.
    Response(crate::selected_author_exchange::SelectedAuthorFiniteResponse),
}

pub(crate) fn classify_attachment(
    outcome: EventHttpOutcome,
    subscription: &str,
    generation: u64,
    cursor: Option<&EventStreamCursor>,
) -> Result<EventHttpOutcome, FiniteHttpFailure> {
    match outcome {
        EventHttpOutcome::Response(response) if matches!(response.status, 409 | 410) => {
            crate::event_stream_reset::decode_event_reset(
                &response,
                crate::event_stream_reset::ResetRequest {
                    route: crate::event_stream_reset::ResetRoute::Events,
                    subscription,
                    generation,
                    committed_cursor: cursor.map(EventStreamCursor::as_text),
                },
            )
            .map(EventHttpOutcome::Reset)
            .map_err(|_| FiniteHttpFailure::Body)
        }
        other => Ok(other),
    }
}

pub(crate) fn request_fields(
    transport: &SelectedAuthorTransport,
    identity: &slingshot_domain::operation_executor::ExecutionIdentity,
    subscription: &str,
    generation: u64,
    cursor: Option<&EventStreamCursor>,
) -> Result<HeaderMap, FiniteHttpFailure> {
    transport.require_execution(identity).map_err(|_| FiniteHttpFailure::Request)?;
    let maximum =
        slingshot_domain::author_agent_transport_contract::AuthorAgentTransportContract::embedded()
            .limit("maximum_daemon_subscription_identifier_bytes");
    if generation == 0 || subscription.is_empty() || subscription.len() as u64 > maximum {
        return Err(FiniteHttpFailure::Request);
    }
    let mut fields = HeaderMap::new();
    fields.insert("accept", HeaderValue::from_static("text/event-stream"));
    if let Some(cursor) = cursor {
        if cursor.as_text().is_empty()
            || cursor.as_text().len() as u64 > DecoderBounds::embedded().identifier_bytes
            || cursor.as_text().as_bytes().first().is_some_and(|b| matches!(b, b' ' | b'\t'))
            || cursor.as_text().as_bytes().last().is_some_and(|b| matches!(b, b' ' | b'\t'))
        {
            return Err(FiniteHttpFailure::Request);
        }
        fields.insert(
            "last-event-id",
            HeaderValue::from_str(cursor.as_text()).map_err(|_| FiniteHttpFailure::Request)?,
        );
    }
    Ok(fields)
}

pub(crate) struct EventDelivery<R, C> {
    decoder: ServerSentEventDecoder<R>,
    consume: C,
    attached: Instant,
    heartbeat: EventStreamHeartbeat,
    poisoned: bool,
}

impl<R: TerminalExpectationResolver, C: FnMut(StreamItem) -> Result<(), FiniteHttpFailure>>
    EventDelivery<R, C>
{
    pub(crate) fn attached(
        head: &ResponseHead,
        media: &str,
        subscription: String,
        generation: u64,
        resolver: R,
        consume: C,
    ) -> Result<Self, FiniteHttpFailure> {
        if head.location.is_some() {
            return Err(FiniteHttpFailure::Head);
        }
        let decoder = ServerSentEventDecoder::attached_subscription(
            head,
            media,
            DecoderBounds::embedded(),
            subscription,
            generation,
            resolver,
        )
        .map_err(|_| FiniteHttpFailure::Head)?;
        Ok(Self {
            decoder,
            consume,
            attached: Instant::now(),
            heartbeat: EventStreamHeartbeat::attached_at(0),
            poisoned: false,
        })
    }
    pub(crate) fn deadline(&self) -> Instant {
        self.attached
            + Duration::from_millis(
                self.heartbeat
                    .last_activity_milliseconds()
                    .saturating_add(self.heartbeat.timeout_milliseconds())
                    .saturating_add(1),
            )
    }
    pub(crate) fn push(&mut self, bytes: &[u8]) -> Result<(), FiniteHttpFailure> {
        if self.poisoned {
            return Err(FiniteHttpFailure::Body);
        }
        self.poisoned = true;
        if Instant::now() >= self.deadline() {
            return Err(FiniteHttpFailure::EventHeartbeat);
        }
        let heartbeat = &mut self.heartbeat;
        let consume = &mut self.consume;
        let attached = self.attached;
        let result = self.decoder.push_each(bytes, |item| {
            let now = u64::try_from(attached.elapsed().as_millis()).unwrap_or(u64::MAX);
            if heartbeat.state_at(now).map_err(|_| StreamRefusal::Consumer)?.requires_reconnection()
            {
                return Err(StreamRefusal::Consumer);
            }
            heartbeat.observe(&item, now).map_err(|_| StreamRefusal::Consumer)?;
            consume(item).map_err(|_| StreamRefusal::Consumer)
        });
        if Instant::now() >= self.deadline() {
            return Err(FiniteHttpFailure::EventHeartbeat);
        }
        result.map_err(|_| FiniteHttpFailure::Body)?;
        self.poisoned = false;
        Ok(())
    }
    pub(crate) fn finish(self) -> Result<(), FiniteHttpFailure> {
        if Instant::now() >= self.deadline() {
            return Err(FiniteHttpFailure::EventHeartbeat);
        }
        if self.poisoned || self.decoder.has_partial_event() {
            return Err(FiniteHttpFailure::Body);
        }
        Ok(())
    }
}
