//! The lifecycle as a fold, with every edge and every pairing enumerated.
//!
//! Three exhaustive tables carry this. All thirty-six state pairs, all
//! twenty-eight category-and-evidence pairs, and all forty-nine
//! kind-and-disposition pairs are checked - not a sample, because the ones that
//! matter are precisely the combinations nobody thought to write a case for.
//!
//! The pairing rules are where the meaning lives. `RecoveryWindowExpired` may
//! only accompany an indeterminate disposition: once a remote provably
//! succeeded and only retrieval remains, the honest failure is
//! `ResultUnavailable` with authoritative remote success. Letting those two
//! swap would report proven work as never attempted.

use serde_json::Value;
use slingshot_domain::operation::{
    LifecycleFailure, OperationExecutionCertainty, OperationFact, OperationLifecycleState,
    OperationRecord, RecoveryCategory, RecoveryExecutionEvidence, RecoveryFact, TerminalFailure,
    TerminalFailureDisposition, TerminalFailureKind, remaining_delay_milliseconds,
    terminal_pairing_is_legal,
};

/// Transition vectors this test reads.
const TRANSITIONS: &str = include_str!("fixtures/operation_lifecycle/transitions.jsonl");

/// Evidence vectors this test reads.
const EVIDENCE: &str = include_str!("fixtures/operation_lifecycle/evidence.jsonl");

/// Terminal vectors this test reads.
const TERMINALS: &str = include_str!("fixtures/operation_lifecycle/terminals.jsonl");

/// Deadline vectors this test reads.
const DEADLINES: &str = include_str!("fixtures/operation_lifecycle/deadlines.jsonl");

/// Milliseconds a sample recovery waits before retrying.
const SAMPLE_RETRY_DELAY: u64 = 500;

/// Instant a sample recovery measured its delay from.
const SAMPLE_OBSERVED_AT: u64 = 1000;

/// Attempts a sample recovery has already made.
const SAMPLE_ATTEMPT_COUNT: u32 = 2;

/// Reads one row's string member.
fn text<'row>(row: &'row Value, member: &str) -> &'row str {
    row[member].as_str().unwrap_or_else(|| panic!("{member} is a string in {row}"))
}

/// Returns every row of one fixture.
fn rows(fixture: &str) -> Vec<Value> {
    fixture
        .lines()
        .map(|line| serde_json::from_str(line).expect("every fixture line is one object"))
        .collect()
}

/// Returns the state one spelling names.
fn state_of(spelling: &str) -> OperationLifecycleState {
    serde_json::from_value(Value::from(spelling)).expect("a state this contract has")
}

/// Returns an operation sitting in `state`.
fn record_in(state: OperationLifecycleState) -> OperationRecord {
    if state == OperationLifecycleState::Failed {
        return OperationRecord::admitted()
            .fold(&OperationFact::Terminal {
                failure: TerminalFailure {
                    disposition: TerminalFailureDisposition::AuthoritativeRemoteFailure,
                    kind: TerminalFailureKind::RemoteFailed,
                    metadata: None,
                },
            })
            .expect("a legal terminal failure");
    }
    let mut record = OperationRecord::admitted();
    for step in [
        OperationLifecycleState::Submitting,
        OperationLifecycleState::Accepted,
        OperationLifecycleState::Running,
        OperationLifecycleState::Succeeded,
    ] {
        if record.lifecycle_state == state {
            break;
        }
        record = record
            .fold(&OperationFact::Lifecycle { lifecycle_state: step })
            .expect("a forward step");
    }
    record
}

#[test]
fn every_one_of_the_thirty_six_state_pairs_lands_where_the_fixture_says() {
    let vectors = rows(TRANSITIONS);
    assert_eq!(vectors.len(), 36, "six states, both ways, including each with itself");
    for row in &vectors {
        let held = state_of(text(row, "held"));
        let proposed = state_of(text(row, "proposed"));
        let record = record_in(held);
        assert_eq!(record.lifecycle_state, held, "the vector's starting state");
        let outcome = record.fold(&OperationFact::Lifecycle { lifecycle_state: proposed });
        let note = text(row, "note");
        match (text(row, "outcome"), outcome) {
            ("no_op", Ok(folded)) => {
                assert_eq!(folded.revision, record.revision, "{note}: a duplicate costs nothing");
                assert_eq!(folded.lifecycle_state, held, "{note}");
            }
            ("advanced", Ok(folded)) => {
                assert_eq!(folded.lifecycle_state, proposed, "{note}");
                assert_eq!(folded.revision, record.revision + 1, "{note}: exactly one revision");
            }
            ("already_terminal", Err(LifecycleFailure::AlreadyTerminal)) => (),
            ("transition_not_allowed", Err(LifecycleFailure::TransitionNotAllowed)) => (),
            (expected, actual) => panic!("{note}: expected {expected}, got {actual:?}"),
        }
    }
}

#[test]
fn a_terminal_operation_is_unchanged_by_everything_after_it() {
    for terminal in [OperationLifecycleState::Succeeded, OperationLifecycleState::Failed] {
        let record = record_in(terminal);
        assert!(record.lifecycle_state.is_terminal());
        for state in [
            OperationLifecycleState::Queued,
            OperationLifecycleState::Submitting,
            OperationLifecycleState::Running,
        ] {
            assert_eq!(
                record.fold(&OperationFact::Lifecycle { lifecycle_state: state }),
                Err(LifecycleFailure::AlreadyTerminal),
                "{terminal:?} does not become {state:?}"
            );
        }
        let progress = record
            .fold(&OperationFact::Progress { detail: "late news".to_owned() })
            .expect("a late note is not an error");
        assert_eq!(progress, record, "it is simply no longer news");
    }
}

#[test]
fn every_category_carries_only_the_evidence_its_kind_allows() {
    let vectors = rows(EVIDENCE);
    assert_eq!(vectors.len(), 28, "seven categories against every evidence shape");
    for row in &vectors {
        let category: RecoveryCategory =
            serde_json::from_value(row["category"].clone()).expect("a category");
        let evidence: RecoveryExecutionEvidence =
            serde_json::from_value(row["evidence"].clone()).expect("an evidence shape");
        let admitted = row["admitted"].as_bool().expect("a verdict");
        assert_eq!(category.admits(evidence), admitted, "{}", text(row, "note"));

        let record = OperationRecord::admitted();
        let recovery = RecoveryFact {
            attempt_count: 1,
            category,
            detail: "a bounded note".to_owned(),
            evidence,
            manual_resume_eligible: false,
            retry_delay_milliseconds: SAMPLE_RETRY_DELAY,
            retry_observed_at_unix_milliseconds: SAMPLE_OBSERVED_AT,
        };
        let outcome = record.fold(&OperationFact::Recovery { recovery: recovery.clone() });
        assert_eq!(outcome.is_ok(), admitted, "{}", text(row, "note"));
        if admitted {
            let folded = outcome.expect("admitted");
            assert_eq!(folded.revision, record.revision + 1);
            assert_eq!(
                folded.fold(&OperationFact::Recovery { recovery }).expect("a duplicate"),
                folded,
                "the same recovery twice is one fact"
            );
        }
    }
}

#[test]
fn every_one_of_the_forty_nine_terminal_pairings_is_judged() {
    let vectors = rows(TERMINALS);
    assert_eq!(vectors.len(), 49, "seven kinds against seven dispositions");
    for row in &vectors {
        let kind: TerminalFailureKind =
            serde_json::from_value(row["kind"].clone()).expect("a kind");
        let disposition: TerminalFailureDisposition =
            serde_json::from_value(row["disposition"].clone()).expect("a disposition");
        let legal = row["legal"].as_bool().expect("a verdict");
        assert_eq!(terminal_pairing_is_legal(kind, disposition), legal, "{}", text(row, "note"));

        let outcome = OperationRecord::admitted().fold(&OperationFact::Terminal {
            failure: TerminalFailure { disposition, kind, metadata: None },
        });
        assert_eq!(outcome.is_ok(), legal, "{}", text(row, "note"));
    }
}

#[test]
fn a_proven_success_and_an_expired_window_are_not_interchangeable() {
    let expired = TerminalFailure {
        disposition: TerminalFailureDisposition::FailClosedIndeterminate {
            certainty: OperationExecutionCertainty::RemoteOutcomeUnknown,
        },
        kind: TerminalFailureKind::RecoveryWindowExpired,
        metadata: None,
    };
    let unavailable = TerminalFailure {
        disposition: TerminalFailureDisposition::AuthoritativeRemoteSuccess,
        kind: TerminalFailureKind::ResultUnavailable,
        metadata: None,
    };
    assert!(terminal_pairing_is_legal(expired.kind, expired.disposition));
    assert!(terminal_pairing_is_legal(unavailable.kind, unavailable.disposition));
    assert!(
        !terminal_pairing_is_legal(
            TerminalFailureKind::RecoveryWindowExpired,
            TerminalFailureDisposition::AuthoritativeRemoteSuccess
        ),
        "a window that expired after a proven success is a result that was unavailable, \
         and calling it an expired window would report proven work as never attempted"
    );
    assert!(!terminal_pairing_is_legal(
        TerminalFailureKind::ResultUnavailable,
        TerminalFailureDisposition::FailClosedIndeterminate {
            certainty: OperationExecutionCertainty::RemoteOutcomeUnknown
        }
    ));
}

#[test]
fn two_terminal_facts_that_differ_conflict_and_one_that_repeats_does_not() {
    let record = OperationRecord::admitted();
    let failure = TerminalFailure {
        disposition: TerminalFailureDisposition::AuthoritativeRemoteFailure,
        kind: TerminalFailureKind::RemoteFailed,
        metadata: Some("the remote said no".to_owned()),
    };
    let ended = record
        .fold(&OperationFact::Terminal { failure: failure.clone() })
        .expect("a legal terminal failure");
    assert_eq!(ended.lifecycle_state, OperationLifecycleState::Failed);
    assert_eq!(
        ended.fold(&OperationFact::Terminal { failure: failure.clone() }).expect("a duplicate"),
        ended,
        "the same ending twice is one ending"
    );
    let differing =
        TerminalFailure { metadata: Some("something else".to_owned()), ..failure.clone() };
    assert_eq!(
        ended.fold(&OperationFact::Terminal { failure: differing }),
        Err(LifecycleFailure::ConflictingFact),
        "two endings that differ cannot both be true"
    );
}

#[test]
fn recovery_is_cleared_when_the_operation_ends() {
    let record = OperationRecord::admitted()
        .fold(&OperationFact::Recovery {
            recovery: RecoveryFact {
                attempt_count: SAMPLE_ATTEMPT_COUNT,
                category: RecoveryCategory::AmbiguousSubmission,
                detail: "a bounded note".to_owned(),
                evidence: RecoveryExecutionEvidence::ExecutionCertainty {
                    certainty: OperationExecutionCertainty::SubmissionUnknown,
                },
                manual_resume_eligible: true,
                retry_delay_milliseconds: SAMPLE_RETRY_DELAY,
                retry_observed_at_unix_milliseconds: SAMPLE_OBSERVED_AT,
            },
        })
        .expect("a legal recovery");
    assert!(record.outstanding_recovery.is_some());
    let ended = record
        .fold(&OperationFact::Terminal {
            failure: TerminalFailure {
                disposition: TerminalFailureDisposition::FailClosedIndeterminate {
                    certainty: OperationExecutionCertainty::SubmissionUnknown,
                },
                kind: TerminalFailureKind::RetryPolicyExhausted,
                metadata: None,
            },
        })
        .expect("a legal terminal failure");
    assert_eq!(ended.outstanding_recovery, None, "nothing is outstanding against an ending");
}

#[test]
fn every_deadline_vector_clamps_the_way_the_fixture_says() {
    for row in &rows(DEADLINES) {
        let recovery = RecoveryFact {
            attempt_count: 1,
            category: RecoveryCategory::AmbiguousSubmission,
            detail: String::new(),
            evidence: RecoveryExecutionEvidence::ExecutionCertainty {
                certainty: OperationExecutionCertainty::SubmissionUnknown,
            },
            manual_resume_eligible: false,
            retry_delay_milliseconds: row["delay"].as_u64().expect("a delay"),
            retry_observed_at_unix_milliseconds: row["observed"].as_u64().expect("an instant"),
        };
        assert_eq!(
            remaining_delay_milliseconds(&recovery, row["now"].as_u64().expect("an instant")),
            row["remaining"].as_u64().expect("a remainder"),
            "{}",
            text(row, "note")
        );
    }
}

#[test]
fn the_lifecycle_holds_nothing_about_a_process_or_a_connection() {
    // Only the code. The module documentation names these deliberately, to say
    // they are somebody else's, and finding them there would be finding the
    // sentence rather than the mistake.
    let code: String = include_str!("../src/operation.rs")
        .lines()
        .filter(|line| !line.trim_start().starts_with("//"))
        .collect::<Vec<&str>>()
        .join("\n");
    // Spelled precisely enough not to catch `EventReconnection`, which is a
    // recovery category rather than a fact about a socket.
    for absent in ["process_identifier", "connection_health", "waiter", "socket", "endpoint"] {
        assert!(!code.contains(absent), "an operation's state does not change when {absent} does");
    }
}
