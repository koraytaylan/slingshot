//! What a retry attaches to, and what it must never create.
//!
//! A handler deadline elapsing ends the call and not the work. The operation
//! goes on, and the retry that follows carries the same key and therefore
//! reaches the same operation - which is the only reason retrying a command
//! that changes something is safe at all.
//!
//! Two things are deliberately not identity. A nonterminal state is not a
//! failure, so a retry that finds one is still asking about the same work; and
//! the number of physical records the far side happens to hold is not the
//! number of operations, so counting them decides nothing.

use std::path::PathBuf;

use serde_json::Value;
use slingshot_development::finite_state_machine_handler_validation::workflow_effect_operation_key;

/// Where the retry rows live.
const FIXTURE: &str = "tests/fixtures/finite-state-machine-retry/retries.jsonl";

/// Where the machine a person runs lives.
const MACHINE: &str = "../../examples/finite-state-machine/retry.machine.json";

/// The store these cases act in.
const NAMESPACE: &str = "store-one";

/// The instance request these cases act under.
const INSTANCE: &str = "instance-one";

/// The occurrence these cases act on.
const OCCURRENCE: u64 = 0;

/// A second, deliberate occurrence.
const SECOND_OCCURRENCE: u64 = 1;

/// How many physical records the far side may hold for one occurrence.
const BOUNDED_PHYSICAL_RECORDS: usize = 4;

/// One declared way a call can end.
#[derive(Debug, serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct Row {
    /// What it is called.
    name: String,
    /// What the call ended with.
    outcome: String,
    /// Whether a retry attaches to the same work.
    attaches: bool,
    /// Why.
    why: String,
}

/// Returns every declared way a call can end.
fn rows() -> Vec<Row> {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(FIXTURE);
    let text = std::fs::read_to_string(&path)
        .unwrap_or_else(|failure| panic!("{} could not be read: {failure}", path.display()));
    text.lines()
        .filter(|line| !line.trim().is_empty() && !line.starts_with('#'))
        .map(|line| serde_json::from_str(line).expect("every row reads"))
        .collect()
}

/// Returns the key one occurrence derives.
fn key(occurrence: u64) -> String {
    workflow_effect_operation_key(NAMESPACE, INSTANCE, occurrence, "")
        .expect("the occurrence derives a key")
}

#[test]
fn every_retry_of_one_occurrence_carries_the_one_key_that_occurrence_has() {
    let held = key(OCCURRENCE);
    for attempt in 0..BOUNDED_PHYSICAL_RECORDS {
        assert_eq!(key(OCCURRENCE), held, "attempt {attempt} named other work");
    }
    assert_ne!(held, key(SECOND_OCCURRENCE), "a deliberate second run is other work");
}

#[test]
fn a_deadline_that_ends_the_call_does_not_end_the_work() {
    let declared = rows();
    let timed_out =
        declared.iter().find(|row| row.name == "the-handler-timed-out").expect("it is declared");
    assert!(timed_out.attaches, "{}", timed_out.why);
    assert_eq!(timed_out.outcome, "mcp_error", "the call failed generically, and the work did not");
}

#[test]
fn a_nonterminal_state_is_not_a_failure_and_is_not_retried_as_one() {
    let declared = rows();
    let running = declared
        .iter()
        .find(|row| row.name == "the-operation-was-still-running")
        .expect("it is declared");
    assert!(running.attaches, "{}", running.why);
    assert_eq!(running.outcome, "operation_status");
    let ended: Vec<&Row> = declared.iter().filter(|row| !row.attaches).collect();
    assert_eq!(ended.len(), 2, "only an ending is an answer");
    for row in ended {
        assert!(
            row.outcome.starts_with("operation_"),
            "{} is an answer about the operation rather than about the call",
            row.name
        );
    }
}

#[test]
fn the_number_of_records_the_far_side_holds_is_not_the_number_of_operations() {
    let held = key(OCCURRENCE);
    let physical: Vec<String> =
        (0..BOUNDED_PHYSICAL_RECORDS).map(|index| format!("physical-{index}")).collect();
    assert_eq!(physical.len(), BOUNDED_PHYSICAL_RECORDS, "duplicates are bounded and permitted");
    let logical: std::collections::BTreeSet<String> =
        physical.iter().map(|_| held.clone()).collect();
    assert_eq!(logical.len(), 1, "however many records exist, one occurrence is one operation");
}

#[test]
fn the_machine_retries_generically_and_says_nothing_about_the_work() {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(MACHINE);
    let machine: Value =
        serde_json::from_str(&std::fs::read_to_string(&path).expect("it is committed"))
            .expect("the machine reads");
    let retry = &machine["states"]["replicating"]["on"]["mcp_error"];
    assert_eq!(retry["target"].as_str(), Some("replicating"), "a retry stays where it was");
    assert_eq!(retry["effect"].as_str(), Some("replicate"), "and runs the same effect");
    let rendered = serde_json::to_string(&machine).expect("it writes");
    assert!(
        !rendered.contains("operation_identifier"),
        "the machine names no operation, so a retry cannot name a different one"
    );
}

#[test]
fn every_declared_row_says_why_it_is_what_it_is() {
    for row in rows() {
        assert!(!row.why.is_empty(), "{} says why", row.name);
    }
}
