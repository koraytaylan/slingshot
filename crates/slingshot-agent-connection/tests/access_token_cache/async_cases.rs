//! Deterministically polled async flights: no sleeps or detached tasks.

use super::*;
use slingshot_agent_connection::authentication::async_access_token_cache::{
    AsyncAccessTokenSource, AsyncCloudAccessTokenCache,
};
use std::{
    future::Future,
    pin::Pin,
    sync::{
        Arc,
        atomic::{AtomicBool, AtomicUsize, Ordering},
    },
    task::{Context, Poll, Wake, Waker},
};

struct Source {
    exchanges: AtomicUsize,
    dropped: AtomicUsize,
    refusing: AtomicBool,
    gate: tokio::sync::Semaphore,
}
impl Source {
    fn new() -> Self {
        Self {
            exchanges: AtomicUsize::new(0),
            dropped: AtomicUsize::new(0),
            refusing: AtomicBool::new(false),
            gate: tokio::sync::Semaphore::new(0),
        }
    }
    fn count(&self) -> usize {
        self.exchanges.load(Ordering::SeqCst)
    }
}
struct Dropped<'a>(&'a AtomicUsize);
impl Drop for Dropped<'_> {
    fn drop(&mut self) {
        self.0.fetch_add(1, Ordering::SeqCst);
    }
}
impl AsyncAccessTokenSource for Source {
    fn exchange(
        &self,
    ) -> Pin<Box<dyn Future<Output = Result<AccessToken, ExchangeFailure>> + Send + '_>> {
        Box::pin(async move {
            self.exchanges.fetch_add(1, Ordering::SeqCst);
            let _dropped = Dropped(&self.dropped);
            self.gate.acquire().await.unwrap().forget();
            if self.refusing.load(Ordering::SeqCst) {
                Err(ExchangeFailure::new(
                    ConfigurationFailureCode::IdentityManagementTransportFailed,
                ))
            } else {
                exchanged_token(COMFORTABLE_LIFETIME)
            }
        })
    }
}
fn pending<F: Future>(future: Pin<&mut F>) {
    assert!(matches!(future.poll(&mut Context::from_waker(Waker::noop())), Poll::Pending));
}

#[derive(Default)]
struct WakeCount(AtomicUsize);
impl Wake for WakeCount {
    fn wake(self: Arc<Self>) {
        self.0.fetch_add(1, Ordering::SeqCst);
    }
    fn wake_by_ref(self: &Arc<Self>) {
        self.0.fetch_add(1, Ordering::SeqCst);
    }
}

#[tokio::test]
async fn concurrent_success_and_failure_are_one_flight() {
    for refusing in [false, true] {
        let cache = AsyncCloudAccessTokenCache::with_identity_factory(|| Ok([17; 32])).unwrap();
        let source = Source::new();
        source.refusing.store(refusing, Ordering::SeqCst);
        let mut calls: Vec<_> =
            (0..CONCURRENT_CALLERS).map(|_| Box::pin(cache.token(0, &source, leased))).collect();
        for call in &mut calls {
            pending(call.as_mut());
        }
        assert_eq!(source.count(), 1);
        source.gate.add_permits(1);
        for call in calls {
            let result = call.await;
            if refusing {
                assert_eq!(
                    result.unwrap_err().code,
                    ConfigurationFailureCode::IdentityManagementTransportFailed
                );
            } else {
                assert_eq!(result.unwrap().1.generation(), 1);
            }
        }
        assert_eq!(source.count(), 1);
        assert_eq!(source.dropped.load(Ordering::SeqCst), 1);
    }
}

#[tokio::test]
async fn owner_cancellation_drops_exchange_and_fails_every_joined_waiter() {
    let cache = AsyncCloudAccessTokenCache::with_identity_factory(|| Ok([17; 32])).unwrap();
    let source = Source::new();
    let mut owner = Box::pin(cache.token(0, &source, leased));
    pending(owner.as_mut());
    let mut waiters: Vec<_> =
        (0..CONCURRENT_CALLERS).map(|_| Box::pin(cache.token(0, &source, leased))).collect();
    let wakes: Vec<_> = (0..CONCURRENT_CALLERS).map(|_| Arc::new(WakeCount::default())).collect();
    for (waiter, wake) in waiters.iter_mut().zip(&wakes) {
        let waker = Waker::from(wake.clone());
        assert!(waiter.as_mut().poll(&mut Context::from_waker(&waker)).is_pending());
    }
    drop(owner);
    assert!(wakes.iter().all(|wake| wake.0.load(Ordering::SeqCst) > 0));
    assert_eq!(source.dropped.load(Ordering::SeqCst), 1);
    // A later successful flight cannot overwrite the old flight's cancellation.
    source.gate.add_permits(1);
    assert_eq!(cache.token(0, &source, leased).await.unwrap().1.generation(), 1);
    for waiter in waiters {
        assert_eq!(
            waiter.await.unwrap_err().code,
            ConfigurationFailureCode::IdentityManagementCancelled
        );
    }
    assert_eq!(source.count(), 2);
}

#[tokio::test]
async fn waiter_cancellation_leaves_owner_running() {
    let cache = AsyncCloudAccessTokenCache::with_identity_factory(|| Ok([17; 32])).unwrap();
    let source = Source::new();
    let mut owner = Box::pin(cache.token(0, &source, leased));
    pending(owner.as_mut());
    let mut waiter = Box::pin(cache.token(0, &source, leased));
    pending(waiter.as_mut());
    drop(waiter);
    assert_eq!(source.dropped.load(Ordering::SeqCst), 0);
    source.gate.add_permits(1);
    assert_eq!(owner.await.unwrap().1.generation(), 1);
    assert_eq!(source.count(), 1);
}

#[tokio::test]
async fn stale_and_foreign_leases_cannot_evict_a_replacement() {
    let cache = AsyncCloudAccessTokenCache::with_identity_factory(|| Ok([17; 32])).unwrap();
    let source = Source::new();
    source.gate.add_permits(2);
    let (_, first) = cache.token(0, &source, leased).await.unwrap();
    let (_, second) =
        cache.refresh_after_unauthorized(0, first.clone(), &source, leased).await.unwrap();
    assert_eq!(second.generation(), 2);
    let (_, third) = cache.refresh_after_unauthorized(0, first, &source, leased).await.unwrap();
    assert_eq!(third.generation(), 2);
    let other = AsyncCloudAccessTokenCache::with_identity_factory(|| Ok([17; 32])).unwrap();
    assert_eq!(
        other
            .refresh_after_unauthorized(0, second.clone(), &source, leased)
            .await
            .unwrap_err()
            .code,
        ConfigurationFailureCode::AuthenticationTargetMismatch
    );
    assert_eq!(source.count(), 2);
    assert_eq!(format!("{second:?}"), "AsyncAccessTokenLease([redacted])");
    assert_eq!(format!("{cache:?}"), "AsyncCloudAccessTokenCache([redacted])");
}

#[tokio::test]
async fn failed_refresh_has_no_fallback_and_short_replacement_has_no_recursive_exchange() {
    let cache = AsyncCloudAccessTokenCache::with_identity_factory(|| Ok([17; 32])).unwrap();
    let source = Source::new();
    source.gate.add_permits(4);
    let (_, first) = cache.token(0, &source, leased).await.unwrap();
    source.refusing.store(true, Ordering::SeqCst);
    assert!(cache.refresh_after_unauthorized(0, first, &source, leased).await.is_err());
    assert!(cache.token(0, &source, |_| panic!("served fallback")).await.is_err());
    assert_eq!(source.count(), 3);
    source.refusing.store(false, Ordering::SeqCst);
    assert_eq!(
        cache.token(COMFORTABLE_LIFETIME, &source, leased).await.unwrap_err().code,
        ConfigurationFailureCode::AccessTokenLifetimeTooShort
    );
    assert_eq!(source.count(), 4);
}
