//! Invocation-scoped authentication selection for durable author coordinators.

use slingshot_agent_connection::{
    authentication::{
        access_token_cache::AccessTokenSource,
        environment_provider::{
            AsyncEnvironmentAuthenticationProvider, EnvironmentAuthenticationProvider,
            RequestAuthentication, SelectedAuthorConnectionRefusal,
        },
        identity_management_exchange::MonotonicClock,
        token_assertion::CoordinatedUniversalTimeClock,
    },
    capability_discovery::{AdvertisedCapabilities, CapabilityExchangeRefusal},
    command_submission::{Submission, SubmissionOutcome},
    selected_author_submission::SubmissionSendRefusal,
    selected_author_transport::SelectedAuthorTransport,
};
use slingshot_domain::operation_executor::ExecutionIdentity;

use super::subscription_reset::ResetTransport;

/// One frozen authentication policy, reused without reloading configuration.
/// Provider credentials are acquired separately for each request; no borrowed
/// token is promoted into invocation-long authentication.
#[derive(Clone, Copy)]
pub enum AuthorAuthentication<'runtime> {
    /// Runtime-owned async provider and the clocks used by its token exchange.
    AsyncProvider {
        /// Frozen selected credentials and the sole invocation-shared cache.
        provider: &'runtime AsyncEnvironmentAuthenticationProvider,
        /// Monotonic domain shared with request/receipt lifetime anchors.
        clock: &'runtime (dyn MonotonicClock + Sync),
        /// Samples assertion issuance time; never consulted for Basic requests.
        utc: &'runtime (dyn CoordinatedUniversalTimeClock + Sync),
    },
    /// Explicit transport fixtures and callers already holding bound credentials.
    Fixed {
        /// Credentials already bound to the selected target/revision.
        authentication: &'runtime RequestAuthentication,
        /// Explicit or negotiated transport selection.
        protocol: ResetTransport,
    },
    /// Runtime provider with the token source's own monotonic clock.
    Provider {
        /// Immutable selected environment and its process-memory token cache.
        provider: &'runtime EnvironmentAuthenticationProvider,
        /// Exchanges only through the selected provider's credential authority.
        source: &'runtime dyn AccessTokenSource,
        /// Samples the same clock domain used by token receipt anchors.
        clock: &'runtime dyn MonotonicClock,
    },
}

impl core::fmt::Debug for AuthorAuthentication<'_> {
    fn fmt(&self, formatter: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        formatter.write_str("AuthorAuthentication([redacted])")
    }
}

// Sized borrowing adapters preserve the actual clock domains without copying
// readings, introducing clock state, or requiring generic durable coordinators.
struct RuntimeClock<'a>(&'a (dyn MonotonicClock + Sync));
impl MonotonicClock for RuntimeClock<'_> {
    fn reading_milliseconds(&self) -> u64 {
        self.0.reading_milliseconds()
    }
}
struct RuntimeUtc<'a>(&'a (dyn CoordinatedUniversalTimeClock + Sync));
impl CoordinatedUniversalTimeClock for RuntimeUtc<'_> {
    fn sample(&self) -> Option<u64> {
        self.0.sample()
    }
}

impl AuthorAuthentication<'_> {
    /// Binds policy ownership before an invocation can retain it; no token,
    /// clock, socket or mutable cache is consulted for this check.
    pub(crate) fn require_execution(
        self,
        identity: &ExecutionIdentity,
    ) -> Result<(), SelectedAuthorConnectionRefusal> {
        match self {
            Self::Fixed { authentication, .. } => authentication.require_execution(identity),
            Self::Provider { provider, .. } => {
                provider.snapshot().author_connection().require_execution(identity)
            }
            Self::AsyncProvider { provider, .. } => {
                provider.snapshot().author_connection().require_execution(identity)
            }
        }
    }

    pub(crate) async fn events<
        R: slingshot_agent_connection::server_sent_event_decoder::TerminalExpectationResolver,
    >(
        self,
        transport: &SelectedAuthorTransport,
        identity: &ExecutionIdentity,
        subscription: &str,
        generation: u64,
        cursor: Option<&slingshot_agent_connection::server_sent_event_decoder::EventStreamCursor>,
        resolver: R,
        consume: impl FnMut(
            slingshot_agent_connection::server_sent_event_decoder::StreamItem,
        ) -> Result<
            (),
            slingshot_agent_connection::selected_author_http::FiniteHttpFailure,
        >,
    ) -> Result<
        slingshot_agent_connection::selected_author_events::EventHttpOutcome,
        slingshot_agent_connection::selected_author_http::FiniteHttpFailure,
    > {
        match self {
            Self::AsyncProvider { provider, clock, utc } => {
                transport
                    .events_authenticated_async(
                        identity,
                        subscription,
                        generation,
                        cursor,
                        provider,
                        &RuntimeClock(clock),
                        &RuntimeUtc(utc),
                        resolver,
                        consume,
                    )
                    .await
            }
            Self::Provider { provider, source, clock } => {
                transport
                    .events_authenticated(
                        identity,
                        subscription,
                        generation,
                        cursor,
                        provider,
                        source,
                        clock.reading_milliseconds(),
                        resolver,
                        consume,
                    )
                    .await
            }
            Self::Fixed { authentication, protocol: ResetTransport::Automatic } => {
                transport
                    .events_negotiated(
                        identity,
                        subscription,
                        generation,
                        cursor,
                        authentication,
                        resolver,
                        consume,
                    )
                    .await
            }
            Self::Fixed { authentication, protocol: ResetTransport::Http1 } => {
                transport
                    .events_http1(
                        identity,
                        subscription,
                        generation,
                        cursor,
                        authentication,
                        resolver,
                        consume,
                    )
                    .await
            }
            Self::Fixed { authentication, protocol: ResetTransport::Http2 } => {
                transport
                    .events_http2(
                        identity,
                        subscription,
                        generation,
                        cursor,
                        authentication,
                        resolver,
                        consume,
                    )
                    .await
            }
        }
    }

    pub(crate) async fn physical_lookup(
        self,
        transport: &SelectedAuthorTransport,
        identity: &ExecutionIdentity,
        submission: &Submission,
        identifier: &str,
        generation: u64,
    ) -> Result<
        slingshot_agent_connection::selected_author_lookup::PhysicalLookupReceipt,
        slingshot_agent_connection::selected_author_lookup::SnapshotLookupRefusal,
    > {
        match self {
            Self::AsyncProvider { provider, clock, utc } => {
                transport
                    .lookup_physical_job_authenticated_async(
                        identity,
                        submission,
                        identifier,
                        generation,
                        provider,
                        &RuntimeClock(clock),
                        &RuntimeUtc(utc),
                    )
                    .await
            }
            Self::Provider { provider, source, clock } => {
                transport
                    .lookup_physical_job_authenticated(
                        identity,
                        submission,
                        identifier,
                        generation,
                        provider,
                        source,
                        clock.reading_milliseconds(),
                    )
                    .await
            }
            Self::Fixed { authentication, protocol: ResetTransport::Automatic } => {
                transport
                    .lookup_physical_job_negotiated(
                        identity,
                        submission,
                        identifier,
                        generation,
                        authentication,
                    )
                    .await
            }
            Self::Fixed { authentication, protocol: ResetTransport::Http1 } => {
                transport
                    .lookup_physical_job(
                        identity,
                        submission,
                        identifier,
                        generation,
                        authentication,
                    )
                    .await
            }
            Self::Fixed { authentication, protocol: ResetTransport::Http2 } => {
                transport
                    .lookup_physical_job_http2(
                        identity,
                        submission,
                        identifier,
                        generation,
                        authentication,
                    )
                    .await
            }
        }
    }

    pub(crate) async fn high_water(
        self,
        transport: &SelectedAuthorTransport,
        identity: &ExecutionIdentity,
        subscription: &str,
        generation: u64,
    ) -> Result<
        slingshot_agent_connection::subscription_high_water::HighWaterOutcome,
        slingshot_agent_connection::selected_author_http::FiniteHttpFailure,
    > {
        match self {
            Self::AsyncProvider { provider, clock, utc } => {
                transport
                    .capture_high_water_authenticated_async(
                        identity,
                        subscription,
                        generation,
                        provider,
                        &RuntimeClock(clock),
                        &RuntimeUtc(utc),
                    )
                    .await
            }
            Self::Provider { provider, source, clock } => {
                transport
                    .capture_high_water_authenticated(
                        identity,
                        subscription,
                        generation,
                        provider,
                        source,
                        clock.reading_milliseconds(),
                    )
                    .await
            }
            Self::Fixed { authentication, protocol: ResetTransport::Automatic } => {
                transport
                    .capture_high_water_negotiated(
                        identity,
                        subscription,
                        generation,
                        authentication,
                    )
                    .await
            }
            Self::Fixed { authentication, protocol: ResetTransport::Http1 } => {
                transport
                    .capture_high_water_http1(identity, subscription, generation, authentication)
                    .await
            }
            Self::Fixed { authentication, protocol: ResetTransport::Http2 } => {
                transport
                    .capture_high_water_http2(identity, subscription, generation, authentication)
                    .await
            }
        }
    }

    pub(crate) async fn lookup(
        self,
        transport: &SelectedAuthorTransport,
        identity: &ExecutionIdentity,
        submission: &Submission,
    ) -> Result<
        slingshot_agent_connection::selected_author_lookup::OperationLookupReceipt,
        slingshot_agent_connection::selected_author_lookup::SnapshotLookupRefusal,
    > {
        match self {
            Self::AsyncProvider { provider, clock, utc } => {
                transport
                    .lookup_operation_authenticated_async(
                        identity,
                        submission,
                        provider,
                        &RuntimeClock(clock),
                        &RuntimeUtc(utc),
                    )
                    .await
            }
            Self::Provider { provider, source, clock } => {
                transport
                    .lookup_operation_authenticated(
                        identity,
                        submission,
                        provider,
                        source,
                        clock.reading_milliseconds(),
                    )
                    .await
            }
            Self::Fixed { authentication, protocol: ResetTransport::Automatic } => {
                transport.lookup_operation_negotiated(identity, submission, authentication).await
            }
            Self::Fixed { authentication, protocol: ResetTransport::Http2 } => {
                transport.lookup_operation_http2(identity, submission, authentication).await
            }
            Self::Fixed { authentication, protocol: ResetTransport::Http1 } => {
                transport.lookup_operation(identity, submission, authentication).await
            }
        }
    }

    pub(crate) async fn artifact(
        self,
        transport: &SelectedAuthorTransport,
        identity: &ExecutionIdentity,
        submission: &Submission,
        expected: &slingshot_agent_connection::artifact_download::ExpectedArtifact,
        identifier: &str,
        sink: impl FnMut(
            &[u8],
        ) -> Result<
            (),
            slingshot_agent_connection::selected_author_http::FiniteHttpFailure,
        >,
    ) -> Result<
        slingshot_agent_connection::selected_author_http::ArtifactHttpOutcome,
        slingshot_agent_connection::selected_author_http::FiniteHttpFailure,
    > {
        match self {
            Self::AsyncProvider { provider, clock, utc } => {
                transport
                    .artifact_authenticated_async(
                        identity,
                        submission,
                        expected,
                        identifier,
                        provider,
                        &RuntimeClock(clock),
                        &RuntimeUtc(utc),
                        sink,
                    )
                    .await
            }
            Self::Provider { provider, source, clock } => {
                transport
                    .artifact_authenticated(
                        identity,
                        submission,
                        expected,
                        identifier,
                        provider,
                        source,
                        clock.reading_milliseconds(),
                        sink,
                    )
                    .await
            }
            Self::Fixed { authentication, protocol: ResetTransport::Automatic } => {
                transport
                    .artifact_negotiated(
                        identity,
                        submission,
                        expected,
                        identifier,
                        authentication,
                        sink,
                    )
                    .await
            }
            Self::Fixed { authentication, protocol: ResetTransport::Http2 } => {
                transport
                    .artifact_http2(
                        identity,
                        submission,
                        expected,
                        identifier,
                        authentication,
                        sink,
                    )
                    .await
            }
            Self::Fixed { authentication, protocol: ResetTransport::Http1 } => {
                transport
                    .artifact_http1(
                        identity,
                        submission,
                        expected,
                        identifier,
                        authentication,
                        sink,
                    )
                    .await
            }
        }
    }

    pub(crate) async fn discover(
        self,
        transport: &SelectedAuthorTransport,
        identity: &ExecutionIdentity,
        submission: &Submission,
    ) -> Result<AdvertisedCapabilities, CapabilityExchangeRefusal> {
        let command = &submission.provenance.command_contract.command_wire_name;
        let generation = Some(submission.operation.agent_event_store_generation);
        match self {
            Self::AsyncProvider { provider, clock, utc } => {
                transport
                    .discover_capabilities_authenticated_async(
                        identity,
                        command,
                        generation,
                        provider,
                        &RuntimeClock(clock),
                        &RuntimeUtc(utc),
                    )
                    .await
            }
            Self::Provider { provider, source, clock } => {
                transport
                    .discover_capabilities_authenticated(
                        identity,
                        command,
                        generation,
                        provider,
                        source,
                        clock.reading_milliseconds(),
                    )
                    .await
            }
            Self::Fixed { authentication, protocol: ResetTransport::Automatic } => {
                transport
                    .discover_capabilities_negotiated(identity, command, generation, authentication)
                    .await
            }
            Self::Fixed { authentication, protocol: ResetTransport::Http2 } => {
                transport
                    .discover_capabilities_http2(identity, command, generation, authentication)
                    .await
            }
            Self::Fixed { authentication, protocol: ResetTransport::Http1 } => {
                transport.discover_capabilities(identity, command, generation, authentication).await
            }
        }
    }

    pub(crate) async fn submit(
        self,
        transport: &SelectedAuthorTransport,
        identity: &ExecutionIdentity,
        submission: &Submission,
        now: u64,
        guard: impl FnOnce() -> Result<(), SubmissionSendRefusal>,
    ) -> Result<SubmissionOutcome, SubmissionSendRefusal> {
        match self {
            Self::AsyncProvider { provider, clock, utc } => {
                transport
                    .send_submission_authenticated_async_guarded(
                        identity,
                        submission,
                        provider,
                        &RuntimeClock(clock),
                        &RuntimeUtc(utc),
                        now,
                        guard,
                    )
                    .await
            }
            Self::Provider { provider, source, clock } => {
                transport
                    .send_submission_authenticated_guarded(
                        identity,
                        submission,
                        provider,
                        source,
                        clock.reading_milliseconds(),
                        now,
                        guard,
                    )
                    .await
            }
            Self::Fixed { authentication, protocol: ResetTransport::Automatic } => {
                transport
                    .send_submission_with_fresh_token_negotiated_guarded(
                        identity,
                        submission,
                        authentication,
                        now,
                        guard,
                    )
                    .await
            }
            Self::Fixed { authentication, protocol: ResetTransport::Http2 } => {
                transport
                    .send_submission_with_fresh_token_http2_guarded(
                        identity,
                        submission,
                        authentication,
                        now,
                        guard,
                    )
                    .await
            }
            Self::Fixed { authentication, protocol: ResetTransport::Http1 } => {
                transport
                    .send_submission_with_fresh_token_guarded(
                        identity,
                        submission,
                        authentication,
                        now,
                        guard,
                    )
                    .await
            }
        }
    }
}
