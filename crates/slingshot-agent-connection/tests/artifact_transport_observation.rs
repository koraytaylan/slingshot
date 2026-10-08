//! Exact phase outcomes in a dedicated process, without concurrent exchanges.

use sha2::{Digest, Sha256};
use slingshot_agent_connection::{
    artifact_download::ExpectedArtifact,
    authentication::{
        access_token_cache::AccessTokenSource,
        environment_provider::{EnvironmentAuthenticationProvider, RequestAuthentication},
        identity_management_exchange::{AccessToken, ExchangeFailure},
        runtime_snapshot::build_runtime_snapshot,
    },
    author_hypertext_transfer_protocol_policy::ExchangeDeadlines,
    capability_timing_observation::snapshot as capability_snapshot,
    command_submission::{ExpectedArtifactManifest, Submission},
    selected_author_http::{ArtifactHttpOutcome, FiniteHttpFailure},
    selected_author_transport::SelectedAuthorTransport,
    submission_timing_observation::snapshot as submission_snapshot,
    transport_observation::{Phase, PhaseSnapshot, Route, Snapshot},
};
use slingshot_agent_protocol::{
    identity::WireOperationIdentity, wire_contract::ExpectedProvenance,
};
use slingshot_configuration::{
    additional_certificate_authority::AdditionalAuthorCertificates,
    platform_trust::{PlatformTrustSource, ProviderDecision, ProviderRecord},
    profile_loader::{ConfigurationDiagnostic, load_profiles},
    profile_selection::RequestedSelection,
    testing::credential_filesystem::ScriptedFilesystem,
};
use slingshot_domain::{
    agent_identity::AgentEventStoreGeneration,
    author_agent_transport_contract::AuthorAgentTransportContract,
    command::schema::canonical_contract_digest,
    operation_executor::ExecutionIdentity,
    profile::{EnvironmentName, ProfileName},
    selected_command_contract_identity::SelectedCommandContractIdentity,
};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::TcpListener,
    time::Duration,
};

const GENERATION: u64 = 7;
const CAPABILITY_TIMING: &str = "cap_shape;dur=3,cap_identity;dur=17,cap_document;dur=5";
const SUBMISSION_TIMING: &str = "admission;dur=3,execution;dur=17,persistence;dur=5";
const SUBMISSION_STATUS: u16 = 202;
const OUTCOME_COUNT: usize = 4;
const SUCCESS_STATUS: u16 = 200;
const MILLISECONDS_PER_SECOND: u64 = 1000;
const ARTIFACT_LENGTH: u64 = 3;
const DIGEST_CHARACTERS: usize = 64;
const TEST_TIMEOUT_SECONDS: u64 = 5;

struct Platform;
impl PlatformTrustSource for Platform {
    fn records(&self) -> Result<Vec<ProviderRecord>, ConfigurationDiagnostic> {
        Ok(AdditionalAuthorCertificates::parse(include_bytes!(
            "../../slingshot-test-support/fixtures/additional-certificate-authority/one-authority.pem"
        )).unwrap().certificates().iter().map(|der| ProviderRecord {
            der: der.clone(),
            decision: ProviderDecision::UnconditionallyTrustedForServerAuthentication,
        }).collect())
    }
}

struct NoToken;
impl AccessTokenSource for NoToken {
    fn exchange(&self) -> Result<AccessToken, ExchangeFailure> {
        panic!("Basic authentication must not exchange a token")
    }
}

fn provider(endpoint: &str) -> EnvironmentAuthenticationProvider {
    let profile = include_str!(
        "../../slingshot-test-support/fixtures/profile-directories/ordered/profiles/mike.toml"
    )
    .replace("http://author.example.com", endpoint)
    .replace("allow_insecure_author_transport = true\n", "");
    let inventory = format!(
        "format_version = 1\n[[sources]]\nreference = \"profiles/test.toml\"\nsha256 = \"{}\"\n",
        digest(profile.as_bytes())
    );
    let loaded = load_profiles(
        ScriptedFilesystem::new()
            .with_directory("profiles")
            .with_source("profiles/test.toml", profile.as_bytes())
            .with_source("configuration-snapshot.toml", inventory.as_bytes()),
    )
    .unwrap();
    let requested = RequestedSelection {
        profile: Some(ProfileName::parse("remote-site").unwrap()),
        environment: Some(EnvironmentName::parse("staging").unwrap()),
    };
    EnvironmentAuthenticationProvider::new(
        build_runtime_snapshot(loaded, &requested, &Platform).unwrap(),
        1,
    )
}

fn digest(bytes: &[u8]) -> String {
    Sha256::digest(bytes).iter().map(|byte| format!("{byte:02x}")).collect()
}

#[derive(Clone, Copy, Debug)]
enum End {
    Success,
    HeadFailure,
    BodyFailure,
    CancelHead,
    CancelBody,
    TimeoutHead,
    TimeoutBody,
}

struct Case {
    status: u16,
    defect: &'static str,
    end: End,
}

fn wire(case: &Case, submission: &Submission, identifier: &str) -> Vec<u8> {
    let body = if case.status == SUCCESS_STATUS {
        if case.defect == "digest" { "abd".to_owned() } else { "abc".to_owned() }
    } else {
        serde_json::json!({
            "provenance": submission.provenance,
            "agent_event_store_generation": GENERATION,
            "agent_operation_identifier": submission.operation.agent_operation_identifier,
            "artifact_identifier": if case.defect == "identity" { "wrong" } else { identifier },
            "artifact_slot": "content_package", "reason": "missing",
        })
        .to_string()
    };
    let media = if case.status == SUCCESS_STATUS { "application/zip" } else { "application/json" };
    if matches!(case.end, End::CancelHead | End::TimeoutHead) {
        return Vec::new();
    }
    let length = if case.defect == "length" { body.len() + 1 } else { body.len() };
    let framing = if case.defect == "chunked" {
        "Transfer-Encoding: chunked".to_owned()
    } else {
        format!("Content-Length: {length}")
    };
    let payload = wire_payload(case, &body);
    let timing =
        if case.status == SUBMISSION_STATUS { SUBMISSION_TIMING } else { CAPABILITY_TIMING };
    format!("HTTP/1.1 {} Test\r\nContent-Type: {media}\r\n{framing}\r\nServer-Timing: {timing}\r\n\r\n{payload}", case.status)
        .into_bytes()
}

fn wire_payload(case: &Case, body: &str) -> String {
    if matches!(case.end, End::CancelBody | End::TimeoutBody) {
        return String::new();
    }
    match case.defect {
        "chunked" => format!("{:x}\r\n{body}\r\n0\r\n\r\n", body.len()),
        "short" => body[..body.len() - 1].to_owned(),
        "extra" => format!("{body}!"),
        _ => body.to_owned(),
    }
}

fn phase(snapshot: &Snapshot, phase: Phase) -> PhaseSnapshot {
    snapshot.phases[phase as usize]
}

fn verify_phase(
    before: &Snapshot,
    after: &Snapshot,
    phase_name: Phase,
    expected: [u64; OUTCOME_COUNT],
) {
    let previous = phase(before, phase_name);
    let current = phase(after, phase_name);
    assert_eq!(
        [
            current.started - previous.started,
            current.succeeded - previous.succeeded,
            current.failed - previous.failed,
            current.abandoned - previous.abandoned,
        ],
        expected,
        "phase={}",
        after.phase_names[phase_name as usize]
    );
    if expected[0] != 0 {
        assert!(current.elapsed_nanoseconds > previous.elapsed_nanoseconds);
    }
}

async fn control(end: End, before: &Snapshot) {
    let target = match end {
        End::CancelHead | End::TimeoutHead => Phase::ResponseHead,
        End::CancelBody | End::TimeoutBody => Phase::ResponseBody,
        _ => return std::future::pending().await,
    };
    while phase(&Route::Author.snapshot(), target).started == phase(before, target).started {
        tokio::task::yield_now().await;
    }
    match end {
        End::TimeoutHead => {
            tokio::time::pause();
            tokio::time::advance(Duration::from_millis(
                ExchangeDeadlines::embedded().response_header_milliseconds + 1,
            ))
            .await;
            std::future::pending::<()>().await;
        }
        End::TimeoutBody => {
            tokio::time::pause();
            tokio::time::advance(Duration::from_millis(
                AuthorAgentTransportContract::embedded()
                    .limit("artifact_transfer_idle_timeout_milliseconds")
                    + 1,
            ))
            .await;
            std::future::pending::<()>().await;
        }
        _ => {}
    }
}

struct Invocation<'held> {
    transport: &'held SelectedAuthorTransport,
    identity: &'held ExecutionIdentity,
    submission: &'held Submission,
    artifact: &'held ExpectedArtifact,
    identifier: &'held str,
    authentication: &'held RequestAuthentication,
    negotiated: bool,
}

async fn exchange(case: &Case, invocation: &Invocation<'_>, listener: &TcpListener) {
    let before = Route::Author.snapshot();
    let numeric_before = (capability_snapshot(), submission_snapshot());
    let response = wire(case, invocation.submission, invocation.identifier);
    let peer = async {
        let (mut socket, _) = listener.accept().await.unwrap();
        let mut request = Vec::new();
        while !request.ends_with(b"\r\n\r\n") {
            request.push(socket.read_u8().await.unwrap());
        }
        assert!(request.starts_with(b"GET /aem/bin/slingshot/agent/artifact?"));
        socket.write_all(&response).await.unwrap();
        if matches!(
            case.end,
            End::CancelHead | End::CancelBody | End::TimeoutHead | End::TimeoutBody
        ) {
            assert_eq!(
                socket.read_u8().await.unwrap_err().kind(),
                std::io::ErrorKind::UnexpectedEof
            );
        } else {
            socket.shutdown().await.unwrap();
        }
    };
    let mut staged = Vec::new();
    let request = async {
        let sink = |bytes: &[u8]| {
            assert_eq!(case.status, SUCCESS_STATUS, "refusal document entered staging");
            if case.defect == "sink" {
                return Err(FiniteHttpFailure::Body);
            }
            staged.extend_from_slice(bytes);
            Ok(())
        };
        let transfer = async {
            if invocation.negotiated {
                invocation
                    .transport
                    .artifact_negotiated(
                        invocation.identity,
                        invocation.submission,
                        invocation.artifact,
                        invocation.identifier,
                        invocation.authentication,
                        sink,
                    )
                    .await
            } else {
                invocation
                    .transport
                    .artifact_http1(
                        invocation.identity,
                        invocation.submission,
                        invocation.artifact,
                        invocation.identifier,
                        invocation.authentication,
                        sink,
                    )
                    .await
            }
        };
        tokio::select! {
            result = transfer => Some(result),
            () = control(case.end, &before) => None,
        }
    };
    let (result, ()) = tokio::time::timeout(
        Duration::from_millis(
            ExchangeDeadlines::embedded().response_header_milliseconds
                + AuthorAgentTransportContract::embedded()
                    .limit("artifact_transfer_total_timeout_milliseconds")
                + TEST_TIMEOUT_SECONDS * MILLISECONDS_PER_SECOND,
        ),
        async { tokio::join!(request, peer) },
    )
    .await
    .unwrap();
    if matches!(case.end, End::TimeoutHead | End::TimeoutBody) {
        tokio::time::resume();
    }
    verify_outcome(case, result, &staged);
    let after = Route::Author.snapshot();
    verify_phase(&before, &after, Phase::Request, [1, 1, 0, 0]);
    let (head, body, exchange) = match case.end {
        End::Success => ([1, 1, 0, 0], [1, 1, 0, 0], [1, 1, 0, 0]),
        End::HeadFailure | End::TimeoutHead => ([1, 0, 1, 0], [0; OUTCOME_COUNT], [1, 0, 1, 0]),
        End::BodyFailure | End::TimeoutBody => ([1, 1, 0, 0], [1, 0, 1, 0], [1, 0, 1, 0]),
        End::CancelHead => ([1, 0, 0, 1], [0; OUTCOME_COUNT], [1, 0, 0, 1]),
        End::CancelBody => ([1, 1, 0, 0], [1, 0, 0, 1], [1, 0, 0, 1]),
    };
    verify_phase(&before, &after, Phase::ResponseHead, head);
    verify_phase(&before, &after, Phase::ResponseBody, body);
    verify_phase(&before, &after, Phase::Exchange, exchange);
    assert_eq!(after.opened_sockets - before.opened_sockets, 1);
    assert_eq!(after.dropped_sockets - before.dropped_sockets, 1);
    assert_eq!(
        (capability_snapshot(), submission_snapshot()),
        numeric_before,
        "artifact streaming must not publish finite-response timing before body proof"
    );
}

fn verify_outcome(
    case: &Case,
    result: Option<Result<ArtifactHttpOutcome, FiniteHttpFailure>>,
    staged: &[u8],
) {
    match case.end {
        End::Success => match result.unwrap().unwrap() {
            ArtifactHttpOutcome::Transferred(receipt) => {
                assert_eq!(receipt.byte_length(), ARTIFACT_LENGTH);
                assert_eq!(staged, b"abc");
            }
            ArtifactHttpOutcome::Unavailable { .. } | ArtifactHttpOutcome::Unauthorized => {
                assert!(staged.is_empty())
            }
        },
        End::HeadFailure | End::TimeoutHead => {
            assert!(matches!(result, Some(Err(FiniteHttpFailure::Head))))
        }
        End::BodyFailure | End::TimeoutBody => {
            assert!(matches!(result, Some(Err(FiniteHttpFailure::Body))))
        }
        End::CancelHead | End::CancelBody => assert!(result.is_none()),
    }
}

#[tokio::test]
async fn http_one_artifact_phases_cover_integrity_refusals_deadlines_and_cancellation() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let endpoint = format!("http://{}/aem", listener.local_addr().unwrap());
    let provider = provider(&endpoint);
    let (authentication, _) = provider.authenticate(&endpoint, 0, &NoToken).unwrap();
    let transport = SelectedAuthorTransport::new(provider.snapshot().author_connection()).unwrap();
    let identity = ExecutionIdentity {
        attempt: 1,
        operation_identifier: "artifact-observation".to_owned(),
        author_target_identity_digest: provider.snapshot().target().to_string(),
        selected_environment_revision: provider.snapshot().revision().to_string(),
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
            AgentEventStoreGeneration::of(GENERATION),
        ),
        "subscription-one",
        r#"{"root_path":"/content"}"#,
        ExpectedArtifactManifest::empty(),
    )
    .unwrap();
    let artifact = ExpectedArtifact {
        artifact_digest: digest(b"abc"),
        artifact_slot: "content_package".to_owned(),
        byte_length: ARTIFACT_LENGTH,
        media_type: "application/zip".to_owned(),
    };
    let identifier = "3".repeat(DIGEST_CHARACTERS);
    for negotiated in [false, true] {
        let invocation = Invocation {
            transport: &transport,
            identity: &identity,
            submission: &submission,
            artifact: &artifact,
            identifier: &identifier,
            authentication: &authentication,
            negotiated,
        };
        for (status, defect, end) in [
            (200, "", End::Success),
            (200, "chunked", End::Success),
            (200, "digest", End::BodyFailure),
            (200, "sink", End::BodyFailure),
            (200, "short", End::BodyFailure),
            (200, "extra", End::BodyFailure),
            (200, "length", End::HeadFailure),
            (404, "", End::Success),
            (401, "", End::Success),
            (404, "identity", End::BodyFailure),
            (404, "short", End::BodyFailure),
            (200, "", End::CancelHead),
            (200, "", End::CancelBody),
            (200, "", End::TimeoutHead),
            (200, "", End::TimeoutBody),
            (SUBMISSION_STATUS, "", End::HeadFailure),
        ] {
            exchange(&Case { status, defect, end }, &invocation, &listener).await;
        }
    }
}
