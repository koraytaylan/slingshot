//! Assertions for listing a group's members.
//!
//! The direct-or-indirect distinction is what makes a membership listing an
//! answer to "why does this person have access" rather than a list that does not
//! contain the reason.

use serde_json::Value;
use slingshot_domain::command::authorizable_identity::{AuthorizableIdentifier, AuthorizableKind};
use slingshot_domain::command::list_group_members::{
    GroupMemberMatch, ListGroupMembersCommand, ListGroupMembersFailure, ListGroupMembersRefusal,
    ListGroupMembersResult,
};
use slingshot_domain::command::operational_listing::ListingResultFailure;
use slingshot_domain::command::repository_path::RepositoryPath;

/// Commands this test reads.
const COMMANDS: &str = include_str!("fixtures/commands/list_group_members/commands.jsonl");

/// Group every vector lists.
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

/// Returns one member row.
fn matched(value: &str, direct: bool) -> GroupMemberMatch {
    GroupMemberMatch {
        authorizable_identifier: identifier(value),
        direct,
        kind: AuthorizableKind::User,
        repository_path: path("/home/users/s/somewhere"),
    }
}

/// Returns one request over `include_indirect`.
fn command(include_indirect: bool) -> ListGroupMembersCommand {
    ListGroupMembersCommand {
        group_identifier: identifier(GROUP),
        include_indirect,
        result_window: None,
    }
}

#[test]
fn every_command_vector_parses_exactly_as_the_fixture_says() {
    let vectors = rows(COMMANDS);
    assert!(vectors.len() >= 4, "every document shape and every refusal");
    for row in &vectors {
        let document = text(row, "document");
        let note = text(row, "note");
        match (row["accepted"].as_bool(), serde_json::from_str::<ListGroupMembersCommand>(document))
        {
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
fn an_indirect_member_is_refused_in_an_answer_to_a_direct_only_request() {
    let indirect =
        ListGroupMembersResult::new(vec![matched("someone", false)], None).expect("a legal page");
    assert_eq!(indirect.require_answers(&command(true)), Ok(()));
    assert_eq!(
        indirect.require_answers(&command(false)),
        Err(ListingResultFailure::NotThisRequest)
    );
    let direct =
        ListGroupMembersResult::new(vec![matched("someone", true)], None).expect("a legal page");
    assert_eq!(direct.require_answers(&command(false)), Ok(()));
}

#[test]
fn rows_are_strictly_ascending_by_member_identifier() {
    assert!(
        ListGroupMembersResult::new(vec![matched("alex", true), matched("blake", true)], None)
            .is_ok()
    );
    assert_eq!(
        ListGroupMembersResult::new(vec![matched("blake", true), matched("alex", true)], None),
        Err(ListingResultFailure::NotStrictlyAscending)
    );
}

#[test]
fn a_user_identifier_as_the_group_is_a_kind_mismatch_rather_than_an_empty_listing() {
    let refusal = ListGroupMembersRefusal {
        failure: ListGroupMembersFailure::AuthorizableKindMismatch,
        group_identifier: identifier(GROUP),
    };
    assert_eq!(refusal.require_answers(&command(false)), Ok(()));
    let elsewhere = ListGroupMembersRefusal {
        failure: ListGroupMembersFailure::AuthorizableKindMismatch,
        group_identifier: identifier("another-group"),
    };
    assert_eq!(
        elsewhere.require_answers(&command(false)),
        Err(ListingResultFailure::NotThisRequest)
    );
}
