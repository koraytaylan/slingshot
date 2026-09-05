//! Cancellation-owned asynchronous token flights. No exchange task is detached.

use super::access_token_cache::AccessTokenCacheIdentity;
use super::identity_management_exchange::{AccessToken, ExchangeFailure};
use slingshot_domain::profile_authentication_contract::ConfigurationFailureCode;
use std::{
    future::Future,
    pin::Pin,
    sync::{Arc, Mutex},
};
use tokio::sync::watch;

/// Token exchange whose future owns its network operation and cancels on drop.
pub trait AsyncAccessTokenSource: Send + Sync {
    /// Starts one bounded exchange; dropping this future must stop its work.
    fn exchange(
        &self,
    ) -> Pin<Box<dyn Future<Output = Result<AccessToken, ExchangeFailure>> + Send + '_>>;
}

/// Unforgeable process-local claim on one installed async-cache generation.
#[derive(Clone)]
pub struct AsyncAccessTokenLease {
    owner: Arc<AccessTokenCacheIdentity>,
    generation: u64,
}
impl AsyncAccessTokenLease {
    /// Installed generation, never credential-derived.
    pub fn generation(&self) -> u64 {
        self.generation
    }
}
impl core::fmt::Debug for AsyncAccessTokenLease {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str("AsyncAccessTokenLease([redacted])")
    }
}

struct Flight {
    completed: watch::Sender<Option<Result<(), ExchangeFailure>>>,
}
#[derive(Default)]
struct State {
    token: Option<AccessToken>,
    generation: u64,
    flight: Option<Arc<Flight>>,
}

/// One cache with at most one in-progress exchange and shared failure delivery.
/// Mutexes protect only memory transitions, never network awaits. Tokens are
/// removed before refresh, so failed or cancelled flights cannot serve fallback.
pub struct AsyncCloudAccessTokenCache {
    owner: Arc<AccessTokenCacheIdentity>,
    state: Mutex<State>,
}
impl core::fmt::Debug for AsyncCloudAccessTokenCache {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str("AsyncCloudAccessTokenCache([redacted])")
    }
}
impl AsyncCloudAccessTokenCache {
    /// Creates one process-random opaque identity using the transport crypto
    /// provider. Entropy failure refuses initialization; no fallback is installed.
    pub fn new() -> Result<Self, ExchangeFailure> {
        Self::with_identity_factory(AccessTokenCacheIdentity::random_bytes)
    }

    /// Injects identity generation for deterministic tests. Even repeated factory
    /// bytes cannot let a lease cross separately allocated cache ownership.
    pub fn with_identity_factory(
        factory: impl FnOnce() -> Result<[u8; 32], ExchangeFailure>,
    ) -> Result<Self, ExchangeFailure> {
        Ok(Self {
            owner: Arc::new(AccessTokenCacheIdentity::from_factory(factory)?),
            state: Mutex::new(State::default()),
        })
    }

    /// Obtains a usable token or joins the current exchange. `reading` uses the
    /// source's monotonic domain; time spent waiting is included when rechecking.
    pub async fn token<T>(
        &self,
        reading: u64,
        source: &dyn AsyncAccessTokenSource,
        use_token: impl FnOnce(&AccessToken) -> T,
    ) -> Result<(T, AsyncAccessTokenLease), ExchangeFailure> {
        self.acquire(reading, None, source, use_token).await
    }

    /// Invalidates only the rejected generation, then joins one replacement
    /// flight. Foreign leases are refused before changing state or exchanging.
    pub async fn refresh_after_unauthorized<T>(
        &self,
        reading: u64,
        lease: AsyncAccessTokenLease,
        source: &dyn AsyncAccessTokenSource,
        use_token: impl FnOnce(&AccessToken) -> T,
    ) -> Result<(T, AsyncAccessTokenLease), ExchangeFailure> {
        if !Arc::ptr_eq(&lease.owner, &self.owner) {
            return Err(ExchangeFailure::new(
                ConfigurationFailureCode::AuthenticationTargetMismatch,
            ));
        }
        self.acquire(reading, Some(lease.generation), source, use_token).await
    }

    async fn acquire<T>(
        &self,
        reading: u64,
        mut rejected: Option<u64>,
        source: &dyn AsyncAccessTokenSource,
        use_token: impl FnOnce(&AccessToken) -> T,
    ) -> Result<(T, AsyncAccessTokenLease), ExchangeFailure> {
        let started = tokio::time::Instant::now();
        loop {
            let (flight, owner) = {
                let mut state =
                    self.state.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
                if rejected.take().is_some_and(|generation| generation == state.generation) {
                    state.token = None;
                }
                let now = reading.saturating_add(
                    u64::try_from(started.elapsed().as_nanos().div_ceil(1_000_000))
                        .unwrap_or(u64::MAX),
                );
                if let Some(token) =
                    state.token.as_ref().filter(|token| !token.refresh_required(now))
                {
                    return Ok((
                        use_token(token),
                        AsyncAccessTokenLease {
                            owner: self.owner.clone(),
                            generation: state.generation,
                        },
                    ));
                }
                if let Some(flight) = &state.flight {
                    (flight.clone(), false)
                } else {
                    state.token = None;
                    state.generation.checked_add(1).ok_or_else(|| {
                        ExchangeFailure::new(
                            ConfigurationFailureCode::AccessTokenInstallationGenerationExhausted,
                        )
                    })?;
                    let (completed, _) = watch::channel(None);
                    let flight = Arc::new(Flight { completed });
                    state.flight = Some(flight.clone());
                    (flight, true)
                }
            };
            if owner {
                let mut guard = FlightOwner { cache: self, flight: flight.clone(), armed: true };
                let replacement = source.exchange().await;
                let result = {
                    let mut state =
                        self.state.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
                    let now = reading.saturating_add(
                        u64::try_from(started.elapsed().as_nanos().div_ceil(1_000_000))
                            .unwrap_or(u64::MAX),
                    );
                    let result = replacement.and_then(|token| {
                        if token.refresh_required(now) {
                            return Err(ExchangeFailure::new(
                                ConfigurationFailureCode::AccessTokenLifetimeTooShort,
                            ));
                        }
                        state.generation += 1; // checked before the exclusive flight began
                        state.token = Some(token);
                        Ok(())
                    });
                    state.flight = None;
                    flight.completed.send_replace(Some(result));
                    guard.armed = false;
                    result
                };
                result?;
            } else {
                let mut receiver = flight.completed.subscribe();
                loop {
                    let result = *receiver.borrow_and_update();
                    if let Some(result) = result {
                        result?;
                        break;
                    }
                    receiver.changed().await.map_err(|_| cancelled())?;
                }
            }
        }
    }
}

struct FlightOwner<'a> {
    cache: &'a AsyncCloudAccessTokenCache,
    flight: Arc<Flight>,
    armed: bool,
}
impl Drop for FlightOwner<'_> {
    fn drop(&mut self) {
        if self.armed {
            let mut state =
                self.cache.state.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
            if state.flight.as_ref().is_some_and(|flight| Arc::ptr_eq(flight, &self.flight)) {
                state.flight = None;
                state.token = None;
                self.flight.completed.send_replace(Some(Err(cancelled())));
            }
        }
    }
}
fn cancelled() -> ExchangeFailure {
    ExchangeFailure::new(ConfigurationFailureCode::IdentityManagementCancelled)
}

#[cfg(test)]
mod tests {
    use super::*;

    struct NoExchange;
    impl AsyncAccessTokenSource for NoExchange {
        fn exchange(
            &self,
        ) -> Pin<Box<dyn Future<Output = Result<AccessToken, ExchangeFailure>> + Send + '_>>
        {
            panic!("exhausted generations must fail before source invocation")
        }
    }

    #[test]
    fn identities_are_random_redacted_and_factory_failures_are_terminal() {
        let first = AsyncCloudAccessTokenCache::new().unwrap();
        let second = AsyncCloudAccessTokenCache::new().unwrap();
        assert_ne!(first.owner.as_ref(), second.owner.as_ref());
        assert_eq!(format!("{:?}", first.owner), "AccessTokenCacheIdentity([redacted])");
        let calls = std::cell::Cell::new(0);
        let error = AsyncCloudAccessTokenCache::with_identity_factory(|| {
            calls.set(calls.get() + 1);
            Err(ExchangeFailure::new(ConfigurationFailureCode::IdentityManagementTransportFailed))
        })
        .unwrap_err();
        assert_eq!(calls.get(), 1);
        assert_eq!(error.code, ConfigurationFailureCode::IdentityManagementTransportFailed);
    }

    #[tokio::test]
    async fn repeated_test_identity_bytes_do_not_allow_foreign_leases() {
        let first = AsyncCloudAccessTokenCache::with_identity_factory(|| Ok([7; 32])).unwrap();
        let second = AsyncCloudAccessTokenCache::with_identity_factory(|| Ok([7; 32])).unwrap();
        assert_eq!(first.owner.as_ref(), second.owner.as_ref());
        let lease = AsyncAccessTokenLease { owner: first.owner.clone(), generation: 1 };
        assert_eq!(
            second
                .refresh_after_unauthorized(0, lease, &NoExchange, |_| ())
                .await
                .unwrap_err()
                .code,
            ConfigurationFailureCode::AuthenticationTargetMismatch
        );
        let state = second.state.lock().unwrap();
        assert!(state.token.is_none() && state.flight.is_none());
        assert_eq!(state.generation, 0);
    }

    #[tokio::test]
    async fn exhausted_generation_never_starts_a_flight() {
        let cache = AsyncCloudAccessTokenCache::new().unwrap();
        cache.state.lock().unwrap().generation = u64::MAX;
        assert_eq!(
            cache.token(0, &NoExchange, |_| ()).await.unwrap_err().code,
            ConfigurationFailureCode::AccessTokenInstallationGenerationExhausted
        );
        assert!(cache.state.lock().unwrap().flight.is_none());
    }
}
