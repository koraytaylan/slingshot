//! Assertions for removing a page and everything under it.
//!
//! Two decisions are pinned here because both are easy to soften later. The
//! reference policy has no default, so a document that omits it is refused
//! rather than given one; and an absent page is a failure rather than a success
//! with nothing to do, which is what makes this command not idempotent and
//! therefore key-carrying like every other write.

use serde_json::Value;
use slingshot_domain::command::command_identity::CommandContract;
use slingshot_domain::command::delete_page::{
    DeletePageCommand, DeletePageFailure, DeletePageRefusal, DeletePageResult,
};
use slingshot_domain::command::repository_path::RepositoryPath;
use slingshot_domain::command::resource_mutation::{DeletedResourceResult, MutationResultFailure};

/// Commands this test reads.
const COMMANDS: &str = include_str!("fixtures/commands/delete_page/commands.jsonl");

/// Failures this test reads.
const FAILURES: &str = include_str!("fixtures/commands/delete_page/failures.jsonl");

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

/// Returns one legal command over `policy`.
fn command(policy: &str) -> DeletePageCommand {
    serde_json::from_str(&format!(
        "{{\"page_path\":\"/content/example/en/report\",\"reference_policy\":\"{policy}\"}}"
    ))
    .expect("a legal command")
}

#[test]
fn every_command_vector_parses_exactly_as_the_fixture_says() {
    let vectors = rows(COMMANDS);
    assert!(vectors.len() >= 7, "both policies, the root, and every refusal");
    for row in &vectors {
        let document = text(row, "document");
        let note = text(row, "note");
        match (row["accepted"].as_bool(), serde_json::from_str::<DeletePageCommand>(document)) {
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
fn the_reference_policy_is_stated_and_never_supplied() {
    assert!(
        serde_json::from_str::<DeletePageCommand>(r#"{"page_path":"/content/example"}"#).is_err(),
        "a deletion without a reference policy was given one"
    );
    assert!(command("refuse_when_referenced").refuses_when_referenced());
    assert!(!command("ignore_references").refuses_when_referenced());
}

#[test]
fn a_removed_node_count_is_accepted_at_its_bound_and_refused_one_past_it() {
    let bound = CommandContract::embedded().limit("maximum_deleted_nodes");
    assert!(DeletedResourceResult::new(path("/content/example/en/report"), bound).is_ok());
    assert_eq!(
        DeletedResourceResult::new(path("/content/example/en/report"), bound + 1),
        Err(MutationResultFailure::CountTooLarge)
    );
}

#[test]
fn a_result_answers_only_the_request_that_named_its_page() {
    let asked = command("ignore_references");
    let answered = DeletePageResult {
        deleted: DeletedResourceResult::new(path("/content/example/en/report"), 1)
            .expect("a legal deletion"),
    };
    assert_eq!(answered.require_answers(&asked), Ok(()));
    let elsewhere = DeletePageResult {
        deleted: DeletedResourceResult::new(path("/content/other"), 1).expect("a legal deletion"),
    };
    assert_eq!(elsewhere.require_answers(&asked), Err(MutationResultFailure::NotThisRequest));
}

#[test]
fn a_reference_refusal_belongs_only_to_a_request_that_asked_to_refuse() {
    let refusal = DeletePageRefusal {
        failure: DeletePageFailure::TargetIsReferenced,
        page_path: path("/content/example/en/report"),
    };
    assert_eq!(refusal.require_answers(&command("refuse_when_referenced")), Ok(()));
    assert_eq!(
        refusal.require_answers(&command("ignore_references")),
        Err(MutationResultFailure::NotThisRequest),
        "a request that said to ignore references was answered with one"
    );
}

#[test]
fn every_failure_document_carries_its_two_members_and_says_what_it_proves() {
    let vectors = rows(FAILURES);
    assert_eq!(vectors.len(), 7, "one for each category this command allows");
    for row in &vectors {
        let note = text(row, "note");
        let document = text(row, "document");
        let refusal: DeletePageRefusal =
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
        serde_json::from_str::<DeletePageRefusal>(
            r#"{"failure":"target_not_found","page_path":"/content/example","extra":1}"#
        )
        .is_err()
    );
}

#[test]
fn an_absent_page_is_a_failure_rather_than_a_quiet_success() {
    let refusal = DeletePageRefusal {
        failure: DeletePageFailure::TargetNotFound,
        page_path: path("/content/example/en/report"),
    };
    assert!(refusal.proves_no_effect());
    assert_eq!(refusal.require_answers(&command("ignore_references")), Ok(()));
}
