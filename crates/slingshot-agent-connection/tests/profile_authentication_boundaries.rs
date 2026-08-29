//! The transport crate's own boundary, proved without the composed harness.
//!
//! This file deliberately imports nothing from the outermost tooling crate. The
//! composed transcript belongs there; what belongs here is the contract this
//! crate is responsible for on its own - that the two routes cannot be
//! confused, that the endpoint is built from the manifest, and that nothing it
//! renders carries what it was given to carry.

use std::path::PathBuf;

use slingshot_agent_connection::authentication::identity_management_exchange::{
    ExchangeFailure, identity_management_endpoint,
};
use slingshot_agent_connection::transport_policy::{
    AuthorTrustInput, DirectTransportPolicy, IdentityManagementTrustInput,
};
use slingshot_configuration::additional_certificate_authority::AdditionalAuthorCertificates;
use slingshot_configuration::platform_trust::{
    PlatformTrustSnapshot, PlatformTrustSource, ProviderDecision, ProviderRecord,
};
use slingshot_configuration::profile_loader::ConfigurationDiagnostic;
use slingshot_domain::profile_authentication_contract::{
    ConfigurationFailureCode, ProfileAuthenticationContract,
};

/// Certificate the platform snapshot holds.
const PLATFORM_FIXTURE: &str =
    "../slingshot-test-support/fixtures/additional-certificate-authority/one-authority.pem";

/// Certificate an environment selects as an author trust extension.
const EXTENSION_FIXTURE: &str =
    "../slingshot-test-support/fixtures/additional-certificate-authority/other-authority.pem";

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

/// Returns the certificates one committed source holds.
fn certificates(fixture: &str) -> AdditionalAuthorCertificates {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(fixture);
    let bytes = std::fs::read(&path).expect("the certificate reads");
    AdditionalAuthorCertificates::parse(&bytes).expect("the certificate parses")
}

/// Returns the platform snapshot both routes start from.
fn platform() -> PlatformTrustSnapshot {
    let store = ScriptedStore {
        records: certificates(PLATFORM_FIXTURE)
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

#[test]
fn the_credential_route_never_holds_what_the_author_route_was_extended_with() {
    let platform = platform();
    let extension = certificates(EXTENSION_FIXTURE);
    let identity_management =
        IdentityManagementTrustInput::from_platform(&platform).expect("the input builds");
    let author = AuthorTrustInput::from_platform_and_extension(&platform, Some(&extension))
        .expect("the input builds");
    for certificate in extension.certificates() {
        assert!(author.roots().contains(certificate), "the extension reached nothing");
        assert!(
            !identity_management.roots().contains(certificate),
            "the extension reached the credential route"
        );
    }
}

#[test]
fn the_endpoint_and_the_connection_policy_come_from_the_manifest() {
    let literals = &ProfileAuthenticationContract::embedded().literals;
    assert!(identity_management_endpoint().contains(&literals.identity_management_authorities[0]));
    assert_eq!(DirectTransportPolicy::policy(), literals.proxy_policy);
    assert_eq!(DirectTransportPolicy::redirect_policy(), literals.redirect_policy);
    assert!(!DirectTransportPolicy::ignored_proxy_variables().is_empty());
}

#[test]
fn a_transport_failure_says_only_what_failed() {
    let failure = ExchangeFailure::new(ConfigurationFailureCode::IdentityManagementTlsFailed);
    let rendered = format!("{failure} {failure:?}");
    assert!(rendered.contains("identity_management_tls_failed"), "{rendered}");
    for sentinel in ["not-a-real", "PRIVATE KEY", "Bearer "] {
        assert!(!rendered.contains(sentinel), "{rendered} carries {sentinel}");
    }
}

#[test]
fn nothing_here_reaches_for_the_outermost_crate() {
    let crate_directory = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let source =
        std::fs::read_to_string(crate_directory.join("tests/profile_authentication_boundaries.rs"))
            .expect("this file reads");
    let importing: Vec<&str> = source
        .lines()
        .map(str::trim)
        .filter(|line| line.starts_with("use ") || line.starts_with("extern crate "))
        .filter(|line| line.contains("development"))
        .collect();
    assert!(importing.is_empty(), "a focused boundary test imports the harness: {importing:?}");

    let manifest =
        std::fs::read_to_string(crate_directory.join("Cargo.toml")).expect("the manifest reads");
    let declared: Vec<&str> = manifest
        .lines()
        .map(str::trim)
        .filter(|line| line.starts_with("slingshot-development"))
        .collect();
    assert!(declared.is_empty(), "this crate can reach the harness at all: {declared:?}");
}
