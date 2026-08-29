//! Assertions for the two routes' trust and connection policy.
//!
//! The property here is a type-level one, and it is the whole reason these are
//! two types rather than one list with a flag. An operator extending author
//! trust to a corporate authority is not asking to extend it to the place their
//! credentials go, so the identity-management builder must be unable to accept
//! an author input at all - not merely unlikely to be handed one.
//!
//! A compile-time question asks whether either input converts into the other,
//! and a control type that does convert proves the question can answer yes.
//!
//! The live proof that a hostile authority cannot intercept an exchange - a
//! certificate for the identity-management host, signed by an authority the
//! platform does not hold - needs a real listener and belongs to the composed
//! transcript in `prove-profile-authentication-boundaries`. What is provable
//! here is that its bytes cannot reach the builder in the first place.

use std::path::PathBuf;

use slingshot_agent_connection::transport_policy::{
    AuthorTrustInput, DirectTransportPolicy, IdentityManagementTrustInput,
};
use slingshot_configuration::additional_certificate_authority::AdditionalAuthorCertificates;
use slingshot_configuration::platform_trust::{
    PlatformTrustSnapshot, PlatformTrustSource, ProviderDecision, ProviderRecord,
};
use slingshot_configuration::profile_loader::ConfigurationDiagnostic;
use slingshot_domain::profile_authentication_contract::ProfileAuthenticationContract;
use slingshot_domain::selected_environment_revision::TrustPolicyIdentity;

/// Directory holding the committed certificates.
const CERTIFICATE_FIXTURES: &str =
    "../slingshot-test-support/fixtures/additional-certificate-authority";

/// A trust store holding exactly the roots a test gives it.
struct ScriptedStore {
    /// Records the store holds.
    records: Vec<ProviderRecord>,
}

impl PlatformTrustSource for ScriptedStore {
    fn records(&self) -> Result<Vec<ProviderRecord>, ConfigurationDiagnostic> {
        Ok(self.records.clone())
    }
}

/// Returns one committed certificate source.
fn source(name: &str) -> Vec<u8> {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(CERTIFICATE_FIXTURES).join(name);
    std::fs::read(&path).expect("the certificate source reads")
}

/// Returns a platform snapshot holding one authority.
fn platform_snapshot() -> PlatformTrustSnapshot {
    let certificates = AdditionalAuthorCertificates::parse(&source("one-authority.pem"))
        .expect("the authority parses");
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

/// Returns the extension an environment may add to author trust.
///
/// It names an authority the platform snapshot does not hold, which is the only
/// case where "the extension reached the wrong route" is a question at all.
fn extension() -> AdditionalAuthorCertificates {
    AdditionalAuthorCertificates::parse(&source("other-authority.pem"))
        .expect("the extension parses")
}

/// A value that does convert, so the question below can answer yes.
#[derive(Clone)]
struct ConvertibleControl(PlatformTrustSnapshot);

impl From<ConvertibleControl> for IdentityManagementTrustInput {
    fn from(control: ConvertibleControl) -> Self {
        Self::from_platform(&control.0).expect("the input builds")
    }
}

/// Borrowed value the conversion question below is asked about.
struct Question<'subject, Subject>(&'subject Subject);

/// The answer given when the conversion does not exist.
trait AbsentConversion {
    /// Whether the value converts into an identity-management input.
    fn reaches_identity_management(&self) -> bool {
        false
    }
}

impl<Subject> AbsentConversion for Question<'_, Subject> {}

impl<Subject> Question<'_, Subject> {
    /// Returns the value the question is asked about.
    fn subject(&self) -> &Subject {
        self.0
    }
}

impl<Subject: Clone + Into<IdentityManagementTrustInput>> Question<'_, Subject> {
    /// Answers that the value converts into an identity-management input.
    fn reaches_identity_management(&self) -> bool {
        true
    }
}

#[test]
fn the_two_routes_carry_different_identities_over_the_same_roots() {
    let snapshot = platform_snapshot();
    let identity_management =
        IdentityManagementTrustInput::from_platform(&snapshot).expect("the input builds");
    let author =
        AuthorTrustInput::from_platform_and_extension(&snapshot, None).expect("the input builds");
    assert_eq!(identity_management.roots(), author.roots(), "the roots differ");
    assert_ne!(
        identity_management.identity(),
        author.identity(),
        "one root set produced one identity for two routes"
    );
    assert_eq!(
        identity_management.identity(),
        TrustPolicyIdentity::identity_management(snapshot.roots()).expect("it builds")
    );
}

#[test]
fn an_extension_reaches_the_author_route_and_nothing_else() {
    let snapshot = platform_snapshot();
    let plain =
        AuthorTrustInput::from_platform_and_extension(&snapshot, None).expect("the input builds");
    let extended = AuthorTrustInput::from_platform_and_extension(&snapshot, Some(&extension()))
        .expect("the input builds");
    assert_ne!(plain.identity(), extended.identity(), "the extension changed nothing");
    assert!(extended.roots().len() > plain.roots().len(), "the extension added no root");
    for root in plain.roots() {
        assert!(extended.roots().contains(root), "the extension replaced a platform root");
    }

    let identity_management =
        IdentityManagementTrustInput::from_platform(&snapshot).expect("the input builds");
    for root in extension().certificates() {
        assert!(
            !identity_management.roots().contains(root),
            "an author extension reached identity-management trust"
        );
    }
}

#[test]
fn no_author_value_converts_into_an_identity_management_input() {
    let snapshot = platform_snapshot();
    let author = AuthorTrustInput::from_platform_and_extension(&snapshot, Some(&extension()))
        .expect("the input builds");
    let certificates = extension();
    assert!(!Question(&author).reaches_identity_management(), "the author input converts");
    assert!(!Question(&certificates).reaches_identity_management(), "the extension converts");
    assert!(
        !Question(&author.identity()).reaches_identity_management(),
        "the author identity converts"
    );
    let control = ConvertibleControl(snapshot);
    let control = Question(&control);
    assert!(!control.subject().0.roots().is_empty(), "the control holds no root");
    assert!(
        control.reaches_identity_management(),
        "the question cannot answer yes, so its no proves nothing"
    );
}

#[test]
fn both_routes_publish_the_same_direct_connection_policy() {
    let literals = &ProfileAuthenticationContract::embedded().literals;
    assert_eq!(DirectTransportPolicy::policy(), literals.proxy_policy);
    assert_eq!(DirectTransportPolicy::redirect_policy(), literals.redirect_policy);
    assert_eq!(
        DirectTransportPolicy::transport_layer_security_versions(),
        literals.transport_layer_security_versions
    );
    let ignored = DirectTransportPolicy::ignored_proxy_variables();
    for variable in ["HTTP_PROXY", "HTTPS_PROXY", "ALL_PROXY", "NO_PROXY"] {
        assert!(ignored.iter().any(|known| known == variable), "{variable} is not ignored");
        assert!(
            ignored.iter().any(|known| known == &variable.to_lowercase()),
            "{variable} is ignored in only one case"
        );
    }
}

#[test]
fn this_crate_offers_no_publisher_builder_at_all() {
    // Carrying a publisher address is fine and necessary; what must not exist
    // is anything that turns one into a connection, a client, or an endpoint.

    let directory = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("src");
    let mut pending = vec![directory];
    let mut named = Vec::new();
    while let Some(entry) = pending.pop() {
        for child in std::fs::read_dir(&entry).expect("the source directory reads") {
            let path = child.expect("the entry reads").path();
            if path.is_dir() {
                pending.push(path);
                continue;
            }
            let text = std::fs::read_to_string(&path).expect("the module reads");
            named.extend(
                text.lines()
                    .map(str::trim)
                    .filter(|line| line.starts_with("pub fn ") && line.contains("publisher"))
                    .filter(|line| {
                        ["client", "connect", "endpoint", "builder", "authenticate"]
                            .iter()
                            .any(|verb| line.contains(verb))
                    })
                    .map(str::to_owned),
            );
        }
    }
    assert!(named.is_empty(), "a publisher connection can be built: {named:?}");
}
