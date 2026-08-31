//! Assertions for changing a page that already exists.
//!
//! Two rules carry the weight. The content resource is computed from the page
//! address rather than accepted, so a caller cannot aim content at the page node
//! where it would be stored and never rendered; and a property may be assigned
//! or removed by one request and never both, because there is no order between
//! the two documents that a caller could rely on.

use serde_json::Value;
use slingshot_domain::command::repository_path::RepositoryPath;
use slingshot_domain::command::resource_mutation::{
    MutationResultFailure, PropertyMutationFailure, ResourceMutationResult,
};
use slingshot_domain::command::update_page::{
    UpdatePageCommand, UpdatePageFailure, UpdatePageRefusal, UpdatePageResult,
};

/// Commands this test reads.
const COMMANDS: &str = include_str!("fixtures/commands/update_page/commands.jsonl");

/// Parsable requests the shared mutation rule then refuses.
const UNUSABLE: &str = include_str!("fixtures/commands/update_page/unusable.jsonl");

/// Failures this test reads.
const FAILURES: &str = include_str!("fixtures/commands/update_page/failures.jsonl");

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
    assert!(vectors.len() >= 11, "every document shape and every refusal");
    for row in &vectors {
        let document = text(row, "document");
        let note = text(row, "note");
        match (row["accepted"].as_bool(), serde_json::from_str::<UpdatePageCommand>(document)) {
            (Some(true), Ok(command)) => {
                assert_eq!(
                    serde_json::to_string(&command).expect("a command serializes"),
                    document,
                    "{note}: rewritten differently"
                );
                assert_eq!(
                    command.content_path().map(|resource| resource.as_text().to_owned()),
                    Ok(text(row, "content_path").to_owned()),
                    "{note}: computed the wrong content resource"
                );
                assert_eq!(command.require_usable(), Ok(()), "{note}: refused as unusable");
            }
            (Some(false), Err(_)) => (),
            (_, parsed) => panic!("{note}: the command answered {parsed:?}"),
        }
    }
}

#[test]
fn the_content_resource_is_computed_from_the_page_rather_than_accepted() {
    let command: UpdatePageCommand = serde_json::from_str(
        r#"{"page_path":"/content/example/en/report","title":"Annual Report"}"#,
    )
    .expect("a legal command");
    assert_eq!(
        command.content_path().expect("a content resource"),
        path("/content/example/en/report/jcr:content")
    );
    let answered = UpdatePageResult {
        mutated: ResourceMutationResult {
            repository_path: command.content_path().expect("a content resource"),
        },
    };
    assert_eq!(answered.require_answers(&command), Ok(()));
    let elsewhere = UpdatePageResult {
        mutated: ResourceMutationResult { repository_path: path("/content/other/jcr:content") },
    };
    assert_eq!(
        elsewhere.require_answers(&command),
        Err(MutationResultFailure::NotThisRequest),
        "a result naming another page's content resource was accepted"
    );
}

#[test]
fn a_request_that_changes_nothing_and_one_that_changes_a_property_twice_are_both_refused() {
    for row in rows(UNUSABLE) {
        let note = text(&row, "note");
        let command: UpdatePageCommand =
            serde_json::from_str(text(&row, "document")).expect("the document parses");
        let expected = match text(&row, "refusal") {
            "changes_nothing" => PropertyMutationFailure::ChangesNothing,
            "both_assigned_and_removed" => PropertyMutationFailure::BothAssignedAndRemoved,
            other => panic!("{note}: the fixture names an unknown refusal {other}"),
        };
        assert_eq!(command.require_usable(), Err(expected), "{note}");
    }
}

#[test]
fn every_failure_document_carries_its_two_members_and_says_what_it_proves() {
    let vectors = rows(FAILURES);
    assert_eq!(vectors.len(), 7, "one for each category this command allows");
    for row in &vectors {
        let note = text(row, "note");
        let document = text(row, "document");
        let refusal: UpdatePageRefusal =
            serde_json::from_str(document).unwrap_or_else(|failure| panic!("{note}: {failure}"));
        assert_eq!(
            serde_json::to_string(&refusal).expect("a refusal serializes"),
            document,
            "{note}: rewritten differently"
        );
        let members: Vec<String> = row["members"]
            .as_array()
            .expect("a member list")
            .iter()
            .map(|member| member.as_str().expect("a member name").to_owned())
            .collect();
        assert_eq!(members, vec!["failure".to_owned(), "page_path".to_owned()], "{note}");
        assert_eq!(
            refusal.proves_no_effect(),
            row["proves_no_effect"].as_bool().expect("a verdict"),
            "{note}"
        );
    }
}

#[test]
fn a_failure_document_refuses_a_surplus_member_and_an_unknown_category() {
    assert!(
        serde_json::from_str::<UpdatePageRefusal>(
            r#"{"failure":"page_not_found","page_path":"/content/example","extra":1}"#
        )
        .is_err()
    );
    assert!(
        serde_json::from_str::<UpdatePageRefusal>(
            r#"{"failure":"page_was_grumpy","page_path":"/content/example"}"#
        )
        .is_err()
    );
}

#[test]
fn a_refusal_answers_only_the_request_that_named_its_page() {
    let command: UpdatePageCommand = serde_json::from_str(
        r#"{"page_path":"/content/example/en/report","title":"Annual Report"}"#,
    )
    .expect("a legal command");
    let refusal = UpdatePageRefusal {
        failure: UpdatePageFailure::PageNotFound,
        page_path: path("/content/example/en/report"),
    };
    assert_eq!(refusal.require_answers(&command), Ok(()));
    let elsewhere = UpdatePageRefusal {
        failure: UpdatePageFailure::PageNotFound,
        page_path: path("/content/other"),
    };
    assert_eq!(elsewhere.require_answers(&command), Err(MutationResultFailure::NotThisRequest));
}

#[test]
fn only_the_unknown_outcome_declines_to_prove_no_effect() {
    for failure in [
        UpdatePageFailure::PageNotFound,
        UpdatePageFailure::PageAccessDenied,
        UpdatePageFailure::PageInvalid,
        UpdatePageFailure::PropertyRejected,
        UpdatePageFailure::PropertyNotRemovable,
        UpdatePageFailure::RepositoryCommitFailed,
    ] {
        let refusal = UpdatePageRefusal { failure, page_path: path("/content/example") };
        assert!(refusal.proves_no_effect(), "{failure:?} did not prove no effect");
    }
    let unknown = UpdatePageRefusal {
        failure: UpdatePageFailure::MutationOutcomeUnknown,
        page_path: path("/content/example"),
    };
    assert!(!unknown.proves_no_effect());
}
