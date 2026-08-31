//! What a workflow journals when a command fails, and what that entitles it to.
//!
//! A failure is two separate facts: what went wrong, and whether the work
//! provably ran. The first is a description the author supplied and the second
//! is the daemon's own judgement, and only the second entitles anybody to act.
//! Deriving authority from a category, an error flag, or a message that reads
//! like a failure is how a workflow ends up undoing something that never
//! happened.
//!
//! The machine advances on a declared event that says nothing about authority
//! either. What state the workflow reaches and what it may do next are separate
//! decisions, and joining them would make every failure a licence.

use std::path::PathBuf;

use serde_json::Value;
use slingshot_development::finite_state_machine_acknowledgement::{Authority, acknowledged};

/// Where the endings live.
const FIXTURE: &str = "tests/fixtures/finite-state-machine-failure/failures.jsonl";

/// Where the machine a person runs lives.
const MACHINE: &str = "../../examples/finite-state-machine/failure.machine.json";

/// The event a failed replication advances on.
const FAILED_EVENT: &str = "replication_failed";

/// One declared ending.
#[derive(Debug, serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct Row {
    /// What it is called.
    name: String,
    /// What the daemon answered.
    envelope: Value,
    /// Which authority it establishes.
    authority: String,
    /// Whether that authority permits undoing anything.
    compensable: bool,
}

/// Returns every declared ending.
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
fn every_ending_establishes_the_authority_its_disposition_names_and_no_other() {
    for row in rows() {
        let spelled =
            row.envelope["disposition"].as_str().expect("an ending names its disposition");
        let held = Authority::named(spelled)
            .unwrap_or_else(|| panic!("{} names an unknown disposition", row.name));
        assert_eq!(held.as_text(), row.authority, "{}", row.name);
        assert_eq!(
            held.permits_compensation_review(),
            row.compensable,
            "{} entitles the workflow to the wrong thing",
            row.name
        );
    }
}

#[test]
fn exactly_one_ending_in_the_inventory_may_be_undone() {
    let compensable = rows().into_iter().filter(|row| row.compensable).count();
    assert_eq!(compensable, 1, "only work that provably ran and failed may be undone");
}

#[test]
fn the_failure_category_never_decides_whether_anything_may_be_undone() {
    let rejected_but_not_run = serde_json::json!({
        "outcome": "operation_terminal_error",
        "disposition": "AuthoritativeNonExecution",
        "kind": "Rejected",
        "failure": { "category": "admission_rejected" },
    });
    let disposition = rejected_but_not_run["disposition"].as_str().expect("it names one");
    assert!(
        !Authority::named(disposition).expect("it is known").permits_compensation_review(),
        "the same category with another disposition entitles nothing"
    );
    let same_category_ran = serde_json::json!({ "disposition": "AuthoritativeRemoteFailure" });
    assert!(
        Authority::named(same_category_ran["disposition"].as_str().expect("it names one"))
            .expect("it is known")
            .permits_compensation_review(),
        "the category was the same and the entitlement was not"
    );
}

#[test]
fn a_failed_call_journals_the_semantic_failure_exactly_and_says_the_call_failed() {
    for row in rows() {
        let held = acknowledged(&row.envelope, false, FAILED_EVENT);
        assert!(!held.ok, "{} is a failed call to its caller", row.name);
        assert_eq!(held.structured(), &row.envelope, "{} journalled a summary", row.name);
        assert_eq!(
            held.structured()["failure"],
            row.envelope["failure"],
            "{} lost the failure the author reported",
            row.name
        );
        assert!(held.carried_whole(), "{} needs no digest", row.name);
    }
}

#[test]
fn the_machine_advances_on_a_declared_event_that_says_nothing_about_authority() {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(MACHINE);
    let machine: Value =
        serde_json::from_str(&std::fs::read_to_string(&path).expect("it is committed"))
            .expect("the machine reads");
    let transition = &machine["states"]["replicating"]["on"][FAILED_EVENT];
    assert_eq!(transition["target"].as_str(), Some("failed"));
    let rendered = serde_json::to_string(&machine).expect("it writes");
    for disposition in
        ["AuthoritativeRemoteFailure", "AuthoritativeNonExecution", "FailClosedIndeterminate"]
    {
        assert!(
            !rendered.contains(disposition),
            "the machine names {disposition}, so its state would imply an entitlement"
        );
    }
}
