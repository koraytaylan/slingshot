//! Selected-author connection integration checks.

use super::*;

const HTTP2_PREPARATION_BYTES: usize = 39;
const HTTP2_FRAME_HEADER_BYTES: usize = 9;
const PREPARATION_TIMEOUT_SECONDS: u64 = 2;
const CONNECTION_TIMEOUT_SECONDS: u64 = 5;
const PROTECTED_EXCHANGE_TIMEOUT_SECONDS: u64 = 10;
const HTTP2_CONNECTION_MODE: u8 = 2;
const AUTHENTICATED_REQUEST_MODES: u8 = 3;

#[tokio::test]
async fn selected_http2_preparation_negotiates_without_sending_a_request() {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::time::{Duration, timeout};
    for valid in [true, false] {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let endpoint = format!("http://{}", listener.local_addr().unwrap());
        let mut files = profile_files();
        replace_profile(&mut files, "profiles/mike.toml", |text| {
            text.replace("http://author.example.com", &endpoint)
                .replace("allow_insecure_author_transport = true\n", "")
        });
        let provider = provider_from_loaded(
            loaded_from_files(files),
            CLEARTEXT_PROFILE,
            CLEARTEXT_ENVIRONMENT,
        );
        let transport =
            SelectedAuthorTransport::new(provider.snapshot().author_connection()).unwrap();
        let client = async {
            let result = transport.prepare_http2().await;
            assert_eq!(result.is_ok(), valid);
            if let Err(failure) = result {
                assert!(!failure.request_may_have_reached_author());
            }
        };
        let peer = async {
            let (mut stream, _) = listener.accept().await.unwrap();
            let mut initial = [0; HTTP2_PREPARATION_BYTES];
            stream.read_exact(&mut initial).await.unwrap();
            assert_eq!(&initial[..24], b"PRI * HTTP/2.0\r\n\r\nSM\r\n\r\n");
            assert_eq!(&initial[24..], &[0, 0, 6, 4, 0, 0, 0, 0, 0, 0, 2, 0, 0, 0, 0]);
            if valid {
                stream.write_all(&[0, 0, 0, 4, 0, 0, 0, 0, 0]).await.unwrap();
                let mut ack = [0; HTTP2_FRAME_HEADER_BYTES];
                stream.read_exact(&mut ack).await.unwrap();
                assert_eq!(ack, [0, 0, 0, 4, 1, 0, 0, 0, 0]);
                stream.write_all(&ack).await.unwrap();
            } else {
                stream.write_all(b"HTTP/1.1 200 OK\r\n\r\n").await.unwrap();
            }
            // An invalid peer may observe a TCP reset with unread response
            // bytes, but neither outcome may contain application request bytes.
            let mut extra = [0; 1];
            assert!(!matches!(stream.read(&mut extra).await, Ok(1..)));
        };
        timeout(Duration::from_secs(PREPARATION_TIMEOUT_SECONDS), async {
            tokio::join!(client, peer)
        })
        .await
        .unwrap();
    }
}

/// The selected connector reaches a real loopback fake only after its endpoint
/// came through the normal profile-selection/snapshot path. The test changes
/// the fixture before loading it rather than using a transport-only factory,
/// so an arbitrary caller cannot redirect this connection.
#[tokio::test]
async fn selected_author_transport_dials_the_profile_selected_loopback_author() {
    let author = Arc::new(FakeAuthor::following(
        Script::of(vec![ScriptedExchange {
            response: ScriptedResponse::Respond { body: b"{}".to_vec(), status: OK_STATUS },
            route: "/bin/slingshot/agent/submit".to_owned(),
        }])
        .expect("the submit route is scriptable"),
        CredentialPolicy::Basic,
    ));
    let recording = author.recording();
    let server = Arc::clone(&author).serve_loopback().await.expect("the fake author binds");
    let mut files = profile_files();
    replace_profile(&mut files, "profiles/mike.toml", |text| {
        text.replace("http://author.example.com", server.endpoint())
            .replace("allow_insecure_author_transport = true\n", "")
    });
    let provider =
        provider_from_loaded(loaded_from_files(files), CLEARTEXT_PROFILE, CLEARTEXT_ENVIRONMENT);
    let transport = SelectedAuthorTransport::new(provider.snapshot().author_connection())
        .expect("the warned loopback author is selected");
    let source = CountingSource { exchanges: Cell::new(0) };
    let (authentication, _) = provider
        .authenticate(server.endpoint(), READING, &source)
        .expect("authenticate selected author");
    let receipt = transport
        .finite_http1(
            http::Method::POST,
            &["bin", "slingshot", "agent", "submit"],
            &authentication,
            &http::HeaderMap::new(),
            b"",
        )
        .await
        .expect("complete bounded HTTP exchange");
    assert_eq!(receipt.response.status, 200);
    assert_eq!(receipt.response.body, b"{}");
    assert_eq!(recording.requests().len(), 1);
    assert!(recording.holds_no_credential_values());
    server.stop().await;
}

/// URI brackets belong on the wire but not in socket/TLS host APIs.
#[tokio::test]
async fn selected_ipv6_author_uses_bare_socket_host_and_bracketed_wire_authority() {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::time::{Duration, timeout};
    let listener = tokio::net::TcpListener::bind("[::1]:0").await.unwrap();
    let origin = format!("http://{}", listener.local_addr().unwrap());
    let endpoint = format!("{origin}/aem");
    let mut files = profile_files();
    replace_profile(&mut files, "profiles/mike.toml", |text| {
        text.replace("http://author.example.com", &endpoint)
            .replace("allow_insecure_author_transport = true\n", "")
    });
    let provider =
        provider_from_loaded(loaded_from_files(files), CLEARTEXT_PROFILE, CLEARTEXT_ENVIRONMENT);
    let transport = SelectedAuthorTransport::new(provider.snapshot().author_connection()).unwrap();
    assert_eq!(transport.origin(), origin);
    let source = CountingSource { exchanges: Cell::new(0) };
    let (authentication, _) = provider.authenticate(&endpoint, READING, &source).unwrap();
    let peer = async {
        let (mut socket, _) = listener.accept().await.unwrap();
        let mut head = Vec::new();
        while !head.ends_with(b"\r\n\r\n") {
            head.push(socket.read_u8().await.unwrap());
        }
        let head = String::from_utf8(head).unwrap();
        assert!(head.starts_with("GET /aem/bin/slingshot/agent/capabilities HTTP/1.1\r\n"));
        assert!(head.contains(&format!("Host: {}\r\n", listener.local_addr().unwrap())));
        socket
            .write_all(
                b"HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: 2\r\n\r\n{}",
            )
            .await
            .unwrap();
    };
    let (receipt, ()) = timeout(Duration::from_secs(CONNECTION_TIMEOUT_SECONDS), async {
        let fields = http::HeaderMap::new();
        tokio::join!(
            transport.finite_http1(
                http::Method::GET,
                &["bin", "slingshot", "agent", "capabilities"],
                &authentication,
                &fields,
                b""
            ),
            peer
        )
    })
    .await
    .unwrap();
    assert_eq!(receipt.unwrap().response.body, b"{}");
}

/// A TCP connection is not sufficient to authenticate a protected author.
#[tokio::test]
async fn selected_author_rejects_non_tls_peer_during_handshake() {
    use slingshot_agent_connection::selected_author_transport::SelectedAuthorTransportFailure;
    use tokio::io::AsyncWriteExt;
    use tokio::time::{Duration, timeout};

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.expect("bind peer");
    let endpoint = format!("https://{}", listener.local_addr().expect("peer address"));
    let mut files = profile_files();
    replace_profile(&mut files, "profiles/zulu.toml", |text| {
        text.replace("https://author.example.com", &endpoint)
    });
    let provider =
        provider_from_loaded(loaded_from_files(files), PROTECTED_PROFILE, PROTECTED_ENVIRONMENT);
    let transport = SelectedAuthorTransport::new(provider.snapshot().author_connection())
        .expect("TLS configuration");
    let peer = async {
        let (mut socket, _) = listener.accept().await.expect("accept connector");
        socket.write_all(b"HTTP/1.1 200 OK\r\n\r\n").await.expect("send invalid TLS");
    };
    let (result, ()) = timeout(Duration::from_secs(CONNECTION_TIMEOUT_SECONDS), async {
        tokio::join!(transport.connect(), peer)
    })
    .await
    .expect("invalid handshake fails promptly");
    assert!(matches!(result, Err(SelectedAuthorTransportFailure::TransportLayerSecurityFailed)));
}

/// Real TLS peers exercise the product connector, not a parallel TLS client.
/// Rejected certificates must fail before any HTTP credentials reach the peer.
#[tokio::test]
async fn selected_author_tls_authenticates_versions_roots_and_hostnames_before_http() {
    use rustls_pki_types::{CertificateDer, PrivateKeyDer, pem::PemObject};
    use slingshot_agent_connection::selected_author_http::FiniteHttpFailure;
    use tokio::io::AsyncWriteExt;
    use tokio::time::{Duration, timeout};

    for automatic in [false, true] {
        for version in [&rustls::version::TLS12, &rustls::version::TLS13] {
            for (trusted, matching_host) in [(true, true), (false, true), (true, false)] {
                let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
                let host = if matching_host { "127.0.0.1" } else { "localhost" };
                let endpoint = format!("https://{host}:{}", listener.local_addr().unwrap().port());
                let mut files = profile_files();
                replace_profile(&mut files, "profiles/mike.toml", |text| {
                    text.replace("http://author.example.com", &endpoint)
                        .replace("allow_insecure_author_transport = true\n", "")
                });
                let selected_platform = if trusted {
                    let root = CertificateDer::from_pem_slice(include_bytes!(
                        "../fixtures/selected-author-tls/root.pem"
                    ))
                    .unwrap();
                    PlatformTrustSnapshot::take(&ScriptedStore {
                        records: vec![ProviderRecord {
                            der: root.as_ref().to_vec(),
                            decision:
                                ProviderDecision::UnconditionallyTrustedForServerAuthentication,
                        }],
                    })
                    .unwrap()
                } else {
                    platform()
                };
                let provider = provider_from_loaded_with_platform(
                    loaded_from_files(files),
                    CLEARTEXT_PROFILE,
                    CLEARTEXT_ENVIRONMENT,
                    selected_platform,
                );
                let transport =
                    SelectedAuthorTransport::new(provider.snapshot().author_connection()).unwrap();
                let source = CountingSource { exchanges: Cell::new(0) };
                let (authentication, _) =
                    provider.authenticate(&endpoint, READING, &source).unwrap();
                let configuration = rustls::ServerConfig::builder_with_provider(Arc::new(
                    rustls::crypto::ring::default_provider(),
                ))
                .with_protocol_versions(&[version])
                .unwrap()
                .with_no_client_auth()
                .with_single_cert(
                    vec![
                        CertificateDer::from_pem_slice(include_bytes!(
                            "../fixtures/selected-author-tls/leaf.pem"
                        ))
                        .unwrap(),
                    ],
                    PrivateKeyDer::from_pem_slice(include_bytes!(
                        "../fixtures/selected-author-tls/test-only-private-key.pem"
                    ))
                    .unwrap(),
                )
                .unwrap();
                let peer = async {
                    let (socket, _) = listener.accept().await.unwrap();
                    let accepted = tokio_rustls::TlsAcceptor::from(Arc::new(configuration))
                        .accept(socket)
                        .await;
                    let Ok(mut socket) = accepted else {
                        return (None, 0);
                    };
                    let negotiated = socket.get_ref().1.protocol_version();
                    let (head, complete) = read_protected_request_head(&mut socket).await;
                    if !complete {
                        return (negotiated, head.len());
                    }
                    assert!(trusted && matching_host, "a rejected certificate received HTTP bytes");
                    assert!(
                        head.starts_with(b"GET /bin/slingshot/agent/capabilities HTTP/1.1\r\n")
                    );
                    socket.write_all(b"HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: 2\r\nConnection: close\r\n\r\n{}").await.unwrap();
                    socket.shutdown().await.unwrap();
                    (negotiated, head.len())
                };
                let headers = http::HeaderMap::new();
                let request = async {
                    if automatic {
                        transport
                            .finite_negotiated_query(
                                http::Method::GET,
                                &["bin", "slingshot", "agent", "capabilities"],
                                &[],
                                &authentication,
                                &headers,
                                b"",
                            )
                            .await
                    } else {
                        transport
                            .finite_http1(
                                http::Method::GET,
                                &["bin", "slingshot", "agent", "capabilities"],
                                &authentication,
                                &headers,
                                b"",
                            )
                            .await
                    }
                };
                let (result, (negotiated, bytes)) =
                    timeout(Duration::from_secs(PROTECTED_EXCHANGE_TIMEOUT_SECONDS), async {
                        tokio::join!(request, peer)
                    })
                    .await
                    .unwrap();
                if trusted && matching_host {
                    let receipt = result.unwrap();
                    assert_eq!(receipt.response.body, b"{}");
                    assert_eq!(negotiated, Some(version.version));
                    assert!(bytes > 0);
                } else {
                    assert!(matches!(result, Err(FiniteHttpFailure::Connect)));
                    assert_eq!(bytes, 0, "credentials cannot precede certificate authentication");
                }
                assert!(
                    timeout(Duration::from_millis(10), listener.accept()).await.is_err(),
                    "finite exchange reconnected"
                );
            }
        }
    }
}

#[tokio::test]
async fn selected_tls_protocol_negotiation_never_silently_downgrades_http2() {
    use rustls_pki_types::{CertificateDer, PrivateKeyDer, pem::PemObject};
    use slingshot_agent_connection::selected_author_transport::{
        SelectedAuthorStream, SelectedAuthorTransportFailure,
    };
    use tokio::io::AsyncReadExt;
    use tokio::time::{Duration, timeout};

    for version in [&rustls::version::TLS12, &rustls::version::TLS13] {
        for (mode, protocols, expected, accepted) in [
            (2, vec![b"h2".as_slice(), b"http/1.1"], Some(b"h2".as_slice()), true),
            (1, vec![b"h2".as_slice(), b"http/1.1"], Some(b"http/1.1".as_slice()), true),
            (2, vec![b"http/1.1".as_slice()], None, false),
            (1, vec![b"h2".as_slice()], None, false),
            (2, vec![], None, false),
            (1, vec![], None, true),
            (2, vec![b"h3".as_slice()], None, false),
            (1, vec![b"h3".as_slice()], None, false),
            (0, vec![b"h2".as_slice(), b"http/1.1"], Some(b"h2".as_slice()), true),
            (0, vec![b"http/1.1".as_slice(), b"h2"], Some(b"http/1.1".as_slice()), true),
            (0, vec![b"h2".as_slice()], Some(b"h2".as_slice()), true),
            (0, vec![b"http/1.1".as_slice()], Some(b"http/1.1".as_slice()), true),
            (0, vec![], None, true),
            (0, vec![b"h3".as_slice()], None, false),
        ] {
            let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
            let endpoint = format!("https://{}", listener.local_addr().unwrap());
            let mut files = profile_files();
            replace_profile(&mut files, "profiles/mike.toml", |text| {
                text.replace("http://author.example.com", &endpoint)
                    .replace("allow_insecure_author_transport = true\n", "")
            });
            let root = CertificateDer::from_pem_slice(include_bytes!(
                "../fixtures/selected-author-tls/root.pem"
            ))
            .unwrap();
            let platform = PlatformTrustSnapshot::take(&ScriptedStore {
                records: vec![ProviderRecord {
                    der: root.as_ref().to_vec(),
                    decision: ProviderDecision::UnconditionallyTrustedForServerAuthentication,
                }],
            })
            .unwrap();
            let provider = provider_from_loaded_with_platform(
                loaded_from_files(files),
                CLEARTEXT_PROFILE,
                CLEARTEXT_ENVIRONMENT,
                platform,
            );
            let transport =
                SelectedAuthorTransport::new(provider.snapshot().author_connection()).unwrap();
            let mut configuration = rustls::ServerConfig::builder_with_provider(Arc::new(
                rustls::crypto::ring::default_provider(),
            ))
            .with_protocol_versions(&[version])
            .unwrap()
            .with_no_client_auth()
            .with_single_cert(
                vec![
                    CertificateDer::from_pem_slice(include_bytes!(
                        "../fixtures/selected-author-tls/leaf.pem"
                    ))
                    .unwrap(),
                ],
                PrivateKeyDer::from_pem_slice(include_bytes!(
                    "../fixtures/selected-author-tls/test-only-private-key.pem"
                ))
                .unwrap(),
            )
            .unwrap();
            configuration.alpn_protocols =
                protocols.iter().map(|protocol| protocol.to_vec()).collect();
            let peer = async {
                let (socket, _) = listener.accept().await.unwrap();
                match tokio_rustls::TlsAcceptor::from(Arc::new(configuration)).accept(socket).await
                {
                    Ok(mut socket) => {
                        let protocol = socket.get_ref().1.alpn_protocol().map(<[u8]>::to_vec);
                        assert!(
                            socket.read_u8().await.is_err(),
                            "negotiation cannot send application bytes"
                        );
                        protocol
                    }
                    Err(_) => None,
                }
            };
            let client = async {
                let result = if mode == 0 {
                    transport.connect_negotiated().await.map(|negotiated| {
                        assert_eq!(format!("{negotiated:?}"),"NegotiatedAuthorStream([redacted])");
                        let (protocol,stream)=negotiated.into_parts();
                        use slingshot_agent_connection::selected_author_transport::SelectedHttpProtocol;
                        assert_eq!(protocol,if expected==Some(b"h2".as_slice()) {SelectedHttpProtocol::Http2} else {SelectedHttpProtocol::Http1});
                        stream
                    })
                } else if mode == HTTP2_CONNECTION_MODE {
                    transport.connect_http2().await
                } else {
                    transport.connect().await
                };
                assert_eq!(result.is_ok(), accepted, "mode={mode}, protocols={protocols:?}");
                if let Ok(SelectedAuthorStream::Protected(stream)) = &result {
                    assert_eq!(stream.get_ref().1.alpn_protocol(), expected);
                    assert_eq!(stream.get_ref().1.protocol_version(), Some(version.version));
                }
                if mode == HTTP2_CONNECTION_MODE && protocols.is_empty() {
                    assert!(matches!(
                        result,
                        Err(SelectedAuthorTransportFailure::ApplicationProtocolUnavailable)
                    ));
                }
                drop(result);
            };
            let ((), protocol) =
                timeout(Duration::from_secs(PROTECTED_EXCHANGE_TIMEOUT_SECONDS), async {
                    tokio::join!(client, peer)
                })
                .await
                .unwrap();
            if accepted {
                assert_eq!(protocol.as_deref(), expected);
            }
            assert!(
                timeout(Duration::from_millis(10), listener.accept()).await.is_err(),
                "negotiation retried a connection"
            );
        }
    }
}

#[tokio::test]
async fn request_authentication_cannot_cross_selected_target_or_revision() {
    use slingshot_agent_connection::selected_author_http::FiniteHttpFailure;
    use tokio::time::{Duration, timeout};
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let endpoint = format!("http://{}", listener.local_addr().unwrap());
    let build = |change: &str| {
        let mut files = profile_files();
        replace_profile(&mut files, "profiles/mike.toml", |text| {
            let text = text
                .replace("http://author.example.com", &endpoint)
                .replace("allow_insecure_author_transport = true\n", "");
            if change == "target" {
                text.replace("user_name = \"admin\"", "user_name = \"another\"")
            } else if change == "revision" {
                text.replace("http://publish.example.com", "http://other-publisher.example.com")
            } else {
                text
            }
        });
        provider_from_loaded(loaded_from_files(files), CLEARTEXT_PROFILE, CLEARTEXT_ENVIRONMENT)
    };
    let selected = build("");
    let transport = SelectedAuthorTransport::new(selected.snapshot().author_connection()).unwrap();
    let source = CountingSource { exchanges: Cell::new(0) };
    for change in ["", "target", "revision"] {
        let provider = build(change);
        let (authentication, _) = provider.authenticate(&endpoint, READING, &source).unwrap();
        assert_eq!(format!("{authentication:?}"), "RequestAuthentication([redacted])");
        assert_eq!(
            provider.snapshot().target() == selected.snapshot().target(),
            change != "target"
        );
        assert_eq!(
            provider.snapshot().revision() == selected.snapshot().revision(),
            change.is_empty()
        );
        let fields = http::HeaderMap::new();
        assert_eq!(
            transport
                .encode_http2_request_head(
                    http::Method::GET,
                    &["bin"],
                    &[],
                    &authentication,
                    &fields,
                    b""
                )
                .is_ok(),
            change.is_empty()
        );
        if change.is_empty() {
            continue;
        }
        let refusal = transport
            .authenticated_finite_get(&provider, &source, READING, &["bin"], &[], &fields)
            .await
            .unwrap_err();
        assert_eq!(refusal, slingshot_agent_connection::selected_author_authenticated_read::AuthenticatedReadFailure::Selection(
            if change == "target" { SelectedAuthorConnectionRefusal::AnotherTarget }
            else { SelectedAuthorConnectionRefusal::AnotherRevision }));
        for mode in 0..AUTHENTICATED_REQUEST_MODES {
            let result = match mode {
                0 => {
                    transport
                        .finite_http1(http::Method::GET, &["bin"], &authentication, &fields, b"")
                        .await
                }
                1 => {
                    transport
                        .finite_http2_query(
                            http::Method::GET,
                            &["bin"],
                            &[],
                            &authentication,
                            &fields,
                            b"",
                        )
                        .await
                }
                _ => {
                    transport
                        .finite_negotiated_query(
                            http::Method::GET,
                            &["bin"],
                            &[],
                            &authentication,
                            &fields,
                            b"",
                        )
                        .await
                }
            };
            assert!(matches!(result, Err(FiniteHttpFailure::Request)));
            assert!(
                timeout(Duration::from_millis(10), listener.accept()).await.is_err(),
                "foreign authentication reached the socket"
            );
        }
    }
    assert_eq!(source.exchanges.get(), 0);
    let foreign_cloud = provider(PROTECTED_PROFILE, PROTECTED_ENVIRONMENT);
    assert!(matches!(transport.authenticated_finite_get(&foreign_cloud, &source, READING,
        &["bin"], &[], &http::HeaderMap::new()).await,
        Err(slingshot_agent_connection::selected_author_authenticated_read::AuthenticatedReadFailure::Selection(_))));
    assert_eq!(source.exchanges.get(), 0, "foreign Cloud selection exchanged before refusal");
    assert!(timeout(Duration::from_millis(10), listener.accept()).await.is_err());
}

#[tokio::test]
async fn selected_cleartext_negotiation_uses_http1_without_upgrade_bytes() {
    use slingshot_agent_connection::selected_author_transport::{
        SelectedAuthorStream, SelectedHttpProtocol,
    };
    use tokio::{
        io::AsyncReadExt,
        time::{Duration, timeout},
    };
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let endpoint = format!("http://{}", listener.local_addr().unwrap());
    let mut files = profile_files();
    replace_profile(&mut files, "profiles/mike.toml", |text| {
        text.replace("http://author.example.com", &endpoint)
            .replace("allow_insecure_author_transport = true\n", "")
    });
    let provider =
        provider_from_loaded(loaded_from_files(files), CLEARTEXT_PROFILE, CLEARTEXT_ENVIRONMENT);
    let transport = SelectedAuthorTransport::new(provider.snapshot().author_connection()).unwrap();
    let client = async {
        let (protocol, stream) = transport.connect_negotiated().await.unwrap().into_parts();
        assert_eq!(protocol, SelectedHttpProtocol::Http1);
        assert!(matches!(stream, SelectedAuthorStream::Cleartext(_)));
        drop(stream);
    };
    let peer = async {
        let (mut socket, _) = listener.accept().await.unwrap();
        let mut received = Vec::new();
        socket.read_to_end(&mut received).await.unwrap();
        assert!(received.is_empty(), "cleartext negotiation emitted upgrade or probe bytes");
    };
    timeout(Duration::from_secs(CONNECTION_TIMEOUT_SECONDS), async { tokio::join!(client, peer) })
        .await
        .unwrap();
    assert!(timeout(Duration::from_millis(10), listener.accept()).await.is_err());
}

async fn read_protected_request_head(
    socket: &mut (impl tokio::io::AsyncRead + Unpin),
) -> (Vec<u8>, bool) {
    use tokio::io::AsyncReadExt;
    const MAXIMUM_FIXTURE_HEAD_BYTES: usize = 8192;
    let mut head = Vec::new();
    while !head.ends_with(b"\r\n\r\n") {
        match socket.read_u8().await {
            Ok(byte) => head.push(byte),
            Err(_) => return (head, false),
        }
        assert!(head.len() <= MAXIMUM_FIXTURE_HEAD_BYTES);
    }
    (head, true)
}
