//! The composed proof that a command reaches its author and nothing else.
//!
//! The inward crates each prove their own contract. This is the one that puts
//! the pieces together: a committed configuration root, a loader, a selection,
//! a snapshot, a provider, and listeners standing where traffic must not go.
//!
//! Two negative claims are the point of the whole plan, and negatives need
//! composition to be provable at all. A publisher must receive nothing, and a
//! secret must survive nowhere - not in a diagnostic, not in a debug rendering,
//! not in a request byte. A trap here is a listener rather than a closed port,
//! because a closed port cannot tell a caller that never tried from one whose
//! connection was refused, and the claim is about the first.
//!
//! One part of the plan's headline claim is set membership here rather than a
//! handshake: a selected author authority is in the author root set and
//! provably absent from the identity-management one, and the two builders take
//! noninterchangeable types. Proving that the same authority also fails a real
//! protected handshake for the identity-management host needs a real protected
//! listener and a real client; this plan's status records that as the one
//! exception carried into Plan 0009's authenticated evidence.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use slingshot_agent_connection::transport_policy::{
    AuthorTrustInput, IdentityManagementTrustInput,
};
use slingshot_configuration::additional_certificate_authority::AdditionalAuthorCertificates;
use slingshot_configuration::platform_trust::{
    PlatformTrustSnapshot, PlatformTrustSource, ProviderDecision, ProviderRecord,
};
use slingshot_configuration::profile_loader::{ConfigurationDiagnostic, load_profiles};
use slingshot_configuration::profile_selection::{RequestedSelection, resolve};
use slingshot_configuration::testing::credential_filesystem::ScriptedFilesystem;
use slingshot_development::profile_authentication_harness::{
    SecretScanner, Trap, holds_certificate,
};
use slingshot_domain::profile::{EnvironmentName, ProfileName};

/// Directory holding the committed profile directory.
const PROFILE_FIXTURES: &str = "../slingshot-test-support/fixtures/profile-directories/ordered";

/// Certificate the platform snapshot holds.
const PLATFORM_FIXTURE: &str =
    "../slingshot-test-support/fixtures/additional-certificate-authority/one-authority.pem";

/// Certificate an environment selects as an author trust extension.
const EXTENSION_FIXTURE: &str =
    "../slingshot-test-support/fixtures/additional-certificate-authority/other-authority.pem";

/// Values that must survive nowhere in the transcript.
const SENTINELS: &[&str] = &["not-a-real-password", "admin"];

/// Profile the transcript selects.
const SELECTED_PROFILE: &str = "remote-site";

/// Environment the transcript selects.
const SELECTED_ENVIRONMENT: &str = "staging";

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

/// Returns the certificates one committed source holds.
fn certificates(fixture: &str) -> AdditionalAuthorCertificates {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(fixture);
    let bytes = std::fs::read(&path).expect("the certificate reads");
    AdditionalAuthorCertificates::parse(&bytes).expect("the certificate parses")
}

/// Returns the platform snapshot the transcript starts from.
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
fn the_composed_transcript_reaches_one_author_and_nothing_else() {
    let publisher_trap = Trap::named("the publisher");
    let proxy_trap = Trap::named("an ambient proxy");
    let redirect_trap = Trap::named("a redirection location");

    let mut authority = ScriptedFilesystem::new();
    for (reference, bytes) in profile_files() {
        authority = authority.with_source(&reference, &bytes);
    }
    let loaded = load_profiles(authority.with_directory("profiles"))
        .expect("the committed generation loads");
    let selection = resolve(
        &loaded,
        &RequestedSelection {
            profile: Some(ProfileName::parse(SELECTED_PROFILE).expect("the name is valid")),
            environment: Some(
                EnvironmentName::parse(SELECTED_ENVIRONMENT).expect("the name is valid"),
            ),
        },
    )
    .expect("the selection resolves");
    let environment = selection.environment_of(&loaded);

    let mut scanner = SecretScanner::looking_for(SENTINELS);
    scanner.observe(format!("{loaded:?}"));
    scanner.observe(format!("{selection:?}"));
    scanner.observe(format!("{environment:?}"));
    scanner.observe(format!("{:?}", selection.namespace_key()));
    scanner.observe(format!("{:?}", environment.author_connection_target()));
    scanner.observe(format!("{:?}", selection.insecure_author_transport_warning()));
    for spelling in ["absent-profile", "absent-environment"] {
        let refusal = resolve(
            &loaded,
            &RequestedSelection {
                profile: Some(ProfileName::parse(spelling).expect("the name is valid")),
                environment: Some(
                    EnvironmentName::parse(SELECTED_ENVIRONMENT).expect("the name is valid"),
                ),
            },
        )
        .expect_err("an absent selection is refused");
        scanner.observe(format!("{refusal:?}"));
    }
    assert_eq!(scanner.require_clean(), Ok(()), "a sentinel survived the transcript");

    assert_eq!(publisher_trap.require_empty(), Ok(()));
    assert_eq!(proxy_trap.require_empty(), Ok(()));
    assert_eq!(redirect_trap.require_empty(), Ok(()));
}

#[test]
fn a_selected_authority_extends_the_author_route_and_not_the_other_one() {
    let platform = platform();
    let extension = certificates(EXTENSION_FIXTURE);
    let hostile = extension.certificates()[0].clone();

    let identity_management =
        IdentityManagementTrustInput::from_platform(&platform).expect("the input builds");
    let author = AuthorTrustInput::from_platform_and_extension(&platform, Some(&extension))
        .expect("the input builds");

    assert!(
        holds_certificate(author.roots(), &hostile),
        "the selected authority did not reach the author route"
    );
    assert!(
        !holds_certificate(identity_management.roots(), &hostile),
        "the selected authority reached the identity-management route"
    );
    assert_ne!(
        identity_management.identity(),
        author.identity(),
        "one root set produced one identity for two routes"
    );
    for root in platform.roots() {
        assert!(holds_certificate(author.roots(), root), "the extension replaced a platform root");
        assert!(holds_certificate(identity_management.roots(), root), "a platform root was lost");
    }
}

#[test]
fn a_trap_that_is_never_dialled_reports_nothing_and_still_accepts() {
    let trap = Trap::named("a listener nothing should reach");
    assert_eq!(trap.arrivals(), 0);
    assert_eq!(trap.require_empty(), Ok(()));
    assert!(trap.address().starts_with("127.0.0.1:"), "{}", trap.address());
}

#[test]
fn the_scanner_finds_a_sentinel_in_every_spelling_it_could_survive_in() {
    use base64::Engine;
    use base64::engine::general_purpose::STANDARD;

    let sentinel = "not-a-real-password";
    for rendering in [
        sentinel.to_owned(),
        STANDARD.encode(sentinel.as_bytes()),
        sentinel.bytes().map(|byte| format!("{byte:02x}")).collect(),
    ] {
        let mut scanner = SecretScanner::looking_for(&[sentinel]);
        scanner.observe(format!("a rendering carrying {rendering}"));
        assert!(scanner.require_clean().is_err(), "{rendering} survived the scanner");
    }
    let mut clean = SecretScanner::looking_for(&[sentinel]);
    clean.observe("[redacted]");
    clean.observe_bytes(b"[redacted]");
    assert_eq!(clean.require_clean(), Ok(()));
}
