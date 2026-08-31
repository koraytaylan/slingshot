//! Assertions for creating a user or a group.
//!
//! The structural assertion is the one that matters: no member of either command
//! or of the result could hold a credential. A created account therefore cannot
//! authenticate, which is a real limitation and is stated in the command's own
//! documentation rather than left to be discovered at the first sign-in.

use serde_json::Value;
use slingshot_domain::command::authorizable_identity::{AuthorizableIdentifier, AuthorizableKind};
use slingshot_domain::command::create_authorizable::{
    CreateAuthorizableFailure, CreateAuthorizableRefusal, CreateAuthorizableResult,
    CreateGroupCommand, CreateUserCommand,
};
use slingshot_domain::command::repository_path::RepositoryPath;
use slingshot_domain::command::resource_mutation::MutationResultFailure;

/// User commands this test reads.
const COMMANDS: &str = include_str!("fixtures/commands/create_authorizable/commands.jsonl");

/// Group commands this test reads.
const GROUPS: &str = include_str!("fixtures/commands/create_authorizable/groups.jsonl");

/// Failures this test reads.
const FAILURES: &str = include_str!("fixtures/commands/create_authorizable/failures.jsonl");

/// A value nothing may carry.
const SENTINEL: &str = "correct-horse-battery-staple";

/// User every vector creates.
const USER: &str = "author";

/// Group every vector creates.
const GROUP: &str = "content-authors";

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

#[test]
fn every_command_vector_parses_exactly_as_the_fixture_says() {
    let vectors = rows(COMMANDS);
    assert!(vectors.len() >= 6, "every document shape and every refusal");
    for row in &vectors {
        let document = text(row, "document");
        let note = text(row, "note");
        match (row["accepted"].as_bool(), serde_json::from_str::<CreateUserCommand>(document)) {
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
fn every_group_vector_parses_exactly_as_the_fixture_says() {
    let vectors = rows(GROUPS);
    assert!(vectors.len() >= 3, "the group, its place, and the refusal that matters");
    for row in &vectors {
        let document = text(row, "document");
        let note = text(row, "note");
        match (row["accepted"].as_bool(), serde_json::from_str::<CreateGroupCommand>(document)) {
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
fn no_member_of_either_command_could_carry_a_credential() {
    let user: CreateUserCommand =
        serde_json::from_str(&format!("{{\"authorizable_identifier\":\"{USER}\"}}"))
            .expect("a legal command");
    let written = serde_json::to_value(&user).expect("a command serializes");
    let members: Vec<&str> =
        written.as_object().expect("an object").keys().map(String::as_str).collect();
    assert_eq!(members, vec!["authorizable_identifier"]);
    for carrying in [
        format!("{{\"authorizable_identifier\":\"{USER}\",\"password\":\"{SENTINEL}\"}}"),
        format!("{{\"authorizable_identifier\":\"{USER}\",\"private_key\":\"{SENTINEL}\"}}"),
    ] {
        assert!(
            serde_json::from_str::<CreateUserCommand>(&carrying).is_err(),
            "a credential-shaped member was accepted"
        );
        assert!(serde_json::from_str::<CreateGroupCommand>(&carrying).is_err());
    }
}

#[test]
fn a_result_names_the_requested_identifier_and_kind_and_not_the_address() {
    let answered = CreateAuthorizableResult {
        authorizable_identifier: identifier(USER),
        kind: AuthorizableKind::User,
        repository_path: path("/home/users/s/somewhere-the-author-chose"),
    };
    assert_eq!(answered.require_answers(&identifier(USER), AuthorizableKind::User), Ok(()));
    assert_eq!(
        answered.require_answers(&identifier(USER), AuthorizableKind::Group),
        Err(MutationResultFailure::NotThisRequest),
        "a user was accepted as the answer to a group creation"
    );
    assert_eq!(
        answered.require_answers(&identifier(GROUP), AuthorizableKind::User),
        Err(MutationResultFailure::NotThisRequest)
    );
}

#[test]
fn a_refusal_answers_only_the_request_that_named_its_identifier() {
    let refusal = CreateAuthorizableRefusal {
        authorizable_identifier: identifier(USER),
        failure: CreateAuthorizableFailure::AuthorizableAlreadyExists,
    };
    assert_eq!(refusal.require_answers(&identifier(USER)), Ok(()));
    assert_eq!(
        refusal.require_answers(&identifier(GROUP)),
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
        let refusal: CreateAuthorizableRefusal =
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
        serde_json::from_str::<CreateAuthorizableRefusal>(
            r#"{"authorizable_identifier":"author","failure":"identifier_rejected","extra":1}"#
        )
        .is_err(),
        "a surplus member was accepted"
    );
}
