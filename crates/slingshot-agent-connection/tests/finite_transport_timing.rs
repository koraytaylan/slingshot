//! Real socket exchanges publish numeric observations only for complete finite responses.

use sha2::{Digest, Sha256};
use slingshot_agent_connection::{
    authentication::{
        access_token_cache::AccessTokenSource,
        environment_provider::EnvironmentAuthenticationProvider,
        identity_management_exchange::{AccessToken, ExchangeFailure},
        runtime_snapshot::build_runtime_snapshot,
    },
    author_hypertext_transfer_protocol_policy::ExchangeDeadlines,
    capability_timing_observation::{
        Snapshot as CapabilitySnapshot, snapshot as capability_snapshot,
    },
    selected_author_transport::SelectedAuthorTransport,
    submission_timing_observation::{
        Snapshot as SubmissionSnapshot, snapshot as submission_snapshot,
    },
};
use slingshot_configuration::{
    additional_certificate_authority::AdditionalAuthorCertificates,
    platform_trust::{PlatformTrustSource, ProviderDecision, ProviderRecord},
    profile_loader::{ConfigurationDiagnostic, load_profiles},
    profile_selection::RequestedSelection,
    testing::credential_filesystem::ScriptedFilesystem,
};
use slingshot_domain::profile::{EnvironmentName, ProfileName};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::{TcpListener, TcpStream},
    time::{Duration, timeout},
};

const CAPABILITY_TIMING: &str = "cap_shape;dur=3,cap_identity;dur=17,cap_document;dur=5";
const SUBMISSION_TIMING: &str = "admission;dur=3,execution;dur=17,persistence;dur=5";
const DURATIONS: [u64; 3] = [3, 17, 5];
const COMPLETE_BODY: &[u8] = b"{}";
const CACHE_IDENTITY: u64 = 1;
const AUTHENTICATION_READING: u64 = 0;
const PREPARATION_BYTES: usize = 39;
const FRAME_HEADER_BYTES: usize = 9;
const END_HEADERS_FLAG: u8 = 4;
const END_STREAM_FLAG: u8 = 1;

struct Platform;

impl PlatformTrustSource for Platform {
    fn records(&self) -> Result<Vec<ProviderRecord>, ConfigurationDiagnostic> {
        Ok(AdditionalAuthorCertificates::parse(include_bytes!(
            "../../slingshot-test-support/fixtures/additional-certificate-authority/one-authority.pem"
        ))
        .unwrap()
        .certificates()
        .iter()
        .map(|certificate| ProviderRecord {
            der: certificate.clone(),
            decision: ProviderDecision::UnconditionallyTrustedForServerAuthentication,
        })
        .collect())
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
    let digest: String =
        Sha256::digest(profile.as_bytes()).iter().map(|byte| format!("{byte:02x}")).collect();
    let inventory = format!(
        "format_version = 1\n[[sources]]\nreference = \"profiles/test.toml\"\nsha256 = \"{digest}\"\n"
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
        CACHE_IDENTITY,
    )
}

#[derive(Clone, Copy, Debug)]
enum Protocol {
    HttpOne,
    HttpTwo,
}

#[derive(Clone, Copy, Debug)]
enum ResponseCase {
    Complete,
    AbsentTiming,
    UnusableTiming,
    TruncatedBody,
    SurplusBody,
    RepeatedRetry,
    EncodedBody,
    MissingType,
    Submission,
    TruncatedSubmission,
}

impl ResponseCase {
    fn accepted(self) -> bool {
        matches!(
            self,
            Self::Complete | Self::AbsentTiming | Self::UnusableTiming | Self::Submission
        )
    }

    fn submission(self) -> bool {
        matches!(self, Self::Submission | Self::TruncatedSubmission)
    }

    fn body(self) -> &'static [u8] {
        match self {
            Self::TruncatedBody | Self::TruncatedSubmission => b"{",
            Self::SurplusBody => b"{}!",
            _ => COMPLETE_BODY,
        }
    }

    fn fields(self) -> Vec<(&'static str, String)> {
        let mut fields = vec![("content-length", COMPLETE_BODY.len().to_string())];
        if !matches!(self, Self::MissingType) {
            fields.push(("content-type", "application/json".to_owned()));
        }
        if !matches!(self, Self::AbsentTiming) {
            let value = if self.submission() { SUBMISSION_TIMING } else { CAPABILITY_TIMING };
            fields.push((
                "server-timing",
                if matches!(self, Self::UnusableTiming) { "unusable" } else { value }.to_owned(),
            ));
        }
        if matches!(self, Self::RepeatedRetry) {
            fields.extend([("retry-after", "1".to_owned()), ("retry-after", "1".to_owned())]);
        }
        if matches!(self, Self::EncodedBody) {
            fields.push(("content-encoding", "gzip".to_owned()));
        }
        fields
    }
}

fn frame(kind: u8, flags: u8, payload: &[u8]) -> Vec<u8> {
    let mut bytes = vec![0, 0, 0, kind, flags, 0, 0, 0, 1];
    let length = u32::try_from(payload.len()).unwrap().to_be_bytes();
    bytes[..3].copy_from_slice(&length[1..]);
    bytes.extend_from_slice(payload);
    bytes
}

fn response(protocol: Protocol, case: ResponseCase) -> Vec<u8> {
    let fields = case.fields();
    match protocol {
        Protocol::HttpOne => {
            let mut bytes =
                format!("HTTP/1.1 {} Test\r\n", if case.submission() { "202" } else { "200" })
                    .into_bytes();
            for (name, value) in fields {
                bytes.extend_from_slice(format!("{name}: {value}\r\n").as_bytes());
            }
            bytes.extend_from_slice(b"\r\n");
            bytes.extend_from_slice(case.body());
            bytes
        }
        Protocol::HttpTwo => {
            let mut block =
                if case.submission() { vec![0x08, 3, b'2', b'0', b'2'] } else { vec![0x88] };
            for (name, value) in fields {
                block.extend_from_slice(&[0, u8::try_from(name.len()).unwrap()]);
                block.extend_from_slice(name.as_bytes());
                block.push(u8::try_from(value.len()).unwrap());
                block.extend_from_slice(value.as_bytes());
            }
            let mut bytes = frame(1, END_HEADERS_FLAG, &block);
            bytes.extend(frame(0, END_STREAM_FLAG, case.body()));
            bytes
        }
    }
}

async fn peer(
    listener: &TcpListener,
    protocol: Protocol,
    case: ResponseCase,
    expected_head: &[u8],
) {
    let (mut socket, _) = listener.accept().await.unwrap();
    match protocol {
        Protocol::HttpOne => {
            let mut head = Vec::new();
            while !head.ends_with(b"\r\n\r\n") {
                head.push(socket.read_u8().await.unwrap());
            }
            assert!(head.starts_with(b"GET /bin/slingshot/agent/capabilities HTTP/1.1\r\n"));
            assert!(
                head.windows(b"Authorization: Basic ".len())
                    .any(|part| part == b"Authorization: Basic ")
            );
        }
        Protocol::HttpTwo => prepare_http_two(&mut socket, expected_head).await,
    }
    socket.write_all(&response(protocol, case)).await.unwrap();
    if matches!(protocol, Protocol::HttpTwo) {
        let mut shutdown = Vec::new();
        let _closed = socket.read_to_end(&mut shutdown).await;
    }
}

async fn prepare_http_two(socket: &mut TcpStream, expected_head: &[u8]) {
    let mut preparation = [0; PREPARATION_BYTES];
    socket.read_exact(&mut preparation).await.unwrap();
    assert_eq!(&preparation[..24], b"PRI * HTTP/2.0\r\n\r\nSM\r\n\r\n");
    socket.write_all(&[0, 0, 0, 4, 0, 0, 0, 0, 0]).await.unwrap();
    let mut acknowledgement = [0; FRAME_HEADER_BYTES];
    socket.read_exact(&mut acknowledgement).await.unwrap();
    assert_eq!(acknowledgement, [0, 0, 0, 4, 1, 0, 0, 0, 0]);
    socket.write_all(&acknowledgement).await.unwrap();
    let mut head = vec![0; expected_head.len()];
    socket.read_exact(&mut head).await.unwrap();
    assert_eq!(head, expected_head);
}

fn expected_capability(before: CapabilitySnapshot, case: ResponseCase) -> CapabilitySnapshot {
    if !case.accepted() || case.submission() {
        return before;
    }
    match case {
        ResponseCase::AbsentTiming => {
            CapabilitySnapshot { absent_headers: before.absent_headers + 1, ..before }
        }
        ResponseCase::UnusableTiming => {
            CapabilitySnapshot { rejected_headers: before.rejected_headers + 1, ..before }
        }
        _ => CapabilitySnapshot {
            parsed_headers: before.parsed_headers + 1,
            shape_milliseconds: before.shape_milliseconds + DURATIONS[0],
            identity_milliseconds: before.identity_milliseconds + DURATIONS[1],
            document_milliseconds: before.document_milliseconds + DURATIONS[2],
            ..before
        },
    }
}

fn expected_submission(before: SubmissionSnapshot, case: ResponseCase) -> SubmissionSnapshot {
    if !matches!(case, ResponseCase::Submission) {
        return before;
    }
    SubmissionSnapshot {
        parsed_headers: before.parsed_headers + 1,
        admission_milliseconds: before.admission_milliseconds + DURATIONS[0],
        execution_milliseconds: before.execution_milliseconds + DURATIONS[1],
        persistence_milliseconds: before.persistence_milliseconds + DURATIONS[2],
        ..before
    }
}

#[tokio::test]
async fn real_finite_transports_observe_complete_responses_once_and_refusals_never() {
    let mut observations = Vec::new();
    let mut expected = Vec::new();
    for protocol in [Protocol::HttpOne, Protocol::HttpTwo] {
        for case in [
            ResponseCase::Complete,
            ResponseCase::AbsentTiming,
            ResponseCase::UnusableTiming,
            ResponseCase::TruncatedBody,
            ResponseCase::SurplusBody,
            ResponseCase::RepeatedRetry,
            ResponseCase::EncodedBody,
            ResponseCase::MissingType,
            ResponseCase::Submission,
            ResponseCase::TruncatedSubmission,
        ] {
            let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
            let endpoint = format!("http://{}", listener.local_addr().unwrap());
            let provider = provider(&endpoint);
            let transport =
                SelectedAuthorTransport::new(provider.snapshot().author_connection()).unwrap();
            let (authentication, _) =
                provider.authenticate(&endpoint, AUTHENTICATION_READING, &NoToken).unwrap();
            let fields = http::HeaderMap::new();
            let path = ["bin", "slingshot", "agent", "capabilities"];
            let encoded: Vec<u8> = transport
                .encode_http2_request_head(
                    http::Method::GET,
                    &path,
                    &[],
                    &authentication,
                    &fields,
                    b"",
                )
                .unwrap()
                .frames()
                .flatten()
                .collect();
            let capability_before = capability_snapshot();
            let submission_before = submission_snapshot();
            let request = async {
                match protocol {
                    Protocol::HttpOne => {
                        transport
                            .finite_http1(http::Method::GET, &path, &authentication, &fields, b"")
                            .await
                    }
                    Protocol::HttpTwo => {
                        transport
                            .finite_http2_query(
                                http::Method::GET,
                                &path,
                                &[],
                                &authentication,
                                &fields,
                                b"",
                            )
                            .await
                    }
                }
            };
            let (answer, ()) = timeout(
                Duration::from_millis(ExchangeDeadlines::embedded().finite_total_milliseconds),
                async { tokio::join!(request, peer(&listener, protocol, case, &encoded)) },
            )
            .await
            .unwrap();
            assert_eq!(answer.is_ok(), case.accepted(), "{protocol:?}/{case:?}");
            if let Ok(answer) = answer {
                assert_eq!(answer.response.body, COMPLETE_BODY);
            }
            observations.push((capability_snapshot(), submission_snapshot()));
            expected.push((
                expected_capability(capability_before, case),
                expected_submission(submission_before, case),
            ));
        }
    }
    assert_eq!(observations, expected, "each network response is observed once after body proof");
}
