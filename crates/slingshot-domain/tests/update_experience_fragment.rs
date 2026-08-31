//! Assertions for editing one experience fragment variation.
//!
//! The variation is addressed directly rather than composed from a fragment and
//! a name, and the content resource under it is computed rather than accepted -
//! the same rule a page update follows, for the same reason: properties written
//! to the variation node would be stored and never rendered.

use serde_json::Value;
use slingshot_domain::command::repository_path::RepositoryPath;
use slingshot_domain::command::resource_mutation::{
    MutationResultFailure, PropertyMutationFailure, ResourceMutationResult,
};
use slingshot_domain::command::update_experience_fragment::{
    UpdateExperienceFragmentCommand, UpdateExperienceFragmentFailure,
    UpdateExperienceFragmentRefusal, UpdateExperienceFragmentResult,
};

/// Commands this test reads.
const COMMANDS: &str = include_str!("fixtures/commands/update_experience_fragment/commands.jsonl");

/// Parsable requests the shared mutation rule then refuses.
const UNUSABLE: &str = include_str!("fixtures/commands/update_experience_fragment/unusable.jsonl");

/// Failures this test reads.
const FAILURES: &str = include_str!("fixtures/commands/update_experience_fragment/failures.jsonl");

/// Variation every vector addresses.
const VARIATION: &str = "/content/experience-fragments/example/hero/web";

/// Content resource under it.
const CONTENT: &str = "/content/experience-fragments/example/hero/web/jcr:content";

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
fn command() -> UpdateExperienceFragmentCommand {
    serde_json::from_str(&format!("{{\"title\":\"Hero\",\"variation_path\":\"{VARIATION}\"}}"))
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
            serde_json::from_str::<UpdateExperienceFragmentCommand>(document),
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
fn the_content_resource_is_computed_from_the_variation() {
    assert_eq!(command().content_path().expect("a content resource"), path(CONTENT));
    let answered = UpdateExperienceFragmentResult {
        mutated: ResourceMutationResult { repository_path: path(CONTENT) },
    };
    assert_eq!(answered.require_answers(&command()), Ok(()));
    let elsewhere = UpdateExperienceFragmentResult {
        mutated: ResourceMutationResult { repository_path: path(VARIATION) },
    };
    assert_eq!(
        elsewhere.require_answers(&command()),
        Err(MutationResultFailure::NotThisRequest),
        "a result naming the variation node itself was accepted"
    );
}

#[test]
fn the_shared_mutation_rule_applies_here_unchanged() {
    for row in rows(UNUSABLE) {
        let note = text(&row, "note");
        let parsed: UpdateExperienceFragmentCommand = serde_json::from_str(text(&row, "document"))
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
fn a_refusal_answers_only_the_request_that_named_its_variation() {
    let refusal = UpdateExperienceFragmentRefusal {
        failure: UpdateExperienceFragmentFailure::VariationNotFound,
        variation_path: path(VARIATION),
    };
    assert_eq!(refusal.require_answers(&command()), Ok(()));
    let elsewhere = UpdateExperienceFragmentRefusal {
        failure: UpdateExperienceFragmentFailure::VariationNotFound,
        variation_path: path("/content/experience-fragments/example/hero/mobile"),
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
        let refusal: UpdateExperienceFragmentRefusal =
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
        serde_json::from_str::<UpdateExperienceFragmentRefusal>(r#"{"failure":"variation_invalid","variation_path":"/content/experience-fragments/example/hero/web","extra":1}"#).is_err(),
        "a surplus member was accepted"
    );
}
