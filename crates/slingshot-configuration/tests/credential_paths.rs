//! Assertions for resolving credential and certificate references.
//!
//! Every accepted reference must land inside the configuration root, and every
//! spelling that could land outside it must be refused rather than repaired. A
//! reference that has to be sanitized before it is safe is a reference whose
//! meaning was never agreed on, so the table below is a table of decisions, not
//! of rewrites.
//!
//! A generated component test makes the same claim over inputs nobody wrote
//! down: whatever the grammar accepts, the resolved path is a strict descendant
//! of the root once the platform has normalized it.

use std::path::{Component, Path, PathBuf};

use proptest::prelude::*;
use serde::Deserialize;
use slingshot_configuration::configuration_root::{AccountIdentity, ConfigurationRoot};
use slingshot_configuration::credential_path::CredentialPath;
use slingshot_domain::configuration_snapshot::ConfigurationReference;
use slingshot_domain::profile_authentication_contract::ConfigurationFailureCode;

/// Fixture that records every accepted and refused reference.
const REFERENCE_FIXTURE: &str = "tests/fixtures/configuration-root/reference-paths.toml";

/// Root every resolution in this file is made against.
const TEST_ROOT: &str = "/slingshot-test-root/.config/slingshot";

/// Account identity the test root is owned by.
const TEST_IDENTIFIER: u32 = 1_000;

/// Grammar every generated reference component matches.
const GENERATED_COMPONENT: &str = "[A-Za-z0-9][A-Za-z0-9._-]{0,8}";

/// Most components one generated reference names.
const GENERATED_COMPONENTS: usize = 8;

/// The reference fixture.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct ReferencePaths {
    /// Format identifier of the fixture.
    format: String,
    /// References that resolve inside the root.
    accepted: Vec<AcceptedReference>,
    /// References that cannot be resolved at all.
    refused: Vec<RefusedReference>,
}

/// One reference that resolves.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct AcceptedReference {
    /// Reference as a profile spells it.
    reference: String,
}

/// One reference that is refused.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct RefusedReference {
    /// Reference as a profile spells it.
    reference: String,
    /// Why it cannot be accepted.
    reason: String,
}

/// Returns the root every resolution here is made against.
fn test_root() -> ConfigurationRoot {
    ConfigurationRoot::at_explicit_path(
        AccountIdentity::UnixUser(TEST_IDENTIFIER),
        PathBuf::from(TEST_ROOT),
    )
}

/// Returns the committed reference table.
fn reference_paths() -> ReferencePaths {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(REFERENCE_FIXTURE);
    let text = std::fs::read_to_string(&path)
        .unwrap_or_else(|failure| panic!("{} could not be read: {failure}", path.display()));
    toml::from_str(&text).expect("the reference table reads")
}

/// Reports whether `path` is a strict descendant of `root` after the platform
/// has resolved every component it understands.
fn is_strict_descendant(root: &Path, path: &Path) -> bool {
    let normalized: PathBuf =
        path.components().filter(|component| !matches!(component, Component::CurDir)).collect();
    normalized != root
        && normalized.starts_with(root)
        && !normalized.components().any(|component| matches!(component, Component::ParentDir))
}

#[test]
fn every_accepted_reference_resolves_inside_the_root() {
    let root = test_root();
    let table = reference_paths();
    assert_eq!(table.format, "slingshot.reference-paths/1");
    for accepted in table.accepted {
        let reference = ConfigurationReference::parse(&accepted.reference)
            .unwrap_or_else(|failure| panic!("{} was refused: {failure}", accepted.reference));
        let resolved = CredentialPath::resolve(&root, &reference)
            .unwrap_or_else(|failure| panic!("{} was refused: {failure}", accepted.reference));
        assert!(
            is_strict_descendant(root.path(), resolved.path()),
            "{} left the root",
            accepted.reference
        );
        assert_eq!(resolved.reference(), &reference);
        let components: Vec<&str> = resolved.components().collect();
        assert_eq!(components.join("/"), accepted.reference, "the components changed");
        assert_eq!(root.path().join(components.join("/")), resolved.path());
    }
}

#[test]
fn every_refused_reference_is_refused_before_it_becomes_a_path() {
    let root = test_root();
    for refused in reference_paths().refused {
        let Ok(reference) = ConfigurationReference::parse(&refused.reference) else {
            continue;
        };
        let resolved = CredentialPath::resolve(&root, &reference);
        assert!(resolved.is_err(), "{} was accepted despite {}", refused.reference, refused.reason);
    }
}

#[test]
fn a_refused_reference_reports_the_contract_code_without_naming_itself() {
    let root = test_root();
    let sentinel = "credentials/not-a-real-secret.json";
    let reference = ConfigurationReference::parse(sentinel).expect("the reference is valid");
    let resolved = CredentialPath::resolve(&root, &reference).expect("the reference resolves");
    assert!(resolved.path().ends_with("not-a-real-secret.json"));

    for spelling in ["../outside.json", "/etc/passwd", "credentials\\production.json"] {
        let failure = ConfigurationReference::parse(spelling).expect_err("the spelling is refused");
        assert_eq!(failure.code, ConfigurationFailureCode::ConfigurationReferenceInvalid);
        let rendered = format!("{failure}");
        assert!(!rendered.contains(spelling), "{rendered} names the reference");
    }
}

proptest! {
    /// Every reference the grammar accepts resolves to a strict descendant.
    #[test]
    fn a_generated_reference_cannot_normalize_outside_the_root(
        components in proptest::collection::vec(GENERATED_COMPONENT, 1..GENERATED_COMPONENTS)
    ) {
        let root = test_root();
        let spelling = components.join("/");
        let reference = ConfigurationReference::parse(&spelling).expect("the grammar accepts it");
        let resolved = CredentialPath::resolve(&root, &reference).expect("it resolves");
        prop_assert!(is_strict_descendant(root.path(), resolved.path()));
        prop_assert!(resolved.path().is_absolute());
    }
}
