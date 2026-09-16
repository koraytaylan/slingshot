//! Request-scoped authentication for finite selected-author exchanges.
//!
//! Only a completely framed Cloud 401 permits one refresh and one identical
//! repeat of the bounded request, whatever its method. Route-specific response
//! interpretation remains the caller's responsibility.

use http::{HeaderMap, Method, StatusCode};
use std::future::Future;
use tokio::time::Instant;

use crate::authentication::access_token_cache::AccessTokenSource;
use crate::authentication::environment_provider::{
    EnvironmentAuthenticationProvider, ProviderFailure, SelectedAuthorConnectionRefusal,
};
use crate::selected_author_http::{FiniteHttpFailure, FiniteHttpReceipt};
use crate::selected_author_transport::SelectedAuthorTransport;

const NANOSECONDS_PER_MILLISECOND: u128 = 1_000_000;

/// A bounded read failed without retaining remote strings or credential bytes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum AuthenticatedReadFailure {
    /// The provider belongs to another immutable selection; no exchange occurs.
    #[error("{0}")]
    Selection(#[from] SelectedAuthorConnectionRefusal),
    /// Initial authentication or the one allowed refresh failed.
    #[error("{0}")]
    Authentication(#[from] ProviderFailure),
    /// Request encoding or the bounded wire exchange failed.
    #[error("{0}")]
    Transport(#[from] FiniteHttpFailure),
}

impl SelectedAuthorTransport {
    /// Sends one GET, refreshing Cloud credentials once after a validated 401.
    ///
    /// `reading` belongs to the token source's monotonic clock. Basic 401s,
    /// 403s, and transport failures never retry. A second 401 is returned to
    /// the route as the terminal response for this request. The elapsed receipt
    /// includes authentication, both exchanges and refresh, so retry cannot
    /// extend the caller's advertised retention budget. Dropping this future
    /// cannot schedule a detached retry.
    ///
    /// # Errors
    /// Refuses a provider from another selection, failed initial authentication
    /// or refresh, invalid request encoding, and failed bounded exchanges.
    pub async fn authenticated_finite_get(
        &self,
        provider: &EnvironmentAuthenticationProvider,
        source: &dyn AccessTokenSource,
        reading: u64,
        segments: &[&str],
        query: &[(&str, &str)],
        fields: &HeaderMap,
    ) -> Result<FiniteHttpReceipt, AuthenticatedReadFailure> {
        self.require_provider(provider)?;
        self.finite_get_with_authentication(
            segments,
            query,
            fields,
            async { provider.authenticate(&provider.author_endpoint(segments), reading, source) },
            |lease| async move { provider.refresh_after_unauthorized(lease, source) },
        )
        .await
    }

    /// Uses the async provider's single cache and its own selected-credential IMS
    /// client. Refresh and request cancellation remain owned by this future.
    ///
    /// # Errors
    /// Refuses a provider from another selection, failed initial authentication
    /// or refresh, invalid request encoding, and failed bounded exchanges.
    pub async fn authenticated_finite_get_async<Clock, Utc>(
        &self,
        provider: &crate::authentication::environment_provider::AsyncEnvironmentAuthenticationProvider,
        clock: &Clock,
        utc: &Utc,
        segments: &[&str],
        query: &[(&str, &str)],
        fields: &HeaderMap,
    ) -> Result<FiniteHttpReceipt, AuthenticatedReadFailure>
    where
        Clock: crate::authentication::identity_management_exchange::MonotonicClock + Sync,
        Utc: crate::authentication::token_assertion::CoordinatedUniversalTimeClock + Sync,
    {
        self.require_provider(provider)?;
        let endpoint = provider.author_endpoint(segments);
        self.finite_get_with_authentication(
            segments,
            query,
            fields,
            provider.authenticate(&endpoint, clock, utc),
            |lease| provider.refresh_after_unauthorized(lease, clock, utc),
        )
        .await
    }

    async fn finite_get_with_authentication<Lease, Refresh>(
        &self,
        segments: &[&str],
        query: &[(&str, &str)],
        fields: &HeaderMap,
        initial: impl Future<
            Output = Result<
                (crate::authentication::environment_provider::RequestAuthentication, Option<Lease>),
                ProviderFailure,
            >,
        >,
        refresh: impl FnOnce(Lease) -> Refresh,
    ) -> Result<FiniteHttpReceipt, AuthenticatedReadFailure>
    where
        Refresh: Future<
            Output = Result<
                (crate::authentication::environment_provider::RequestAuthentication, Lease),
                ProviderFailure,
            >,
        >,
    {
        self.finite_with_authentication(Method::GET, segments, query, fields, b"", initial, refresh)
            .await
    }

    /// Sends one request with the given method and body, refreshing Cloud
    /// credentials once after a validated 401. The same request, byte for
    /// byte, is repeated on that refresh; nothing else ever retries it.
    pub(crate) async fn finite_with_authentication<Lease, Refresh>(
        &self,
        method: Method,
        segments: &[&str],
        query: &[(&str, &str)],
        fields: &HeaderMap,
        body: &[u8],
        initial: impl Future<
            Output = Result<
                (crate::authentication::environment_provider::RequestAuthentication, Option<Lease>),
                ProviderFailure,
            >,
        >,
        refresh: impl FnOnce(Lease) -> Refresh,
    ) -> Result<FiniteHttpReceipt, AuthenticatedReadFailure>
    where
        Refresh: Future<
            Output = Result<
                (crate::authentication::environment_provider::RequestAuthentication, Lease),
                ProviderFailure,
            >,
        >,
    {
        let started = Instant::now();
        let (authentication, lease) = initial.await?;
        let mut receipt = self
            .finite_negotiated_query(method.clone(), segments, query, &authentication, fields, body)
            .await?;
        drop(authentication);
        if receipt.response.status == StatusCode::UNAUTHORIZED.as_u16() {
            if let Some(lease) = lease {
                let (authentication, _) = refresh(lease).await?;
                receipt = self
                    .finite_negotiated_query(method, segments, query, &authentication, fields, body)
                    .await?;
            }
        }
        receipt.elapsed_milliseconds =
            u64::try_from(started.elapsed().as_nanos().div_ceil(NANOSECONDS_PER_MILLISECOND))
                .unwrap_or(u64::MAX);
        Ok(receipt)
    }

    /// Fetches one fresh cross-site request forgery token from the selected
    /// author, through the provider with the same one-refresh-on-401 policy as
    /// any other bounded read. The token's only lifetime is the immediately
    /// following write it protects; it is never cached or returned onward.
    pub(crate) async fn fresh_token_authenticated(
        &self,
        provider: &EnvironmentAuthenticationProvider,
        source: &dyn AccessTokenSource,
        reading: u64,
    ) -> Result<FiniteHttpReceipt, AuthenticatedReadFailure> {
        self.authenticated_finite_get(
            provider,
            source,
            reading,
            &["libs", "granite", "csrf", "token.json"],
            &[],
            &HeaderMap::new(),
        )
        .await
    }

    /// Fetches one fresh cross-site request forgery token through the async
    /// provider, with the same one-refresh-on-401 policy as any other read.
    pub(crate) async fn fresh_token_authenticated_async<Clock, Utc>(
        &self,
        provider: &crate::authentication::environment_provider::AsyncEnvironmentAuthenticationProvider,
        clock: &Clock,
        utc: &Utc,
    ) -> Result<FiniteHttpReceipt, AuthenticatedReadFailure>
    where
        Clock: crate::authentication::identity_management_exchange::MonotonicClock + Sync,
        Utc: crate::authentication::token_assertion::CoordinatedUniversalTimeClock + Sync,
    {
        self.authenticated_finite_get_async(
            provider,
            clock,
            utc,
            &["libs", "granite", "csrf", "token.json"],
            &[],
            &HeaderMap::new(),
        )
        .await
    }

    /// Sends one POST, refreshing Cloud credentials once after a validated 401
    /// and repeating the identical request on that refresh alone.
    pub(crate) async fn authenticated_finite_post(
        &self,
        provider: &EnvironmentAuthenticationProvider,
        source: &dyn AccessTokenSource,
        reading: u64,
        segments: &[&str],
        query: &[(&str, &str)],
        fields: &HeaderMap,
        body: &[u8],
    ) -> Result<FiniteHttpReceipt, AuthenticatedReadFailure> {
        self.require_provider(provider)?;
        self.finite_with_authentication(
            Method::POST,
            segments,
            query,
            fields,
            body,
            async { provider.authenticate(&provider.author_endpoint(segments), reading, source) },
            |lease| async move { provider.refresh_after_unauthorized(lease, source) },
        )
        .await
    }

    /// Sends one POST through the async provider, with the same one-refresh
    /// policy. The identical request is repeated on that refresh alone.
    pub(crate) async fn authenticated_finite_post_async<Clock, Utc>(
        &self,
        provider: &crate::authentication::environment_provider::AsyncEnvironmentAuthenticationProvider,
        clock: &Clock,
        utc: &Utc,
        segments: &[&str],
        query: &[(&str, &str)],
        fields: &HeaderMap,
        body: &[u8],
    ) -> Result<FiniteHttpReceipt, AuthenticatedReadFailure>
    where
        Clock: crate::authentication::identity_management_exchange::MonotonicClock + Sync,
        Utc: crate::authentication::token_assertion::CoordinatedUniversalTimeClock + Sync,
    {
        self.require_provider(provider)?;
        let endpoint = provider.author_endpoint(segments);
        self.finite_with_authentication(
            Method::POST,
            segments,
            query,
            fields,
            body,
            provider.authenticate(&endpoint, clock, utc),
            |lease| provider.refresh_after_unauthorized(lease, clock, utc),
        )
        .await
    }
}
