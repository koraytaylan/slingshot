//! Assertions for adding a member to a group and taking one out.
//!
//! Each result answers the question a caller has afterwards - did this change
//! anything - so a no-op is distinguishable from a change without a second
//! request. A group may be a member of a group, because that is the ordinary
//! case in any deployment with more than a handful of them.

use serde_json::Value;
use slingshot_domain::command::authorizable_identity::AuthorizableIdentifier;
use slingshot_domain::command::group_membership::{
    AddGroupMemberCommand, AddGroupMemberResult, GroupMembershipFailure, GroupMembershipRefusal,
    RemoveGroupMemberCommand, RemoveGroupMemberResult,
};
use slingshot_domain::command::resource_mutation::MutationResultFailure;

/// Commands this test reads.
const COMMANDS: &str = include_str!("fixtures/commands/group_membership/commands.jsonl");

/// Failures this test reads.
const FAILURES: &str = include_str!("fixtures/commands/group_membership/failures.jsonl");

/// Group every vector changes.
const GROUP: &str = "content-authors";

/// Member every vector moves.
const MEMBER: &str = "author";

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

/// Returns one authorizable identifier.
fn identifier(value: &str) -> AuthorizableIdentifier {
    AuthorizableIdentifier::parse(value).expect("a legal identifier")
}

/// Returns one addition.
fn adding() -> AddGroupMemberCommand {
    AddGroupMemberCommand {
        group_identifier: identifier(GROUP),
        member_identifier: identifier(MEMBER),
    }
}

/// Returns one removal.
fn removing() -> RemoveGroupMemberCommand {
    RemoveGroupMemberCommand {
        group_identifier: identifier(GROUP),
        member_identifier: identifier(MEMBER),
    }
}

#[test]
fn every_command_vector_parses_exactly_as_the_fixture_says() {
    let vectors = rows(COMMANDS);
    assert!(vectors.len() >= 5, "every document shape and every refusal");
    for row in &vectors {
        let document = text(row, "document");
        let note = text(row, "note");
        match (row["accepted"].as_bool(), serde_json::from_str::<AddGroupMemberCommand>(document)) {
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
fn a_group_cannot_be_its_own_member() {
    let itself = AddGroupMemberCommand {
        group_identifier: identifier(GROUP),
        member_identifier: identifier(GROUP),
    };
    assert_eq!(itself.require_usable(), Err(MutationResultFailure::RequestContradictsItself));
    let removing_itself = RemoveGroupMemberCommand {
        group_identifier: identifier(GROUP),
        member_identifier: identifier(GROUP),
    };
    assert_eq!(
        removing_itself.require_usable(),
        Err(MutationResultFailure::RequestContradictsItself)
    );
}

#[test]
fn each_result_says_whether_it_changed_anything() {
    for already_a_member in [true, false] {
        let answered = AddGroupMemberResult {
            already_a_member,
            group_identifier: identifier(GROUP),
            member_identifier: identifier(MEMBER),
        };
        assert_eq!(answered.require_answers(&adding()), Ok(()));
    }
    for was_a_member in [true, false] {
        let answered = RemoveGroupMemberResult {
            group_identifier: identifier(GROUP),
            member_identifier: identifier(MEMBER),
            was_a_member,
        };
        assert_eq!(answered.require_answers(&removing()), Ok(()));
    }
}

#[test]
fn neither_result_accepts_the_others_outcome_member() {
    assert!(
        serde_json::from_str::<AddGroupMemberResult>(&format!(
            "{{\"group_identifier\":\"{GROUP}\",\"member_identifier\":\"{MEMBER}\",\"was_a_member\":true}}"
        ))
        .is_err()
    );
    assert!(
        serde_json::from_str::<RemoveGroupMemberResult>(&format!(
            "{{\"already_a_member\":true,\"group_identifier\":\"{GROUP}\",\"member_identifier\":\"{MEMBER}\"}}"
        ))
        .is_err()
    );
}

#[test]
fn each_result_answers_only_the_request_that_named_its_pair() {
    let elsewhere = AddGroupMemberResult {
        already_a_member: false,
        group_identifier: identifier("another-group"),
        member_identifier: identifier(MEMBER),
    };
    assert_eq!(elsewhere.require_answers(&adding()), Err(MutationResultFailure::NotThisRequest));
    let refusal = GroupMembershipRefusal {
        failure: GroupMembershipFailure::MembershipCycleRefused,
        group_identifier: identifier(GROUP),
        member_identifier: identifier(MEMBER),
    };
    assert_eq!(refusal.require_answers(&identifier(GROUP), &identifier(MEMBER)), Ok(()));
    assert_eq!(
        refusal.require_answers(&identifier(GROUP), &identifier("someone-else")),
        Err(MutationResultFailure::NotThisRequest)
    );
}

#[test]
fn a_group_may_be_a_member_of_a_group() {
    let nested = AddGroupMemberCommand {
        group_identifier: identifier(GROUP),
        member_identifier: identifier("editors"),
    };
    assert_eq!(nested.require_usable(), Ok(()));
}

#[test]
fn every_failure_document_carries_its_members_and_says_what_it_proves() {
    let vectors = rows(FAILURES);
    assert_eq!(vectors.len(), 7, "one for each category this command allows");
    for row in &vectors {
        let note = text(row, "note");
        let document = text(row, "document");
        let refusal: GroupMembershipRefusal =
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
        serde_json::from_str::<GroupMembershipRefusal>(r#"{"failure":"group_not_found","group_identifier":"content-authors","member_identifier":"author","extra":1}"#).is_err(),
        "a surplus member was accepted"
    );
}
