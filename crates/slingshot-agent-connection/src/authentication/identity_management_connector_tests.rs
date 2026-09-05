//! Real TLS proof for the route-typed IMS connector, with no public dial override.

use super::*;
use rustls_pki_types::{PrivateKeyDer, pem::PemObject};
use slingshot_configuration::{
    platform_trust::{
        PlatformTrustSnapshot, PlatformTrustSource, ProviderDecision, ProviderRecord,
    },
    profile_loader::ConfigurationDiagnostic,
};
use tokio::{io::AsyncReadExt, net::TcpListener};
use tokio::time::timeout;

struct Store(Vec<Vec<u8>>);
impl PlatformTrustSource for Store {
    fn records(&self) -> Result<Vec<ProviderRecord>, ConfigurationDiagnostic> {
        Ok(self
            .0
            .iter()
            .map(|der| ProviderRecord {
                der: der.clone(),
                decision: ProviderDecision::UnconditionallyTrustedForServerAuthentication,
            })
            .collect())
    }
}
fn connector(trusted: bool) -> IdentityManagementConnector {
    let pem: &[u8] = if trusted {
        include_bytes!("../../tests/fixtures/selected-author-tls/root.pem")
    } else {
        include_bytes!(
            "../../../slingshot-test-support/fixtures/additional-certificate-authority/one-authority.pem"
        )
    };
    let root = CertificateDer::from_pem_slice(pem).unwrap();
    let snapshot = PlatformTrustSnapshot::take(&Store(vec![root.as_ref().to_vec()])).unwrap();
    IdentityManagementConnector::new(
        &IdentityManagementTrustInput::from_platform(&snapshot).unwrap(),
    )
    .unwrap()
}
fn server(
    version: &'static rustls::SupportedProtocolVersion,
    alpn: Option<&[u8]>,
) -> tokio_rustls::TlsAcceptor {
    let mut config = rustls::ServerConfig::builder_with_provider(Arc::new(
        rustls::crypto::ring::default_provider(),
    ))
    .with_protocol_versions(&[version])
    .unwrap()
    .with_no_client_auth()
    .with_single_cert(
        vec![
            CertificateDer::from_pem_slice(include_bytes!(
                "../../tests/fixtures/selected-author-tls/leaf.pem"
            ))
            .unwrap(),
        ],
        PrivateKeyDer::from_pem_slice(include_bytes!(
            "../../tests/fixtures/selected-author-tls/test-only-private-key.pem"
        ))
        .unwrap(),
    )
    .unwrap();
    config.alpn_protocols = alpn.map(|value| vec![value.to_vec()]).unwrap_or_default();
    tokio_rustls::TlsAcceptor::from(Arc::new(config))
}

#[tokio::test]
async fn platform_roots_hostname_versions_and_alpn_are_enforced_before_http() {
    for version in [&rustls::version::TLS12, &rustls::version::TLS13] {
        for alpn in [None, Some(b"http/1.1".as_slice()), Some(b"h2".as_slice())] {
            for (trusted, host) in
                [(true, "127.0.0.1"), (false, "127.0.0.1"), (true, "ims-na1.adobelogin.com")]
            {
                let connector = connector(trusted);
                let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
                let address = listener.local_addr().unwrap();
                let acceptor = server(version, alpn);
                let client = async {
                    let socket = TcpStream::connect(address).await.unwrap();
                    let result =
                        connector.handshake(socket, host.to_owned(), Duration::from_secs(3)).await;
                    if trusted && host == "127.0.0.1" {
                        let connection = result.unwrap();
                        assert_eq!(
                            format!("{connection:?}"),
                            "IdentityManagementConnection([redacted])"
                        );
                        let (protocol, stream) = connection.into_parts();
                        assert_eq!(stream.get_ref().1.protocol_version(), Some(version.version));
                        assert_eq!(
                            protocol,
                            if alpn == Some(b"h2".as_slice()) {
                                IdentityManagementHttpProtocol::Http2
                            } else {
                                IdentityManagementHttpProtocol::Http1
                            }
                        );
                        drop(stream);
                    } else {
                        assert_eq!(result.unwrap_err().code, Code::IdentityManagementTlsFailed);
                    }
                };
                let peer = async {
                    let (socket, _) = listener.accept().await.unwrap();
                    if let Ok(mut stream) = acceptor.accept(socket).await {
                        let mut byte = [0];
                        assert!(
                            !matches!(stream.read(&mut byte).await, Ok(1)),
                            "connector wrote HTTP before request dispatch"
                        );
                    }
                };
                timeout(Duration::from_secs(5), async {
                    tokio::join!(client, peer);
                })
                .await
                .unwrap();
                assert_eq!(format!("{connector:?}"), "IdentityManagementConnector([redacted])");
            }
        }
    }
}

#[tokio::test]
async fn handshake_timeout_and_cancellation_close_the_owned_socket() {
    for cancelled in [false, true] {
        let connector = connector(true);
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let (client, peer) =
            tokio::join!(TcpStream::connect(listener.local_addr().unwrap()), listener.accept());
        let (mut peer, _) = peer.unwrap();
        let mut handshake = Box::pin(connector.handshake(
            client.unwrap(),
            "127.0.0.1".to_owned(),
            Duration::from_millis(30),
        ));
        let mut bytes = [0; 8192];
        tokio::select! {
            result = &mut handshake => panic!("handshake ended before ClientHello: {result:?}"),
            count = peer.read(&mut bytes) => assert!(count.unwrap() > 0),
        }
        if cancelled {
            drop(handshake);
        } else {
            assert_eq!(
                handshake.await.unwrap_err().code,
                Code::IdentityManagementTlsHandshakeTimeout
            );
        }
        let remaining =
            timeout(Duration::from_secs(1), peer.read_to_end(&mut Vec::new())).await.unwrap();
        assert!(remaining.is_ok(), "socket did not close cleanly after pending TLS was dropped");
    }
}
