//! Assertions for removing a configuration.
//!
//! Deleting a configuration hands the decision back to whatever default the code
//! carries, so an absent configuration is a failure rather than a success with
//! nothing to do, and the result says whether what was removed was a factory
//! instance - which is what decides whether the deletion removed a thing or
//! reverted one.

use serde_json::Value;
use slingshot_domain::command::delete_open_service_gateway_initiative_configuration::{
    DeleteOpenServiceGatewayInitiativeConfigurationCommand,
    DeleteOpenServiceGatewayInitiativeConfigurationFailure,
    DeleteOpenServiceGatewayInitiativeConfigurationRefusal,
    DeleteOpenServiceGatewayInitiativeConfigurationResult,
};
use slingshot_domain::command::inspect_open_service_gateway_initiative_configuration::OpenServiceGatewayInitiativePersistentIdentifier;
use slingshot_domain::command::update_open_service_gateway_initiative_configuration::ConfigurationUpdateFailure;

/// Commands this test reads.
const COMMANDS: &str = include_str!(
    "fixtures/commands/delete_open_service_gateway_initiative_configuration/commands.jsonl"
);

/// Failures this test reads.
const FAILURES: &str = include_str!(
    "fixtures/commands/delete_open_service_gateway_initiative_configuration/failures.jsonl"
);

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
fn command() -> DeleteOpenServiceGatewayInitiativeConfigurationCommand {
    DeleteOpenServiceGatewayInitiativeConfigurationCommand {
        persistent_identifier: identifier("com.example.service.Configuration"),
    }
}

#[test]
fn every_command_vector_parses_exactly_as_the_fixture_says() {
    let vectors = rows(COMMANDS);
    assert!(vectors.len() >= 3, "every document shape and every refusal");
    for row in &vectors {
        let document = text(row, "document");
        let note = text(row, "note");
        match (
            row["accepted"].as_bool(),
            serde_json::from_str::<DeleteOpenServiceGatewayInitiativeConfigurationCommand>(
                document,
            ),
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
fn an_absent_configuration_is_a_failure_rather_than_a_quiet_success() {
    let refusal = DeleteOpenServiceGatewayInitiativeConfigurationRefusal {
        failure:
            DeleteOpenServiceGatewayInitiativeConfigurationFailure::ConfigurationLookupMismatch,
        persistent_identifier: identifier("com.example.service.Configuration"),
    };
    assert!(refusal.proves_no_effect());
    assert_eq!(refusal.require_answers(&command()), Ok(()));
}

#[test]
fn a_result_says_whether_what_it_removed_was_a_factory_instance() {
    for was_a_factory_instance in [true, false] {
        let answered = DeleteOpenServiceGatewayInitiativeConfigurationResult {
            was_a_factory_instance,
            persistent_identifier: identifier("com.example.service.Configuration"),
        };
        assert_eq!(answered.require_answers(&command()), Ok(()));
        let written = serde_json::to_string(&answered).expect("a result serializes");
        let read: DeleteOpenServiceGatewayInitiativeConfigurationResult =
            serde_json::from_str(&written).expect("a result parses");
        assert_eq!(read, answered);
    }
}

#[test]
fn a_result_answers_only_the_request_that_named_its_configuration() {
    let elsewhere = DeleteOpenServiceGatewayInitiativeConfigurationResult {
        was_a_factory_instance: false,
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
    assert_eq!(vectors.len(), 5, "one for each category this command allows");
    for row in &vectors {
        let note = text(row, "note");
        let document = text(row, "document");
        let refusal: DeleteOpenServiceGatewayInitiativeConfigurationRefusal =
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
        serde_json::from_str::<DeleteOpenServiceGatewayInitiativeConfigurationRefusal>(r#"{"failure":"configuration_lookup_failed","persistent_identifier":"com.example.service.Configuration","extra":1}"#).is_err(),
        "a surplus member was accepted"
    );
}
