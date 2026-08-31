//! Assertions for writing a user's profile.
//!
//! A group identifier is refused rather than quietly writing a group's node as
//! though it were a profile, which is a mistake nothing afterwards would show.

use serde_json::Value;
use slingshot_domain::command::authorizable_identity::AuthorizableIdentifier;
use slingshot_domain::command::repository_path::RepositoryPath;
use slingshot_domain::command::resource_mutation::{
    MutationResultFailure, PropertyMutationFailure,
};
use slingshot_domain::command::update_user_profile::{
    UpdateUserProfileCommand, UpdateUserProfileFailure, UpdateUserProfileRefusal,
    UpdateUserProfileResult,
};

/// Commands this test reads.
const COMMANDS: &str = include_str!("fixtures/commands/update_user_profile/commands.jsonl");

/// Parsable requests the shared mutation rule then refuses.
const UNUSABLE: &str = include_str!("fixtures/commands/update_user_profile/unusable.jsonl");

/// Failures this test reads.
const FAILURES: &str = include_str!("fixtures/commands/update_user_profile/failures.jsonl");

/// User every vector addresses.
const USER: &str = "author";

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

/// Returns one authorizable identifier.
fn identifier(value: &str) -> AuthorizableIdentifier {
    AuthorizableIdentifier::parse(value).expect("a legal identifier")
}

/// Returns one legal request.
fn command() -> UpdateUserProfileCommand {
    serde_json::from_str(&format!(
        "{{\"authorizable_identifier\":\"{USER}\",\"removed_property_names\":[\"givenName\"]}}"
    ))
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
            serde_json::from_str::<UpdateUserProfileCommand>(document),
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
fn the_shared_mutation_rule_applies_here_unchanged() {
    for row in rows(UNUSABLE) {
        let note = text(&row, "note");
        let parsed: UpdateUserProfileCommand = serde_json::from_str(text(&row, "document"))
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
fn a_group_identifier_is_a_kind_mismatch_rather_than_a_quiet_write() {
    let refusal = UpdateUserProfileRefusal {
        authorizable_identifier: identifier(USER),
        failure: UpdateUserProfileFailure::AuthorizableKindMismatch,
    };
    assert!(refusal.proves_no_effect());
    assert_eq!(refusal.require_answers(&command()), Ok(()));
}

#[test]
fn a_result_answers_only_the_request_that_named_its_user() {
    let answered = UpdateUserProfileResult {
        authorizable_identifier: identifier(USER),
        repository_path: path("/home/users/s/somewhere/profile"),
    };
    assert_eq!(answered.require_answers(&command()), Ok(()));
    let elsewhere = UpdateUserProfileResult {
        authorizable_identifier: identifier("someone-else"),
        repository_path: path("/home/users/s/somewhere/profile"),
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
        let refusal: UpdateUserProfileRefusal =
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
        serde_json::from_str::<UpdateUserProfileRefusal>(
            r#"{"authorizable_identifier":"author","failure":"authorizable_not_found","extra":1}"#
        )
        .is_err(),
        "a surplus member was accepted"
    );
}
