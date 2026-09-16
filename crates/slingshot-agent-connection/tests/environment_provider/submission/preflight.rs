//! Submission preflight checks on the same listener as the subsequent POSTs.

use super::*;
use slingshot_agent_connection::authentication::environment_provider::{
    AsyncEnvironmentAuthenticationProvider, RequestAuthentication,
};
use slingshot_agent_connection::command_submission::{ExpectedArtifactManifest, Submission};
use slingshot_agent_connection::selected_author_submission::SubmissionSendRefusal;
use slingshot_agent_protocol::{
    identity::WireOperationIdentity, wire_contract::ExpectedProvenance,
};
use slingshot_domain::agent_identity::AgentEventStoreGeneration;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::time::{Duration, timeout};

const CHANGED_EVENT_GENERATION: u64 = 8;
const HTTP_TWO_PREFACE_AND_SETTINGS_BYTES: usize = 39;
const HTTP_TWO_FRAME_HEADER_BYTES: usize = 9;
const LENGTH_HIGH_SHIFT: usize = 16;
const LENGTH_MIDDLE_SHIFT: usize = 8;
const EXCHANGE_TIMEOUT_SECONDS: u64 = 5;
const NEGOTIATED_MODE: u8 = 2;
const AUTHENTICATED_MODE: u8 = 3;
const AUTHENTICATED_ASYNC_MODE: u8 = 4;

/// Borrows the exact exchange state used by the submission matrix.
pub(super) struct Preflight<'fixture> {
    pub(super) listener: &'fixture tokio::net::TcpListener,
    pub(super) transport: &'fixture SelectedAuthorTransport,
    pub(super) identity: &'fixture ExecutionIdentity,
    pub(super) authentication: &'fixture RequestAuthentication,
    pub(super) async_provider: &'fixture AsyncEnvironmentAuthenticationProvider,
    pub(super) provider: &'fixture EnvironmentAuthenticationProvider,
    pub(super) source: &'fixture CountingSource,
    pub(super) expected: &'fixture ExpectedProvenance,
}

impl Preflight<'_> {
    async fn run_discovery(&self, mode: u8, http2: bool) -> bool {
        let result = match mode {
            AUTHENTICATED_ASYNC_MODE => self
                .transport
                .discover_capabilities_authenticated_async(
                    self.identity,
                    "query_paths",
                    Some(EVENT_GENERATION),
                    self.async_provider,
                    &async_cases::NoClocks,
                    &async_cases::NoClocks,
                )
                .await
                .is_ok(),
            AUTHENTICATED_MODE => self
                .transport
                .discover_capabilities_authenticated(
                    self.identity,
                    "query_paths",
                    Some(EVENT_GENERATION),
                    self.provider,
                    self.source,
                    READING,
                )
                .await
                .is_ok(),
            NEGOTIATED_MODE => self
                .transport
                .discover_capabilities_negotiated(
                    self.identity,
                    "query_paths",
                    Some(EVENT_GENERATION),
                    self.authentication,
                )
                .await
                .is_ok(),
            _ if http2 => self
                .transport
                .discover_capabilities_http2(
                    self.identity,
                    "query_paths",
                    Some(EVENT_GENERATION),
                    self.authentication,
                )
                .await
                .is_ok(),
            _ => self
                .transport
                .discover_capabilities(
                    self.identity,
                    "query_paths",
                    Some(EVENT_GENERATION),
                    self.authentication,
                )
                .await
                .is_ok(),
        };
        result
    }

    /// Checks every discovery mode before the submission exchanges start.
    pub(super) async fn discover(&self) {
        let Self { listener, authentication, expected, .. } = *self;
        for mode in [0, 1, NEGOTIATED_MODE, AUTHENTICATED_MODE, AUTHENTICATED_ASYNC_MODE] {
            let http2 = mode == 1;
            for (generation, ready, location, compatible) in [
                (EVENT_GENERATION, true, false, true),
                (CHANGED_EVENT_GENERATION, true, false, false),
                (EVENT_GENERATION, false, false, false),
                (EVENT_GENERATION, true, true, false),
            ] {
                let document = serde_json::json!({
                "format": "slingshot.agent/1",
                "agent_event_store_generation": generation,
                "canonical_json_contract_digest": expected.canonical_json_contract_digest,
                "transport_contract_digest": expected.transport_contract_digest,
                "command_contracts": [slingshot_agent_protocol::identity::WireContractIdentity::from(&expected.command_contract)],
                "continuation_authority_ready": ready,
            }).to_string();
                let peer = async {
                    let (mut socket, _) = listener.accept().await.unwrap();
                    let mut head = Vec::new();
                    if http2 {
                        let mut preface = [0; HTTP_TWO_PREFACE_AND_SETTINGS_BYTES];
                        socket.read_exact(&mut preface).await.unwrap();
                        assert_eq!(&preface[..24], b"PRI * HTTP/2.0\r\n\r\nSM\r\n\r\n");
                        socket.write_all(&[0, 0, 0, 4, 0, 0, 0, 0, 0]).await.unwrap();
                        let mut header = [0; HTTP_TWO_FRAME_HEADER_BYTES];
                        socket.read_exact(&mut header).await.unwrap();
                        socket.write_all(&header).await.unwrap();
                        socket.read_exact(&mut header).await.unwrap();
                        assert_eq!((header[3], header[4]), (1, 5));
                        let length = usize::from(header[0]) << LENGTH_HIGH_SHIFT
                            | usize::from(header[1]) << LENGTH_MIDDLE_SHIFT
                            | usize::from(header[2]);
                        assert!(length <= 16384);
                        head.resize(length, 0);
                        socket.read_exact(&mut head).await.unwrap();
                        assert!(
                            head.windows(b"/aem/bin/slingshot/agent/capabilities".len())
                                .any(|part| part == b"/aem/bin/slingshot/agent/capabilities")
                        );
                        authentication.lend_value_bytes(|value| {
                            assert!(head.windows(value.len()).any(|part| part == value))
                        });
                        let mut block = vec![0x88];
                        for (name, value) in [
                            ("content-type", "application/json".to_owned()),
                            ("content-length", document.len().to_string()),
                        ] {
                            block.extend_from_slice(&[0, name.len() as u8]);
                            block.extend_from_slice(name.as_bytes());
                            block.push(value.len() as u8);
                            block.extend_from_slice(value.as_bytes());
                        }
                        if location {
                            block.extend_from_slice(b"\x00\x08location\x0a/elsewhere");
                        }
                        for (kind, flags, bytes) in
                            [(1, 4, block.as_slice()), (0, 1, document.as_bytes())]
                        {
                            let length = (bytes.len() as u32).to_be_bytes();
                            socket
                                .write_all(&[
                                    length[1], length[2], length[3], kind, flags, 0, 0, 0, 1,
                                ])
                                .await
                                .unwrap();
                            socket.write_all(bytes).await.unwrap();
                        }
                        let mut close = Vec::new();
                        let _ = socket.read_to_end(&mut close).await;
                        return;
                    }
                    while !head.ends_with(b"\r\n\r\n") {
                        head.push(socket.read_u8().await.unwrap());
                    }
                    let head = String::from_utf8(head).unwrap();
                    assert!(
                        head.starts_with("GET /aem/bin/slingshot/agent/capabilities HTTP/1.1\r\n")
                    );
                    assert!(head.contains("Authorization: Basic "));
                    socket.write_all(format!("HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\n{}\r\n{document}", document.len(), if location {"Location: /elsewhere\r\n"} else {""}).as_bytes()).await.unwrap();
                };
                let (outcome, ()) = timeout(Duration::from_secs(EXCHANGE_TIMEOUT_SECONDS), async {
                    tokio::join!(self.run_discovery(mode, http2), peer)
                })
                .await
                .unwrap();
                assert_eq!(outcome, compatible);
            }
        }
    }

    /// Preserves Unicode and proves local refusals do not open a socket.
    pub(super) async fn checked_submission(&self, arguments: &str) -> Submission {
        let Self { listener, transport, identity, authentication, expected, .. } = *self;
        let submission = Submission::build(
            expected,
            WireOperationIdentity::of(
                &identity.author_target_identity_digest,
                &identity.selected_environment_revision,
                &identity.operation_identifier,
                AgentEventStoreGeneration::of(EVENT_GENERATION),
            ),
            "subscription-one",
            arguments,
            ExpectedArtifactManifest::empty(),
        )
        .unwrap();
        let mut wrong = identity.clone();
        for invalid in [
            "{}",
            r#" {"root_path":"/content/é"}"#,
            r#"{"root_path":"/content/../invalid"}"#,
            r#"{"extra":true,"root_path":"/content/é"}"#,
            r#"{"result_window":null,"root_path":"/content/é"}"#,
        ] {
            let invalid = Submission::build(
                expected,
                submission.operation.clone(),
                "subscription-one",
                invalid,
                ExpectedArtifactManifest::empty(),
            )
            .unwrap();
            assert!(transport.require_submission(identity, &invalid).is_err());
        }
        wrong.operation_identifier = "another-operation".to_owned();
        assert_eq!(
            transport
                .send_submission_with_fresh_token(&wrong, &submission, authentication, 1)
                .await,
            Err(SubmissionSendRefusal::Identity)
        );
        let mut drifted = submission.clone();
        drifted.submitted_command_digest = "another-digest".to_owned();
        assert_eq!(
            transport.send_submission_with_fresh_token(identity, &drifted, authentication, 1).await,
            Err(SubmissionSendRefusal::Derivation)
        );
        assert!(
            timeout(Duration::from_millis(10), listener.accept()).await.is_err(),
            "identity refusal opened a socket"
        );
        submission
    }
}
