//! HTTP/2 submission peer stages preserve framing, TLS negotiation and bound bytes.

use super::*;
use rustls_pki_types::{CertificateDer, PrivateKeyDer, pem::PemObject};
use slingshot_agent_connection::authentication::environment_provider::RequestAuthentication;
use slingshot_agent_connection::command_submission::Submission;
use tokio::io::{AsyncReadExt, AsyncWriteExt};

const PREFACE_AND_SETTINGS_BYTES: usize = 39;
const FRAME_HEADER_BYTES: usize = 9;
const LENGTH_HIGH_SHIFT: usize = 16;
const LENGTH_MIDDLE_SHIFT: usize = 8;
const DEFAULT_FRAME_BYTES: usize = 16384;
const END_HEADERS_FLAG: u8 = 4;
const END_HEADERS_AND_STREAM_FLAGS: u8 = 5;
const END_STREAM_FLAG: u8 = 1;

trait PeerStream: tokio::io::AsyncRead + tokio::io::AsyncWrite + Unpin {}
impl<Stream: tokio::io::AsyncRead + tokio::io::AsyncWrite + Unpin> PeerStream for Stream {}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Stage {
    Capability,
    Token,
    Submission,
    Snapshot,
}

/// Borrows the exact identity, responses and listener owned by the submission matrix.
pub(super) struct Peer<'fixture> {
    pub(super) listener: &'fixture tokio::net::TcpListener,
    pub(super) authentication: &'fixture RequestAuthentication,
    pub(super) automatic: bool,
    pub(super) endpoint: &'fixture str,
    pub(super) submission: &'fixture Submission,
    pub(super) media: &'fixture str,
    pub(super) answer: &'fixture str,
    pub(super) snapshot: &'fixture str,
    pub(super) capability: &'fixture str,
}

impl Peer<'_> {
    /// Serves the original negotiated or explicit HTTP/2 exchange sequence.
    pub(super) async fn serve(&self, defect: &str) {
        for stage in Self::stages(self.automatic, defect) {
            let mut socket = self.accept().await;
            let head = read_head(socket.as_mut(), stage).await;
            self.verify_head(&head, stage);
            if stage == Stage::Submission {
                let body = read_body(socket.as_mut()).await;
                assert_eq!(body, self.submission.wire_body().unwrap());
            }
            self.write_response(socket.as_mut(), stage, defect).await;
            let mut close = Vec::new();
            let _ = socket.read_to_end(&mut close).await;
            if self.automatic {
                socket.shutdown().await.unwrap();
            }
        }
    }

    fn stages(automatic: bool, defect: &str) -> Vec<Stage> {
        let mut stages = Vec::new();
        if automatic {
            stages.push(Stage::Capability);
        }
        stages.push(Stage::Token);
        if !["token", "guard"].contains(&defect) {
            stages.push(Stage::Submission);
        }
        if automatic && defect.is_empty() {
            stages.push(Stage::Snapshot);
        }
        stages
    }

    async fn accept(&self) -> Box<dyn PeerStream> {
        let (socket, _) = self.listener.accept().await.unwrap();
        if !self.automatic {
            return Box::new(socket);
        }
        let mut configuration = rustls::ServerConfig::builder_with_provider(Arc::new(
            rustls::crypto::ring::default_provider(),
        ))
        .with_protocol_versions(&[&rustls::version::TLS13])
        .unwrap()
        .with_no_client_auth()
        .with_single_cert(
            vec![
                CertificateDer::from_pem_slice(include_bytes!(
                    "../../fixtures/selected-author-tls/leaf.pem"
                ))
                .unwrap(),
            ],
            PrivateKeyDer::from_pem_slice(include_bytes!(
                "../../fixtures/selected-author-tls/test-only-private-key.pem"
            ))
            .unwrap(),
        )
        .unwrap();
        configuration.alpn_protocols = vec![b"h2".to_vec(), b"http/1.1".to_vec()];
        let socket =
            tokio_rustls::TlsAcceptor::from(Arc::new(configuration)).accept(socket).await.unwrap();
        assert_eq!(socket.get_ref().1.alpn_protocol(), Some(b"h2".as_slice()));
        Box::new(socket)
    }

    fn verify_head(&self, head: &[u8], stage: Stage) {
        let lookup_route = format!(
            "/aem/bin/slingshot/agent/snapshot?agent_operation_identifier={}",
            self.submission.operation.agent_operation_identifier
        );
        let route = match stage {
            Stage::Snapshot => lookup_route.as_bytes(),
            Stage::Capability => b"/aem/bin/slingshot/agent/capabilities".as_slice(),
            Stage::Token => b"/aem/libs/granite/csrf/token.json".as_slice(),
            Stage::Submission => b"/aem/bin/slingshot/agent/submit".as_slice(),
        };
        assert!(head.windows(route.len()).any(|part| part == route));
        self.authentication.lend_value_bytes(|value| {
            assert!(head.windows(value.len()).any(|part| part == value));
        });
        if stage != Stage::Submission {
            assert!(!head.windows(b"csrf-token".len()).any(|part| part == b"csrf-token"));
        } else {
            for value in [
                "csrf-test-value",
                self.submission.submitted_command_digest.as_str(),
                &format!("{}/", self.endpoint),
            ] {
                assert!(head.windows(value.len()).any(|part| part == value.as_bytes()));
            }
        }
    }

    fn response_body(&self, stage: Stage, defect: &str) -> &str {
        match stage {
            Stage::Snapshot => self.snapshot,
            Stage::Capability => self.capability,
            Stage::Token if defect == "token" => r#"{"token":""}"#,
            Stage::Token => r#"{"token":"csrf-test-value"}"#,
            Stage::Submission => self.answer,
        }
    }

    async fn write_response(&self, socket: &mut dyn PeerStream, stage: Stage, defect: &str) {
        let body = self.response_body(stage, defect);
        let media = if stage != Stage::Submission { "application/json" } else { self.media };
        let mut block =
            if stage != Stage::Submission { vec![0x88] } else { vec![0x08, 3, b'2', b'0', b'2'] };
        for (name, value) in
            [("content-type", media.to_owned()), ("content-length", body.len().to_string())]
        {
            block.extend_from_slice(&[0, name.len() as u8]);
            block.extend_from_slice(name.as_bytes());
            block.push(value.len() as u8);
            block.extend_from_slice(value.as_bytes());
        }
        let bytes = if stage == Stage::Submission && defect == "truncated" {
            &body.as_bytes()[..body.len() - 1]
        } else {
            body.as_bytes()
        };
        for (kind, flags, bytes) in [(1, 4, block.as_slice()), (0, 1, bytes)] {
            let length = (bytes.len() as u32).to_be_bytes();
            socket
                .write_all(&[length[1], length[2], length[3], kind, flags, 0, 0, 0, 1])
                .await
                .unwrap();
            socket.write_all(bytes).await.unwrap();
        }
    }
}

async fn read_head(socket: &mut dyn PeerStream, stage: Stage) -> Vec<u8> {
    let mut preface = [0; PREFACE_AND_SETTINGS_BYTES];
    socket.read_exact(&mut preface).await.unwrap();
    assert_eq!(&preface[..24], b"PRI * HTTP/2.0\r\n\r\nSM\r\n\r\n");
    socket.write_all(&[0, 0, 0, 4, 0, 0, 0, 0, 0]).await.unwrap();
    let mut frame = [0; FRAME_HEADER_BYTES];
    socket.read_exact(&mut frame).await.unwrap();
    assert_eq!(frame, [0, 0, 0, 4, 1, 0, 0, 0, 0]);
    socket.write_all(&frame).await.unwrap();
    socket.read_exact(&mut frame).await.unwrap();
    assert_eq!(
        (frame[3], frame[4]),
        (
            1,
            if stage == Stage::Submission {
                END_HEADERS_FLAG
            } else {
                END_HEADERS_AND_STREAM_FLAGS
            }
        )
    );
    let length = frame_length(&frame);
    assert!(length <= DEFAULT_FRAME_BYTES);
    let mut head = vec![0; length];
    socket.read_exact(&mut head).await.unwrap();
    head
}

async fn read_body(socket: &mut dyn PeerStream) -> Vec<u8> {
    let mut body = Vec::new();
    let mut frame = [0; FRAME_HEADER_BYTES];
    loop {
        socket.read_exact(&mut frame).await.unwrap();
        assert_eq!(frame[3], 0);
        let length = frame_length(&frame);
        assert!(length <= DEFAULT_FRAME_BYTES);
        let start = body.len();
        body.resize(start + length, 0);
        socket.read_exact(&mut body[start..]).await.unwrap();
        if frame[4] & END_STREAM_FLAG != 0 {
            break;
        }
    }
    body
}

fn frame_length(frame: &[u8; FRAME_HEADER_BYTES]) -> usize {
    usize::from(frame[0]) << LENGTH_HIGH_SHIFT
        | usize::from(frame[1]) << LENGTH_MIDDLE_SHIFT
        | usize::from(frame[2])
}

pub(super) async fn exercise_case(
    listener: &tokio::net::TcpListener,
    authentication: &RequestAuthentication,
    automatic: bool,
    endpoint: &str,
    submission: &Submission,
    media: &str,
    answer: &str,
    snapshot: &str,
    capability: &str,
    transport: &SelectedAuthorTransport,
    identity: &ExecutionIdentity,
    accepted: bool,
) {
    use slingshot_agent_connection::command_submission::SubmissionOutcome;
    use slingshot_agent_connection::selected_author_submission::SubmissionSendRefusal;
    use tokio::time::{Duration, timeout};
    for defect in ["", "token", "guard", "truncated"] {
        let peer_case = Peer {
            listener,
            authentication,
            automatic,
            endpoint,
            submission,
            media,
            answer,
            snapshot,
            capability,
        };
        let peer = peer_case.serve(defect);
        let mut guarded = false;
        let (outcome, ()) = timeout(Duration::from_secs(EXCHANGE_TIMEOUT_SECONDS), async {
            tokio::join!(async {
                if automatic { transport.discover_capabilities_negotiated(identity, "query_paths", Some(EVENT_GENERATION), authentication).await.unwrap(); }
                let guard = || { guarded = true; if defect == "guard" { Err(SubmissionSendRefusal::Identity) } else { Ok(()) } };
                let outcome = if automatic { transport.send_submission_with_fresh_token_negotiated_guarded(identity, submission, authentication, 1, guard).await } else { transport.send_submission_with_fresh_token_http2_guarded(identity, submission, authentication, 1, guard).await };
                if automatic && defect.is_empty() {
                    let slingshot_agent_connection::selected_author_lookup::OperationLookupReceipt::Found(receipt) = transport.lookup_operation_negotiated(identity, submission, authentication).await.unwrap() else { panic!("negotiated snapshot missing") };
                    assert_eq!(receipt.snapshot.progress, 10);
                    assert_eq!(receipt.snapshot.sequence.value(), 2);
                    assert_eq!(receipt.snapshot.physical_sling_job_identifiers, ["job-one"]);
                    assert!(receipt.remaining_retention_milliseconds > 0 && receipt.remaining_retention_milliseconds <= 120000);
                }
                outcome
            }, peer)
        }).await.unwrap();
        assert_eq!(guarded, defect != "token");
        match defect {
            "guard" => assert_eq!(outcome, Err(SubmissionSendRefusal::Identity)),
            "token" => assert_eq!(outcome, Err(SubmissionSendRefusal::Request)),
            _ => {
                let outcome = outcome.unwrap();
                assert_eq!(outcome.provably_recorded(), accepted && defect.is_empty());
                if defect == "truncated" {
                    assert!(matches!(outcome, SubmissionOutcome::SubmissionUnknown { .. }));
                }
            }
        }
        assert!(
            timeout(Duration::from_millis(NO_REPEAT_OBSERVATION_MILLISECONDS), listener.accept())
                .await
                .is_err(),
            "HTTP/2 submission retried or fell back"
        );
    }
}

#[test]
fn stage_selection_preserves_refusal_and_recovery_sequences() {
    use Stage::{Capability, Snapshot, Submission, Token};
    for (automatic, defect, expected) in [
        (false, "", vec![Token, Submission]),
        (false, "token", vec![Token]),
        (false, "guard", vec![Token]),
        (false, "truncated", vec![Token, Submission]),
        (true, "", vec![Capability, Token, Submission, Snapshot]),
        (true, "token", vec![Capability, Token]),
        (true, "guard", vec![Capability, Token]),
        (true, "truncated", vec![Capability, Token, Submission]),
    ] {
        assert!(Peer::stages(automatic, defect) == expected);
    }
}
