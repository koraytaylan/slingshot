//! Assertions for disabling an account and enabling it again.
//!
//! A reason belongs to a disabling. A reason for an enabling would be a value
//! the author stores and nobody reads, and a member that is sometimes
//! meaningless is a member somebody eventually fills in meaninglessly - so it is
//! refused rather than ignored.

use serde_json::Value;
use slingshot_domain::command::authorizable_identity::AuthorizableIdentifier;
use slingshot_domain::command::command_identity::CommandContract;
use slingshot_domain::command::resource_mutation::MutationResultFailure;
use slingshot_domain::command::set_user_disabled::{
    SetUserDisabledCommand, SetUserDisabledFailure, SetUserDisabledRefusal, SetUserDisabledResult,
};

/// Commands this test reads.
const COMMANDS: &str = include_str!("fixtures/commands/set_user_disabled/commands.jsonl");

/// Failures this test reads.
const FAILURES: &str = include_str!("fixtures/commands/set_user_disabled/failures.jsonl");

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

/// Returns one authorizable identifier.
fn identifier(value: &str) -> AuthorizableIdentifier {
    AuthorizableIdentifier::parse(value).expect("a legal identifier")
}

/// Returns one request over `disabled` and an optional reason.
fn command(disabled: bool, reason: Option<&str>) -> SetUserDisabledCommand {
    SetUserDisabledCommand {
        authorizable_identifier: identifier(USER),
        disabled,
        reason: reason.map(str::to_owned),
    }
}

#[test]
fn every_command_vector_parses_exactly_as_the_fixture_says() {
    let vectors = rows(COMMANDS);
    assert!(vectors.len() >= 4, "every document shape and every refusal");
    for row in &vectors {
        let document = text(row, "document");
        let note = text(row, "note");
        match (row["accepted"].as_bool(), serde_json::from_str::<SetUserDisabledCommand>(document))
        {
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
fn a_reason_belongs_to_a_disabling_and_to_nothing_else() {
    assert_eq!(command(true, Some("Left the company")).require_usable(), Ok(()));
    assert_eq!(
        command(false, Some("Left the company")).require_usable(),
        Err(MutationResultFailure::RequestContradictsItself),
        "a reason for an enabling was accepted"
    );
    assert_eq!(command(false, None).require_usable(), Ok(()));
}

#[test]
fn a_reason_is_accepted_at_its_bound_and_refused_one_byte_past_it() {
    let bound = usize::try_from(
        CommandContract::embedded().limit("maximum_authorizable_disabled_reason_bytes"),
    )
    .expect("the bound fits");
    let exact = "a".repeat(bound);
    assert_eq!(command(true, Some(&exact)).require_usable(), Ok(()));
    let beyond = "a".repeat(bound + 1);
    assert_eq!(
        command(true, Some(&beyond)).require_usable(),
        Err(MutationResultFailure::CountTooLarge)
    );
}

#[test]
fn a_result_answers_only_the_request_that_named_its_user() {
    let answered =
        SetUserDisabledResult { authorizable_identifier: identifier(USER), disabled: true };
    assert_eq!(answered.require_answers(&command(true, None)), Ok(()));
    let elsewhere = SetUserDisabledResult {
        authorizable_identifier: identifier("someone-else"),
        disabled: true,
    };
    assert_eq!(
        elsewhere.require_answers(&command(true, None)),
        Err(MutationResultFailure::NotThisRequest)
    );
}

#[test]
fn a_group_identifier_is_a_kind_mismatch() {
    let refusal = SetUserDisabledRefusal {
        authorizable_identifier: identifier(USER),
        failure: SetUserDisabledFailure::AuthorizableKindMismatch,
    };
    assert!(refusal.proves_no_effect());
    assert_eq!(refusal.require_answers(&command(true, None)), Ok(()));
}

#[test]
fn every_failure_document_carries_its_members_and_says_what_it_proves() {
    let vectors = rows(FAILURES);
    assert_eq!(vectors.len(), 5, "one for each category this command allows");
    for row in &vectors {
        let note = text(row, "note");
        let document = text(row, "document");
        let refusal: SetUserDisabledRefusal =
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
        serde_json::from_str::<SetUserDisabledRefusal>(
            r#"{"authorizable_identifier":"author","failure":"authorizable_not_found","extra":1}"#
        )
        .is_err(),
        "a surplus member was accepted"
    );
}
