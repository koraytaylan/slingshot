//! Selected-author selection integration checks.

use super::*;

const LITERAL_DECODER_CAPACITY_BYTES: u64 = 65536;
const QUERY_EXCHANGE_TIMEOUT_SECONDS: u64 = 5;

#[tokio::test]
async fn selected_query_limit_counts_encoded_bytes_before_opening_a_socket() {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::time::{Duration, timeout};
    const NO_REQUEST_OBSERVATION_MILLISECONDS: u64 = 10;
    let limit = usize::try_from(
        slingshot_domain::author_agent_transport_contract::AuthorAgentTransportContract::embedded()
            .limit("maximum_route_query_bytes"),
    )
    .unwrap();
    for (suffix, encoded_suffix) in [("x", "x"), (" ", "%20")] {
        for extra in [0, 1] {
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
            let source = CountingSource { exchanges: Cell::new(0) };
            let (authentication, _) = provider.authenticate(&endpoint, READING, &source).unwrap();
            let prefix = "x".repeat(limit - "q=".len() - encoded_suffix.len() + extra);
            let value = format!("{prefix}{suffix}");
            let query = [("q", value.as_str())];
            let fields = http::HeaderMap::new();
            let request = transport.finite_http1_query(
                http::Method::GET,
                &["lookup"],
                &query,
                &authentication,
                &fields,
                b"",
            );
            if extra != 0 {
                assert!(request.await.is_err());
                assert!(
                    timeout(
                        Duration::from_millis(NO_REQUEST_OBSERVATION_MILLISECONDS),
                        listener.accept()
                    )
                    .await
                    .is_err()
                );
                continue;
            }
            let peer = async {
                let (mut socket, _) = listener.accept().await.unwrap();
                let mut head = Vec::new();
                while !head.ends_with(b"\r\n\r\n") {
                    head.push(socket.read_u8().await.unwrap());
                }
                let head = String::from_utf8(head).unwrap();
                assert!(
                    head.starts_with(&format!(
                        "GET /lookup?q={prefix}{encoded_suffix} HTTP/1.1\r\n"
                    ))
                );
                socket.write_all(b"HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: 2\r\n\r\n{}").await.unwrap();
            };
            let (answer, ()) =
                timeout(Duration::from_secs(QUERY_EXCHANGE_TIMEOUT_SECONDS), async {
                    tokio::join!(request, peer)
                })
                .await
                .unwrap();
            assert_eq!(answer.unwrap().response.body, b"{}");
            assert!(
                timeout(
                    Duration::from_millis(NO_REQUEST_OBSERVATION_MILLISECONDS),
                    listener.accept()
                )
                .await
                .is_err()
            );
        }
    }
}

#[test]
fn author_connection_is_a_frozen_target_bound_boundary_without_a_publisher() {
    let provider = provider(PROTECTED_PROFILE, PROTECTED_ENVIRONMENT);
    let connection = provider.snapshot().author_connection();

    assert_eq!(connection.author(), provider.snapshot().author());
    assert_eq!(connection.target(), provider.snapshot().target());
    assert_eq!(connection.revision(), provider.snapshot().revision());
    assert_eq!(connection.trust(), provider.snapshot().author_trust());
    assert_eq!(
        connection.requires_transport_layer_security(),
        provider.snapshot().author().is_protected()
    );
    assert_eq!(
        connection.endpoint(&["bin", "slingshot", "agent", "jobs"]),
        provider.author_endpoint(&["bin", "slingshot", "agent", "jobs"]),
    );
    connection
        .require_target(&provider.snapshot().target().to_string())
        .expect("the selected target is accepted");
    assert_eq!(
        connection.require_target("another-target"),
        Err(SelectedAuthorConnectionRefusal::AnotherTarget),
    );
    let identity = ExecutionIdentity {
        attempt: 1,
        author_target_identity_digest: provider.snapshot().target().to_string(),
        operation_identifier: "operation-one".to_owned(),
        selected_environment_revision: provider.snapshot().revision().to_string(),
    };
    connection.require_execution(&identity).expect("the selected partition is accepted");
    let wrong_revision = ExecutionIdentity {
        selected_environment_revision: "another-revision".to_owned(),
        ..identity
    };
    assert_eq!(
        connection.require_execution(&wrong_revision),
        Err(SelectedAuthorConnectionRefusal::AnotherRevision),
    );
}

/// The production connector can be constructed only from the frozen selected
/// connection, and keeps its route/trust material out of debug output.
#[test]
fn selected_author_transport_freezes_the_author_only_connection_policy() {
    let protected = provider(PROTECTED_PROFILE, PROTECTED_ENVIRONMENT);
    let connection = protected.snapshot().author_connection();
    let transport = SelectedAuthorTransport::new(connection)
        .expect("the protected fixture carries selected author trust");
    let rendered = format!("{transport:?}");
    assert!(!rendered.contains(protected.snapshot().author().as_text()));
    assert!(!rendered.contains(protected.snapshot().publisher_metadata().as_text()));

    let cleartext = provider(CLEARTEXT_PROFILE, CLEARTEXT_ENVIRONMENT);
    SelectedAuthorTransport::new(cleartext.snapshot().author_connection())
        .expect("the explicitly warned cleartext fixture is still selected at startup");
}

#[test]
fn http2_request_heads_share_selected_origin_query_and_authentication_boundaries() {
    use slingshot_agent_connection::selected_author_hpack_string::{LiteralString, StringRefusal};
    fn string(bytes: &mut &[u8]) -> Vec<u8> {
        let mut decoder = LiteralString::start(bytes[0], LITERAL_DECODER_CAPACITY_BYTES).unwrap();
        *bytes = &bytes[1..];
        let mut output = Vec::new();
        while !decoder.is_complete() {
            decoder
                .push(bytes[0], |byte| {
                    output.push(byte);
                    Ok::<(), StringRefusal>(())
                })
                .unwrap();
            *bytes = &bytes[1..];
        }
        decoder.finish().unwrap();
        output
    }
    let endpoint = "http://127.0.0.1:4502/aem";
    let mut files = profile_files();
    replace_profile(&mut files, "profiles/mike.toml", |text| {
        text.replace("http://author.example.com", endpoint)
            .replace("allow_insecure_author_transport = true\n", "")
    });
    let provider =
        provider_from_loaded(loaded_from_files(files), CLEARTEXT_PROFILE, CLEARTEXT_ENVIRONMENT);
    let transport = SelectedAuthorTransport::new(provider.snapshot().author_connection()).unwrap();
    let (authentication, _) = provider
        .authenticate(endpoint, READING, &CountingSource { exchanges: Cell::new(0) })
        .unwrap();
    let fields = http::HeaderMap::new();
    let head = transport
        .encode_http2_request_head(
            http::Method::POST,
            &["bin", "slingshot", "agent", "submit"],
            &[("q", "é /%")],
            &authentication,
            &fields,
            b"{}",
        )
        .unwrap();
    assert_eq!(format!("{head:?}"), "EncodedRequestHead([redacted])");
    let mut block = Vec::new();
    for frame in head.frames() {
        assert_eq!(frame[4] & 1, 0);
        block.extend_from_slice(&frame[9..]);
    }
    let mut input = block.as_slice();
    let mut decoded = Vec::new();
    while !input.is_empty() {
        assert_eq!(input[0], 0x10, "private request fields must never be indexed");
        input = &input[1..];
        let name = string(&mut input);
        let value = string(&mut input);
        decoded.push((name, value));
    }
    for (index, name, value) in [
        (0, ":method", "POST"),
        (1, ":scheme", "http"),
        (2, ":authority", "127.0.0.1:4502"),
        (3, ":path", "/aem/bin/slingshot/agent/submit?q=%C3%A9%20%2F%25"),
        (4, "accept-encoding", "identity"),
        (5, "content-length", "2"),
    ] {
        assert_eq!(decoded[index], (name.as_bytes().to_vec(), value.as_bytes().to_vec()));
    }
    assert_eq!(decoded[6].0, b"authorization");
    authentication.lend_value_bytes(|value| assert_eq!(decoded[6].1, value));
    assert_eq!(decoded.len(), 7);
    for name in [
        "host",
        "authorization",
        "content-length",
        "connection",
        "transfer-encoding",
        "upgrade",
        "trailer",
        "proxy-connection",
        "keep-alive",
        "te",
    ] {
        let mut forbidden = http::HeaderMap::new();
        forbidden.insert(
            http::HeaderName::from_bytes(name.as_bytes()).unwrap(),
            http::HeaderValue::from_static("x"),
        );
        assert!(
            transport
                .encode_http2_request_head(
                    http::Method::GET,
                    &["bin"],
                    &[],
                    &authentication,
                    &forbidden,
                    b""
                )
                .is_err()
        );
    }
    assert!(
        transport
            .encode_http2_request_head(
                http::Method::GET,
                &["bin"],
                &[("q", "a"), ("q", "b")],
                &authentication,
                &fields,
                b""
            )
            .is_err()
    );
    assert!(
        transport
            .encode_http2_request_head(
                http::Method::CONNECT,
                &["bin"],
                &[],
                &authentication,
                &fields,
                b""
            )
            .is_err()
    );
    for value in [" value", "value ", "\tvalue", "value\t"] {
        let mut malformed = http::HeaderMap::new();
        malformed.insert("x", http::HeaderValue::from_str(value).unwrap());
        assert!(
            transport
                .encode_http2_request_head(
                    http::Method::GET,
                    &["bin"],
                    &[],
                    &authentication,
                    &malformed,
                    b""
                )
                .is_err()
        );
    }
}

/// The wire query keeps the selected context and encodes data, not a URL.
#[tokio::test]
async fn selected_lookup_query_is_bounded_ordered_and_encoded_once() {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::time::{Duration, timeout};
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let endpoint = format!("http://{}", listener.local_addr().unwrap());
    let mut files = profile_files();
    replace_profile(&mut files, "profiles/mike.toml", |text| {
        text.replace("http://author.example.com", &format!("{endpoint}/aem"))
            .replace("allow_insecure_author_transport = true\n", "")
    });
    let provider =
        provider_from_loaded(loaded_from_files(files), CLEARTEXT_PROFILE, CLEARTEXT_ENVIRONMENT);
    let transport = SelectedAuthorTransport::new(provider.snapshot().author_connection()).unwrap();
    let source = CountingSource { exchanges: Cell::new(0) };
    let (authentication, _) =
        provider.authenticate(provider.snapshot().author().as_text(), READING, &source).unwrap();
    let fields = http::HeaderMap::new();
    let route = &["bin", "slingshot", "agent", "snapshot"];
    let query_limit =
        slingshot_domain::author_agent_transport_contract::AuthorAgentTransportContract::embedded()
            .limit("maximum_route_query_bytes");
    let huge = "x".repeat(usize::try_from(query_limit).unwrap() + 1);
    for query in [vec![("id", "one"), ("id", "two")], vec![("id", huge.as_str())]] {
        assert!(
            transport
                .finite_http1_query(http::Method::GET, route, &query, &authentication, &fields, b"")
                .await
                .is_err()
        );
    }
    assert!(timeout(Duration::from_millis(10), listener.accept()).await.is_err());
    let peer = async {
        let (mut stream, _) = listener.accept().await.unwrap();
        let mut bytes = Vec::new();
        while !bytes.ends_with(b"\r\n\r\n") {
            bytes.push(stream.read_u8().await.unwrap());
        }
        assert!(bytes.starts_with(b"GET /aem/bin/slingshot/agent/snapshot?agent_operation_identifier=a%20%26%252F%C3%A9 HTTP/1.1\r\n"));
        stream
            .write_all(
                b"HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: 2\r\n\r\n{}",
            )
            .await
            .unwrap();
    };
    let query = [("agent_operation_identifier", "a &%2Fé")];
    let (result, ()) = timeout(Duration::from_secs(QUERY_EXCHANGE_TIMEOUT_SECONDS), async {
        tokio::join!(
            transport.finite_http1_query(
                http::Method::GET,
                route,
                &query,
                &authentication,
                &fields,
                b""
            ),
            peer
        )
    })
    .await
    .unwrap();
    assert_eq!(result.unwrap().response.body, b"{}");
}
