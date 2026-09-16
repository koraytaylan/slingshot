//! Selected-author artifacts integration checks.

use super::*;

const EVENT_GENERATION: u64 = 7;
const ARTIFACT_BYTES: u64 = 3;
const EXCHANGE_TIMEOUT_SECONDS: u64 = 5;
const DIGEST_CHARACTERS: usize = 64;
const TRANSPORT_MODE_COUNT: usize = 6;
const TLS_THIRTEEN_MODE: usize = 2;
const FIRST_PLAIN_HTTP_ONE_MODE: usize = 3;
const AUTHENTICATED_MODE: usize = 4;
const ASYNC_AUTHENTICATED_MODE: usize = 5;
const HTTP_TWO_PREFACE_AND_SETTINGS_BYTES: usize = 39;
const HTTP_TWO_FRAME_HEADER_BYTES: usize = 9;
const LENGTH_HIGH_SHIFT: usize = 16;
const LENGTH_MIDDLE_SHIFT: usize = 8;
const SUCCESS_STATUS: u16 = 200;
const END_HEADERS_FLAG: u8 = 4;
const END_HEADERS_AND_STREAM_FLAGS: u8 = 5;
const STREAM_BLOCK_BYTES: usize = 8192;

/// Artifact transport proves the end before issuing a receipt, independently
/// of the caller's command-result manifest validation and private staging.
#[tokio::test]
async fn selected_artifact_stream_requires_complete_verified_message() {
    use sha2::{Digest, Sha256};
    use slingshot_agent_connection::{
        artifact_download::ExpectedArtifact,
        command_submission::{ExpectedArtifactManifest, Submission},
        selected_author_http::FiniteHttpFailure,
    };
    use slingshot_agent_protocol::{
        identity::WireOperationIdentity, wire_contract::ExpectedProvenance,
    };
    use slingshot_domain::{
        agent_identity::AgentEventStoreGeneration,
        author_agent_transport_contract::AuthorAgentTransportContract,
        command::schema::canonical_contract_digest,
        selected_command_contract_identity::SelectedCommandContractIdentity,
    };
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::time::{Duration, timeout};

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
    let (authentication, _) =
        provider.authenticate(provider.snapshot().author().as_text(), READING, &source).unwrap();
    let identity = ExecutionIdentity {
        attempt: 1,
        author_target_identity_digest: provider.snapshot().target().to_string(),
        selected_environment_revision: provider.snapshot().revision().to_string(),
        operation_identifier: "local-operation".to_owned(),
    };
    let provenance = ExpectedProvenance {
        canonical_json_contract_digest: canonical_contract_digest(),
        command_contract: SelectedCommandContractIdentity::installed("query_paths").unwrap(),
        transport_contract_digest: AuthorAgentTransportContract::embedded_digest(),
    };
    let submission = Submission::build(
        &provenance,
        WireOperationIdentity::of(
            &identity.author_target_identity_digest,
            &identity.selected_environment_revision,
            &identity.operation_identifier,
            AgentEventStoreGeneration::of(EVENT_GENERATION),
        ),
        "subscription-one",
        r#"{"root_path":"/content"}"#,
        ExpectedArtifactManifest::empty(),
    )
    .unwrap();
    let mut expected = ExpectedArtifact {
        artifact_digest: Sha256::digest(b"abc").iter().map(|byte| format!("{byte:02x}")).collect(),
        artifact_slot: "content_package".to_owned(),
        byte_length: ARTIFACT_BYTES,
        media_type: "application/zip".to_owned(),
    };
    let mut wrong = identity.clone();
    wrong.selected_environment_revision = "wrong".to_owned();
    assert!(
        transport
            .stream_artifact_http1(&wrong, &submission, &expected, &authentication, |_| panic!(
                "invalid identity streamed"
            ))
            .await
            .is_err()
    );
    assert!(timeout(Duration::from_millis(10), listener.accept()).await.is_err());
    verify_http_one_stream_cases(
        &listener,
        &transport,
        &identity,
        &submission,
        &expected,
        &authentication,
    )
    .await;
    let artifact_identifier = "3".repeat(DIGEST_CHARACTERS);
    assert!(
        transport
            .artifact_http2(
                &wrong,
                &submission,
                &expected,
                &artifact_identifier,
                &authentication,
                |_| panic!("invalid HTTP/2 identity streamed")
            )
            .await
            .is_err()
    );
    assert!(
        transport
            .artifact_negotiated(
                &wrong,
                &submission,
                &expected,
                &artifact_identifier,
                &authentication,
                |_| panic!("invalid negotiated identity streamed")
            )
            .await
            .is_err()
    );
    assert!(
        transport
            .artifact_negotiated(
                &identity,
                &submission,
                &expected,
                "",
                &authentication,
                |_| panic!("invalid negotiated artifact identifier streamed")
            )
            .await
            .is_err()
    );
    assert!(timeout(Duration::from_millis(10), listener.accept()).await.is_err());
    for mode in 0..TRANSPORT_MODE_COUNT {
        let automatic = mode != 0;
        let endpoint = format!(
            "{}://{}/aem",
            if mode == 1 || mode == TLS_THIRTEEN_MODE { "https" } else { "http" },
            listener.local_addr().unwrap()
        );
        let mut files = profile_files();
        replace_profile(&mut files, "profiles/mike.toml", |text| {
            text.replace("http://author.example.com", &endpoint)
                .replace("allow_insecure_author_transport = true\n", "")
        });
        use rustls_pki_types::{CertificateDer, pem::PemObject};
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
        let async_provider=slingshot_agent_connection::authentication::environment_provider::AsyncEnvironmentAuthenticationProvider::new_async(
        snapshot_from_loaded_with_platform(loaded_from_files(files.clone()),CLEARTEXT_PROFILE,CLEARTEXT_ENVIRONMENT,platform.clone())).unwrap();
        let provider = provider_from_loaded_with_platform(
            loaded_from_files(files),
            CLEARTEXT_PROFILE,
            CLEARTEXT_ENVIRONMENT,
            platform,
        );
        let transport =
            SelectedAuthorTransport::new(provider.snapshot().author_connection()).unwrap();
        let (authentication, _) = provider.authenticate(&endpoint, READING, &source).unwrap();
        let identity = ExecutionIdentity {
            author_target_identity_digest: provider.snapshot().target().to_string(),
            selected_environment_revision: provider.snapshot().revision().to_string(),
            ..identity.clone()
        };
        let submission = Submission::build(
            &provenance,
            WireOperationIdentity::of(
                &identity.author_target_identity_digest,
                &identity.selected_environment_revision,
                &identity.operation_identifier,
                AgentEventStoreGeneration::of(EVENT_GENERATION),
            ),
            "subscription-one",
            r#"{"root_path":"/content"}"#,
            ExpectedArtifactManifest::empty(),
        )
        .unwrap();
        for (status, defect, accepted) in [
            (200, "", true),
            (200, "digest", false),
            (200, "trailer", false),
            (200, "sink", false),
            (404, "", true),
            (410, "", true),
            (410, "identity", false),
            (404, "bare", false),
            (401, "bare", true),
            (401, "trailer", false),
            (401, "short", false),
        ] {
            let body = artifact_response_body(status, defect, &submission, &artifact_identifier);
            let authorization = authentication.lend_value_bytes(|value| value.to_vec());
            let peer = serve_artifact_peer(ArtifactPeerCase {
                listener: &listener,
                mode,
                status,
                defect,
                accepted,
                body: &body,
                agent_operation_identifier: &submission.operation.agent_operation_identifier,
                authorization: &authorization,
            });
            let mut staged = Vec::new();
            let request = async {
                let sink = |bytes: &[u8]| {
                    assert_eq!(status, SUCCESS_STATUS, "unavailable document entered staging");
                    if defect == "sink" {
                        return Err(FiniteHttpFailure::Body);
                    }
                    staged.extend_from_slice(bytes);
                    Ok(())
                };
                if mode == ASYNC_AUTHENTICATED_MODE {
                    transport
                        .artifact_authenticated_async(
                            &identity,
                            &submission,
                            &expected,
                            &artifact_identifier,
                            &async_provider,
                            &async_cases::NoClocks,
                            &async_cases::NoClocks,
                            sink,
                        )
                        .await
                } else if mode == AUTHENTICATED_MODE {
                    transport
                        .artifact_authenticated(
                            &identity,
                            &submission,
                            &expected,
                            &artifact_identifier,
                            &provider,
                            &source,
                            READING,
                            sink,
                        )
                        .await
                } else if automatic {
                    transport
                        .artifact_negotiated(
                            &identity,
                            &submission,
                            &expected,
                            &artifact_identifier,
                            &authentication,
                            sink,
                        )
                        .await
                } else {
                    transport
                        .artifact_http2(
                            &identity,
                            &submission,
                            &expected,
                            &artifact_identifier,
                            &authentication,
                            sink,
                        )
                        .await
                }
            };
            let (result, ()) = timeout(Duration::from_secs(EXCHANGE_TIMEOUT_SECONDS), async {
                tokio::join!(request, peer)
            })
            .await
            .unwrap();
            assert_eq!(
                result.is_ok(),
                accepted && !(mode >= AUTHENTICATED_MODE && status == 401),
                "status={status} defect={defect}"
            );
            assert!(
                timeout(Duration::from_millis(10), listener.accept()).await.is_err(),
                "artifact exchange retried or fell back"
            );
            if let Ok(outcome) = result {
                verify_artifact_outcome(outcome, &staged, status);
            }
        }
    }
    verify_http_one_unavailable_cases(
        &listener,
        &transport,
        &identity,
        &submission,
        &expected,
        &authentication,
        &artifact_identifier,
    )
    .await;
    verify_large_artifact_streams(
        &listener,
        &transport,
        &identity,
        &submission,
        &mut expected,
        &authentication,
    )
    .await;
    // Cancelling after a partial body drops the connection and cannot produce
    // a receipt. Private-file cleanup is the staging owner's responsibility.
    expected.byte_length = ARTIFACT_BYTES;
    expected.artifact_digest =
        Sha256::digest(b"abc").iter().map(|byte| format!("{byte:02x}")).collect();
    let (sent, partial) = tokio::sync::oneshot::channel();
    let peer = async {
        let (mut stream, _) = listener.accept().await.unwrap();
        let mut request = Vec::new();
        while !request.ends_with(b"\r\n\r\n") {
            request.push(stream.read_u8().await.unwrap());
        }
        stream
            .write_all(
                b"HTTP/1.1 200 OK\r\nContent-Type: application/zip\r\nContent-Length: 3\r\n\r\nab",
            )
            .await
            .unwrap();
        sent.send(()).unwrap();
        assert!(stream.read_u8().await.is_err(), "cancelled transfer kept its connection");
    };
    let client = async {
        let transfer = transport.stream_artifact_http1(
            &identity,
            &submission,
            &expected,
            &authentication,
            |_| Ok(()),
        );
        tokio::pin!(transfer);
        tokio::select! {
            result = &mut transfer => panic!("partial transfer completed: {result:?}"),
            _ = partial => {}
        }
        assert!(timeout(Duration::from_millis(20), &mut transfer).await.is_err());
    };
    timeout(Duration::from_secs(EXCHANGE_TIMEOUT_SECONDS), async {
        tokio::join!(client, peer);
    })
    .await
    .unwrap();
    expected.artifact_slot = "structured_result".to_owned();
    assert!(
        transport
            .stream_artifact_http1(&identity, &submission, &expected, &authentication, |_| panic!(
                "local slot streamed"
            ))
            .await
            .is_err()
    );
    assert!(timeout(Duration::from_millis(10), listener.accept()).await.is_err());
}

fn artifact_response_body(
    status: u16,
    defect: &str,
    submission: &slingshot_agent_connection::command_submission::Submission,
    artifact_identifier: &str,
) -> Vec<u8> {
    if status == SUCCESS_STATUS {
        if defect == "digest" { b"abd".to_vec() } else { b"abc".to_vec() }
    } else if defect == "bare" {
        b"{}".to_vec()
    } else {
        serde_json::json!({
                "provenance":submission.provenance, "agent_event_store_generation":7,
                "agent_operation_identifier":submission.operation.agent_operation_identifier,
                "artifact_identifier":if defect == "identity" { "4".repeat(DIGEST_CHARACTERS) } else { artifact_identifier.to_owned() },
                "artifact_slot":"content_package", "reason":if status == 404 { "missing" } else { "retention_expired" },
            }).to_string().into_bytes()
    }
}

fn unavailable_wire_response(status: u16, body: &str, chunked: bool, defect: &str) -> String {
    let media = if defect == "media" { "text/html" } else { "application/json" };
    let mut framing = if chunked {
        "Transfer-Encoding: chunked".to_owned()
    } else {
        format!("Content-Length: {}", body.len())
    };
    if defect == "location" {
        framing.push_str("\r\nLocation: /elsewhere");
    }
    let payload = if defect == "short" { &body[..body.len() - 1] } else { body };
    let wire_body = if chunked {
        format!(
            "{:x}\r\n{payload}\r\n0\r\n{}\r\n",
            payload.len(),
            if defect == "trailer" { "X-Test: bad\r\n" } else { "" }
        )
    } else {
        format!("{payload}{}", if defect == "surplus" { "!" } else { "" })
    };
    format!("HTTP/1.1 {status} Error\r\nContent-Type: {media}\r\n{framing}\r\n\r\n{wire_body}")
}

trait ArtifactPeer: tokio::io::AsyncRead + tokio::io::AsyncWrite + Unpin {}
impl<Stream: tokio::io::AsyncRead + tokio::io::AsyncWrite + Unpin> ArtifactPeer for Stream {}

async fn accept_artifact_peer(
    listener: &tokio::net::TcpListener,
    mode: usize,
) -> Box<dyn ArtifactPeer> {
    use rustls_pki_types::{CertificateDer, PrivateKeyDer, pem::PemObject};
    let (stream, _) = listener.accept().await.unwrap();
    if mode == 1 || mode == TLS_THIRTEEN_MODE {
        let version = if mode == 1 { &rustls::version::TLS12 } else { &rustls::version::TLS13 };
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
    }
}

struct ArtifactPeerCase<'peer> {
    listener: &'peer tokio::net::TcpListener,
    mode: usize,
    status: u16,
    defect: &'peer str,
    accepted: bool,
    body: &'peer [u8],
    agent_operation_identifier: &'peer str,
    authorization: &'peer [u8],
}

async fn serve_artifact_peer(case: ArtifactPeerCase<'_>) {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    let ArtifactPeerCase {
        listener,
        mode,
        status,
        defect,
        accepted,
        body,
        agent_operation_identifier,
        authorization,
    } = case;
    let mut stream = accept_artifact_peer(listener, mode).await;
    if mode >= FIRST_PLAIN_HTTP_ONE_MODE {
        let mut request = Vec::new();
        while !request.ends_with(b"\r\n\r\n") {
            request.push(stream.read_u8().await.unwrap());
        }
        let route = format!(
            "GET /aem/bin/slingshot/agent/artifact?agent_operation_identifier={}&artifact_slot=content_package HTTP/1.1\r\n",
            agent_operation_identifier
        );
        assert!(request.starts_with(route.as_bytes()));
        assert!(request.windows(authorization.len()).any(|part| part == authorization));
        stream.write_all(format!("HTTP/1.1 {status} Artifact\r\nContent-Type: {}\r\nContent-Length: {}\r\n{}\r\n",if status==SUCCESS_STATUS {"application/zip"} else {"application/json"},body.len()+usize::from(defect=="short"),if defect=="trailer" {"Trailer: x\r\n"} else {""}).as_bytes()).await.unwrap();
        stream.write_all(body).await.unwrap();
        stream.shutdown().await.unwrap();
        return;
    }
    let mut preface = [0; HTTP_TWO_PREFACE_AND_SETTINGS_BYTES];
    stream.read_exact(&mut preface).await.unwrap();
    assert_eq!(&preface[..24], b"PRI * HTTP/2.0\r\n\r\nSM\r\n\r\n");
    stream.write_all(&[0, 0, 0, 4, 0, 0, 0, 0, 0]).await.unwrap();
    let mut acknowledgement = [0; HTTP_TWO_FRAME_HEADER_BYTES];
    stream.read_exact(&mut acknowledgement).await.unwrap();
    stream.write_all(&acknowledgement).await.unwrap();
    let mut header = [0; HTTP_TWO_FRAME_HEADER_BYTES];
    stream.read_exact(&mut header).await.unwrap();
    assert_eq!((header[3], header[4]), (1, 5));
    let length = usize::from(header[0]) << LENGTH_HIGH_SHIFT
        | usize::from(header[1]) << LENGTH_MIDDLE_SHIFT
        | usize::from(header[2]);
    assert!(length <= 16384);
    let mut request = vec![0; length];
    stream.read_exact(&mut request).await.unwrap();
    let route = format!(
        "/aem/bin/slingshot/agent/artifact?agent_operation_identifier={}&artifact_slot=content_package",
        agent_operation_identifier
    );
    assert!(request.windows(route.len()).any(|bytes| bytes == route.as_bytes()));
    assert!(request.windows(authorization.len()).any(|part| part == authorization));
    let media = if status == SUCCESS_STATUS { "application/zip" } else { "application/json" };
    let mut block = Vec::new();
    for (name, value) in [
        (":status", status.to_string()),
        ("content-type", media.to_owned()),
        ("content-length", (body.len() + usize::from(defect == "short")).to_string()),
    ] {
        block.extend_from_slice(&[0, name.len() as u8]);
        block.extend_from_slice(name.as_bytes());
        block.push(value.len() as u8);
        block.extend_from_slice(value.as_bytes());
    }
    let encode = |kind: u8, flags: u8, payload: &[u8]| {
        let length = (payload.len() as u32).to_be_bytes();
        let mut bytes = vec![length[1], length[2], length[3], kind, flags, 0, 0, 0, 1];
        bytes.extend_from_slice(payload);
        bytes
    };
    let mut wire = encode(1, END_HEADERS_FLAG, &block);
    wire.extend_from_slice(&encode(0, 1, body));
    if defect == "trailer" {
        wire.extend_from_slice(&encode(1, END_HEADERS_AND_STREAM_FLAGS, &[]));
    }
    stream.write_all(&wire).await.unwrap();
    let mut close = Vec::new();
    let _ = stream.read_to_end(&mut close).await;
    if accepted {
        assert_eq!(close, [0, 0, 8, 7, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0]);
    }
    stream.shutdown().await.unwrap();
}

async fn verify_http_one_stream_cases(
    listener: &tokio::net::TcpListener,
    transport: &SelectedAuthorTransport,
    identity: &ExecutionIdentity,
    submission: &slingshot_agent_connection::command_submission::Submission,
    expected: &slingshot_agent_connection::artifact_download::ExpectedArtifact,
    authentication: &slingshot_agent_connection::authentication::environment_provider::RequestAuthentication,
) {
    use slingshot_agent_connection::selected_author_http::FiniteHttpFailure;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::time::{Duration, timeout};
    for (head, body, accepted, sink_refuses) in [
        ("Content-Length: 3", "abc", true, false),
        ("Transfer-Encoding: chunked", "1\r\na\r\n2\r\nbc\r\n0\r\n\r\n", true, false),
        (
            "Transfer-Encoding: chunked",
            "1;flag\r\na\r\n2;v=\";\"\r\nbc\r\n0;end\r\n\r\n",
            true,
            false,
        ),
        ("Transfer-Encoding: chunked", "3\r\nabc\r\n0;broken=\r\n\r\n", false, false),
        ("Content-Length: 3", "ab", false, false),
        ("Content-Length: 3", "abd", false, false),
        ("Content-Length: 3", "abc!", false, false),
        ("Content-Length: 2", "ab", false, false),
        ("Content-Length: 3\r\nContent-Encoding: gzip", "abc", false, false),
        ("Transfer-Encoding: chunked", "3\r\nabc\r\n0\r\nX-Test: bad\r\n\r\n", false, false),
        ("Transfer-Encoding: chunked", "4\r\nabcd\r\n0\r\n\r\n", false, false),
        ("Content-Length: 3", "abc", false, true),
    ] {
        let peer = async {
            let (mut stream, _) = listener.accept().await.unwrap();
            let mut request = Vec::new();
            while !request.ends_with(b"\r\n\r\n") {
                request.push(stream.read_u8().await.unwrap());
            }
            assert!(String::from_utf8(request).unwrap().starts_with(&format!(
                "GET /aem/bin/slingshot/agent/artifact?agent_operation_identifier={}&artifact_slot=content_package HTTP/1.1\r\n",
                submission.operation.agent_operation_identifier
            )));
            let _ = stream
                .write_all(
                    format!(
                        "HTTP/1.1 200 OK\r\nContent-Type: application/zip\r\n{head}\r\n\r\n{body}"
                    )
                    .as_bytes(),
                )
                .await;
        };
        let mut staged = Vec::new();
        let (result, ()) = timeout(Duration::from_secs(EXCHANGE_TIMEOUT_SECONDS), async {
            tokio::join!(
                transport.stream_artifact_http1(
                    identity,
                    submission,
                    expected,
                    authentication,
                    |bytes| {
                        assert!(bytes.len() <= 8192);
                        if sink_refuses {
                            return Err(FiniteHttpFailure::Body);
                        }
                        staged.extend_from_slice(bytes);
                        Ok(())
                    }
                ),
                peer
            )
        })
        .await
        .unwrap();
        assert_eq!(result.is_ok(), accepted, "{head}: {body:?}");
        if accepted {
            assert_eq!(result.unwrap().byte_length(), ARTIFACT_BYTES);
            assert_eq!(staged, b"abc");
        }
    }
}

async fn verify_http_one_unavailable_cases(
    listener: &tokio::net::TcpListener,
    transport: &SelectedAuthorTransport,
    identity: &ExecutionIdentity,
    submission: &slingshot_agent_connection::command_submission::Submission,
    expected: &slingshot_agent_connection::artifact_download::ExpectedArtifact,
    authentication: &slingshot_agent_connection::authentication::environment_provider::RequestAuthentication,
    artifact_identifier: &str,
) {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::time::{Duration, timeout};
    for (status, reason, chunked, defect, accepted) in [
        (404, "missing", false, "", true),
        (410, "retention_expired", true, "", true),
        (404, "retention_expired", false, "", false),
        (410, "retention_expired", false, "short", false),
        (410, "retention_expired", false, "surplus", false),
        (410, "retention_expired", true, "trailer", false),
        (410, "retention_expired", false, "identity", false),
        (410, "retention_expired", false, "media", false),
        (410, "retention_expired", false, "location", false),
    ] {
        let body = serde_json::json!({
            "provenance":submission.provenance, "agent_event_store_generation":7,
            "agent_operation_identifier":submission.operation.agent_operation_identifier,
            "artifact_identifier":if defect == "identity" { "4".repeat(DIGEST_CHARACTERS) } else { artifact_identifier.to_owned() },
            "artifact_slot":"content_package", "reason":reason,
        }).to_string();
        let peer = async {
            let (mut stream, _) = listener.accept().await.unwrap();
            let mut request = Vec::new();
            while !request.ends_with(b"\r\n\r\n") {
                request.push(stream.read_u8().await.unwrap());
            }
            let wire = unavailable_wire_response(status, &body, chunked, defect);
            let _ = stream.write_all(wire.as_bytes()).await;
        };
        let (result, ()) = timeout(Duration::from_secs(EXCHANGE_TIMEOUT_SECONDS), async {
            tokio::join!(
                transport.artifact_http1(
                    identity,
                    submission,
                    expected,
                    artifact_identifier,
                    authentication,
                    |_| panic!("error body entered artifact staging")
                ),
                peer
            )
        })
        .await
        .unwrap();
        assert_eq!(result.is_ok(), accepted, "{status} {reason} {defect}");
        if accepted {
            let slingshot_agent_connection::selected_author_http::ArtifactHttpOutcome::Unavailable { evidence, .. } = result.unwrap() else { panic!("unavailable became artifact bytes") };
            assert_eq!(
                evidence.reason(),
                if status == 404 {
                    slingshot_agent_protocol::artifact_unavailable::UnavailableReason::Missing
                } else {
                    slingshot_agent_protocol::artifact_unavailable::UnavailableReason::RetentionExpired
                }
            );
        }
    }
}

async fn verify_large_artifact_streams(
    listener: &tokio::net::TcpListener,
    transport: &SelectedAuthorTransport,
    identity: &ExecutionIdentity,
    submission: &slingshot_agent_connection::command_submission::Submission,
    expected: &mut slingshot_agent_connection::artifact_download::ExpectedArtifact,
    authentication: &slingshot_agent_connection::authentication::environment_provider::RequestAuthentication,
) {
    use sha2::{Digest, Sha256};
    use slingshot_domain::author_agent_transport_contract::AuthorAgentTransportContract;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::time::{Duration, timeout};
    // A streamed artifact may exceed the finite JSON-body bound; neither peer
    // nor sink needs a whole-artifact allocation to exercise that distinction.
    let block = [b'x'; STREAM_BLOCK_BYTES];
    let blocks = AuthorAgentTransportContract::embedded()
        .limit("maximum_finite_response_body_bytes")
        / block.len() as u64
        + 1;
    expected.byte_length = blocks * block.len() as u64;
    let mut digest = Sha256::new();
    for _ in 0..blocks {
        digest.update(block);
    }
    expected.artifact_digest = digest.finalize().iter().map(|byte| format!("{byte:02x}")).collect();
    for chunked in [false, true] {
        let peer = async {
            let (mut stream, _) = listener.accept().await.unwrap();
            let mut request = Vec::new();
            while !request.ends_with(b"\r\n\r\n") {
                request.push(stream.read_u8().await.unwrap());
            }
            let framing = if chunked {
                "Transfer-Encoding: chunked".to_owned()
            } else {
                format!("Content-Length: {}", expected.byte_length)
            };
            stream
                .write_all(
                    format!(
                        "HTTP/1.1 200 OK\r\nContent-Type: application/zip\r\n{framing}\r\n\r\n"
                    )
                    .as_bytes(),
                )
                .await
                .unwrap();
            for _ in 0..blocks {
                if chunked {
                    stream.write_all(b"2000\r\n").await.unwrap();
                }
                stream.write_all(&block).await.unwrap();
                if chunked {
                    stream.write_all(b"\r\n").await.unwrap();
                }
            }
            if chunked {
                stream.write_all(b"0\r\n\r\n").await.unwrap();
            }
        };
        let mut count = 0_u64;
        let (result, ()) = timeout(Duration::from_secs(EXCHANGE_TIMEOUT_SECONDS), async {
            tokio::join!(
                transport.stream_artifact_http1(
                    identity,
                    submission,
                    expected,
                    authentication,
                    |bytes| {
                        assert!(bytes.len() <= block.len());
                        assert!(bytes.iter().all(|byte| *byte == b'x'));
                        count += bytes.len() as u64;
                        Ok(())
                    }
                ),
                peer
            )
        })
        .await
        .unwrap();
        assert_eq!(result.unwrap().byte_length(), expected.byte_length);
        assert_eq!(count, expected.byte_length);
    }
}

fn verify_artifact_outcome(
    outcome: slingshot_agent_connection::selected_author_http::ArtifactHttpOutcome,
    staged: &[u8],
    status: u16,
) {
    use slingshot_agent_connection::selected_author_http::ArtifactHttpOutcome;
    match outcome {
        ArtifactHttpOutcome::Transferred(receipt) => {
            assert_eq!(receipt.byte_length(), ARTIFACT_BYTES);
            assert_eq!(staged, b"abc");
        }
        ArtifactHttpOutcome::Unavailable { .. } => assert!(staged.is_empty()),
        ArtifactHttpOutcome::Unauthorized => {
            assert_eq!(status, 401);
            assert!(staged.is_empty());
        }
    }
}
