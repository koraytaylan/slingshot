//! Async provider ownership, frozen binding, and the real finite-read entry point.

use super::*;
use slingshot_agent_connection::authentication::{
    async_access_token_cache::AsyncAccessTokenSource,
    environment_provider::AsyncEnvironmentAuthenticationProvider,
};
use std::{
    future::Future,
    pin::Pin,
    sync::atomic::{AtomicBool, AtomicUsize, Ordering},
    task::{Context, Waker},
};
use tokio::io::{AsyncReadExt, AsyncWriteExt};

fn async_provider(profile: &str, environment: &str) -> AsyncEnvironmentAuthenticationProvider {
    AsyncEnvironmentAuthenticationProvider::new_async(snapshot_from_loaded_with_platform(
        loaded(),
        profile,
        environment,
        platform(),
    ))
    .unwrap()
}

#[test]
fn identity_initialization_failure_returns_no_provider_and_never_retries() {
    let calls = AtomicUsize::new(0);
    let result = AsyncEnvironmentAuthenticationProvider::new_async_with_identity_factory(
        snapshot_from_loaded_with_platform(
            loaded(),
            CLEARTEXT_PROFILE,
            CLEARTEXT_ENVIRONMENT,
            platform(),
        ),
        || {
            calls.fetch_add(1, Ordering::SeqCst);
            Err(ExchangeFailure::new(ConfigurationFailureCode::IdentityManagementTransportFailed))
        },
    );
    assert_eq!(
        result.unwrap_err().code,
        ConfigurationFailureCode::IdentityManagementTransportFailed
    );
    assert_eq!(calls.load(Ordering::SeqCst), 1);
}
struct Source {
    calls: AtomicUsize,
    dropped: AtomicUsize,
    refusing: AtomicBool,
    gate: tokio::sync::Semaphore,
}
impl Source {
    fn new(permits: usize) -> Self {
        Self {
            calls: AtomicUsize::new(0),
            dropped: AtomicUsize::new(0),
            refusing: AtomicBool::new(false),
            gate: tokio::sync::Semaphore::new(permits),
        }
    }
}
struct DropCount<'a>(&'a AtomicUsize);
impl Drop for DropCount<'_> {
    fn drop(&mut self) {
        self.0.fetch_add(1, Ordering::SeqCst);
    }
}
impl AsyncAccessTokenSource for Source {
    fn exchange(
        &self,
    ) -> Pin<Box<dyn Future<Output = Result<AccessToken, ExchangeFailure>> + Send + '_>> {
        Box::pin(async {
            self.calls.fetch_add(1, Ordering::SeqCst);
            let _drop = DropCount(&self.dropped);
            self.gate.acquire().await.unwrap().forget();
            if self.refusing.load(Ordering::SeqCst) {
                Err(ExchangeFailure::new(
                    ConfigurationFailureCode::IdentityManagementTransportFailed,
                ))
            } else {
                exchanged_token()
            }
        })
    }
}
pub(super) struct NoClocks;
impl MonotonicClock for NoClocks {
    fn reading_milliseconds(&self) -> u64 {
        panic!("Basic/foreign requests must not ask the token clock")
    }
}
impl CoordinatedUniversalTimeClock for NoClocks {
    fn sample(&self) -> Option<u64> {
        panic!("Basic/foreign requests must not build an assertion")
    }
}
pub(super) struct Clock;
impl MonotonicClock for Clock {
    fn reading_milliseconds(&self) -> u64 {
        0
    }
}
pub(super) struct UnavailableUtc;
impl CoordinatedUniversalTimeClock for UnavailableUtc {
    fn sample(&self) -> Option<u64> {
        None
    }
}

pub(super) async fn prime_runtime_cache(provider: &AsyncEnvironmentAuthenticationProvider) {
    provider
        .authenticate_with_source(provider.snapshot().author().as_text(), 0, &Source::new(1))
        .await
        .unwrap();
}

#[tokio::test]
async fn basic_runtime_authentication_never_samples_token_clocks_or_exchanges() {
    let provider = async_provider(CLEARTEXT_PROFILE, CLEARTEXT_ENVIRONMENT);
    let (authentication, lease) = provider
        .authenticate(provider.snapshot().author().as_text(), &NoClocks, &NoClocks)
        .await
        .unwrap();
    assert!(lease.is_none());
    authentication
        .lend_value_bytes(|bytes| assert_eq!(bytes, b"Basic YWRtaW46bm90LWEtcmVhbC1wYXNzd29yZA=="));
    let source = Source::new(0);
    for endpoint in [
        provider.snapshot().publisher_metadata().as_text().to_owned(),
        format!("{}.evil.invalid", provider.snapshot().author().as_text()),
    ] {
        assert_eq!(
            provider.authenticate_with_source(&endpoint, 0, &source).await.unwrap_err().code,
            ConfigurationFailureCode::AuthenticationTargetMismatch
        );
    }
    assert_eq!(source.calls.load(Ordering::SeqCst), 0);
}

#[tokio::test]
async fn runtime_and_injected_paths_share_one_cache_and_failed_refresh_has_no_fallback() {
    let provider = async_provider(PROTECTED_PROFILE, PROTECTED_ENVIRONMENT);
    let endpoint = provider.snapshot().author().as_text();
    let source = Source::new(2);
    let (_, first) = provider.authenticate_with_source(endpoint, 0, &source).await.unwrap();
    let first = first.unwrap();
    let (_, runtime) = provider.authenticate(endpoint, &Clock, &UnavailableUtc).await.unwrap();
    assert_eq!(
        runtime.unwrap().generation(),
        first.generation(),
        "runtime path created another cache"
    );
    let (_, second) = provider.refresh_with_source(0, first.clone(), &source).await.unwrap();
    assert_eq!(second.generation(), 2);
    assert_eq!(provider.refresh_with_source(0, first, &source).await.unwrap().1.generation(), 2);
    let foreign = async_provider(PROTECTED_PROFILE, PROTECTED_ENVIRONMENT);
    assert_eq!(
        foreign.refresh_with_source(0, second.clone(), &source).await.unwrap_err().code,
        ConfigurationFailureCode::AuthenticationTargetMismatch
    );
    assert_eq!(source.calls.load(Ordering::SeqCst), 2);
    assert_eq!(
        provider
            .refresh_after_unauthorized(second, &Clock, &UnavailableUtc)
            .await
            .unwrap_err()
            .code,
        ConfigurationFailureCode::AssertionClockUnavailable
    );
    assert_eq!(
        provider.authenticate(endpoint, &Clock, &UnavailableUtc).await.unwrap_err().code,
        ConfigurationFailureCode::AssertionClockUnavailable
    );
}

#[tokio::test]
async fn provider_waiters_share_failure_and_owner_cancellation() {
    for cancelled in [false, true] {
        let provider = async_provider(PROTECTED_PROFILE, PROTECTED_ENVIRONMENT);
        let endpoint = provider.snapshot().author().as_text();
        let source = Source::new(0);
        source.refusing.store(true, Ordering::SeqCst);
        let mut owner = Box::pin(provider.authenticate_with_source(endpoint, 0, &source));
        let mut waiters: Vec<_> = (0..8)
            .map(|_| Box::pin(provider.authenticate_with_source(endpoint, 0, &source)))
            .collect();
        let mut context = Context::from_waker(Waker::noop());
        assert!(owner.as_mut().poll(&mut context).is_pending());
        for waiter in &mut waiters {
            assert!(waiter.as_mut().poll(&mut context).is_pending());
        }
        let expected = if cancelled {
            drop(owner);
            ConfigurationFailureCode::IdentityManagementCancelled
        } else {
            source.gate.add_permits(1);
            owner.await.unwrap_err().code
        };
        for waiter in waiters {
            assert_eq!(waiter.await.unwrap_err().code, expected);
        }
        assert_eq!(source.calls.load(Ordering::SeqCst), 1);
        assert_eq!(source.dropped.load(Ordering::SeqCst), 1);
    }
}

#[tokio::test]
async fn async_finite_get_preserves_basic_401_and_refuses_foreign_provider_before_io() {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let endpoint = format!("http://127.0.0.1:{}/context", listener.local_addr().unwrap().port());
    let mut files = profile_files();
    replace_profile(&mut files, "profiles/mike.toml", |text| {
        text.replace("http://author.example.com", &endpoint)
            .replace("allow_insecure_author_transport = true\n", "")
    });
    let provider =
        AsyncEnvironmentAuthenticationProvider::new_async(snapshot_from_loaded_with_platform(
            loaded_from_files(files),
            CLEARTEXT_PROFILE,
            CLEARTEXT_ENVIRONMENT,
            platform(),
        ))
        .unwrap();
    let transport = SelectedAuthorTransport::new(provider.snapshot().author_connection()).unwrap();
    let server = async {
        let (mut stream, _) = listener.accept().await.unwrap();
        let mut request = Vec::new();
        while !request.ends_with(b"\r\n\r\n") {
            request.push(stream.read_u8().await.unwrap());
        }
        let request = String::from_utf8(request).unwrap();
        assert!(request.starts_with("GET /context/probe?x=a%20b HTTP/1.1\r\n"));
        assert!(request.contains("Basic YWRtaW46bm90LWEtcmVhbC1wYXNzd29yZA=="));
        stream.write_all(b"HTTP/1.1 401 Unauthorized\r\nContent-Type: application/json\r\nContent-Length: 2\r\nConnection: close\r\n\r\n{}").await.unwrap();
        stream.shutdown().await.unwrap();
    };
    let fields = http::HeaderMap::new();
    let client = transport.authenticated_finite_get_async(
        &provider,
        &NoClocks,
        &NoClocks,
        &["probe"],
        &[("x", "a b")],
        &fields,
    );
    let (result, ()) = tokio::join!(client, server);
    assert_eq!(result.unwrap().response.status, 401);
    let foreign = async_provider(PROTECTED_PROFILE, PROTECTED_ENVIRONMENT);
    assert!(matches!(transport.authenticated_finite_get_async(&foreign, &NoClocks, &NoClocks, &["probe"], &[], &fields).await,
        Err(slingshot_agent_connection::selected_author_authenticated_read::AuthenticatedReadFailure::Selection(_))));
    assert!(
        tokio::time::timeout(std::time::Duration::from_millis(20), listener.accept())
            .await
            .is_err(),
        "Basic 401 or foreign provider caused another socket"
    );
}
