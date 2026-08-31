//! Assertions for moving a component within its parent.
//!
//! Placement is a closed choice with two shapes and no third, and neither shape
//! accepts the other's members. That is the whole reason it is not a nullable
//! sibling name: a document missing a field by accident and a document meaning
//! "last" would otherwise be the same document.

use serde_json::Value;
use slingshot_domain::command::reorder_component::{
    ComponentPlacement, ReorderComponentCommand, ReorderComponentFailure, ReorderComponentRefusal,
    ReorderComponentResult,
};
use slingshot_domain::command::repository_path::{ComponentName, RepositoryPath};
use slingshot_domain::command::resource_mutation::MutationResultFailure;

/// Commands this test reads.
const COMMANDS: &str = include_str!("fixtures/commands/reorder_component/commands.jsonl");

/// Failures this test reads.
const FAILURES: &str = include_str!("fixtures/commands/reorder_component/failures.jsonl");

/// Component every vector addresses.
const COMPONENT: &str = "/content/example/en/report/jcr:content/root/text";

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

/// Returns one legal component name.
fn name(value: &str) -> ComponentName {
    ComponentName::parse(value).expect("a legal component name")
}

/// Returns one legal request over `placement`.
fn command(placement: ComponentPlacement) -> ReorderComponentCommand {
    ReorderComponentCommand { component_path: path(COMPONENT), placement }
}

#[test]
fn every_command_vector_parses_exactly_as_the_fixture_says() {
    let vectors = rows(COMMANDS);
    assert!(vectors.len() >= 6, "both placements and every refusal");
    for row in &vectors {
        let document = text(row, "document");
        let note = text(row, "note");
        match (row["accepted"].as_bool(), serde_json::from_str::<ReorderComponentCommand>(document))
        {
            (Some(true), Ok(parsed)) => {
                assert_eq!(
                    serde_json::to_string(&parsed).expect("a command serializes"),
                    document,
                    "{note}: rewritten differently"
                );
                assert_eq!(parsed.require_usable(), Ok(()), "{note}: refused as unusable");
            }
            (Some(false), Err(_)) => (),
            (_, parsed) => panic!("{note}: the command answered {parsed:?}"),
        }
    }
}

#[test]
fn a_placement_refuses_the_other_placement_s_members() {
    for refused in [
        "{\"mode\":\"last\",\"sibling_name\":\"image\"}",
        "{\"mode\":\"before\"}",
        "{\"mode\":\"first\"}",
    ] {
        assert!(
            serde_json::from_str::<ComponentPlacement>(refused).is_err(),
            "{refused} was accepted as a placement"
        );
    }
}

#[test]
fn a_component_cannot_be_asked_to_precede_itself() {
    let itself = command(ComponentPlacement::Before { sibling_name: name("text") });
    assert_eq!(itself.require_usable(), Err(MutationResultFailure::NotThisRequest));
    let another = command(ComponentPlacement::Before { sibling_name: name("image") });
    assert_eq!(another.require_usable(), Ok(()));
}

#[test]
fn a_result_answers_only_the_request_that_named_its_component() {
    let asked = command(ComponentPlacement::Last {});
    let answered = ReorderComponentResult {
        preceding_sibling_name: Some(name("image")),
        repository_path: path(COMPONENT),
    };
    assert_eq!(answered.require_answers(&asked), Ok(()));
    let elsewhere = ReorderComponentResult {
        preceding_sibling_name: None,
        repository_path: path("/content/other/jcr:content/root/text"),
    };
    assert_eq!(elsewhere.require_answers(&asked), Err(MutationResultFailure::NotThisRequest));
}

#[test]
fn a_component_cannot_be_reported_as_following_itself() {
    let asked = command(ComponentPlacement::Last {});
    let itself = ReorderComponentResult {
        preceding_sibling_name: Some(name("text")),
        repository_path: path(COMPONENT),
    };
    assert_eq!(itself.require_answers(&asked), Err(MutationResultFailure::NotThisRequest));
}

#[test]
fn a_result_that_reports_no_predecessor_omits_the_member_rather_than_nulling_it() {
    let first =
        ReorderComponentResult { preceding_sibling_name: None, repository_path: path(COMPONENT) };
    let written = serde_json::to_string(&first).expect("a result serializes");
    assert!(!written.contains("preceding_sibling_name"), "an absent predecessor was serialized");
    let read: ReorderComponentResult = serde_json::from_str(&written).expect("a result parses");
    assert_eq!(read, first);
}

#[test]
fn a_missing_sibling_belongs_only_to_a_request_that_named_one() {
    let refusal = ReorderComponentRefusal {
        component_path: path(COMPONENT),
        failure: ReorderComponentFailure::SiblingNotFound,
    };
    assert_eq!(
        refusal
            .require_answers(&command(ComponentPlacement::Before { sibling_name: name("image") })),
        Ok(())
    );
    assert_eq!(
        refusal.require_answers(&command(ComponentPlacement::Last {})),
        Err(MutationResultFailure::NotThisRequest),
        "a placement that named no sibling was answered with a missing one"
    );
}

#[test]
fn every_failure_document_carries_its_two_members_and_says_what_it_proves() {
    let vectors = rows(FAILURES);
    assert_eq!(vectors.len(), 6, "one for each category this command allows");
    for row in &vectors {
        let note = text(row, "note");
        let document = text(row, "document");
        let refusal: ReorderComponentRefusal =
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
