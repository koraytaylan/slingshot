//! Assertions for the one provider an author request may use.
//!
//! The provider answers for one target. A publisher address, an unrelated
//! origin, and an address that merely begins with the same characters are all
//! refused before any client sees them, and the refusal happens before an
//! exchange is even attempted - which the tests check by counting exchanges,
//! because "it would have failed later" is not the same as "it never asked".
//!
//! The snapshot behind it is assembled once. Nothing here reloads, so a source
//! that changes underneath a running provider changes nothing about it; that is
//! asserted by rebuilding from changed bytes and observing that the live
//! provider is unmoved.

use std::cell::Cell;

#[path = "environment_provider/async_cases.rs"]
mod async_cases;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use slingshot_agent_connection::authentication::access_token_cache::AccessTokenSource;
use slingshot_agent_connection::authentication::cloud_service_credentials::CloudServiceCredentials;
use slingshot_agent_connection::authentication::environment_provider::{
    EnvironmentAuthenticationProvider, SelectedAuthorConnectionRefusal,
    SelectedEnvironmentSnapshot, SnapshotAuthentication, SnapshotMaterial,
};
use slingshot_agent_connection::authentication::identity_management_exchange::{
    AccessToken, DecodedHead, DecodedResponse, ExchangeFailure, IdentityManagementExchange,
    IdentityManagementTransport, MonotonicClock,
};
use slingshot_agent_connection::authentication::token_assertion::{
    CoordinatedUniversalTimeClock, ServiceCredentialAssertion,
};
use slingshot_agent_connection::selected_author_transport::SelectedAuthorTransport;
use slingshot_agent_connection::transport_policy::{
    AuthorTrustInput, IdentityManagementTrustInput,
};
use slingshot_configuration::additional_certificate_authority::AdditionalAuthorCertificates;
use slingshot_configuration::platform_trust::{
    PlatformTrustSnapshot, PlatformTrustSource, ProviderDecision, ProviderRecord,
};
use slingshot_configuration::profile_loader::{
    ConfigurationDiagnostic, LoadedProfiles, load_profiles,
};
use slingshot_configuration::profile_selection::{ProfileSelection, RequestedSelection, resolve};
use slingshot_configuration::testing::credential_filesystem::ScriptedFilesystem;
use slingshot_domain::operation_executor::ExecutionIdentity;
use slingshot_domain::profile::{
    AdobeExperienceManagerDeployment, EnvironmentAuthentication, EnvironmentName, ProfileName,
};
use slingshot_domain::profile_authentication_contract::ConfigurationFailureCode;
use slingshot_domain::secret_value::SensitiveConfigurationDocument;
use slingshot_domain::selected_environment_revision::{
    AuthenticationPrincipalIdentity, AuthorTargetIdentityDigest, CanonicalMetascopeSet,
    RevisionFields, SelectedEnvironmentRevision,
};
use slingshot_test_support::fake_author::script::{Script, ScriptedExchange, ScriptedResponse};
use slingshot_test_support::fake_author::server::{CredentialPolicy, FakeAuthor, OK_STATUS};

/// Directory holding the committed profile directories.
const PROFILE_FIXTURES: &str = "../slingshot-test-support/fixtures/profile-directories/ordered";

/// Credential document the cloud snapshot is built from.
const CREDENTIAL_FIXTURE: &str = "../slingshot-test-support/fixtures/cloud-credentials/valid.json";

/// Certificates one platform snapshot holds.
const PLATFORM_FIXTURE: &str =
    "../slingshot-test-support/fixtures/additional-certificate-authority/one-authority.pem";

/// Profile whose author address is cleartext and off loopback.
const CLEARTEXT_PROFILE: &str = "remote-site";

/// Environment of that profile.
const CLEARTEXT_ENVIRONMENT: &str = "staging";

/// Profile whose author address is protected.
const PROTECTED_PROFILE: &str = "alpha-site";

/// Environment of that profile.
const PROTECTED_ENVIRONMENT: &str = "production";

/// Identity every cache in this file is built with.
const CACHE_IDENTITY: u64 = 7;

/// Reading the clock reports.
const READING: u64 = 0;

/// Status a successful exchange answers with.
const SUCCESS_STATUS: u16 = 200;

/// A token source that counts how often it was asked.
struct CountingSource {
    /// Exchanges the source performed.
    exchanges: Cell<usize>,
}

impl AccessTokenSource for CountingSource {
    fn exchange(&self) -> Result<AccessToken, ExchangeFailure> {
        self.exchanges.set(self.exchanges.get() + 1);
        exchanged_token()
    }
}

/// A transport answering with one usable token.
struct UsableTransport;

impl IdentityManagementTransport for UsableTransport {
    fn exchange(&self, _body: &[u8]) -> Result<DecodedResponse, ExchangeFailure> {
        Ok(DecodedResponse {
            informational: Vec::new(),
            head: DecodedHead {
                status: SUCCESS_STATUS,
                fields: vec![("content-type".to_owned(), "application/json".to_owned())],
            },
            body: b"{\"access_token\":\"not-a-real-access-token\",\"token_type\":\"bearer\",\"expires_in\":3600000}".to_vec(),
            trailer: None,
        })
    }
}

/// A clock reporting one fixed reading.
struct FixedReading;

impl MonotonicClock for FixedReading {
    fn reading_milliseconds(&self) -> u64 {
        READING
    }
}

/// A clock reporting one fixed second.
struct FixedSecond(u64);

impl CoordinatedUniversalTimeClock for FixedSecond {
    fn sample(&self) -> Option<u64> {
        Some(self.0)
    }
}

/// A trust store holding exactly the roots it is given.
struct ScriptedStore {
    /// Records the store holds.
    records: Vec<ProviderRecord>,
}

impl PlatformTrustSource for ScriptedStore {
    fn records(&self) -> Result<Vec<ProviderRecord>, ConfigurationDiagnostic> {
        Ok(self.records.clone())
    }
}

/// Returns one token an exchange produced.
fn exchanged_token() -> Result<AccessToken, ExchangeFailure> {
    let credentials = credentials();
    let assertion = assertion(&credentials);
    IdentityManagementExchange::new(UsableTransport, FixedReading)
        .exchange(&credentials, &assertion)
}

/// Returns the files the committed profile directory holds.
fn profile_files() -> BTreeMap<String, Vec<u8>> {
    let directory = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(PROFILE_FIXTURES);
    let mut files = BTreeMap::new();
    collect(&directory, &directory, &mut files);
    files
}

/// Collects every file below `directory`, keyed by its root-relative spelling.
fn collect(root: &Path, directory: &Path, files: &mut BTreeMap<String, Vec<u8>>) {
    for entry in std::fs::read_dir(directory).expect("the fixture directory reads") {
        let path = entry.expect("the entry reads").path();
        if path.is_dir() {
            collect(root, &path, files);
            continue;
        }
        let relative = path.strip_prefix(root).expect("the file is inside the fixture");
        files.insert(
            relative.to_str().expect("the path is text").replace('\\', "/"),
            std::fs::read(&path).expect("the file reads"),
        );
    }
}

/// Returns the profiles the committed fixture holds.
fn loaded() -> LoadedProfiles {
    loaded_from_files(profile_files())
}

/// Loads one explicitly supplied profile root.
fn loaded_from_files(files: BTreeMap<String, Vec<u8>>) -> LoadedProfiles {
    let mut authority = ScriptedFilesystem::new();
    for (reference, bytes) in files {
        authority = authority.with_source(&reference, &bytes);
    }
    load_profiles(authority.with_directory("profiles")).expect("the committed root loads")
}

/// Returns the selection naming `profile` and `environment`.
fn selection(loaded: &LoadedProfiles, profile: &str, environment: &str) -> ProfileSelection {
    resolve(
        loaded,
        &RequestedSelection {
            profile: Some(ProfileName::parse(profile).expect("the name is valid")),
            environment: Some(EnvironmentName::parse(environment).expect("the name is valid")),
        },
    )
    .expect("the pair resolves")
}

/// Returns the credentials the cloud snapshot is built from.
fn credentials() -> CloudServiceCredentials {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(CREDENTIAL_FIXTURE);
    let bytes = std::fs::read(&path).expect("the credential reads");
    CloudServiceCredentials::parse(&SensitiveConfigurationDocument::from_bytes(bytes))
        .expect("the credential parses")
}

/// Returns one assertion for `credentials`.
fn assertion(credentials: &CloudServiceCredentials) -> ServiceCredentialAssertion {
    let certificate = credentials.public_certificate();
    use x509_parser::prelude::{FromDer, X509Certificate};

    let (_, parsed) = X509Certificate::from_der(certificate).expect("the certificate parses");
    let second =
        u64::try_from(parsed.validity().not_before.timestamp()).expect("the second fits") + 1;
    ServiceCredentialAssertion::build(credentials, &FixedSecond(second))
        .expect("the assertion builds")
}

/// Returns the platform snapshot every route input here is built from.
fn platform() -> PlatformTrustSnapshot {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(PLATFORM_FIXTURE);
    let bytes = std::fs::read(&path).expect("the certificate reads");
    let certificates = AdditionalAuthorCertificates::parse(&bytes).expect("the certificate parses");
    let store = ScriptedStore {
        records: certificates
            .certificates()
            .iter()
            .map(|der| ProviderRecord {
                der: der.clone(),
                decision: ProviderDecision::UnconditionallyTrustedForServerAuthentication,
            })
            .collect(),
    };
    PlatformTrustSnapshot::take(&store).expect("the snapshot is taken")
}

/// Returns the provider for one selected environment.
fn provider(profile: &str, environment: &str) -> EnvironmentAuthenticationProvider {
    provider_from_loaded(loaded(), profile, environment)
}

/// Returns a provider from a profile root a test has already selected.
fn provider_from_loaded(
    loaded: LoadedProfiles,
    profile: &str,
    environment: &str,
) -> EnvironmentAuthenticationProvider {
    provider_from_loaded_with_platform(loaded, profile, environment, platform())
}

fn provider_from_loaded_with_platform(
    loaded: LoadedProfiles,
    profile: &str,
    environment: &str,
    platform: PlatformTrustSnapshot,
) -> EnvironmentAuthenticationProvider {
    EnvironmentAuthenticationProvider::new(snapshot_from_loaded_with_platform(loaded, profile, environment, platform), CACHE_IDENTITY)
}

fn snapshot_from_loaded_with_platform(
    loaded: LoadedProfiles, profile: &str, environment: &str, platform: PlatformTrustSnapshot,
) -> SelectedEnvironmentSnapshot {
    let selection = selection(&loaded, profile, environment);
    let chosen = selection.environment_of(&loaded);
    let identity_management =
        IdentityManagementTrustInput::from_platform(&platform).expect("the input builds");
    let author_trust =
        AuthorTrustInput::from_platform_and_extension(&platform, None).expect("the input builds");
    let (authentication, principal, metascopes) = match chosen.authentication() {
        EnvironmentAuthentication::BasicCredentials { user_name, password } => {
            let principal = AuthenticationPrincipalIdentity::basic("basic", user_name.as_text())
                .expect("the principal builds");
            let _unused = password;
            (
                SnapshotAuthentication::BasicCredentials {
                    user_name: user_name.clone(),
                    password: rebuilt_password(),
                },
                principal,
                CanonicalMetascopeSet::empty(),
            )
        }
        EnvironmentAuthentication::DeveloperConsoleServiceCredentialsFile { .. } => {
            let credentials = credentials();
            let principal = credentials.principal();
            let metascopes = CanonicalMetascopeSet::from_values(
                &credentials
                    .metascopes()
                    .values()
                    .expect("the scope is text")
                    .iter()
                    .map(|value| (*value).to_owned())
                    .collect::<Vec<String>>(),
            );
            (
                SnapshotAuthentication::ServiceCredentials { credentials: Box::new(credentials) },
                principal,
                metascopes,
            )
        }
    };
    let target = AuthorTargetIdentityDigest::build(
        chosen.deployment().as_text(),
        chosen.author_connection_target().as_text(),
        principal,
    )
    .expect("the target builds");
    let revision = SelectedEnvironmentRevision::build(&RevisionFields {
        profile_name: selection.profile_name().as_text().to_owned(),
        environment_name: selection.environment_name().as_text().to_owned(),
        profile_source_reference: selection.profile_source().as_text().to_owned(),
        selection_source_reference: selection
            .selection_source()
            .map(|source| source.as_text().to_owned()),
        author_target_identity: target,
        publisher_base_address: chosen.publisher_metadata().as_text().to_owned(),
        authentication_method: chosen.authentication().method().to_owned(),
        credential_source_reference: None,
        certificate_source_reference: None,
        proxy_policy: "direct_without_ambient_discovery".to_owned(),
        allow_insecure_author_transport: selection.insecure_author_transport_warning().is_some(),
        canonical_metascope_set: metascopes,
        identity_management_trust_policy_identity: identity_management.identity(),
        author_trust_policy_identity: author_trust.identity(),
    })
    .expect("the revision builds");
    let snapshot = SelectedEnvironmentSnapshot::assemble(
        &selection,
        SnapshotMaterial {
            author: chosen.author_connection_target().clone(),
            publisher: chosen.publisher_metadata().clone(),
            deployment: chosen.deployment(),
            authentication,
            principal,
            target,
            revision,
            identity_management_trust: identity_management,
            author_trust,
        },
    );
    snapshot
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
        connection.endpoint(&["bin", "slingshot-agent", "jobs"]),
        provider.author_endpoint(&["bin", "slingshot-agent", "jobs"]),
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
        let mut decoder = LiteralString::start(bytes[0], 65536).unwrap();
        *bytes = &bytes[1..];
        let mut output = Vec::new();
        while !decoder.is_complete() {
            decoder.push(bytes[0], |byte| { output.push(byte); Ok::<(), StringRefusal>(()) }).unwrap();
            *bytes = &bytes[1..];
        }
        decoder.finish().unwrap(); output
    }
    let endpoint = "http://127.0.0.1:4502/aem";
    let mut files = profile_files();
    replace_profile(&mut files, "profiles/mike.toml", |text| {
        text.replace("http://author.example.com", endpoint).replace("allow_insecure_author_transport = true\n", "")
    });
    let provider = provider_from_loaded(loaded_from_files(files), CLEARTEXT_PROFILE, CLEARTEXT_ENVIRONMENT);
    let transport = SelectedAuthorTransport::new(provider.snapshot().author_connection()).unwrap();
    let (authentication, _) = provider.authenticate(endpoint, READING, &CountingSource { exchanges: Cell::new(0) }).unwrap();
    let fields = http::HeaderMap::new();
    let head = transport.encode_http2_request_head(http::Method::POST, &["bin", "slingshot-agent", "jobs"], &[("q", "é /%")], &authentication, &fields, b"{}").unwrap();
    assert_eq!(format!("{head:?}"), "EncodedRequestHead([redacted])");
    let mut block = Vec::new();
    for frame in head.frames() { assert_eq!(frame[4] & 1, 0); block.extend_from_slice(&frame[9..]); }
    let mut input = block.as_slice();
    let mut decoded = Vec::new();
    while !input.is_empty() {
        assert_eq!(input[0], 0x10, "private request fields must never be indexed"); input = &input[1..];
        let name = string(&mut input); let value = string(&mut input); decoded.push((name, value));
    }
    for (index, name, value) in [
        (0, ":method", "POST"), (1, ":scheme", "http"), (2, ":authority", "127.0.0.1:4502"),
        (3, ":path", "/aem/bin/slingshot-agent/jobs?q=%C3%A9%20%2F%25"),
        (4, "accept-encoding", "identity"), (5, "content-length", "2"),
    ] { assert_eq!(decoded[index], (name.as_bytes().to_vec(), value.as_bytes().to_vec())); }
    assert_eq!(decoded[6].0, b"authorization");
    authentication.lend_value_bytes(|value| assert_eq!(decoded[6].1, value));
    assert_eq!(decoded.len(), 7);
    for name in ["host", "authorization", "content-length", "connection", "transfer-encoding", "upgrade", "trailer", "proxy-connection", "keep-alive", "te"] {
        let mut forbidden = http::HeaderMap::new(); forbidden.insert(http::HeaderName::from_bytes(name.as_bytes()).unwrap(), http::HeaderValue::from_static("x"));
        assert!(transport.encode_http2_request_head(http::Method::GET, &["bin"], &[], &authentication, &forbidden, b"").is_err());
    }
    assert!(transport.encode_http2_request_head(http::Method::GET, &["bin"], &[("q", "a"), ("q", "b")], &authentication, &fields, b"").is_err());
    assert!(transport.encode_http2_request_head(http::Method::CONNECT, &["bin"], &[], &authentication, &fields, b"").is_err());
    for value in [" value", "value ", "\tvalue", "value\t"] {
        let mut malformed = http::HeaderMap::new(); malformed.insert("x", http::HeaderValue::from_str(value).unwrap());
        assert!(transport.encode_http2_request_head(http::Method::GET, &["bin"], &[], &authentication, &malformed, b"").is_err());
    }
}

#[tokio::test]
async fn selected_event_attachments_preserve_context_query_cursor_and_preflight_boundaries() {
    use slingshot_agent_connection::selected_author_http::FiniteHttpFailure;
    use slingshot_agent_connection::selected_author_http2_events::EventHttpOutcome;
    use slingshot_agent_connection::server_sent_event_decoder::{EventStreamCursor, OperationStreamExpectation, StreamItem, StreamRefusal};
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::time::{Duration, timeout};
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let endpoint = format!("http://{}/aem", listener.local_addr().unwrap());
    let mut files = profile_files();
    replace_profile(&mut files, "profiles/mike.toml", |text| text.replace("http://author.example.com", &endpoint).replace("allow_insecure_author_transport = true\n", ""));
    let provider = provider_from_loaded(loaded_from_files(files), CLEARTEXT_PROFILE, CLEARTEXT_ENVIRONMENT);
    let transport = SelectedAuthorTransport::new(provider.snapshot().author_connection()).unwrap();
    let source = CountingSource { exchanges: Cell::new(0) };
    let (authentication, _) = provider.authenticate(&endpoint, READING, &source).unwrap();
    let identity = ExecutionIdentity { attempt: 1, author_target_identity_digest: provider.snapshot().target().to_string(),
        selected_environment_revision: provider.snapshot().revision().to_string(), operation_identifier: "local".into() };
    let resolver = |_: &str| -> Result<OperationStreamExpectation, StreamRefusal> { panic!("heartbeat invoked terminal resolver"); };
    let cursor = EventStreamCursor::new("cursor/one", 96).unwrap();
    for (subscription, generation, cursor_text, wrong_target) in [
        ("", 7, "cursor", false), ("sub", 0, "cursor", false),
        ("sub", 7, "cursor\r\ninjected: x", false), ("sub", 7, "cursor", true),
        ("sub", 7, " cursor", false), ("sub", 7, "cursor ", false),
    ] {
        let mut selected = identity.clone();
        if wrong_target { selected.author_target_identity_digest = "other".into(); }
        let cursor = EventStreamCursor::new(cursor_text, 96).unwrap();
        assert!(matches!(transport.events_http2(&selected, subscription, generation, Some(&cursor), &authentication,
            resolver, |_| panic!("invalid request delivered")).await, Err(FiniteHttpFailure::Request)));
        assert!(matches!(transport.events_http1(&selected, subscription, generation, Some(&cursor), &authentication,
            resolver, |_| panic!("invalid request delivered")).await, Err(FiniteHttpFailure::Request)));
        assert!(matches!(transport.events_negotiated(&selected,subscription,generation,Some(&cursor),&authentication,
            resolver, |_| panic!("invalid negotiated request delivered")).await,Err(FiniteHttpFailure::Request)));
    }
    assert!(timeout(Duration::from_millis(10), listener.accept()).await.is_err());
    let peer = async {
        let (mut stream, _) = listener.accept().await.unwrap();
        let mut initial = [0; 39];
        stream.read_exact(&mut initial).await.unwrap();
        assert_eq!(&initial[..24], b"PRI * HTTP/2.0\r\n\r\nSM\r\n\r\n");
        stream.write_all(&[0, 0, 0, 4, 0, 0, 0, 0, 0]).await.unwrap();
        let mut ack = [0; 9];
        stream.read_exact(&mut ack).await.unwrap();
        assert_eq!(ack, [0, 0, 0, 4, 1, 0, 0, 0, 0]);
        stream.write_all(&ack).await.unwrap();
        let mut head = [0; 9];
        stream.read_exact(&mut head).await.unwrap();
        assert_eq!((head[3], head[4]), (1, 5));
        let length = usize::from(head[0]) << 16 | usize::from(head[1]) << 8 | usize::from(head[2]);
        assert!(length <= 16384);
        let mut block = vec![0; length];
        stream.read_exact(&mut block).await.unwrap();
        for expected in [
            b"/aem/bin/slingshot-agent/events?agent_event_store_generation=7&daemon_subscription_identifier=sub%20%2F%3F".as_slice(),
            b"last-event-id", b"cursor/one", b"authorization", b"text/event-stream",
        ] { assert!(block.windows(expected.len()).any(|bytes| bytes == expected)); }
        authentication.lend_value_bytes(|value| assert!(block.windows(value.len()).any(|bytes| bytes == value)));
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
    let request = transport.events_http2(&identity, "sub /?", 7, Some(&cursor), &authentication, resolver,
        |item| { items.push(item); Ok(()) });
    let (result, ()) = timeout(Duration::from_secs(5), async { tokio::join!(request, peer) }).await.unwrap();
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
            assert!(request.starts_with(b"GET /aem/bin/slingshot-agent/events?agent_event_store_generation=7&daemon_subscription_identifier=sub%20%2F%3F HTTP/1.1\r\n"));
            for header in [b"last-event-id: cursor/one\r\n".as_slice(), b"accept: text/event-stream\r\n"] {
                assert!(request.windows(header.len()).any(|bytes| bytes == header));
            }
            authentication.lend_value_bytes(|value| assert!(request.windows(value.len()).any(|bytes| bytes == value)));
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
        let request = transport.events_http1(&identity, "sub /?", 7, Some(&cursor), &authentication, resolver,
            |item| { items.push(item); Ok(()) });
        let (result, ()) = timeout(Duration::from_secs(5), async { tokio::join!(request, peer) }).await.unwrap();
        assert!(matches!(result.unwrap(), EventHttpOutcome::Closed));
        assert_eq!(items, [StreamItem::Heartbeat]);
    }
    trait EventPeer:tokio::io::AsyncRead+tokio::io::AsyncWrite+Unpin {}
    impl<T:tokio::io::AsyncRead+tokio::io::AsyncWrite+Unpin> EventPeer for T {}
    async fn event_peer(listener:&tokio::net::TcpListener,tls:Option<Arc<rustls::ServerConfig>>)->Box<dyn EventPeer> {
        let (stream,_)=listener.accept().await.unwrap();
        if let Some(config)=tls {
            let stream=tokio_rustls::TlsAcceptor::from(config).accept(stream).await.unwrap();
            assert_eq!(stream.get_ref().1.alpn_protocol(),Some(b"h2".as_slice())); Box::new(stream)
        } else {Box::new(stream)}
    }
    for mode in 0..7 {
        let http2=mode==1 || mode>=3;
        let automatic=mode>=2;
        let endpoint=format!("{}://{}/aem",if mode>=3 {"https"} else {"http"},listener.local_addr().unwrap());
        let mut files=profile_files();
        replace_profile(&mut files,"profiles/mike.toml",|text|text.replace("http://author.example.com",&endpoint).replace("allow_insecure_author_transport = true\n",""));
        use rustls_pki_types::{CertificateDer,PrivateKeyDer,pem::PemObject};
        let root=CertificateDer::from_pem_slice(include_bytes!("fixtures/selected-author-tls/root.pem")).unwrap();
        let platform=PlatformTrustSnapshot::take(&ScriptedStore {records:vec![ProviderRecord {der:root.as_ref().to_vec(),decision:ProviderDecision::UnconditionallyTrustedForServerAuthentication}]}).unwrap();
        let async_provider=slingshot_agent_connection::authentication::environment_provider::AsyncEnvironmentAuthenticationProvider::new_async(
            snapshot_from_loaded_with_platform(loaded_from_files(files.clone()),CLEARTEXT_PROFILE,CLEARTEXT_ENVIRONMENT,platform.clone())).unwrap();
        let provider=provider_from_loaded_with_platform(loaded_from_files(files),CLEARTEXT_PROFILE,CLEARTEXT_ENVIRONMENT,platform);
        let transport=SelectedAuthorTransport::new(provider.snapshot().author_connection()).unwrap();
        let (authentication,_)=provider.authenticate(&endpoint,READING,&source).unwrap();
        let identity=ExecutionIdentity {author_target_identity_digest:provider.snapshot().target().to_string(),selected_environment_revision:provider.snapshot().revision().to_string(),..identity.clone()};
        let tls=if mode>=3 {
            let version=if mode==3 {&rustls::version::TLS12} else {&rustls::version::TLS13};
            let mut config=rustls::ServerConfig::builder_with_provider(Arc::new(rustls::crypto::ring::default_provider()))
                .with_protocol_versions(&[version]).unwrap().with_no_client_auth().with_single_cert(
                    vec![CertificateDer::from_pem_slice(include_bytes!("fixtures/selected-author-tls/leaf.pem")).unwrap()],
                    PrivateKeyDer::from_pem_slice(include_bytes!("fixtures/selected-author-tls/test-only-private-key.pem")).unwrap()).unwrap();
            config.alpn_protocols=vec![b"h2".to_vec(),b"http/1.1".to_vec()]; Some(Arc::new(config))
        } else {None};
        for defect in ["", "identifier", "null-terminal", "state", "physical", "counter-null", "missing-state", "optional-values"] {
            let mut document = serde_json::json!({"agent_event_store_generation":7,
                "agent_operation_identifier":if defect == "identifier" {"invalid-operation".to_owned()} else {"a".repeat(64)},
                "daemon_subscription_identifier":"sub /?", "kind":"progress", "sequence":1,
                "sling_job_identifier":"job-fixture", "state":"running"});
            if defect == "null-terminal" { document["terminal"] = serde_json::Value::Null; }
            if defect == "state" { document["state"] = "queued".into(); }
            if defect == "physical" { document["sling_job_identifier"] = "".into(); }
            if defect == "counter-null" { document["attempt"] = serde_json::Value::Null; }
            if defect == "missing-state" { document.as_object_mut().unwrap().remove("state"); }
            if defect == "optional-values" { document["attempt"] = 0.into(); document["progress"] = 17.into(); }
            let body = format!(": before\nid:cursor-next\ndata:{document}\n\n: after\n");
            let peer = async {
                let mut socket=event_peer(&listener,tls.clone()).await;
                let mut request = Vec::new();
                if http2 {
                    let mut preface = [0;39]; socket.read_exact(&mut preface).await.unwrap();
                    socket.write_all(&[0,0,0,4,0,0,0,0,0]).await.unwrap();
                    let mut header = [0;9]; socket.read_exact(&mut header).await.unwrap(); socket.write_all(&header).await.unwrap();
                    socket.read_exact(&mut header).await.unwrap();
                    let length = usize::from(header[0]) << 16 | usize::from(header[1]) << 8 | usize::from(header[2]);
                    request.resize(length, 0); socket.read_exact(&mut request).await.unwrap();
                } else {
                    while !request.ends_with(b"\r\n\r\n") { request.push(socket.read_u8().await.unwrap()); assert!(request.len() <= 8192); }
                }
                authentication.lend_value_bytes(|value| assert!(request.windows(value.len()).any(|bytes| bytes == value)));
                for expected in [b"/aem/bin/slingshot-agent/events?agent_event_store_generation=7&daemon_subscription_identifier=sub%20%2F%3F".as_slice(),b"cursor/one"] {
                    assert!(request.windows(expected.len()).any(|bytes|bytes==expected));
                }
                if http2 {
                    let mut head = vec![0,0,21,1,4,0,0,0,1,0x88,0x0f,16,17]; head.extend_from_slice(b"text/event-stream");
                    socket.write_all(&head).await.unwrap();
                    let length = (body.len() as u32).to_be_bytes();
                    socket.write_all(&[length[1],length[2],length[3],0,1,0,0,0,1]).await.unwrap();
                    socket.write_all(body.as_bytes()).await.unwrap();
                    let mut close = Vec::new(); let _ = socket.read_to_end(&mut close).await;
                } else {
                    socket.write_all(format!("HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nContent-Length: {}\r\n\r\n{body}", body.len()).as_bytes()).await.unwrap();
                }
                let _ = socket.shutdown().await;
            };
            let mut items = Vec::new();
            let request = async {
                if mode==6 {transport.events_authenticated_async(&identity,"sub /?",7,Some(&cursor),&async_provider,&async_cases::NoClocks,&async_cases::NoClocks,resolver,|item| {items.push(item);Ok(())}).await}
                else if mode==5 {transport.events_authenticated(&identity,"sub /?",7,Some(&cursor),&provider,&source,READING,resolver,|item| {items.push(item);Ok(())}).await}
                else if automatic {transport.events_negotiated(&identity,"sub /?",7,Some(&cursor),&authentication,resolver,|item| {items.push(item);Ok(())}).await}
                else if http2 { transport.events_http2(&identity, "sub /?", 7, Some(&cursor), &authentication, resolver, |item| {items.push(item); Ok(())}).await }
                else { transport.events_http1(&identity, "sub /?", 7, Some(&cursor), &authentication, resolver, |item| {items.push(item); Ok(())}).await }
            };
            let (result, ()) = timeout(Duration::from_secs(5), async {tokio::join!(request, peer)}).await.unwrap();
            if defect.is_empty() || defect == "optional-values" {
                assert!(matches!(result.unwrap(), EventHttpOutcome::Closed)); assert_eq!(items.len(), 3);
                let StreamItem::Event(event) = &items[1] else {panic!("event was not delivered");};
                assert_eq!(event.sling_job_identifier, "job-fixture");
                assert_eq!(event.state, slingshot_agent_protocol::job_event_document::JobEventState::Running);
                assert_eq!(event.attempt, if defect.is_empty() {None} else {Some(0)});
                assert_eq!(event.progress, if defect.is_empty() {None} else {Some(17)});
            }
            else { assert!(result.is_err(), "{defect} http2={http2}"); assert_eq!(items, [StreamItem::Heartbeat]); }
            assert!(timeout(Duration::from_millis(10),listener.accept()).await.is_err(),"event stream retried or fell back");
        }
        for (status, defect) in [(409, ""), (410, ""), (409, "cursor"), (410, "bare"), (409, "truncated")] {
            let body = if defect == "bare" { b"{}".to_vec() } else {
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
            };
            let peer = async {
                let mut stream=event_peer(&listener,tls.clone()).await;
                if http2 {
                    let mut preface = [0; 39];
                    stream.read_exact(&mut preface).await.unwrap();
                    stream.write_all(&[0, 0, 0, 4, 0, 0, 0, 0, 0]).await.unwrap();
                    let mut header = [0; 9];
                    stream.read_exact(&mut header).await.unwrap();
                    stream.write_all(&header).await.unwrap();
                    stream.read_exact(&mut header).await.unwrap();
                    let length = usize::from(header[0]) << 16 | usize::from(header[1]) << 8 | usize::from(header[2]);
                    assert!(length <= 16384);
                    stream.read_exact(&mut vec![0; length]).await.unwrap();
                    let mut block = Vec::new();
                    for (name, value) in [(":status", status.to_string()), ("content-type", "application/json".into()), ("content-length", body.len().to_string())] {
                        block.extend_from_slice(&[0, name.len() as u8]); block.extend_from_slice(name.as_bytes());
                        block.push(value.len() as u8); block.extend_from_slice(value.as_bytes());
                    }
                    let encode = |kind: u8, flags: u8, payload: &[u8]| {
                        let length = (payload.len() as u32).to_be_bytes();
                        let mut frame = vec![length[1], length[2], length[3], kind, flags, 0, 0, 0, 1];
                        frame.extend_from_slice(payload); frame
                    };
                    stream.write_all(&encode(1, 4, &block)).await.unwrap();
                    let payload = if defect == "truncated" { &body[..body.len()-1] } else { &body };
                    stream.write_all(&encode(0, 1, payload)).await.unwrap();
                    let mut discarded = Vec::new();
                    let _ = stream.read_to_end(&mut discarded).await;
                } else {
                    let mut header = Vec::new();
                    while !header.ends_with(b"\r\n\r\n") { header.push(stream.read_u8().await.unwrap()); assert!(header.len() <= 8192); }
                    stream.write_all(format!("HTTP/1.1 {status} Reset\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n", body.len()).as_bytes()).await.unwrap();
                    stream.write_all(if defect == "truncated" { &body[..body.len()-1] } else { &body }).await.unwrap();
                }
                stream.shutdown().await.unwrap();
            };
            let request = async {
                if mode==6 {transport.events_authenticated_async(&identity,"sub /?",7,Some(&cursor),&async_provider,&async_cases::NoClocks,&async_cases::NoClocks,resolver,|_| panic!("reset delivered event")).await}
                else if mode==5 {transport.events_authenticated(&identity,"sub /?",7,Some(&cursor),&provider,&source,READING,resolver,|_| panic!("reset delivered event")).await}
                else if automatic {transport.events_negotiated(&identity,"sub /?",7,Some(&cursor),&authentication,resolver,|_| panic!("reset delivered event")).await}
                else if http2 { transport.events_http2(&identity, "sub /?", 7, Some(&cursor), &authentication, resolver, |_| panic!("reset delivered event")).await }
                else { transport.events_http1(&identity, "sub /?", 7, Some(&cursor), &authentication, resolver, |_| panic!("reset delivered event")).await }
            };
            let (result, ()) = timeout(Duration::from_secs(5), async { tokio::join!(request, peer) }).await.unwrap();
            assert_eq!(result.is_ok(), defect.is_empty(), "http2={http2} status={status} defect={defect}");
            if defect.is_empty() {
                let EventHttpOutcome::Reset(reset) = result.unwrap() else { panic!("missing reset evidence"); };
                assert_eq!(reset.requested_cursor(), Some("cursor/one"));
                assert_eq!(reset.captured_cursor().as_text(), "captured-position");
                assert_eq!(reset.generation(), if status == 409 {8} else {7});
            }
            assert!(timeout(Duration::from_millis(10),listener.accept()).await.is_err(),"event reset retried or fell back");
        }
    }
}

#[tokio::test]
async fn selected_high_water_captures_are_authenticated_bound_and_never_status_only_resets() {
    use slingshot_agent_connection::selected_author_http::FiniteHttpFailure;
    use slingshot_agent_connection::subscription_high_water::HighWaterOutcome;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::time::{Duration, timeout};
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    for mode in 0..7 {
    let http2=mode==1 || mode>=3;
    let automatic=mode>=2;
    let endpoint = format!("{}://{}/aem", if mode>=3 {"https"} else {"http"}, listener.local_addr().unwrap());
    let mut files = profile_files();
    replace_profile(&mut files, "profiles/mike.toml", |text| text.replace("http://author.example.com", &endpoint).replace("allow_insecure_author_transport = true\n", ""));
    use rustls_pki_types::{CertificateDer,PrivateKeyDer,pem::PemObject};
    let root=CertificateDer::from_pem_slice(include_bytes!("fixtures/selected-author-tls/root.pem")).unwrap();
    let platform=PlatformTrustSnapshot::take(&ScriptedStore {records:vec![ProviderRecord {der:root.as_ref().to_vec(),decision:ProviderDecision::UnconditionallyTrustedForServerAuthentication}]}).unwrap();
    let async_provider = slingshot_agent_connection::authentication::environment_provider::AsyncEnvironmentAuthenticationProvider::new_async(
        snapshot_from_loaded_with_platform(loaded_from_files(files.clone()), CLEARTEXT_PROFILE, CLEARTEXT_ENVIRONMENT, platform.clone())).unwrap();
    let provider = provider_from_loaded_with_platform(loaded_from_files(files), CLEARTEXT_PROFILE, CLEARTEXT_ENVIRONMENT, platform);
    let transport = SelectedAuthorTransport::new(provider.snapshot().author_connection()).unwrap();
    let source = CountingSource { exchanges: Cell::new(0) };
    let (authentication, _) = provider.authenticate(&endpoint, READING, &source).unwrap();
    let identity = ExecutionIdentity { attempt: 1, author_target_identity_digest: provider.snapshot().target().to_string(),
        selected_environment_revision: provider.snapshot().revision().to_string(), operation_identifier: "local".into() };
        for (subscription, generation, moved) in [("", 7, false), ("sub", 0, false), ("sub", 7, true)] {
            let mut selected = identity.clone();
            if moved { selected.selected_environment_revision = "other".into(); }
            let result = if mode==6 {transport.capture_high_water_authenticated_async(&selected,subscription,generation,&async_provider,&async_cases::NoClocks,&async_cases::NoClocks).await} else if mode==5 {transport.capture_high_water_authenticated(&selected,subscription,generation,&provider,&source,READING).await} else if automatic {transport.capture_high_water_negotiated(&selected,subscription,generation,&authentication).await} else if http2 { transport.capture_high_water_http2(&selected, subscription, generation, &authentication).await }
                else { transport.capture_high_water_http1(&selected, subscription, generation, &authentication).await };
            assert!(matches!(result, Err(FiniteHttpFailure::Request)));
        }
        assert!(timeout(Duration::from_millis(10), listener.accept()).await.is_err());
        for (status, defect) in [(200, ""), (200, "generation"), (200, "truncated"), (409, ""), (409, "bare"), (410, ""), (401, "")] {
            let body = if defect == "bare" || status == 410 || status == 401 { b"{}".to_vec() }
            else {
                let mut value = serde_json::json!({
                    "format":"slingshot.agent/1",
                    "transport_contract_digest":slingshot_domain::author_agent_transport_contract::AuthorAgentTransportContract::embedded_digest(),
                    "daemon_subscription_identifier":"sub /?",
                    "agent_event_store_generation":if status == 409 || defect == "generation" {8} else {7},
                    "high_water_cursor":"captured-position",
                });
                if status == 409 {
                    value["requested_agent_event_store_generation"] = serde_json::json!(7);
                    value["requested_last_event_identifier"] = serde_json::Value::Null;
                    value["reason"] = serde_json::json!("generation_changed");
                }
                serde_json::to_vec(&value).unwrap()
            };
            let peer = async {
                let (stream, _) = listener.accept().await.unwrap();
                trait Peer:tokio::io::AsyncRead+tokio::io::AsyncWrite+Unpin {}
                impl<T:tokio::io::AsyncRead+tokio::io::AsyncWrite+Unpin> Peer for T {}
                let mut stream:Box<dyn Peer>=if mode>=3 {
                    let version=if mode==3 {&rustls::version::TLS12} else {&rustls::version::TLS13};
                    let mut config=rustls::ServerConfig::builder_with_provider(Arc::new(rustls::crypto::ring::default_provider()))
                        .with_protocol_versions(&[version]).unwrap().with_no_client_auth().with_single_cert(
                            vec![CertificateDer::from_pem_slice(include_bytes!("fixtures/selected-author-tls/leaf.pem")).unwrap()],
                            PrivateKeyDer::from_pem_slice(include_bytes!("fixtures/selected-author-tls/test-only-private-key.pem")).unwrap()).unwrap();
                    config.alpn_protocols=vec![b"h2".to_vec(),b"http/1.1".to_vec()];
                    let stream=tokio_rustls::TlsAcceptor::from(Arc::new(config)).accept(stream).await.unwrap();
                    assert_eq!(stream.get_ref().1.alpn_protocol(),Some(b"h2".as_slice())); Box::new(stream)
                } else {Box::new(stream)};
                let mut request = Vec::new();
                if http2 {
                    let mut preface = [0; 39]; stream.read_exact(&mut preface).await.unwrap();
                    assert_eq!(&preface[..24], b"PRI * HTTP/2.0\r\n\r\nSM\r\n\r\n");
                    stream.write_all(&[0, 0, 0, 4, 0, 0, 0, 0, 0]).await.unwrap();
                    let mut header = [0; 9]; stream.read_exact(&mut header).await.unwrap();
                    assert_eq!(header, [0, 0, 0, 4, 1, 0, 0, 0, 0]);
                    stream.write_all(&header).await.unwrap();
                    stream.read_exact(&mut header).await.unwrap();
                    assert_eq!((header[3], header[4]), (1, 5));
                    let length = usize::from(header[0]) << 16 | usize::from(header[1]) << 8 | usize::from(header[2]);
                    assert!(length <= 16384); request.resize(length, 0);
                    stream.read_exact(&mut request).await.unwrap();
                } else {
                    while !request.ends_with(b"\r\n\r\n") { request.push(stream.read_u8().await.unwrap()); assert!(request.len() <= 8192); }
                    assert!(request.starts_with(b"GET "));
                }
                for expected in [b"/aem/bin/slingshot-agent/events/high-water?agent_event_store_generation=7&daemon_subscription_identifier=sub%20%2F%3F".as_slice(), b"application/json", if http2 { b"authorization" } else { b"Authorization" }] {
                    assert!(request.windows(expected.len()).any(|bytes| bytes == expected), "http2={http2} missing fixed route/header {}", String::from_utf8_lossy(expected));
                }
                assert!(!request.windows(b"last-event-id".len()).any(|bytes| bytes == b"last-event-id"));
                authentication.lend_value_bytes(|value| assert!(request.windows(value.len()).any(|bytes| bytes == value)));
                let payload = if defect == "truncated" { &body[..body.len()-1] } else { &body };
                if http2 {
                    let mut block = Vec::new();
                    for (name, value) in [(":status", status.to_string()), ("content-type", "application/json".into()), ("content-length", body.len().to_string())] {
                        block.extend_from_slice(&[0, name.len() as u8]); block.extend_from_slice(name.as_bytes());
                        block.push(value.len() as u8); block.extend_from_slice(value.as_bytes());
                    }
                    let encode = |kind: u8, flags: u8, payload: &[u8]| {
                        let length = (payload.len() as u32).to_be_bytes();
                        let mut frame = vec![length[1], length[2], length[3], kind, flags, 0, 0, 0, 1];
                        frame.extend_from_slice(payload); frame
                    };
                    stream.write_all(&encode(1, 4, &block)).await.unwrap();
                    stream.write_all(&encode(0, 1, payload)).await.unwrap();
                    let mut close = Vec::new(); let _ = stream.read_to_end(&mut close).await;
                } else {
                    stream.write_all(format!("HTTP/1.1 {status} Response\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n", body.len()).as_bytes()).await.unwrap();
                    stream.write_all(payload).await.unwrap();
                }
                stream.shutdown().await.unwrap();
            };
            let request = async {
                if mode==6 {transport.capture_high_water_authenticated_async(&identity,"sub /?",7,&async_provider,&async_cases::NoClocks,&async_cases::NoClocks).await} else if mode==5 {transport.capture_high_water_authenticated(&identity,"sub /?",7,&provider,&source,READING).await} else if automatic {transport.capture_high_water_negotiated(&identity,"sub /?",7,&authentication).await} else if http2 { transport.capture_high_water_http2(&identity, "sub /?", 7, &authentication).await }
                else { transport.capture_high_water_http1(&identity, "sub /?", 7, &authentication).await }
            };
            let (result, ()) = timeout(Duration::from_secs(5), async { tokio::join!(request, peer) }).await.unwrap();
            assert!(timeout(Duration::from_millis(10),listener.accept()).await.is_err(),"high-water request retried or fell back");
            if !defect.is_empty() { assert!(result.is_err(), "http2={http2} status={status} defect={defect}"); continue; }
            match result.unwrap() {
                HighWaterOutcome::Captured(capture) => {
                    assert_eq!(status, 200); assert_eq!(capture.subscription(), "sub /?");
                    assert_eq!(capture.generation(), 7); assert_eq!(capture.cursor().as_text(), "captured-position");
                }
                HighWaterOutcome::Reset(reset) => {
                    assert_eq!(status, 409); assert_eq!(reset.requested_cursor(), None);
                    assert_eq!(reset.requested_generation(), 7); assert_eq!(reset.generation(), 8);
                }
                HighWaterOutcome::Response(response) => { assert!([401, 410].contains(&status)); assert_eq!(response.status, status); }
            }
        }
    }
}

#[tokio::test]
async fn selected_http2_preparation_negotiates_without_sending_a_request() {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::time::{Duration, timeout};
    for valid in [true, false] {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let endpoint = format!("http://{}", listener.local_addr().unwrap());
        let mut files = profile_files();
        replace_profile(&mut files, "profiles/mike.toml", |text| {
            text.replace("http://author.example.com", &endpoint)
                .replace("allow_insecure_author_transport = true\n", "")
        });
        let provider = provider_from_loaded(loaded_from_files(files), CLEARTEXT_PROFILE, CLEARTEXT_ENVIRONMENT);
        let transport = SelectedAuthorTransport::new(provider.snapshot().author_connection()).unwrap();
        let client = async {
            let result = transport.prepare_http2().await;
            assert_eq!(result.is_ok(), valid);
            if let Err(failure) = result {
                assert!(!failure.request_may_have_reached_author());
            }
        };
        let peer = async {
            let (mut stream, _) = listener.accept().await.unwrap();
            let mut initial = [0; 39];
            stream.read_exact(&mut initial).await.unwrap();
            assert_eq!(&initial[..24], b"PRI * HTTP/2.0\r\n\r\nSM\r\n\r\n");
            assert_eq!(&initial[24..], &[0, 0, 6, 4, 0, 0, 0, 0, 0, 0, 2, 0, 0, 0, 0]);
            if valid {
                stream.write_all(&[0, 0, 0, 4, 0, 0, 0, 0, 0]).await.unwrap();
                let mut ack = [0; 9];
                stream.read_exact(&mut ack).await.unwrap();
                assert_eq!(ack, [0, 0, 0, 4, 1, 0, 0, 0, 0]);
                stream.write_all(&ack).await.unwrap();
            } else {
                stream.write_all(b"HTTP/1.1 200 OK\r\n\r\n").await.unwrap();
            }
            // An invalid peer may observe a TCP reset with unread response
            // bytes, but neither outcome may contain application request bytes.
            let mut extra = [0; 1];
            assert!(!matches!(stream.read(&mut extra).await, Ok(1..)));
        };
        timeout(Duration::from_secs(2), async { tokio::join!(client, peer) }).await.unwrap();
    }
}

/// The selected connector reaches a real loopback fake only after its endpoint
/// came through the normal profile-selection/snapshot path. The test changes
/// the fixture before loading it rather than using a transport-only factory,
/// so an arbitrary caller cannot redirect this connection.
#[tokio::test]
async fn selected_author_transport_dials_the_profile_selected_loopback_author() {
    let author = Arc::new(FakeAuthor::following(
        Script::of(vec![ScriptedExchange {
            response: ScriptedResponse::Respond { body: b"{}".to_vec(), status: OK_STATUS },
            route: "/bin/slingshot-agent/jobs".to_owned(),
        }])
        .expect("the submit route is scriptable"),
        CredentialPolicy::Basic,
    ));
    let recording = author.recording();
    let server = Arc::clone(&author).serve_loopback().await.expect("the fake author binds");
    let mut files = profile_files();
    replace_profile(&mut files, "profiles/mike.toml", |text| {
        text.replace("http://author.example.com", server.endpoint())
            .replace("allow_insecure_author_transport = true\n", "")
    });
    let provider =
        provider_from_loaded(loaded_from_files(files), CLEARTEXT_PROFILE, CLEARTEXT_ENVIRONMENT);
    let transport = SelectedAuthorTransport::new(provider.snapshot().author_connection())
        .expect("the warned loopback author is selected");
    let source = CountingSource { exchanges: Cell::new(0) };
    let (authentication, _) = provider
        .authenticate(server.endpoint(), READING, &source)
        .expect("authenticate selected author");
    let receipt = transport
        .finite_http1(
            http::Method::POST,
            &["bin", "slingshot-agent", "jobs"],
            &authentication,
            &http::HeaderMap::new(),
            b"",
        )
        .await
        .expect("complete bounded HTTP exchange");
    assert_eq!(receipt.response.status, 200);
    assert_eq!(receipt.response.body, b"{}");
    assert_eq!(recording.requests().len(), 1);
    assert!(recording.holds_no_credential_values());
    server.stop().await;
}

/// URI brackets belong on the wire but not in socket/TLS host APIs.
#[tokio::test]
async fn selected_ipv6_author_uses_bare_socket_host_and_bracketed_wire_authority() {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::time::{Duration, timeout};
    let listener = tokio::net::TcpListener::bind("[::1]:0").await.unwrap();
    let origin = format!("http://{}", listener.local_addr().unwrap());
    let endpoint = format!("{origin}/aem");
    let mut files = profile_files();
    replace_profile(&mut files, "profiles/mike.toml", |text| {
        text.replace("http://author.example.com", &endpoint)
            .replace("allow_insecure_author_transport = true\n", "")
    });
    let provider =
        provider_from_loaded(loaded_from_files(files), CLEARTEXT_PROFILE, CLEARTEXT_ENVIRONMENT);
    let transport = SelectedAuthorTransport::new(provider.snapshot().author_connection()).unwrap();
    assert_eq!(transport.origin(), origin);
    let source = CountingSource { exchanges: Cell::new(0) };
    let (authentication, _) = provider.authenticate(&endpoint, READING, &source).unwrap();
    let peer = async {
        let (mut socket, _) = listener.accept().await.unwrap();
        let mut head = Vec::new();
        while !head.ends_with(b"\r\n\r\n") {
            head.push(socket.read_u8().await.unwrap());
        }
        let head = String::from_utf8(head).unwrap();
        assert!(head.starts_with("GET /aem/bin/slingshot-agent/capabilities HTTP/1.1\r\n"));
        assert!(head.contains(&format!("Host: {}\r\n", listener.local_addr().unwrap())));
        socket
            .write_all(
                b"HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: 2\r\n\r\n{}",
            )
            .await
            .unwrap();
    };
    let (receipt, ()) = timeout(Duration::from_secs(5), async {
        let fields = http::HeaderMap::new();
        tokio::join!(
            transport.finite_http1(
                http::Method::GET,
                &["bin", "slingshot-agent", "capabilities"],
                &authentication,
                &fields,
                b""
            ),
            peer
        )
    })
    .await
    .unwrap();
    assert_eq!(receipt.unwrap().response.body, b"{}");
}

/// Updates a fixture and its committed inventory together.
fn replace_profile(
    files: &mut BTreeMap<String, Vec<u8>>,
    reference: &str,
    change: impl FnOnce(&str) -> String,
) {
    use sha2::{Digest, Sha256};
    let profile = files.get_mut(reference).expect("profile exists");
    let old: String = Sha256::digest(&*profile).iter().map(|b| format!("{b:02x}")).collect();
    *profile = change(std::str::from_utf8(profile).expect("profile text")).into_bytes();
    let new: String = Sha256::digest(&*profile).iter().map(|b| format!("{b:02x}")).collect();
    let inventory = files.get_mut("configuration-snapshot.toml").expect("inventory exists");
    *inventory =
        std::str::from_utf8(inventory).expect("inventory text").replace(&old, &new).into_bytes();
}

/// A TCP connection is not sufficient to authenticate a protected author.
#[tokio::test]
async fn selected_author_rejects_non_tls_peer_during_handshake() {
    use slingshot_agent_connection::selected_author_transport::SelectedAuthorTransportFailure;
    use tokio::io::AsyncWriteExt;
    use tokio::time::{Duration, timeout};

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.expect("bind peer");
    let endpoint = format!("https://{}", listener.local_addr().expect("peer address"));
    let mut files = profile_files();
    replace_profile(&mut files, "profiles/zulu.toml", |text| {
        text.replace("https://author.example.com", &endpoint)
    });
    let provider =
        provider_from_loaded(loaded_from_files(files), PROTECTED_PROFILE, PROTECTED_ENVIRONMENT);
    let transport = SelectedAuthorTransport::new(provider.snapshot().author_connection())
        .expect("TLS configuration");
    let peer = async {
        let (mut socket, _) = listener.accept().await.expect("accept connector");
        socket.write_all(b"HTTP/1.1 200 OK\r\n\r\n").await.expect("send invalid TLS");
    };
    let (result, ()) =
        timeout(Duration::from_secs(5), async { tokio::join!(transport.connect(), peer) })
            .await
            .expect("invalid handshake fails promptly");
    assert!(matches!(result, Err(SelectedAuthorTransportFailure::TransportLayerSecurityFailed)));
}

/// Real TLS peers exercise the product connector, not a parallel TLS client.
/// Rejected certificates must fail before any HTTP credentials reach the peer.
#[tokio::test]
async fn selected_author_tls_authenticates_versions_roots_and_hostnames_before_http() {
    use rustls_pki_types::{CertificateDer, PrivateKeyDer, pem::PemObject};
    use slingshot_agent_connection::selected_author_http::FiniteHttpFailure;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::time::{Duration, timeout};

    for automatic in [false, true] {
    for version in [&rustls::version::TLS12, &rustls::version::TLS13] {
        for (trusted, matching_host) in [(true, true), (false, true), (true, false)] {
            let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
            let host = if matching_host { "127.0.0.1" } else { "localhost" };
            let endpoint = format!("https://{host}:{}", listener.local_addr().unwrap().port());
            let mut files = profile_files();
            replace_profile(&mut files, "profiles/mike.toml", |text| {
                text.replace("http://author.example.com", &endpoint)
                    .replace("allow_insecure_author_transport = true\n", "")
            });
            let selected_platform = if trusted {
                let root = CertificateDer::from_pem_slice(include_bytes!("fixtures/selected-author-tls/root.pem")).unwrap();
                PlatformTrustSnapshot::take(&ScriptedStore { records: vec![ProviderRecord {
                    der: root.as_ref().to_vec(),
                    decision: ProviderDecision::UnconditionallyTrustedForServerAuthentication,
                }] }).unwrap()
            } else {
                platform()
            };
            let provider = provider_from_loaded_with_platform(
                loaded_from_files(files), CLEARTEXT_PROFILE, CLEARTEXT_ENVIRONMENT, selected_platform,
            );
            let transport = SelectedAuthorTransport::new(provider.snapshot().author_connection()).unwrap();
            let source = CountingSource { exchanges: Cell::new(0) };
            let (authentication, _) = provider.authenticate(&endpoint, READING, &source).unwrap();
            let configuration = rustls::ServerConfig::builder_with_provider(Arc::new(rustls::crypto::ring::default_provider()))
                .with_protocol_versions(&[version]).unwrap()
                .with_no_client_auth()
                .with_single_cert(
                    vec![CertificateDer::from_pem_slice(include_bytes!("fixtures/selected-author-tls/leaf.pem")).unwrap()],
                    PrivateKeyDer::from_pem_slice(include_bytes!("fixtures/selected-author-tls/test-only-private-key.pem")).unwrap(),
                ).unwrap();
            let peer = async {
                let (socket, _) = listener.accept().await.unwrap();
                let accepted = tokio_rustls::TlsAcceptor::from(Arc::new(configuration)).accept(socket).await;
                let Ok(mut socket) = accepted else { return (None, 0); };
                let negotiated = socket.get_ref().1.protocol_version();
                let mut head = Vec::new();
                while !head.ends_with(b"\r\n\r\n") {
                    match socket.read_u8().await {
                        Ok(byte) => head.push(byte),
                        Err(_) => return (negotiated, head.len()),
                    }
                    assert!(head.len() <= 8192);
                }
                assert!(trusted && matching_host, "a rejected certificate received HTTP bytes");
                assert!(head.starts_with(b"GET /bin/slingshot-agent/capabilities HTTP/1.1\r\n"));
                socket.write_all(b"HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: 2\r\nConnection: close\r\n\r\n{}").await.unwrap();
                socket.shutdown().await.unwrap();
                (negotiated, head.len())
            };
            let headers = http::HeaderMap::new();
            let request = async { if automatic {
                transport.finite_negotiated_query(http::Method::GET, &["bin", "slingshot-agent", "capabilities"], &[], &authentication, &headers, b"").await
            } else { transport.finite_http1(http::Method::GET, &["bin", "slingshot-agent", "capabilities"], &authentication, &headers, b"").await } };
            let (result, (negotiated, bytes)) = timeout(Duration::from_secs(10), async {
                tokio::join!(request, peer)
            }).await.unwrap();
            if trusted && matching_host {
                let receipt = result.unwrap();
                assert_eq!(receipt.response.body, b"{}");
                assert_eq!(negotiated, Some(version.version));
                assert!(bytes > 0);
            } else {
                assert!(matches!(result, Err(FiniteHttpFailure::Connect)));
                assert_eq!(bytes, 0, "credentials cannot precede certificate authentication");
            }
            assert!(timeout(Duration::from_millis(10),listener.accept()).await.is_err(),"finite exchange reconnected");
        }
    }
    }
}

#[tokio::test]
async fn selected_tls_protocol_negotiation_never_silently_downgrades_http2() {
    use rustls_pki_types::{CertificateDer, PrivateKeyDer, pem::PemObject};
    use slingshot_agent_connection::selected_author_transport::{SelectedAuthorStream, SelectedAuthorTransportFailure};
    use tokio::io::AsyncReadExt;
    use tokio::time::{Duration, timeout};

    for version in [&rustls::version::TLS12, &rustls::version::TLS13] {
        for (mode, protocols, expected, accepted) in [
            (2, vec![b"h2".as_slice(), b"http/1.1"], Some(b"h2".as_slice()), true),
            (1, vec![b"h2".as_slice(), b"http/1.1"], Some(b"http/1.1".as_slice()), true),
            (2, vec![b"http/1.1".as_slice()], None, false),
            (1, vec![b"h2".as_slice()], None, false),
            (2, vec![], None, false), (1, vec![], None, true),
            (2, vec![b"h3".as_slice()], None, false),
            (1, vec![b"h3".as_slice()], None, false),
            (0, vec![b"h2".as_slice(),b"http/1.1"], Some(b"h2".as_slice()), true),
            (0, vec![b"http/1.1".as_slice(),b"h2"], Some(b"http/1.1".as_slice()), true),
            (0, vec![b"h2".as_slice()], Some(b"h2".as_slice()), true),
            (0, vec![b"http/1.1".as_slice()], Some(b"http/1.1".as_slice()), true),
            (0, vec![], None, true),
            (0, vec![b"h3".as_slice()], None, false),
        ] {
            let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
            let endpoint = format!("https://{}", listener.local_addr().unwrap());
            let mut files = profile_files();
            replace_profile(&mut files, "profiles/mike.toml", |text| text.replace("http://author.example.com", &endpoint).replace("allow_insecure_author_transport = true\n", ""));
            let root = CertificateDer::from_pem_slice(include_bytes!("fixtures/selected-author-tls/root.pem")).unwrap();
            let platform = PlatformTrustSnapshot::take(&ScriptedStore { records: vec![ProviderRecord {
                der: root.as_ref().to_vec(), decision: ProviderDecision::UnconditionallyTrustedForServerAuthentication,
            }] }).unwrap();
            let provider = provider_from_loaded_with_platform(loaded_from_files(files), CLEARTEXT_PROFILE, CLEARTEXT_ENVIRONMENT, platform);
            let transport = SelectedAuthorTransport::new(provider.snapshot().author_connection()).unwrap();
            let mut configuration = rustls::ServerConfig::builder_with_provider(Arc::new(rustls::crypto::ring::default_provider()))
                .with_protocol_versions(&[version]).unwrap().with_no_client_auth().with_single_cert(
                    vec![CertificateDer::from_pem_slice(include_bytes!("fixtures/selected-author-tls/leaf.pem")).unwrap()],
                    PrivateKeyDer::from_pem_slice(include_bytes!("fixtures/selected-author-tls/test-only-private-key.pem")).unwrap(),
                ).unwrap();
            configuration.alpn_protocols = protocols.iter().map(|protocol| protocol.to_vec()).collect();
            let peer = async {
                let (socket, _) = listener.accept().await.unwrap();
                match tokio_rustls::TlsAcceptor::from(Arc::new(configuration)).accept(socket).await {
                    Ok(mut socket) => {
                        let protocol = socket.get_ref().1.alpn_protocol().map(<[u8]>::to_vec);
                        assert!(socket.read_u8().await.is_err(), "negotiation cannot send application bytes");
                        protocol
                    }
                    Err(_) => None,
                }
            };
            let client = async {
                let result = if mode==0 {
                    transport.connect_negotiated().await.map(|negotiated| {
                        assert_eq!(format!("{negotiated:?}"),"NegotiatedAuthorStream([redacted])");
                        let (protocol,stream)=negotiated.into_parts();
                        use slingshot_agent_connection::selected_author_transport::SelectedHttpProtocol;
                        assert_eq!(protocol,if expected==Some(b"h2".as_slice()) {SelectedHttpProtocol::Http2} else {SelectedHttpProtocol::Http1});
                        stream
                    })
                } else if mode==2 { transport.connect_http2().await } else { transport.connect().await };
                assert_eq!(result.is_ok(), accepted, "mode={mode}, protocols={protocols:?}");
                if let Ok(SelectedAuthorStream::Protected(stream)) = &result {
                    assert_eq!(stream.get_ref().1.alpn_protocol(), expected);
                    assert_eq!(stream.get_ref().1.protocol_version(), Some(version.version));
                }
                if mode==2 && protocols.is_empty() {
                    assert!(matches!(result, Err(SelectedAuthorTransportFailure::ApplicationProtocolUnavailable)));
                }
                drop(result);
            };
            let ((), protocol) = timeout(Duration::from_secs(10), async { tokio::join!(client, peer) }).await.unwrap();
            if accepted { assert_eq!(protocol.as_deref(), expected); }
            assert!(timeout(Duration::from_millis(10),listener.accept()).await.is_err(),"negotiation retried a connection");
        }
    }
}

#[tokio::test]
async fn request_authentication_cannot_cross_selected_target_or_revision() {
    use slingshot_agent_connection::selected_author_http::FiniteHttpFailure;
    use tokio::time::{timeout,Duration};
    let listener=tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let endpoint=format!("http://{}",listener.local_addr().unwrap());
    let build=|change:&str| {
        let mut files=profile_files();
        replace_profile(&mut files,"profiles/mike.toml",|text| {
            let text=text.replace("http://author.example.com",&endpoint).replace("allow_insecure_author_transport = true\n","");
            if change=="target" {text.replace("user_name = \"admin\"","user_name = \"another\"")}
            else if change=="revision" {text.replace("http://publish.example.com","http://other-publisher.example.com")}
            else {text}
        });
        provider_from_loaded(loaded_from_files(files),CLEARTEXT_PROFILE,CLEARTEXT_ENVIRONMENT)
    };
    let selected=build("");
    let transport=SelectedAuthorTransport::new(selected.snapshot().author_connection()).unwrap();
    let source=CountingSource {exchanges:Cell::new(0)};
    for change in ["","target","revision"] {
        let provider=build(change);
        let (authentication,_)=provider.authenticate(&endpoint,READING,&source).unwrap();
        assert_eq!(format!("{authentication:?}"),"RequestAuthentication([redacted])");
        assert_eq!(provider.snapshot().target()==selected.snapshot().target(),change!="target");
        assert_eq!(provider.snapshot().revision()==selected.snapshot().revision(),change.is_empty());
        let fields=http::HeaderMap::new();
        assert_eq!(transport.encode_http2_request_head(http::Method::GET,&["bin"],&[],&authentication,&fields,b"").is_ok(),change.is_empty());
        if change.is_empty() {continue;}
        let refusal = transport.authenticated_finite_get(&provider, &source, READING,
            &["bin"], &[], &fields).await.unwrap_err();
        assert_eq!(refusal, slingshot_agent_connection::selected_author_authenticated_read::AuthenticatedReadFailure::Selection(
            if change == "target" { SelectedAuthorConnectionRefusal::AnotherTarget }
            else { SelectedAuthorConnectionRefusal::AnotherRevision }));
        for mode in 0..3 {
            let result=match mode {
                0=>transport.finite_http1(http::Method::GET,&["bin"],&authentication,&fields,b"").await,
                1=>transport.finite_http2_query(http::Method::GET,&["bin"],&[],&authentication,&fields,b"").await,
                _=>transport.finite_negotiated_query(http::Method::GET,&["bin"],&[],&authentication,&fields,b"").await,
            };
            assert!(matches!(result,Err(FiniteHttpFailure::Request)));
            assert!(timeout(Duration::from_millis(10),listener.accept()).await.is_err(),"foreign authentication reached the socket");
        }
    }
    assert_eq!(source.exchanges.get(),0);
    let foreign_cloud = provider(PROTECTED_PROFILE, PROTECTED_ENVIRONMENT);
    assert!(matches!(transport.authenticated_finite_get(&foreign_cloud, &source, READING,
        &["bin"], &[], &http::HeaderMap::new()).await,
        Err(slingshot_agent_connection::selected_author_authenticated_read::AuthenticatedReadFailure::Selection(_))));
    assert_eq!(source.exchanges.get(), 0, "foreign Cloud selection exchanged before refusal");
    assert!(timeout(Duration::from_millis(10),listener.accept()).await.is_err());
}

#[tokio::test]
async fn selected_cleartext_negotiation_uses_http1_without_upgrade_bytes() {
    use slingshot_agent_connection::selected_author_transport::{SelectedHttpProtocol,SelectedAuthorStream};
    use tokio::{io::AsyncReadExt,time::{timeout,Duration}};
    let listener=tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let endpoint=format!("http://{}",listener.local_addr().unwrap());
    let mut files=profile_files();
    replace_profile(&mut files,"profiles/mike.toml",|text|text.replace("http://author.example.com",&endpoint).replace("allow_insecure_author_transport = true\n",""));
    let provider=provider_from_loaded(loaded_from_files(files),CLEARTEXT_PROFILE,CLEARTEXT_ENVIRONMENT);
    let transport=SelectedAuthorTransport::new(provider.snapshot().author_connection()).unwrap();
    let client=async {
        let (protocol,stream)=transport.connect_negotiated().await.unwrap().into_parts();
        assert_eq!(protocol,SelectedHttpProtocol::Http1);
        assert!(matches!(stream,SelectedAuthorStream::Cleartext(_)));
        drop(stream);
    };
    let peer=async {
        let (mut socket,_)=listener.accept().await.unwrap();
        let mut received=Vec::new(); socket.read_to_end(&mut received).await.unwrap();
        assert!(received.is_empty(),"cleartext negotiation emitted upgrade or probe bytes");
    };
    timeout(Duration::from_secs(5),async {tokio::join!(client,peer)}).await.unwrap();
    assert!(timeout(Duration::from_millis(10),listener.accept()).await.is_err());
}

/// A complete rejection is the only wire evidence authorizing a Cloud read retry.
#[tokio::test]
async fn authenticated_requests_refresh_reads_but_never_repeat_job_posts() {
    use rustls_pki_types::{CertificateDer, PrivateKeyDer, pem::PemObject};
    use slingshot_agent_connection::selected_author_authenticated_read::AuthenticatedReadFailure;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::time::{Duration, timeout};

    struct Source { exchanges: Cell<usize>, fail_refresh: bool }
    struct TokenTransport(usize);
    impl IdentityManagementTransport for TokenTransport {
        fn exchange(&self, _: &[u8]) -> Result<DecodedResponse, ExchangeFailure> {
            let mut response = UsableTransport.exchange(&[])?;
            response.body = format!("{{\"access_token\":\"test-token-{}\",\"token_type\":\"bearer\",\"expires_in\":3600000}}", self.0).into_bytes();
            Ok(response)
        }
    }
    impl AccessTokenSource for Source {
        fn exchange(&self) -> Result<AccessToken, ExchangeFailure> {
            let count = self.exchanges.get() + 1;
            self.exchanges.set(count);
            if self.fail_refresh && count == 2 {
                return Err(ExchangeFailure::new(ConfigurationFailureCode::AuthenticationTargetMismatch));
            }
            let credentials = credentials();
            IdentityManagementExchange::new(TokenTransport(count), FixedReading)
                .exchange(&credentials, &assertion(&credentials))
        }
    }

    for (cloud, asynchronous) in [(false, false), (true, false), (false, true), (true, true)] {
    for scenario in ["success", "twice", "forbidden", "truncated", "refresh-failure", "logical-lookup", "post-401", "post-403", "post-refresh-failure", "post-guard", "post-token401", "post-token-invalid", "artifact", "artifact-short", "artifact-twice", "artifact-401-short", "high-water", "physical-lookup", "event", "event-twice", "event-short", "event-401-short"] {
        let post = scenario.starts_with("post-");
        if asynchronous && !post && !scenario.starts_with("artifact") && !scenario.starts_with("event") { continue; }
        // A cached Cloud token exercises post-byte refresh refusal without
        // dialing external IMS. CSRF refresh success has separate coverage.
        if asynchronous && cloud && scenario == "post-token401" { continue; }
        let artifact = scenario.starts_with("artifact");
        let event = scenario.starts_with("event");
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let endpoint = format!("https://{}", listener.local_addr().unwrap());
        let mut files = profile_files();
        replace_profile(&mut files, if cloud { "profiles/zulu.toml" } else { "profiles/mike.toml" }, |text| {
            text.replace(if cloud { "https://author.example.com" } else { "http://author.example.com" }, &endpoint)
                .replace("allow_insecure_author_transport = true\n", "")
        });
        let root = CertificateDer::from_pem_slice(include_bytes!("fixtures/selected-author-tls/root.pem")).unwrap();
        let platform = PlatformTrustSnapshot::take(&ScriptedStore { records: vec![ProviderRecord {
            der: root.as_ref().to_vec(), decision: ProviderDecision::UnconditionallyTrustedForServerAuthentication,
        }] }).unwrap();
        let async_provider = slingshot_agent_connection::authentication::environment_provider::AsyncEnvironmentAuthenticationProvider::new_async(
            snapshot_from_loaded_with_platform(loaded_from_files(files.clone()),
                if cloud { PROTECTED_PROFILE } else { CLEARTEXT_PROFILE },
                if cloud { PROTECTED_ENVIRONMENT } else { CLEARTEXT_ENVIRONMENT }, platform.clone())).unwrap();
        if asynchronous && cloud { async_cases::prime_runtime_cache(&async_provider).await; }
        let provider = provider_from_loaded_with_platform(loaded_from_files(files),
            if cloud { PROTECTED_PROFILE } else { CLEARTEXT_PROFILE },
            if cloud { PROTECTED_ENVIRONMENT } else { CLEARTEXT_ENVIRONMENT }, platform);
        let transport = SelectedAuthorTransport::new(provider.snapshot().author_connection()).unwrap();
        let source = Source { exchanges: Cell::new(0), fail_refresh: matches!(scenario,"refresh-failure" | "post-refresh-failure") };
        let configuration = rustls::ServerConfig::builder_with_provider(Arc::new(rustls::crypto::ring::default_provider()))
            .with_protocol_versions(&[&rustls::version::TLS13]).unwrap().with_no_client_auth()
            .with_single_cert(vec![CertificateDer::from_pem_slice(include_bytes!("fixtures/selected-author-tls/leaf.pem")).unwrap()],
                PrivateKeyDer::from_pem_slice(include_bytes!("fixtures/selected-author-tls/test-only-private-key.pem")).unwrap()).unwrap();
        let acceptor = tokio_rustls::TlsAcceptor::from(Arc::new(configuration));
        let identity = ExecutionIdentity {
            attempt: 1, author_target_identity_digest: provider.snapshot().target().to_string(),
            selected_environment_revision: provider.snapshot().revision().to_string(),
            operation_identifier: "authenticated-lookup".into(),
        };
        let provenance = slingshot_agent_protocol::wire_contract::ExpectedProvenance {
            canonical_json_contract_digest: slingshot_domain::command::schema::canonical_contract_digest(),
            command_contract: slingshot_domain::selected_command_contract_identity::SelectedCommandContractIdentity::installed("query_paths").unwrap(),
            transport_contract_digest: slingshot_domain::author_agent_transport_contract::AuthorAgentTransportContract::embedded_digest(),
        };
        let submission = slingshot_agent_connection::command_submission::Submission::build(
            &provenance, slingshot_agent_protocol::identity::WireOperationIdentity::of(
                &identity.author_target_identity_digest, &identity.selected_environment_revision,
                &identity.operation_identifier, slingshot_domain::agent_identity::AgentEventStoreGeneration::of(7)),
            "subscription-one", r#"{"root_path":"/content"}"#,
            slingshot_agent_connection::command_submission::ExpectedArtifactManifest::empty(),
        ).unwrap();
        let retries = cloud && !asynchronous && matches!(scenario, "success" | "twice" | "logical-lookup" | "artifact" | "artifact-twice" | "high-water" | "physical-lookup" | "event" | "event-twice");
        let peer = async {
            let mut previous = None;
            let last = if matches!(scenario,"post-guard"|"post-token-invalid") {0} else if scenario == "post-token401" {if cloud {2} else {0}} else {usize::from(retries || post)};
            for attempt in 0..=last {
                let posting = post && attempt == if scenario == "post-token401" {2} else {1};
                let (socket, _) = listener.accept().await.unwrap();
                let mut socket = acceptor.accept(socket).await.unwrap();
                let mut head = Vec::new();
                while !head.ends_with(b"\r\n\r\n") {
                    head.push(socket.read_u8().await.unwrap());
                    assert!(head.len() < 8192);
                }
                let head = String::from_utf8(head).unwrap();
                let expected_route = if event {
                    assert!(head.to_ascii_lowercase().contains("last-event-id: cursor-before\r\n"));
                    "GET /bin/slingshot-agent/events?agent_event_store_generation=7&daemon_subscription_identifier=subscription-one HTTP/1.1\r\n".into()
                } else if scenario=="physical-lookup" {
                    "GET /bin/slingshot-agent/jobs/snapshot?sling_job_identifier=job-one HTTP/1.1\r\n".into()
                } else if scenario=="high-water" {
                    assert!(!head.to_ascii_lowercase().contains("last-event-id:"));
                    "GET /bin/slingshot-agent/events/high-water?agent_event_store_generation=7&daemon_subscription_identifier=subscription-one HTTP/1.1\r\n".into()
                } else if artifact {
                    format!("GET /bin/slingshot-agent/operations/{}/artifacts/content_package HTTP/1.1\r\n",submission.operation.agent_operation_identifier)
                } else if post {
                    if !posting { "GET /libs/granite/csrf/token.json HTTP/1.1\r\n".into() }
                    else { "POST /bin/slingshot-agent/jobs HTTP/1.1\r\n".into() }
                } else if scenario == "logical-lookup" {
                    format!("GET /bin/slingshot-agent/operations/lookup?agent_operation_identifier={} HTTP/1.1\r\n", submission.operation.agent_operation_identifier)
                } else { "GET /bin/slingshot-agent/capabilities?probe=one%20two HTTP/1.1\r\n".into() };
                assert!(head.starts_with(&expected_route));
                if posting {
                    let expected = submission.wire_body().unwrap();
                    let mut body = vec![0;expected.len()];
                    socket.read_exact(&mut body).await.unwrap();
                    assert_eq!(body,expected);
                    assert!(head.to_ascii_lowercase().contains("csrf-token: test-csrf\r\n"));
                    assert!(head.to_ascii_lowercase().contains("idempotency-key:"));
                }
                let normalized = if cloud && asynchronous {
                    assert!(head.contains("Bearer not-a-real-access-token"));
                    head
                } else if cloud {
                    let generation = if scenario == "post-token401" {if attempt==0 {1} else {2}} else if post {1} else {attempt+1};
                    assert!(head.contains(&format!("Bearer test-token-{generation}")));
                    head.replace(&format!("test-token-{generation}"), "test-token")
                } else { assert!(head.contains("Basic ")); head };
                if !post { if let Some(previous) = &previous { assert_eq!(&normalized, previous); } }
                previous = Some(normalized);
                let status = if event {if scenario=="event-short" || (scenario=="event" && attempt==1) {200} else {401}} else if scenario=="physical-lookup" && attempt==1 {404} else if scenario=="high-water" && attempt==1 {200} else if artifact {if scenario=="artifact-short" || (scenario=="artifact" && attempt==1) {200} else {401}} else if scenario == "post-token401" {if attempt==0 {401} else if posting {403} else {200}} else if post && attempt == 0 {200} else if scenario == "post-403" || scenario == "forbidden" { 403 } else if attempt == 1 && scenario == "logical-lookup" { 404 } else if attempt == 1 && scenario == "success" { 200 } else { 401 };
                let body = if event && status==200 {if scenario=="event-short" {": alive\n\nid: unfinished".into()} else {": alive\n\n".into()}} else if scenario=="physical-lookup" && status==404 {serde_json::json!({
                    "kind":"missing", "format":"slingshot.agent/1", "transport_contract_digest":submission.provenance.transport_contract_digest,
                    "agent_event_store_generation":8, "sling_job_identifier":"job-one",
                }).to_string()} else if scenario=="high-water" && status==200 {serde_json::json!({
                    "format":"slingshot.agent/1", "transport_contract_digest":submission.provenance.transport_contract_digest,
                    "daemon_subscription_identifier":"subscription-one", "agent_event_store_generation":7,
                    "high_water_cursor":"captured-position",
                }).to_string()} else if artifact && status==200 {if scenario=="artifact-short" {"ab".into()} else {"abc".into()}} else if scenario == "post-token-invalid" {r#"{"token":""}"#.into()} else if post && status == 200 { r#"{"token":"test-csrf"}"#.into() } else if status == 404 { serde_json::json!({
                    "kind":"missing", "format":"slingshot.agent/1",
                    "transport_contract_digest":submission.provenance.transport_contract_digest,
                    "agent_event_store_generation":submission.operation.agent_event_store_generation,
                    "agent_operation_identifier":submission.operation.agent_operation_identifier,
                    "author_target_identity_digest":submission.operation.author_target_identity_digest,
                }).to_string() } else { "{}".into() };
                let length = if matches!(scenario,"truncated"|"artifact-short"|"artifact-401-short"|"event-short"|"event-401-short") { body.len()+1 } else { body.len() };
                if attempt == 0 && retries { tokio::time::sleep(Duration::from_millis(20)).await; }
                let media=if event && status==200 {"text/event-stream"} else if artifact && status==200 {"application/zip"} else {"application/json"};
                socket.write_all(format!("HTTP/1.1 {status} Test\r\nContent-Type: {media}\r\nContent-Length: {length}\r\nConnection: close\r\n\r\n{body}").as_bytes()).await.unwrap();
                socket.shutdown().await.unwrap();
            }
        };
        let fields = http::HeaderMap::new();
        let guard_calls = Cell::new(0);
        let mut artifact_bytes = Vec::new();
        let mut heartbeats = 0;
        let (result, ()) = timeout(Duration::from_secs(10), async {
            tokio::join!(async {
                if event {
                    use slingshot_agent_connection::{server_sent_event_decoder::{EventStreamCursor,StreamItem},selected_author_events::EventHttpOutcome};
                    let cursor=EventStreamCursor::new("cursor-before",96).unwrap();
                    let resolver=|_:&str| panic!("heartbeat must not resolve an operation");
                    let consume=|item| {assert!(matches!(item,StreamItem::Heartbeat));heartbeats+=1;Ok(())};
                    let outcome=if asynchronous && cloud {
                        transport.events_authenticated_async(&identity,"subscription-one",7,Some(&cursor),&async_provider,&async_cases::Clock,&async_cases::UnavailableUtc,resolver,consume).await
                    } else if asynchronous {
                        transport.events_authenticated_async(&identity,"subscription-one",7,Some(&cursor),&async_provider,&async_cases::NoClocks,&async_cases::NoClocks,resolver,consume).await
                    } else {transport.events_authenticated(&identity,"subscription-one",7,Some(&cursor),&provider,&source,READING,resolver,consume).await};
                    outcome.map(|outcome| match outcome {
                            EventHttpOutcome::Closed=>(200,0),EventHttpOutcome::Response(response)=>(response.status,0),_=>panic!("unexpected reset"),
                        }).map_err(AuthenticatedReadFailure::Transport)
                } else if scenario=="physical-lookup" {
                    transport.lookup_physical_job_authenticated(&identity,&submission,"job-one",8,&provider,&source,READING).await.map(|outcome| {
                        let slingshot_agent_connection::selected_author_lookup::PhysicalLookupReceipt::Missing(proof)=outcome else {panic!("missing physical job became a snapshot")};
                        assert_eq!(proof.generation(),8);assert_eq!(proof.sling_job_identifier(),"job-one");assert_eq!(submission.operation.agent_event_store_generation,7);(404,0)
                    }).map_err(|_|AuthenticatedReadFailure::Transport(slingshot_agent_connection::selected_author_http::FiniteHttpFailure::Head))
                } else if scenario=="high-water" {
                    transport.capture_high_water_authenticated(&identity,"subscription-one",7,&provider,&source,READING).await.map(|outcome| {
                        use slingshot_agent_connection::subscription_high_water::HighWaterOutcome;
                        match outcome {
                            HighWaterOutcome::Captured(capture) => {assert_eq!(capture.cursor().as_text(),"captured-position");assert_eq!(capture.generation(),7);assert_eq!(capture.subscription(),"subscription-one");(200,0)},
                            HighWaterOutcome::Response(response) => (response.status,0),
                            _ => panic!("capture became an unexpected reset"),
                        }
                    }).map_err(AuthenticatedReadFailure::Transport)
                } else if artifact {
                    use sha2::Digest;
                    let expected=slingshot_agent_connection::artifact_download::ExpectedArtifact {
                        artifact_digest:sha2::Sha256::digest(b"abc").iter().map(|byte|format!("{byte:02x}")).collect(),
                        artifact_slot:"content_package".into(),byte_length:3,media_type:"application/zip".into(),
                    };
                    let sink=|bytes:&[u8]| {artifact_bytes.extend_from_slice(bytes);Ok(())};
                    let outcome=if asynchronous && cloud {
                        transport.artifact_authenticated_async(&identity,&submission,&expected,&"3".repeat(64),&async_provider,&async_cases::Clock,&async_cases::UnavailableUtc,sink).await
                    } else if asynchronous {
                        transport.artifact_authenticated_async(&identity,&submission,&expected,&"3".repeat(64),&async_provider,&async_cases::NoClocks,&async_cases::NoClocks,sink).await
                    } else {transport.artifact_authenticated(&identity,&submission,&expected,&"3".repeat(64),&provider,&source,READING,sink).await};
                    outcome
                        .map(|outcome| {let slingshot_agent_connection::selected_author_http::ArtifactHttpOutcome::Transferred(receipt)=outcome else {panic!("artifact was not transferred")};(3,receipt.elapsed_milliseconds())})
                        .map_err(AuthenticatedReadFailure::Transport)
                } else if post {
                    let guard = || {
                        guard_calls.set(guard_calls.get()+1);
                        if scenario == "post-guard" {Err(slingshot_agent_connection::selected_author_submission::SubmissionSendRefusal::Identity)} else {Ok(())}
                    };
                    let outcome = if asynchronous && cloud {
                        transport.send_submission_authenticated_async_guarded(&identity,&submission,&async_provider,
                            &async_cases::Clock,&async_cases::UnavailableUtc,1,guard).await
                    } else if asynchronous {
                        transport.send_submission_authenticated_async_guarded(&identity,&submission,&async_provider,
                            &async_cases::NoClocks,&async_cases::NoClocks,1,guard).await
                    } else {
                        transport.send_submission_authenticated_guarded(&identity,&submission,&provider,&source,READING,1,guard).await
                    };
                    outcome
                        .map(|outcome| { assert!(matches!(outcome,slingshot_agent_connection::command_submission::SubmissionOutcome::SubmissionUnknown {..})); (0,0) })
                        .map_err(|_| AuthenticatedReadFailure::Transport(slingshot_agent_connection::selected_author_http::FiniteHttpFailure::Request))
                } else if scenario == "logical-lookup" {
                    use slingshot_agent_connection::{selected_author_lookup::OperationLookupReceipt, job_snapshot_reconciliation::LookupAnswer};
                    transport.lookup_operation_authenticated(&identity,&submission,&provider,&source,READING).await
                        .map(|receipt| { assert!(matches!(receipt,OperationLookupReceipt::Absent(LookupAnswer::Missing))); (404,0) })
                        .map_err(|_| AuthenticatedReadFailure::Transport(slingshot_agent_connection::selected_author_http::FiniteHttpFailure::Head))
                } else {
                    transport.authenticated_finite_get(&provider, &source, READING,
                        &["bin", "slingshot-agent", "capabilities"], &[("probe", "one two")], &fields).await
                        .map(|receipt| (receipt.response.status,receipt.elapsed_milliseconds))
                }
            }, peer)
        }).await.unwrap();
        if event {if scenario.ends_with("short") || (asynchronous && cloud) {assert!(result.is_err());} else {assert_eq!(result.unwrap().0,if cloud && scenario=="event" {200} else {401});}
            assert_eq!(heartbeats,usize::from(scenario=="event-short" || (scenario=="event" && cloud && !asynchronous)));}
        else if scenario=="physical-lookup" {if cloud {assert_eq!(result.unwrap().0,404);} else {assert!(result.is_err());}}
        else if scenario=="high-water" {assert_eq!(result.unwrap().0,if cloud {200} else {401});}
        else if artifact {
            if cloud && !asynchronous && scenario=="artifact" {let receipt=result.unwrap();assert_eq!(receipt.0,3);assert!(receipt.1>=20);assert_eq!(artifact_bytes,b"abc");}
            else {assert!(result.is_err()); if scenario!="artifact-short" {assert!(artifact_bytes.is_empty());}}
        }
        else if post { if matches!(scenario,"post-guard"|"post-token-invalid") || (scenario == "post-token401" && !cloud) {assert!(result.is_err());} else {assert_eq!(result.unwrap(),(0,0));} }
        else if scenario == "truncated" { assert!(matches!(result, Err(AuthenticatedReadFailure::Transport(_)))); }
        else if cloud && scenario == "refresh-failure" { assert!(matches!(result, Err(AuthenticatedReadFailure::Authentication(_)))); }
        else if scenario == "logical-lookup" { if cloud {assert_eq!(result.unwrap().0,404);} else {assert!(result.is_err());} }
        else {
            let receipt = result.unwrap();
            assert_eq!(receipt.0, if scenario == "forbidden" { 403 } else if retries && scenario == "success" { 200 } else { 401 });
            if retries { assert!(receipt.1 >= 20, "retry discarded first exchange time"); }
        }
        assert_eq!(source.exchanges.get(), if !cloud || asynchronous { 0 } else if retries || scenario == "refresh-failure" || (post && !matches!(scenario,"post-403"|"post-guard"|"post-token-invalid")) { 2 } else { 1 });
        if asynchronous && cloud {
            let authentication = async_provider.authenticate(&endpoint,&async_cases::Clock,&async_cases::UnavailableUtc).await;
            assert_eq!(authentication.is_ok(),matches!(scenario,"post-403"|"post-guard"|"post-token-invalid"|"artifact-short"|"artifact-401-short"|"event-short"|"event-401-short"),
                "only a complete 401 before stream delivery must invalidate the cached Cloud token");
        }
        assert_eq!(guard_calls.get(),usize::from(post && scenario != "post-token-invalid" && !(scenario=="post-token401" && !cloud)));
        assert!(timeout(Duration::from_millis(10), listener.accept()).await.is_err(), "unexpected additional retry");
    }
    }
}

/// Full selected-author HTTP/2 exchanges require a clean protocol/transport end.
#[tokio::test]
async fn selected_http2_finite_exchange_uses_exact_request_over_cleartext_and_tls() {
    use rustls_pki_types::{CertificateDer, PrivateKeyDer, pem::PemObject};
    use slingshot_agent_connection::selected_author_transport::SelectedAuthorStream;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::time::{Duration, timeout};

    for (automatic, version, clean_end) in [
        (false, None, true),
        (false, Some(&rustls::version::TLS12), true), (false, Some(&rustls::version::TLS13), true),
        (false, Some(&rustls::version::TLS12), false), (false, Some(&rustls::version::TLS13), false),
        (true, Some(&rustls::version::TLS12), true), (true, Some(&rustls::version::TLS13), true),
        (true, Some(&rustls::version::TLS12), false), (true, Some(&rustls::version::TLS13), false),
    ] {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let endpoint = format!("{}://{}/aem", if version.is_some() { "https" } else { "http" }, listener.local_addr().unwrap());
        let mut files = profile_files();
        replace_profile(&mut files, "profiles/mike.toml", |text| text.replace("http://author.example.com", &endpoint).replace("allow_insecure_author_transport = true\n", ""));
        let root = CertificateDer::from_pem_slice(include_bytes!("fixtures/selected-author-tls/root.pem")).unwrap();
        let platform = PlatformTrustSnapshot::take(&ScriptedStore { records: vec![ProviderRecord {
            der: root.as_ref().to_vec(), decision: ProviderDecision::UnconditionallyTrustedForServerAuthentication,
        }] }).unwrap();
        let provider = provider_from_loaded_with_platform(loaded_from_files(files), CLEARTEXT_PROFILE, CLEARTEXT_ENVIRONMENT, platform);
        let transport = SelectedAuthorTransport::new(provider.snapshot().author_connection()).unwrap();
        let source = CountingSource { exchanges: Cell::new(0) };
        let (authentication, _) = provider.authenticate(&endpoint, READING, &source).unwrap();
        let headers = http::HeaderMap::new();
        let path = ["bin", "slingshot-agent", "jobs"];
        let query = [("q", "é /%")];
        let expected_head: Vec<u8> = transport.encode_http2_request_head(http::Method::POST, &path, &query, &authentication, &headers, b"{}").unwrap().frames().flatten().collect();
        let peer = async {
            let (socket, _) = listener.accept().await.unwrap();
            // Use one AsyncRead/Write trait object for cleartext and the server TLS stream.
            trait Peer: tokio::io::AsyncRead + tokio::io::AsyncWrite + Unpin {}
            impl<T: tokio::io::AsyncRead + tokio::io::AsyncWrite + Unpin> Peer for T {}
            let mut socket: Box<dyn Peer> = if let Some(version) = version {
                let mut configuration = rustls::ServerConfig::builder_with_provider(Arc::new(rustls::crypto::ring::default_provider()))
                    .with_protocol_versions(&[version]).unwrap().with_no_client_auth().with_single_cert(
                        vec![CertificateDer::from_pem_slice(include_bytes!("fixtures/selected-author-tls/leaf.pem")).unwrap()],
                        PrivateKeyDer::from_pem_slice(include_bytes!("fixtures/selected-author-tls/test-only-private-key.pem")).unwrap(),
                    ).unwrap();
                configuration.alpn_protocols = vec![b"h2".to_vec()];
                let socket = tokio_rustls::TlsAcceptor::from(Arc::new(configuration)).accept(socket).await.unwrap();
                assert_eq!(socket.get_ref().1.alpn_protocol(), Some(b"h2".as_slice()));
                Box::new(socket)
            } else { Box::new(SelectedAuthorStream::Cleartext(socket)) };
            let mut preface = [0; 39];
            socket.read_exact(&mut preface).await.unwrap();
            assert_eq!(&preface[..24], b"PRI * HTTP/2.0\r\n\r\nSM\r\n\r\n");
            socket.write_all(&[0, 0, 0, 4, 0, 0, 0, 0, 0]).await.unwrap();
            let mut ack = [0; 9];
            socket.read_exact(&mut ack).await.unwrap();
            assert_eq!(ack, [0, 0, 0, 4, 1, 0, 0, 0, 0]);
            socket.write_all(&ack).await.unwrap();
            let mut actual = vec![0; expected_head.len()];
            socket.read_exact(&mut actual).await.unwrap();
            assert!(actual == expected_head, "request bytes differ from selected-origin authenticated encoder");
            let mut data = [0; 11];
            socket.read_exact(&mut data).await.unwrap();
            assert_eq!(&data[..9], &[0, 0, 2, 0, 1, 0, 0, 0, 1]);
            assert_eq!(&data[9..], b"{}");
            // Independent literal HPACK response, then one exact END_STREAM DATA.
            let mut response = vec![0, 0, 20, 1, 4, 0, 0, 0, 1, 0x88, 0x0f, 16, 16];
            response.extend_from_slice(b"application/json");
            response.extend_from_slice(&[0, 0, 2, 0, 1, 0, 0, 0, 1, b'{', b'}']);
            socket.write_all(&response).await.unwrap();
            let mut shutdown = [0; 17];
            socket.read_exact(&mut shutdown).await.unwrap();
            assert_eq!(shutdown, [0, 0, 8, 7, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0]);
            assert_eq!(socket.read(&mut [0; 1]).await.unwrap(), 0);
            if clean_end { socket.shutdown().await.unwrap(); }
        };
        let request = async { if automatic {
            transport.finite_negotiated_query(http::Method::POST, &path, &query, &authentication, &headers, b"{}").await
        } else { transport.finite_http2_query(http::Method::POST, &path, &query, &authentication, &headers, b"{}").await } };
        let (receipt, ()) = timeout(Duration::from_secs(5), async { tokio::join!(request, peer) }).await.unwrap();
        assert_eq!(receipt.is_ok(), clean_end);
        assert!(timeout(Duration::from_millis(10),listener.accept()).await.is_err(),"negotiated request reconnected or retried");
        if !clean_end {
            assert!(receipt.unwrap_err().request_may_have_reached_author());
            continue;
        }
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
    for automatic in [false,true] {
    for (response, valid) in
        rejected.iter().map(|r| (*r, false)).chain(accepted.iter().map(|r| (*r, true)))
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
        let exchange = async { if automatic {
            transport.finite_negotiated_query(http::Method::POST, &["submit"], &[], &authentication, &fields, b"").await
        } else { transport.finite_http1(http::Method::POST, &["submit"], &authentication, &fields, b"").await } };
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
        let (result, ()) = timeout(Duration::from_secs(5), async { tokio::join!(exchange, peer) })
            .await
            .expect("exchange bounded");
        if valid {
            assert_eq!(result.expect("valid framing").response.body, b"{}");
        } else {
            let failure = result.expect_err("hostile response cannot publish a receipt");
            assert!(failure.request_may_have_reached_author());
        }
        assert!(timeout(Duration::from_millis(10),listener.accept()).await.is_err(),"finite response refusal caused fallback");
    }
    }
}

/// The concrete POST binds identity and preserves canonical text on the wire.
#[tokio::test]
async fn selected_submission_sends_bound_bytes_once_and_validates_the_answer() {
    use slingshot_agent_connection::command_submission::{
        ExpectedArtifactManifest, Submission, SubmissionOutcome,
    };
    use slingshot_agent_connection::selected_author_submission::SubmissionSendRefusal;
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

    for (media, wrong_echo, accepted) in [
        ("application/json", false, true),
        ("Application/JSON; CHARSET=\"UTF-8\"", false, true),
        ("application/json; charset=latin1", false, false),
        ("application/json; charset=utf-8; charset=utf-8", false, false),
        ("application/json", true, false),
    ] {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let endpoint = format!("http://{}", listener.local_addr().unwrap());
        let mut files = profile_files();
        replace_profile(&mut files, "profiles/mike.toml", |text| {
            text.replace("http://author.example.com", &format!("{endpoint}/aem"))
                .replace("allow_insecure_author_transport = true\n", "")
        });
        let async_provider = slingshot_agent_connection::authentication::environment_provider::AsyncEnvironmentAuthenticationProvider::new_async(
            snapshot_from_loaded_with_platform(loaded_from_files(files.clone()), CLEARTEXT_PROFILE, CLEARTEXT_ENVIRONMENT, platform())).unwrap();
        let provider = provider_from_loaded(
            loaded_from_files(files),
            CLEARTEXT_PROFILE,
            CLEARTEXT_ENVIRONMENT,
        );
        let transport =
            SelectedAuthorTransport::new(provider.snapshot().author_connection()).unwrap();
        let source = CountingSource { exchanges: Cell::new(0) };
        let (authentication, _) = provider
            .authenticate(provider.snapshot().author().as_text(), READING, &source)
            .unwrap();
        let identity = ExecutionIdentity {
            attempt: 1,
            author_target_identity_digest: provider.snapshot().target().to_string(),
            selected_environment_revision: provider.snapshot().revision().to_string(),
            operation_identifier: "local-operation".to_owned(),
        };
        let expected = ExpectedProvenance {
            canonical_json_contract_digest: canonical_contract_digest(),
            command_contract: SelectedCommandContractIdentity::installed("query_paths").unwrap(),
            transport_contract_digest: AuthorAgentTransportContract::embedded_digest(),
        };
        // Unicode must survive the outer JSON string unchanged.
        for mode in [0,1,2,3,4] {
        let http2=mode==1;
        for (generation, ready, location, compatible) in
            [(7, true, false, true), (8, true, false, false), (7, false, false, false), (7, true, true, false)]
        {
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
                    let mut preface = [0;39]; socket.read_exact(&mut preface).await.unwrap();
                    assert_eq!(&preface[..24], b"PRI * HTTP/2.0\r\n\r\nSM\r\n\r\n");
                    socket.write_all(&[0,0,0,4,0,0,0,0,0]).await.unwrap();
                    let mut header = [0;9]; socket.read_exact(&mut header).await.unwrap(); socket.write_all(&header).await.unwrap();
                    socket.read_exact(&mut header).await.unwrap(); assert_eq!((header[3],header[4]),(1,5));
                    let length = usize::from(header[0]) << 16 | usize::from(header[1]) << 8 | usize::from(header[2]);
                    assert!(length <= 16384); head.resize(length,0); socket.read_exact(&mut head).await.unwrap();
                    assert!(head.windows(b"/aem/bin/slingshot-agent/capabilities".len()).any(|part| part == b"/aem/bin/slingshot-agent/capabilities"));
                    authentication.lend_value_bytes(|value| assert!(head.windows(value.len()).any(|part| part == value)));
                    let mut block = vec![0x88];
                    for (name,value) in [("content-type","application/json".to_owned()),("content-length",document.len().to_string())] {
                        block.extend_from_slice(&[0,name.len() as u8]); block.extend_from_slice(name.as_bytes()); block.push(value.len() as u8); block.extend_from_slice(value.as_bytes());
                    }
                    if location { block.extend_from_slice(b"\x00\x08location\x0a/elsewhere"); }
                    for (kind,flags,bytes) in [(1,4,block.as_slice()),(0,1,document.as_bytes())] {
                        let length = (bytes.len() as u32).to_be_bytes(); socket.write_all(&[length[1],length[2],length[3],kind,flags,0,0,0,1]).await.unwrap(); socket.write_all(bytes).await.unwrap();
                    }
                    let mut close = Vec::new(); let _ = socket.read_to_end(&mut close).await;
                    return;
                }
                while !head.ends_with(b"\r\n\r\n") {
                    head.push(socket.read_u8().await.unwrap());
                }
                let head = String::from_utf8(head).unwrap();
                assert!(head.starts_with("GET /aem/bin/slingshot-agent/capabilities HTTP/1.1\r\n"));
                assert!(head.contains("Authorization: Basic "));
                socket.write_all(format!("HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\n{}\r\n{document}", document.len(), if location {"Location: /elsewhere\r\n"} else {""}).as_bytes()).await.unwrap();
            };
            let (outcome, ()) = timeout(Duration::from_secs(5), async {
                tokio::join!(
                    async { if mode==4 {transport.discover_capabilities_authenticated_async(&identity,"query_paths",Some(7),&async_provider,&async_cases::NoClocks,&async_cases::NoClocks).await} else if mode==3 {transport.discover_capabilities_authenticated(&identity,"query_paths",Some(7),&provider,&source,READING).await} else if mode==2 {transport.discover_capabilities_negotiated(&identity,"query_paths",Some(7),&authentication).await} else if http2 { transport.discover_capabilities_http2(&identity,"query_paths",Some(7),&authentication).await } else { transport.discover_capabilities(
                        &identity,
                        "query_paths",
                        Some(7),
                        &authentication
                    ).await } },
                    peer
                )
            })
            .await
            .unwrap();
            assert_eq!(outcome.is_ok(), compatible);
        }
        }
        let arguments = r#"{"root_path":"/content/é"}"#;
        let submission = Submission::build(
            &expected,
            WireOperationIdentity::of(
                &identity.author_target_identity_digest,
                &identity.selected_environment_revision,
                &identity.operation_identifier,
                AgentEventStoreGeneration::of(7),
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
                &expected,
                submission.operation.clone(),
                "subscription-one",
                invalid,
                ExpectedArtifactManifest::empty(),
            )
            .unwrap();
            assert!(transport.require_submission(&identity, &invalid).is_err());
        }
        wrong.operation_identifier = "another-operation".to_owned();
        assert_eq!(
            transport
                .send_submission_with_fresh_token(&wrong, &submission, &authentication, 1)
                .await,
            Err(SubmissionSendRefusal::Identity)
        );
        let mut drifted = submission.clone();
        drifted.submitted_command_digest = "another-digest".to_owned();
        assert_eq!(
            transport
                .send_submission_with_fresh_token(&identity, &drifted, &authentication, 1)
                .await,
            Err(SubmissionSendRefusal::Derivation)
        );
        assert!(
            timeout(Duration::from_millis(10), listener.accept()).await.is_err(),
            "identity refusal opened a socket"
        );
        let answer = serde_json::json!({
            "provenance": submission.provenance,
            "selected_environment_revision": submission.operation.selected_environment_revision,
            "agent_event_store_generation": 7,
            "agent_operation_identifier": if wrong_echo { "another-operation" } else { &submission.operation.agent_operation_identifier },
            "author_target_identity_digest": identity.author_target_identity_digest,
            "already_accepted": false,
            "daemon_subscription_identifier": "subscription-one",
            "granted_retention_milliseconds": 120000,
            "physical_sling_job_identifiers": ["job-one"],
            "retired": false,
            "submitted_command_digest": submission.submitted_command_digest,
        }).to_string();
        let job_sets: Vec<serde_json::Value> = if media == "application/json" && !wrong_echo {
            serde_json::from_str::<serde_json::Value>(include_str!("fixtures/command-submission/job-sets.json"))
                .unwrap()["sets"].as_array().unwrap().clone()
        } else {vec![serde_json::json!({"name":"baseline", "identifiers":["job-one"], "acceptable":true})]};
        for asynchronous in [false,true] {
        for job_set in &job_sets {
        let mut wire_answer: serde_json::Value = serde_json::from_str(&answer).unwrap();
        wire_answer["physical_sling_job_identifiers"] = job_set["identifiers"].clone();
        let answer = wire_answer.to_string();
        let expected_recorded = accepted && job_set["acceptable"].as_bool().unwrap();
        let peer = async {
            let (mut socket, _) = listener.accept().await.unwrap();
            let mut head = Vec::new();
            while !head.ends_with(b"\r\n\r\n") {
                head.push(socket.read_u8().await.unwrap());
            }
            let head = String::from_utf8(head).unwrap();
            assert!(head.starts_with("GET /aem/libs/granite/csrf/token.json HTTP/1.1\r\n"));
            assert!(head.contains("Authorization: Basic "));
            assert!(!head.to_ascii_lowercase().contains("csrf-token:"));
            let body = r#"{"token":"csrf-test-value"}"#;
            socket.write_all(format!("HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n{body}", body.len()).as_bytes()).await.unwrap();
            drop(socket);
            let (mut socket, _) = listener.accept().await.unwrap();
            let mut head = Vec::new();
            while !head.ends_with(b"\r\n\r\n") {
                head.push(socket.read_u8().await.unwrap());
            }
            let head = String::from_utf8(head).unwrap();
            assert!(head.starts_with("POST /aem/bin/slingshot-agent/jobs HTTP/1.1\r\n"));
            assert!(head.contains(&format!("referer: {endpoint}/\r\n")));
            assert!(head.contains("csrf-token: csrf-test-value\r\n"));
            assert!(head.contains(&format!(
                "idempotency-key: {}\r\n",
                submission.operation.agent_operation_identifier
            )));
            assert!(head.contains("Authorization: Basic "));
            let length: usize = head
                .lines()
                .find_map(|line| line.strip_prefix("Content-Length: "))
                .unwrap()
                .parse()
                .unwrap();
            let mut body = vec![0; length];
            socket.read_exact(&mut body).await.unwrap();
            let decoded: serde_json::Value = serde_json::from_slice(&body).unwrap();
            assert_eq!(decoded["canonical_arguments"].as_str(), Some(arguments));
            assert_eq!(decoded["artifact_manifest"]["kind"], "empty");
            let response = format!(
                "HTTP/1.1 202 Accepted\r\nContent-Type: {media}\r\nContent-Length: {}\r\n\r\n{answer}",
                answer.len()
            );
            socket.write_all(response.as_bytes()).await.unwrap();
        };
        let (outcome, ()) = timeout(Duration::from_secs(5), async {
            tokio::join!(
                async {if asynchronous {
                    transport.send_submission_authenticated_async_guarded(
                        &identity,&submission,&async_provider,&async_cases::NoClocks,&async_cases::NoClocks,1,|| Ok(())
                    ).await
                } else {transport.send_submission_with_fresh_token(
                    &identity,
                    &submission,
                    &authentication,
                    1
                ).await}},
                peer
            )
        })
        .await
        .unwrap();
        let outcome = outcome.unwrap();
        assert_eq!(outcome.provably_recorded(), expected_recorded, "job set {:?}, async={asynchronous}", job_set["name"]);
        if !expected_recorded {
            assert!(matches!(outcome, SubmissionOutcome::SubmissionUnknown { .. }));
        }
        assert!(timeout(Duration::from_millis(10),listener.accept()).await.is_err(), "acknowledgement triggered another request");
        }
        }
        let oversized_token = serde_json::json!({"token": "a".repeat(
            AuthorAgentTransportContract::embedded().limit("maximum_author_response_header_bytes") as usize + 1
        )}).to_string();
        for automatic in [false,true] {
        let endpoint=if automatic {format!("https://{}",listener.local_addr().unwrap())} else {endpoint.clone()};
        let mut files=profile_files();
        replace_profile(&mut files,"profiles/mike.toml",|text|text.replace("http://author.example.com",&format!("{endpoint}/aem")).replace("allow_insecure_author_transport = true\n",""));
        use rustls_pki_types::{CertificateDer,PrivateKeyDer,pem::PemObject};
        let root=CertificateDer::from_pem_slice(include_bytes!("fixtures/selected-author-tls/root.pem")).unwrap();
        let platform=PlatformTrustSnapshot::take(&ScriptedStore {records:vec![ProviderRecord {der:root.as_ref().to_vec(),decision:ProviderDecision::UnconditionallyTrustedForServerAuthentication}]}).unwrap();
        let provider=provider_from_loaded_with_platform(loaded_from_files(files),CLEARTEXT_PROFILE,CLEARTEXT_ENVIRONMENT,platform);
        let transport=SelectedAuthorTransport::new(provider.snapshot().author_connection()).unwrap();
        let (authentication,_)=provider.authenticate(provider.snapshot().author().as_text(),READING,&source).unwrap();
        let identity=ExecutionIdentity {author_target_identity_digest:provider.snapshot().target().to_string(),selected_environment_revision:provider.snapshot().revision().to_string(),..identity.clone()};
        let submission=Submission::build(&expected,WireOperationIdentity::of(&identity.author_target_identity_digest,&identity.selected_environment_revision,&identity.operation_identifier,AgentEventStoreGeneration::of(7)),"subscription-one",arguments,ExpectedArtifactManifest::empty()).unwrap();
        let mut response:serde_json::Value=serde_json::from_str(&answer).unwrap();
        response["author_target_identity_digest"]=identity.author_target_identity_digest.clone().into();
        response["selected_environment_revision"]=identity.selected_environment_revision.clone().into();
        response["agent_operation_identifier"]=if wrong_echo {"wrong-operation".into()} else {submission.operation.agent_operation_identifier.clone().into()};
        response["submitted_command_digest"]=submission.submitted_command_digest.clone().into();
        let answer=response.to_string();
        let snapshot=serde_json::json!({
            "provenance":submission.provenance,"agent_event_store_generation":7,
            "agent_operation_identifier":submission.operation.agent_operation_identifier,
            "author_target_identity_digest":identity.author_target_identity_digest,
            "selected_environment_revision":identity.selected_environment_revision,
            "daemon_subscription_identifier":"subscription-one",
            "submitted_command_digest":submission.submitted_command_digest,
            "subscription_watermark":"cursor-010","physical_sling_job_identifiers":["job-one"],
            "granted_retention_milliseconds":120000,"attempt":1,"progress":10,"sequence":2,"kind":"progress",
        }).to_string();
        let capability=serde_json::json!({
            "format":"slingshot.agent/1","agent_event_store_generation":7,
            "canonical_json_contract_digest":expected.canonical_json_contract_digest,
            "transport_contract_digest":expected.transport_contract_digest,
            "command_contracts":[slingshot_agent_protocol::identity::WireContractIdentity::from(&expected.command_contract)],
            "continuation_authority_ready":true,
        }).to_string();
        for defect in ["", "token", "guard", "truncated"] {
            let peer = async {
                for stage in (if automatic {-1} else {0})..(if automatic && defect.is_empty() {3} else {2}) {
                    if stage == 1 && ["token", "guard"].contains(&defect) { break; }
                    let (socket, _) = listener.accept().await.unwrap();
                    trait Peer:tokio::io::AsyncRead+tokio::io::AsyncWrite+Unpin {}
                    impl<T:tokio::io::AsyncRead+tokio::io::AsyncWrite+Unpin> Peer for T {}
                    let mut socket:Box<dyn Peer>=if automatic {
                        let mut configuration=rustls::ServerConfig::builder_with_provider(Arc::new(rustls::crypto::ring::default_provider()))
                            .with_protocol_versions(&[&rustls::version::TLS13]).unwrap().with_no_client_auth().with_single_cert(
                                vec![CertificateDer::from_pem_slice(include_bytes!("fixtures/selected-author-tls/leaf.pem")).unwrap()],
                                PrivateKeyDer::from_pem_slice(include_bytes!("fixtures/selected-author-tls/test-only-private-key.pem")).unwrap()).unwrap();
                        configuration.alpn_protocols=vec![b"h2".to_vec(),b"http/1.1".to_vec()];
                        let socket=tokio_rustls::TlsAcceptor::from(Arc::new(configuration)).accept(socket).await.unwrap();
                        assert_eq!(socket.get_ref().1.alpn_protocol(),Some(b"h2".as_slice())); Box::new(socket)
                    } else {Box::new(socket)};
                    let mut preface = [0;39]; socket.read_exact(&mut preface).await.unwrap();
                    assert_eq!(&preface[..24], b"PRI * HTTP/2.0\r\n\r\nSM\r\n\r\n");
                    socket.write_all(&[0,0,0,4,0,0,0,0,0]).await.unwrap();
                    let mut frame = [0;9]; socket.read_exact(&mut frame).await.unwrap();
                    assert_eq!(frame, [0,0,0,4,1,0,0,0,0]); socket.write_all(&frame).await.unwrap();
                    socket.read_exact(&mut frame).await.unwrap();
                    assert_eq!((frame[3],frame[4]), (1,if stage == 1 {4} else {5}));
                    let length = usize::from(frame[0]) << 16 | usize::from(frame[1]) << 8 | usize::from(frame[2]);
                    assert!(length <= 16384); let mut head=vec![0;length]; socket.read_exact(&mut head).await.unwrap();
                    let lookup_route=format!("/aem/bin/slingshot-agent/operations/lookup?agent_operation_identifier={}",submission.operation.agent_operation_identifier);
                    let route = if stage==2 {lookup_route.as_bytes()} else if stage < 0 {b"/aem/bin/slingshot-agent/capabilities".as_slice()} else if stage == 0 {b"/aem/libs/granite/csrf/token.json".as_slice()} else {b"/aem/bin/slingshot-agent/jobs".as_slice()};
                    assert!(head.windows(route.len()).any(|part|part==route));
                    authentication.lend_value_bytes(|value|assert!(head.windows(value.len()).any(|part|part==value)));
                    if stage != 1 { assert!(!head.windows(10).any(|part|part==b"csrf-token")); }
                    else {
                        for value in ["csrf-test-value", submission.operation.agent_operation_identifier.as_str(), &format!("{endpoint}/")] {
                            assert!(head.windows(value.len()).any(|part|part==value.as_bytes()));
                        }
                        let mut body=Vec::new();
                        loop {
                            socket.read_exact(&mut frame).await.unwrap(); assert_eq!(frame[3],0);
                            let length=usize::from(frame[0])<<16|usize::from(frame[1])<<8|usize::from(frame[2]);
                            assert!(length<=16384); let start=body.len(); body.resize(start+length,0); socket.read_exact(&mut body[start..]).await.unwrap();
                            if frame[4]&1!=0 {break;}
                        }
                        assert_eq!(body,submission.wire_body().unwrap());
                    }
                    let body=if stage==2 {snapshot.as_str()} else if stage<0 {capability.as_str()} else if stage==0 {if defect=="token" {r#"{"token":""}"#} else {r#"{"token":"csrf-test-value"}"#}} else {answer.as_str()};
                    let media=if stage!=1 {"application/json"} else {media};
                    let mut block=if stage!=1 {vec![0x88]} else {vec![0x08,3,b'2',b'0',b'2']};
                    for (name,value) in [("content-type",media.to_owned()),("content-length",body.len().to_string())] {
                        block.extend_from_slice(&[0,name.len() as u8]); block.extend_from_slice(name.as_bytes()); block.push(value.len() as u8); block.extend_from_slice(value.as_bytes());
                    }
                    let bytes=if stage==1 && defect=="truncated" {&body.as_bytes()[..body.len()-1]} else {body.as_bytes()};
                    for (kind,flags,bytes) in [(1,4,block.as_slice()),(0,1,bytes)] {
                        let length=(bytes.len() as u32).to_be_bytes(); socket.write_all(&[length[1],length[2],length[3],kind,flags,0,0,0,1]).await.unwrap(); socket.write_all(bytes).await.unwrap();
                    }
                    let mut close=Vec::new(); let _=socket.read_to_end(&mut close).await;
                    if automatic {socket.shutdown().await.unwrap();}
                }
            };
            let mut guarded=false;
            let (outcome,())=timeout(Duration::from_secs(5),async {tokio::join!(
                async {
                if automatic {transport.discover_capabilities_negotiated(&identity,"query_paths",Some(7),&authentication).await.unwrap();}
                let guard=|| {
                    guarded=true; if defect=="guard" {Err(SubmissionSendRefusal::Identity)} else {Ok(())}
                }; let outcome=if automatic {transport.send_submission_with_fresh_token_negotiated_guarded(&identity,&submission,&authentication,1,guard).await}
                else {transport.send_submission_with_fresh_token_http2_guarded(&identity,&submission,&authentication,1,guard).await};
                if automatic && defect.is_empty() {
                    let slingshot_agent_connection::selected_author_lookup::OperationLookupReceipt::Found(receipt)=transport.lookup_operation_negotiated(&identity,&submission,&authentication).await.unwrap() else {panic!("negotiated snapshot missing");};
                    assert_eq!(receipt.snapshot.progress,10); assert_eq!(receipt.snapshot.sequence.value(),2);
                    assert_eq!(receipt.snapshot.physical_sling_job_identifiers,["job-one"]);
                    assert!(receipt.remaining_retention_milliseconds>0 && receipt.remaining_retention_milliseconds<=120000);
                }
                outcome},peer
            )}).await.unwrap();
            assert_eq!(guarded,defect!="token");
            if defect=="guard" {assert_eq!(outcome,Err(SubmissionSendRefusal::Identity));}
            else if defect=="token" {assert_eq!(outcome,Err(SubmissionSendRefusal::Request));}
            else {
                let outcome=outcome.unwrap(); assert_eq!(outcome.provably_recorded(),accepted && defect.is_empty());
                if defect=="truncated" {assert!(matches!(outcome,SubmissionOutcome::SubmissionUnknown {..}));}
            }
            assert!(timeout(Duration::from_millis(10),listener.accept()).await.is_err(),"HTTP/2 submission retried or fell back");
        }
        }
        for body in [
            r#"{}"#,
            r#"{"token":""}"#,
            r#"{"token":"one","token":"two"}"#,
            r#"{"token":"one","extra":true}"#,
            r#"{"token":"a\r\nb"}"#,
            oversized_token.as_str(),
        ] {
            let peer = async {
                let (mut socket, _) = listener.accept().await.unwrap();
                let mut head = Vec::new();
                while !head.ends_with(b"\r\n\r\n") {
                    head.push(socket.read_u8().await.unwrap());
                }
                assert!(
                    String::from_utf8(head)
                        .unwrap()
                        .starts_with("GET /aem/libs/granite/csrf/token.json ")
                );
                socket.write_all(format!("HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n{body}", body.len()).as_bytes()).await.unwrap();
            };
            let (outcome, ()) = timeout(Duration::from_secs(5), async {
                tokio::join!(
                    transport.send_submission_with_fresh_token(
                        &identity,
                        &submission,
                        &authentication,
                        1
                    ),
                    peer
                )
            })
            .await
            .unwrap();
            assert_eq!(outcome, Err(SubmissionSendRefusal::Request));
            assert!(
                timeout(Duration::from_millis(10), listener.accept()).await.is_err(),
                "invalid token caused a POST"
            );
        }
        if accepted {
            let snapshot_body = serde_json::json!({
                "provenance": submission.provenance,
                "agent_event_store_generation": submission.operation.agent_event_store_generation,
                "agent_operation_identifier": submission.operation.agent_operation_identifier,
                "author_target_identity_digest": submission.operation.author_target_identity_digest,
                "selected_environment_revision": submission.operation.selected_environment_revision,
                "daemon_subscription_identifier": submission.daemon_subscription_identifier,
                "submitted_command_digest": submission.submitted_command_digest,
                "subscription_watermark":"cursor-010", "physical_sling_job_identifiers": ["job-one"],
                "granted_retention_milliseconds": 120000,
                "attempt": 1, "progress": 10, "sequence": 2, "kind": "progress"
            })
            .to_string();
            let peer = async {
                let (mut socket, _) = listener.accept().await.unwrap();
                let mut bytes = Vec::new();
                while !bytes.ends_with(b"\r\n\r\n") {
                    bytes.push(socket.read_u8().await.unwrap());
                }
                let prefix = format!(
                    "GET /aem/bin/slingshot-agent/operations/lookup?agent_operation_identifier={} HTTP/1.1\r\n",
                    submission.operation.agent_operation_identifier
                );
                assert!(bytes.starts_with(prefix.as_bytes()));
                let response = format!(
                    "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n{snapshot_body}",
                    snapshot_body.len()
                );
                socket.write_all(response.as_bytes()).await.unwrap();
            };
            let (snapshot, ()) = timeout(Duration::from_secs(5), async {
                tokio::join!(
                    transport.lookup_snapshot(&identity, &submission, &authentication),
                    peer
                )
            })
            .await
            .unwrap();
            let receipt = snapshot.unwrap();
            assert!(receipt.remaining_retention_milliseconds <= 120000);
            let snapshot = receipt.snapshot;
            assert_eq!(snapshot.progress, 10);
            assert_eq!(snapshot.sequence.value(), 2);
            assert_eq!(snapshot.granted_retention_milliseconds, 120000);
            for mode in [0,1,2,3,4] {
                let http2=mode==1;
                for invalid in [String::new(), "x".repeat(1025)] {
                    let result = if mode==4 {transport.lookup_physical_snapshot_authenticated_async(&identity,&submission,&invalid,&async_provider,&async_cases::NoClocks,&async_cases::NoClocks).await} else if mode==3 {transport.lookup_physical_snapshot_authenticated(&identity,&submission,&invalid,&provider,&source,READING).await} else if mode==2 {transport.lookup_physical_snapshot_negotiated(&identity,&submission,&invalid,&authentication).await} else if http2 { transport.lookup_physical_snapshot_http2(&identity, &submission, &invalid, &authentication).await }
                        else { transport.lookup_physical_snapshot(&identity, &submission, &invalid, &authentication).await };
                    assert!(result.is_err());
                }
                let mut moved = identity.clone(); moved.selected_environment_revision = "other".into();
                let refused = if mode==4 {transport.lookup_physical_snapshot_authenticated_async(&moved,&submission,"job /?é",&async_provider,&async_cases::NoClocks,&async_cases::NoClocks).await} else if mode==3 {transport.lookup_physical_snapshot_authenticated(&moved,&submission,"job /?é",&provider,&source,READING).await} else if mode==2 {transport.lookup_physical_snapshot_negotiated(&moved,&submission,"job /?é",&authentication).await} else if http2 { transport.lookup_physical_snapshot_http2(&moved, &submission, "job /?é", &authentication).await }
                    else { transport.lookup_physical_snapshot(&moved, &submission, "job /?é", &authentication).await };
                assert!(refused.is_err());
                let zero_generation = if mode==4 {transport.lookup_physical_job_authenticated_async(&identity,&submission,"job /?é",0,&async_provider,&async_cases::NoClocks,&async_cases::NoClocks).await} else if mode==3 {transport.lookup_physical_job_authenticated(&identity,&submission,"job /?é",0,&provider,&source,READING).await} else if mode==2 {transport.lookup_physical_job_negotiated(&identity,&submission,"job /?é",0,&authentication).await} else if http2 { transport.lookup_physical_job_http2(&identity, &submission, "job /?é", 0, &authentication).await }
                    else { transport.lookup_physical_job(&identity, &submission, "job /?é", 0, &authentication).await };
                assert!(zero_generation.is_err());
                assert!(timeout(Duration::from_millis(10), listener.accept()).await.is_err());
                for (status, defect) in [(200, ""), (200, "physical"), (200, "digest"), (200, "truncated"), (404, "logical"), (410, "logical"), (404, "missing"), (404, "missing-generation"), (404, "missing-identifier"), (404, "missing-truncated")] {
                    let mut body: serde_json::Value = serde_json::from_str(&snapshot_body).unwrap();
                    body["physical_sling_job_identifiers"] = serde_json::json!([if defect == "physical" {"other-job"} else {"job /?é"}]);
                    if defect == "digest" { body["submitted_command_digest"] = serde_json::json!("0".repeat(64)); }
                    if status == 404 {
                        body = serde_json::json!({ "kind":"missing", "format":"slingshot.agent/1", "transport_contract_digest":submission.provenance.transport_contract_digest,
                            "agent_event_store_generation":submission.operation.agent_event_store_generation, "agent_operation_identifier":submission.operation.agent_operation_identifier,
                            "author_target_identity_digest":submission.operation.author_target_identity_digest });
                        if defect.starts_with("missing") {
                            body = serde_json::json!({"kind":"missing", "format":"slingshot.agent/1", "transport_contract_digest":submission.provenance.transport_contract_digest,
                                "agent_event_store_generation":if defect == "missing-generation" {9} else {8},
                                "sling_job_identifier":if defect == "missing-identifier" {"other-job"} else {"job /?é"}});
                        }
                    } else if status == 410 {
                        body = serde_json::json!({ "kind":"retired", "provenance":submission.provenance,
                            "agent_event_store_generation":submission.operation.agent_event_store_generation, "agent_operation_identifier":submission.operation.agent_operation_identifier,
                            "author_target_identity_digest":submission.operation.author_target_identity_digest, "selected_environment_revision":submission.operation.selected_environment_revision,
                            "daemon_subscription_identifier":submission.daemon_subscription_identifier, "submitted_command_digest":submission.submitted_command_digest });
                    }
                    let body = serde_json::to_vec(&body).unwrap();
                    let peer = async {
                        let (mut socket, _) = listener.accept().await.unwrap(); let mut request = Vec::new();
                        if http2 {
                            let mut preface = [0; 39]; socket.read_exact(&mut preface).await.unwrap();
                            assert_eq!(&preface[..24], b"PRI * HTTP/2.0\r\n\r\nSM\r\n\r\n");
                            socket.write_all(&[0,0,0,4,0,0,0,0,0]).await.unwrap();
                            let mut header = [0; 9]; socket.read_exact(&mut header).await.unwrap();
                            socket.write_all(&header).await.unwrap(); socket.read_exact(&mut header).await.unwrap();
                            assert_eq!((header[3], header[4]), (1, 5));
                            let length = usize::from(header[0]) << 16 | usize::from(header[1]) << 8 | usize::from(header[2]);
                            assert!(length <= 16384); request.resize(length, 0); socket.read_exact(&mut request).await.unwrap();
                        } else {
                            while !request.ends_with(b"\r\n\r\n") { request.push(socket.read_u8().await.unwrap()); assert!(request.len() <= 8192); }
                            assert!(request.starts_with(b"GET "));
                        }
                        let route = b"/aem/bin/slingshot-agent/jobs/snapshot?sling_job_identifier=job%20%2F%3F%C3%A9";
                        assert!(request.windows(route.len()).any(|bytes| bytes == route));
                        authentication.lend_value_bytes(|value| assert!(request.windows(value.len()).any(|bytes| bytes == value)));
                        let payload = if defect.ends_with("truncated") { &body[..body.len()-1] } else { &body };
                        if http2 {
                            let mut block = vec![0, 7]; block.extend_from_slice(b":status"); block.push(3); block.extend_from_slice(status.to_string().as_bytes());
                            for (name, value) in [("content-type", "application/json".to_owned()), ("content-length", body.len().to_string())] {
                                block.extend_from_slice(&[0, name.len() as u8]); block.extend_from_slice(name.as_bytes()); block.push(value.len() as u8); block.extend_from_slice(value.as_bytes());
                            }
                            for (kind, flags, bytes) in [(1, 4, block.as_slice()), (0, 1, payload)] {
                                let length = (bytes.len() as u32).to_be_bytes();
                                socket.write_all(&[length[1], length[2], length[3], kind, flags, 0,0,0,1]).await.unwrap(); socket.write_all(bytes).await.unwrap();
                            }
                            let mut close = Vec::new(); let _ = socket.read_to_end(&mut close).await;
                        } else {
                            socket.write_all(format!("HTTP/1.1 {status} Snapshot\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n", body.len()).as_bytes()).await.unwrap();
                            socket.write_all(payload).await.unwrap();
                        }
                        socket.shutdown().await.unwrap();
                    };
                    let request = async {
                        if mode==4 {transport.lookup_physical_job_authenticated_async(&identity,&submission,"job /?é",8,&async_provider,&async_cases::NoClocks,&async_cases::NoClocks).await} else if mode==3 {transport.lookup_physical_job_authenticated(&identity,&submission,"job /?é",8,&provider,&source,READING).await} else if mode==2 {transport.lookup_physical_job_negotiated(&identity,&submission,"job /?é",8,&authentication).await} else if http2 { transport.lookup_physical_job_http2(&identity, &submission, "job /?é", 8, &authentication).await }
                        else { transport.lookup_physical_job(&identity, &submission, "job /?é", 8, &authentication).await }
                    };
                    let (result, ()) = timeout(Duration::from_secs(5), async { tokio::join!(request, peer) }).await.unwrap();
                    assert_eq!(result.is_ok(), defect.is_empty() || defect == "missing", "http2={http2} status={status} defect={defect}");
                    if let Ok(receipt) = result {
                        match receipt {
                            slingshot_agent_connection::selected_author_lookup::PhysicalLookupReceipt::Found(receipt) => {
                                assert_eq!(status, 200);
                                assert_eq!(receipt.snapshot.physical_sling_job_identifiers, ["job /?é"]);
                                assert_eq!(receipt.snapshot.echo.agent_event_store_generation, submission.operation.agent_event_store_generation);
                                assert!(receipt.remaining_retention_milliseconds > 0 && receipt.remaining_retention_milliseconds <= 120000);
                            },
                            slingshot_agent_connection::selected_author_lookup::PhysicalLookupReceipt::Missing(proof) => {
                                assert_eq!(defect, "missing"); assert_eq!(proof.generation(), 8); assert_eq!(proof.sling_job_identifier(), "job /?é");
                            },
                        }
                    }
                }
            }
            for automatic in [0,1,2,3] {
            for (status, kind) in [(404, "missing"), (410, "retired")] {
                let mut document = serde_json::json!({
                    "kind": kind,
                    "agent_event_store_generation": submission.operation.agent_event_store_generation,
                    "agent_operation_identifier": submission.operation.agent_operation_identifier,
                    "author_target_identity_digest": submission.operation.author_target_identity_digest,
                });
                if status == 404 {
                    document["format"] = serde_json::json!("slingshot.agent/1");
                    document["transport_contract_digest"] =
                        serde_json::json!(submission.provenance.transport_contract_digest);
                } else {
                    document["provenance"] = serde_json::json!(submission.provenance);
                    document["selected_environment_revision"] =
                        serde_json::json!(submission.operation.selected_environment_revision);
                    document["daemon_subscription_identifier"] =
                        serde_json::json!(submission.daemon_subscription_identifier);
                    document["submitted_command_digest"] =
                        serde_json::json!(submission.submitted_command_digest);
                }
                let body = document.to_string();
                let peer = async {
                    let (mut socket, _) = listener.accept().await.unwrap();
                    let mut bytes = Vec::new();
                    while !bytes.ends_with(b"\r\n\r\n") {
                        bytes.push(socket.read_u8().await.unwrap());
                    }
                    assert!(bytes.starts_with(
                        b"GET /aem/bin/slingshot-agent/operations/lookup?agent_operation_identifier="
                    ));
                    let response = format!(
                        "HTTP/1.1 {status} Absent\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n{body}",
                        body.len()
                    );
                    socket.write_all(response.as_bytes()).await.unwrap();
                };
                let (receipt, ()) = timeout(Duration::from_secs(5), async {
                    tokio::join!(
                        async {if automatic==3 {transport.lookup_operation_authenticated_async(&identity,&submission,&async_provider,&async_cases::NoClocks,&async_cases::NoClocks).await} else if automatic==2 {transport.lookup_operation_authenticated(&identity,&submission,&provider,&source,READING).await} else if automatic==1 {transport.lookup_operation_negotiated(&identity,&submission,&authentication).await} else {transport.lookup_operation(&identity, &submission, &authentication).await}},
                        peer
                    )
                })
                .await
                .unwrap();
                use slingshot_agent_connection::{
                    job_snapshot_reconciliation::LookupAnswer,
                    selected_author_lookup::OperationLookupReceipt,
                };
                assert!(matches!(
                    (status, receipt.unwrap()),
                    (404, OperationLookupReceipt::Absent(LookupAnswer::Missing))
                        | (410, OperationLookupReceipt::Absent(LookupAnswer::Retired(_)))
                ));
            }
            }
        }
        assert!(
            timeout(Duration::from_millis(10), listener.accept()).await.is_err(),
            "a second POST was attempted"
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
    let route = &["bin", "slingshot-agent", "operations", "lookup"];
    let huge = "x".repeat(8193);
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
        assert!(bytes.starts_with(b"GET /aem/bin/slingshot-agent/operations/lookup?agent_operation_identifier=a%20%26%252F%C3%A9 HTTP/1.1\r\n"));
        stream
            .write_all(
                b"HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: 2\r\n\r\n{}",
            )
            .await
            .unwrap();
    };
    let query = [("agent_operation_identifier", "a &%2Fé")];
    let (result, ()) = timeout(Duration::from_secs(5), async {
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
            AgentEventStoreGeneration::of(7),
        ),
        "subscription-one",
        r#"{"root_path":"/content"}"#,
        ExpectedArtifactManifest::empty(),
    )
    .unwrap();
    let mut expected = ExpectedArtifact {
        artifact_digest: Sha256::digest(b"abc").iter().map(|byte| format!("{byte:02x}")).collect(),
        artifact_slot: "content_package".to_owned(),
        byte_length: 3,
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
                "GET /aem/bin/slingshot-agent/operations/{}/artifacts/content_package HTTP/1.1\r\n",
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
        let (result, ()) = timeout(Duration::from_secs(5), async {
            tokio::join!(
                transport.stream_artifact_http1(
                    &identity,
                    &submission,
                    &expected,
                    &authentication,
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
            assert_eq!(result.unwrap().byte_length(), 3);
            assert_eq!(staged, b"abc");
        }
    }
    let artifact_identifier = "3".repeat(64);
    assert!(transport.artifact_http2(&wrong, &submission, &expected, &artifact_identifier, &authentication,
        |_| panic!("invalid HTTP/2 identity streamed")).await.is_err());
    assert!(transport.artifact_negotiated(&wrong,&submission,&expected,&artifact_identifier,&authentication,
        |_| panic!("invalid negotiated identity streamed")).await.is_err());
    assert!(transport.artifact_negotiated(&identity,&submission,&expected,"",&authentication,
        |_| panic!("invalid negotiated artifact identifier streamed")).await.is_err());
    assert!(timeout(Duration::from_millis(10), listener.accept()).await.is_err());
    for mode in 0..6 {
    let automatic=mode!=0;
    let endpoint=format!("{}://{}/aem",if mode==1 || mode==2 {"https"} else {"http"},listener.local_addr().unwrap());
    let mut files=profile_files();
    replace_profile(&mut files,"profiles/mike.toml",|text|text.replace("http://author.example.com",&endpoint).replace("allow_insecure_author_transport = true\n",""));
    use rustls_pki_types::{CertificateDer,PrivateKeyDer,pem::PemObject};
    let root=CertificateDer::from_pem_slice(include_bytes!("fixtures/selected-author-tls/root.pem")).unwrap();
    let platform=PlatformTrustSnapshot::take(&ScriptedStore {records:vec![ProviderRecord {der:root.as_ref().to_vec(),decision:ProviderDecision::UnconditionallyTrustedForServerAuthentication}]}).unwrap();
    let async_provider=slingshot_agent_connection::authentication::environment_provider::AsyncEnvironmentAuthenticationProvider::new_async(
        snapshot_from_loaded_with_platform(loaded_from_files(files.clone()),CLEARTEXT_PROFILE,CLEARTEXT_ENVIRONMENT,platform.clone())).unwrap();
    let provider=provider_from_loaded_with_platform(loaded_from_files(files),CLEARTEXT_PROFILE,CLEARTEXT_ENVIRONMENT,platform);
    let transport=SelectedAuthorTransport::new(provider.snapshot().author_connection()).unwrap();
    let (authentication,_)=provider.authenticate(&endpoint,READING,&source).unwrap();
    let identity=ExecutionIdentity {author_target_identity_digest:provider.snapshot().target().to_string(),selected_environment_revision:provider.snapshot().revision().to_string(),..identity.clone()};
    let submission=Submission::build(&provenance,WireOperationIdentity::of(&identity.author_target_identity_digest,&identity.selected_environment_revision,&identity.operation_identifier,AgentEventStoreGeneration::of(7)),"subscription-one",r#"{"root_path":"/content"}"#,ExpectedArtifactManifest::empty()).unwrap();
    for (status, defect, accepted) in [
        (200, "", true), (200, "digest", false), (200, "trailer", false), (200, "sink", false),
        (404, "", true), (410, "", true), (410, "identity", false), (404, "bare", false),
        (401, "bare", true), (401, "trailer", false), (401, "short", false),
    ] {
        let body = if status == 200 {
            if defect == "digest" { b"abd".to_vec() } else { b"abc".to_vec() }
        } else if defect == "bare" { b"{}".to_vec() } else {
            serde_json::json!({
                "provenance":submission.provenance, "agent_event_store_generation":7,
                "agent_operation_identifier":submission.operation.agent_operation_identifier,
                "artifact_identifier":if defect == "identity" { "4".repeat(64) } else { artifact_identifier.clone() },
                "artifact_slot":"content_package", "reason":if status == 404 { "missing" } else { "retention_expired" },
            }).to_string().into_bytes()
        };
        let peer = async {
            let (stream, _) = listener.accept().await.unwrap();
            trait Peer:tokio::io::AsyncRead+tokio::io::AsyncWrite+Unpin {}
            impl<T:tokio::io::AsyncRead+tokio::io::AsyncWrite+Unpin> Peer for T {}
            let mut stream:Box<dyn Peer>=if mode==1 || mode==2 {
                let version=if mode==1 {&rustls::version::TLS12} else {&rustls::version::TLS13};
                let mut config=rustls::ServerConfig::builder_with_provider(Arc::new(rustls::crypto::ring::default_provider()))
                    .with_protocol_versions(&[version]).unwrap().with_no_client_auth().with_single_cert(
                        vec![CertificateDer::from_pem_slice(include_bytes!("fixtures/selected-author-tls/leaf.pem")).unwrap()],
                        PrivateKeyDer::from_pem_slice(include_bytes!("fixtures/selected-author-tls/test-only-private-key.pem")).unwrap()).unwrap();
                config.alpn_protocols=vec![b"h2".to_vec(),b"http/1.1".to_vec()];
                let stream=tokio_rustls::TlsAcceptor::from(Arc::new(config)).accept(stream).await.unwrap();
                assert_eq!(stream.get_ref().1.alpn_protocol(),Some(b"h2".as_slice())); Box::new(stream)
            } else {Box::new(stream)};
            if mode>=3 {
                let mut request=Vec::new(); while !request.ends_with(b"\r\n\r\n") {request.push(stream.read_u8().await.unwrap());}
                let route=format!("GET /aem/bin/slingshot-agent/operations/{}/artifacts/content_package HTTP/1.1\r\n",submission.operation.agent_operation_identifier);
                assert!(request.starts_with(route.as_bytes()));
                authentication.lend_value_bytes(|value|assert!(request.windows(value.len()).any(|part|part==value)));
                stream.write_all(format!("HTTP/1.1 {status} Artifact\r\nContent-Type: {}\r\nContent-Length: {}\r\n{}\r\n",if status==200 {"application/zip"} else {"application/json"},body.len()+usize::from(defect=="short"),if defect=="trailer" {"Trailer: x\r\n"} else {""}).as_bytes()).await.unwrap();
                stream.write_all(&body).await.unwrap(); stream.shutdown().await.unwrap(); return;
            }
            let mut preface = [0; 39];
            stream.read_exact(&mut preface).await.unwrap();
            assert_eq!(&preface[..24], b"PRI * HTTP/2.0\r\n\r\nSM\r\n\r\n");
            stream.write_all(&[0, 0, 0, 4, 0, 0, 0, 0, 0]).await.unwrap();
            let mut acknowledgement = [0; 9];
            stream.read_exact(&mut acknowledgement).await.unwrap();
            stream.write_all(&acknowledgement).await.unwrap();
            let mut header = [0; 9];
            stream.read_exact(&mut header).await.unwrap();
            assert_eq!((header[3], header[4]), (1, 5));
            let length = usize::from(header[0]) << 16 | usize::from(header[1]) << 8 | usize::from(header[2]);
            assert!(length <= 16384);
            let mut request = vec![0; length];
            stream.read_exact(&mut request).await.unwrap();
            let route = format!("/aem/bin/slingshot-agent/operations/{}/artifacts/content_package", submission.operation.agent_operation_identifier);
            assert!(request.windows(route.len()).any(|bytes| bytes == route.as_bytes()));
            authentication.lend_value_bytes(|value|assert!(request.windows(value.len()).any(|part|part==value)));
            let media = if status == 200 { "application/zip" } else { "application/json" };
            let mut block = Vec::new();
            for (name, value) in [(":status", status.to_string()), ("content-type", media.to_owned()), ("content-length", (body.len()+usize::from(defect=="short")).to_string())] {
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
            let mut wire = encode(1, 4, &block);
            wire.extend_from_slice(&encode(0, 1, &body));
            if defect == "trailer" { wire.extend_from_slice(&encode(1, 5, &[])); }
            stream.write_all(&wire).await.unwrap();
            let mut close = Vec::new();
            let _ = stream.read_to_end(&mut close).await;
            if accepted { assert_eq!(close, [0, 0, 8, 7, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0]); }
            stream.shutdown().await.unwrap();
        };
        let mut staged = Vec::new();
        let request = async {let sink=|bytes:&[u8]| {
            assert_eq!(status, 200, "unavailable document entered staging");
            if defect == "sink" { return Err(FiniteHttpFailure::Body); }
            staged.extend_from_slice(bytes); Ok(())
        }; if mode==5 {transport.artifact_authenticated_async(&identity,&submission,&expected,&artifact_identifier,&async_provider,&async_cases::NoClocks,&async_cases::NoClocks,sink).await} else if mode==4 {transport.artifact_authenticated(&identity,&submission,&expected,&artifact_identifier,&provider,&source,READING,sink).await} else if automatic {transport.artifact_negotiated(&identity,&submission,&expected,&artifact_identifier,&authentication,sink).await}
        else {transport.artifact_http2(&identity,&submission,&expected,&artifact_identifier,&authentication,sink).await}};
        let (result, ()) = timeout(Duration::from_secs(5), async { tokio::join!(request, peer) }).await.unwrap();
        assert_eq!(result.is_ok(), accepted && !(mode>=4 && status==401), "status={status} defect={defect}");
        assert!(timeout(Duration::from_millis(10),listener.accept()).await.is_err(),"artifact exchange retried or fell back");
        if let Ok(outcome) = result {
            use slingshot_agent_connection::selected_author_http::ArtifactHttpOutcome;
            match outcome {
                ArtifactHttpOutcome::Transferred(receipt) => { assert_eq!(receipt.byte_length(), 3); assert_eq!(staged, b"abc"); }
                ArtifactHttpOutcome::Unavailable { .. } => assert!(staged.is_empty()),
                ArtifactHttpOutcome::Unauthorized => {assert_eq!(status,401);assert!(staged.is_empty());},
            }
        }
    }
    }
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
            "artifact_identifier":if defect == "identity" { "4".repeat(64) } else { artifact_identifier.clone() },
            "artifact_slot":"content_package", "reason":reason,
        }).to_string();
        let peer = async {
            let (mut stream, _) = listener.accept().await.unwrap();
            let mut request = Vec::new();
            while !request.ends_with(b"\r\n\r\n") {
                request.push(stream.read_u8().await.unwrap());
            }
            let media = if defect == "media" { "text/html" } else { "application/json" };
            let mut framing = if chunked {
                "Transfer-Encoding: chunked".to_owned()
            } else {
                format!("Content-Length: {}", body.len())
            };
            if defect == "location" {
                framing.push_str("\r\nLocation: /elsewhere");
            }
            let payload = if defect == "short" { &body[..body.len() - 1] } else { body.as_str() };
            let wire_body = if chunked {
                format!(
                    "{:x}\r\n{payload}\r\n0\r\n{}\r\n",
                    payload.len(),
                    if defect == "trailer" { "X-Test: bad\r\n" } else { "" }
                )
            } else {
                format!("{payload}{}", if defect == "surplus" { "!" } else { "" })
            };
            let _ = stream.write_all(format!("HTTP/1.1 {status} Error\r\nContent-Type: {media}\r\n{framing}\r\n\r\n{wire_body}").as_bytes()).await;
        };
        let (result, ()) = timeout(Duration::from_secs(5), async {
            tokio::join!(
                transport.artifact_http1(
                    &identity,
                    &submission,
                    &expected,
                    &artifact_identifier,
                    &authentication,
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
    // A streamed artifact may exceed the finite JSON-body bound; neither peer
    // nor sink needs a whole-artifact allocation to exercise that distinction.
    let block = [b'x'; 8192];
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
        let (result, ()) = timeout(Duration::from_secs(5), async {
            tokio::join!(
                transport.stream_artifact_http1(
                    &identity,
                    &submission,
                    &expected,
                    &authentication,
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
    // Cancelling after a partial body drops the connection and cannot produce
    // a receipt. Private-file cleanup is the staging owner's responsibility.
    expected.byte_length = 3;
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
    timeout(Duration::from_secs(5), async {
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

/// Returns the password the committed installation profile carries.
fn rebuilt_password() -> slingshot_domain::secret_value::SecretValue {
    slingshot_domain::secret_value::SecretValue::from_text("not-a-real-password".to_owned())
}

#[test]
fn a_basic_environment_carries_the_exact_authorization_value() {
    let provider = provider(CLEARTEXT_PROFILE, CLEARTEXT_ENVIRONMENT);
    let source = CountingSource { exchanges: Cell::new(0) };
    let (authentication, lease) = provider
        .authenticate(provider.snapshot().author().as_text(), READING, &source)
        .expect("the author target authenticates");
    assert!(lease.is_none(), "a Basic environment leased a token");
    assert_eq!(source.exchanges.get(), 0, "a Basic environment exchanged");
    authentication.lend_value_bytes(|bytes| {
        let value = String::from_utf8_lossy(bytes);
        assert_eq!(value, "Basic YWRtaW46bm90LWEtcmVhbC1wYXNzd29yZA==", "{value}");
    });
    assert!(provider.snapshot().insecure_author_transport_warning().is_some());
}

#[test]
fn a_cloud_environment_leases_one_token_and_reuses_it() {
    let provider = provider(PROTECTED_PROFILE, PROTECTED_ENVIRONMENT);
    let source = CountingSource { exchanges: Cell::new(0) };
    let author = provider.snapshot().author().as_text().to_owned();
    let (authentication, lease) =
        provider.authenticate(&author, READING, &source).expect("the author authenticates");
    assert!(lease.is_some(), "a cloud environment leased nothing");
    authentication.lend_value_bytes(|bytes| {
        assert!(String::from_utf8_lossy(bytes).starts_with("Bearer "), "the scheme is wrong");
    });
    provider.authenticate(&author, READING, &source).expect("the author authenticates again");
    assert_eq!(source.exchanges.get(), 1, "a usable token was exchanged twice");
    assert!(provider.snapshot().insecure_author_transport_warning().is_none());
}

#[test]
fn every_target_that_is_not_the_author_is_refused_before_anything_is_asked() {
    let provider = provider(PROTECTED_PROFILE, PROTECTED_ENVIRONMENT);
    let source = CountingSource { exchanges: Cell::new(0) };
    let author = provider.snapshot().author().as_text().to_owned();
    for endpoint in [
        provider.snapshot().publisher_metadata().as_text().to_owned(),
        "https://unrelated.example.com".to_owned(),
        format!("{author}.evil.example.com"),
        format!("{author}extra"),
    ] {
        let refused = provider
            .authenticate(&endpoint, READING, &source)
            .map_or_else(|failure| failure.code, |_| panic!("{endpoint} was authenticated"));
        assert_eq!(refused, ConfigurationFailureCode::AuthenticationTargetMismatch, "{endpoint}");
    }
    assert_eq!(source.exchanges.get(), 0, "a refused target still exchanged");
    provider
        .authenticate(&provider.author_endpoint(&["bin", "querybuilder.json"]), READING, &source)
        .expect("an endpoint below the author authenticates");
}

#[test]
fn the_snapshot_never_reloads_and_never_dials_a_publisher() {
    let built = provider(PROTECTED_PROFILE, PROTECTED_ENVIRONMENT);
    let before = built.snapshot().revision();
    let again = provider(PROTECTED_PROFILE, PROTECTED_ENVIRONMENT);
    assert_eq!(again.snapshot().revision(), before, "one selection produced two revisions");
    assert_eq!(again.snapshot().target(), built.snapshot().target());
    assert_eq!(
        built.snapshot().deployment(),
        AdobeExperienceManagerDeployment::AdobeExperienceManagerCloudService
    );

    let source = std::fs::read_to_string(
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("src/authentication/environment_provider.rs"),
    )
    .expect("the module reads");
    let reloading: Vec<&str> = source
        .lines()
        .map(str::trim)
        .filter(|line| line.starts_with("pub fn "))
        .filter(|line| line.contains("reload") || line.contains("publisher_endpoint"))
        .collect();
    assert!(reloading.is_empty(), "the provider can reload or dial a publisher: {reloading:?}");
}

#[test]
fn no_rendering_of_the_authorization_value_carries_the_credential() {
    let provider = provider(CLEARTEXT_PROFILE, CLEARTEXT_ENVIRONMENT);
    let source = CountingSource { exchanges: Cell::new(0) };
    let (authentication, _) = provider
        .authenticate(provider.snapshot().author().as_text(), READING, &source)
        .expect("the author authenticates");
    let rendered = format!("{authentication:?}");
    assert!(!rendered.contains("not-a-real-password"), "{rendered}");
    assert!(!rendered.contains("YWRtaW4"), "{rendered}");
}
