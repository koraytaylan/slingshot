//! The five physical lookup modes used by the submission wire matrix.

use super::*;
use slingshot_agent_connection::command_submission::Submission;
use slingshot_agent_connection::selected_author_lookup::{
    PhysicalLookupReceipt, SnapshotLookupReceipt, SnapshotLookupRefusal,
};

/// The independently observed physical-job generation, distinct from the operation's.
pub(super) const GENERATION: u64 = 8;

/// Every physical lookup route, in the wire matrix's original order.
pub(super) const MODES: [Mode; 5] =
    [Mode::Direct, Mode::HttpTwo, Mode::Negotiated, Mode::Authenticated, Mode::AuthenticatedAsync];

/// How one physical lookup obtains authentication and selects its wire protocol.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum Mode {
    /// Plain HTTP/1 with the retained authentication.
    Direct,
    /// Strict HTTP/2 without fallback.
    HttpTwo,
    /// Transport protocol negotiated by the selected author.
    Negotiated,
    /// Authentication acquired through the synchronous provider.
    Authenticated,
    /// Authentication acquired through the asynchronous provider.
    AuthenticatedAsync,
}

impl Mode {
    /// Dispatches the same physical snapshot lookup for one matrix mode.
    ///
    /// # Errors
    /// Returns the selected transport's refusal unchanged.
    pub(super) async fn snapshot(
        self,
        fixture: &preflight::Preflight<'_>,
        identity: &ExecutionIdentity,
        submission: &Submission,
        identifier: &str,
    ) -> Result<SnapshotLookupReceipt, SnapshotLookupRefusal> {
        let preflight::Preflight {
            transport,
            authentication,
            async_provider,
            provider,
            source,
            ..
        } = *fixture;
        match self {
            Self::Direct => {
                transport
                    .lookup_physical_snapshot(identity, submission, identifier, authentication)
                    .await
            }
            Self::HttpTwo => {
                transport
                    .lookup_physical_snapshot_http2(
                        identity,
                        submission,
                        identifier,
                        authentication,
                    )
                    .await
            }
            Self::Negotiated => {
                transport
                    .lookup_physical_snapshot_negotiated(
                        identity,
                        submission,
                        identifier,
                        authentication,
                    )
                    .await
            }
            Self::Authenticated => {
                transport
                    .lookup_physical_snapshot_authenticated(
                        identity, submission, identifier, provider, source, READING,
                    )
                    .await
            }
            Self::AuthenticatedAsync => {
                transport
                    .lookup_physical_snapshot_authenticated_async(
                        identity,
                        submission,
                        identifier,
                        async_provider,
                        &async_cases::NoClocks,
                        &async_cases::NoClocks,
                    )
                    .await
            }
        }
    }
    /// Dispatches the same physical job lookup for one matrix mode.
    ///
    /// # Errors
    /// Returns the selected transport's refusal unchanged.
    pub(super) async fn job(
        self,
        fixture: &preflight::Preflight<'_>,
        identity: &ExecutionIdentity,
        submission: &Submission,
        identifier: &str,
        generation: u64,
    ) -> Result<PhysicalLookupReceipt, SnapshotLookupRefusal> {
        let preflight::Preflight {
            transport,
            authentication,
            async_provider,
            provider,
            source,
            ..
        } = *fixture;
        match self {
            Self::Direct => {
                transport
                    .lookup_physical_job(
                        identity,
                        submission,
                        identifier,
                        generation,
                        authentication,
                    )
                    .await
            }
            Self::HttpTwo => {
                transport
                    .lookup_physical_job_http2(
                        identity,
                        submission,
                        identifier,
                        generation,
                        authentication,
                    )
                    .await
            }
            Self::Negotiated => {
                transport
                    .lookup_physical_job_negotiated(
                        identity,
                        submission,
                        identifier,
                        generation,
                        authentication,
                    )
                    .await
            }
            Self::Authenticated => {
                transport
                    .lookup_physical_job_authenticated(
                        identity, submission, identifier, generation, provider, source, READING,
                    )
                    .await
            }
            Self::AuthenticatedAsync => {
                transport
                    .lookup_physical_job_authenticated_async(
                        identity,
                        submission,
                        identifier,
                        generation,
                        async_provider,
                        &async_cases::NoClocks,
                        &async_cases::NoClocks,
                    )
                    .await
            }
        }
    }
}

const MISSING_STATUS: u16 = 404;
const RETIRED_STATUS: u16 = 410;
const HTTP_TWO_PREFACE_AND_SETTINGS_BYTES: usize = 39;
const HTTP_TWO_FRAME_HEADER_BYTES: usize = 9;
const LENGTH_HIGH_SHIFT: usize = 16;
const LENGTH_MIDDLE_SHIFT: usize = 8;
const STATUS_DIGITS: u8 = 3;

/// Builds the original success, logical-absence and physical-absence response cases.
pub(super) fn response_body(
    submission: &Submission,
    snapshot_body: &str,
    status: u16,
    defect: &str,
) -> Vec<u8> {
    let mut body: serde_json::Value = serde_json::from_str(snapshot_body).unwrap();
    body["physical_sling_job_identifiers"] =
        serde_json::json!([if defect == "physical" { "other-job" } else { "job /?é" }]);
    if defect == "digest" {
        body["submitted_command_digest"] = serde_json::json!("0".repeat(64));
    }
    if status == MISSING_STATUS {
        body = serde_json::json!({ "kind":"missing", "format":"slingshot.agent/1", "transport_contract_digest":submission.provenance.transport_contract_digest,
            "agent_event_store_generation":submission.operation.agent_event_store_generation, "agent_operation_identifier":submission.operation.agent_operation_identifier,
            "author_target_identity_digest":submission.operation.author_target_identity_digest });
        if defect.starts_with("missing") {
            body = serde_json::json!({"kind":"missing", "format":"slingshot.agent/1", "transport_contract_digest":submission.provenance.transport_contract_digest,
                "agent_event_store_generation":if defect == "missing-generation" {GENERATION + 1} else {GENERATION},
                "sling_job_identifier":if defect == "missing-identifier" {"other-job"} else {"job /?é"}});
        }
    } else if status == RETIRED_STATUS {
        body = serde_json::json!({ "kind":"retired", "provenance":submission.provenance,
            "agent_event_store_generation":submission.operation.agent_event_store_generation, "agent_operation_identifier":submission.operation.agent_operation_identifier,
            "author_target_identity_digest":submission.operation.author_target_identity_digest, "selected_environment_revision":submission.operation.selected_environment_revision,
            "daemon_subscription_identifier":submission.daemon_subscription_identifier, "submitted_command_digest":submission.submitted_command_digest });
    }
    serde_json::to_vec(&body).unwrap()
}

/// Serves one physical lookup with the matrix's original framing and truncation checks.
pub(super) async fn serve(
    listener: &tokio::net::TcpListener,
    authentication: &slingshot_agent_connection::authentication::environment_provider::RequestAuthentication,
    http_two: bool,
    status: u16,
    defect: &str,
    body: &[u8],
) {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    let (mut socket, _) = listener.accept().await.unwrap();
    let mut request = Vec::new();
    if http_two {
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
        request.resize(length, 0);
        socket.read_exact(&mut request).await.unwrap();
    } else {
        while !request.ends_with(b"\r\n\r\n") {
            request.push(socket.read_u8().await.unwrap());
            assert!(request.len() <= 8192);
        }
        assert!(request.starts_with(b"GET "));
    }
    let route = b"/aem/bin/slingshot/agent/jobs?sling_job_identifier=job%20%2F%3F%C3%A9";
    assert!(request.windows(route.len()).any(|bytes| bytes == route));
    authentication.lend_value_bytes(|value| {
        assert!(request.windows(value.len()).any(|bytes| bytes == value))
    });
    let payload = if defect.ends_with("truncated") { &body[..body.len() - 1] } else { body };
    if http_two {
        let mut block = vec![0, 7];
        block.extend_from_slice(b":status");
        block.push(STATUS_DIGITS);
        block.extend_from_slice(status.to_string().as_bytes());
        for (name, value) in [
            ("content-type", "application/json".to_owned()),
            ("content-length", body.len().to_string()),
        ] {
            block.extend_from_slice(&[0, name.len() as u8]);
            block.extend_from_slice(name.as_bytes());
            block.push(value.len() as u8);
            block.extend_from_slice(value.as_bytes());
        }
        for (kind, flags, bytes) in [(1, 4, block.as_slice()), (0, 1, payload)] {
            let length = (bytes.len() as u32).to_be_bytes();
            socket
                .write_all(&[length[1], length[2], length[3], kind, flags, 0, 0, 0, 1])
                .await
                .unwrap();
            socket.write_all(bytes).await.unwrap();
        }
        let mut close = Vec::new();
        let _ = socket.read_to_end(&mut close).await;
    } else {
        socket.write_all(format!("HTTP/1.1 {status} Snapshot\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n", body.len()).as_bytes()).await.unwrap();
        socket.write_all(payload).await.unwrap();
    }
    socket.shutdown().await.unwrap();
}

const EXCHANGE_TIMEOUT_SECONDS: u64 = 5;
const REFUSAL_OBSERVATION_MILLISECONDS: u64 = 10;

impl preflight::Preflight<'_> {
    /// Checks physical preflight refusals and every existing status/defect wire case.
    pub(super) async fn check_physical(&self, submission: &Submission, snapshot_body: &str) {
        use slingshot_domain::author_agent_transport_contract::AuthorAgentTransportContract;
        use tokio::time::{Duration, timeout};
        let Self { listener, identity, authentication, .. } = *self;
        let identifier_bound = AuthorAgentTransportContract::embedded()
            .limit("maximum_sling_job_identifier_bytes") as usize;
        for mode in MODES {
            let http2 = mode == Mode::HttpTwo;
            for invalid in [String::new(), "x".repeat(identifier_bound + 1)] {
                let result = mode.snapshot(self, identity, submission, &invalid).await;
                assert!(result.is_err());
            }
            let mut moved = (*identity).clone();
            moved.selected_environment_revision = "other".into();
            let refused = mode.snapshot(self, &moved, submission, "job /?é").await;
            assert!(refused.is_err());
            let zero_generation = mode.job(self, identity, submission, "job /?é", 0).await;
            assert!(zero_generation.is_err());
            assert!(
                timeout(Duration::from_millis(REFUSAL_OBSERVATION_MILLISECONDS), listener.accept())
                    .await
                    .is_err()
            );
            for (status, defect) in [
                (200, ""),
                (200, "physical"),
                (200, "digest"),
                (200, "truncated"),
                (404, "logical"),
                (410, "logical"),
                (404, "missing"),
                (404, "missing-generation"),
                (404, "missing-identifier"),
                (404, "missing-truncated"),
            ] {
                let body = response_body(submission, snapshot_body, status, defect);
                let peer = serve(listener, authentication, http2, status, defect, &body);
                let request = mode.job(self, identity, submission, "job /?é", GENERATION);
                let (result, ()) = timeout(Duration::from_secs(EXCHANGE_TIMEOUT_SECONDS), async {
                    tokio::join!(request, peer)
                })
                .await
                .unwrap();
                assert_eq!(
                    result.is_ok(),
                    defect.is_empty() || defect == "missing",
                    "http2={http2} status={status} defect={defect}"
                );
                if let Ok(receipt) = result {
                    match receipt {
                        slingshot_agent_connection::selected_author_lookup::PhysicalLookupReceipt::Found(receipt) => {
                            assert_eq!(status, 200);
                            assert_eq!(receipt.snapshot.physical_sling_job_identifiers, ["job /?é"]);
                            assert_eq!(receipt.snapshot.echo.agent_event_store_generation, submission.operation.agent_event_store_generation);
                            assert!(receipt.remaining_retention_milliseconds > 0 && receipt.remaining_retention_milliseconds <= 120000);
                        },
                        slingshot_agent_connection::selected_author_lookup::PhysicalLookupReceipt::Missing(proof) => {
                            assert_eq!(defect, "missing"); assert_eq!(proof.generation(), GENERATION); assert_eq!(proof.sling_job_identifier(), "job /?é");
                        },
                    }
                }
            }
        }
    }
}
