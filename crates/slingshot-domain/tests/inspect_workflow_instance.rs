//! Assertions for asking why one instance is not moving.
//!
//! An assignee is an authorizable identifier and never a display name, which is
//! the same identifier every other command in this registry addresses a person
//! by. A diagnostic has no reason to carry somebody's name.

use serde_json::Value;
use slingshot_domain::command::authorizable_identity::AuthorizableIdentifier;
use slingshot_domain::command::command_identity::CommandContract;
use slingshot_domain::command::find_pages_containing_phrase::PageTitle;
use slingshot_domain::command::inspect_workflow_instance::{
    InspectWorkflowInstanceCommand, InspectWorkflowInstanceFailure, InspectWorkflowInstanceRefusal,
    InspectWorkflowInstanceResult, WorkItem,
};
use slingshot_domain::command::operational_listing::ListingResultFailure;
use slingshot_domain::command::process_identity::{
    WorkItemIdentifier, WorkflowInstanceIdentifier, WorkflowInstanceState, WorkflowModelIdentifier,
};
use slingshot_domain::command::repository_path::RepositoryPath;

/// Commands this test reads.
const COMMANDS: &str = include_str!("fixtures/commands/inspect_workflow_instance/commands.jsonl");

/// Failures this test reads.
const FAILURES: &str = include_str!("fixtures/commands/inspect_workflow_instance/failures.jsonl");

/// Instance every vector inspects.
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

/// Returns one legal path.
fn path(value: &str) -> RepositoryPath {
    RepositoryPath::parse(value).expect("a legal path")
}

/// Returns one instance identifier.
fn instance(value: &str) -> WorkflowInstanceIdentifier {
    WorkflowInstanceIdentifier::parse(value).expect("a legal identifier")
}

/// Returns one work item.
fn item(identifier: &str, assigned: bool) -> WorkItem {
    WorkItem {
        assignee: assigned
            .then(|| AuthorizableIdentifier::parse("reviewer").expect("a legal identifier")),
        node_title: PageTitle::new("Review").expect("a legal title"),
        work_item_identifier: WorkItemIdentifier::parse(identifier).expect("a legal identifier"),
    }
}

/// Returns one inspection carrying `work_items`.
fn result(
    work_items: Vec<WorkItem>,
) -> Result<InspectWorkflowInstanceResult, ListingResultFailure> {
    InspectWorkflowInstanceResult::new(
        instance(INSTANCE),
        WorkflowModelIdentifier::parse(
            "/var/workflow/models/request-for-activation/jcr:content/model",
        )
        .expect("a legal identifier"),
        path("/content/example/en/report"),
        WorkflowInstanceState::Running,
        work_items,
    )
}

/// Returns the request every result assertion answers.
fn command() -> InspectWorkflowInstanceCommand {
    InspectWorkflowInstanceCommand { instance_identifier: instance(INSTANCE) }
}

#[test]
fn every_command_vector_parses_exactly_as_the_fixture_says() {
    let vectors = rows(COMMANDS);
    assert!(vectors.len() >= 3, "every document shape and every refusal");
    for row in &vectors {
        let document = text(row, "document");
        let note = text(row, "note");
        match (
            row["accepted"].as_bool(),
            serde_json::from_str::<InspectWorkflowInstanceCommand>(document),
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
fn an_empty_work_item_list_and_a_full_one_both_round_trip() {
    for items in [Vec::new(), vec![item("w-1", true)], vec![item("w-1", true), item("w-2", false)]]
    {
        let answered = result(items).expect("a legal inspection");
        let written = serde_json::to_string(&answered).expect("an inspection serializes");
        let read: InspectWorkflowInstanceResult =
            serde_json::from_str(&written).expect("an inspection parses");
        assert_eq!(read, answered);
    }
}

#[test]
fn work_items_are_strictly_ascending_and_bounded() {
    assert_eq!(
        result(vec![item("w-2", false), item("w-1", false)]),
        Err(ListingResultFailure::NotStrictlyAscending)
    );
    let bound = usize::try_from(CommandContract::embedded().limit("maximum_workflow_work_items"))
        .expect("the bound fits");
    let items: Vec<WorkItem> =
        (0..=bound).map(|index| item(&format!("w-{index:06}"), false)).collect();
    assert!(result(items[..bound].to_vec()).is_ok(), "the bound itself was refused");
    assert_eq!(result(items), Err(ListingResultFailure::TooManyRequested));
}

#[test]
fn an_unassigned_work_item_omits_its_assignee_rather_than_nulling_it() {
    let written = serde_json::to_string(&item("w-1", false)).expect("an item serializes");
    assert!(!written.contains("assignee"), "an absent assignee was serialized");
}

#[test]
fn a_result_and_a_refusal_answer_only_the_request_that_named_the_instance() {
    let answered = result(Vec::new()).expect("a legal inspection");
    assert_eq!(answered.require_answers(&command()), Ok(()));
    let elsewhere = InspectWorkflowInstanceResult::new(
        instance("another-instance"),
        WorkflowModelIdentifier::parse("/var/workflow/models/other").expect("a legal identifier"),
        path("/content/example/en/report"),
        WorkflowInstanceState::Running,
        Vec::new(),
    )
    .expect("a legal inspection");
    assert_eq!(elsewhere.require_answers(&command()), Err(ListingResultFailure::NotThisRequest));
    let refusal = InspectWorkflowInstanceRefusal {
        failure: InspectWorkflowInstanceFailure::InstanceNotFound,
        instance_identifier: instance(INSTANCE),
    };
    assert_eq!(refusal.require_answers(&command()), Ok(()));
}

#[test]
fn every_failure_document_round_trips() {
    let vectors = rows(FAILURES);
    assert_eq!(vectors.len(), 4, "one for each category this command allows");
    for row in &vectors {
        let note = text(row, "note");
        let document = text(row, "document");
        let refusal: InspectWorkflowInstanceRefusal =
            serde_json::from_str(document).unwrap_or_else(|failure| panic!("{note}: {failure}"));
        assert_eq!(
            serde_json::to_string(&refusal).expect("a refusal serializes"),
            document,
            "{note}: rewritten differently"
        );
    }
}
