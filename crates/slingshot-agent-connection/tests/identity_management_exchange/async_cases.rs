//! Real assertion construction through cancellation-owned async exchange.

use super::*;
use slingshot_agent_connection::authentication::async_identity_management_exchange::IdentityManagementReceipt;
use slingshot_agent_connection::authentication::{
    async_access_token_cache::{AsyncAccessTokenSource, AsyncCloudAccessTokenCache},
    async_identity_management_exchange::{
        AsyncIdentityManagementTransport, AsyncServiceCredentialTokenSource,
    },
};
use std::{
    future::Future,
    pin::Pin,
    sync::atomic::{AtomicU64, AtomicUsize, Ordering},
    task::{Context, Waker},
};

struct Clock(AtomicU64);
impl MonotonicClock for Clock {
    fn reading_milliseconds(&self) -> u64 {
        self.0.load(Ordering::SeqCst)
    }
}
struct Transport<'a> {
    clock: &'a Clock,
    response: Result<DecodedResponse, ExchangeFailure>,
    calls: AtomicUsize,
    drops: AtomicUsize,
    gate: tokio::sync::Semaphore,
}
struct DropCount<'a>(&'a AtomicUsize);
impl Drop for DropCount<'_> {
    fn drop(&mut self) {
        self.0.fetch_add(1, Ordering::SeqCst);
    }
}
impl AsyncIdentityManagementTransport for Transport<'_> {
    fn exchange<'a>(
        &'a self,
        body: &'a [u8],
        clock: &'a (dyn MonotonicClock + Sync),
    ) -> Pin<Box<dyn Future<Output = Result<IdentityManagementReceipt, ExchangeFailure>> + Send + 'a>>
    {
        Box::pin(async move {
            self.calls.fetch_add(1, Ordering::SeqCst);
            let _drop = DropCount(&self.drops);
            // Simulated connection setup can advance the clock before the
            // request anchor; the source must not substitute an earlier sample.
            self.clock.0.store(ANCHOR_READING, Ordering::SeqCst);
            let anchor = clock.reading_milliseconds();
            let fields: Vec<_> = url::form_urlencoded::parse(body).collect();
            let names = &ProfileAuthenticationContract::embedded()
                .literals
                .identity_management_request_fields;
            assert_eq!(fields.len(), names.len());
            for (field, name) in fields.iter().zip(names) {
                assert_eq!(field.0.as_ref(), name);
            }
            let credentials = credentials();
            assert_eq!(
                fields[0].1.as_bytes(),
                credentials.technical_account_client_identifier().as_bytes()
            );
            assert_eq!(fields[1].1.as_bytes(), credentials.client_secret().expose_secret_bytes());
            assertion(&credentials)
                .lend_compact_bytes(|bytes| assert_eq!(fields[2].1.as_bytes(), bytes));
            self.gate.acquire().await.unwrap().forget();
            self.clock.0.store(RECEIPT_READING, Ordering::SeqCst);
            self.response.clone().map(|response| {
                IdentityManagementReceipt::new(response, anchor, clock.reading_milliseconds())
            })
        })
    }
}
fn utc() -> FixedSecond {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(ASSERTION_FIXTURE);
    let vectors: serde_json::Value = serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap();
    FixedSecond(vectors["sampled_second"].as_u64().unwrap())
}
fn transport(
    clock: &Clock,
    response: Result<DecodedResponse, ExchangeFailure>,
    permits: usize,
) -> Transport<'_> {
    Transport {
        clock,
        response,
        calls: AtomicUsize::new(0),
        drops: AtomicUsize::new(0),
        gate: tokio::sync::Semaphore::new(permits),
    }
}

#[tokio::test]
async fn async_source_preserves_exact_form_validation_and_anchored_lease() {
    let credentials = credentials();
    let utc = utc();
    let limits = &ProfileAuthenticationContract::embedded().limits;
    let threshold = RECEIPT_READING - ANCHOR_READING
        + limits.access_token_refresh_skew_milliseconds
        + limits.minimum_access_token_usable_lease_milliseconds;
    for lifetime in [threshold, threshold + 1, COMFORTABLE_LIFETIME] {
        let clock = Clock(AtomicU64::new(0));
        let transport = transport(&clock, Ok(success(lifetime)), 1);
        let source = AsyncServiceCredentialTokenSource::new(&credentials, &transport, &clock, &utc);
        let result = source.exchange().await;
        if lifetime == threshold {
            assert_eq!(
                result.unwrap_err().code,
                ConfigurationFailureCode::AccessTokenLifetimeTooShort
            );
        } else {
            let token = result.unwrap();
            assert_eq!(token.deadline_milliseconds(), ANCHOR_READING + lifetime);
            token.lend_token_bytes(|bytes| assert_eq!(bytes, TOKEN.as_bytes()));
        }
        assert_eq!(transport.calls.load(Ordering::SeqCst), 1);
        assert_eq!(format!("{source:?}"), "AsyncServiceCredentialTokenSource([redacted])");
    }
}

#[tokio::test]
async fn async_source_reuses_closed_response_refusals_without_retry() {
    let credentials = credentials();
    let utc = utc();
    let mut redirect = success(COMFORTABLE_LIFETIME);
    redirect.head.status = REDIRECTION_STATUS;
    let mut invalid = success(COMFORTABLE_LIFETIME);
    invalid.body = b"{\"access_token\":\"secret\",\"extra\":true}".to_vec();
    for (response, expected) in [
        (Ok(redirect), ConfigurationFailureCode::IdentityManagementRedirectRefused),
        (Ok(invalid), ConfigurationFailureCode::IdentityManagementResponseDocumentInvalid),
        (
            Err(ExchangeFailure::new(ConfigurationFailureCode::IdentityManagementTransportFailed)),
            ConfigurationFailureCode::IdentityManagementTransportFailed,
        ),
    ] {
        let clock = Clock(AtomicU64::new(ANCHOR_READING));
        let transport = transport(&clock, response, 1);
        let source = AsyncServiceCredentialTokenSource::new(&credentials, &transport, &clock, &utc);
        assert_eq!(source.exchange().await.unwrap_err().code, expected);
        assert_eq!(transport.calls.load(Ordering::SeqCst), 1);
    }
}

#[tokio::test]
async fn cache_cancellation_drops_real_assertion_exchange_and_wakes_joiner() {
    let credentials = credentials();
    let utc = utc();
    let clock = Clock(AtomicU64::new(ANCHOR_READING));
    let transport = transport(&clock, Ok(success(COMFORTABLE_LIFETIME)), 0);
    let source = AsyncServiceCredentialTokenSource::new(&credentials, &transport, &clock, &utc);
    let cache = AsyncCloudAccessTokenCache::new().unwrap();
    let mut owner = Box::pin(cache.token(ANCHOR_READING, &source, |_| ()));
    let mut waiter = Box::pin(cache.token(ANCHOR_READING, &source, |_| ()));
    let mut context = Context::from_waker(Waker::noop());
    assert!(owner.as_mut().poll(&mut context).is_pending());
    assert!(waiter.as_mut().poll(&mut context).is_pending());
    drop(owner);
    assert_eq!(transport.drops.load(Ordering::SeqCst), 1);
    assert_eq!(
        waiter.await.unwrap_err().code,
        ConfigurationFailureCode::IdentityManagementCancelled
    );
    assert_eq!(transport.calls.load(Ordering::SeqCst), 1);
}

#[tokio::test]
async fn assertion_refusal_precedes_transport_invocation() {
    struct MissingUtc;
    impl CoordinatedUniversalTimeClock for MissingUtc {
        fn sample(&self) -> Option<u64> {
            None
        }
    }
    let credentials = credentials();
    let clock = Clock(AtomicU64::new(ANCHOR_READING));
    let transport = transport(&clock, Ok(success(COMFORTABLE_LIFETIME)), 0);
    let source =
        AsyncServiceCredentialTokenSource::new(&credentials, &transport, &clock, &MissingUtc);
    assert_eq!(
        source.exchange().await.unwrap_err().code,
        ConfigurationFailureCode::AssertionClockUnavailable
    );
    assert_eq!(transport.calls.load(Ordering::SeqCst), 0);
}
