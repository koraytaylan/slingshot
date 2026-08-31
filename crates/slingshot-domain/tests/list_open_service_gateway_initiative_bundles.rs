//! Assertions for listing bundles.
//!
//! The composite order is what these vectors are for. A deployment can hold two
//! versions of one bundle, so ordering by symbolic name alone leaves those two
//! rows in no defined order - and a page resumed at an undefined boundary is a
//! page that silently skips a row.

use serde_json::Value;
use slingshot_domain::command::list_open_service_gateway_initiative_bundles::{
    BundleMatch, ListOpenServiceGatewayInitiativeBundlesCommand,
    ListOpenServiceGatewayInitiativeBundlesResult,
};
use slingshot_domain::command::operational_listing::ListingResultFailure;
use slingshot_domain::command::platform_service_identity::{
    BundleState, BundleSymbolicName, BundleVersion, RequestedBundleStates,
};

/// Commands this test reads.
const COMMANDS: &str =
    include_str!("fixtures/commands/list_open_service_gateway_initiative_bundles/commands.jsonl");

/// The author's own identifier for a bundle in these vectors.
const IDENTIFIER: u64 = 42;

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
fn matched(name: &str, version: &str, state: BundleState) -> BundleMatch {
    BundleMatch {
        bundle_identifier: IDENTIFIER,
        state,
        symbolic_name: BundleSymbolicName::parse(name).expect("a legal name"),
        version: BundleVersion::parse(version).expect("a legal version"),
    }
}

/// Returns one request over a prefix and a state set.
fn command(
    prefix: Option<&str>,
    states: Option<Vec<BundleState>>,
) -> ListOpenServiceGatewayInitiativeBundlesCommand {
    ListOpenServiceGatewayInitiativeBundlesCommand {
        result_window: None,
        states: states.map(|states| RequestedBundleStates::new(states).expect("a legal set")),
        symbolic_name_prefix: prefix
            .map(|prefix| BundleSymbolicName::parse(prefix).expect("a legal prefix")),
    }
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
            serde_json::from_str::<ListOpenServiceGatewayInitiativeBundlesCommand>(document),
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
fn two_versions_of_one_bundle_are_ordered_by_version() {
    let ascending = ListOpenServiceGatewayInitiativeBundlesResult::new(
        vec![
            matched("com.example.bundle", "1.0.0", BundleState::Active),
            matched("com.example.bundle", "2.0.0", BundleState::Resolved),
        ],
        None,
    );
    assert!(ascending.is_ok(), "two versions in order were refused");
    let descending = ListOpenServiceGatewayInitiativeBundlesResult::new(
        vec![
            matched("com.example.bundle", "2.0.0", BundleState::Resolved),
            matched("com.example.bundle", "1.0.0", BundleState::Active),
        ],
        None,
    );
    assert_eq!(descending, Err(ListingResultFailure::NotStrictlyAscending));
    let repeated = ListOpenServiceGatewayInitiativeBundlesResult::new(
        vec![
            matched("com.example.bundle", "1.0.0", BundleState::Active),
            matched("com.example.bundle", "1.0.0", BundleState::Active),
        ],
        None,
    );
    assert_eq!(repeated, Err(ListingResultFailure::NotStrictlyAscending));
}

#[test]
fn a_match_outside_the_requested_prefix_or_states_is_refused() {
    let page = ListOpenServiceGatewayInitiativeBundlesResult::new(
        vec![matched("com.other.bundle", "1.0.0", BundleState::Active)],
        None,
    )
    .expect("a legal page");
    assert_eq!(page.require_answers(&command(None, None)), Ok(()));
    assert_eq!(
        page.require_answers(&command(Some("com.example"), None)),
        Err(ListingResultFailure::NotThisRequest)
    );
    assert_eq!(
        page.require_answers(&command(None, Some(vec![BundleState::Resolved]))),
        Err(ListingResultFailure::NotThisRequest)
    );
    assert_eq!(page.require_answers(&command(None, Some(vec![BundleState::Active]))), Ok(()));
}
