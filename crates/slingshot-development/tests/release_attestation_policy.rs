//! What makes an archive's origin believable.
//!
//! The accepted cryptographic fixture is a real attestation that somebody
//! else's release workflow produced, pinned here with its own policy and its
//! own identity. That is deliberate: proving the mechanism against output this
//! build generated would be proving it against itself, and claiming a bundle
//! came from a Slingshot release workflow that does not exist yet would be
//! claiming evidence nobody produced.
//!
//! So the suite proves two separate things. The mechanism works, against a
//! genuine bundle under the policy that bundle satisfies. And Slingshot's own
//! policy is the one the owner approved, refuses everything that would let the
//! thing being verified choose what verifies it, and is bound to a trust-root
//! snapshot whose bytes are checked before any bundle is read.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use base64::Engine as _;
use serde_json::Value;
use slingshot_development::release_attestation_policy::{
    AttestationRefusal, POLICY_FORMAT, POLICY_PATH, ReleaseAttestationPolicy, parse_policy,
    read_statement, require_admissible, require_trusted_root,
};

/// Where the fixtures live.
const FIXTURES: &str = "tests/fixtures/release-attestation-policy";

/// The subject the independently pinned bundle attests.
const INDEPENDENT_SUBJECT: &str = "pkg:npm/sigstore@3.0.0";

/// Returns the workspace root.
fn workspace_root() -> PathBuf {
    slingshot_development::locate_workspace_root(Path::new(env!("CARGO_MANIFEST_DIR")))
        .expect("the development crate lives inside the workspace")
}

/// Returns one repository file's text.
fn read_repository_file(relative: &str) -> String {
    let path = workspace_root().join(relative);
    std::fs::read_to_string(&path)
        .unwrap_or_else(|failure| panic!("{} could not be read: {failure}", path.display()))
}

/// Returns one fixture's text.
fn fixture(name: &str) -> String {
    std::fs::read_to_string(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(FIXTURES).join(name))
        .unwrap_or_else(|failure| panic!("{name} could not be read: {failure}"))
}

/// Returns the rows one fixture states.
fn fixture_rows(name: &str) -> Vec<Value> {
    fixture(name)
        .lines()
        .filter(|line| !line.trim().is_empty() && !line.starts_with('#'))
        .map(|line| serde_json::from_str(line).expect("every row reads"))
        .collect()
}

/// Returns the committed policy.
fn committed() -> ReleaseAttestationPolicy {
    parse_policy(&read_repository_file(POLICY_PATH)).expect("the committed policy parses")
}

/// Returns the policy the independently pinned bundle satisfies.
fn independent() -> ReleaseAttestationPolicy {
    parse_policy(&fixture("independent-policy.toml")).expect("the fixture policy parses")
}

/// Returns which refusal one failure is.
fn refusal_name(failure: &AttestationRefusal) -> &'static str {
    match failure {
        AttestationRefusal::Unreadable(_) => "Unreadable",
        AttestationRefusal::ForeignFormat(_) => "ForeignFormat",
        AttestationRefusal::ValueUnacceptable { .. } => "ValueUnacceptable",
        AttestationRefusal::TrustedRootDrift { .. } => "TrustedRootDrift",
        AttestationRefusal::TrustWouldBeChosenByTheVerified(_) => "TrustWouldBeChosenByTheVerified",
        AttestationRefusal::BundleUnreadable(_) => "BundleUnreadable",
        AttestationRefusal::IdentityUnauthorized { .. } => "IdentityUnauthorized",
        AttestationRefusal::SubjectsUnexpected { .. } => "SubjectsUnexpected",
    }
}

/// Returns the independently pinned bundle with one statement member replaced.
fn bundle_with(pointer: &str, replacement: &str) -> String {
    let mut bundle: Value =
        serde_json::from_str(&fixture("independent-bundle.json")).expect("the bundle reads");
    let payload = bundle["dsseEnvelope"]["payload"].as_str().expect("an envelope payload");
    let decoded =
        base64::engine::general_purpose::STANDARD.decode(payload).expect("the payload decodes");
    let mut statement: Value = serde_json::from_slice(&decoded).expect("the statement reads");
    let held = statement.pointer_mut(pointer).expect("the statement has that member");
    *held = Value::String(replacement.to_owned());
    let rendered = serde_json::to_vec(&statement).expect("the statement renders");
    bundle["dsseEnvelope"]["payload"] =
        Value::String(base64::engine::general_purpose::STANDARD.encode(rendered));
    serde_json::to_string(&bundle).expect("the bundle renders")
}

/// Returns the one subject the independently pinned bundle attests.
fn independent_subjects() -> BTreeSet<String> {
    BTreeSet::from([INDEPENDENT_SUBJECT.to_owned()])
}

#[test]
fn the_committed_policy_is_the_one_the_owner_approved() {
    let held = committed();
    assert_eq!(held.format, POLICY_FORMAT);
    assert_eq!(held.issuer.oidc_issuer, "https://token.actions.githubusercontent.com");
    assert_eq!(held.issuer.instance, "public-good");
    assert_eq!(held.repository.visibility, "public");
    assert_eq!(held.identity.source_repository_uri, "https://github.com/koraytaylan/slingshot");
    assert_eq!(held.identity.workflow_path, ".github/workflows/release.yml");
    assert_eq!(held.identity.runner_environment, "github-hosted");
    assert_eq!(held.verifier.name, "gh");
    assert!(!held.verifier.version.is_empty(), "a verifier's behaviour is its version");
}

#[test]
fn the_policy_stores_no_signing_material_of_any_kind() {
    let held = read_repository_file(POLICY_PATH).to_lowercase();
    for material in ["private key", "-----begin", "signing-key", "token = ", "password"] {
        assert!(!held.contains(material), "the policy carries {material}");
    }
    let flowed = held.replace('#', " ").split_whitespace().collect::<Vec<&str>>().join(" ");
    assert!(
        flowed.contains("nothing in this repository signs anything"),
        "and says so, because a reader should not have to infer it from an absence"
    );
}

#[test]
fn the_trusted_root_is_the_reviewed_snapshot_and_is_checked_before_a_bundle_is_read() {
    let held = committed();
    require_trusted_root(&held, &workspace_root()).expect("the root is the reviewed one");
    assert!(
        held.trusted_root.provenance.contains("sigstore/root-signing@"),
        "the snapshot says exactly where it came from"
    );
    assert_eq!(
        held.trusted_root.provenance.split('@').nth(1).and_then(|rest| rest.split(' ').next()),
        Some("c9bda74ad2221f938f7d2e0295ca3aad2da710a8"),
        "at one full commit"
    );
    let source =
        read_repository_file("crates/slingshot-development/src/release_attestation_policy.rs");
    let root_check = source.find("pub fn require_trusted_root").expect("the check exists");
    let bundle_read = source.find("pub fn read_statement").expect("the read exists");
    assert!(root_check < bundle_read, "the root is authenticated before a bundle is parsed");
}

#[test]
fn a_trusted_root_that_is_not_the_reviewed_bytes_is_refused() {
    let held = committed();
    let root = std::env::temp_dir().join(format!("attestation-root-{}", std::process::id()));
    std::fs::remove_dir_all(&root).ok();
    std::fs::create_dir_all(root.join("compatibility")).expect("a temporary root is created");
    let altered = format!("{} ", read_repository_file(&held.trusted_root.path));
    std::fs::write(root.join(&held.trusted_root.path), altered).expect("the snapshot is written");
    let failure = require_trusted_root(&held, &root).expect_err("one byte is another snapshot");
    assert_eq!(refusal_name(&failure), "TrustedRootDrift");
    std::fs::remove_dir_all(&root).ok();
}

#[test]
fn every_policy_that_would_let_the_verified_choose_its_verifier_is_refused() {
    let committed_text = read_repository_file(POLICY_PATH);
    let declared = fixture_rows("refused-policies.jsonl");
    assert!(!declared.is_empty());
    for row in declared {
        let name = row["name"].as_str().expect("a name");
        let find = row["find"].as_str().expect("a find");
        assert!(committed_text.contains(find), "{name}: the policy has no {find:?}");
        let altered =
            committed_text.replacen(find, row["replace"].as_str().expect("a replacement"), 1);
        let failure = parse_policy(&altered).expect_err(&format!("{name} was accepted"));
        assert_eq!(
            refusal_name(&failure),
            row["refusal"].as_str().expect("a refusal"),
            "{name}: {failure}"
        );
    }
}

#[test]
fn the_independently_pinned_bundle_is_a_real_attestation_of_somebody_else_s_build() {
    let policy = independent();
    let statement = read_statement(&fixture("independent-bundle.json"), "sha512")
        .expect("the bundle carries a statement");
    require_admissible(&policy, &statement, &independent_subjects())
        .expect("the mechanism accepts a genuine bundle under its own policy");
    assert_ne!(
        policy.identity.source_repository_uri,
        committed().identity.source_repository_uri,
        "the fixture is somebody else's build, which is what makes it independent"
    );
    assert_eq!(statement.subjects.len(), 1);
    assert!(!statement.subjects[0].1.is_empty(), "and the subject carries a digest");
}

#[test]
fn slingshot_s_own_policy_refuses_somebody_else_s_bundle() {
    let statement = read_statement(&fixture("independent-bundle.json"), "sha512")
        .expect("the bundle carries a statement");
    let failure = require_admissible(&committed(), &statement, &independent_subjects())
        .expect_err("this is not evidence about this repository");
    assert_eq!(refusal_name(&failure), "IdentityUnauthorized");
    assert!(
        failure.to_string().contains("koraytaylan/slingshot"),
        "and the diagnostic names what was expected"
    );
}

#[test]
fn every_declared_change_to_the_statement_is_refused_for_its_own_reason() {
    let policy = independent();
    let declared = fixture_rows("refused-statements.jsonl");
    assert!(!declared.is_empty());
    for row in declared {
        let name = row["name"].as_str().expect("a name");
        let altered = bundle_with(
            row["pointer"].as_str().expect("a pointer"),
            row["replacement"].as_str().expect("a replacement"),
        );
        let statement = read_statement(&altered, "sha512").expect("it still reads");
        let failure = require_admissible(&policy, &statement, &independent_subjects())
            .expect_err(&format!("{name} was accepted"));
        assert_eq!(
            refusal_name(&failure),
            row["refusal"].as_str().expect("a refusal"),
            "{name}: {failure}"
        );
    }
}

#[test]
fn a_bundle_attesting_more_or_fewer_subjects_than_expected_is_refused() {
    let policy = independent();
    let statement = read_statement(&fixture("independent-bundle.json"), "sha512")
        .expect("the bundle carries a statement");
    let mut extra = independent_subjects();
    extra.insert("pkg:npm/something@1.0.0".to_owned());
    let failure =
        require_admissible(&policy, &statement, &extra).expect_err("a subject is missing");
    assert_eq!(refusal_name(&failure), "SubjectsUnexpected");

    let failure = require_admissible(&policy, &statement, &BTreeSet::new())
        .expect_err("a subject is attested that was not expected");
    assert_eq!(refusal_name(&failure), "SubjectsUnexpected");
}

#[test]
fn a_subject_with_no_digest_under_the_declared_algorithm_is_refused() {
    let policy = committed();
    let statement =
        read_statement(&fixture("independent-bundle.json"), &policy.statement.digest_algorithm)
            .expect("the bundle still reads");
    assert!(
        statement.subjects.iter().all(|(_, digest)| digest.is_empty()),
        "this bundle names its subject under another algorithm"
    );
    let failure = require_admissible(&independent(), &statement, &independent_subjects())
        .expect_err("a subject nobody digested is a subject nobody addressed");
    assert_eq!(refusal_name(&failure), "SubjectsUnexpected");
}

#[test]
fn a_bundle_that_is_not_one_is_refused_rather_than_read_around() {
    for held in ["", "{}", "{\"dsseEnvelope\":{}}", "not json"] {
        let failure = read_statement(held, "sha256").expect_err("this is not a bundle");
        assert_eq!(refusal_name(&failure), "BundleUnreadable", "{held}");
    }
}

#[test]
fn the_pinned_verifier_is_the_one_the_quality_gate_checks() {
    let policy = committed();
    let tools = read_repository_file("support/repository-tools.toml");
    assert!(
        tools.contains(&format!("version = \"{}\"", policy.verifier.version)),
        "the pinned tool table names the verifier version the policy approved"
    );
    assert!(tools.contains(&format!("name = \"{}\"", policy.verifier.name)), "and its name");
}
