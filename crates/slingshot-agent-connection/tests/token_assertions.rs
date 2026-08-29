//! Assertions for the compact assertion's exact bytes.
//!
//! A signature is over bytes, not over claims. Two implementations that agree
//! on the audience, the subject, and the expiry can still disagree on the
//! bytes - one escapes a solidus, one orders members differently, one pads its
//! base64 - and Adobe would reject one of them. So the fixture pins the
//! complete compact form, built by a separate implementation of the byte rules
//! and signed through the platform's own tool, and the comparison here is
//! byte for byte rather than claim by claim.
//!
//! The clock is injected, which is what makes every boundary provable: a clock
//! with no answer, a second past the accepted range, a certificate that is not
//! yet valid, one that has expired, and equality at either edge.

use std::path::PathBuf;

use serde::Deserialize;
use slingshot_agent_connection::authentication::cloud_service_credentials::CloudServiceCredentials;
use slingshot_agent_connection::authentication::token_assertion::{
    CoordinatedUniversalTimeClock, ServiceCredentialAssertion,
};
use slingshot_domain::profile_authentication_contract::{
    ConfigurationFailureCode, ProfileAuthenticationContract,
};
use slingshot_domain::secret_value::SensitiveConfigurationDocument;

/// Fixture holding the independently calculated vectors.
const VECTOR_FIXTURE: &str =
    "../slingshot-test-support/fixtures/token-assertions/assertion-vectors.json";

/// Credential document the vectors were built for.
const CREDENTIAL_FIXTURE: &str = "../slingshot-test-support/fixtures/cloud-credentials/valid.json";

/// The independently calculated vectors.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct AssertionVectors {
    /// Format identifier of the fixture.
    format: String,
    /// Second the clock was sampled at.
    sampled_second: u64,
    /// Second the assertion expires at.
    expiry_second: u64,
    /// Exact protected-header bytes.
    protected_header: String,
    /// Exact payload bytes.
    payload: String,
    /// Exact two-segment signing input.
    signing_input: String,
    /// Exact compact serialization.
    compact: String,
}

/// A clock that answers exactly what a test tells it to.
struct ScriptedClock {
    /// Second the clock reports, when it has one.
    second: Option<u64>,
}

impl CoordinatedUniversalTimeClock for ScriptedClock {
    fn sample(&self) -> Option<u64> {
        self.second
    }
}

/// Returns the committed vectors.
fn vectors() -> AssertionVectors {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(VECTOR_FIXTURE);
    let text = std::fs::read_to_string(&path).expect("the vectors read");
    serde_json::from_str(&text).expect("the vectors parse")
}

/// Returns the credentials the vectors were built for.
fn credentials() -> CloudServiceCredentials {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(CREDENTIAL_FIXTURE);
    let bytes = std::fs::read(&path).expect("the credential reads");
    CloudServiceCredentials::parse(&SensitiveConfigurationDocument::from_bytes(bytes))
        .expect("the credential parses")
}

/// Returns the compact assertion built at `second`.
fn built(second: Option<u64>) -> Result<String, ConfigurationFailureCode> {
    ServiceCredentialAssertion::build(&credentials(), &ScriptedClock { second })
        .map(|assertion| {
            assertion.lend_compact_bytes(|bytes| String::from_utf8_lossy(bytes).into_owned())
        })
        .map_err(|failure| failure.code)
}

#[test]
fn the_assertion_is_byte_identical_to_the_independent_vector() {
    let vectors = vectors();
    assert_eq!(vectors.format, "slingshot.assertion-vectors/1");
    let compact = built(Some(vectors.sampled_second)).expect("the assertion builds");
    assert_eq!(compact, vectors.compact, "the assertion is not the pinned bytes");

    let segments: Vec<&str> = compact.split('.').collect();
    assert_eq!(segments.len(), 3, "the compact form is not three segments");
    assert_eq!(format!("{}.{}", segments[0], segments[1]), vectors.signing_input);
    assert_eq!(decode(segments[0]), vectors.protected_header);
    assert_eq!(decode(segments[1]), vectors.payload);
    assert!(!compact.contains('='), "a segment carries padding");
    assert!(!compact.contains('+') && !compact.contains('/'), "a segment uses another alphabet");
}

#[test]
fn the_payload_carries_exactly_the_claims_the_contract_names() {
    let vectors = vectors();
    let literals = &ProfileAuthenticationContract::embedded().literals;
    let payload = vectors.payload;
    assert!(payload.contains(&format!("\"exp\":{}", vectors.expiry_second)), "{payload}");
    assert!(!payload.contains("\\/"), "a solidus was escaped");
    assert!(!payload.contains("\\u"), "a numeric escape was written");
    assert!(payload.contains(&literals.assertion_audience_prefix.clone()), "{payload}");
    assert!(payload.contains(&literals.assertion_metascope_claim_prefix.clone()), "{payload}");

    let names: Vec<&str> = payload
        .split(',')
        .filter_map(|member| member.split(':').next())
        .map(|name| name.trim_start_matches('{'))
        .collect();
    let mut ordered = names.clone();
    ordered.sort_by_key(|name| name.as_bytes().to_vec());
    assert_eq!(names, ordered, "the members are not in ascending key order");
}

#[test]
fn the_expiry_is_the_sample_plus_the_named_lifetime() {
    let vectors = vectors();
    let lifetime = ProfileAuthenticationContract::embedded()
        .limits
        .service_credential_assertion_lifetime_seconds;
    assert_eq!(vectors.expiry_second, vectors.sampled_second + lifetime);
    let assertion = ServiceCredentialAssertion::build(
        &credentials(),
        &ScriptedClock { second: Some(vectors.sampled_second) },
    )
    .expect("the assertion builds");
    assert_eq!(assertion.issued_at(), vectors.sampled_second);
    assert_eq!(assertion.expires_at(), vectors.expiry_second);
    assert!(
        u64::try_from(assertion.compact_byte_length()).expect("it fits")
            <= ProfileAuthenticationContract::embedded()
                .limits
                .maximum_service_credential_assertion_bytes
    );
}

#[test]
fn every_clock_and_certificate_boundary_reports_its_own_code() {
    let limits = &ProfileAuthenticationContract::embedded().limits;
    assert_eq!(built(None), Err(ConfigurationFailureCode::AssertionClockUnavailable));
    assert_eq!(
        built(Some(limits.maximum_service_credential_utc_unix_seconds + 1)),
        Err(ConfigurationFailureCode::AssertionClockOutOfRange)
    );
    assert_eq!(built(Some(0)), Err(ConfigurationFailureCode::AssertionCertificateNotYetValid));
    assert_eq!(
        built(Some(limits.maximum_service_credential_utc_unix_seconds)),
        Err(ConfigurationFailureCode::AssertionCertificateExpired)
    );
}

#[test]
fn a_certificate_boundary_second_is_inside_the_validity() {
    let credentials = credentials();
    let (not_before, not_after) = validity(credentials.public_certificate());
    for second in [not_before, not_after] {
        ServiceCredentialAssertion::build(&credentials, &ScriptedClock { second: Some(second) })
            .expect("equality at a boundary is inside the validity");
    }
    for second in [not_before - 1, not_after + 1] {
        assert!(
            ServiceCredentialAssertion::build(
                &credentials,
                &ScriptedClock { second: Some(second) }
            )
            .is_err(),
            "one second outside the validity was accepted"
        );
    }
}

#[test]
fn no_rendering_of_an_assertion_carries_its_bytes() {
    let vectors = vectors();
    let assertion = ServiceCredentialAssertion::build(
        &credentials(),
        &ScriptedClock { second: Some(vectors.sampled_second) },
    )
    .expect("the assertion builds");
    let rendered = format!("{assertion:?}");
    assert!(!rendered.contains(&vectors.compact), "{rendered}");
    assert!(!rendered.contains(&vectors.signing_input), "{rendered}");
}

/// Returns the decoded bytes of one compact segment, as text.
fn decode(segment: &str) -> String {
    use base64::Engine;
    use base64::engine::general_purpose::URL_SAFE_NO_PAD;

    String::from_utf8(URL_SAFE_NO_PAD.decode(segment).expect("the segment decodes"))
        .expect("the segment is text")
}

/// Returns the seconds one certificate is valid between.
fn validity(certificate: &[u8]) -> (u64, u64) {
    use x509_parser::prelude::{FromDer, X509Certificate};

    let (_, parsed) = X509Certificate::from_der(certificate).expect("the certificate parses");
    let not_before = u64::try_from(parsed.validity().not_before.timestamp()).expect("it fits");
    let not_after = u64::try_from(parsed.validity().not_after.timestamp()).expect("it fits");
    (not_before, not_after)
}
