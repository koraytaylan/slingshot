//! Assertions for listing workflow models.
//!
//! The prefix filters on the title rather than the identifier, because the title
//! is the thing a person recognizes and the identifier is the thing they came
//! here to learn. Filtering on what the caller already has would be no help at
//! all.

use serde_json::Value;
use slingshot_domain::command::find_pages_containing_phrase::PageTitle;
use slingshot_domain::command::list_workflow_models::{
    ListWorkflowModelsCommand, ListWorkflowModelsResult, WorkflowModelMatch,
};
use slingshot_domain::command::operational_listing::ListingResultFailure;
use slingshot_domain::command::process_identity::WorkflowModelIdentifier;

/// Commands this test reads.
const COMMANDS: &str = include_str!("fixtures/commands/list_workflow_models/commands.jsonl");

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

/// Returns one title.
fn title(value: &str) -> PageTitle {
    PageTitle::new(value).expect("a legal title")
}

/// Returns one match.
fn matched(identifier: &str, name: &str) -> WorkflowModelMatch {
    WorkflowModelMatch {
        model_identifier: WorkflowModelIdentifier::parse(identifier).expect("a legal identifier"),
        title: title(name),
        version: None,
    }
}

/// Returns one request over `prefix`.
fn command(prefix: Option<&str>) -> ListWorkflowModelsCommand {
    ListWorkflowModelsCommand { result_window: None, title_prefix: prefix.map(title) }
}

#[test]
fn every_command_vector_parses_exactly_as_the_fixture_says() {
    let vectors = rows(COMMANDS);
    assert!(vectors.len() >= 5, "every document shape and every refusal");
    for row in &vectors {
        let document = text(row, "document");
        let note = text(row, "note");
        match (
            row["accepted"].as_bool(),
            serde_json::from_str::<ListWorkflowModelsCommand>(document),
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
fn rows_are_strictly_ascending_by_model_identifier() {
    assert!(
        ListWorkflowModelsResult::new(
            vec![matched("/var/workflow/models/a", "A"), matched("/var/workflow/models/b", "B")],
            None
        )
        .is_ok()
    );
    assert_eq!(
        ListWorkflowModelsResult::new(
            vec![matched("/var/workflow/models/b", "B"), matched("/var/workflow/models/a", "A")],
            None
        ),
        Err(ListingResultFailure::NotStrictlyAscending)
    );
}

#[test]
fn a_match_whose_title_is_outside_the_prefix_is_refused() {
    let page = ListWorkflowModelsResult::new(
        vec![matched(
            "/var/workflow/models/request-for-activation/jcr:content/model",
            "Publish now",
        )],
        None,
    )
    .expect("a legal page");
    assert_eq!(page.require_answers(&command(None)), Ok(()));
    assert_eq!(
        page.require_answers(&command(Some("Request"))),
        Err(ListingResultFailure::NotThisRequest)
    );
    let matching = ListWorkflowModelsResult::new(
        vec![matched(
            "/var/workflow/models/request-for-activation/jcr:content/model",
            "Request for activation",
        )],
        None,
    )
    .expect("a legal page");
    assert_eq!(matching.require_answers(&command(Some("Request"))), Ok(()));
}

#[test]
fn an_absent_version_is_omitted_rather_than_nulled() {
    let without = matched(
        "/var/workflow/models/request-for-activation/jcr:content/model",
        "Request for activation",
    );
    let written = serde_json::to_string(&without).expect("a match serializes");
    assert!(!written.contains("version"), "an absent version was serialized");
    let read: WorkflowModelMatch = serde_json::from_str(&written).expect("a match parses");
    assert_eq!(read, without);
}
