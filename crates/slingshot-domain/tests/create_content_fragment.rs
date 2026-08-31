//! Assertions for making a content fragment from a model.
//!
//! What is deliberately absent is model validation. This contract cannot read a
//! model, so it does not pretend to check a request against one: an element the
//! model does not declare is the author's refusal under its own closed category,
//! which is what lets a caller tell "no such element" from "that value was too
//! long" without guessing.

use serde_json::Value;
use slingshot_domain::command::create_content_fragment::{
    CreateContentFragmentCommand, CreateContentFragmentFailure, CreateContentFragmentRefusal,
    CreateContentFragmentResult,
};
use slingshot_domain::command::repository_path::RepositoryPath;
use slingshot_domain::command::resource_mutation::{MutationResultFailure, ResourceMutationResult};

/// Commands this test reads.
const COMMANDS: &str = include_str!("fixtures/commands/create_content_fragment/commands.jsonl");

/// Failures this test reads.
const FAILURES: &str = include_str!("fixtures/commands/create_content_fragment/failures.jsonl");

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

/// Returns one legal request.
fn command() -> CreateContentFragmentCommand {
    serde_json::from_str(
        r#"{"model_path":"/conf/example/settings/dam/cfm/models/offer","name":"offer","parent_path":"/content/dam/example/fragments"}"#,
    )
    .expect("a legal command")
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
            serde_json::from_str::<CreateContentFragmentCommand>(document),
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
fn the_target_is_computed_and_the_model_stays_a_separate_address() {
    let asked = command();
    assert_eq!(
        asked.target_path().expect("a target"),
        path("/content/dam/example/fragments/offer")
    );
    assert_eq!(
        asked.model_path,
        path("/conf/example/settings/dam/cfm/models/offer"),
        "the model address was confused with the target"
    );
}

#[test]
fn a_result_answers_only_the_request_that_computed_its_address() {
    let answered = CreateContentFragmentResult {
        mutated: ResourceMutationResult {
            repository_path: path("/content/dam/example/fragments/offer"),
        },
    };
    assert_eq!(answered.require_answers(&command()), Ok(()));
    let elsewhere = CreateContentFragmentResult {
        mutated: ResourceMutationResult {
            repository_path: path("/content/dam/example/fragments/other"),
        },
    };
    assert_eq!(elsewhere.require_answers(&command()), Err(MutationResultFailure::NotThisRequest));
}

#[test]
fn a_refusal_answers_only_the_request_that_computed_its_target() {
    let refusal = CreateContentFragmentRefusal {
        failure: CreateContentFragmentFailure::ElementUnknown,
        target_path: path("/content/dam/example/fragments/offer"),
    };
    assert_eq!(refusal.require_answers(&command()), Ok(()));
    let elsewhere = CreateContentFragmentRefusal {
        failure: CreateContentFragmentFailure::ElementUnknown,
        target_path: path("/content/dam/example/fragments/other"),
    };
    assert_eq!(elsewhere.require_answers(&command()), Err(MutationResultFailure::NotThisRequest));
}

#[test]
fn every_failure_document_carries_its_members_and_says_what_it_proves() {
    let vectors = rows(FAILURES);
    assert_eq!(vectors.len(), 9, "one for each category this command allows");
    for row in &vectors {
        let note = text(row, "note");
        let document = text(row, "document");
        let refusal: CreateContentFragmentRefusal =
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
        serde_json::from_str::<CreateContentFragmentRefusal>(r#"{"failure":"model_not_found","target_path":"/content/dam/example/fragments/offer","extra":1}"#).is_err(),
        "a surplus member was accepted"
    );
}
