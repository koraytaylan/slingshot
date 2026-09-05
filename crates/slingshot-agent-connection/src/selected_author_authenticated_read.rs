//! Request-scoped authentication for finite, read-only selected-author exchanges.
//!
//! Only a completely framed Cloud 401 permits one refresh and one identical
//! GET. This API deliberately cannot submit a body or choose a write method.
//! Route-specific response interpretation remains the caller's responsibility.

use http::{HeaderMap, Method};
use std::future::Future;
use tokio::time::Instant;

use crate::authentication::access_token_cache::AccessTokenSource;
use crate::authentication::environment_provider::{
    EnvironmentAuthenticationProvider, ProviderFailure, SelectedAuthorConnectionRefusal,
};
use crate::selected_author_http::{FiniteHttpFailure, FiniteHttpReceipt};
use crate::selected_author_transport::SelectedAuthorTransport;

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
        let started = Instant::now();
        let (authentication, lease) = initial.await?;
        let mut receipt = self
            .finite_negotiated_query(Method::GET, segments, query, &authentication, fields, &[])
            .await?;
        drop(authentication);
        if receipt.response.status == 401 {
            if let Some(lease) = lease {
                let (authentication, _) = refresh(lease).await?;
                receipt = self
                    .finite_negotiated_query(
                        Method::GET,
                        segments,
                        query,
                        &authentication,
                        fields,
                        &[],
                    )
                    .await?;
            }
        }
        receipt.elapsed_milliseconds =
            u64::try_from(started.elapsed().as_nanos().div_ceil(1_000_000)).unwrap_or(u64::MAX);
        Ok(receipt)
    }
}
