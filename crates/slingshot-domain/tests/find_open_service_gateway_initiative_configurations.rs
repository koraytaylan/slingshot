//! Assertions for finding configurations by prefix.
//!
//! The assertion that matters most is structural: the match type has no member
//! that could carry a configuration value. Reading a value is allowed only
//! behind the metatype evidence and redaction the inspection command applies,
//! and a listing has made none of those judgements, so it must not be able to
//! carry one even by accident.

use serde_json::Value;
use slingshot_domain::command::find_open_service_gateway_initiative_configurations::{
    ConfigurationMatch, FindOpenServiceGatewayInitiativeConfigurationsCommand,
    FindOpenServiceGatewayInitiativeConfigurationsResult,
};
use slingshot_domain::command::inspect_open_service_gateway_initiative_configuration::{
    OpenServiceGatewayInitiativePersistentIdentifier, maximum_inspected_configuration_properties,
};
use slingshot_domain::command::operational_listing::ListingResultFailure;

/// Commands this test reads.
const COMMANDS: &str = include_str!(
    "fixtures/commands/find_open_service_gateway_initiative_configurations/commands.jsonl"
);

/// Keys one match reported holding.
const KEYS: u64 = 4;

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

/// Returns one identifier.
fn identifier(value: &str) -> OpenServiceGatewayInitiativePersistentIdentifier {
    OpenServiceGatewayInitiativePersistentIdentifier::new(value).expect("a legal identifier")
}

/// Returns one match over `value`.
fn matched(value: &str) -> ConfigurationMatch {
    ConfigurationMatch::new(false, None, identifier(value), KEYS).expect("a legal match")
}

/// Returns one request over `prefix`.
fn command(prefix: Option<&str>) -> FindOpenServiceGatewayInitiativeConfigurationsCommand {
    FindOpenServiceGatewayInitiativeConfigurationsCommand {
        persistent_identifier_prefix: prefix.map(identifier),
        result_window: None,
    }
}

#[test]
fn every_command_vector_parses_exactly_as_the_fixture_says() {
    let vectors = rows(COMMANDS);
    assert!(vectors.len() >= 5, "every document shape and every refusal");
    for row in &vectors {
        let document = text(row, "document");
        let note = text(row, "note");
        match (
            row["accepted"].as_bool(),
            serde_json::from_str::<FindOpenServiceGatewayInitiativeConfigurationsCommand>(document),
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
fn no_member_of_a_match_could_carry_a_configuration_value() {
    let written = serde_json::to_value(matched("com.example.service.Configuration"))
        .expect("a match serializes");
    let members: Vec<&str> =
        written.as_object().expect("a match is an object").keys().map(String::as_str).collect();
    assert_eq!(
        members,
        vec!["bound_to_a_bundle_location", "persistent_identifier", "property_key_count",],
        "a listing row grew a member that could hold a configuration value"
    );
}

#[test]
fn a_key_count_is_accepted_at_its_bound_and_refused_one_past_it() {
    let bound = maximum_inspected_configuration_properties();
    assert!(
        ConfigurationMatch::new(
            false,
            None,
            identifier("com.example.service.Configuration"),
            bound
        )
        .is_ok()
    );
    assert!(
        ConfigurationMatch::new(
            false,
            None,
            identifier("com.example.service.Configuration"),
            bound + 1
        )
        .is_err()
    );
}

#[test]
fn rows_are_strictly_ascending_and_a_repeat_is_refused() {
    assert!(
        FindOpenServiceGatewayInitiativeConfigurationsResult::new(
            vec![matched("com.example.a"), matched("com.example.b")],
            None
        )
        .is_ok()
    );
    assert_eq!(
        FindOpenServiceGatewayInitiativeConfigurationsResult::new(
            vec![matched("com.example.b"), matched("com.example.a")],
            None
        ),
        Err(ListingResultFailure::NotStrictlyAscending)
    );
}

#[test]
fn a_match_outside_the_requested_prefix_is_refused() {
    let page = FindOpenServiceGatewayInitiativeConfigurationsResult::new(
        vec![matched("com.other.service")],
        None,
    )
    .expect("a legal page");
    assert_eq!(page.require_answers(&command(None)), Ok(()));
    assert_eq!(
        page.require_answers(&command(Some("com.example"))),
        Err(ListingResultFailure::NotThisRequest)
    );
}
