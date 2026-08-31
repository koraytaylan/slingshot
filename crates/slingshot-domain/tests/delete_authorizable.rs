//! Assertions for removing a user or a group.
//!
//! The kind guard is proved in both directions, because it exists for the case
//! where an identifier that is one character off names something else that
//! exists - and removing that looks exactly like success.

use serde_json::Value;
use slingshot_domain::command::authorizable_identity::{AuthorizableIdentifier, AuthorizableKind};
use slingshot_domain::command::delete_authorizable::{
    DeleteAuthorizableCommand, DeleteAuthorizableFailure, DeleteAuthorizableRefusal,
    DeleteAuthorizableResult,
};
use slingshot_domain::command::repository_path::RepositoryPath;
use slingshot_domain::command::resource_mutation::MutationResultFailure;

/// Commands this test reads.
const COMMANDS: &str = include_str!("fixtures/commands/delete_authorizable/commands.jsonl");

/// Failures this test reads.
const FAILURES: &str = include_str!("fixtures/commands/delete_authorizable/failures.jsonl");

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

/// Returns one request over `expected_kind`.
fn command(expected_kind: AuthorizableKind) -> DeleteAuthorizableCommand {
    DeleteAuthorizableCommand { authorizable_identifier: identifier(USER), expected_kind }
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
            serde_json::from_str::<DeleteAuthorizableCommand>(document),
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
fn the_kind_guard_holds_in_both_directions() {
    let removed_user = DeleteAuthorizableResult {
        authorizable_identifier: identifier(USER),
        kind: AuthorizableKind::User,
        repository_path: path("/home/users/s/somewhere"),
    };
    assert_eq!(removed_user.require_answers(&command(AuthorizableKind::User)), Ok(()));
    assert_eq!(
        removed_user.require_answers(&command(AuthorizableKind::Group)),
        Err(MutationResultFailure::NotThisRequest),
        "a user was removed under a request that expected a group"
    );
    let removed_group = DeleteAuthorizableResult {
        authorizable_identifier: identifier(USER),
        kind: AuthorizableKind::Group,
        repository_path: path("/home/groups/s/somewhere"),
    };
    assert_eq!(
        removed_group.require_answers(&command(AuthorizableKind::User)),
        Err(MutationResultFailure::NotThisRequest)
    );
}

#[test]
fn a_group_with_members_is_refused_rather_than_emptied() {
    let refusal = DeleteAuthorizableRefusal {
        authorizable_identifier: identifier(USER),
        failure: DeleteAuthorizableFailure::GroupHasMembers,
    };
    assert!(refusal.proves_no_effect());
    assert_eq!(refusal.require_answers(&command(AuthorizableKind::Group)), Ok(()));
    assert_eq!(
        refusal.require_answers(&command(AuthorizableKind::User)),
        Err(MutationResultFailure::NotThisRequest),
        "a request that expected a user was answered with a group's members"
    );
}

#[test]
fn an_absent_authorizable_is_a_failure_rather_than_a_quiet_success() {
    let refusal = DeleteAuthorizableRefusal {
        authorizable_identifier: identifier(USER),
        failure: DeleteAuthorizableFailure::AuthorizableNotFound,
    };
    assert!(refusal.proves_no_effect());
    assert_eq!(refusal.require_answers(&command(AuthorizableKind::User)), Ok(()));
}

#[test]
fn every_failure_document_carries_its_members_and_says_what_it_proves() {
    let vectors = rows(FAILURES);
    assert_eq!(vectors.len(), 6, "one for each category this command allows");
    for row in &vectors {
        let note = text(row, "note");
        let document = text(row, "document");
        let refusal: DeleteAuthorizableRefusal =
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
        serde_json::from_str::<DeleteAuthorizableRefusal>(
            r#"{"authorizable_identifier":"author","failure":"authorizable_not_found","extra":1}"#
        )
        .is_err(),
        "a surplus member was accepted"
    );
}
