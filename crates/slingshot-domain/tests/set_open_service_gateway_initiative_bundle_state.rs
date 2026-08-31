//! Assertions for starting, stopping, and refreshing a bundle.
//!
//! The observed-state rule is the one worth pinning. A transition can be accepted
//! and not take effect, and this contract reports what the author observed rather
//! than deciding the author is wrong about its own bundle - so a result observing
//! `active` after a stop is accepted, and a caller can see that it did not take.

use serde_json::Value;
use slingshot_domain::command::platform_service_identity::{BundleState, BundleSymbolicName};
use slingshot_domain::command::resource_mutation::MutationResultFailure;
use slingshot_domain::command::set_open_service_gateway_initiative_bundle_state::{
    BundleTransition, SetOpenServiceGatewayInitiativeBundleStateCommand,
    SetOpenServiceGatewayInitiativeBundleStateFailure,
    SetOpenServiceGatewayInitiativeBundleStateRefusal,
    SetOpenServiceGatewayInitiativeBundleStateResult,
};

/// Commands this test reads.
const COMMANDS: &str = include_str!(
    "fixtures/commands/set_open_service_gateway_initiative_bundle_state/commands.jsonl"
);

/// Failures this test reads.
const FAILURES: &str = include_str!(
    "fixtures/commands/set_open_service_gateway_initiative_bundle_state/failures.jsonl"
);

/// Bundle every vector names.
const BUNDLE: &str = "com.example.bundle";

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

/// Returns one symbolic name.
fn name(value: &str) -> BundleSymbolicName {
    BundleSymbolicName::parse(value).expect("a legal symbolic name")
}

/// Returns one legal request over `transition`.
fn command(transition: BundleTransition) -> SetOpenServiceGatewayInitiativeBundleStateCommand {
    SetOpenServiceGatewayInitiativeBundleStateCommand { symbolic_name: name(BUNDLE), transition }
}

#[test]
fn every_command_vector_parses_exactly_as_the_fixture_says() {
    let vectors = rows(COMMANDS);
    assert!(vectors.len() >= 6, "every document shape and every refusal");
    for row in &vectors {
        let document = text(row, "document");
        let note = text(row, "note");
        match (
            row["accepted"].as_bool(),
            serde_json::from_str::<SetOpenServiceGatewayInitiativeBundleStateCommand>(document),
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
fn all_three_transitions_round_trip_and_a_fourth_is_refused() {
    for (transition, spelling) in [
        (BundleTransition::Refresh, "\"refresh\""),
        (BundleTransition::Start, "\"start\""),
        (BundleTransition::Stop, "\"stop\""),
    ] {
        assert_eq!(serde_json::to_string(&transition).expect("it serializes"), spelling);
    }
    assert!(serde_json::from_str::<BundleTransition>("\"reboot\"").is_err());
}

#[test]
fn the_answer_is_the_state_that_was_observed_and_not_the_one_that_was_asked_for() {
    let asked = command(BundleTransition::Stop);
    let stubborn = SetOpenServiceGatewayInitiativeBundleStateResult {
        observed_state: BundleState::Active,
        symbolic_name: name(BUNDLE),
    };
    assert_eq!(
        stubborn.require_answers(&asked),
        Ok(()),
        "a bundle that refused to stop was reported as a contract violation"
    );
}

#[test]
fn a_result_answers_only_the_request_that_named_its_bundle() {
    let asked = command(BundleTransition::Start);
    let elsewhere = SetOpenServiceGatewayInitiativeBundleStateResult {
        observed_state: BundleState::Active,
        symbolic_name: name("com.other.bundle"),
    };
    assert_eq!(elsewhere.require_answers(&asked), Err(MutationResultFailure::NotThisRequest));
}

#[test]
fn a_refusal_answers_only_the_request_that_named_its_bundle() {
    let asked = command(BundleTransition::Start);
    let refusal = SetOpenServiceGatewayInitiativeBundleStateRefusal {
        failure: SetOpenServiceGatewayInitiativeBundleStateFailure::BundleNotFound,
        symbolic_name: name(BUNDLE),
    };
    assert_eq!(refusal.require_answers(&asked), Ok(()));
    let elsewhere = SetOpenServiceGatewayInitiativeBundleStateRefusal {
        failure: SetOpenServiceGatewayInitiativeBundleStateFailure::BundleNotFound,
        symbolic_name: name("com.other.bundle"),
    };
    assert_eq!(elsewhere.require_answers(&asked), Err(MutationResultFailure::NotThisRequest));
}

#[test]
fn every_failure_document_carries_its_members_and_says_what_it_proves() {
    let vectors = rows(FAILURES);
    assert_eq!(vectors.len(), 4, "one for each category this command allows");
    for row in &vectors {
        let note = text(row, "note");
        let document = text(row, "document");
        let refusal: SetOpenServiceGatewayInitiativeBundleStateRefusal =
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
        serde_json::from_str::<SetOpenServiceGatewayInitiativeBundleStateRefusal>(
            r#"{"failure":"bundle_not_found","symbolic_name":"com.example.bundle","extra":1}"#
        )
        .is_err(),
        "a surplus member was accepted"
    );
}
