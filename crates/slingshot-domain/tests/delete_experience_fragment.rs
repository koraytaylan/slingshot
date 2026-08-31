//! Assertions for removing an experience fragment.
//!
//! What refers to an experience fragment refers to one of its variations, so the
//! reference policy matters more here than anywhere: the caller is removing
//! things it may never have looked at.

use serde_json::Value;
use slingshot_domain::command::delete_experience_fragment::{
    DeleteExperienceFragmentCommand, DeleteExperienceFragmentFailure,
    DeleteExperienceFragmentRefusal, DeleteExperienceFragmentResult,
};
use slingshot_domain::command::repository_path::RepositoryPath;
use slingshot_domain::command::resource_mutation::{DeletedResourceResult, MutationResultFailure};

/// Commands this test reads.
const COMMANDS: &str = include_str!("fixtures/commands/delete_experience_fragment/commands.jsonl");

/// Failures this test reads.
const FAILURES: &str = include_str!("fixtures/commands/delete_experience_fragment/failures.jsonl");

/// Fragment every vector addresses.
const FRAGMENT: &str = "/content/experience-fragments/example/hero";

/// Nodes one removal reported taking with it.
const REMOVED: u64 = 20;

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
fn command(policy: &str) -> DeleteExperienceFragmentCommand {
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
            serde_json::from_str::<DeleteExperienceFragmentCommand>(document),
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
        serde_json::from_str::<DeleteExperienceFragmentCommand>(&format!(
            "{{\"fragment_path\":\"{FRAGMENT}\"}}"
        ))
        .is_err()
    );
    assert!(command("refuse_when_referenced").refuses_when_referenced());
    assert!(!command("ignore_references").refuses_when_referenced());
}

#[test]
fn a_reference_refusal_belongs_only_to_a_request_that_asked_to_refuse() {
    let refusal = DeleteExperienceFragmentRefusal {
        failure: DeleteExperienceFragmentFailure::FragmentIsReferenced,
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
    let answered = DeleteExperienceFragmentResult {
        deleted: DeletedResourceResult::new(path(FRAGMENT), REMOVED).expect("a legal deletion"),
    };
    assert_eq!(answered.require_answers(&command("ignore_references")), Ok(()));
    let elsewhere = DeleteExperienceFragmentResult {
        deleted: DeletedResourceResult::new(
            path("/content/experience-fragments/example/other"),
            REMOVED,
        )
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
        let refusal: DeleteExperienceFragmentRefusal =
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
        serde_json::from_str::<DeleteExperienceFragmentRefusal>(r#"{"failure":"fragment_invalid","fragment_path":"/content/experience-fragments/example/hero","extra":1}"#).is_err(),
        "a surplus member was accepted"
    );
}
