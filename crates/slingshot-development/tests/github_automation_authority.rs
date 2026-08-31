//! Which repository this is, and which machine each row runs on.
//!
//! The whole point of a committed authority is that a hosted job cannot decide
//! for itself what repository it belongs to. So the suite drives the document
//! that ships, changes one thing at a time, and requires each change to be
//! refused for its own reason - a display name that matches at a different
//! identity, a runner nobody mapped, a claim nobody probed.
//!
//! The committed authority records no repository identity, because the
//! repository does not exist at that address yet. Every hosted run is therefore
//! refused, and a test says so out loud rather than leaving the reader to
//! discover it: an authority with a hole in it that still admitted evidence
//! would be worse than no authority at all.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use serde_json::Value;
use slingshot_development::github_automation_authority::{
    AUTHORITY_FORMAT, AUTHORITY_PATH, AuthorityRefusal, CANONICAL_PREFIX, CREDENTIAL_SHAPED_NAMES,
    GithubAutomationAuthority, PROVIDER, ReportedRun, UNASSIGNED, parse_authority,
    propose_repository_identifier, require_authorized, require_covers,
};
use slingshot_development::supported_platform_matrix;

/// Where the fixtures live.
const FIXTURES: &str = "tests/fixtures/github-automation-authority";

/// A reviewed repository identity, as one would be committed.
const REVIEWED_IDENTIFIER: &str = "424242";

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

/// Returns the rows one fixture states.
fn fixture_rows(name: &str) -> Vec<Value> {
    std::fs::read_to_string(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(FIXTURES).join(name))
        .unwrap_or_else(|failure| panic!("{name} could not be read: {failure}"))
        .lines()
        .filter(|line| !line.trim().is_empty() && !line.starts_with('#'))
        .map(|line| serde_json::from_str(line).expect("every row reads"))
        .collect()
}

/// Returns the committed authority.
fn committed() -> GithubAutomationAuthority {
    parse_authority(&read_repository_file(AUTHORITY_PATH)).expect("the committed authority parses")
}

/// Returns the committed authority with a reviewed identity recorded.
fn reviewed() -> GithubAutomationAuthority {
    let text = read_repository_file(AUTHORITY_PATH).replace(
        &format!("identifier = \"{UNASSIGNED}\""),
        &format!("identifier = \"{REVIEWED_IDENTIFIER}\""),
    );
    parse_authority(&text).expect("a reviewed identity parses")
}

/// Returns which refusal one failure is.
fn refusal_name(failure: &AuthorityRefusal) -> &'static str {
    match failure {
        AuthorityRefusal::Unreadable(_) => "Unreadable",
        AuthorityRefusal::ForeignFormat(_) => "ForeignFormat",
        AuthorityRefusal::ProviderUnacceptable(_) => "ProviderUnacceptable",
        AuthorityRefusal::ValueUnacceptable { .. } => "ValueUnacceptable",
        AuthorityRefusal::CredentialShaped(_) => "CredentialShaped",
        AuthorityRefusal::RepositoryUnassigned => "RepositoryUnassigned",
        AuthorityRefusal::RowMissing(_) => "RowMissing",
        AuthorityRefusal::RowRepeated(_) => "RowRepeated",
        AuthorityRefusal::ExclusiveRole { .. } => "ExclusiveRole",
        AuthorityRefusal::RunUnauthorized { .. } => "RunUnauthorized",
    }
}

/// Returns the run one fixture row describes.
fn reported(held: &Value) -> ReportedRun {
    let named = |member: &str| held[member].as_str().expect("a member").to_owned();
    ReportedRun {
        repository: named("repository"),
        repository_identifier: named("repository_identifier"),
        repository_owner_identifier: named("repository_owner_identifier"),
        workflow_path: named("workflow_path"),
        runner_selector: named("runner_selector"),
    }
}

#[test]
fn the_committed_authority_says_what_the_owner_confirmed() {
    let held = committed();
    assert_eq!(held.format, AUTHORITY_FORMAT);
    assert_eq!(held.provider, PROVIDER);
    assert_eq!(held.repository.owner, "koraytaylan");
    assert_eq!(held.repository.name, "slingshot");
    assert_eq!(held.repository.visibility, "public");
    assert_eq!(
        held.repository.canonical_address,
        format!("{CANONICAL_PREFIX}{}/{}", held.repository.owner, held.repository.name)
    );
    assert!(held.repository.owner_identifier > 0, "the account has an immutable identity");
    assert_eq!(held.release_review.reviewer_policy, "required-reviewers");
    assert!(!held.release_review.environment.is_empty(), "the review runs somewhere protected");
}

#[test]
fn the_authority_maps_exactly_the_supported_targets_once_each() {
    let held = committed();
    let matrix =
        supported_platform_matrix::parse_matrix(&read_repository_file("support/platforms.toml"))
            .expect("the committed matrix is valid");
    let supported: Vec<String> = matrix.target.iter().map(|row| row.triple.clone()).collect();
    require_covers(&held, &supported).expect("every supported target is mapped");
    assert_eq!(held.row.len(), supported.len(), "and nothing else is");
    let selectors: BTreeSet<&str> =
        held.row.iter().map(|row| row.runner_selector.as_str()).collect();
    assert_eq!(selectors.len(), held.row.len(), "two rows share a machine");
    assert_eq!(held.row.iter().filter(|row| row.coordinator).count(), 1);
    assert_eq!(held.row.iter().filter(|row| row.finite_state_machine).count(), 1);
}

#[test]
fn a_supported_target_the_authority_maps_nowhere_is_refused() {
    let held = committed();
    let invented = vec!["riscv64-unknown-linux-gnu".to_owned()];
    let failure = require_covers(&held, &invented).expect_err("nothing maps it");
    assert_eq!(refusal_name(&failure), "RowMissing");
}

#[test]
fn every_row_names_the_probe_that_establishes_each_claim() {
    for row in committed().row {
        assert!(!row.source_protection_probe.trim().is_empty(), "{}", row.triple);
        assert!(!row.network_denial_probe.trim().is_empty(), "{}", row.triple);
        assert_eq!(
            row.source_protection, "digest_observation_only",
            "{}: a hosted job owns its own filesystem, and the claim says so",
            row.triple
        );
        assert_eq!(row.runner_class, "github-hosted", "{}", row.triple);
        assert_eq!(row.toolchain, "1.98.0", "{}: the row builds at the pinned version", row.triple);
    }
}

#[test]
fn every_declared_change_to_the_authority_is_refused_for_its_own_reason() {
    let committed_text = read_repository_file(AUTHORITY_PATH);
    let declared = fixture_rows("refused-documents.jsonl");
    assert!(!declared.is_empty());
    for row in declared {
        let name = row["name"].as_str().expect("a name");
        let find = row["find"].as_str().expect("a find");
        let replace = row["replace"].as_str().expect("a replacement");
        assert!(committed_text.contains(find), "{name}: the committed document has no {find:?}");
        let altered = committed_text.replacen(find, replace, 1);
        let failure = parse_authority(&altered).expect_err(&format!("{name} was accepted"));
        assert_eq!(
            refusal_name(&failure),
            row["refusal"].as_str().expect("a refusal"),
            "{name}: {failure}"
        );
    }
}

#[test]
fn no_hosted_run_authenticates_while_the_repository_has_no_identity() {
    let held = committed();
    assert_eq!(held.repository.identifier, UNASSIGNED, "the repository does not exist yet");
    for row in fixture_rows("reported-runs.jsonl") {
        let failure = require_authorized(&held, &reported(&row["run"]))
            .expect_err("an unassigned identity authorizes nothing");
        assert_eq!(
            refusal_name(&failure),
            "RepositoryUnassigned",
            "{}: a name is not an identity",
            row["name"]
        );
    }
}

#[test]
fn a_reviewed_identity_authorizes_exactly_the_run_that_matches_it() {
    let held = reviewed();
    for row in fixture_rows("reported-runs.jsonl") {
        let name = row["name"].as_str().expect("a name");
        let outcome = require_authorized(&held, &reported(&row["run"]));
        match row["refusal"].as_str() {
            None => outcome.unwrap_or_else(|failure| panic!("{name}: {failure}")),
            Some(expected) => {
                let failure = outcome.expect_err(&format!("{name} was authorized"));
                assert_eq!(refusal_name(&failure), expected, "{name}: {failure}");
            }
        }
    }
}

#[test]
fn the_validator_reads_neither_a_git_remote_nor_the_environment_for_authority() {
    let source =
        read_repository_file("crates/slingshot-development/src/github_automation_authority.rs");
    for reaching in ["Command::new", "std::env::var", "git config", "remote get-url"] {
        assert!(
            !source.contains(reaching),
            "the authority would be whatever the checkout was configured with: it names {reaching}"
        );
    }
}

#[test]
fn nothing_a_credential_would_be_written_under_may_enter_the_document() {
    let committed_text = read_repository_file(AUTHORITY_PATH);
    let declared: Vec<&str> = committed_text
        .lines()
        .map(str::trim)
        .filter(|line| !line.starts_with('#'))
        .filter_map(|line| line.split(" = ").next())
        .collect();
    for named in CREDENTIAL_SHAPED_NAMES {
        assert!(
            !declared.iter().any(|key| key.to_lowercase().contains(named)),
            "the committed authority declares {named}, and it is committed and public"
        );
        let altered = format!("{committed_text}\n{named} = \"held\"\n");
        let failure = parse_authority(&altered).expect_err("a credential-shaped name is refused");
        assert_eq!(refusal_name(&failure), "CredentialShaped", "{named}");
    }
    assert!(
        committed_text.contains("No credential, token, signing key"),
        "and the document says so where a reader will meet it, which the key scan permits"
    );
}

#[test]
fn every_diagnostic_names_what_is_wrong_and_carries_no_secret() {
    let committed_text = read_repository_file(AUTHORITY_PATH);
    let altered = committed_text.replacen(PROVIDER, "another-provider", 1);
    let failure = parse_authority(&altered).expect_err("another provider is refused");
    let stated = failure.to_string();
    assert!(stated.contains("another-provider"), "the diagnostic names what it found");
    assert!(!stated.contains('\n'), "and stays one bounded line");
}

#[test]
fn a_reviewed_identity_comes_from_a_provider_response_rather_than_from_a_guess() {
    let held = committed();
    let response = serde_json::json!({
        "full_name": "koraytaylan/slingshot",
        "id": 424_242_u64,
    })
    .to_string();
    let proposed = propose_repository_identifier(&held, &response).expect("it proposes");
    assert_eq!(proposed, format!("identifier = \"{REVIEWED_IDENTIFIER}\"\n"));

    let elsewhere = serde_json::json!({ "full_name": "somebody/else", "id": 1_u64 }).to_string();
    let failure = propose_repository_identifier(&held, &elsewhere).expect_err("another repository");
    assert_eq!(refusal_name(&failure), "ValueUnacceptable");
}
