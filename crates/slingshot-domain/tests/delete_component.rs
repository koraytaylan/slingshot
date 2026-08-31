//! Assertions for removing a component from a page.
//!
//! One thing here is worth stating as an assertion rather than as prose: this
//! command has no reference policy, and a document that carries one is refused
//! rather than tolerated. A policy that could never apply would suggest there
//! was a case where it did.

use serde_json::Value;
use slingshot_domain::command::delete_component::{
    DeleteComponentCommand, DeleteComponentFailure, DeleteComponentRefusal, DeleteComponentResult,
};
use slingshot_domain::command::repository_path::RepositoryPath;
use slingshot_domain::command::resource_mutation::{DeletedResourceResult, MutationResultFailure};

/// Commands this test reads.
const COMMANDS: &str = include_str!("fixtures/commands/delete_component/commands.jsonl");

/// Failures this test reads.
const FAILURES: &str = include_str!("fixtures/commands/delete_component/failures.jsonl");

/// Component every vector addresses.
const COMPONENT: &str = "/content/example/en/report/jcr:content/root/text";

/// Nodes one removal reported taking with it.
const REMOVED: u64 = 4;

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

/// Returns the command every result assertion answers.
fn command() -> DeleteComponentCommand {
    DeleteComponentCommand { component_path: path(COMPONENT) }
}

#[test]
fn every_command_vector_parses_exactly_as_the_fixture_says() {
    let vectors = rows(COMMANDS);
    assert!(vectors.len() >= 4, "the removal and every refusal");
    for row in &vectors {
        let document = text(row, "document");
        let note = text(row, "note");
        match (row["accepted"].as_bool(), serde_json::from_str::<DeleteComponentCommand>(document))
        {
            (Some(true), Ok(parsed)) => assert_eq!(
                serde_json::to_string(&parsed).expect("a command serializes"),
                document,
                "{note}: rewritten differently"
            ),
            (Some(false), Err(_)) => (),
            (_, parsed) => panic!("{note}: the command answered {parsed:?}"),
        }
    }
}

#[test]
fn this_command_carries_no_reference_policy_and_refuses_one() {
    assert!(
        serde_json::from_str::<DeleteComponentCommand>(&format!(
            "{{\"component_path\":\"{COMPONENT}\",\"reference_policy\":\"ignore_references\"}}"
        ))
        .is_err(),
        "a reference policy this command does not have was accepted"
    );
}

#[test]
fn a_result_answers_only_the_request_that_named_its_component() {
    let answered = DeleteComponentResult {
        deleted: DeletedResourceResult::new(path(COMPONENT), REMOVED).expect("a legal deletion"),
    };
    assert_eq!(answered.require_answers(&command()), Ok(()));
    let elsewhere = DeleteComponentResult {
        deleted: DeletedResourceResult::new(path("/content/other"), REMOVED)
            .expect("a legal deletion"),
    };
    assert_eq!(elsewhere.require_answers(&command()), Err(MutationResultFailure::NotThisRequest));
}

#[test]
fn every_failure_document_carries_its_two_members_and_says_what_it_proves() {
    let vectors = rows(FAILURES);
    assert_eq!(vectors.len(), 5, "one for each category this command allows");
    for row in &vectors {
        let note = text(row, "note");
        let document = text(row, "document");
        let refusal: DeleteComponentRefusal =
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
}

#[test]
fn an_absent_component_is_a_failure_rather_than_a_quiet_success() {
    let refusal = DeleteComponentRefusal {
        component_path: path(COMPONENT),
        failure: DeleteComponentFailure::ComponentNotFound,
    };
    assert!(refusal.proves_no_effect());
    assert_eq!(refusal.require_answers(&command()), Ok(()));
}
