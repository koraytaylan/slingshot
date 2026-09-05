//! Fixed IMS TLS identity and codec dispatch on one real loopback connection.
use super::*;
use rustls_pki_types::{CertificateDer, PrivateKeyDer, pem::PemObject};
use slingshot_configuration::{
    platform_trust::{
        PlatformTrustSnapshot, PlatformTrustSource, ProviderDecision, ProviderRecord,
    },
    profile_loader::ConfigurationDiagnostic,
};
use std::sync::{
    Arc,
    atomic::{AtomicU64, Ordering},
};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::{TcpListener, TcpStream},
};

const CERT: &[u8] = include_bytes!("../../tests/fixtures/selected-author-tls/ims-test-leaf.pem");
const BODY: &[u8] = b"client_id=fixture&client_secret=test-only&jwt_token=fixture";
const ANSWER: &[u8] =
    b"{\"access_token\":\"fixture\",\"token_type\":\"bearer\",\"expires_in\":3600000}";
struct Store(Vec<u8>);
impl PlatformTrustSource for Store {
    fn records(&self) -> Result<Vec<ProviderRecord>, ConfigurationDiagnostic> {
        Ok(vec![ProviderRecord {
            der: self.0.clone(),
            decision: ProviderDecision::UnconditionallyTrustedForServerAuthentication,
        }])
    }
}
fn client(trusted: bool) -> IdentityManagementClient {
    let pem = if trusted {
        include_bytes!("../../tests/fixtures/selected-author-tls/ims-test-root.pem").as_slice()
    } else {
        include_bytes!("../../tests/fixtures/selected-author-tls/root.pem")
    };
    let root = CertificateDer::from_pem_slice(pem).unwrap();
    let snapshot = PlatformTrustSnapshot::take(&Store(root.as_ref().to_vec())).unwrap();
    IdentityManagementClient::new(&IdentityManagementTrustInput::from_platform(&snapshot).unwrap())
        .unwrap()
}
fn server(
    version: &'static rustls::SupportedProtocolVersion,
    alpn: Option<&[u8]>,
    wrong_name: bool,
) -> tokio_rustls::TlsAcceptor {
    let mut config = rustls::ServerConfig::builder_with_provider(Arc::new(
        rustls::crypto::ring::default_provider(),
    ))
    .with_protocol_versions(&[version])
    .unwrap()
    .with_no_client_auth()
    .with_single_cert(
        vec![
            CertificateDer::from_pem_slice(if wrong_name {
                include_bytes!("../../tests/fixtures/selected-author-tls/leaf.pem")
            } else {
                CERT
            })
            .unwrap(),
        ],
        PrivateKeyDer::from_pem_slice(include_bytes!(
            "../../tests/fixtures/selected-author-tls/test-only-private-key.pem"
        ))
        .unwrap(),
    )
    .unwrap();
    config.alpn_protocols = alpn.map(|p| vec![p.to_vec()]).unwrap_or_default();
    tokio_rustls::TlsAcceptor::from(Arc::new(config))
}
struct Clock(AtomicU64);
impl MonotonicClock for Clock {
    fn reading_milliseconds(&self) -> u64 {
        self.0.fetch_add(100, Ordering::SeqCst)
    }
}
type Peer = tokio_rustls::server::TlsStream<TcpStream>;

#[path = "identity_management_client_token_tests.rs"]
mod token_tests;
async fn receive(peer: &mut Peer) -> (u8, u8, Vec<u8>) {
    let mut header = [0; 9];
    peer.read_exact(&mut header).await.unwrap();
    let length =
        usize::from(header[0]) * 65536 + usize::from(header[1]) * 256 + usize::from(header[2]);
    assert!(length <= 16384);
    let mut payload = vec![0; length];
    peer.read_exact(&mut payload).await.unwrap();
    (header[3], header[4], payload)
}

#[tokio::test]
async fn fixed_identity_tls_dispatches_one_exact_request_on_the_authenticated_socket() {
    use crate::selected_author_http2::tests::frame;
    for version in [&rustls::version::TLS12, &rustls::version::TLS13] {
        for alpn in [None, Some(b"http/1.1".as_slice()), Some(b"h2".as_slice())] {
            let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
            let client = client(true);
            let acceptor = server(version, alpn, false);
            let clock = Clock(AtomicU64::new(0));
            let connect = async {
                let socket = TcpStream::connect(listener.local_addr().unwrap()).await.unwrap();
                client.connector.connect_test_socket(socket).await
            };
            let peer = async {
                let (socket, _) = listener.accept().await.unwrap();
                let mut peer = acceptor.accept(socket).await.unwrap();
                assert_eq!(peer.get_ref().1.server_name(), Some("ims-na1.adobelogin.com"));
                assert_eq!(peer.get_ref().1.protocol_version(), Some(version.version));
                if alpn == Some(b"h2".as_slice()) {
                    let mut preface = [0; 24];
                    peer.read_exact(&mut preface).await.unwrap();
                    assert_eq!(&preface, b"PRI * HTTP/2.0\r\n\r\nSM\r\n\r\n");
                    assert_eq!(receive(&mut peer).await.0, 4);
                    peer.write_all(&frame(4, 0, 0, &[])).await.unwrap();
                    peer.write_all(&frame(4, 1, 0, &[])).await.unwrap();
                    assert_eq!(receive(&mut peer).await.0, 4);
                    let (kind, _, head) = receive(&mut peer).await;
                    assert_eq!(kind, 1);
                    for value in [
                        b"ims-na1.adobelogin.com".as_slice(),
                        b"/ims/exchange/jwt",
                        b"application/x-www-form-urlencoded",
                    ] {
                        assert!(head.windows(value.len()).any(|w| w == value));
                    }
                    let (kind, flags, body) = receive(&mut peer).await;
                    assert_eq!((kind, flags), (0, 1));
                    assert_eq!(body, BODY);
                    let mut head = vec![0x88, 0x0f, 16, 16];
                    head.extend_from_slice(b"application/json");
                    peer.write_all(&frame(1, 4, 1, &head)).await.unwrap();
                    peer.write_all(&frame(0, 1, 1, ANSWER)).await.unwrap();
                    while receive(&mut peer).await.0 != 7 {}
                } else {
                    let mut head = Vec::new();
                    while !head.ends_with(b"\r\n\r\n") {
                        head.push(peer.read_u8().await.unwrap());
                        assert!(head.len() < 8192);
                    }
                    let head = String::from_utf8(head).unwrap();
                    assert!(head.starts_with("POST /ims/exchange/jwt HTTP/1.1\r\n"));
                    assert!(head.to_ascii_lowercase().contains("host: ims-na1.adobelogin.com\r\n"));
                    let mut body = vec![0; BODY.len()];
                    peer.read_exact(&mut body).await.unwrap();
                    assert_eq!(body, BODY);
                    peer.write_all(format!("HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",ANSWER.len()).as_bytes()).await.unwrap();
                    peer.write_all(ANSWER).await.unwrap();
                }
                peer.shutdown().await.unwrap();
            };
            let (result, ()) = tokio::time::timeout(Duration::from_secs(5), async {
                tokio::join!(exchange_connecting(connect, BODY, &clock), peer)
            })
            .await
            .unwrap();
            assert_eq!(result.unwrap().response.body, ANSWER);
            assert_eq!(clock.0.load(Ordering::SeqCst), 200);
            assert!(
                tokio::time::timeout(Duration::from_millis(10), listener.accept()).await.is_err()
            );
        }
    }
}

#[tokio::test]
async fn tls_refusal_never_reaches_http_or_token_clock() {
    for wrong_name in [false, true] {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let client = client(false);
        let acceptor = server(&rustls::version::TLS13, Some(b"h2"), wrong_name);
        let clock = Clock(AtomicU64::new(0));
        let connect = async {
            client
                .connector
                .connect_test_socket(
                    TcpStream::connect(listener.local_addr().unwrap()).await.unwrap(),
                )
                .await
        };
        let peer = async {
            let (socket, _) = listener.accept().await.unwrap();
            if let Ok(mut peer) = acceptor.accept(socket).await {
                let mut byte = [0];
                assert!(!matches!(peer.read(&mut byte).await, Ok(1)));
            }
        };
        let (result, ()) = tokio::time::timeout(Duration::from_secs(5), async {
            tokio::join!(exchange_connecting(connect, BODY, &clock), peer)
        })
        .await
        .unwrap();
        assert_eq!(result.unwrap_err().code, ConfigurationFailureCode::IdentityManagementTlsFailed);
        assert_eq!(clock.0.load(Ordering::SeqCst), 0);
        assert!(tokio::time::timeout(Duration::from_millis(10), listener.accept()).await.is_err());
    }
}

#[tokio::test]
async fn oversized_form_refuses_before_polling_the_connection() {
    let body = vec![
        0;
        ProfileAuthenticationContract::embedded()
            .limits
            .maximum_identity_management_request_body_bytes as usize
            + 1
    ];
    let clock = Clock(AtomicU64::new(0));
    let connection = async { panic!("oversized forms must not resolve or dial") };
    assert_eq!(
        exchange_connecting(connection, &body, &clock).await.unwrap_err().code,
        ConfigurationFailureCode::IdentityManagementResponseHeadLimitExceeded
    );
    assert_eq!(clock.0.load(Ordering::SeqCst), 0);
}

#[tokio::test]
async fn cancelling_after_tls_and_request_drops_the_owned_socket_without_receipt() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let client = client(true);
    let acceptor = server(&rustls::version::TLS13, Some(b"http/1.1"), false);
    let clock = Clock(AtomicU64::new(0));
    let (sent, ready) = tokio::sync::oneshot::channel();
    let connect = async {
        client
            .connector
            .connect_test_socket(TcpStream::connect(listener.local_addr().unwrap()).await.unwrap())
            .await
    };
    let request = async {
        let mut exchange = Box::pin(exchange_connecting(connect, BODY, &clock));
        tokio::select! {
            result = &mut exchange => panic!("peer has not supplied a response: {result:?}"),
            result = ready => result.unwrap(),
        }
        drop(exchange);
    };
    let peer = async {
        let (socket, _) = listener.accept().await.unwrap();
        let mut peer = acceptor.accept(socket).await.unwrap();
        let mut head = Vec::new();
        while !head.ends_with(b"\r\n\r\n") {
            head.push(peer.read_u8().await.unwrap());
            assert!(head.len() < 8192);
        }
        let mut body = vec![0; BODY.len()];
        peer.read_exact(&mut body).await.unwrap();
        assert_eq!(body, BODY);
        sent.send(()).unwrap();
        let mut byte = [0];
        assert!(!matches!(peer.read(&mut byte).await, Ok(1)), "cancelled exchange kept writing");
    };
    tokio::time::timeout(Duration::from_secs(5), async { tokio::join!(request, peer) })
        .await
        .unwrap();
    assert_eq!(clock.0.load(Ordering::SeqCst), 100, "no response receipt after cancellation");
    assert!(tokio::time::timeout(Duration::from_millis(10), listener.accept()).await.is_err());
}
