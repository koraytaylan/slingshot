//! Selected-author events integration checks.

use super::*;

const EVENT_GENERATION: u64 = 7;
const CURSOR_FIXTURE_CAPACITY_BYTES: u64 = 96;
const HTTP2_PREPARATION_BYTES: usize = 39;
const HTTP2_FRAME_HEADER_BYTES: usize = 9;
const FRAME_LENGTH_HIGH_SHIFT: usize = 16;
const FRAME_LENGTH_MIDDLE_SHIFT: usize = 8;
const EXCHANGE_TIMEOUT_SECONDS: u64 = 5;
const EVENT_TRANSPORT_MODES: usize = 7;
const FIRST_NEGOTIATED_MODE: usize = 2;
const FIRST_PROTECTED_MODE: usize = 3;
const AUTHENTICATED_MODE: usize = 5;
const ASYNC_AUTHENTICATED_MODE: usize = 6;
const OPTIONAL_PROGRESS_PERCENT: u64 = 17;
const HTTP2_END_HEADERS_FLAG: u8 = 4;

use slingshot_agent_connection::selected_author_http::FiniteHttpFailure;
use slingshot_agent_connection::selected_author_http2_events::EventHttpOutcome;
use slingshot_agent_connection::server_sent_event_decoder::{
    EventStreamCursor, OperationStreamExpectation, StreamItem, StreamRefusal,
};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::time::{Duration, timeout};

#[tokio::test]
async fn selected_event_attachments_preserve_context_query_cursor_and_preflight_boundaries() {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let endpoint = format!("http://{}/aem", listener.local_addr().unwrap());
    let mut files = profile_files();
    replace_profile(&mut files, "profiles/mike.toml", |text| {
        text.replace("http://author.example.com", &endpoint)
            .replace("allow_insecure_author_transport = true\n", "")
    });
    let provider =
        provider_from_loaded(loaded_from_files(files), CLEARTEXT_PROFILE, CLEARTEXT_ENVIRONMENT);
    let transport = SelectedAuthorTransport::new(provider.snapshot().author_connection()).unwrap();
    let source = CountingSource { exchanges: Cell::new(0) };
    let (authentication, _) = provider.authenticate(&endpoint, READING, &source).unwrap();
    let identity = ExecutionIdentity {
        attempt: 1,
        author_target_identity_digest: provider.snapshot().target().to_string(),
        selected_environment_revision: provider.snapshot().revision().to_string(),
        operation_identifier: "local".into(),
    };
    let operation = slingshot_agent_protocol::identity::WireOperationIdentity::of(
        &identity.author_target_identity_digest,
        &identity.selected_environment_revision,
        &identity.operation_identifier,
        slingshot_domain::agent_identity::AgentEventStoreGeneration::of(EVENT_GENERATION),
    )
    .agent_operation_identifier;
    let resolver = |_: &str| -> Result<OperationStreamExpectation, StreamRefusal> {
        panic!("heartbeat invoked terminal resolver");
    };
    let cursor = EventStreamCursor::new("cursor/one", CURSOR_FIXTURE_CAPACITY_BYTES).unwrap();
    for (subscription, generation, cursor_text, wrong_target) in [
        ("", 7, "cursor", false),
        ("sub", 0, "cursor", false),
        ("sub", 7, "cursor\r\ninjected: x", false),
        ("sub", 7, "cursor", true),
        ("sub", 7, " cursor", false),
        ("sub", 7, "cursor ", false),
    ] {
        let mut selected = identity.clone();
        if wrong_target {
            selected.author_target_identity_digest = "other".into();
        }
        let cursor = EventStreamCursor::new(cursor_text, CURSOR_FIXTURE_CAPACITY_BYTES).unwrap();
        assert!(matches!(
            transport
                .events_http2(
                    &selected,
                    subscription,
                    generation,
                    Some(&cursor),
                    &authentication,
                    resolver,
                    |_| panic!("invalid request delivered")
                )
                .await,
            Err(FiniteHttpFailure::Request)
        ));
        assert!(matches!(
            transport
                .events_http1(
                    &selected,
                    subscription,
                    generation,
                    Some(&cursor),
                    &authentication,
                    resolver,
                    |_| panic!("invalid request delivered")
                )
                .await,
            Err(FiniteHttpFailure::Request)
        ));
        assert!(matches!(
            transport
                .events_negotiated(
                    &selected,
                    subscription,
                    generation,
                    Some(&cursor),
                    &authentication,
                    resolver,
                    |_| panic!("invalid negotiated request delivered")
                )
                .await,
            Err(FiniteHttpFailure::Request)
        ));
    }
    assert!(timeout(Duration::from_millis(10), listener.accept()).await.is_err());
    let peer = async {
        let (mut stream, _) = listener.accept().await.unwrap();
        let mut initial = [0; HTTP2_PREPARATION_BYTES];
        stream.read_exact(&mut initial).await.unwrap();
        assert_eq!(&initial[..24], b"PRI * HTTP/2.0\r\n\r\nSM\r\n\r\n");
        stream.write_all(&[0, 0, 0, 4, 0, 0, 0, 0, 0]).await.unwrap();
        let mut ack = [0; HTTP2_FRAME_HEADER_BYTES];
        stream.read_exact(&mut ack).await.unwrap();
        assert_eq!(ack, [0, 0, 0, 4, 1, 0, 0, 0, 0]);
        stream.write_all(&ack).await.unwrap();
        let mut head = [0; HTTP2_FRAME_HEADER_BYTES];
        stream.read_exact(&mut head).await.unwrap();
        assert_eq!((head[3], head[4]), (1, 5));
        let length = usize::from(head[0]) << FRAME_LENGTH_HIGH_SHIFT
            | usize::from(head[1]) << FRAME_LENGTH_MIDDLE_SHIFT
            | usize::from(head[2]);
        assert!(length <= 16384);
        let mut block = vec![0; length];
        stream.read_exact(&mut block).await.unwrap();
        for expected in [
            format!(
                "/aem/bin/slingshot/agent/events?agent_event_store_generation=7&agent_operation_identifier={operation}&daemon_subscription_identifier=sub%20%2F%3F"
            )
            .into_bytes(),
            b"last-event-id".to_vec(),
            b"cursor/one".to_vec(),
            b"authorization".to_vec(),
            b"text/event-stream".to_vec(),
        ] { assert!(block.windows(expected.len()).any(|bytes| bytes == expected)); }
        authentication.lend_value_bytes(|value| {
            assert!(block.windows(value.len()).any(|bytes| bytes == value))
        });
        let mut response = vec![0, 0, 21, 1, 4, 0, 0, 0, 1, 0x88, 0x0f, 16, 17];
        response.extend_from_slice(b"text/event-stream");
        response.extend_from_slice(&[0, 0, 3, 0, 1, 0, 0, 0, 1, b':', b' ', b'\n']);
        stream.write_all(&response).await.unwrap();
        let mut close = Vec::new();
        stream.read_to_end(&mut close).await.unwrap();
        assert_eq!(close, [0, 0, 8, 7, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0]);
        stream.shutdown().await.unwrap();
    };
    let mut items = Vec::new();
    let request = transport.events_http2(
        &identity,
        "sub /?",
        EVENT_GENERATION,
        Some(&cursor),
        &authentication,
        resolver,
        |item| {
            items.push(item);
            Ok(())
        },
    );
    let (result, ()) = timeout(Duration::from_secs(EXCHANGE_TIMEOUT_SECONDS), async {
        tokio::join!(request, peer)
    })
    .await
    .unwrap();
    assert!(matches!(result.unwrap(), EventHttpOutcome::Closed));
    assert_eq!(items, [StreamItem::Heartbeat]);
    for chunked in [false, true] {
        let peer = async {
            let (mut stream, _) = listener.accept().await.unwrap();
            let mut request = Vec::new();
            while !request.ends_with(b"\r\n\r\n") {
                request.push(stream.read_u8().await.unwrap());
                assert!(request.len() <= 8192);
            }
            assert!(request.starts_with(
                format!(
                    "GET /aem/bin/slingshot/agent/events?agent_event_store_generation=7&agent_operation_identifier={operation}&daemon_subscription_identifier=sub%20%2F%3F HTTP/1.1\r\n"
                )
                .as_bytes()
            ));
            for header in
                [b"last-event-id: cursor/one\r\n".as_slice(), b"accept: text/event-stream\r\n"]
            {
                assert!(request.windows(header.len()).any(|bytes| bytes == header));
            }
            authentication.lend_value_bytes(|value| {
                assert!(request.windows(value.len()).any(|bytes| bytes == value))
            });
            let response = if chunked {
                b"HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nTransfer-Encoding: chunked\r\n\r\n3;flag\r\n: \n\r\n0\r\n\r\n".as_slice()
            } else {
                b"HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nContent-Length: 3\r\n\r\n: \n"
            };
            stream.write_all(response).await.unwrap();
            stream.shutdown().await.unwrap();
            assert_eq!(stream.read(&mut [0; 1]).await.unwrap(), 0);
        };
        let mut items = Vec::new();
        let request = transport.events_http1(
            &identity,
            "sub /?",
            EVENT_GENERATION,
            Some(&cursor),
            &authentication,
            resolver,
            |item| {
                items.push(item);
                Ok(())
            },
        );
        let (result, ()) = timeout(Duration::from_secs(EXCHANGE_TIMEOUT_SECONDS), async {
            tokio::join!(request, peer)
        })
        .await
        .unwrap();
        assert!(matches!(result.unwrap(), EventHttpOutcome::Closed));
        assert_eq!(items, [StreamItem::Heartbeat]);
    }
    for mode in 0..EVENT_TRANSPORT_MODES {
        exercise_event_mode(&listener, &source, &identity, &cursor, mode).await;
    }
}

trait EventPeer: tokio::io::AsyncRead + tokio::io::AsyncWrite + Unpin {}
impl<Stream: tokio::io::AsyncRead + tokio::io::AsyncWrite + Unpin> EventPeer for Stream {}
async fn event_peer(
    listener: &tokio::net::TcpListener,
    tls: Option<Arc<rustls::ServerConfig>>,
) -> Box<dyn EventPeer> {
    let (stream, _) = listener.accept().await.unwrap();
    if let Some(configuration) = tls {
        let stream = tokio_rustls::TlsAcceptor::from(configuration).accept(stream).await.unwrap();
        assert_eq!(stream.get_ref().1.alpn_protocol(), Some(b"h2".as_slice()));
        Box::new(stream)
    } else {
        Box::new(stream)
    }
}

async fn exercise_event_mode(
    listener: &tokio::net::TcpListener,
    source: &CountingSource,
    identity: &ExecutionIdentity,
    cursor: &EventStreamCursor,
    mode: usize,
) {
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
    use rustls_pki_types::{CertificateDer, PrivateKeyDer, pem::PemObject};
    let root =
        CertificateDer::from_pem_slice(include_bytes!("../fixtures/selected-author-tls/root.pem"))
            .unwrap();
    let platform = PlatformTrustSnapshot::take(&ScriptedStore {
        records: vec![ProviderRecord {
            der: root.as_ref().to_vec(),
            decision: ProviderDecision::UnconditionallyTrustedForServerAuthentication,
        }],
    })
    .unwrap();
    let async_provider=slingshot_agent_connection::authentication::environment_provider::AsyncEnvironmentAuthenticationProvider::new_async(
        snapshot_from_loaded_with_platform(loaded_from_files(files.clone()),CLEARTEXT_PROFILE,CLEARTEXT_ENVIRONMENT,platform.clone())).unwrap();
    let provider = provider_from_loaded_with_platform(
        loaded_from_files(files),
        CLEARTEXT_PROFILE,
        CLEARTEXT_ENVIRONMENT,
        platform,
    );
    let transport = SelectedAuthorTransport::new(provider.snapshot().author_connection()).unwrap();
    let (authentication, _) = provider.authenticate(&endpoint, READING, source).unwrap();
    let identity = ExecutionIdentity {
        author_target_identity_digest: provider.snapshot().target().to_string(),
        selected_environment_revision: provider.snapshot().revision().to_string(),
        ..identity.clone()
    };
    let operation = slingshot_agent_protocol::identity::WireOperationIdentity::of(
        &identity.author_target_identity_digest,
        &identity.selected_environment_revision,
        &identity.operation_identifier,
        slingshot_domain::agent_identity::AgentEventStoreGeneration::of(EVENT_GENERATION),
    )
    .agent_operation_identifier;
    let tls = if mode >= FIRST_PROTECTED_MODE {
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
        Some(Arc::new(configuration))
    } else {
        None
    };
    for defect in [
        "",
        "identifier",
        "null-terminal",
        "state",
        "physical",
        "counter-null",
        "missing-state",
        "optional-values",
    ] {
        let document = progress_document(defect);
        let body = format!(": before\nid:cursor-next\ndata:{document}\n\n: after\n");
        let peer =
            serve_event_document(listener, tls.clone(), http2, &authentication, &operation, &body);
        let mut items = Vec::new();
        let request = request_event_mode(
            &transport,
            &identity,
            cursor,
            &authentication,
            &async_provider,
            &provider,
            source,
            mode,
            http2,
            automatic,
            |item| {
                items.push(item);
                Ok(())
            },
        );
        let (result, ()) = timeout(Duration::from_secs(EXCHANGE_TIMEOUT_SECONDS), async {
            tokio::join!(request, peer)
        })
        .await
        .unwrap();
        verify_event_document(result, &items, defect, http2);
        assert!(
            timeout(Duration::from_millis(10), listener.accept()).await.is_err(),
            "event stream retried or fell back"
        );
    }
    for (status, defect) in
        [(409, ""), (410, ""), (409, "cursor"), (410, "bare"), (409, "truncated")]
    {
        let body = reset_document(status, defect);
        let peer = serve_event_reset(listener, tls.clone(), http2, status, defect, &body);
        let request = request_event_mode(
            &transport,
            &identity,
            cursor,
            &authentication,
            &async_provider,
            &provider,
            source,
            mode,
            http2,
            automatic,
            |_| panic!("reset delivered event"),
        );
        let (result, ()) = timeout(Duration::from_secs(EXCHANGE_TIMEOUT_SECONDS), async {
            tokio::join!(request, peer)
        })
        .await
        .unwrap();
        verify_event_reset(result, defect, status, http2);
        assert!(
            timeout(Duration::from_millis(10), listener.accept()).await.is_err(),
            "event reset retried or fell back"
        );
    }
}

fn progress_document(defect: &str) -> serde_json::Value {
    let mut document = serde_json::json!({"agent_event_store_generation":7,
        "agent_operation_identifier":if defect == "identifier" {"invalid-operation".to_owned()} else {"a".repeat(64)},
        "daemon_subscription_identifier":"sub /?", "kind":"progress", "sequence":1,
        "sling_job_identifier":"job-fixture", "state":"running"});
    if defect == "null-terminal" {
        document["terminal"] = serde_json::Value::Null;
    }
    if defect == "state" {
        document["state"] = "queued".into();
    }
    if defect == "physical" {
        document["sling_job_identifier"] = "".into();
    }
    if defect == "counter-null" {
        document["attempt"] = serde_json::Value::Null;
    }
    if defect == "missing-state" {
        document.as_object_mut().unwrap().remove("state");
    }
    if defect == "optional-values" {
        document["attempt"] = 0.into();
        document["progress"] = OPTIONAL_PROGRESS_PERCENT.into();
    }
    document
}

async fn serve_event_document(
    listener: &tokio::net::TcpListener,
    tls: Option<Arc<rustls::ServerConfig>>,
    http2: bool,
    authentication: &slingshot_agent_connection::authentication::environment_provider::RequestAuthentication,
    operation: &str,
    body: &str,
) {
    let mut socket = event_peer(listener, tls.clone()).await;
    let mut request = Vec::new();
    if http2 {
        let mut preface = [0; HTTP2_PREPARATION_BYTES];
        socket.read_exact(&mut preface).await.unwrap();
        socket.write_all(&[0, 0, 0, 4, 0, 0, 0, 0, 0]).await.unwrap();
        let mut header = [0; HTTP2_FRAME_HEADER_BYTES];
        socket.read_exact(&mut header).await.unwrap();
        socket.write_all(&header).await.unwrap();
        socket.read_exact(&mut header).await.unwrap();
        let length = usize::from(header[0]) << FRAME_LENGTH_HIGH_SHIFT
            | usize::from(header[1]) << FRAME_LENGTH_MIDDLE_SHIFT
            | usize::from(header[2]);
        request.resize(length, 0);
        socket.read_exact(&mut request).await.unwrap();
    } else {
        while !request.ends_with(b"\r\n\r\n") {
            request.push(socket.read_u8().await.unwrap());
            assert!(request.len() <= 8192);
        }
    }
    authentication.lend_value_bytes(|value| {
        assert!(request.windows(value.len()).any(|bytes| bytes == value))
    });
    for expected in [
        format!(
            "/aem/bin/slingshot/agent/events?agent_event_store_generation=7&agent_operation_identifier={operation}&daemon_subscription_identifier=sub%20%2F%3F"
        )
        .into_bytes(),
        b"cursor/one".to_vec(),
    ] {
        assert!(request.windows(expected.len()).any(|bytes|bytes==expected));
    }
    if http2 {
        let mut head = vec![0, 0, 21, 1, 4, 0, 0, 0, 1, 0x88, 0x0f, 16, 17];
        head.extend_from_slice(b"text/event-stream");
        socket.write_all(&head).await.unwrap();
        let length = (body.len() as u32).to_be_bytes();
        socket.write_all(&[length[1], length[2], length[3], 0, 1, 0, 0, 0, 1]).await.unwrap();
        socket.write_all(body.as_bytes()).await.unwrap();
        let mut close = Vec::new();
        let _ = socket.read_to_end(&mut close).await;
    } else {
        socket.write_all(format!("HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nContent-Length: {}\r\n\r\n{body}", body.len()).as_bytes()).await.unwrap();
    }
    let _ = socket.shutdown().await;
}

async fn request_event_mode(
    transport: &SelectedAuthorTransport,
    identity: &ExecutionIdentity,
    cursor: &EventStreamCursor,
    authentication: &slingshot_agent_connection::authentication::environment_provider::RequestAuthentication,
    async_provider: &slingshot_agent_connection::authentication::environment_provider::AsyncEnvironmentAuthenticationProvider,
    provider: &EnvironmentAuthenticationProvider,
    source: &CountingSource,
    mode: usize,
    http2: bool,
    automatic: bool,
    consume: impl FnMut(StreamItem) -> Result<(), FiniteHttpFailure>,
) -> Result<EventHttpOutcome, FiniteHttpFailure> {
    let resolver = |_: &str| -> Result<OperationStreamExpectation, StreamRefusal> {
        panic!("heartbeat invoked terminal resolver");
    };
    if mode == ASYNC_AUTHENTICATED_MODE {
        transport
            .events_authenticated_async(
                identity,
                "sub /?",
                EVENT_GENERATION,
                Some(cursor),
                async_provider,
                &async_cases::NoClocks,
                &async_cases::NoClocks,
                resolver,
                consume,
            )
            .await
    } else if mode == AUTHENTICATED_MODE {
        transport
            .events_authenticated(
                identity,
                "sub /?",
                EVENT_GENERATION,
                Some(cursor),
                provider,
                source,
                READING,
                resolver,
                consume,
            )
            .await
    } else if automatic {
        transport
            .events_negotiated(
                identity,
                "sub /?",
                EVENT_GENERATION,
                Some(cursor),
                authentication,
                resolver,
                consume,
            )
            .await
    } else if http2 {
        transport
            .events_http2(
                identity,
                "sub /?",
                EVENT_GENERATION,
                Some(cursor),
                authentication,
                resolver,
                consume,
            )
            .await
    } else {
        transport
            .events_http1(
                identity,
                "sub /?",
                EVENT_GENERATION,
                Some(cursor),
                authentication,
                resolver,
                consume,
            )
            .await
    }
}

fn verify_event_document(
    result: Result<EventHttpOutcome, FiniteHttpFailure>,
    items: &[StreamItem],
    defect: &str,
    http2: bool,
) {
    if defect.is_empty() || defect == "optional-values" {
        assert!(matches!(result.unwrap(), EventHttpOutcome::Closed));
        assert_eq!(items.len(), 3);
        let StreamItem::Event(event) = &items[1] else {
            panic!("event was not delivered");
        };
        assert_eq!(event.sling_job_identifier, "job-fixture");
        assert_eq!(
            event.state,
            slingshot_agent_protocol::job_event_document::JobEventState::Running
        );
        assert_eq!(event.attempt, if defect.is_empty() { None } else { Some(0) });
        assert_eq!(
            event.progress,
            if defect.is_empty() { None } else { Some(OPTIONAL_PROGRESS_PERCENT) }
        );
    } else {
        assert!(result.is_err(), "{defect} http2={http2}");
        assert_eq!(items, [StreamItem::Heartbeat]);
    }
}

fn reset_document(status: u16, defect: &str) -> Vec<u8> {
    if defect == "bare" {
        b"{}".to_vec()
    } else {
        serde_json::to_vec(&serde_json::json!({
            "format":"slingshot.agent/1",
            "transport_contract_digest":slingshot_domain::author_agent_transport_contract::AuthorAgentTransportContract::embedded_digest(),
            "daemon_subscription_identifier":"sub /?",
            "requested_agent_event_store_generation":7,
            "requested_last_event_identifier":if defect == "cursor" {"wrong"} else {"cursor/one"},
            "agent_event_store_generation":if status == 409 {8} else {7},
            "high_water_cursor":"captured-position",
            "reason":if status == 409 {"generation_changed"} else {"cursor_expired"},
        })).unwrap()
    }
}

async fn serve_event_reset(
    listener: &tokio::net::TcpListener,
    tls: Option<Arc<rustls::ServerConfig>>,
    http2: bool,
    status: u16,
    defect: &str,
    body: &[u8],
) {
    let mut stream = event_peer(listener, tls.clone()).await;
    if http2 {
        let mut preface = [0; HTTP2_PREPARATION_BYTES];
        stream.read_exact(&mut preface).await.unwrap();
        stream.write_all(&[0, 0, 0, 4, 0, 0, 0, 0, 0]).await.unwrap();
        let mut header = [0; HTTP2_FRAME_HEADER_BYTES];
        stream.read_exact(&mut header).await.unwrap();
        stream.write_all(&header).await.unwrap();
        stream.read_exact(&mut header).await.unwrap();
        let length = usize::from(header[0]) << FRAME_LENGTH_HIGH_SHIFT
            | usize::from(header[1]) << FRAME_LENGTH_MIDDLE_SHIFT
            | usize::from(header[2]);
        assert!(length <= 16384);
        stream.read_exact(&mut vec![0; length]).await.unwrap();
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
        let payload = if defect == "truncated" { &body[..body.len() - 1] } else { body };
        stream.write_all(&encode(0, 1, payload)).await.unwrap();
        let mut discarded = Vec::new();
        let _ = stream.read_to_end(&mut discarded).await;
    } else {
        let mut header = Vec::new();
        while !header.ends_with(b"\r\n\r\n") {
            header.push(stream.read_u8().await.unwrap());
            assert!(header.len() <= 8192);
        }
        stream.write_all(format!("HTTP/1.1 {status} Reset\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n", body.len()).as_bytes()).await.unwrap();
        stream
            .write_all(if defect == "truncated" { &body[..body.len() - 1] } else { body })
            .await
            .unwrap();
    }
    stream.shutdown().await.unwrap();
}

fn verify_event_reset(
    result: Result<EventHttpOutcome, FiniteHttpFailure>,
    defect: &str,
    status: u16,
    http2: bool,
) {
    assert_eq!(result.is_ok(), defect.is_empty(), "http2={http2} status={status} defect={defect}");
    if defect.is_empty() {
        let EventHttpOutcome::Reset(reset) = result.unwrap() else {
            panic!("missing reset evidence");
        };
        assert_eq!(reset.requested_cursor(), Some("cursor/one"));
        assert_eq!(reset.captured_cursor().as_text(), "captured-position");
        assert_eq!(reset.generation(), if status == 409 { 8 } else { 7 });
    }
}
