//! Assertions for making an experience fragment with its first variation.
//!
//! The result carries two addresses, and both are checked: each has to be the one
//! this request computes, and the variation has to lie inside the fragment. A
//! result that named a plausible variation belonging to some other fragment
//! would otherwise pass every other check here.

use serde_json::Value;
use slingshot_domain::command::create_experience_fragment::{
    CreateExperienceFragmentCommand, CreateExperienceFragmentFailure,
    CreateExperienceFragmentRefusal, CreateExperienceFragmentResult,
};
use slingshot_domain::command::repository_path::RepositoryPath;
use slingshot_domain::command::resource_mutation::MutationResultFailure;

/// Commands this test reads.
const COMMANDS: &str = include_str!("fixtures/commands/create_experience_fragment/commands.jsonl");

/// Failures this test reads.
const FAILURES: &str = include_str!("fixtures/commands/create_experience_fragment/failures.jsonl");

/// Fragment every vector computes.
const FRAGMENT: &str = "/content/experience-fragments/example/hero";

/// Variation every vector computes with it.
const VARIATION: &str = "/content/experience-fragments/example/hero/web";

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
fn command() -> CreateExperienceFragmentCommand {
    serde_json::from_str(
        r#"{"name":"hero","parent_path":"/content/experience-fragments/example","template_path":"/conf/example/settings/wcm/templates/experience-fragment","variation_name":"web"}"#,
    )
    .expect("a legal command")
}

#[test]
fn every_command_vector_parses_exactly_as_the_fixture_says() {
    let vectors = rows(COMMANDS);
    assert!(vectors.len() >= 4, "every document shape and every refusal");
    for row in &vectors {
        let document = text(row, "document");
        let note = text(row, "note");
        match (
            row["accepted"].as_bool(),
            serde_json::from_str::<CreateExperienceFragmentCommand>(document),
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
fn both_addresses_are_computed_and_the_variation_lies_inside_the_fragment() {
    let asked = command();
    assert_eq!(asked.target_path().expect("a target"), path(FRAGMENT));
    assert_eq!(asked.variation_path().expect("a variation"), path(VARIATION));
    let answered = CreateExperienceFragmentResult {
        repository_path: path(FRAGMENT),
        variation_path: path(VARIATION),
    };
    assert_eq!(answered.require_answers(&asked), Ok(()));
}

#[test]
fn a_variation_belonging_to_another_fragment_is_refused() {
    let asked = command();
    let elsewhere = CreateExperienceFragmentResult {
        repository_path: path(FRAGMENT),
        variation_path: path("/content/experience-fragments/example/other/web"),
    };
    assert_eq!(elsewhere.require_answers(&asked), Err(MutationResultFailure::NotThisRequest));
}

#[test]
fn a_refusal_answers_only_the_request_that_computed_its_target() {
    let refusal = CreateExperienceFragmentRefusal {
        failure: CreateExperienceFragmentFailure::TemplateInvalid,
        target_path: path(FRAGMENT),
    };
    assert_eq!(refusal.require_answers(&command()), Ok(()));
    let elsewhere = CreateExperienceFragmentRefusal {
        failure: CreateExperienceFragmentFailure::TemplateInvalid,
        target_path: path("/content/experience-fragments/example/other"),
    };
    assert_eq!(elsewhere.require_answers(&command()), Err(MutationResultFailure::NotThisRequest));
}

#[test]
fn every_failure_document_carries_its_members_and_says_what_it_proves() {
    let vectors = rows(FAILURES);
    assert_eq!(vectors.len(), 7, "one for each category this command allows");
    for row in &vectors {
        let note = text(row, "note");
        let document = text(row, "document");
        let refusal: CreateExperienceFragmentRefusal =
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
        serde_json::from_str::<CreateExperienceFragmentRefusal>(r#"{"failure":"template_not_found","target_path":"/content/experience-fragments/example/hero","extra":1}"#).is_err(),
        "a surplus member was accepted"
    );
}
