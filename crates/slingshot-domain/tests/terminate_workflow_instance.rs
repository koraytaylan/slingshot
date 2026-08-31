//! Assertions for ending a workflow instance.
//!
//! Two rules. An instance that has already ended is refused rather than reported
//! as ended again, so a caller can tell "I ended it" from "somebody else did";
//! and the answer is the state the author observed, not the state the request
//! aimed at.

use serde_json::Value;
use slingshot_domain::command::process_identity::{
    WorkflowInstanceIdentifier, WorkflowInstanceState,
};
use slingshot_domain::command::resource_mutation::MutationResultFailure;
use slingshot_domain::command::terminate_workflow_instance::{
    TerminateWorkflowInstanceCommand, TerminateWorkflowInstanceFailure,
    TerminateWorkflowInstanceRefusal, TerminateWorkflowInstanceResult,
};

/// Commands this test reads.
const COMMANDS: &str = include_str!("fixtures/commands/terminate_workflow_instance/commands.jsonl");

/// Failures this test reads.
const FAILURES: &str = include_str!("fixtures/commands/terminate_workflow_instance/failures.jsonl");

/// Instance every vector ends.
const INSTANCE: &str = "/var/workflow/instances/server0/2024-01-01/request-for-activation_1";

/// Reads one row's string member.
fn text<'row>(row: &'row Value, member: &str) -> &'row str {
    row[member].as_str().unwrap_or_else(|| panic!("{member} is a string in {row}"))
}

/// Returns every row of one fixture.
fn rows(fixture: &str) -> Vec<Value> {
    fixture
        .lines()
        .map(|line| serde_json::from_str::<Value>(line).expect("every fixture line is one object"))
        .collect()
}

/// Returns one instance identifier.
fn instance(value: &str) -> WorkflowInstanceIdentifier {
    WorkflowInstanceIdentifier::parse(value).expect("a legal identifier")
}

/// Returns the request every result assertion answers.
fn command() -> TerminateWorkflowInstanceCommand {
    TerminateWorkflowInstanceCommand { instance_identifier: instance(INSTANCE) }
}

#[test]
fn every_command_vector_parses_exactly_as_the_fixture_says() {
    let vectors = rows(COMMANDS);
    assert!(vectors.len() >= 2, "every document shape and every refusal");
    for row in &vectors {
        let document = text(row, "document");
        let note = text(row, "note");
        match (
            row["accepted"].as_bool(),
            serde_json::from_str::<TerminateWorkflowInstanceCommand>(document),
        ) {
            (Some(true), Ok(parsed)) => {
                assert_eq!(
                    serde_json::to_string(&parsed).expect("a command serializes"),
                    document,
                    "{note}: rewritten differently"
                );
            }
            (Some(false), Err(_)) => (),
            (_, parsed) => panic!("{note}: the command answered {parsed:?}"),
        }
    }
}

#[test]
fn an_instance_that_already_ended_is_refused_rather_than_ended_again() {
    let refusal = TerminateWorkflowInstanceRefusal {
        failure: TerminateWorkflowInstanceFailure::InstanceNotTerminable,
        instance_identifier: instance(INSTANCE),
    };
    assert!(refusal.proves_no_effect());
    assert_eq!(refusal.require_answers(&command()), Ok(()));
}

#[test]
fn the_answer_is_the_state_that_was_observed() {
    for observed_state in [
        WorkflowInstanceState::Aborted,
        WorkflowInstanceState::Running,
        WorkflowInstanceState::Stale,
    ] {
        let answered = TerminateWorkflowInstanceResult {
            instance_identifier: instance(INSTANCE),
            observed_state,
        };
        assert_eq!(answered.require_answers(&command()), Ok(()));
    }
}

#[test]
fn a_result_answers_only_the_request_that_named_its_instance() {
    let elsewhere = TerminateWorkflowInstanceResult {
        instance_identifier: instance("another-instance"),
        observed_state: WorkflowInstanceState::Aborted,
    };
    assert_eq!(elsewhere.require_answers(&command()), Err(MutationResultFailure::NotThisRequest));
}

#[test]
fn every_failure_document_carries_its_members_and_says_what_it_proves() {
    let vectors = rows(FAILURES);
    assert_eq!(vectors.len(), 5, "one for each category this command allows");
    for row in &vectors {
        let note = text(row, "note");
        let document = text(row, "document");
        let refusal: TerminateWorkflowInstanceRefusal =
            serde_json::from_str(document).unwrap_or_else(|failure| panic!("{note}: {failure}"));
        assert_eq!(
            serde_json::to_string(&refusal).expect("a refusal serializes"),
            document,
            "{note}: rewritten differently"
        );
        assert_eq!(
            refusal.proves_no_effect(),
            row["proves_no_effect"].as_bool().expect("a verdict"),
            "{note}"
        );
    }
    assert!(
        serde_json::from_str::<TerminateWorkflowInstanceRefusal>(r#"{"failure":"instance_not_found","instance_identifier":"/var/workflow/instances/server0/2024-01-01/request-for-activation_1","extra":1}"#).is_err(),
        "a surplus member was accepted"
    );
}
