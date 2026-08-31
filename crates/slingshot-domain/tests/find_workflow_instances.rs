//! Assertions for finding workflow instances.
//!
//! The archived states are members of the same set as the live ones, which is
//! what makes "show me what ran last week" and "show me what is running" one
//! command. The state set is required, because "every instance this deployment
//! has ever run" is not a question anybody means to ask by default.

use serde_json::Value;
use slingshot_domain::command::find_workflow_instances::{
    FindWorkflowInstancesCommand, FindWorkflowInstancesResult, WorkflowInstanceMatch,
};
use slingshot_domain::command::operational_listing::ListingResultFailure;
use slingshot_domain::command::process_identity::{
    RequestedWorkflowInstanceStates, WorkflowInstanceIdentifier, WorkflowInstanceState,
    WorkflowModelIdentifier,
};
use slingshot_domain::command::repository_path::RepositoryPath;

/// Commands this test reads.
const COMMANDS: &str = include_str!("fixtures/commands/find_workflow_instances/commands.jsonl");

/// Model every vector reports.
const MODEL: &str = "/var/workflow/models/request-for-activation/jcr:content/model";

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

/// Returns one legal path.
fn path(value: &str) -> RepositoryPath {
    RepositoryPath::parse(value).expect("a legal path")
}

/// Returns one match.
fn matched(identifier: &str, payload: &str, state: WorkflowInstanceState) -> WorkflowInstanceMatch {
    WorkflowInstanceMatch {
        instance_identifier: WorkflowInstanceIdentifier::parse(identifier)
            .expect("a legal identifier"),
        model_identifier: WorkflowModelIdentifier::parse(MODEL).expect("a legal identifier"),
        payload_path: path(payload),
        started_at: None,
        state,
    }
}

/// Returns one request over its states and an optional payload anchor.
fn command(
    states: Vec<WorkflowInstanceState>,
    payload_prefix: Option<&str>,
) -> FindWorkflowInstancesCommand {
    FindWorkflowInstancesCommand {
        model_identifier: None,
        payload_prefix: payload_prefix.map(path),
        result_window: None,
        states: RequestedWorkflowInstanceStates::new(states).expect("a legal set"),
    }
}

#[test]
fn every_command_vector_parses_exactly_as_the_fixture_says() {
    let vectors = rows(COMMANDS);
    assert!(vectors.len() >= 6, "every document shape and every refusal");
    for row in &vectors {
        let document = text(row, "document");
        let note = text(row, "note");
        match (
            row["accepted"].as_bool(),
            serde_json::from_str::<FindWorkflowInstancesCommand>(document),
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
fn the_state_set_is_required_and_has_no_default() {
    assert!(
        serde_json::from_str::<FindWorkflowInstancesCommand>("{}").is_err(),
        "a request with no state set was given one"
    );
}

#[test]
fn an_archived_instance_is_found_by_the_command_that_finds_a_live_one() {
    let archived = FindWorkflowInstancesResult::new(
        vec![matched("i-1", "/content/example/en/report", WorkflowInstanceState::Completed)],
        None,
    )
    .expect("a legal page");
    assert_eq!(
        archived.require_answers(&command(vec![WorkflowInstanceState::Completed], None)),
        Ok(())
    );
    assert_eq!(
        archived.require_answers(&command(vec![WorkflowInstanceState::Running], None)),
        Err(ListingResultFailure::NotThisRequest)
    );
}

#[test]
fn a_payload_outside_the_requested_anchor_is_refused_at_its_boundary() {
    let page = FindWorkflowInstancesResult::new(
        vec![matched("i-1", "/content/example", WorkflowInstanceState::Running)],
        None,
    )
    .expect("a legal page");
    assert_eq!(
        page.require_answers(&command(
            vec![WorkflowInstanceState::Running],
            Some("/content/example")
        )),
        Ok(()),
        "a payload equal to the anchor is inside it"
    );
    assert_eq!(
        page.require_answers(&command(vec![WorkflowInstanceState::Running], Some("/content/exam"))),
        Err(ListingResultFailure::NotThisRequest),
        "a prefix that is not a path segment boundary was treated as containment"
    );
}

#[test]
fn rows_are_strictly_ascending_by_instance_identifier() {
    assert_eq!(
        FindWorkflowInstancesResult::new(
            vec![
                matched("i-2", "/content/example", WorkflowInstanceState::Running),
                matched("i-1", "/content/example", WorkflowInstanceState::Running),
            ],
            None
        ),
        Err(ListingResultFailure::NotStrictlyAscending)
    );
}
