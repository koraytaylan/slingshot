//! What a script branches on, and what a keystroke is allowed to mean.
//!
//! The exit comes from the daemon's disposition and never from what a failure
//! is called. A category is a name the agent chose from a closed list; a
//! disposition is what the daemon derived about execution. Choosing an exit by
//! matching a spelling would make a renamed category change a script's
//! behaviour and make two categories that mean the same thing exit differently,
//! so the suite drives every published category through the classifier and
//! asserts the name has no influence at all.
//!
//! An interrupt is not a cancellation. Stopping watching is stopping watching,
//! so nothing here asks the daemon or the agent to stop anything, and the four
//! phases are the four honest accounts of how far it got. A committed
//! publication wins over a later signal, because the rename that makes a
//! destination appear is the success and a signal cannot turn a finished thing
//! into an interrupted one.

use slingshot_command_line::exit_classification::{
    AGENT_REJECTION, EVERY_EXIT, INDETERMINATE, INTERRUPTED, LOCAL_FAILURE, REMOTE_FAILURE,
    SUCCESS, TerminalDisposition, UNAVAILABLE, USAGE, exit_for, permits_another_attempt,
};
use slingshot_command_line::interrupt::{Phase, SignalOutcome, on_signal};
use slingshot_command_line::machine_outcome_envelope::{Interruption, MachineOutcomeEnvelope};
use slingshot_domain::command::catalog::CommandCatalog;

/// The operation these phases are about.
const OPERATION: &str = "operation-one";

/// One artifact of it.
const ARTIFACT: &str = "artifact-one";

/// The partition a maintenance fetch belongs to.
const TARGET: &str = "target-identity-digest-one";

/// One maintenance result.
const MAINTENANCE_RESULT: &str = "maintenance-result-one";

/// What a caller quotes to find out what happened.
const RETRY_IDENTIFIER: &str = "retry-one";

/// The revision an observed operation was admitted at.
const ADMITTED_REVISION: u64 = 3;

/// Every disposition, with the exit each produces.
const DISPOSITIONS: &[(TerminalDisposition, i32)] = &[
    (TerminalDisposition::AuthoritativeNonExecution, AGENT_REJECTION),
    (TerminalDisposition::AuthoritativeRemoteFailure, REMOTE_FAILURE),
    (TerminalDisposition::AuthoritativeRemoteSuccess, UNAVAILABLE),
    (TerminalDisposition::FailClosedIndeterminate, INDETERMINATE),
];

#[test]
fn every_disposition_produces_its_own_exit_and_no_two_share_one() {
    let mut produced: Vec<i32> = Vec::new();
    for (disposition, expected) in DISPOSITIONS {
        assert_eq!(exit_for(*disposition), *expected, "{disposition:?}");
        assert!(!produced.contains(expected), "{disposition:?} shares an exit with another");
        produced.push(*expected);
    }
    assert_eq!(produced.len(), DISPOSITIONS.len());
    for exit in &produced {
        assert!(EVERY_EXIT.contains(exit), "{exit} is produced and not declared");
    }
}

#[test]
fn the_name_of_a_failure_has_no_influence_on_the_exit_at_all() {
    let mut categories: Vec<String> = CommandCatalog::published()
        .descriptors()
        .iter()
        .flat_map(|descriptor| descriptor.failure_categories.clone())
        .collect();
    categories.sort();
    categories.dedup();
    assert!(!categories.is_empty(), "this build publishes failure categories");
    for category in &categories {
        for (disposition, expected) in DISPOSITIONS {
            assert_eq!(
                exit_for(*disposition),
                *expected,
                "{category}: a renamed category must not change what a script does"
            );
        }
    }
    let source =
        std::fs::read_to_string("src/exit_classification.rs").expect("the classifier is readable");
    for category in &categories {
        assert!(
            !source.contains(category.as_str()),
            "{category} appears in the classifier, so a spelling could choose an exit"
        );
    }
}

#[test]
fn only_a_refusal_and_a_usage_mistake_say_the_command_may_go_again() {
    assert!(
        permits_another_attempt(AGENT_REJECTION),
        "it ran nothing, so fixing it and retrying \
         is the ordinary thing to do"
    );
    assert!(permits_another_attempt(USAGE), "and a usage mistake never reached anything");
    for exit in [SUCCESS, REMOTE_FAILURE, INDETERMINATE, UNAVAILABLE, LOCAL_FAILURE, INTERRUPTED] {
        assert!(
            !permits_another_attempt(exit),
            "{exit}: running it again is exactly the risk these describe"
        );
    }
}

#[test]
fn every_phase_says_what_it_knows_and_nothing_more() {
    let cases = [
        (
            Phase::BeforeReceipt { retry_operation_identifier: RETRY_IDENTIFIER.to_owned() },
            Interruption::PreReceipt { retry_identifier: RETRY_IDENTIFIER.to_owned() },
        ),
        (
            Phase::Observing {
                operation_identifier: OPERATION.to_owned(),
                revision: ADMITTED_REVISION,
            },
            Interruption::PostReceipt {
                operation_identifier: OPERATION.to_owned(),
                revision: ADMITTED_REVISION,
            },
        ),
        (
            Phase::FetchingArtifact {
                artifact_identifier: ARTIFACT.to_owned(),
                operation_identifier: OPERATION.to_owned(),
            },
            Interruption::ArtifactTransfer {
                artifact_identifier: ARTIFACT.to_owned(),
                operation_identifier: OPERATION.to_owned(),
            },
        ),
        (
            Phase::FetchingMaintenanceResult {
                author_target_identity_digest: TARGET.to_owned(),
                maintenance_result_identifier: MAINTENANCE_RESULT.to_owned(),
            },
            Interruption::MaintenanceResultTransfer {
                author_target_identity_digest: TARGET.to_owned(),
                maintenance_result_identifier: MAINTENANCE_RESULT.to_owned(),
            },
        ),
    ];
    for (phase, expected) in cases {
        let outcome = on_signal(&phase);
        assert_eq!(
            outcome,
            SignalOutcome::Interrupted { interruption: expected },
            "a fifth hedging answer would leave a person unsure which of the four they are in"
        );
        assert_eq!(outcome.exit(), INTERRUPTED);
        assert!(!outcome.asked_anything_to_stop());
    }
}

#[test]
fn a_committed_publication_wins_over_a_signal_that_arrives_after_it() {
    let outcome = on_signal(&Phase::Committed);
    assert_eq!(outcome, SignalOutcome::CommittedWork);
    assert_eq!(
        outcome.exit(),
        SUCCESS,
        "the rename that makes a destination appear is the success, and a signal cannot turn a \
         finished thing into an interrupted one"
    );
}

#[test]
fn no_interruption_can_be_rendered_as_something_that_claims_a_remote_fact() {
    for phase in [
        Phase::BeforeReceipt { retry_operation_identifier: RETRY_IDENTIFIER.to_owned() },
        Phase::Observing {
            operation_identifier: OPERATION.to_owned(),
            revision: ADMITTED_REVISION,
        },
    ] {
        let SignalOutcome::Interrupted { interruption } = on_signal(&phase) else {
            panic!("nothing was committed")
        };
        let envelope = MachineOutcomeEnvelope::LocalApplicationError { interruption };
        assert!(!envelope.claims_remote_authority());
        assert_eq!(envelope.tag(), "local_application_error");
    }
}

#[test]
fn a_pre_receipt_signal_keeps_only_the_identifier_a_caller_can_quote() {
    let SignalOutcome::Interrupted { interruption } = on_signal(&Phase::BeforeReceipt {
        retry_operation_identifier: RETRY_IDENTIFIER.to_owned(),
    }) else {
        panic!("nothing was committed")
    };
    let rendered = serde_json::to_string(&interruption).expect("it serializes");
    assert!(rendered.contains(RETRY_IDENTIFIER));
    for absent in ["operation_identifier", "revision", "state"] {
        assert!(
            !rendered.contains(absent),
            "before the daemon answered there is no durable operation to claim: {rendered}"
        );
    }
}

#[test]
fn a_transfer_signal_names_no_local_path() {
    let SignalOutcome::Interrupted { interruption } = on_signal(&Phase::FetchingArtifact {
        artifact_identifier: ARTIFACT.to_owned(),
        operation_identifier: OPERATION.to_owned(),
    }) else {
        panic!("nothing was committed")
    };
    let rendered = serde_json::to_string(&interruption).expect("it serializes");
    for absent in ["destination", "staging", "/tmp", "path"] {
        assert!(!rendered.contains(absent), "where a caller was writing is their business");
    }
    assert!(rendered.contains(ARTIFACT) && rendered.contains(OPERATION));
}

#[test]
fn a_maintenance_signal_names_a_target_and_a_result_and_no_operation() {
    let SignalOutcome::Interrupted { interruption } =
        on_signal(&Phase::FetchingMaintenanceResult {
            author_target_identity_digest: TARGET.to_owned(),
            maintenance_result_identifier: MAINTENANCE_RESULT.to_owned(),
        })
    else {
        panic!("nothing was committed")
    };
    let rendered = serde_json::to_string(&interruption).expect("it serializes");
    assert!(rendered.contains(TARGET) && rendered.contains(MAINTENANCE_RESULT));
    assert!(
        !rendered.contains("operation_identifier") && !rendered.contains("slot"),
        "a maintenance result belongs to a target, and naming an operation would invent one"
    );
}

#[test]
fn nothing_in_the_interrupt_path_reaches_for_a_daemon_or_an_agent() {
    let source = std::fs::read_to_string("src/interrupt.rs").expect("it is readable");
    let code: String = source
        .lines()
        .filter(|line| !line.trim_start().starts_with("//"))
        .collect::<Vec<&str>>()
        .join("\n");
    for reach in ["cancel", "std::net", "std::process", "daemon_connection", "abort"] {
        assert!(
            !code.contains(reach),
            "an interrupt that cancelled would make a keystroke destroy remote work: the code \
             names {reach}"
        );
    }
    assert!(
        source.contains("Stopping watching is not stopping work"),
        "and the module says so, so a later reader knows it was a decision"
    );
}
