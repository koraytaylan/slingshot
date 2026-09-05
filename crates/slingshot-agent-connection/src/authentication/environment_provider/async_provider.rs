//! Async provider strategy: one immutable snapshot, one cache, one IMS client.

use super::*;
use crate::authentication::{
    async_access_token_cache::{
        AsyncAccessTokenLease, AsyncAccessTokenSource, AsyncCloudAccessTokenCache,
    },
    async_identity_management_exchange::AsyncServiceCredentialTokenSource,
    identity_management_client::IdentityManagementClient,
    identity_management_exchange::MonotonicClock,
    token_assertion::CoordinatedUniversalTimeClock,
};

/// Owned async state. It is constructed only with the corresponding snapshot;
/// callers cannot replace its client or install a second cache through this type.
pub struct AsyncProviderState {
    cache: AsyncCloudAccessTokenCache,
    client: Option<IdentityManagementClient>,
}
impl core::fmt::Debug for AsyncProviderState {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str("AsyncProviderState([redacted])")
    }
}

/// Cancellation-owned authentication provider for the immutable runtime snapshot.
pub type AsyncEnvironmentAuthenticationProvider =
    EnvironmentAuthenticationProvider<AsyncProviderState>;

impl EnvironmentAuthenticationProvider<AsyncProviderState> {
    /// Cross-crate conformance seam: only TCP destination changes, and only to
    /// loopback. Fixed IMS TLS identity, frozen platform trust, and codecs remain
    /// unchanged. Not available in normal builds or runtime configuration.
    #[doc(hidden)]
    #[cfg(any(test, feature = "test-support"))]
    pub fn new_async_test_socket(
        snapshot: SelectedEnvironmentSnapshot,
        address: std::net::SocketAddr,
    ) -> Result<Self, ProviderFailure> {
        if !address.ip().is_loopback() {
            return Err(target_mismatch());
        }
        let mut provider = Self::new_async(snapshot)?;
        if let Some(client) = provider.cache.client.as_mut() {
            client.set_test_socket(address);
        }
        Ok(provider)
    }

    /// Creates exactly one async cache and, for Cloud, one fixed-endpoint IMS
    /// client from the snapshot's platform-only roots. Basic creates no IMS client.
    pub fn new_async(snapshot: SelectedEnvironmentSnapshot) -> Result<Self, ProviderFailure> {
        Self::new_async_with_identity_factory(
            snapshot,
            crate::authentication::access_token_cache::AccessTokenCacheIdentity::random_bytes,
        )
    }

    /// Deterministic initialization seam; the factory runs once and failure
    /// returns no provider. Runtime construction uses process randomness above.
    pub fn new_async_with_identity_factory(
        snapshot: SelectedEnvironmentSnapshot,
        factory: impl FnOnce() -> Result<[u8; 32], ExchangeFailure>,
    ) -> Result<Self, ProviderFailure> {
        let client = match &snapshot.authentication {
            SnapshotAuthentication::BasicCredentials { .. } => None,
            SnapshotAuthentication::ServiceCredentials { .. } => {
                Some(IdentityManagementClient::new(snapshot.identity_management_trust())?)
            }
        };
        Ok(Self {
            snapshot,
            cache: AsyncProviderState {
                cache: AsyncCloudAccessTokenCache::with_identity_factory(factory)?,
                client,
            },
        })
    }

    /// Authenticates from this provider's own selected credentials and IMS client.
    /// Clock inputs are runtime-owned; no caller-selected token source is involved.
    pub async fn authenticate<
        Clock: MonotonicClock + Sync,
        Utc: CoordinatedUniversalTimeClock + Sync,
    >(
        &self,
        endpoint: &str,
        clock: &Clock,
        utc: &Utc,
    ) -> Result<(RequestAuthentication, Option<AsyncAccessTokenLease>), ProviderFailure> {
        self.require_author_target(endpoint)?;
        self.require_permitted_transport()?;
        match &self.snapshot.authentication {
            SnapshotAuthentication::BasicCredentials { user_name, password } => {
                Ok((self.bind_authentication(basic_authentication(user_name, password)), None))
            }
            SnapshotAuthentication::ServiceCredentials { credentials } => {
                let client = self.cache.client.as_ref().ok_or_else(target_mismatch)?;
                let source =
                    AsyncServiceCredentialTokenSource::new(credentials, client, clock, utc);
                self.authenticate_with_source(endpoint, clock.reading_milliseconds(), &source).await
            }
        }
    }

    /// Replaces only the rejected cache generation using this snapshot's own
    /// service credentials. A foreign lease cannot invalidate this cache.
    pub async fn refresh_after_unauthorized<
        Clock: MonotonicClock + Sync,
        Utc: CoordinatedUniversalTimeClock + Sync,
    >(
        &self,
        lease: AsyncAccessTokenLease,
        clock: &Clock,
        utc: &Utc,
    ) -> Result<(RequestAuthentication, AsyncAccessTokenLease), ProviderFailure> {
        let SnapshotAuthentication::ServiceCredentials { credentials } =
            &self.snapshot.authentication
        else {
            return Err(target_mismatch());
        };
        let client = self.cache.client.as_ref().ok_or_else(target_mismatch)?;
        let source = AsyncServiceCredentialTokenSource::new(credentials, client, clock, utc);
        self.refresh_with_source(clock.reading_milliseconds(), lease, &source).await
    }

    /// Injects an explicitly trusted async source for transport tests/adapters.
    /// It shares the same cache and target checks as normal runtime authentication.
    pub async fn authenticate_with_source(
        &self,
        endpoint: &str,
        reading: u64,
        source: &dyn AsyncAccessTokenSource,
    ) -> Result<(RequestAuthentication, Option<AsyncAccessTokenLease>), ProviderFailure> {
        self.require_author_target(endpoint)?;
        self.require_permitted_transport()?;
        match &self.snapshot.authentication {
            SnapshotAuthentication::BasicCredentials { user_name, password } => {
                Ok((self.bind_authentication(basic_authentication(user_name, password)), None))
            }
            SnapshotAuthentication::ServiceCredentials { .. } => {
                let (value, lease) =
                    self.cache.cache.token(reading, source, bearer_authentication).await?;
                Ok((self.bind_authentication(value), Some(lease)))
            }
        }
    }

    /// Refreshes through an explicitly trusted source without replacing the
    /// provider's sole cache. Basic and foreign leases are refused before exchange.
    pub async fn refresh_with_source(
        &self,
        reading: u64,
        lease: AsyncAccessTokenLease,
        source: &dyn AsyncAccessTokenSource,
    ) -> Result<(RequestAuthentication, AsyncAccessTokenLease), ProviderFailure> {
        if !matches!(
            self.snapshot.authentication,
            SnapshotAuthentication::ServiceCredentials { .. }
        ) {
            return Err(target_mismatch());
        }
        let (value, lease) = self
            .cache
            .cache
            .refresh_after_unauthorized(reading, lease, source, bearer_authentication)
            .await?;
        Ok((self.bind_authentication(value), lease))
    }
}
fn target_mismatch() -> ProviderFailure {
    ProviderFailure::new(ConfigurationFailureCode::AuthenticationTargetMismatch)
}
