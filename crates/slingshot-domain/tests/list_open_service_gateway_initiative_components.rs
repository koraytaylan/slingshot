//! Assertions for listing declarative service components.
//!
//! A bundle can be active while a component inside it is unsatisfied, which is
//! why this listing exists at all. Each row names the bundle that declares the
//! component, so a caller who finds an unsatisfied one knows where to look next
//! without a second request.

use serde_json::Value;
use slingshot_domain::command::inspect_open_service_gateway_initiative_configuration::OpenServiceGatewayInitiativePersistentIdentifier;
use slingshot_domain::command::list_open_service_gateway_initiative_components::{
    ComponentMatch, ListOpenServiceGatewayInitiativeComponentsCommand,
    ListOpenServiceGatewayInitiativeComponentsResult,
};
use slingshot_domain::command::operational_listing::ListingResultFailure;
use slingshot_domain::command::platform_service_identity::{
    BundleSymbolicName, ComponentState, DeclarativeServiceComponentName, RequestedComponentStates,
};

/// Commands this test reads.
const COMMANDS: &str = include_str!(
    "fixtures/commands/list_open_service_gateway_initiative_components/commands.jsonl"
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

/// Returns one match.
fn matched(name: &str, state: ComponentState, configured: bool) -> ComponentMatch {
    ComponentMatch {
        bundle_symbolic_name: BundleSymbolicName::parse("com.example.bundle")
            .expect("a legal symbolic name"),
        name: DeclarativeServiceComponentName::parse(name).expect("a legal component name"),
        service_persistent_identifier: configured.then(|| {
            OpenServiceGatewayInitiativePersistentIdentifier::new(name).expect("a legal identifier")
        }),
        state,
    }
}

/// Returns one request over a prefix and a state set.
fn command(
    prefix: Option<&str>,
    states: Option<Vec<ComponentState>>,
) -> ListOpenServiceGatewayInitiativeComponentsCommand {
    ListOpenServiceGatewayInitiativeComponentsCommand {
        name_prefix: prefix
            .map(|prefix| DeclarativeServiceComponentName::parse(prefix).expect("a legal prefix")),
        result_window: None,
        states: states.map(|states| RequestedComponentStates::new(states).expect("a legal set")),
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
            serde_json::from_str::<ListOpenServiceGatewayInitiativeComponentsCommand>(document),
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
fn rows_are_strictly_ascending_by_component_name() {
    assert!(
        ListOpenServiceGatewayInitiativeComponentsResult::new(
            vec![
                matched("com.example.A", ComponentState::Active, true),
                matched("com.example.B", ComponentState::Unsatisfied, false),
            ],
            None
        )
        .is_ok()
    );
    assert_eq!(
        ListOpenServiceGatewayInitiativeComponentsResult::new(
            vec![
                matched("com.example.B", ComponentState::Active, true),
                matched("com.example.A", ComponentState::Active, true),
            ],
            None
        ),
        Err(ListingResultFailure::NotStrictlyAscending)
    );
}

#[test]
fn an_absent_service_identifier_is_omitted_rather_than_nulled() {
    let unconfigured = matched("com.example.A", ComponentState::Unsatisfied, false);
    let written = serde_json::to_string(&unconfigured).expect("a match serializes");
    assert!(
        !written.contains("service_persistent_identifier"),
        "an absent service identifier was serialized"
    );
    let read: ComponentMatch = serde_json::from_str(&written).expect("a match parses");
    assert_eq!(read, unconfigured);
}

#[test]
fn a_match_outside_the_requested_prefix_or_states_is_refused() {
    let page = ListOpenServiceGatewayInitiativeComponentsResult::new(
        vec![matched("com.other.A", ComponentState::Active, false)],
        None,
    )
    .expect("a legal page");
    assert_eq!(page.require_answers(&command(None, None)), Ok(()));
    assert_eq!(
        page.require_answers(&command(Some("com.example"), None)),
        Err(ListingResultFailure::NotThisRequest)
    );
    assert_eq!(
        page.require_answers(&command(None, Some(vec![ComponentState::Unsatisfied]))),
        Err(ListingResultFailure::NotThisRequest)
    );
}
