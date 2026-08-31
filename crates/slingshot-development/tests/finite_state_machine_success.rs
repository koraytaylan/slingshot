//! What a workflow journals when a command succeeds.
//!
//! The acknowledgement is the durable record a later decision is made from, so
//! the claim that matters is that it carries the envelope exactly. A summary
//! would be a second description of the same outcome, and the two would
//! disagree the first time either changed.
//!
//! The chain of real processes needs the pinned executor, which this
//! environment cannot build; what is proved here is every mapping the chain
//! would journal, and the machine definition a person actually runs.

use std::path::PathBuf;

use serde_json::Value;
use slingshot_development::finite_state_machine_acknowledgement::{
    ACKNOWLEDGEMENT_CAP_BYTES, Authority, DIGEST_MEMBER, Externalization, acknowledged,
    externalization_of,
};

/// Where the acknowledgement rows live.
const FIXTURE: &str = "tests/fixtures/finite-state-machine-success/acknowledgements.jsonl";

/// Where the machine a person runs lives.
const MACHINE: &str = "../../examples/finite-state-machine/success.machine.json";

/// One declared acknowledgement.
#[derive(Debug, serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct Row {
    /// What it is called.
    name: String,
    /// What the daemon answered.
    envelope: Value,
    /// Whether the call succeeded.
    ok: bool,
    /// Which event the machine advances on.
    event: String,
    /// Whether the answer travels whole.
    whole: bool,
}

/// Returns every declared acknowledgement.
fn rows() -> Vec<Row> {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(FIXTURE);
    let text = std::fs::read_to_string(&path)
        .unwrap_or_else(|failure| panic!("{} could not be read: {failure}", path.display()));
    text.lines()
        .filter(|line| !line.trim().is_empty() && !line.starts_with('#'))
        .map(|line| serde_json::from_str(line).expect("every row reads"))
        .collect()
}

#[test]
fn every_successful_outcome_is_journalled_exactly_as_the_daemon_answered_it() {
    for row in rows() {
        let held = acknowledged(&row.envelope, row.ok, &row.event);
        assert_eq!(held.ok, row.ok, "{}", row.name);
        assert_eq!(held.event, row.event, "{}", row.name);
        assert_eq!(held.structured(), &row.envelope, "{} journalled a summary", row.name);
        assert_eq!(held.carried_whole(), row.whole, "{}", row.name);
        assert!(
            held.structured()[DIGEST_MEMBER].is_null(),
            "{} carries a needless digest",
            row.name
        );
    }
}

#[test]
fn an_answer_inside_the_cap_carries_no_digest_and_one_past_it_carries_nothing_else() {
    let inside = serde_json::json!({ "outcome": "operation_result", "result": "held" });
    let held = acknowledged(&inside, true, "replicated");
    assert!(held.carried_whole(), "an answer that fits travels whole");

    let enormous = serde_json::json!({
        "outcome": "operation_result",
        "result": "x".repeat(ACKNOWLEDGEMENT_CAP_BYTES),
    });
    let over = acknowledged(&enormous, true, "replicated");
    assert!(!over.carried_whole(), "an answer that does not fit is identifiable rather than whole");
    let digest = over.structured()[DIGEST_MEMBER].as_str().expect("it carries its digest");
    assert_eq!(digest.len(), DIGEST_CHARACTERS);
    assert!(
        over.structured()["structured_prefix"].as_str().is_some(),
        "and enough of itself to be recognized"
    );
}

/// How many characters a digest is written in.
const DIGEST_CHARACTERS: usize = 64;

#[test]
fn a_successful_command_creates_one_logical_operation_named_by_its_receipt() {
    let declared = rows();
    let receipts: Vec<&Row> = declared
        .iter()
        .filter(|row| row.envelope["outcome"].as_str() == Some("operation_receipt"))
        .collect();
    assert!(!receipts.is_empty());
    let named: std::collections::BTreeSet<&str> =
        receipts.iter().filter_map(|row| row.envelope["operation_identifier"].as_str()).collect();
    assert_eq!(named.len(), 1, "one occurrence is one logical operation, replayed or not");
}

#[test]
fn a_successful_result_externalizes_nothing_and_reserves_the_artifact_slot() {
    for row in rows() {
        assert_eq!(
            externalization_of(&row.envelope),
            Externalization::None,
            "{} externalized something that fits",
            row.name
        );
    }
    let externalized = serde_json::json!({ "outcome": "structured_result_artifact_access" });
    assert_eq!(externalization_of(&externalized), Externalization::OperationArtifact);
    let maintenance = serde_json::json!({ "outcome": "maintenance_result_access" });
    assert_eq!(
        externalization_of(&maintenance),
        Externalization::MaintenanceResult,
        "a maintenance result never takes the operation artifact slot"
    );
}

#[test]
fn success_establishes_no_authority_to_undo_anything() {
    assert!(!Authority::AuthoritativeRemoteSuccess.permits_compensation_review());
    assert!(Authority::AuthoritativeRemoteFailure.permits_compensation_review());
}

#[test]
fn the_machine_a_person_runs_advances_on_the_events_the_handlers_declare() {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(MACHINE);
    let machine: Value =
        serde_json::from_str(&std::fs::read_to_string(&path).expect("the machine is committed"))
            .expect("the machine reads");
    let replicating = &machine["states"]["replicating"]["on"];
    for event in ["replicated", "replication_failed"] {
        assert!(!replicating[event].is_null(), "the machine handles {event}");
    }
    assert_eq!(machine["states"]["done"]["type"].as_str(), Some("final"));
}
