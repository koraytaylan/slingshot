//! When a workflow may undo something, and what has to happen first.
//!
//! Two gates, in order, and both of them required. The first is evidence: the
//! daemon said the work provably ran and failed. The second is a person: a
//! separate approval event, which the machine waits for and cannot produce
//! itself. Either alone is not enough - evidence without approval would undo
//! things nobody asked to undo, and approval without evidence would undo things
//! that never happened.
//!
//! Compensation is its own effect with its own key suffix, so the operation it
//! creates is a third operation rather than a repeat of either of the first
//! two. A retry of the compensation attaches to that third one, and to nothing
//! else.

use std::path::PathBuf;

use serde_json::Value;
use slingshot_development::finite_state_machine_acknowledgement::Authority;
use slingshot_development::finite_state_machine_handler_validation::{
    EVERY_SUFFIX, workflow_effect_operation_key,
};

/// Where the evidence rows live.
const FIXTURE: &str = "tests/fixtures/finite-state-machine-compensation/evidence.jsonl";

/// Where the machine a person runs lives.
const MACHINE: &str = "../../examples/finite-state-machine/compensation.machine.json";

/// The event a person sends to approve undoing something.
const APPROVAL_EVENT: &str = "backup_restore_approved";

/// The suffix the compensating effect's key carries.
const COMPENSATION_SUFFIX: &str = "-backup-restore";

/// The store these cases act in.
const NAMESPACE: &str = "store-one";

/// The instance request these cases act under.
const INSTANCE: &str = "instance-one";

/// The occurrence these cases act on.
const OCCURRENCE: u64 = 0;

/// One declared piece of evidence.
#[derive(Debug, serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct Row {
    /// What it is called.
    name: String,
    /// What the daemon said about execution.
    disposition: String,
    /// Whether a review may be entered on it.
    reviewable: bool,
}

/// Returns every declared piece of evidence.
fn rows() -> Vec<Row> {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(FIXTURE);
    let text = std::fs::read_to_string(&path)
        .unwrap_or_else(|failure| panic!("{} could not be read: {failure}", path.display()));
    text.lines()
        .filter(|line| !line.trim().is_empty() && !line.starts_with('#'))
        .map(|line| serde_json::from_str(line).expect("every row reads"))
        .collect()
}

/// Returns the committed machine.
fn machine() -> Value {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(MACHINE);
    serde_json::from_str(&std::fs::read_to_string(&path).expect("it is committed"))
        .expect("the machine reads")
}

#[test]
fn only_work_that_provably_ran_and_failed_enters_review() {
    for row in rows() {
        let held = Authority::named(&row.disposition)
            .unwrap_or_else(|| panic!("{} names an unknown disposition", row.name));
        assert_eq!(
            held.permits_compensation_review(),
            row.reviewable,
            "{} enters the wrong place",
            row.name
        );
    }
    assert_eq!(rows().into_iter().filter(|row| row.reviewable).count(), 1);
}

#[test]
fn review_alone_undoes_nothing_and_waits_for_a_person() {
    let held = machine();
    let reviewing = &held["states"]["reviewing"];
    assert!(reviewing["on"][APPROVAL_EVENT]["effect"].is_string(), "approval is what runs it");
    assert_eq!(reviewing["on"][APPROVAL_EVENT]["target"].as_str(), Some("compensating"));
    assert!(
        reviewing["on"]["backup_restore_declined"]["effect"].is_null(),
        "declining runs nothing"
    );
    assert!(
        reviewing["effect"].is_null(),
        "entering review is not itself an effect, so no work happens on arrival"
    );
}

#[test]
fn the_approval_is_an_event_the_machine_cannot_produce_for_itself() {
    let rendered = serde_json::to_string(&machine()).expect("it writes");
    let produced = rendered.matches(APPROVAL_EVENT).count();
    assert_eq!(produced, 1, "the approval appears once, as something awaited");
    for state in machine()["states"].as_object().expect("the states are an object").values() {
        let advances = state["on"].as_object().cloned().unwrap_or_default();
        for (event, transition) in advances {
            if event == APPROVAL_EVENT {
                continue;
            }
            assert_ne!(
                transition["target"].as_str(),
                Some("compensating"),
                "{event} reaches compensation without an approval"
            );
        }
    }
}

#[test]
fn the_compensating_effect_creates_a_third_operation_rather_than_repeating_either() {
    let ordinary = workflow_effect_operation_key(NAMESPACE, INSTANCE, OCCURRENCE, "")
        .expect("the ordinary effect derives a key");
    let compensating =
        workflow_effect_operation_key(NAMESPACE, INSTANCE, OCCURRENCE, COMPENSATION_SUFFIX)
            .expect("the compensating effect derives a key");
    assert_ne!(ordinary, compensating, "undoing something is not doing it again");
    assert!(compensating.starts_with(&ordinary), "and it is about the same occurrence");
    assert!(EVERY_SUFFIX.contains(&COMPENSATION_SUFFIX));

    let retried =
        workflow_effect_operation_key(NAMESPACE, INSTANCE, OCCURRENCE, COMPENSATION_SUFFIX)
            .expect("a retry derives a key");
    assert_eq!(retried, compensating, "a retry of the compensation is the same compensation");
}

#[test]
fn a_compensation_that_fails_reaches_the_failed_state_rather_than_trying_again() {
    let held = machine();
    let compensating = &held["states"]["compensating"]["on"];
    assert_eq!(compensating["backup_restored"]["target"].as_str(), Some("compensated"));
    assert_eq!(compensating["backup_restore_failed"]["target"].as_str(), Some("failed"));
    assert!(
        compensating["backup_restore_failed"]["effect"].is_null(),
        "a failed compensation runs nothing further on its own"
    );
}
