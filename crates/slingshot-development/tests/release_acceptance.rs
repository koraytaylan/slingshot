//! Whether one source revision is releasable, and what makes that answerable.
//!
//! Two properties carry this suite. The isolation has to actually deny what it
//! says it denies - so every flag that makes a denial real is weakened one at a
//! time and each weakening is refused by name, because a contract that admitted
//! one of them would be a description rather than a boundary. And the gate
//! inventory has to be complete, ordered, and unanimous: a gate that did not
//! run is not a gate that passed, and the five ways a record can be wrong are
//! five different defects with one consequence.
//!
//! The runner is held to preparing nothing. Acceptance that fetched a missing
//! input, installed a missing tool, or repaired a dirty tree would be accepting
//! whatever it managed to assemble rather than what it was given.

use std::path::{Path, PathBuf};

use serde_json::{Value, json};
use slingshot_development::github_automation_authority::{AUTHORITY_PATH, parse_authority};
use slingshot_development::release_acceptance::{
    AcceptanceManifest, AcceptanceRefusal, CONTAINER_PATH, HELD, MANIFEST_FORMAT, NETWORK_NONE,
    RELEASABLE, REQUIRED_GATES, SCHEMA_PATH, parse_container, parse_manifest, require_complete,
    require_revision,
};

/// Where the fixtures live.
const FIXTURES: &str = "tests/fixtures/release-acceptance";

/// The revision a fixture acceptance run is about.
const SOURCE_COMMIT: &str = "1111111111111111111111111111111111111111";

/// How many characters a digest is written in.
const DIGEST_CHARACTERS: usize = 64;

/// How many characters a commit is written in.
const COMMIT_CHARACTERS: usize = 40;

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

/// Returns which refusal one failure is.
fn refusal_name(failure: &AcceptanceRefusal) -> &'static str {
    match failure {
        AcceptanceRefusal::Unreadable(_) => "Unreadable",
        AcceptanceRefusal::ForeignFormat { .. } => "ForeignFormat",
        AcceptanceRefusal::IsolationWeakened(_) => "IsolationWeakened",
        AcceptanceRefusal::GateMissing(_) => "GateMissing",
        AcceptanceRefusal::GateRepeated(_) => "GateRepeated",
        AcceptanceRefusal::GateOutOfOrder { .. } => "GateOutOfOrder",
        AcceptanceRefusal::GateRefused(_) => "GateRefused",
        AcceptanceRefusal::GateUnknown(_) => "GateUnknown",
        AcceptanceRefusal::RevisionDrift { .. } => "RevisionDrift",
        AcceptanceRefusal::OutcomeUnsupported(_) => "OutcomeUnsupported",
    }
}

/// Returns one complete acceptance manifest, as a run would write it.
fn complete_manifest() -> Value {
    let digest = "a".repeat(DIGEST_CHARACTERS);
    json!({
        "coordinator_row": "x86_64-unknown-linux-gnu",
        "format": MANIFEST_FORMAT,
        "gates": REQUIRED_GATES
            .iter()
            .map(|name| json!({ "name": name, "outcome": HELD, "report_sha256": digest }))
            .collect::<Vec<Value>>(),
        "isolation_sha256": digest,
        "outcome": RELEASABLE,
        "platform_evidence_sha256": digest,
        "provider_run": ".github/workflows/release.yml@refs/heads/main",
        "rustsec_review_record_sha256": digest,
        "source_commit": SOURCE_COMMIT,
        "source_tree": SOURCE_COMMIT,
    })
}

/// Returns the manifest one value parses into.
fn parsed(held: &Value) -> AcceptanceManifest {
    parse_manifest(&held.to_string()).expect("the manifest parses")
}

#[test]
fn the_committed_isolation_denies_everything_it_says_it_denies() {
    let held = parse_container(&read_repository_file(CONTAINER_PATH)).expect("it parses");
    assert_eq!(held.isolation.network, NETWORK_NONE);
    assert!(!held.isolation.privileged);
    assert!(held.isolation.no_new_privileges);
    assert!(held.isolation.read_only_root);
    assert!(!held.isolation.host_engine_socket);
    assert!(held.isolation.host_namespaces.is_empty());
    assert!(held.isolation.host_devices.is_empty());
    assert!(held.isolation.add_capabilities.is_empty());
    assert!(held.runtime.rootless && held.runtime.daemonless);
    assert!(!held.image.pull, "the image is loaded from what was transferred");
    assert!(!held.mounts.read_only.is_empty(), "every input is mounted read-only");
    assert!(!held.mounts.writable_output_root.is_empty(), "one writable root, and one only");
}

#[test]
fn every_weakening_of_the_isolation_is_refused_for_its_own_reason() {
    let committed = read_repository_file(CONTAINER_PATH);
    let declared = fixture_rows("weakened-isolation.jsonl");
    assert!(!declared.is_empty());
    for row in declared {
        let name = row["name"].as_str().expect("a name");
        let find = row["find"].as_str().expect("a find");
        assert!(committed.contains(find), "{name}: the contract has no {find:?}");
        let altered = committed.replacen(find, row["replace"].as_str().expect("a replacement"), 1);
        let failure = parse_container(&altered).expect_err(&format!("{name} was accepted"));
        assert_eq!(
            refusal_name(&failure),
            row["refusal"].as_str().expect("a refusal"),
            "{name}: {failure}"
        );
    }
}

#[test]
fn the_coordinator_is_the_row_the_owner_declared() {
    let container = parse_container(&read_repository_file(CONTAINER_PATH)).expect("it parses");
    let authority = parse_authority(&read_repository_file(AUTHORITY_PATH)).expect("it parses");
    let coordinator =
        authority.row.iter().find(|row| row.coordinator).expect("one row is the coordinator");
    assert_eq!(container.coordinator.triple, coordinator.triple);
    assert_eq!(container.coordinator.runner_selector, coordinator.runner_selector);
}

#[test]
fn a_complete_run_records_every_gate_once_in_order_and_all_holding() {
    let held = parsed(&complete_manifest());
    require_complete(&held).expect("this run is complete");
    require_revision(&held, SOURCE_COMMIT).expect("and about this revision");
    assert_eq!(held.gates.len(), REQUIRED_GATES.len());
}

#[test]
fn a_gate_that_did_not_run_is_not_a_gate_that_passed() {
    let mut manifest = complete_manifest();
    let gates = manifest["gates"].as_array_mut().expect("the gates");
    let removed = gates.remove(0);
    let failure = require_complete(&parsed(&manifest)).expect_err("one gate is missing");
    assert_eq!(refusal_name(&failure), "GateMissing");
    assert!(
        failure.to_string().contains(removed["name"].as_str().expect("a name")),
        "and the refusal names which"
    );
}

#[test]
fn a_gate_recorded_twice_decided_nothing_the_second_time() {
    let mut manifest = complete_manifest();
    let gates = manifest["gates"].as_array_mut().expect("the gates");
    let first = gates[0].clone();
    gates.push(first);
    let failure = require_complete(&parsed(&manifest)).expect_err("one gate is repeated");
    assert_eq!(refusal_name(&failure), "GateRepeated");
}

#[test]
fn gates_recorded_out_of_the_order_they_run_in_are_refused() {
    let mut manifest = complete_manifest();
    let gates = manifest["gates"].as_array_mut().expect("the gates");
    gates.swap(0, 1);
    let failure = require_complete(&parsed(&manifest)).expect_err("they are out of order");
    assert_eq!(refusal_name(&failure), "GateOutOfOrder");
    assert!(
        failure.to_string().contains(REQUIRED_GATES[0]),
        "and the refusal names what belongs there"
    );
}

#[test]
fn one_refused_gate_makes_the_revision_unreleasable() {
    for position in 0..REQUIRED_GATES.len() {
        let mut manifest = complete_manifest();
        manifest["gates"][position]["outcome"] = json!("refused");
        let failure = require_complete(&parsed(&manifest)).expect_err("a gate refused");
        assert_eq!(refusal_name(&failure), "GateRefused");
        assert!(failure.to_string().contains(REQUIRED_GATES[position]), "and names which");
    }
}

#[test]
fn a_gate_acceptance_does_not_require_cannot_be_counted_toward_it() {
    let mut manifest = complete_manifest();
    let digest = "a".repeat(DIGEST_CHARACTERS);
    manifest["gates"]
        .as_array_mut()
        .expect("the gates")
        .push(json!({ "name": "something-else", "outcome": HELD, "report_sha256": digest }));
    let failure = require_complete(&parsed(&manifest)).expect_err("no such gate");
    assert_eq!(refusal_name(&failure), "GateUnknown");
}

#[test]
fn a_run_that_concludes_more_than_its_gates_support_is_refused() {
    let mut manifest = complete_manifest();
    manifest["outcome"] = json!("refused");
    let failure = require_complete(&parsed(&manifest)).expect_err("the outcome is not supported");
    assert_eq!(refusal_name(&failure), "OutcomeUnsupported");
}

#[test]
fn a_manifest_about_another_revision_is_about_another_revision() {
    let held = parsed(&complete_manifest());
    let failure =
        require_revision(&held, &"0".repeat(COMMIT_CHARACTERS)).expect_err("another revision");
    assert_eq!(refusal_name(&failure), "RevisionDrift");
}

#[test]
fn the_schema_and_the_manifest_a_run_writes_agree() {
    let schema: Value =
        serde_json::from_str(&read_repository_file(SCHEMA_PATH)).expect("the schema reads");
    let manifest = complete_manifest();
    for member in schema["required"].as_array().expect("the schema names what is required") {
        let named = member.as_str().expect("a member is named");
        assert!(!manifest[named].is_null(), "a run writes no {named}");
    }
    let properties = schema["properties"].as_object().expect("the schema names its members");
    for named in manifest.as_object().expect("the manifest is an object").keys() {
        assert!(properties.contains_key(named), "the schema describes no {named}");
    }
    assert_eq!(schema["properties"]["format"]["const"].as_str(), Some(MANIFEST_FORMAT));
    assert_eq!(
        schema["properties"]["outcome"]["enum"].as_array().map(Vec::len),
        Some(2),
        "a run is releasable or it is refused, and there is no third answer"
    );
}

#[test]
fn acceptance_prepares_nothing_and_says_so_when_something_is_missing() {
    let runner = read_repository_file("scripts/release_acceptance");
    for preparing in ["cargo install", "curl", "git clone", "git fetch", "pull"] {
        assert!(
            !runner.contains(preparing),
            "acceptance that ran {preparing} would accept whatever it assembled"
        );
    }
    for refused in [
        "name the verified pinned source",
        "name the verified advisory database",
        "name the same-run owner review record",
        "name the verified coordinator cache member",
        "name the authenticated platform evidence",
    ] {
        assert!(runner.contains(refused), "a missing input is refused: {refused}");
    }
    assert!(
        runner.contains("git diff --quiet HEAD"),
        "and a tree that differs from the commit is refused before a gate runs"
    );
}

#[test]
fn every_denial_the_contract_names_appears_in_the_invocation_that_makes_it() {
    let runner = read_repository_file("scripts/release_acceptance");
    for enforced in
        ["--network none", "--read-only", "--cap-drop ALL", "--security-opt no-new-privileges"]
    {
        assert!(runner.contains(enforced), "the invocation does not apply {enforced}");
    }
    let read_only = runner.matches(":ro").count();
    let container = parse_container(&read_repository_file(CONTAINER_PATH)).expect("it parses");
    assert_eq!(
        read_only,
        container.mounts.read_only.len(),
        "every input the contract names read-only is mounted read-only"
    );
    // The writable roots the contract declares: the one evidence leaves through
    // and the one a build works in. A mount beyond those is a mount nothing
    // named, which is the thing this whole document exists to prevent.
    let writable = [&container.mounts.writable_output_root, &container.mounts.writable_build_root];
    assert_eq!(
        runner.matches("--volume").count(),
        container.mounts.read_only.len() + writable.len(),
        "and exactly the writable roots the contract declares, and no other mount"
    );
    for root in writable {
        assert!(runner.contains(root.as_str()), "the invocation does not mount {root}");
    }
}

#[test]
fn the_gates_run_inside_the_container_and_the_host_starts_nothing_after_it() {
    let inside = read_repository_file("scripts/run_acceptance_gates");
    assert!(inside.contains("--frozen --offline"), "the gates resolve nothing");
    assert!(
        inside.contains("SLINGSHOT_ACCEPTANCE_OUTPUT"),
        "and write into the one root that leaves the container"
    );
    let runner = read_repository_file("scripts/release_acceptance");
    let container = runner.find("run --rm").expect("it starts the container");
    let manifest = runner.find("verify-release-acceptance").expect("it verifies the manifest");
    assert!(container < manifest, "the manifest is read after the run that produced it");
    assert!(
        !runner.contains("scripts/quality"),
        "the host runs no gate of its own after the container exits"
    );
}
