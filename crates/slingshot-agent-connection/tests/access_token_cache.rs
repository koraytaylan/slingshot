//! Assertions for the in-memory lease of one cloud access token.
//!
//! The property that matters is that one need produces one exchange. A
//! scheduled refresh and an unauthorized response are the same need, so they
//! join one flight; otherwise a server that rejects one request receives as
//! many exchanges as there are callers, which is how a rejection becomes an
//! outage.
//!
//! The other property is that a stale lease is harmless. A caller whose request
//! used a generation that has already been replaced is handed the replacement
//! rather than being allowed to evict a token somebody else just installed.

use std::cell::Cell;

use slingshot_agent_connection::authentication::access_token_cache::{
    AccessTokenSource, CloudAccessTokenCache,
};
use slingshot_agent_connection::authentication::cloud_service_credentials::CloudServiceCredentials;
use slingshot_agent_connection::authentication::identity_management_exchange::{
    AccessToken, DecodedHead, DecodedResponse, ExchangeFailure, IdentityManagementExchange,
    IdentityManagementTransport, MonotonicClock,
};
use slingshot_agent_connection::authentication::token_assertion::{
    CoordinatedUniversalTimeClock, ServiceCredentialAssertion,
};
use slingshot_domain::profile_authentication_contract::{
    ConfigurationFailureCode, ProfileAuthenticationContract,
};
use slingshot_domain::secret_value::SensitiveConfigurationDocument;

/// Credential document every exchange here is built from.
const CREDENTIAL_FIXTURE: &str = "../slingshot-test-support/fixtures/cloud-credentials/valid.json";

/// Identity every cache here is built with.
const CACHE_IDENTITY: u64 = 11;

/// Lifetime a comfortable response advertises, in milliseconds.
const COMFORTABLE_LIFETIME: u64 = 3_600_000;

/// Reading the clock reports when a token is exchanged.
const ANCHOR_READING: u64 = 0;

/// Status a successful exchange answers with.
const SUCCESS_STATUS: u16 = 200;

/// Milliseconds one scripted exchange takes.
const EXCHANGE_MILLISECONDS: u64 = 5;

/// Callers the concurrency assertion starts.
const CONCURRENT_CALLERS: u32 = 8;

/// A source that counts how often it was asked and can be made to fail.
struct CountingSource {
    /// Exchanges the source performed.
    exchanges: Cell<usize>,
    /// Whether the next exchange refuses.
    refusing: Cell<bool>,
}

impl CountingSource {
    /// Returns a source that always succeeds.
    fn new() -> Self {
        Self { exchanges: Cell::new(0), refusing: Cell::new(false) }
    }
}

impl AccessTokenSource for CountingSource {
    fn exchange(&self) -> Result<AccessToken, ExchangeFailure> {
        self.exchanges.set(self.exchanges.get() + 1);
        if self.refusing.get() {
            return Err(ExchangeFailure::new(
                ConfigurationFailureCode::IdentityManagementTransportFailed,
            ));
        }
        exchanged_token(COMFORTABLE_LIFETIME)
    }
}

/// A transport answering with one token of a scripted lifetime.
struct ScriptedTransport {
    /// Lifetime the answer advertises.
    lifetime: u64,
}

impl IdentityManagementTransport for ScriptedTransport {
    fn exchange(&self, _body: &[u8]) -> Result<DecodedResponse, ExchangeFailure> {
        Ok(DecodedResponse {
            informational: Vec::new(),
            head: DecodedHead {
                status: SUCCESS_STATUS,
                fields: vec![("content-type".to_owned(), "application/json".to_owned())],
            },
            body: format!(
                "{{\"access_token\":\"not-a-real-access-token\",\"token_type\":\"bearer\",\"expires_in\":{}}}",
                self.lifetime
            )
            .into_bytes(),
            trailer: None,
        })
    }
}

/// A clock reporting one fixed reading.
struct FixedReading;

impl MonotonicClock for FixedReading {
    fn reading_milliseconds(&self) -> u64 {
        ANCHOR_READING
    }
}

/// A clock reporting one fixed second.
struct FixedSecond(u64);

impl CoordinatedUniversalTimeClock for FixedSecond {
    fn sample(&self) -> Option<u64> {
        Some(self.0)
    }
}

/// Returns one token an exchange of `lifetime` produced.
fn exchanged_token(lifetime: u64) -> Result<AccessToken, ExchangeFailure> {
    let path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(CREDENTIAL_FIXTURE);
    let bytes = std::fs::read(&path).expect("the credential reads");
    let credentials =
        CloudServiceCredentials::parse(&SensitiveConfigurationDocument::from_bytes(bytes))
            .expect("the credential parses");
    let second = certificate_second(&credentials);
    let assertion = ServiceCredentialAssertion::build(&credentials, &FixedSecond(second))
        .expect("the assertion builds");
    IdentityManagementExchange::new(ScriptedTransport { lifetime }, FixedReading)
        .exchange(&credentials, &assertion)
}

/// Returns a second inside the credential certificate's validity.
fn certificate_second(credentials: &CloudServiceCredentials) -> u64 {
    use x509_parser::prelude::{FromDer, X509Certificate};

    let (_, parsed) =
        X509Certificate::from_der(credentials.public_certificate()).expect("it parses");
    u64::try_from(parsed.validity().not_before.timestamp()).expect("the second fits") + 1
}

/// Returns the bytes one leased token carries.
fn leased(token: &AccessToken) -> Vec<u8> {
    token.lend_token_bytes(<[u8]>::to_vec)
}

#[test]
fn a_usable_token_is_leased_again_rather_than_exchanged_again() {
    let cache = CloudAccessTokenCache::with_identity(CACHE_IDENTITY);
    let source = CountingSource::new();
    let (first, first_lease) =
        cache.token(ANCHOR_READING, &source, leased).expect("the first lease succeeds");
    let (second, second_lease) =
        cache.token(ANCHOR_READING, &source, leased).expect("the second lease succeeds");
    assert_eq!(first, second, "two leases carried different tokens");
    assert_eq!(first_lease, second_lease, "a usable token changed generation");
    assert_eq!(source.exchanges.get(), 1, "a usable token was exchanged twice");
}

#[test]
fn a_token_past_its_refresh_skew_is_replaced_once() {
    let skew =
        ProfileAuthenticationContract::embedded().limits.access_token_refresh_skew_milliseconds;
    let cache = CloudAccessTokenCache::with_identity(CACHE_IDENTITY);
    let source = CountingSource::new();
    let (_, before) = cache.token(ANCHOR_READING, &source, leased).expect("the lease succeeds");
    let due = ANCHOR_READING + COMFORTABLE_LIFETIME - skew;
    let (_, after) = cache.token(due, &source, leased).expect("the refresh succeeds");
    assert_ne!(after.generation(), before.generation(), "equality at the skew did not refresh");
    assert_eq!(source.exchanges.get(), 2, "the refresh exchanged more than once");
}

#[test]
fn a_rejected_generation_is_replaced_and_a_stale_lease_is_handed_the_replacement() {
    let cache = CloudAccessTokenCache::with_identity(CACHE_IDENTITY);
    let source = CountingSource::new();
    let (_, first) = cache.token(ANCHOR_READING, &source, leased).expect("the lease succeeds");
    let (_, second) = cache
        .refresh_after_unauthorized(first, &source, leased)
        .expect("the rejected generation is replaced");
    assert_ne!(second.generation(), first.generation(), "the generation did not advance");
    assert_eq!(source.exchanges.get(), 2);

    let (_, third) = cache
        .refresh_after_unauthorized(first, &source, leased)
        .expect("a stale lease is answered");
    assert_eq!(third.generation(), second.generation(), "a stale lease evicted a fresh token");
    assert_eq!(source.exchanges.get(), 2, "a stale lease caused another exchange");
}

#[test]
fn concurrent_callers_converge_on_one_replacement() {
    let cache = std::sync::Arc::new(CloudAccessTokenCache::with_identity(CACHE_IDENTITY));
    let exchanges = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));

    /// A source shared across threads that counts what it was asked for.
    struct SharedSource {
        /// Exchanges the source performed.
        exchanges: std::sync::Arc<std::sync::atomic::AtomicUsize>,
    }

    impl AccessTokenSource for SharedSource {
        fn exchange(&self) -> Result<AccessToken, ExchangeFailure> {
            self.exchanges.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            std::thread::sleep(std::time::Duration::from_millis(EXCHANGE_MILLISECONDS));
            exchanged_token(COMFORTABLE_LIFETIME)
        }
    }

    let callers: Vec<std::thread::JoinHandle<u64>> = (0..CONCURRENT_CALLERS)
        .map(|_| {
            let cache = std::sync::Arc::clone(&cache);
            let exchanges = std::sync::Arc::clone(&exchanges);
            std::thread::spawn(move || {
                let source = SharedSource { exchanges };
                let (_, lease) =
                    cache.token(ANCHOR_READING, &source, leased).expect("the lease succeeds");
                lease.generation()
            })
        })
        .collect();
    let generations: Vec<u64> =
        callers.into_iter().map(|caller| caller.join().expect("the caller finished")).collect();
    assert!(generations.iter().all(|generation| *generation == generations[0]), "{generations:?}");
    assert_eq!(
        exchanges.load(std::sync::atomic::Ordering::SeqCst),
        1,
        "concurrent callers each started their own exchange"
    );
}

#[test]
fn a_failed_exchange_installs_nothing_and_is_delivered_to_the_caller() {
    let cache = CloudAccessTokenCache::with_identity(CACHE_IDENTITY);
    let source = CountingSource::new();
    source.refusing.set(true);
    let refused = cache
        .token(ANCHOR_READING, &source, leased)
        .map_or_else(|failure| failure.code, |_| panic!("a refused exchange installed a token"));
    assert_eq!(refused, ConfigurationFailureCode::IdentityManagementTransportFailed);
    assert_eq!(source.exchanges.get(), 1, "a refused exchange was retried");

    source.refusing.set(false);
    let (_, lease) = cache.token(ANCHOR_READING, &source, leased).expect("the retry succeeds");
    assert_eq!(lease.generation(), 1, "a refused exchange consumed a generation");
}

#[test]
fn a_cache_identity_has_no_rendering_that_could_be_correlated() {
    let cache = CloudAccessTokenCache::with_identity(CACHE_IDENTITY);
    let rendered = format!("{:?}", cache.identity());
    assert!(!rendered.contains("not-a-real-access-token"), "{rendered}");
    let other = CloudAccessTokenCache::with_identity(CACHE_IDENTITY + 1);
    assert_ne!(cache.identity(), other.identity(), "two caches share one identity");
}
