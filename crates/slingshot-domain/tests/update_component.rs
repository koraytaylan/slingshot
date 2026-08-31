//! Assertions for changing a component that is already on a page.
//!
//! The shared property-mutation rule is what this command is mostly made of, so
//! what is proved here is that it applies unchanged: one property is assigned or
//! removed and never both, and a request that would change nothing is refused
//! rather than answered.

use serde_json::Value;
use slingshot_domain::command::repository_path::RepositoryPath;
use slingshot_domain::command::resource_mutation::{
    MutationResultFailure, PropertyMutationFailure, ResourceMutationResult,
};
use slingshot_domain::command::update_component::{
    UpdateComponentCommand, UpdateComponentFailure, UpdateComponentRefusal, UpdateComponentResult,
};

/// Commands this test reads.
const COMMANDS: &str = include_str!("fixtures/commands/update_component/commands.jsonl");

/// Parsable requests the shared mutation rule then refuses.
const UNUSABLE: &str = include_str!("fixtures/commands/update_component/unusable.jsonl");

/// Failures this test reads.
const FAILURES: &str = include_str!("fixtures/commands/update_component/failures.jsonl");

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

#[test]
fn every_command_vector_parses_exactly_as_the_fixture_says() {
    let vectors = rows(COMMANDS);
    assert!(vectors.len() >= 5, "both documents, their overlap, and every refusal");
    for row in &vectors {
        let document = text(row, "document");
        let note = text(row, "note");
        match (row["accepted"].as_bool(), serde_json::from_str::<UpdateComponentCommand>(document))
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
fn the_shared_mutation_rule_applies_here_unchanged() {
    for row in rows(UNUSABLE) {
        let note = text(&row, "note");
        let parsed: UpdateComponentCommand = serde_json::from_str(text(&row, "document"))
            .unwrap_or_else(|failure| panic!("{note}: the document did not parse: {failure}"));
        let expected = match text(&row, "refusal") {
            "changes_nothing" => PropertyMutationFailure::ChangesNothing,
            "both_assigned_and_removed" => PropertyMutationFailure::BothAssignedAndRemoved,
            other => panic!("{note}: the fixture names an unknown refusal {other}"),
        };
        assert_eq!(parsed.require_usable(), Err(expected), "{note}");
    }
}

#[test]
fn a_result_answers_only_the_request_that_named_its_component() {
    let asked: UpdateComponentCommand = serde_json::from_str(&format!(
        "{{\"component_path\":\"{COMPONENT}\",\"removed_property_names\":[\"text\"]}}"
    ))
    .expect("a legal command");
    let answered = UpdateComponentResult {
        mutated: ResourceMutationResult { repository_path: path(COMPONENT) },
    };
    assert_eq!(answered.require_answers(&asked), Ok(()));
    let elsewhere = UpdateComponentResult {
        mutated: ResourceMutationResult { repository_path: path("/content/other/jcr:content") },
    };
    assert_eq!(elsewhere.require_answers(&asked), Err(MutationResultFailure::NotThisRequest));
}

#[test]
fn every_failure_document_carries_its_two_members_and_says_what_it_proves() {
    let vectors = rows(FAILURES);
    assert_eq!(vectors.len(), 7, "one for each category this command allows");
    for row in &vectors {
        let note = text(row, "note");
        let document = text(row, "document");
        let refusal: UpdateComponentRefusal =
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
        serde_json::from_str::<UpdateComponentRefusal>(&format!(
            "{{\"component_path\":\"{COMPONENT}\",\"failure\":\"component_not_found\",\"extra\":1}}"
        ))
        .is_err()
    );
}

#[test]
fn a_refusal_answers_only_the_request_that_named_its_component() {
    let asked: UpdateComponentCommand = serde_json::from_str(&format!(
        "{{\"component_path\":\"{COMPONENT}\",\"removed_property_names\":[\"text\"]}}"
    ))
    .expect("a legal command");
    let refusal = UpdateComponentRefusal {
        component_path: path(COMPONENT),
        failure: UpdateComponentFailure::ComponentNotFound,
    };
    assert_eq!(refusal.require_answers(&asked), Ok(()));
    let elsewhere = UpdateComponentRefusal {
        component_path: path("/content/other"),
        failure: UpdateComponentFailure::ComponentNotFound,
    };
    assert_eq!(elsewhere.require_answers(&asked), Err(MutationResultFailure::NotThisRequest));
}
