//! Assertion-to-token exchange owned by the requesting async cache flight.

use super::{
    async_access_token_cache::AsyncAccessTokenSource,
    cloud_service_credentials::CloudServiceCredentials,
    identity_management_exchange::{
        AccessToken, DecodedResponse, ExchangeFailure, MonotonicClock, accept_response,
        build_form_body, install,
    },
    token_assertion::{CoordinatedUniversalTimeClock, ServiceCredentialAssertion},
};
use std::{future::Future, pin::Pin};

/// Complete response with observations taken at the actual wire boundaries.
pub struct IdentityManagementReceipt {
    pub(super) response: DecodedResponse,
    request_anchor: u64,
    body_receipt: u64,
}
impl core::fmt::Debug for IdentityManagementReceipt {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str("IdentityManagementReceipt([redacted])")
    }
}
impl IdentityManagementReceipt {
    /// Records the injected-clock reading immediately before the first request
    /// byte and immediately after complete response/trailer-free stream end.
    /// Connection setup precedes the first observation, not the token lease.
    pub fn new(response: DecodedResponse, request_anchor: u64, body_receipt: u64) -> Self {
        Self { response, request_anchor, body_receipt }
    }
}

/// Direct, bounded exchange to the contract's fixed identity-management endpoint.
/// Implementations must use frozen platform-only trust, enforce wire limits and
/// phase deadlines, refuse redirects, and own their I/O until completion or drop.
/// This interface never accepts an author endpoint, trust extension or proxy.
pub trait AsyncIdentityManagementTransport: Send + Sync {
    /// Sends exactly one form body. Cancellation drops the request operation;
    /// implementations must not detach network work or retain borrowed secrets.
    fn exchange<'a>(
        &'a self,
        body: &'a [u8],
        clock: &'a (dyn MonotonicClock + Sync),
    ) -> Pin<Box<dyn Future<Output = Result<IdentityManagementReceipt, ExchangeFailure>> + Send + 'a>>;
}

/// A fresh signed assertion and a single awaited exchange per cache flight.
/// Credential and clock references belong to the immutable runtime context.
pub struct AsyncServiceCredentialTokenSource<'runtime, Transport, Clock, UtcClock> {
    credentials: &'runtime CloudServiceCredentials,
    transport: &'runtime Transport,
    clock: &'runtime Clock,
    utc: &'runtime UtcClock,
}

impl<Transport, Clock, UtcClock> core::fmt::Debug
    for AsyncServiceCredentialTokenSource<'_, Transport, Clock, UtcClock>
{
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str("AsyncServiceCredentialTokenSource([redacted])")
    }
}

impl<'runtime, Transport, Clock, UtcClock>
    AsyncServiceCredentialTokenSource<'runtime, Transport, Clock, UtcClock>
{
    /// Binds one source to the startup-owned credentials, transport and clocks.
    pub fn new(
        credentials: &'runtime CloudServiceCredentials,
        transport: &'runtime Transport,
        clock: &'runtime Clock,
        utc: &'runtime UtcClock,
    ) -> Self {
        Self { credentials, transport, clock, utc }
    }
}

impl<Transport, Clock, UtcClock> AsyncAccessTokenSource
    for AsyncServiceCredentialTokenSource<'_, Transport, Clock, UtcClock>
where
    Transport: AsyncIdentityManagementTransport,
    Clock: MonotonicClock + Sync,
    UtcClock: CoordinatedUniversalTimeClock + Sync,
{
    fn exchange(
        &self,
    ) -> Pin<Box<dyn Future<Output = Result<AccessToken, ExchangeFailure>> + Send + '_>> {
        Box::pin(async move {
            let assertion = ServiceCredentialAssertion::build(self.credentials, self.utc)
                .map_err(|failure| ExchangeFailure::new(failure.code))?;
            let body = build_form_body(self.credentials, &assertion)?;
            let receipt = self.transport.exchange(body.expose_secret_bytes(), self.clock).await?;
            let document = accept_response(&receipt.response)?;
            install(document, receipt.request_anchor, receipt.body_receipt)
        })
    }
}
