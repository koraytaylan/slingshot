//! What stays true of an operation and a job however they are driven.
//!
//! These are the invariants that a table of examples cannot establish. A
//! transition table is small enough to read and still large enough that the one
//! pair nobody wrote down is the one that lets a finished operation start
//! again, so every pair is generated and every claim is checked against all of
//! them.
//!
//! Each claim is written down beside the suite. A property that quietly stops
//! being checked is worse than one that was never claimed, because the row
//! saying it holds is still there.

use std::path::PathBuf;

use proptest::prelude::*;
use slingshot_domain::operation::OperationLifecycleState;
use slingshot_domain::remote_job::{AgentJobState, JobEventSequence, RemoteJobObservation};

/// Where the claims live.
const CLAIMS: &str = "tests/fixtures/state-properties/claims.txt";

/// Every state an operation can be in.
const EVERY_OPERATION_STATE: &[OperationLifecycleState] = &[
    OperationLifecycleState::Queued,
    OperationLifecycleState::Submitting,
    OperationLifecycleState::Accepted,
    OperationLifecycleState::Running,
    OperationLifecycleState::Succeeded,
    OperationLifecycleState::Failed,
];

/// Every state a job can be in.
const EVERY_JOB_STATE: &[AgentJobState] = &[
    AgentJobState::Queued,
    AgentJobState::Running,
    AgentJobState::Succeeded,
    AgentJobState::Failed,
];

/// How many advances one generated history makes.
const HISTORY_LENGTH: usize = 12;

/// Returns every claim this suite establishes.
fn claims() -> Vec<(String, String)> {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(CLAIMS);
    let text = std::fs::read_to_string(&path)
        .unwrap_or_else(|failure| panic!("{} could not be read: {failure}", path.display()));
    text.lines()
        .filter(|line| !line.trim().is_empty() && !line.starts_with('#'))
        .map(|line| {
            let (name, why) = line.split_once('|').expect("every claim says why");
            (name.to_owned(), why.to_owned())
        })
        .collect()
}

#[test]
fn every_claim_this_suite_makes_is_written_down_and_says_why() {
    let declared = claims();
    assert_eq!(declared.len(), CLAIM_COUNT, "a claim was added or removed without its row");
    for (name, why) in declared {
        assert!(!why.trim().is_empty(), "{name} says why it matters");
    }
}

/// How many claims this suite establishes.
const CLAIM_COUNT: usize = 7;

#[test]
fn an_operation_never_goes_backwards_and_never_leaves_an_ending() {
    for held in EVERY_OPERATION_STATE {
        for next in EVERY_OPERATION_STATE {
            let permitted = held.may_become(*next);
            if held.is_terminal() {
                assert!(!permitted, "{held:?} is an ending and may become {next:?}");
                continue;
            }
            if permitted {
                assert_ne!(held, next, "{held:?} may become itself, which is not a change");
                let backwards = next.may_become(*held);
                assert!(!backwards || next.is_terminal(), "{held:?} and {next:?} are both ways");
            }
        }
    }
}

#[test]
fn every_operation_state_that_is_not_an_ending_can_reach_one() {
    for held in EVERY_OPERATION_STATE.iter().filter(|held| !held.is_terminal()) {
        assert!(
            held.may_become(OperationLifecycleState::Failed),
            "{held:?} cannot fail, and everything can be interrupted by something outside it"
        );
        assert!(held.may_become(OperationLifecycleState::Succeeded), "{held:?} cannot succeed");
    }
}

#[test]
fn a_job_never_returns_to_queued_and_an_ending_is_its_own_only_successor() {
    for held in EVERY_JOB_STATE {
        for next in EVERY_JOB_STATE {
            let permitted = held.may_become(*next);
            if held.is_terminal() {
                assert_eq!(
                    permitted,
                    held == next,
                    "{held:?} is an ending, and only repeating itself is not a change"
                );
                continue;
            }
            if *next == AgentJobState::Queued && *held != AgentJobState::Queued {
                assert!(!permitted, "{held:?} may become queued, which counts work twice");
            }
        }
    }
}

proptest! {
    #[test]
    fn no_generated_history_ever_moves_an_observation_backwards(
        steps in prop::collection::vec((0_usize..EVERY_JOB_STATE.len(), 0_u64..8, 0_u64..8), HISTORY_LENGTH)
    ) {
        let mut held = RemoteJobObservation::accepted();
        let mut sequence = JobEventSequence::first();
        for (index, attempt, progress) in steps {
            let state = EVERY_JOB_STATE[index];
            let next = JobEventSequence::of(sequence.value() + 1);
            let Ok(advanced) = held.advanced(state, next, attempt, progress) else {
                continue;
            };
            prop_assert!(
                advanced.attempt >= held.attempt,
                "an attempt went backwards, so a later observation knew less"
            );
            prop_assert!(advanced.progress >= held.progress, "progress went backwards");
            prop_assert!(
                held.state.may_become(advanced.state),
                "a history reached a state the table forbids"
            );
            held = advanced;
            sequence = next;
        }
    }

    #[test]
    fn an_ending_survives_every_later_observation(
        steps in prop::collection::vec(0_usize..EVERY_JOB_STATE.len(), HISTORY_LENGTH)
    ) {
        let mut held = RemoteJobObservation::accepted();
        let mut sequence = JobEventSequence::first();
        for index in steps {
            let next = JobEventSequence::of(sequence.value() + 1);
            if let Ok(advanced) = held.advanced(EVERY_JOB_STATE[index], next, 0, 0) {
                held = advanced;
                sequence = next;
            }
            if held.state.is_terminal() {
                let later = JobEventSequence::of(sequence.value() + 1);
                let reopened = held.advanced(AgentJobState::Running, later, 0, 0);
                prop_assert!(reopened.is_err(), "an ending was reopened");
            }
        }
    }
}
