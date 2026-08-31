//! Assertions for editing one variation of a content fragment.
//!
//! Two rules are pinned. A request that carries neither a title nor an element is
//! refused rather than answered with a success that did nothing; and a refusal
//! reporting a missing variation belongs only to a request that named one,
//! because a request that named none was asking about the master and the master
//! is always there.

use serde_json::Value;
use slingshot_domain::command::content_fragment_element::{
    ContentFragmentFailure, ContentFragmentVariationName,
};
use slingshot_domain::command::repository_path::RepositoryPath;
use slingshot_domain::command::resource_mutation::{MutationResultFailure, ResourceMutationResult};
use slingshot_domain::command::update_content_fragment::{
    UpdateContentFragmentCommand, UpdateContentFragmentFailure, UpdateContentFragmentRefusal,
    UpdateContentFragmentResult,
};

/// Commands this test reads.
const COMMANDS: &str = include_str!("fixtures/commands/update_content_fragment/commands.jsonl");

/// Parsable requests that would change nothing.
const UNUSABLE: &str = include_str!("fixtures/commands/update_content_fragment/unusable.jsonl");

/// Failures this test reads.
const FAILURES: &str = include_str!("fixtures/commands/update_content_fragment/failures.jsonl");

/// Fragment every vector addresses.
const FRAGMENT: &str = "/content/dam/example/fragments/offer";

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

/// Returns one request, over the variation it names.
fn command(variation: Option<&str>) -> UpdateContentFragmentCommand {
    UpdateContentFragmentCommand {
        elements: None,
        fragment_path: path(FRAGMENT),
        title: None,
        variation_name: variation
            .map(|name| ContentFragmentVariationName::parse(name).expect("a legal name")),
    }
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
            serde_json::from_str::<UpdateContentFragmentCommand>(document),
        ) {
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
fn a_request_that_would_change_nothing_is_refused() {
    for row in rows(UNUSABLE) {
        let note = text(&row, "note");
        let parsed: UpdateContentFragmentCommand = serde_json::from_str(text(&row, "document"))
            .unwrap_or_else(|failure| panic!("{note}: the document did not parse: {failure}"));
        assert_eq!(parsed.require_usable(), Err(ContentFragmentFailure::NotThisRequest), "{note}");
    }
}

#[test]
fn a_missing_variation_belongs_only_to_a_request_that_named_one() {
    let refusal = UpdateContentFragmentRefusal {
        failure: UpdateContentFragmentFailure::VariationNotFound,
        fragment_path: path(FRAGMENT),
    };
    assert_eq!(refusal.require_answers(&command(Some("mobile"))), Ok(()));
    assert_eq!(
        refusal.require_answers(&command(None)),
        Err(MutationResultFailure::NotThisRequest),
        "a request about the master was answered with a missing variation"
    );
}

#[test]
fn a_result_answers_only_the_request_that_named_its_fragment() {
    let answered = UpdateContentFragmentResult {
        mutated: ResourceMutationResult { repository_path: path(FRAGMENT) },
    };
    assert_eq!(answered.require_answers(&command(None)), Ok(()));
    let elsewhere = UpdateContentFragmentResult {
        mutated: ResourceMutationResult {
            repository_path: path("/content/dam/example/fragments/other"),
        },
    };
    assert_eq!(
        elsewhere.require_answers(&command(None)),
        Err(MutationResultFailure::NotThisRequest)
    );
}

#[test]
fn every_failure_document_carries_its_members_and_says_what_it_proves() {
    let vectors = rows(FAILURES);
    assert_eq!(vectors.len(), 8, "one for each category this command allows");
    for row in &vectors {
        let note = text(row, "note");
        let document = text(row, "document");
        let refusal: UpdateContentFragmentRefusal =
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
        serde_json::from_str::<UpdateContentFragmentRefusal>(r#"{"failure":"element_unknown","fragment_path":"/content/dam/example/fragments/offer","extra":1}"#).is_err(),
        "a surplus member was accepted"
    );
}
