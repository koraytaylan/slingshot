//! Selected-author high water integration checks.

use super::*;
use slingshot_agent_connection::selected_author_http::FiniteHttpFailure;
use slingshot_agent_connection::subscription_high_water::HighWaterOutcome;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::time::{Duration, timeout};

use rustls_pki_types::{CertificateDer, PrivateKeyDer, pem::PemObject};

const TRANSPORT_MODES: usize = 7;
const FIRST_NEGOTIATED_MODE: usize = 2;
const FIRST_PROTECTED_MODE: usize = 3;
const AUTHENTICATED_MODE: usize = 5;
const ASYNC_AUTHENTICATED_MODE: usize = 6;
const EVENT_GENERATION: u64 = 7;
const HTTP2_PREPARATION_BYTES: usize = 39;
const FRAME_LENGTH_HIGH_SHIFT: usize = 16;
const FRAME_LENGTH_MIDDLE_SHIFT: usize = 8;
const HTTP2_END_HEADERS_FLAG: u8 = 4;
const EXCHANGE_TIMEOUT_SECONDS: u64 = 5;
const CAPTURED_STATUS: u16 = 200;
const UNAUTHORIZED_STATUS: u16 = 401;
const GENERATION_CHANGED_STATUS: u16 = 409;
const EXPIRED_STATUS: u16 = 410;

const EXPECTED_CAPTURE_BODY: &[u8] =
    br#"{"agent_event_store_generation":7,"daemon_subscription_identifier":"sub /?"}"#;
const CAPTURE_FRAME_HEADER_BYTES: usize = 9;

async fn read_capture_data(stream: &mut (impl tokio::io::AsyncRead + Unpin + ?Sized)) -> Vec<u8> {
    use tokio::io::AsyncReadExt;
    let mut header = [0; CAPTURE_FRAME_HEADER_BYTES];
    stream.read_exact(&mut header).await.unwrap();
    let length = u32::try_from(EXPECTED_CAPTURE_BODY.len()).unwrap().to_be_bytes();
    assert_eq!(
        header,
        [length[1], length[2], length[3], 0, 1, 0, 0, 0, 1],
        "the capture body must end stream one in a complete DATA frame"
    );
    let mut body = vec![0; EXPECTED_CAPTURE_BODY.len()];
    stream.read_exact(&mut body).await.unwrap();
    body
}

#[tokio::test]
async fn selected_high_water_captures_are_authenticated_bound_and_never_status_only_resets() {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    for mode in 0..TRANSPORT_MODES {
        let http2 = mode == 1 || mode >= FIRST_PROTECTED_MODE;
        let automatic = mode >= FIRST_NEGOTIATED_MODE;
        let endpoint = format!(
            "{}://{}/aem",
            if mode >= FIRST_PROTECTED_MODE { "https" } else { "http" },
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
        let async_provider = slingshot_agent_connection::authentication::environment_provider::AsyncEnvironmentAuthenticationProvider::new_async(
        snapshot_from_loaded_with_platform(loaded_from_files(files.clone()), CLEARTEXT_PROFILE, CLEARTEXT_ENVIRONMENT, platform.clone())).unwrap();
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
        let authentication = &authentication;
        let identity = ExecutionIdentity {
            attempt: 1,
            author_target_identity_digest: provider.snapshot().target().to_string(),
            selected_environment_revision: provider.snapshot().revision().to_string(),
            operation_identifier: "local".into(),
        };
        for (subscription, generation, moved) in
            [("", 7, false), ("sub", 0, false), ("sub", 7, true)]
        {
            let mut selected = identity.clone();
            if moved {
                selected.selected_environment_revision = "other".into();
            }
            let result = capture_mode(
                &transport,
                &selected,
                subscription,
                generation,
                authentication,
                &async_provider,
                &provider,
                &source,
                mode,
                http2,
                automatic,
            )
            .await;
            assert!(matches!(result, Err(FiniteHttpFailure::Request)));
        }
        assert!(timeout(Duration::from_millis(10), listener.accept()).await.is_err());
        for (status, defect) in [
            (200, ""),
            (200, "generation"),
            (200, "truncated"),
            (409, ""),
            (409, "bare"),
            (410, ""),
            (401, ""),
        ] {
            let peer = async {
                // The capture is a POST, so the author first serves the token
                // the write must present, then reads the write itself.
                let stream = accept_stream(&listener, mode).await;
                let (stream, _, _) = read_request(stream, false, http2, mode, authentication).await;
                serve_json(
                    stream,
                    CAPTURED_STATUS,
                    b"{\"token\":\"test-csrf\"}".to_vec(),
                    false,
                    http2,
                    defect,
                )
                .await;
                let stream = accept_stream(&listener, mode).await;
                let (stream, request, capture_body) =
                    read_request(stream, true, http2, mode, authentication).await;
                assert_eq!(
                    capture_body, EXPECTED_CAPTURE_BODY,
                    "the capture POST must carry the exact selected generation and subscription body"
                );
                if http2 {
                    // The HPACK header block is literal-encoded here, so the
                    // POST's headers are visible in the request bytes; the
                    // body was independently read and verified above.
                    assert!(
                        request
                            .windows(b"csrf-token".len())
                            .any(|bytes| bytes == b"csrf-token" as &[u8]),
                        "the POST presents the token it fetched"
                    );
                    assert!(
                        request
                            .windows(b"application/json".len())
                            .any(|bytes| bytes == b"application/json"),
                        "the POST declares its JSON content type"
                    );
                } else {
                    let head = String::from_utf8_lossy(&request).to_ascii_lowercase();
                    assert!(head.contains("csrf-token: test-csrf\r\n"));
                    assert!(head.contains("content-type: application/json\r\n"));
                }

                let body = capture_document(status, defect);
                serve_json(stream, status, body, true, http2, defect).await;
            };
            let request = capture_mode(
                &transport,
                &identity,
                "sub /?",
                EVENT_GENERATION,
                authentication,
                &async_provider,
                &provider,
                &source,
                mode,
                http2,
                automatic,
            );
            let (result, ()) = timeout(Duration::from_secs(EXCHANGE_TIMEOUT_SECONDS), async {
                tokio::join!(request, peer)
            })
            .await
            .unwrap();
            assert!(
                timeout(Duration::from_millis(10), listener.accept()).await.is_err(),
                "high-water request retried or fell back"
            );
            if !defect.is_empty() {
                assert!(result.is_err(), "http2={http2} status={status} defect={defect}");
                continue;
            }
            match result.unwrap() {
                HighWaterOutcome::Captured(capture) => {
                    assert_eq!(status, 200);
                    assert_eq!(capture.subscription(), "sub /?");
                    assert_eq!(capture.generation(), 7);
                    assert_eq!(capture.cursor().as_text(), "captured-position");
                }
                HighWaterOutcome::Reset(reset) => {
                    assert_eq!(status, 409);
                    assert_eq!(reset.requested_cursor(), None);
                    assert_eq!(reset.requested_generation(), 7);
                    assert_eq!(reset.generation(), 8);
                }
                HighWaterOutcome::Response(response) => {
                    assert!([401, 410].contains(&status));
                    assert_eq!(response.status, status);
                }
            }
        }
    }
}

trait Peer: tokio::io::AsyncRead + tokio::io::AsyncWrite + Unpin {}
impl<Stream: tokio::io::AsyncRead + tokio::io::AsyncWrite + Unpin> Peer for Stream {}

async fn accept_stream(listener: &tokio::net::TcpListener, mode: usize) -> Box<dyn Peer> {
    let (stream, _) = listener.accept().await.unwrap();
    let stream: Box<dyn Peer> = if mode >= FIRST_PROTECTED_MODE {
        let version = if mode == FIRST_PROTECTED_MODE {
            &rustls::version::TLS12
        } else {
            &rustls::version::TLS13
        };
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
        configuration.alpn_protocols = vec![b"h2".to_vec(), b"http/1.1".to_vec()];
        let stream =
            tokio_rustls::TlsAcceptor::from(Arc::new(configuration)).accept(stream).await.unwrap();
        assert_eq!(stream.get_ref().1.alpn_protocol(), Some(b"h2".as_slice()));
        Box::new(stream)
    } else {
        Box::new(stream)
    };
    stream
}

fn verify_capture_request(
    request: &[u8],
    body_expected: bool,
    http2: bool,
    mode: usize,
    authentication: &slingshot_agent_connection::authentication::environment_provider::RequestAuthentication,
) {
    for expected in [
        if body_expected {
            &b"/aem/bin/slingshot/agent/subscriptions/high-water"[..]
        } else {
            &b"/aem/libs/granite/csrf/token.json"[..]
        },
        if http2 { b"authorization" } else { b"Authorization" },
    ] {
        assert!(
            request.windows(expected.len()).any(|bytes| bytes == expected),
            "mode={mode} body={body_expected} http2={http2} missing fixed route/header {}",
            String::from_utf8_lossy(expected)
        );
    }
    if body_expected {
        assert!(
            request.windows(b"application/json".len()).any(|bytes| bytes == b"application/json"),
            "mode={mode} http2={http2} the POST does not ask for or declare JSON"
        );
    }
    assert!(!request.windows(b"last-event-id".len()).any(|bytes| bytes == b"last-event-id"));
    if body_expected {
        assert!(
            request.windows(b"authorization".len()).any(|bytes| bytes == b"authorization")
                || request.windows(b"Authorization".len()).any(|bytes| bytes == b"Authorization"),
            "mode={mode} http2={http2} the POST carries no credential header"
        );
        authentication.lend_value_bytes(|value| {
            assert!(
                request.windows(value.len()).any(|bytes| bytes == value),
                "mode={mode} http2={http2} the POST presents another credential"
            );
        });
    }
}

async fn read_request(
    mut stream: Box<dyn Peer>,
    body_expected: bool,
    http2: bool,
    mode: usize,
    authentication: &slingshot_agent_connection::authentication::environment_provider::RequestAuthentication,
) -> (Box<dyn Peer>, Vec<u8>, Vec<u8>) {
    let mut request = Vec::new();
    if http2 {
        let mut preface = [0; HTTP2_PREPARATION_BYTES];
        stream.read_exact(&mut preface).await.unwrap();
        assert_eq!(&preface[..24], b"PRI * HTTP/2.0\r\n\r\nSM\r\n\r\n");
        stream.write_all(&[0, 0, 0, 4, 0, 0, 0, 0, 0]).await.unwrap();
        let mut header = [0; CAPTURE_FRAME_HEADER_BYTES];
        stream.read_exact(&mut header).await.unwrap();
        assert_eq!(header, [0, 0, 0, 4, 1, 0, 0, 0, 0]);
        stream.write_all(&header).await.unwrap();
        stream.read_exact(&mut header).await.unwrap();
        assert_eq!(header[3], 1);
        assert_eq!(header[4] & 1 != 0, !body_expected);
        let length = usize::from(header[0]) << FRAME_LENGTH_HIGH_SHIFT
            | usize::from(header[1]) << FRAME_LENGTH_MIDDLE_SHIFT
            | usize::from(header[2]);
        assert!(length <= 16384);
        request.resize(length, 0);
        stream.read_exact(&mut request).await.unwrap();
    } else {
        while !request.ends_with(b"\r\n\r\n") {
            request.push(stream.read_u8().await.unwrap());
            assert!(request.len() <= 8192);
        }
        if body_expected {
            assert!(request.starts_with(b"POST "));
        } else {
            assert!(request.starts_with(b"GET "));
        }
    }
    let mut capture_body = Vec::new();
    if body_expected && http2 {
        capture_body = read_capture_data(stream.as_mut()).await;
    }
    if body_expected && !http2 {
        let length_start = request
            .to_ascii_lowercase()
            .windows(b"content-length:".len())
            .position(|bytes| bytes == b"content-length:")
            .expect("a POST declares its length");
        let tail = &request[length_start + b"content-length:".len()..];
        let end = tail.iter().position(|byte| *byte == b'\r').unwrap();
        let length: usize = std::str::from_utf8(&tail[..end]).unwrap().trim().parse().unwrap();
        let mut body_bytes = vec![0; length];
        stream.read_exact(&mut body_bytes).await.unwrap();
        request.extend_from_slice(&body_bytes);
        capture_body = body_bytes;
    }
    verify_capture_request(&request, body_expected, http2, mode, authentication);
    (stream, request, capture_body)
}

async fn serve_json(
    mut stream: Box<dyn Peer>,
    status: u16,
    body_bytes: Vec<u8>,
    allow_truncation: bool,
    http2: bool,
    defect: &str,
) {
    let body = body_bytes.as_slice();
    let payload =
        if defect == "truncated" && allow_truncation { &body[..body.len() - 1] } else { body };
    if http2 {
        let mut block = Vec::new();
        for (name, value) in [
            (":status", status.to_string()),
            ("content-type", "application/json".into()),
            ("content-length", body.len().to_string()),
        ] {
            block.extend_from_slice(&[0, name.len() as u8]);
            block.extend_from_slice(name.as_bytes());
            block.push(value.len() as u8);
            block.extend_from_slice(value.as_bytes());
        }
        let encode = |kind: u8, flags: u8, payload: &[u8]| {
            let length = (payload.len() as u32).to_be_bytes();
            let mut frame = vec![length[1], length[2], length[3], kind, flags, 0, 0, 0, 1];
            frame.extend_from_slice(payload);
            frame
        };
        stream.write_all(&encode(1, HTTP2_END_HEADERS_FLAG, &block)).await.unwrap();
        stream.write_all(&encode(0, 1, payload)).await.unwrap();
        let mut close = Vec::new();
        let _ = stream.read_to_end(&mut close).await;
    } else {
        stream.write_all(format!("HTTP/1.1 {status} Response\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n", body.len()).as_bytes()).await.unwrap();
        stream.write_all(payload).await.unwrap();
    }
    stream.shutdown().await.unwrap();
}

fn capture_document(status: u16, defect: &str) -> Vec<u8> {
    if defect == "bare" || status == EXPIRED_STATUS || status == UNAUTHORIZED_STATUS {
        b"{}".to_vec()
    } else {
        let mut value = serde_json::json!({
            "format":"slingshot.agent/1",
            "transport_contract_digest":slingshot_domain::author_agent_transport_contract::AuthorAgentTransportContract::embedded_digest(),
            "daemon_subscription_identifier":"sub /?",
            "agent_event_store_generation":if status == GENERATION_CHANGED_STATUS || defect == "generation" {8} else {7},
            "high_water_cursor":"captured-position",
        });
        if status == GENERATION_CHANGED_STATUS {
            value["requested_agent_event_store_generation"] = serde_json::json!(7);
            value["requested_last_event_identifier"] = serde_json::Value::Null;
            value["reason"] = serde_json::json!("generation_changed");
        }
        serde_json::to_vec(&value).unwrap()
    }
}

async fn capture_mode(
    transport: &SelectedAuthorTransport,
    identity: &ExecutionIdentity,
    subscription: &str,
    generation: u64,
    authentication: &slingshot_agent_connection::authentication::environment_provider::RequestAuthentication,
    async_provider: &slingshot_agent_connection::authentication::environment_provider::AsyncEnvironmentAuthenticationProvider,
    provider: &EnvironmentAuthenticationProvider,
    source: &CountingSource,
    mode: usize,
    http2: bool,
    automatic: bool,
) -> Result<HighWaterOutcome, FiniteHttpFailure> {
    if mode == ASYNC_AUTHENTICATED_MODE {
        transport
            .capture_high_water_authenticated_async(
                identity,
                subscription,
                generation,
                async_provider,
                &async_cases::NoClocks,
                &async_cases::NoClocks,
            )
            .await
    } else if mode == AUTHENTICATED_MODE {
        transport
            .capture_high_water_authenticated(
                identity,
                subscription,
                generation,
                provider,
                source,
                READING,
            )
            .await
    } else if automatic {
        transport
            .capture_high_water_negotiated(identity, subscription, generation, authentication)
            .await
    } else if http2 {
        transport.capture_high_water_http2(identity, subscription, generation, authentication).await
    } else {
        transport.capture_high_water_http1(identity, subscription, generation, authentication).await
    }
}
