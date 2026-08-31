//! What runs next, decided from persisted facts and nothing else.
//!
//! Every fixture is a snapshot and an exact expected order, so the tests state
//! what the scheduler decides rather than that it decided something reasonable.
//! Two of them are about clocks moving the wrong way, which is the case a
//! scheduler is most likely to get wrong and least likely to be told about.

use serde_json::Value;
use slingshot_daemon::operation_scheduler::{
    AdmissionRefusal, OperationScheduler, ScheduledOperation, SchedulerBounds, SchedulerObservation,
};
use slingshot_domain::operation::{
    OperationExecutionCertainty, RecoveryCategory, RecoveryExecutionEvidence, RecoveryFact,
};

/// Decision fixtures this test reads.
const DECISIONS: &str = include_str!("fixtures/operation_scheduler/decisions.jsonl");

/// Operations that may be in flight at once, from the runtime contract.
const GLOBAL_IN_FLIGHT: u64 = 8;

/// Operations that may be waiting at once, from the runtime contract.
const GLOBAL_PENDING: u64 = 256;

/// Operations one caller may have waiting, from the runtime contract.
const PENDING_PER_CALLER: u64 = 64;

/// Operations one tick may select, from the runtime contract.
const SELECTIONS_PER_TICK: u64 = 32;

/// Two-character pairs in a sixty-four-character hexadecimal value.
const DIGEST_PAIRS: usize = 32;

/// Operations the busy caller queues, far more than one tick can take.
const BUSY_BACKLOG: u64 = 100;

/// Where the quiet caller's one operation sits in arrival order.
const QUIET_SEQUENCE: u64 = 1000;

/// Operations the bound test queues, well past every bound at once.
const CROWDED_BACKLOG: u64 = 200;

/// Distinct callers the bound test spreads that backlog across.
const CROWDED_CALLERS: u64 = 7;

/// Returns one row's string member.
fn text<'row>(row: &'row Value, member: &str) -> &'row str {
    row[member].as_str().unwrap_or_else(|| panic!("{member} is a string in {row}"))
}

/// Returns every case of the fixture.
fn cases() -> Vec<Value> {
    DECISIONS
        .lines()
        .map(|line| serde_json::from_str(line).expect("every fixture line is one object"))
        .collect()
}

/// Returns the operation one fixture row describes.
fn operation(row: &Value) -> ScheduledOperation {
    ScheduledOperation {
        author_target_identity_digest: text(row, "author_target_identity_digest").to_owned(),
        caller_identity: Some(text(row, "caller_identity").to_owned()),
        enqueue_sequence: row["enqueue_sequence"].as_u64().expect("a sequence"),
        operation_identifier: text(row, "operation_identifier").to_owned(),
        resume_committed: row["resume_committed"].as_bool().expect("a resume verdict"),
        outstanding_recovery: row.get("recovery").map(|recovery| RecoveryFact {
            attempt_count: 1,
            category: RecoveryCategory::AmbiguousSubmission,
            detail: "outstanding".to_owned(),
            evidence: RecoveryExecutionEvidence::ExecutionCertainty {
                certainty: OperationExecutionCertainty::SubmissionUnknown,
            },
            manual_resume_eligible: true,
            retry_delay_milliseconds: recovery["delay"].as_u64().expect("a delay"),
            retry_observed_at_unix_milliseconds: recovery["observed"].as_u64().expect("an instant"),
        }),
    }
}

/// Returns the observation one fixture case describes.
fn observation(case: &Value) -> SchedulerObservation {
    SchedulerObservation {
        in_flight: case["in_flight"].as_u64().expect("a flight count"),
        waiting: case["waiting"]
            .as_array()
            .expect("a waiting list")
            .iter()
            .map(operation)
            .collect(),
    }
}

/// Returns the identifiers one fixture case expects, in order.
fn expected(case: &Value) -> Vec<String> {
    case["selected"]
        .as_array()
        .expect("a selection list")
        .iter()
        .map(|value| value.as_str().expect("an identifier").to_owned())
        .collect()
}

/// Returns a scheduler held to the contract's own bounds.
fn scheduler() -> OperationScheduler {
    OperationScheduler::new(SchedulerBounds::embedded())
}

#[test]
fn the_bounds_are_the_manifest_s_and_this_module_declares_none() {
    let bounds = SchedulerBounds::embedded();
    assert_eq!(bounds.global_in_flight, GLOBAL_IN_FLIGHT);
    assert_eq!(bounds.global_pending, GLOBAL_PENDING);
    assert_eq!(bounds.pending_per_caller, PENDING_PER_CALLER);
    assert_eq!(bounds.selections_per_tick, SELECTIONS_PER_TICK);
    assert!(
        bounds.global_in_flight < bounds.selections_per_tick,
        "so what one tick may select is never what limits a busy daemon; the flight is"
    );
}

#[test]
fn every_fixture_produces_the_exact_order_it_states() {
    let scheduler = scheduler();
    let fixtures = cases();
    assert!(fixtures.len() >= 12, "fairness, order, both clock directions, and both bounds");
    for case in &fixtures {
        let selected: Vec<String> = scheduler
            .select(&observation(case), case["now"].as_u64().expect("an instant"))
            .into_iter()
            .map(|directive| directive.operation_identifier)
            .collect();
        assert_eq!(selected, expected(case), "{}", text(case, "note"));
    }
}

#[test]
fn the_same_snapshot_decides_the_same_way_however_the_rows_arrived() {
    let scheduler = scheduler();
    for case in cases() {
        let held = observation(&case);
        let mut reversed = held.clone();
        reversed.waiting.reverse();
        let now = case["now"].as_u64().expect("an instant");
        assert_eq!(
            scheduler.select(&held, now),
            scheduler.select(&reversed, now),
            "{}: the repository's row order is not part of the decision",
            text(&case, "note")
        );
    }
}

#[test]
fn a_busy_caller_cannot_starve_a_quiet_one_however_long_it_goes_on() {
    let scheduler = scheduler();
    let mut waiting: Vec<ScheduledOperation> = (1..=BUSY_BACKLOG)
        .map(|index| ScheduledOperation {
            author_target_identity_digest: "1d".repeat(DIGEST_PAIRS),
            caller_identity: Some("busy".to_owned()),
            enqueue_sequence: index,
            operation_identifier: format!("busy-{index}"),
            resume_committed: false,
            outstanding_recovery: None,
        })
        .collect();
    waiting.push(ScheduledOperation {
        author_target_identity_digest: "1d".repeat(DIGEST_PAIRS),
        caller_identity: Some("quiet".to_owned()),
        enqueue_sequence: QUIET_SEQUENCE,
        operation_identifier: "quiet-1".to_owned(),
        resume_committed: false,
        outstanding_recovery: None,
    });

    let selected = scheduler.select(&SchedulerObservation { in_flight: 0, waiting }, 0);
    let position = selected
        .iter()
        .position(|directive| directive.operation_identifier == "quiet-1")
        .expect("the quiet caller was selected");
    assert!(
        position <= 1,
        "the quiet caller's turn comes on the first pass, not after a hundred: {position}"
    );
}

#[test]
fn no_decision_ever_exceeds_a_bound() {
    let scheduler = scheduler();
    let waiting: Vec<ScheduledOperation> = (1..=CROWDED_BACKLOG)
        .map(|index| ScheduledOperation {
            author_target_identity_digest: "1d".repeat(DIGEST_PAIRS),
            caller_identity: Some(format!("caller-{}", index % CROWDED_CALLERS)),
            enqueue_sequence: index,
            operation_identifier: format!("operation-{index}"),
            resume_committed: false,
            outstanding_recovery: None,
        })
        .collect();

    for in_flight in 0..=GLOBAL_IN_FLIGHT {
        let selected =
            scheduler.select(&SchedulerObservation { in_flight, waiting: waiting.clone() }, 0);
        let count = u64::try_from(selected.len()).expect("a countable selection");
        assert!(
            in_flight + count <= GLOBAL_IN_FLIGHT,
            "{in_flight} in flight plus {count} selected crosses the bound"
        );
        assert!(count <= SELECTIONS_PER_TICK, "and no tick selects more than it may");
    }
}

#[test]
fn admission_refuses_at_each_bound_and_reports_counts_that_are_still_true() {
    let scheduler = scheduler();
    let filled = |caller: &str, count: u64| SchedulerObservation {
        in_flight: 0,
        waiting: (1..=count)
            .map(|index| ScheduledOperation {
                author_target_identity_digest: "1d".repeat(DIGEST_PAIRS),
                caller_identity: Some(caller.to_owned()),
                enqueue_sequence: index,
                operation_identifier: format!("operation-{index}"),
                resume_committed: false,
                outstanding_recovery: None,
            })
            .collect(),
    };

    scheduler
        .require_room(&filled("alice", PENDING_PER_CALLER - 1), Some("alice"))
        .expect("room below the caller bound");
    let refused = scheduler.require_room(&filled("alice", PENDING_PER_CALLER), Some("alice"));
    assert!(
        matches!(refused, Err(AdmissionRefusal::CallerPending { held, limit })
            if held == PENDING_PER_CALLER && limit == PENDING_PER_CALLER),
        "one caller at its bound is refused: {refused:?}"
    );
    scheduler
        .require_room(&filled("alice", PENDING_PER_CALLER), Some("bob"))
        .expect("while another caller still has room of its own");

    let refused = scheduler.require_room(&filled("alice", GLOBAL_PENDING), Some("bob"));
    assert!(
        matches!(refused, Err(AdmissionRefusal::GlobalPending { held, limit })
            if held == GLOBAL_PENDING && limit == GLOBAL_PENDING),
        "and the namespace bound refuses everyone: {refused:?}"
    );
}
