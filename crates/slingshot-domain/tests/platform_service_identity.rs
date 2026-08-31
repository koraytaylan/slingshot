//! Assertions for the values the platform and replication families address.

use slingshot_domain::command::command_identity::CommandContract;
use slingshot_domain::command::operational_listing::ListingResultFailure;
use slingshot_domain::command::platform_service_identity::{
    BUNDLE_STATE_COUNT, BundleState, BundleSymbolicName, BundleVersion, COMPONENT_STATE_COUNT,
    ComponentState, DeclarativeServiceComponentName, REPLICATION_ACTION_COUNT,
    REPLICATION_TRANSPORT_KIND_COUNT, ReplicationAction, ReplicationAgentIdentifier,
    ReplicationQueueEntryIdentifier, ReplicationTransportKind, RequestedBundleStates,
    RequestedComponentStates,
};

/// Returns one limit by name.
fn limit(name: &str) -> usize {
    usize::try_from(CommandContract::embedded().limit(name)).expect("the bound fits")
}

#[test]
fn a_symbolic_name_is_full_stop_separated_tokens() {
    for accepted in ["com.example.bundle", "single", "com.example.bundle-core", "a_b.c-d.e1"] {
        assert!(BundleSymbolicName::parse(accepted).is_ok(), "{accepted} was refused");
    }
    for refused in
        ["", ".com.example", "com.example.", "com..example", "com.exa mple", "com/example"]
    {
        assert!(BundleSymbolicName::parse(refused).is_err(), "{refused:?} was accepted");
    }
}

#[test]
fn a_symbolic_name_is_accepted_at_its_bound_and_refused_one_byte_past_it() {
    let bound = limit("maximum_bundle_symbolic_name_bytes");
    let exact = "a".repeat(bound);
    assert!(BundleSymbolicName::parse(&exact).is_ok(), "the bound itself was refused");
    assert!(
        BundleSymbolicName::parse(&format!("{exact}a")).is_err(),
        "one byte past the bound was accepted"
    );
}

#[test]
fn a_version_is_three_numbers_and_an_optional_qualifier() {
    for accepted in ["1.0.0", "0.0.0", "12.34.56", "1.0.0.SNAPSHOT", "1.0.0.build-7"] {
        assert!(BundleVersion::parse(accepted).is_ok(), "{accepted} was refused");
    }
    for refused in ["", "1.0", "1.0.0.0.0", "01.0.0", "1.00.0", "1.0.0.", "1.0.x", "1.0.0.qual.x"] {
        assert!(BundleVersion::parse(refused).is_err(), "{refused:?} was accepted");
    }
}

#[test]
fn a_version_keeps_the_exact_spelling_it_was_given() {
    let version = BundleVersion::parse("2.11.0.R20240101").expect("a legal version");
    assert_eq!(version.as_text(), "2.11.0.R20240101");
    assert_eq!(version.to_string(), "2.11.0.R20240101");
}

#[test]
fn every_opaque_identifier_refuses_controls_and_edge_spaces() {
    for accepted in ["publish", "publish_replication", "flush-agent", "entry:1"] {
        assert!(ReplicationAgentIdentifier::parse(accepted).is_ok(), "{accepted} was refused");
        assert!(ReplicationQueueEntryIdentifier::parse(accepted).is_ok(), "{accepted} was refused");
        assert!(DeclarativeServiceComponentName::parse(accepted).is_ok(), "{accepted} was refused");
    }
    for refused in ["", " publish", "publish ", "pub\u{0}lish"] {
        assert!(ReplicationAgentIdentifier::parse(refused).is_err(), "{refused:?} was accepted");
        assert!(
            ReplicationQueueEntryIdentifier::parse(refused).is_err(),
            "{refused:?} was accepted"
        );
        assert!(
            DeclarativeServiceComponentName::parse(refused).is_err(),
            "{refused:?} was accepted"
        );
    }
}

#[test]
fn every_opaque_identifier_is_accepted_at_its_bound_and_refused_one_byte_past_it() {
    for (bound_name, parse) in [
        (
            "maximum_replication_agent_identifier_bytes",
            (|value: &str| ReplicationAgentIdentifier::parse(value).is_ok()) as fn(&str) -> bool,
        ),
        ("maximum_replication_queue_entry_identifier_bytes", |value| {
            ReplicationQueueEntryIdentifier::parse(value).is_ok()
        }),
        ("maximum_declarative_service_component_name_bytes", |value| {
            DeclarativeServiceComponentName::parse(value).is_ok()
        }),
    ] {
        let exact = "a".repeat(limit(bound_name));
        assert!(parse(&exact), "{bound_name}: the bound itself was refused");
        assert!(!parse(&format!("{exact}a")), "{bound_name}: one byte past the bound was accepted");
    }
}

#[test]
fn every_closed_set_has_exactly_the_members_the_contract_names() {
    assert_eq!(BundleState::every().len(), BUNDLE_STATE_COUNT);
    assert_eq!(ComponentState::every().len(), COMPONENT_STATE_COUNT);
    assert_eq!(ReplicationTransportKind::every().len(), REPLICATION_TRANSPORT_KIND_COUNT);
    assert_eq!(ReplicationAction::every().len(), REPLICATION_ACTION_COUNT);
    assert!(BUNDLE_STATE_COUNT <= limit("maximum_bundle_states"));
    assert!(COMPONENT_STATE_COUNT <= limit("maximum_component_states"));
}

#[test]
fn every_closed_set_is_written_in_the_byte_order_of_its_own_spellings() {
    let spelling = |value: &BundleState| serde_json::to_string(value).expect("a state serializes");
    let written: Vec<String> = BundleState::every().iter().map(spelling).collect();
    let mut ordered = written.clone();
    ordered.sort();
    assert_eq!(written, ordered, "the declared order is not the wire order");
    assert_eq!(spelling(&BundleState::Active), "\"active\"");
    assert!(serde_json::from_str::<BundleState>("\"unknown\"").is_err());
    assert!(serde_json::from_str::<ComponentState>("\"missing\"").is_err());
    assert!(serde_json::from_str::<ReplicationTransportKind>("\"unknown\"").is_err());
    assert!(serde_json::from_str::<ReplicationAction>("\"unknown\"").is_err());
}

#[test]
fn a_requested_state_set_is_nonempty_ascending_and_distinct() {
    let asked = RequestedBundleStates::new(vec![BundleState::Active, BundleState::Resolved])
        .expect("a legal set");
    assert!(asked.contains(BundleState::Active));
    assert!(!asked.contains(BundleState::Starting));
    assert_eq!(asked.states(), [BundleState::Active, BundleState::Resolved]);
    assert_eq!(
        RequestedBundleStates::new(Vec::new()),
        Err(ListingResultFailure::NotAscendingDistinct)
    );
    assert_eq!(
        RequestedBundleStates::new(vec![BundleState::Resolved, BundleState::Active]),
        Err(ListingResultFailure::NotAscendingDistinct)
    );
    assert_eq!(
        RequestedComponentStates::new(vec![ComponentState::Active, ComponentState::Active]),
        Err(ListingResultFailure::NotAscendingDistinct)
    );
}

#[test]
fn a_requested_state_set_round_trips_and_refuses_an_unordered_document() {
    let asked =
        RequestedComponentStates::new(vec![ComponentState::Active, ComponentState::Unsatisfied])
            .expect("a legal set");
    let written = serde_json::to_string(&asked).expect("a set serializes");
    assert_eq!(written, "[\"active\",\"unsatisfied\"]");
    let read: RequestedComponentStates = serde_json::from_str(&written).expect("a set parses");
    assert_eq!(read, asked);
    assert!(
        serde_json::from_str::<RequestedComponentStates>("[\"unsatisfied\",\"active\"]").is_err()
    );
    assert!(serde_json::from_str::<RequestedComponentStates>("[]").is_err());
}
