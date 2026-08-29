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
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use slingshot_agent_connection::authentication::access_token_cache::AccessTokenSource;
use slingshot_agent_connection::authentication::cloud_service_credentials::CloudServiceCredentials;
use slingshot_agent_connection::authentication::environment_provider::{
    EnvironmentAuthenticationProvider, SelectedEnvironmentSnapshot, SnapshotAuthentication,
    SnapshotMaterial,
};
use slingshot_agent_connection::authentication::identity_management_exchange::{
    AccessToken, DecodedHead, DecodedResponse, ExchangeFailure, IdentityManagementExchange,
    IdentityManagementTransport, MonotonicClock,
};
use slingshot_agent_connection::authentication::token_assertion::{
    CoordinatedUniversalTimeClock, ServiceCredentialAssertion,
};
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
use slingshot_domain::profile::{
    AdobeExperienceManagerDeployment, EnvironmentAuthentication, EnvironmentName, ProfileName,
};
use slingshot_domain::profile_authentication_contract::ConfigurationFailureCode;
use slingshot_domain::secret_value::SensitiveConfigurationDocument;
use slingshot_domain::selected_environment_revision::{
    AuthenticationPrincipalIdentity, AuthorTargetIdentityDigest, CanonicalMetascopeSet,
    RevisionFields, SelectedEnvironmentRevision,
};

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
    let mut authority = ScriptedFilesystem::new();
    for (reference, bytes) in profile_files() {
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
    let loaded = loaded();
    let selection = selection(&loaded, profile, environment);
    let chosen = selection.environment_of(&loaded);
    let platform = platform();
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
    EnvironmentAuthenticationProvider::new(snapshot, CACHE_IDENTITY)
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
