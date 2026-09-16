//! Selected-author finite integration checks.

use super::*;

const HTTP2_PREPARATION_BYTES: usize = 39;
const HTTP2_FRAME_HEADER_BYTES: usize = 9;
const EMPTY_OBJECT_DATA_FRAME_BYTES: usize = 11;
const HTTP2_GOAWAY_FRAME_BYTES: usize = 17;
const EXCHANGE_TIMEOUT_SECONDS: u64 = 5;

/// Full selected-author HTTP/2 exchanges complete at the response stream end.
#[tokio::test]
async fn selected_http2_finite_exchange_uses_exact_request_over_cleartext_and_tls() {
    use rustls_pki_types::{CertificateDer, PrivateKeyDer, pem::PemObject};
    use slingshot_agent_connection::selected_author_transport::SelectedAuthorStream;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::time::{Duration, timeout};

    for (automatic, version) in [
        (false, None),
        (false, Some(&rustls::version::TLS12)),
        (false, Some(&rustls::version::TLS13)),
        (true, Some(&rustls::version::TLS12)),
        (true, Some(&rustls::version::TLS13)),
    ] {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let endpoint = format!(
            "{}://{}/aem",
            if version.is_some() { "https" } else { "http" },
            listener.local_addr().unwrap()
        );
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
        let source = CountingSource { exchanges: Cell::new(0) };
        let (authentication, _) = provider.authenticate(&endpoint, READING, &source).unwrap();
        let headers = http::HeaderMap::new();
        let path = ["bin", "slingshot", "agent", "jobs"];
        let query = [("q", "é /%")];
        let expected_head: Vec<u8> = transport
            .encode_http2_request_head(
                http::Method::POST,
                &path,
                &query,
                &authentication,
                &headers,
                b"{}",
            )
            .unwrap()
            .frames()
            .flatten()
            .collect();
        let peer = async {
            let (socket, _) = listener.accept().await.unwrap();
            // Use one AsyncRead/Write trait object for cleartext and the server TLS stream.
            trait Peer: tokio::io::AsyncRead + tokio::io::AsyncWrite + Unpin {}
            impl<T: tokio::io::AsyncRead + tokio::io::AsyncWrite + Unpin> Peer for T {}
            let mut socket: Box<dyn Peer> = if let Some(version) = version {
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
                configuration.alpn_protocols = vec![b"h2".to_vec()];
                let socket = tokio_rustls::TlsAcceptor::from(Arc::new(configuration))
                    .accept(socket)
                    .await
                    .unwrap();
                assert_eq!(socket.get_ref().1.alpn_protocol(), Some(b"h2".as_slice()));
                Box::new(socket)
            } else {
                Box::new(SelectedAuthorStream::Cleartext(socket))
            };
            let mut preface = [0; HTTP2_PREPARATION_BYTES];
            socket.read_exact(&mut preface).await.unwrap();
            assert_eq!(&preface[..24], b"PRI * HTTP/2.0\r\n\r\nSM\r\n\r\n");
            socket.write_all(&[0, 0, 0, 4, 0, 0, 0, 0, 0]).await.unwrap();
            let mut ack = [0; HTTP2_FRAME_HEADER_BYTES];
            socket.read_exact(&mut ack).await.unwrap();
            assert_eq!(ack, [0, 0, 0, 4, 1, 0, 0, 0, 0]);
            socket.write_all(&ack).await.unwrap();
            let mut actual = vec![0; expected_head.len()];
            socket.read_exact(&mut actual).await.unwrap();
            assert!(
                actual == expected_head,
                "request bytes differ from selected-origin authenticated encoder"
            );
            let mut data = [0; EMPTY_OBJECT_DATA_FRAME_BYTES];
            socket.read_exact(&mut data).await.unwrap();
            assert_eq!(&data[..9], &[0, 0, 2, 0, 1, 0, 0, 0, 1]);
            assert_eq!(&data[9..], b"{}");
            // Independent literal HPACK response, then one exact END_STREAM DATA.
            let mut response = vec![0, 0, 20, 1, 4, 0, 0, 0, 1, 0x88, 0x0f, 16, 16];
            response.extend_from_slice(b"application/json");
            response.extend_from_slice(&[0, 0, 2, 0, 1, 0, 0, 0, 1, b'{', b'}']);
            socket.write_all(&response).await.unwrap();
            let mut shutdown = [0; HTTP2_GOAWAY_FRAME_BYTES];
            socket.read_exact(&mut shutdown).await.unwrap();
            assert_eq!(shutdown, [0, 0, 8, 7, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0]);
            assert_eq!(socket.read(&mut [0; 1]).await.unwrap(), 0);
        };
        let request = async {
            if automatic {
                transport
                    .finite_negotiated_query(
                        http::Method::POST,
                        &path,
                        &query,
                        &authentication,
                        &headers,
                        b"{}",
                    )
                    .await
            } else {
                transport
                    .finite_http2_query(
                        http::Method::POST,
                        &path,
                        &query,
                        &authentication,
                        &headers,
                        b"{}",
                    )
                    .await
            }
        };
        let (receipt, ()) = timeout(Duration::from_secs(EXCHANGE_TIMEOUT_SECONDS), async {
            tokio::join!(request, peer)
        })
        .await
        .unwrap();
        assert!(receipt.is_ok());
        assert!(
            timeout(Duration::from_millis(10), listener.accept()).await.is_err(),
            "negotiated request reconnected or retried"
        );
        let receipt = receipt.unwrap();
        assert_eq!(receipt.response.body, b"{}");
        assert_eq!(receipt.response.status, 200);
        assert_eq!(format!("{receipt:?}"), "FiniteHttpReceipt([redacted])");
    }
}

/// Both finite framings are accepted; invalid responses remain uncertain.
#[tokio::test]
async fn finite_http_refuses_ambiguous_truncated_and_surplus_wire_bytes() {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::time::{Duration, timeout};
    let chunk_line_bound =
        slingshot_agent_connection::author_hypertext_transfer_protocol_policy::HeadBounds::embedded(
        )
        .field_bytes as usize;
    let bounded_extension = format!(
        "HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\n2;x={}\r\n{{}}\r\n0\r\n\r\n",
        "x".repeat(chunk_line_bound - 6)
    );
    let oversized_extension = format!(
        "HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\n2;x={}\r\n{{}}\r\n0\r\n\r\n",
        "x".repeat(
            slingshot_agent_connection::author_hypertext_transfer_protocol_policy::HeadBounds::embedded().field_bytes
                as usize
        )
    );
    let exact_field = format!(
        "HTTP/1.1 200 OK\r\nContent-Length: 2\r\nx: \t{} \t\r\n\r\n{{}}",
        "v".repeat(chunk_line_bound - 1)
    );
    let oversized_field = format!(
        "HTTP/1.1 200 OK\r\nContent-Length: 2\r\nx: \t{} \t\r\n\r\n{{}}",
        "v".repeat(chunk_line_bound)
    );
    let rejected: &[&[u8]] = &[
        oversized_field.as_bytes(),
        oversized_extension.as_bytes(),
        &b"HTTP/1.1 200 OK\r\nContent-Length: 2\r\nContent-Length: 2\r\n\r\n{}"[..],
        &b"HTTP/1.1 200 OK\r\nContent-Length: 3\r\n\r\n{}"[..],
        &b"HTTP/1.1 200 OK\r\nContent-Length: 2\r\n\r\n{}surplus"[..],
        &b"HTTP/1.1 100 Continue\r\n\r\nHTTP/1.1 200 OK\r\nContent-Length: 2\r\n\r\n{}"[..],
        &b"HTTP/1.1 200 OK\r\nContent-Encoding: gzip\r\nContent-Length: 2\r\n\r\n{}"[..],
        b"HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\nContent-Length: 2\r\n\r\n0\r\n\r\n",
        b"HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\nTransfer-Encoding: chunked\r\n\r\n0\r\n\r\n",
        b"HTTP/1.1 200 OK\r\nTransfer-Encoding: gzip, chunked\r\n\r\n0\r\n\r\n",
        b"HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\n2\r\n{}\r\n0\r\nX-Trailer: value\r\n\r\n",
        b"HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\n3\r\n{}",
        b"HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\n2\r\n{}x\r\n0\r\n\r\n",
        b"HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\n0\r\n\r\nsurplus",
        b"HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\nFFFFFFFFFFFFFFFF\r\n",
        b"HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\n2;broken=\"unterminated\r\n{}\r\n0\r\n\r\n",
        b"HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\n2;flag\r\n{}\r\n0;=bad\r\n\r\n",
    ];
    let accepted: &[&[u8]] = &[
        exact_field.as_bytes(),
        bounded_extension.as_bytes(),
        b"HTTP/1.1 200 OK\r\nContent-Length: 2\r\n\r\n{}",
        b"HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\n1\r\n{\r\n1\r\n}\r\n0\r\n\r\n",
        b"HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\n2 ; flag ; quoted = \"a;=b\\\"c\"\r\n{}\r\n00;last=yes\r\n\r\n",
    ];
    for automatic in [false, true] {
        for (response, valid) in rejected
            .iter()
            .map(|response| (*response, false))
            .chain(accepted.iter().map(|response| (*response, true)))
        {
            let listener =
                tokio::net::TcpListener::bind("127.0.0.1:0").await.expect("bind hostile peer");
            let endpoint = format!("http://{}", listener.local_addr().expect("peer address"));
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
            let source = CountingSource { exchanges: Cell::new(0) };
            let (authentication, _) =
                provider.authenticate(&endpoint, READING, &source).expect("authenticate");
            let transport = SelectedAuthorTransport::new(provider.snapshot().author_connection())
                .expect("connector");
            let fields = http::HeaderMap::new();
            let exchange = async {
                if automatic {
                    transport
                        .finite_negotiated_query(
                            http::Method::POST,
                            &["submit"],
                            &[],
                            &authentication,
                            &fields,
                            b"",
                        )
                        .await
                } else {
                    transport
                        .finite_http1(
                            http::Method::POST,
                            &["submit"],
                            &authentication,
                            &fields,
                            b"",
                        )
                        .await
                }
            };
            let peer = async {
                let (mut socket, _) = listener.accept().await.expect("accept");
                let mut request = Vec::new();
                while !request.ends_with(b"\r\n\r\n") {
                    request.push(socket.read_u8().await.expect("read request"));
                }
                // Keep media valid so each case exercises its intended framing.
                let response = String::from_utf8(response.to_vec())
                    .expect("ASCII wire fixture")
                    .replacen("\r\n", "\r\nContent-Type: application/json\r\n", 1);
                socket.write_all(response.as_bytes()).await.expect("write response");
            };
            let (result, ()) = timeout(Duration::from_secs(EXCHANGE_TIMEOUT_SECONDS), async {
                tokio::join!(exchange, peer)
            })
            .await
            .expect("exchange bounded");
            if valid {
                assert_eq!(result.expect("valid framing").response.body, b"{}");
            } else {
                let failure = result.expect_err("hostile response cannot publish a receipt");
                assert!(failure.request_may_have_reached_author());
            }
            assert!(
                timeout(Duration::from_millis(10), listener.accept()).await.is_err(),
                "finite response refusal caused fallback"
            );
        }
    }
}
