//! One logical command, at-least-once delivery, and one effect.
//!
//! Sling delivers at least once, so duplicate records are the normal case and
//! the suite treats them as such. What it holds to account is the effect: one
//! compare-and-set decides who owns the work, and a holder that loses its lease
//! stops being able to record rather than releasing the work to somebody who
//! might run it a second time.
//!
//! The event tests are about a related confusion. An event stream is not a
//! record - events can be missed, replayed, or arrive from a store that has
//! since been rebuilt - so a daemon reconciles against a snapshot rather than
//! accumulating whatever reached it.

use serde_json::Value;
use slingshot_agent_protocol::job_contract::{
    EventVerdict, JobEvent, JobEventKind, JobSnapshot, ObservedJob,
};
use slingshot_domain::logical_agent_operation::{
    LogicalAgentOperation, LogicalExecutionFailure, LogicalExecutionState,
};

/// The generated schema manifest this test reads.
const MANIFEST: &str = include_str!("fixtures/agent-store-and-logical-execution/manifest.json");

/// Two-character pairs in a sixty-four-character hexadecimal value.
const DIGEST_PAIRS: usize = 32;

/// Directories between this crate's manifest and the workspace root.
const WORKSPACE_ROOT_ANCESTORS: usize = 2;

/// The lease one caller holds.
const FIRST_LEASE: u64 = 7;

/// A lease somebody else took afterwards.
const SECOND_LEASE: u64 = FIRST_LEASE + 1;

/// The generation these fixtures follow.
const GENERATION: u64 = 3;

/// The sequence this daemon has already applied.
const APPLIED: u64 = 10;

/// How far ahead of the daemon one reconciling snapshot is.
const SNAPSHOT_AHEAD: u64 = 5;

/// Returns the operation these fixtures follow.
fn operation_identifier() -> String {
    "1d".repeat(DIGEST_PAIRS)
}

/// Returns what this daemon already knows.
fn observed() -> ObservedJob {
    ObservedJob {
        agent_event_store_generation: GENERATION,
        agent_operation_identifier: operation_identifier(),
        applied_sequence: APPLIED,
    }
}

/// Returns one event with the fields a test wants to vary.
fn event(generation: u64, identifier: &str, sequence: u64) -> JobEvent {
    JobEvent {
        agent_event_store_generation: generation,
        agent_operation_identifier: identifier.to_owned(),
        kind: JobEventKind::Progress,
        sequence,
    }
}

#[test]
fn duplicate_physical_records_are_the_normal_case_and_change_nothing() {
    let mut operation = LogicalAgentOperation::recorded();
    operation.physical_record("sling-job-1").expect("a first delivery");
    operation.physical_record("sling-job-1").expect("the same delivery again");
    operation.physical_record("sling-job-0").expect("another record for one command");

    assert_eq!(operation.physical_records, vec!["sling-job-0", "sling-job-1"]);
    assert_eq!(
        operation.state,
        LogicalExecutionState::ExecutionNotStarted,
        "recording deliveries is not starting work"
    );
}

#[test]
fn more_records_than_one_operation_matches_fails_closed() {
    let mut operation = LogicalAgentOperation::recorded();
    let allowed =
        usize::try_from(LogicalAgentOperation::maximum_records()).expect("a countable bound");
    for index in 0..allowed {
        operation.physical_record(&format!("sling-job-{index:04}")).expect("a delivery");
    }
    let refused = operation.physical_record("sling-job-one-too-many");
    assert!(
        matches!(refused, Err(LogicalExecutionFailure::TooManyRecords { .. })),
        "more records than the bound may mean deliveries this daemon does not know about, which \
         is not a state to act on: {refused:?}"
    );
}

#[test]
fn exactly_one_caller_crosses_into_started_and_only_it_may_record_an_effect() {
    let mut operation = LogicalAgentOperation::recorded();
    operation.start(FIRST_LEASE).expect("one caller wins");
    assert_eq!(operation.state, LogicalExecutionState::ExecutionStarted);
    assert_eq!(operation.attempts, 1);

    assert_eq!(
        operation.start(SECOND_LEASE),
        Err(LogicalExecutionFailure::AlreadyStarted),
        "starting is not a thing that happens twice"
    );
    assert_eq!(
        operation.effect(SECOND_LEASE),
        Err(LogicalExecutionFailure::NotTheHolder),
        "and a lost lease never permits a second effect, because the first holder may still be \
         running"
    );

    operation.effect(FIRST_LEASE).expect("the holder may record what it did");
    assert_eq!(operation.state, LogicalExecutionState::Effected);
}

#[test]
fn an_effect_before_a_start_is_refused() {
    let mut operation = LogicalAgentOperation::recorded();
    assert_eq!(
        operation.effect(FIRST_LEASE),
        Err(LogicalExecutionFailure::NotStarted),
        "there is nothing to record against work nobody began"
    );
}

#[test]
fn attempts_run_out_rather_than_retrying_a_failing_remote_without_end() {
    let mut operation = LogicalAgentOperation::recorded();
    let allowed = LogicalAgentOperation::maximum_attempts();
    for _ in 0..allowed {
        operation.start(FIRST_LEASE).expect("an attempt");
        operation.state = LogicalExecutionState::ExecutionNotStarted;
        operation.holder = None;
    }
    assert_eq!(operation.attempts, allowed);
    assert_eq!(
        operation.start(FIRST_LEASE),
        Err(LogicalExecutionFailure::AttemptsExhausted { allowed }),
        "an unbounded retry against a failing remote turns one problem into a larger one"
    );
}

#[test]
fn every_state_a_crash_can_leave_is_one_something_can_act_on() {
    let mut operation = LogicalAgentOperation::recorded();
    assert!(operation.is_recoverable(), "nobody began, so anybody may");
    operation.start(FIRST_LEASE).expect("a start");
    assert!(operation.is_recoverable(), "somebody owns it, whatever happened to them");
    operation.effect(FIRST_LEASE).expect("an effect");
    assert!(operation.is_recoverable(), "and it is finished");
}

#[test]
fn an_event_is_judged_by_generation_then_operation_then_sequence() {
    let observed = observed();
    assert_eq!(
        observed.verdict(&event(GENERATION, &operation_identifier(), APPLIED + 1)),
        EventVerdict::Applies
    );
    assert_eq!(
        observed.verdict(&event(GENERATION, &operation_identifier(), APPLIED)),
        EventVerdict::Superseded,
        "an event this daemon already applied is not news"
    );
    assert_eq!(
        observed.verdict(&event(GENERATION, &"2d".repeat(DIGEST_PAIRS), APPLIED + 1)),
        EventVerdict::AnotherOperation
    );
    assert_eq!(
        observed.verdict(&event(GENERATION + 1, &operation_identifier(), APPLIED + 1)),
        EventVerdict::AnotherGeneration,
        "an event from a rebuilt store describes something that no longer exists, however high \
         its sequence happens to be"
    );
}

#[test]
fn applying_an_event_that_does_not_apply_changes_nothing() {
    let observed = observed();
    for held in [
        event(GENERATION + 1, &operation_identifier(), APPLIED + 1),
        event(GENERATION, &"2d".repeat(DIGEST_PAIRS), APPLIED + 1),
        event(GENERATION, &operation_identifier(), APPLIED),
    ] {
        assert_eq!(observed.applying(&held), observed, "only what applies advances anything");
    }
    assert_eq!(
        observed
            .applying(&event(GENERATION, &operation_identifier(), APPLIED + 1))
            .applied_sequence,
        APPLIED + 1
    );
}

#[test]
fn a_snapshot_behind_the_daemon_is_a_disagreement_rather_than_a_gap() {
    let observed = observed();
    let ahead = JobSnapshot {
        agent_event_store_generation: GENERATION,
        agent_operation_identifier: operation_identifier(),
        kind: JobEventKind::Succeeded,
        sequence: APPLIED + SNAPSHOT_AHEAD,
    };
    assert!(ahead.reconciles_with(&observed), "a snapshot ahead means events were missed");

    let behind = JobSnapshot { sequence: APPLIED - 1, ..ahead.clone() };
    assert!(
        !behind.reconciles_with(&observed),
        "one behind means this daemon applied something the store does not have, which is worth \
         refusing rather than rationalising"
    );
    let elsewhere = JobSnapshot { agent_event_store_generation: GENERATION + 1, ..ahead };
    assert!(!elsewhere.reconciles_with(&observed), "and one from another store is not about this");
}

#[test]
fn only_two_event_kinds_end_a_job() {
    let ending: Vec<JobEventKind> = [
        JobEventKind::Accepted,
        JobEventKind::Started,
        JobEventKind::Progress,
        JobEventKind::Succeeded,
        JobEventKind::Failed,
    ]
    .into_iter()
    .filter(|kind| kind.is_terminal())
    .collect();
    assert_eq!(
        ending,
        vec![JobEventKind::Succeeded, JobEventKind::Failed],
        "accepted and started are news about work that is still going"
    );
}

#[test]
fn every_generated_schema_regenerates_to_the_bytes_the_manifest_records() {
    let manifest: Value = serde_json::from_str(MANIFEST).expect("the manifest is one value");
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(WORKSPACE_ROOT_ANCESTORS)
        .expect("the workspace root")
        .join("schemas/agent-protocol");
    for (relative, recorded) in manifest["schemas"].as_object().expect("a schema list") {
        let bytes = std::fs::read(root.join(relative)).expect("a generated schema reads");
        let digest: String = <sha2::Sha256 as sha2::Digest>::digest(&bytes)
            .iter()
            .map(|octet| format!("{octet:02x}"))
            .collect();
        assert_eq!(Some(digest.as_str()), recorded.as_str(), "{relative} is not what was recorded");
    }
}
