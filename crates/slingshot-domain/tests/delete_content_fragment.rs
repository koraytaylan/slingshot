//! Assertions for removing a content fragment.
//!
//! The same two rules an asset deletion is held to, because a fragment is
//! referred to the same way: the reference policy has no default, and a refusal
//! reporting a reference belongs only to a request that asked to be refused.

use serde_json::Value;
use slingshot_domain::command::delete_content_fragment::{
    DeleteContentFragmentCommand, DeleteContentFragmentFailure, DeleteContentFragmentRefusal,
    DeleteContentFragmentResult,
};
use slingshot_domain::command::repository_path::RepositoryPath;
use slingshot_domain::command::resource_mutation::{DeletedResourceResult, MutationResultFailure};

/// Commands this test reads.
const COMMANDS: &str = include_str!("fixtures/commands/delete_content_fragment/commands.jsonl");

/// Failures this test reads.
const FAILURES: &str = include_str!("fixtures/commands/delete_content_fragment/failures.jsonl");

/// Fragment every vector addresses.
const FRAGMENT: &str = "/content/dam/example/fragments/offer";

/// Nodes one removal reported taking with it.
const REMOVED: u64 = 12;

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

/// Returns one legal request over `policy`.
fn command(policy: &str) -> DeleteContentFragmentCommand {
    serde_json::from_str(&format!(
        "{{\"fragment_path\":\"{FRAGMENT}\",\"reference_policy\":\"{policy}\"}}"
    ))
    .expect("a legal command")
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
            serde_json::from_str::<DeleteContentFragmentCommand>(document),
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
fn the_reference_policy_is_stated_and_never_supplied() {
    assert!(
        serde_json::from_str::<DeleteContentFragmentCommand>(&format!(
            "{{\"fragment_path\":\"{FRAGMENT}\"}}"
        ))
        .is_err()
    );
    assert!(command("refuse_when_referenced").refuses_when_referenced());
    assert!(!command("ignore_references").refuses_when_referenced());
}

#[test]
fn a_reference_refusal_belongs_only_to_a_request_that_asked_to_refuse() {
    let refusal = DeleteContentFragmentRefusal {
        failure: DeleteContentFragmentFailure::FragmentIsReferenced,
        fragment_path: path(FRAGMENT),
    };
    assert_eq!(refusal.require_answers(&command("refuse_when_referenced")), Ok(()));
    assert_eq!(
        refusal.require_answers(&command("ignore_references")),
        Err(MutationResultFailure::NotThisRequest)
    );
}

#[test]
fn a_result_answers_only_the_request_that_named_its_fragment() {
    let answered = DeleteContentFragmentResult {
        deleted: DeletedResourceResult::new(path(FRAGMENT), REMOVED).expect("a legal deletion"),
    };
    assert_eq!(answered.require_answers(&command("ignore_references")), Ok(()));
    let elsewhere = DeleteContentFragmentResult {
        deleted: DeletedResourceResult::new(path("/content/dam/example/fragments/other"), REMOVED)
            .expect("a legal deletion"),
    };
    assert_eq!(
        elsewhere.require_answers(&command("ignore_references")),
        Err(MutationResultFailure::NotThisRequest)
    );
}

#[test]
fn every_failure_document_carries_its_members_and_says_what_it_proves() {
    let vectors = rows(FAILURES);
    assert_eq!(vectors.len(), 7, "one for each category this command allows");
    for row in &vectors {
        let note = text(row, "note");
        let document = text(row, "document");
        let refusal: DeleteContentFragmentRefusal =
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
        serde_json::from_str::<DeleteContentFragmentRefusal>(r#"{"failure":"fragment_not_found","fragment_path":"/content/dam/example/fragments/offer","extra":1}"#).is_err(),
        "a surplus member was accepted"
    );
}
