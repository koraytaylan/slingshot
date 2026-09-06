//! Authenticated high-water capture. No cursor, job or snapshot is mutated here.
use crate::authentication::environment_provider::RequestAuthentication;
use crate::event_stream_reset::{
    ResetRequest, ResetRoute, ValidatedEventReset, decode_event_reset, require_cursor,
};
use crate::selected_author_exchange::SelectedAuthorFiniteResponse;
use crate::selected_author_http::FiniteHttpFailure;
use crate::selected_author_transport::SelectedAuthorTransport;
use crate::server_sent_event_decoder::{DecoderBounds, EventStreamCursor};
use http::{HeaderValue, Method};
use slingshot_agent_protocol::subscription_high_water::SubscriptionHighWater;
use slingshot_domain::author_agent_transport_contract::AuthorAgentTransportContract;

/// A captured position bound to its echoed request, not a completed reset.
pub struct ValidatedHighWater {
    subscription: String,
    generation: u64,
    cursor: EventStreamCursor,
}
impl core::fmt::Debug for ValidatedHighWater {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str("ValidatedHighWater([redacted])")
    }
}
impl ValidatedHighWater {
    /// The subscription actually captured.
    pub fn subscription(&self) -> &str {
        &self.subscription
    }
    /// The generation whose snapshots must cover this capture.
    pub fn generation(&self) -> u64 {
        self.generation
    }
    /// Position to reconcile through, not permission to skip directly to it.
    pub fn cursor(&self) -> &EventStreamCursor {
        &self.cursor
    }
    /// Requires a separately identity-validated snapshot to cover this capture.
    /// This checks one member only; it cannot authorize a subscription reset.
    pub fn require_snapshot_coverage(
        &self,
        snapshot: &crate::job_snapshot_reconciliation::JobSnapshot,
    ) -> Result<(), HighWaterRefusal> {
        require_cursor(snapshot.subscription_watermark.as_text()).map_err(|_| HighWaterRefusal)?;
        if snapshot.echo.daemon_subscription_identifier != self.subscription
            || snapshot.echo.agent_event_store_generation != self.generation
            || snapshot.subscription_watermark.as_text() < self.cursor.as_text()
        {
            return Err(HighWaterRefusal);
        }
        Ok(())
    }
}

/// A high-water route result, with reset truth separate from generic statuses.
#[derive(Debug)]
pub enum HighWaterOutcome {
    /// Validated capture; every relevant snapshot still needs to cover it.
    Captured(ValidatedHighWater),
    /// Validated generation change, not cursor-expiry or resubmission authority.
    Reset(ValidatedEventReset),
    /// Other complete JSON response requiring route-aware retry/authentication policy.
    Response(SelectedAuthorFiniteResponse),
}

/// Opaque refusal; no response or cursor values enter diagnostics.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[error("the author high-water capture is not verified")]
pub struct HighWaterRefusal;

/// Validates a complete selected-author response against independent request
/// values. A matching body cannot substitute another subscription/generation.
pub fn decode_high_water(
    response: &SelectedAuthorFiniteResponse,
    subscription: &str,
    generation: u64,
) -> Result<ValidatedHighWater, HighWaterRefusal> {
    let contract = AuthorAgentTransportContract::embedded();
    if subscription.is_empty()
        || subscription.len() as u64
            > contract.limit("maximum_daemon_subscription_identifier_bytes")
        || generation == 0
        || response.status != 200
        || response.head.location.is_some()
        || response.body.len() as u64 > contract.limit("maximum_agent_protocol_document_bytes")
        || !crate::selected_author_submission::json_media_type(
            response.content_type.as_deref().unwrap_or(""),
        )
    {
        return Err(HighWaterRefusal);
    }
    response.head.require_acceptable().map_err(|_| HighWaterRefusal)?;
    let document: SubscriptionHighWater =
        serde_json::from_slice(&response.body).map_err(|_| HighWaterRefusal)?;
    if document.format != slingshot_agent_protocol::identity::AGENT_FORMAT
        || document.transport_contract_digest != AuthorAgentTransportContract::embedded_digest()
        || document.daemon_subscription_identifier != subscription
        || document.agent_event_store_generation != generation
    {
        return Err(HighWaterRefusal);
    }
    require_cursor(&document.high_water_cursor).map_err(|_| HighWaterRefusal)?;
    Ok(ValidatedHighWater {
        subscription: document.daemon_subscription_identifier,
        generation,
        cursor: EventStreamCursor::new(
            &document.high_water_cursor,
            DecoderBounds::embedded().identifier_bytes,
        )
        .map_err(|_| HighWaterRefusal)?,
    })
}

impl SelectedAuthorTransport {
    /// Captures one selected subscription position over HTTP/1.1 without retry
    /// or installing the returned position.
    pub async fn capture_high_water_http1(
        &self,
        identity: &slingshot_domain::operation_executor::ExecutionIdentity,
        subscription: &str,
        generation: u64,
        authentication: &RequestAuthentication,
    ) -> Result<HighWaterOutcome, FiniteHttpFailure> {
        self.capture_high_water(identity, subscription, generation, authentication, Some(false))
            .await
    }
    /// Captures the same fixed route through strict HTTP/2 negotiation.
    pub async fn capture_high_water_http2(
        &self,
        identity: &slingshot_domain::operation_executor::ExecutionIdentity,
        subscription: &str,
        generation: u64,
        authentication: &RequestAuthentication,
    ) -> Result<HighWaterOutcome, FiniteHttpFailure> {
        self.capture_high_water(identity, subscription, generation, authentication, Some(true))
            .await
    }
    /// Captures on the original negotiated connection. The captured position
    /// remains evidence for durable reconciliation, not permission to install
    /// a cursor, change generation or retry the request.
    pub async fn capture_high_water_negotiated(
        &self,
        identity: &slingshot_domain::operation_executor::ExecutionIdentity,
        subscription: &str,
        generation: u64,
        authentication: &RequestAuthentication,
    ) -> Result<HighWaterOutcome, FiniteHttpFailure> {
        self.capture_high_water(identity, subscription, generation, authentication, None).await
    }
    /// Captures with request-scoped provider authentication. Only a validated
    /// Cloud 401 allows one identical GET; neither attempt installs a cursor.
    pub async fn capture_high_water_authenticated(
        &self,
        identity: &slingshot_domain::operation_executor::ExecutionIdentity,
        subscription: &str,
        generation: u64,
        provider: &crate::authentication::environment_provider::EnvironmentAuthenticationProvider,
        source: &dyn crate::authentication::access_token_cache::AccessTokenSource,
        reading: u64,
    ) -> Result<HighWaterOutcome, FiniteHttpFailure> {
        let mut fields = crate::selected_author_events::request_fields(
            self,
            identity,
            subscription,
            generation,
            None,
        )?;
        fields.insert("accept", HeaderValue::from_static("application/json"));
        let generation_text = generation.to_string();
        let receipt = self
            .authenticated_finite_get(
                provider,
                source,
                reading,
                &["bin", "slingshot-agent", "events", "high-water"],
                &[
                    ("agent_event_store_generation", generation_text.as_str()),
                    ("daemon_subscription_identifier", subscription),
                ],
                &fields,
            )
            .await
            .map_err(|error| match error {
                crate::selected_author_authenticated_read::AuthenticatedReadFailure::Transport(
                    failure,
                ) => failure,
                _ => FiniteHttpFailure::Request,
            })?;
        Self::decode_high_water_response(receipt.response, subscription, generation)
    }

    /// Captures through the selected provider's asynchronous credential exchange.
    /// Selection checks run before authentication; response validation and the
    /// single refreshed-GET policy are shared with the other transports.
    pub async fn capture_high_water_authenticated_async<Clock, Utc>(
        &self,
        identity: &slingshot_domain::operation_executor::ExecutionIdentity,
        subscription: &str,
        generation: u64,
        provider: &crate::authentication::environment_provider::AsyncEnvironmentAuthenticationProvider,
        clock: &Clock,
        utc: &Utc,
    ) -> Result<HighWaterOutcome, FiniteHttpFailure>
    where
        Clock: crate::authentication::identity_management_exchange::MonotonicClock + Sync,
        Utc: crate::authentication::token_assertion::CoordinatedUniversalTimeClock + Sync,
    {
        let mut fields = crate::selected_author_events::request_fields(
            self,
            identity,
            subscription,
            generation,
            None,
        )?;
        fields.insert("accept", HeaderValue::from_static("application/json"));
        let generation_text = generation.to_string();
        let receipt = self
            .authenticated_finite_get_async(
                provider,
                clock,
                utc,
                &["bin", "slingshot-agent", "events", "high-water"],
                &[
                    ("agent_event_store_generation", generation_text.as_str()),
                    ("daemon_subscription_identifier", subscription),
                ],
                &fields,
            )
            .await
            .map_err(|error| match error {
                crate::selected_author_authenticated_read::AuthenticatedReadFailure::Transport(
                    failure,
                ) => failure,
                _ => FiniteHttpFailure::Request,
            })?;
        Self::decode_high_water_response(receipt.response, subscription, generation)
    }

    async fn capture_high_water(
        &self,
        identity: &slingshot_domain::operation_executor::ExecutionIdentity,
        subscription: &str,
        generation: u64,
        authentication: &RequestAuthentication,
        http2: Option<bool>,
    ) -> Result<HighWaterOutcome, FiniteHttpFailure> {
        let mut fields = crate::selected_author_events::request_fields(
            self,
            identity,
            subscription,
            generation,
            None,
        )?;
        fields.insert("accept", HeaderValue::from_static("application/json"));
        let generation_text = generation.to_string();
        let segments = ["bin", "slingshot-agent", "events", "high-water"];
        let query = [
            ("agent_event_store_generation", generation_text.as_str()),
            ("daemon_subscription_identifier", subscription),
        ];
        let receipt = if http2.is_none() {
            self.finite_negotiated_query(
                Method::GET,
                &segments,
                &query,
                authentication,
                &fields,
                b"",
            )
            .await?
        } else if http2 == Some(true) {
            self.finite_http2_query(Method::GET, &segments, &query, authentication, &fields, b"")
                .await?
        } else {
            self.finite_http1_query(Method::GET, &segments, &query, authentication, &fields, b"")
                .await?
        };
        Self::decode_high_water_response(receipt.response, subscription, generation)
    }

    fn decode_high_water_response(
        response: SelectedAuthorFiniteResponse,
        subscription: &str,
        generation: u64,
    ) -> Result<HighWaterOutcome, FiniteHttpFailure> {
        if response.head.location.is_some()
            || !crate::selected_author_submission::json_media_type(
                response.content_type.as_deref().unwrap_or(""),
            )
        {
            return Err(FiniteHttpFailure::Head);
        }
        match response.status {
            200 => decode_high_water(&response, subscription, generation)
                .map(HighWaterOutcome::Captured)
                .map_err(|_| FiniteHttpFailure::Body),
            409 => decode_event_reset(
                &response,
                ResetRequest {
                    route: ResetRoute::HighWater,
                    subscription,
                    generation,
                    committed_cursor: None,
                },
            )
            .map(HighWaterOutcome::Reset)
            .map_err(|_| FiniteHttpFailure::Body),
            _ => Ok(HighWaterOutcome::Response(response)),
        }
    }
}
