//! Signed-source and cache flights through the real fixed-identity TLS codecs.
use super::*;
use crate::authentication::{
    async_access_token_cache::AsyncCloudAccessTokenCache,
    async_identity_management_exchange::AsyncServiceCredentialTokenSource,
    cloud_service_credentials::CloudServiceCredentials,
    token_assertion::{CoordinatedUniversalTimeClock, ServiceCredentialAssertion},
};
use crate::selected_author_http2::tests::frame;
use slingshot_domain::secret_value::SensitiveConfigurationDocument;
use std::{net::SocketAddr, sync::atomic::AtomicUsize, task::Poll};

#[path = "identity_management_provider_tests.rs"]
mod provider_tests;

struct Transport {
    client: IdentityManagementClient,
    address: SocketAddr,
    calls: AtomicUsize,
}
impl AsyncIdentityManagementTransport for Transport {
    fn exchange<'a>(
        &'a self,
        body: &'a [u8],
        clock: &'a (dyn MonotonicClock + Sync),
    ) -> Pin<Box<dyn Future<Output = Result<IdentityManagementReceipt, ExchangeFailure>> + Send + 'a>>
    {
        Box::pin(async move {
            self.calls.fetch_add(1, Ordering::SeqCst);
            exchange_connecting(
                async {
                    self.client
                        .connector
                        .connect_test_socket(TcpStream::connect(self.address).await.unwrap())
                        .await
                },
                body,
                clock,
            )
            .await
        })
    }
}
struct Utc(AtomicU64);
impl CoordinatedUniversalTimeClock for Utc {
    fn sample(&self) -> Option<u64> {
        Some(self.0.fetch_add(1, Ordering::SeqCst))
    }
}
struct Fixed(u64);
impl CoordinatedUniversalTimeClock for Fixed {
    fn sample(&self) -> Option<u64> {
        Some(self.0)
    }
}
fn credentials() -> CloudServiceCredentials {
    CloudServiceCredentials::parse(&SensitiveConfigurationDocument::from_bytes(
        include_bytes!("../../../slingshot-test-support/fixtures/cloud-credentials/valid.json")
            .to_vec(),
    ))
    .unwrap()
}
fn vectors() -> serde_json::Value {
    serde_json::from_slice(include_bytes!(
        "../../../slingshot-test-support/fixtures/token-assertions/assertion-vectors.json"
    ))
    .unwrap()
}
async fn signed_request(
    peer: &mut Peer,
    h2: bool,
    credentials: &CloudServiceCredentials,
    second: u64,
) {
    let body = if h2 {
        let mut preface = [0; 24];
        peer.read_exact(&mut preface).await.unwrap();
        assert_eq!(&preface, b"PRI * HTTP/2.0\r\n\r\nSM\r\n\r\n");
        assert_eq!(receive(peer).await.0, 4);
        peer.write_all(&frame(4, 0, 0, &[])).await.unwrap();
        peer.write_all(&frame(4, 1, 0, &[])).await.unwrap();
        assert_eq!(receive(peer).await.0, 4);
        let (kind, _, head) = receive(peer).await;
        assert_eq!(kind, 1);
        for value in [b"ims-na1.adobelogin.com".as_slice(), b"/ims/exchange/jwt"] {
            assert!(head.windows(value.len()).any(|w| w == value));
        }
        let mut body = Vec::new();
        loop {
            let (kind, flags, payload) = receive(peer).await;
            assert_eq!(kind, 0);
            body.extend_from_slice(&payload);
            if flags & 1 != 0 {
                break;
            }
        }
        body
    } else {
        let mut head = Vec::new();
        while !head.ends_with(b"\r\n\r\n") {
            head.push(peer.read_u8().await.unwrap());
            assert!(head.len() < 8192);
        }
        let head = String::from_utf8(head).unwrap();
        assert!(head.starts_with("POST /ims/exchange/jwt HTTP/1.1\r\n"));
        assert!(head.to_ascii_lowercase().contains("host: ims-na1.adobelogin.com\r\n"));
        let length: usize = head
            .lines()
            .find_map(|line| {
                line.to_ascii_lowercase().strip_prefix("content-length: ").map(str::to_owned)
            })
            .unwrap()
            .parse()
            .unwrap();
        assert!(length < 8192);
        let mut body = vec![0; length];
        peer.read_exact(&mut body).await.unwrap();
        body
    };
    let fields: Vec<_> = url::form_urlencoded::parse(&body).collect();
    assert_eq!(
        fields.iter().map(|p| p.0.as_ref()).collect::<Vec<_>>(),
        ["client_id", "client_secret", "jwt_token"]
    );
    assert_eq!(
        fields[0].1.as_bytes(),
        credentials.technical_account_client_identifier().as_bytes()
    );
    assert_eq!(fields[1].1.as_bytes(), credentials.client_secret().expose_secret_bytes());
    ServiceCredentialAssertion::build(credentials, &Fixed(second))
        .unwrap()
        .lend_compact_bytes(|expected| assert_eq!(fields[2].1.as_bytes(), expected));
    let vectors = vectors();
    if second == vectors["sampled_second"].as_u64().unwrap() {
        assert_eq!(fields[2].1.as_ref(), vectors["compact"].as_str().unwrap());
    }
}
async fn answer(peer: &mut Peer, h2: bool, number: usize) {
    let body = format!(
        "{{\"access_token\":\"fixture-{number}\",\"token_type\":\"bearer\",\"expires_in\":3600000}}"
    );
    if h2 {
        let mut head = vec![0x88, 0x0f, 16, 16];
        head.extend_from_slice(b"application/json");
        peer.write_all(&frame(1, 4, 1, &head)).await.unwrap();
        peer.write_all(&frame(0, 1, 1, body.as_bytes())).await.unwrap();
        while receive(peer).await.0 != 7 {}
    } else {
        peer.write_all(format!("HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",body.len()).as_bytes()).await.unwrap();
    }
    peer.shutdown().await.unwrap();
}

#[tokio::test]
async fn signed_cache_refresh_uses_new_assertion_and_preserves_generation_ownership() {
    for h2 in [false, true] {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let transport = Transport {
            client: client(true),
            address: listener.local_addr().unwrap(),
            calls: AtomicUsize::new(0),
        };
        let acceptor =
            server(&rustls::version::TLS13, Some(if h2 { b"h2" } else { b"http/1.1" }), false);
        let credentials = credentials();
        let second = vectors()["sampled_second"].as_u64().unwrap();
        let utc = Utc(AtomicU64::new(second));
        let clock = Clock(AtomicU64::new(0));
        let source = AsyncServiceCredentialTokenSource::new(&credentials, &transport, &clock, &utc);
        let cache = AsyncCloudAccessTokenCache::new().unwrap();
        let request = async {
            let (_, first) = cache
                .token(0, &source, |token| token.lend_token_bytes(|b| assert_eq!(b, b"fixture-1")))
                .await
                .unwrap();
            assert_eq!(first.generation(), 1);
            let (_, second) = cache
                .refresh_after_unauthorized(0, first.clone(), &source, |token| {
                    token.lend_token_bytes(|b| assert_eq!(b, b"fixture-2"))
                })
                .await
                .unwrap();
            assert_eq!(second.generation(), 2);
            let (_, stale) =
                cache.refresh_after_unauthorized(0, first, &source, |_| ()).await.unwrap();
            assert_eq!(stale.generation(), 2);
            let foreign = AsyncCloudAccessTokenCache::new().unwrap();
            assert_eq!(
                foreign
                    .refresh_after_unauthorized(0, second, &source, |_| ())
                    .await
                    .unwrap_err()
                    .code,
                ConfigurationFailureCode::AuthenticationTargetMismatch
            );
        };
        let peer = async {
            for number in 1..=2 {
                let (socket, _) = listener.accept().await.unwrap();
                let mut peer = acceptor.accept(socket).await.unwrap();
                signed_request(&mut peer, h2, &credentials, second + number as u64 - 1).await;
                answer(&mut peer, h2, number).await;
            }
        };
        tokio::time::timeout(Duration::from_secs(10), async { tokio::join!(request, peer) })
            .await
            .unwrap();
        assert_eq!(transport.calls.load(Ordering::SeqCst), 2);
        assert_eq!(utc.0.load(Ordering::SeqCst), second + 2);
        assert!(tokio::time::timeout(Duration::from_millis(10), listener.accept()).await.is_err());
    }
}

#[tokio::test]
async fn cancelled_signed_refresh_closes_tls_wakes_joiner_and_never_serves_old_token() {
    for h2 in [false, true] {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let transport = Transport {
            client: client(true),
            address: listener.local_addr().unwrap(),
            calls: AtomicUsize::new(0),
        };
        let acceptor =
            server(&rustls::version::TLS13, Some(if h2 { b"h2" } else { b"http/1.1" }), false);
        let credentials = credentials();
        let second = vectors()["sampled_second"].as_u64().unwrap();
        let utc = Utc(AtomicU64::new(second));
        let clock = Clock(AtomicU64::new(0));
        let source = AsyncServiceCredentialTokenSource::new(&credentials, &transport, &clock, &utc);
        let cache = AsyncCloudAccessTokenCache::new().unwrap();
        let (sent, ready) = tokio::sync::oneshot::channel();
        let request = async {
            let (_, lease) = cache.token(0, &source, |_| ()).await.unwrap();
            let mut owner = Box::pin(cache.refresh_after_unauthorized(0, lease, &source, |_| {
                panic!("cancelled refresh cannot install")
            }));
            tokio::select! {result=&mut owner=>panic!("unfinished refresh returned {result:?}"),result=ready=>result.unwrap()};
            let mut waiter =
                Box::pin(cache.token(0, &source, |_| panic!("old token cannot survive refresh")));
            std::future::poll_fn(|cx| {
                assert!(waiter.as_mut().poll(cx).is_pending());
                Poll::Ready(())
            })
            .await;
            drop(owner);
            assert_eq!(
                waiter.await.unwrap_err().code,
                ConfigurationFailureCode::IdentityManagementCancelled
            );
            let (_, replacement) = cache
                .token(0, &source, |token| token.lend_token_bytes(|b| assert_eq!(b, b"fixture-3")))
                .await
                .unwrap();
            assert_eq!(replacement.generation(), 2);
        };
        let peer = async {
            let mut sent = Some(sent);
            for number in 1..=3 {
                let (socket, _) = listener.accept().await.unwrap();
                let mut peer = acceptor.accept(socket).await.unwrap();
                signed_request(&mut peer, h2, &credentials, second + number as u64 - 1).await;
                if number == 2 {
                    sent.take().unwrap().send(()).unwrap();
                    let mut byte = [0];
                    assert!(!matches!(peer.read(&mut byte).await, Ok(1)));
                } else {
                    answer(&mut peer, h2, number).await;
                }
            }
        };
        tokio::time::timeout(Duration::from_secs(10), async { tokio::join!(request, peer) })
            .await
            .unwrap();
        assert_eq!(transport.calls.load(Ordering::SeqCst), 3);
        assert_eq!(utc.0.load(Ordering::SeqCst), second + 3);
        assert!(tokio::time::timeout(Duration::from_millis(10), listener.accept()).await.is_err());
    }
}
