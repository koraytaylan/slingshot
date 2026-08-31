//! Assertions for changing a configuration.
//!
//! One assertion here is the whole security posture of the command: the result
//! type has no member that could hold an assigned value, and a secret placed in
//! an assignment never reaches any rendered result or refusal. Values go in
//! because that is the point; nothing comes back out but a count.

use serde_json::Value;
use slingshot_domain::command::inspect_open_service_gateway_initiative_configuration::{
    OpenServiceGatewayInitiativePersistentIdentifier, maximum_inspected_configuration_properties,
};
use slingshot_domain::command::update_open_service_gateway_initiative_configuration::{
    ConfigurationUpdateFailure, UpdateOpenServiceGatewayInitiativeConfigurationCommand,
    UpdateOpenServiceGatewayInitiativeConfigurationFailure,
    UpdateOpenServiceGatewayInitiativeConfigurationRefusal,
    UpdateOpenServiceGatewayInitiativeConfigurationResult,
};

/// Commands this test reads.
const COMMANDS: &str = include_str!(
    "fixtures/commands/update_open_service_gateway_initiative_configuration/commands.jsonl"
);

/// Failures this test reads.
const FAILURES: &str = include_str!(
    "fixtures/commands/update_open_service_gateway_initiative_configuration/failures.jsonl"
);

/// A value nothing may echo.
const SENTINEL: &str = "correct-horse-battery-staple";

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

/// Returns one legal request.
fn command() -> UpdateOpenServiceGatewayInitiativeConfigurationCommand {
    serde_json::from_str(
        r#"{"persistent_identifier":"com.example.service.Configuration","removed_property_keys":["host"]}"#,
    )
    .expect("a legal command")
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
            serde_json::from_str::<UpdateOpenServiceGatewayInitiativeConfigurationCommand>(
                document,
            ),
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
fn a_key_assigned_and_removed_by_one_request_is_refused() {
    let both: UpdateOpenServiceGatewayInitiativeConfigurationCommand = serde_json::from_str(
        r#"{"assignments":{"host":{"type":"string","cardinality":"scalar","value":"example.test"}},"persistent_identifier":"com.example.service.Configuration","removed_property_keys":["host"]}"#,
    )
    .expect("a parsable command");
    assert_eq!(both.require_usable(), Err(ConfigurationUpdateFailure::BothAssignedAndRemoved));
}

#[test]
fn a_request_that_changes_nothing_is_refused() {
    let nothing: UpdateOpenServiceGatewayInitiativeConfigurationCommand =
        serde_json::from_str(r#"{"persistent_identifier":"com.example.service.Configuration"}"#)
            .expect("a parsable command");
    assert_eq!(nothing.require_usable(), Err(ConfigurationUpdateFailure::ChangesNothing));
}

#[test]
fn no_member_of_the_result_could_carry_an_assigned_value() {
    let answered = UpdateOpenServiceGatewayInitiativeConfigurationResult {
        changed_property_key_count: 1,
        persistent_identifier: identifier("com.example.service.Configuration"),
    };
    let written = serde_json::to_value(&answered).expect("a result serializes");
    let members: Vec<&str> =
        written.as_object().expect("a result is an object").keys().map(String::as_str).collect();
    assert_eq!(members, vec!["changed_property_key_count", "persistent_identifier"]);
}

#[test]
fn a_secret_in_an_assignment_never_reaches_a_rendered_result_or_refusal() {
    let carrying: UpdateOpenServiceGatewayInitiativeConfigurationCommand = serde_json::from_str(
        &format!(
            r#"{{"assignments":{{"password":{{"type":"string","cardinality":"scalar","value":"{SENTINEL}"}}}},"persistent_identifier":"com.example.service.Configuration"}}"#
        ),
    )
    .expect("a legal command");
    assert_eq!(carrying.require_usable(), Ok(()));
    let answered = UpdateOpenServiceGatewayInitiativeConfigurationResult {
        changed_property_key_count: 1,
        persistent_identifier: carrying.persistent_identifier.clone(),
    };
    let rendered = serde_json::to_string(&answered).expect("a result serializes");
    assert!(!rendered.contains(SENTINEL), "the result echoed an assigned value");
    let refusal = UpdateOpenServiceGatewayInitiativeConfigurationRefusal {
        failure:
            UpdateOpenServiceGatewayInitiativeConfigurationFailure::ConfigurationValueMalformed,
        persistent_identifier: carrying.persistent_identifier.clone(),
    };
    let rendered = serde_json::to_string(&refusal).expect("a refusal serializes");
    assert!(!rendered.contains(SENTINEL), "the refusal echoed an assigned value");
}

#[test]
fn a_changed_key_count_is_accepted_at_its_bound_and_refused_one_past_it() {
    let asked = command();
    let bound = maximum_inspected_configuration_properties();
    let exact = UpdateOpenServiceGatewayInitiativeConfigurationResult {
        changed_property_key_count: bound,
        persistent_identifier: identifier("com.example.service.Configuration"),
    };
    assert_eq!(exact.require_answers(&asked), Ok(()));
    let beyond = UpdateOpenServiceGatewayInitiativeConfigurationResult {
        changed_property_key_count: bound + 1,
        persistent_identifier: identifier("com.example.service.Configuration"),
    };
    assert_eq!(beyond.require_answers(&asked), Err(ConfigurationUpdateFailure::TooManyKeys));
}

#[test]
fn a_result_answers_only_the_request_that_named_its_configuration() {
    let elsewhere = UpdateOpenServiceGatewayInitiativeConfigurationResult {
        changed_property_key_count: 1,
        persistent_identifier: identifier("com.example.other"),
    };
    assert_eq!(
        elsewhere.require_answers(&command()),
        Err(ConfigurationUpdateFailure::NotThisRequest)
    );
}

#[test]
fn every_failure_document_carries_its_members_and_says_what_it_proves() {
    let vectors = rows(FAILURES);
    assert_eq!(vectors.len(), 7, "one for each category this command allows");
    for row in &vectors {
        let note = text(row, "note");
        let document = text(row, "document");
        let refusal: UpdateOpenServiceGatewayInitiativeConfigurationRefusal =
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
        serde_json::from_str::<UpdateOpenServiceGatewayInitiativeConfigurationRefusal>(r#"{"failure":"configuration_lookup_failed","persistent_identifier":"com.example.service.Configuration","extra":1}"#).is_err(),
        "a surplus member was accepted"
    );
}
