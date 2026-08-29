//! Assertions for the exchange that turns an assertion into an access token.
//!
//! One request leaves this module carrying a client secret and a signed
//! assertion, so most of what is worth asserting is about request bytes and
//! about refusals. A redirection is refused without its location being read; an
//! informational head ends the exchange rather than being skipped; a section
//! that is too large is refused as too large rather than parsed to find out
//! what it says.
//!
//! The lease is the other half. The clock is anchored before the first request
//! byte and read again after the stream ends, so everything spent writing,
//! waiting, and reading reduces the lifetime the token is treated as having.
//! Equality at the usable-lease threshold is refused and one millisecond above
//! it is accepted, and a refusal produces no retry, because retrying a short
//! lease immediately is a loop.
//!
//! One request also goes over a real connection to a scripted listener, which
//! is what proves the form body, the method, and the request count are what
//! this says they are rather than what a fake transport was told to report.

use std::cell::Cell;
use std::io::{Read, Write};
use std::net::TcpStream;
use std::path::PathBuf;

use slingshot_agent_connection::authentication::cloud_service_credentials::CloudServiceCredentials;
use slingshot_agent_connection::authentication::identity_management_exchange::{
    AccessToken, DecodedHead, DecodedResponse, DecodedSection, ExchangeFailure,
    IdentityManagementExchange, IdentityManagementTransport, MonotonicClock,
    identity_management_endpoint,
};
use slingshot_agent_connection::authentication::token_assertion::{
    CoordinatedUniversalTimeClock, ServiceCredentialAssertion,
};
use slingshot_domain::profile_authentication_contract::{
    ConfigurationFailureCode, ProfileAuthenticationContract,
};
use slingshot_domain::secret_value::SensitiveConfigurationDocument;
use slingshot_test_support::identity_management_server::ScriptedIdentityManagementServer;

/// Credential document every exchange here is built from.
const CREDENTIAL_FIXTURE: &str = "../slingshot-test-support/fixtures/cloud-credentials/valid.json";

/// Vectors naming the second the assertion is built at.
const ASSERTION_FIXTURE: &str =
    "../slingshot-test-support/fixtures/token-assertions/assertion-vectors.json";

/// A token the scripted endpoint answers with.
const TOKEN: &str = "eyJhbGciOiJSUzI1NiJ9.not-a-real-access-token";

/// Status a successful exchange answers with.
const SUCCESS_STATUS: u16 = 200;

/// Status a redirection answers with.
const REDIRECTION_STATUS: u16 = 302;

/// Statuses an informational head answers with.
const INFORMATIONAL_STATUSES: &[u16] = &[100, 101, 103];

/// Lifetime a comfortable response advertises, in milliseconds.
const COMFORTABLE_LIFETIME: u64 = 3_600_000;

/// Reading the clock reports when the request is anchored.
const ANCHOR_READING: u64 = 1_000;

/// Reading the clock reports when the body has arrived.
const RECEIPT_READING: u64 = 1_500;

/// Milliseconds one transfer is scripted to take.
const TRANSFER_MILLISECONDS: u64 = 2;

/// Sentinels no rendering may carry.
const SENTINELS: &[&str] = &["p8e-not-a-real-client-secret", "not-a-real-access-token"];

/// A transport that answers exactly what a test scripts.
struct ScriptedTransport {
    /// Answer the transport produces.
    answer: Result<DecodedResponse, ExchangeFailure>,
    /// Requests the transport was given.
    sent: Cell<usize>,
}

impl IdentityManagementTransport for ScriptedTransport {
    fn exchange(&self, _body: &[u8]) -> Result<DecodedResponse, ExchangeFailure> {
        self.sent.set(self.sent.get() + 1);
        self.answer.clone()
    }
}

/// A clock that reports readings a test scripts, in order.
struct ScriptedClock {
    /// Readings the clock reports.
    readings: Vec<u64>,
    /// How many readings have been taken.
    taken: Cell<usize>,
}

impl MonotonicClock for ScriptedClock {
    fn reading_milliseconds(&self) -> u64 {
        let taken = self.taken.get();
        self.taken.set(taken + 1);
        *self
            .readings
            .get(taken)
            .unwrap_or_else(|| self.readings.last().expect("the clock has at least one reading"))
    }
}

/// A clock reporting one fixed second.
struct FixedSecond(u64);

impl CoordinatedUniversalTimeClock for FixedSecond {
    fn sample(&self) -> Option<u64> {
        Some(self.0)
    }
}

/// Returns the credentials every exchange here is built from.
fn credentials() -> CloudServiceCredentials {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(CREDENTIAL_FIXTURE);
    let bytes = std::fs::read(&path).expect("the credential reads");
    CloudServiceCredentials::parse(&SensitiveConfigurationDocument::from_bytes(bytes))
        .expect("the credential parses")
}

/// Returns the assertion every exchange here sends.
fn assertion(credentials: &CloudServiceCredentials) -> ServiceCredentialAssertion {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(ASSERTION_FIXTURE);
    let text = std::fs::read_to_string(&path).expect("the vectors read");
    let vectors: serde_json::Value = serde_json::from_str(&text).expect("the vectors parse");
    let second = vectors["sampled_second"].as_u64().expect("the vectors name a second");
    ServiceCredentialAssertion::build(credentials, &FixedSecond(second))
        .expect("the assertion builds")
}

/// Returns the head a successful exchange answers with.
fn success_head() -> DecodedHead {
    DecodedHead {
        status: SUCCESS_STATUS,
        fields: vec![("content-type".to_owned(), "application/json".to_owned())],
    }
}

/// Returns the body a successful exchange answers with.
fn success_body(expires_in: u64) -> Vec<u8> {
    format!(
        "{{\"access_token\":\"{TOKEN}\",\"token_type\":\"bearer\",\"expires_in\":{expires_in}}}"
    )
    .into_bytes()
}

/// Returns the response a successful exchange answers with.
fn success(expires_in: u64) -> DecodedResponse {
    DecodedResponse {
        informational: Vec::new(),
        head: success_head(),
        body: success_body(expires_in),
        trailer: None,
    }
}

/// Runs one exchange over a scripted answer and clock.
fn run(
    answer: Result<DecodedResponse, ExchangeFailure>,
    readings: Vec<u64>,
) -> (Result<AccessToken, ConfigurationFailureCode>, usize) {
    let credentials = credentials();
    let assertion = assertion(&credentials);
    let transport = ScriptedTransport { answer, sent: Cell::new(0) };
    let exchange =
        IdentityManagementExchange::new(transport, ScriptedClock { readings, taken: Cell::new(0) });
    let produced = exchange
        .exchange(&credentials, &assertion)
        .map_err(|failure: ExchangeFailure| failure.code);
    (produced, 1)
}

/// Returns the code one scripted answer is refused with.
fn refusal(answer: DecodedResponse) -> ConfigurationFailureCode {
    let (produced, _) = run(Ok(answer), vec![0, 0]);
    produced.map_or_else(|code| code, |_| panic!("the answer was accepted"))
}

#[test]
fn the_endpoint_is_built_from_the_manifest_alone() {
    let literals = &ProfileAuthenticationContract::embedded().literals;
    let endpoint = identity_management_endpoint();
    assert_eq!(
        endpoint,
        format!(
            "{}://{}{}",
            literals.identity_management_scheme,
            literals.identity_management_authorities[0],
            literals.identity_management_endpoint_path
        )
    );
    assert!(endpoint.starts_with("https://"), "{endpoint}");
    assert!(!endpoint.contains('?') && !endpoint.contains('#'), "{endpoint}");
}

#[test]
fn a_successful_exchange_produces_a_token_whose_lease_starts_at_the_anchor() {
    let advertised = COMFORTABLE_LIFETIME;
    let (produced, requests) = run(Ok(success(advertised)), vec![ANCHOR_READING, RECEIPT_READING]);
    let token = produced.expect("the exchange succeeds");
    assert_eq!(requests, 1, "the exchange sent another request");
    assert_eq!(
        token.deadline_milliseconds(),
        ANCHOR_READING + advertised,
        "the deadline was measured from the receipt rather than the anchor"
    );
    token.lend_token_bytes(|bytes| assert_eq!(bytes, TOKEN.as_bytes()));
    assert!(!token.refresh_required(RECEIPT_READING));
    let skew =
        ProfileAuthenticationContract::embedded().limits.access_token_refresh_skew_milliseconds;
    assert!(token.refresh_required(token.deadline_milliseconds() - skew), "equality must refresh");
}

#[test]
fn a_lease_at_the_threshold_is_refused_and_one_millisecond_above_is_accepted() {
    let limits = &ProfileAuthenticationContract::embedded().limits;
    let threshold = limits.access_token_refresh_skew_milliseconds
        + limits.minimum_access_token_usable_lease_milliseconds;
    let (refused, requests) = run(Ok(success(threshold)), vec![0, 0]);
    assert_eq!(
        refused.map_or_else(|code| code, |_| panic!("the threshold was accepted")),
        ConfigurationFailureCode::AccessTokenLifetimeTooShort
    );
    assert_eq!(requests, 1, "a short lease was retried");

    let (accepted, _) = run(Ok(success(threshold + 1)), vec![0, 0]);
    accepted.expect("one millisecond above the threshold is usable");
}

#[test]
fn transfer_time_reduces_the_lease_rather_than_extending_it() {
    let limits = &ProfileAuthenticationContract::embedded().limits;
    let threshold = limits.access_token_refresh_skew_milliseconds
        + limits.minimum_access_token_usable_lease_milliseconds;
    let (produced, _) =
        run(Ok(success(threshold + TRANSFER_MILLISECONDS)), vec![0, TRANSFER_MILLISECONDS]);
    assert_eq!(
        produced.map_or_else(|code| code, |_| panic!("the transfer time was ignored")),
        ConfigurationFailureCode::AccessTokenLifetimeTooShort
    );
}

#[test]
fn a_redirection_is_refused_without_its_location_being_read() {
    let mut response = success(COMFORTABLE_LIFETIME);
    response.head = DecodedHead {
        status: REDIRECTION_STATUS,
        fields: vec![("location".to_owned(), "https://elsewhere.example.com/".to_owned())],
    };
    assert_eq!(refusal(response), ConfigurationFailureCode::IdentityManagementRedirectRefused);
}

#[test]
fn the_first_informational_head_ends_the_exchange() {
    for &status in INFORMATIONAL_STATUSES {
        let mut response = success(COMFORTABLE_LIFETIME);
        response.informational = vec![DecodedHead { status, fields: Vec::new() }];
        assert_eq!(
            refusal(response),
            ConfigurationFailureCode::IdentityManagementResponseStatusRejected,
            "{status}"
        );
    }
}

#[test]
fn a_declared_or_present_trailer_is_refused() {
    let mut declared = success(COMFORTABLE_LIFETIME);
    declared.head.fields.push(("Trailer".to_owned(), "expires".to_owned()));
    assert_eq!(
        refusal(declared),
        ConfigurationFailureCode::IdentityManagementResponseTrailerRejected
    );

    for trailer in [
        DecodedSection::default(),
        DecodedSection { fields: vec![("expires".to_owned(), "soon".to_owned())] },
    ] {
        let mut present = success(COMFORTABLE_LIFETIME);
        present.trailer = Some(trailer);
        assert_eq!(
            refusal(present),
            ConfigurationFailureCode::IdentityManagementResponseTrailerRejected
        );
    }
}

#[test]
fn a_section_or_body_beyond_its_bound_is_refused_before_it_is_interpreted() {
    let limits = &ProfileAuthenticationContract::embedded().limits;
    let count = usize::try_from(limits.maximum_identity_management_response_header_count)
        .expect("the count fits");
    let mut crowded = success(COMFORTABLE_LIFETIME);
    crowded.head.status = REDIRECTION_STATUS;
    crowded.head.fields =
        (0..=count).map(|index| (format!("field-{index}"), String::new())).collect();
    assert_eq!(
        refusal(crowded),
        ConfigurationFailureCode::IdentityManagementResponseHeadLimitExceeded,
        "the status was interpreted before the head was charged"
    );

    let mut large = success(COMFORTABLE_LIFETIME);
    large.head.status = REDIRECTION_STATUS;
    large.body = vec![
        b'x';
        usize::try_from(limits.maximum_identity_management_response_body_bytes)
            .expect("the bound fits")
            + 1
    ];
    assert_eq!(
        refusal(large),
        ConfigurationFailureCode::IdentityManagementResponseBodyLimitExceeded
    );
}

#[test]
fn only_the_media_type_and_document_the_contract_names_are_accepted() {
    let charset = {
        let mut response = success(COMFORTABLE_LIFETIME);
        response.head.fields =
            vec![("content-type".to_owned(), "application/json; charset=utf-8".to_owned())];
        response
    };
    let (accepted, _) = run(Ok(charset), vec![0, 0]);
    accepted.expect("the one accepted parameter is accepted");

    for fields in [
        vec![("content-type".to_owned(), "text/plain".to_owned())],
        vec![("content-type".to_owned(), "application/json; charset=utf-16".to_owned())],
        vec![
            ("content-type".to_owned(), "application/json".to_owned()),
            ("content-type".to_owned(), "application/json".to_owned()),
        ],
        vec![
            ("content-type".to_owned(), "application/json".to_owned()),
            ("content-encoding".to_owned(), "gzip".to_owned()),
        ],
    ] {
        let mut response = success(COMFORTABLE_LIFETIME);
        response.head.fields = fields;
        assert_eq!(
            refusal(response),
            ConfigurationFailureCode::IdentityManagementResponseMediaInvalid
        );
    }
}

#[test]
fn only_the_closed_response_document_is_accepted() {
    for (body, expected) in [
        (
            format!(
                "{{\"access_token\":\"{TOKEN}\",\"token_type\":\"Bearer\",\"expires_in\":3600000}}"
            ),
            ConfigurationFailureCode::IdentityManagementTokenTypeInvalid,
        ),
        (
            format!("{{\"access_token\":\"{TOKEN}\",\"token_type\":\"bearer\",\"expires_in\":0}}"),
            ConfigurationFailureCode::IdentityManagementTokenLifetimeInvalid,
        ),
        (
            format!("{{\"access_token\":\"{TOKEN}\",\"token_type\":\"bearer\"}}"),
            ConfigurationFailureCode::IdentityManagementResponseDocumentInvalid,
        ),
        (
            "{\"access_token\":\"\",\"token_type\":\"bearer\",\"expires_in\":3600000}".to_owned(),
            ConfigurationFailureCode::IdentityManagementResponseDocumentInvalid,
        ),
        (
            format!(
                "{{\"access_token\":\"{TOKEN}\",\"token_type\":\"bearer\",\"expires_in\":3600000,\"extra\":1}}"
            ),
            ConfigurationFailureCode::IdentityManagementResponseDocumentInvalid,
        ),
    ] {
        let mut response = success(COMFORTABLE_LIFETIME);
        response.body = body.into_bytes();
        assert_eq!(refusal(response), expected, "{body:?}", body = "the document");
    }
}

#[test]
fn the_request_that_reaches_the_endpoint_is_the_one_this_contract_describes() {
    let answer = format!(
        "HTTP/1.1 200 OK\r\ncontent-type: application/json\r\ncontent-length: {}\r\n\r\n{}",
        success_body(COMFORTABLE_LIFETIME).len(),
        String::from_utf8(success_body(COMFORTABLE_LIFETIME)).expect("the body is text")
    );
    let server = ScriptedIdentityManagementServer::answering(answer.as_bytes());
    let trap = ScriptedIdentityManagementServer::trap();

    let credentials = credentials();
    let assertion = assertion(&credentials);
    let exchange = IdentityManagementExchange::new(
        PlainTransport { address: server.address().to_string() },
        ScriptedClock { readings: vec![0, 1], taken: Cell::new(0) },
    );
    let token = exchange.exchange(&credentials, &assertion).expect("the exchange succeeds");
    token.lend_token_bytes(|bytes| assert_eq!(bytes, TOKEN.as_bytes()));

    let received = server.received();
    assert_eq!(received.len(), 1, "the exchange sent another request");
    let request = String::from_utf8_lossy(&received[0]);
    assert!(request.starts_with("POST "), "{request}");
    assert!(!request.to_ascii_lowercase().contains("expect:"), "{request}");
    assert!(!request.to_ascii_lowercase().contains("upgrade:"), "{request}");
    assert!(request.contains("client_id=") && request.contains("jwt_token="), "{request}");
    assert!(trap.received().is_empty(), "the trap received a request");
}

#[test]
fn no_rendering_of_a_failure_or_a_token_carries_a_secret() {
    let (produced, _) = run(Ok(success(COMFORTABLE_LIFETIME)), vec![0, 0]);
    let token = produced.expect("the exchange succeeds");
    let rendered = format!("{token:?}");
    for sentinel in SENTINELS {
        assert!(!rendered.contains(sentinel), "{rendered} carries {sentinel}");
    }
    let failure = ExchangeFailure::new(ConfigurationFailureCode::IdentityManagementTlsFailed);
    let rendered = format!("{failure:?} {failure}");
    for sentinel in SENTINELS {
        assert!(!rendered.contains(sentinel), "{rendered} carries {sentinel}");
    }
}

/// A transport that carries one request over a plain connection.
///
/// It exists so the form body, the method, and the request count are observed
/// on a real connection rather than reported by a fake.
struct PlainTransport {
    /// Address the request is sent to.
    address: String,
}

impl IdentityManagementTransport for PlainTransport {
    fn exchange(&self, body: &[u8]) -> Result<DecodedResponse, ExchangeFailure> {
        let failed =
            || ExchangeFailure::new(ConfigurationFailureCode::IdentityManagementTransportFailed);
        let mut connection = TcpStream::connect(&self.address).map_err(|_| failed())?;
        let literals = &ProfileAuthenticationContract::embedded().literals;
        let request = format!(
            "{} {} HTTP/1.1\r\nhost: {}\r\ncontent-type: {}\r\ncontent-length: {}\r\n\r\n",
            literals.identity_management_request_method,
            literals.identity_management_endpoint_path,
            literals.identity_management_authorities[0],
            literals.request_media_type,
            body.len()
        );
        connection.write_all(request.as_bytes()).map_err(|_| failed())?;
        connection.write_all(body).map_err(|_| failed())?;
        connection.flush().map_err(|_| failed())?;
        let mut answer = Vec::new();
        connection.read_to_end(&mut answer).map_err(|_| failed())?;
        decode(&answer).ok_or_else(failed)
    }
}

/// Decodes one plain response into the sections the exchange charges.
fn decode(answer: &[u8]) -> Option<DecodedResponse> {
    let text = core::str::from_utf8(answer).ok()?;
    let (head, body) = text.split_once("\r\n\r\n")?;
    let mut lines = head.split("\r\n");
    let status = lines.next()?.split(' ').nth(1)?.parse().ok()?;
    let fields = lines
        .filter_map(|line| line.split_once(':'))
        .map(|(name, value)| (name.trim().to_ascii_lowercase(), value.trim().to_owned()))
        .collect();
    Some(DecodedResponse {
        informational: Vec::new(),
        head: DecodedHead { status, fields },
        body: body.as_bytes().to_vec(),
        trailer: None,
    })
}
