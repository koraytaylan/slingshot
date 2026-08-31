//! Assertions for holding a workflow instance and letting it go again.
//!
//! One command with two values rather than two commands: a suspend and a resume
//! that could disagree would only disagree on the instance somebody was already
//! worried about. The answer is the observed state, and an instance observed as
//! ended after a resume request is a real observation this contract reports
//! rather than refuses.

use serde_json::Value;
use slingshot_domain::command::process_identity::{
    WorkflowInstanceIdentifier, WorkflowInstanceState,
};
use slingshot_domain::command::resource_mutation::MutationResultFailure;
use slingshot_domain::command::set_workflow_instance_suspension::{
    RequestedSuspension, SetWorkflowInstanceSuspensionCommand,
    SetWorkflowInstanceSuspensionFailure, SetWorkflowInstanceSuspensionRefusal,
    SetWorkflowInstanceSuspensionResult,
};

/// Commands this test reads.
const COMMANDS: &str =
    include_str!("fixtures/commands/set_workflow_instance_suspension/commands.jsonl");

/// Failures this test reads.
const FAILURES: &str =
    include_str!("fixtures/commands/set_workflow_instance_suspension/failures.jsonl");

/// Instance every vector acts on.
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

/// Returns one request over `requested_state`.
fn command(requested_state: RequestedSuspension) -> SetWorkflowInstanceSuspensionCommand {
    SetWorkflowInstanceSuspensionCommand {
        instance_identifier: instance(INSTANCE),
        requested_state,
    }
}

#[test]
fn every_command_vector_parses_exactly_as_the_fixture_says() {
    let vectors = rows(COMMANDS);
    assert!(vectors.len() >= 4, "every document shape and every refusal");
    for row in &vectors {
        let document = text(row, "document");
        let note = text(row, "note");
        match (
            row["accepted"].as_bool(),
            serde_json::from_str::<SetWorkflowInstanceSuspensionCommand>(document),
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
fn both_requested_states_round_trip_and_a_third_is_refused() {
    for (state, spelling) in [
        (RequestedSuspension::Running, "\"running\""),
        (RequestedSuspension::Suspended, "\"suspended\""),
    ] {
        assert_eq!(serde_json::to_string(&state).expect("it serializes"), spelling);
    }
    assert!(serde_json::from_str::<RequestedSuspension>("\"aborted\"").is_err());
}

#[test]
fn an_ended_instance_observed_after_a_resume_is_reported_rather_than_refused() {
    let answered = SetWorkflowInstanceSuspensionResult {
        instance_identifier: instance(INSTANCE),
        observed_state: WorkflowInstanceState::Completed,
    };
    assert_eq!(answered.require_answers(&command(RequestedSuspension::Running)), Ok(()));
}

#[test]
fn an_ended_instance_can_be_neither_held_nor_released() {
    let refusal = SetWorkflowInstanceSuspensionRefusal {
        failure: SetWorkflowInstanceSuspensionFailure::InstanceNotSuspendable,
        instance_identifier: instance(INSTANCE),
    };
    assert!(refusal.proves_no_effect());
    assert_eq!(refusal.require_answers(&command(RequestedSuspension::Suspended)), Ok(()));
}

#[test]
fn a_result_answers_only_the_request_that_named_its_instance() {
    let elsewhere = SetWorkflowInstanceSuspensionResult {
        instance_identifier: instance("another-instance"),
        observed_state: WorkflowInstanceState::Suspended,
    };
    assert_eq!(
        elsewhere.require_answers(&command(RequestedSuspension::Suspended)),
        Err(MutationResultFailure::NotThisRequest)
    );
}

#[test]
fn every_failure_document_carries_its_members_and_says_what_it_proves() {
    let vectors = rows(FAILURES);
    assert_eq!(vectors.len(), 5, "one for each category this command allows");
    for row in &vectors {
        let note = text(row, "note");
        let document = text(row, "document");
        let refusal: SetWorkflowInstanceSuspensionRefusal =
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
        serde_json::from_str::<SetWorkflowInstanceSuspensionRefusal>(r#"{"failure":"instance_not_found","instance_identifier":"/var/workflow/instances/server0/2024-01-01/request-for-activation_1","extra":1}"#).is_err(),
        "a surplus member was accepted"
    );
}
