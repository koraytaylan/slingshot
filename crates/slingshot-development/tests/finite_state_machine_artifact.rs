//! Where an answer goes when it does not fit, and what it never becomes.
//!
//! Two branches, and they are disjoint by construction. A command's own result
//! is published as the deterministic operation artifact and addressed through
//! the operation that produced it. A maintenance result is published as an
//! association of a target and addressed by that target and its identifier -
//! because it has no operation, and routing it through the artifact slot would
//! mean inventing one.
//!
//! An invented operation identity is not a tidiness problem. A workflow would
//! journal it, a later step would quote it, and the daemon would be asked about
//! an operation that never existed.

use std::path::PathBuf;

use serde_json::Value;
use slingshot_command_line::model_context_protocol::resource_catalog::{
    ResourceAddress, parse, require_maintenance_identifier,
};
use slingshot_development::finite_state_machine_acknowledgement::{
    Externalization, OPERATION_ARTIFACT_SLOT, acknowledged, externalization_of,
};

/// Where the externalized answers live.
const FIXTURE: &str = "tests/fixtures/finite-state-machine-artifact/externalization.jsonl";

/// Where the machine a person runs lives.
const MACHINE: &str = "../../examples/finite-state-machine/artifact.machine.json";

/// One declared externalized answer.
#[derive(Debug, serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct Row {
    /// What it is called.
    name: String,
    /// What the daemon answered.
    envelope: Value,
    /// Which branch carries it.
    branch: String,
}

/// Returns every declared answer.
fn rows() -> Vec<Row> {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(FIXTURE);
    let text = std::fs::read_to_string(&path)
        .unwrap_or_else(|failure| panic!("{} could not be read: {failure}", path.display()));
    text.lines()
        .filter(|line| !line.trim().is_empty() && !line.starts_with('#'))
        .map(|line| serde_json::from_str(line).expect("every row reads"))
        .collect()
}

/// Returns the row one name belongs to.
fn row(named: &str) -> Row {
    rows().into_iter().find(|held| held.name == named).expect("it is declared")
}

#[test]
fn every_answer_takes_the_branch_the_fixture_names() {
    for held in rows() {
        let expected = match held.branch.as_str() {
            "operation-artifact" => Externalization::OperationArtifact,
            "maintenance-result" => Externalization::MaintenanceResult,
            _ => Externalization::None,
        };
        assert_eq!(externalization_of(&held.envelope), expected, "{}", held.name);
    }
}

#[test]
fn the_artifact_slot_carries_a_command_result_and_only_that() {
    let artifact = row("an-operation-artifact");
    assert_eq!(
        artifact.envelope["artifact"]["artifact_identifier"].as_str(),
        Some(OPERATION_ARTIFACT_SLOT),
        "the deterministic slot is what a command's own result occupies"
    );
    let maintenance = row("a-maintenance-result");
    let rendered = serde_json::to_string(&maintenance.envelope).expect("it writes");
    assert!(
        !rendered.contains(OPERATION_ARTIFACT_SLOT),
        "a maintenance result in the artifact slot would need an operation it has none of"
    );
}

#[test]
fn a_maintenance_result_carries_no_operation_identity_at_all() {
    let held = row("a-maintenance-result");
    let access = &held.envelope["access"];
    for invented in ["operation_identifier", "artifact_identifier", "operation_key", "path"] {
        assert!(access[invented].is_null(), "a maintenance result carries {invented}");
    }
    let uri = access["uri"].as_str().expect("it is addressed");
    let ResourceAddress::MaintenanceResult { maintenance_result_identifier, .. } =
        parse(uri).expect("this server publishes that address")
    else {
        panic!("it is addressed as a maintenance result")
    };
    require_maintenance_identifier(&maintenance_result_identifier)
        .expect("its identifier is one Plan 0004 produces");
    assert!(!uri.contains("/operations/"), "and its address names no operation");
}

#[test]
fn an_operation_artifact_is_addressed_through_the_operation_that_produced_it() {
    let held = row("an-operation-artifact");
    let uri = held.envelope["artifact"]["uri"].as_str().expect("it is addressed");
    let ResourceAddress::Artifact { operation_identifier, artifact_identifier, .. } =
        parse(uri).expect("this server publishes that address")
    else {
        panic!("it is addressed as an artifact")
    };
    assert_eq!(operation_identifier, "one");
    assert_eq!(artifact_identifier, OPERATION_ARTIFACT_SLOT);
}

#[test]
fn an_externalized_answer_is_journalled_whole_because_the_bytes_are_elsewhere() {
    for held in rows() {
        let journalled = acknowledged(&held.envelope, true, "packaged");
        assert!(
            journalled.carried_whole(),
            "{} is an address rather than the bytes, so it fits",
            held.name
        );
        assert_eq!(journalled.structured(), &held.envelope, "{}", held.name);
    }
}

#[test]
fn the_machine_reads_what_was_published_as_a_separate_step() {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(MACHINE);
    let machine: Value =
        serde_json::from_str(&std::fs::read_to_string(&path).expect("it is committed"))
            .expect("the machine reads");
    assert_eq!(
        machine["states"]["packaging"]["on"]["packaged"]["target"].as_str(),
        Some("reading")
    );
    assert!(
        machine["states"]["reading"]["on"]["read"].is_object(),
        "reading what was published is its own step, so a failure to read is not a failure to publish"
    );
}
