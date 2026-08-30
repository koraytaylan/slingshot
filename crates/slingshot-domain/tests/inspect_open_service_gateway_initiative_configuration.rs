//! Inspecting one configuration, proved side-effect free and proved quiet.
//!
//! Two properties carry the weight. The first is that the lookup cannot create
//! what it is asking about: no trace contains `getConfiguration`,
//! `getFactoryConfiguration`, or any other call that binds a configuration into
//! existence.
//!
//! The second is that the decision to read a value happens before the value
//! exists in this process. A password attribute, absent evidence, or a name
//! that reads like a secret all answer "do not read", and the trace for such a
//! property contains no `get` at all. That ordering is why a malformed or
//! oversized value under a sensitive name is reported as redacted rather than
//! as malformed - nothing here ever learned what it was.

use std::collections::BTreeMap;

use serde_json::Value;
use slingshot_domain::command::inspect_open_service_gateway_initiative_configuration::{
    ConfigurationCardinality, ConfigurationFailure, ConfigurationRefusal, DECLARED_CARDINALITIES,
    DECLARED_SCALAR_TYPES, InspectOpenServiceGatewayInitiativeConfigurationCommand,
    InspectOpenServiceGatewayInitiativeConfigurationResult, MetatypeEvidence,
    ObservedConfigurationProperty, OpenServiceGatewayInitiativeConfigurationPropertyKey,
    OpenServiceGatewayInitiativeConfigurationScalar,
    OpenServiceGatewayInitiativeConfigurationValue,
    OpenServiceGatewayInitiativePersistentIdentifier, PropertyObservation, SENSITIVE_NAME_LITERALS,
    maximum_configuration_lookup_filter_bytes, maximum_configuration_lookup_matches,
    maximum_configuration_persistent_identifier_bytes, maximum_inspected_configuration_properties,
};

/// Identifiers this test reads.
const IDENTIFIERS: &str = include_str!(
    "fixtures/commands/inspect_open_service_gateway_initiative_configuration/identifiers.jsonl"
);

/// Values this test reads.
const VALUES: &str = include_str!(
    "fixtures/commands/inspect_open_service_gateway_initiative_configuration/values.jsonl"
);

/// Observations this test reads.
const OBSERVATIONS: &str = include_str!(
    "fixtures/commands/inspect_open_service_gateway_initiative_configuration/observations.jsonl"
);

/// Names this test reads.
const NAMES: &str = include_str!(
    "fixtures/commands/inspect_open_service_gateway_initiative_configuration/sensitive-names.jsonl"
);

/// Call traces this test reads.
const LOOKUP: &str = include_str!(
    "fixtures/commands/inspect_open_service_gateway_initiative_configuration/lookup.jsonl"
);

/// Failures this test reads.
const FAILURES: &str = include_str!(
    "fixtures/commands/inspect_open_service_gateway_initiative_configuration/failures.jsonl"
);

/// Every refusal the fixtures can name, beside the variant that produces it.
const DECLARED_REFUSALS: &[(&str, ConfigurationFailure)] = &[
    ("IdentifierOutOfBounds", ConfigurationFailure::IdentifierOutOfBounds),
    ("FilterTooLong", ConfigurationFailure::FilterTooLong),
    ("KeyOutOfBounds", ConfigurationFailure::KeyOutOfBounds),
    ("DuplicateKey", ConfigurationFailure::DuplicateKey),
    ("UnknownScalarType", ConfigurationFailure::UnknownScalarType),
    ("UnknownCardinality", ConfigurationFailure::UnknownCardinality),
    ("TypeMismatch", ConfigurationFailure::TypeMismatch),
    ("NotOneScalar", ConfigurationFailure::NotOneScalar),
    ("IntegerOutOfRange", ConfigurationFailure::IntegerOutOfRange),
    ("NotBitString", ConfigurationFailure::NotBitString),
    ("StringTooLong", ConfigurationFailure::StringTooLong),
    ("SequenceTooManyItems", ConfigurationFailure::SequenceTooManyItems),
    ("SequenceTooLong", ConfigurationFailure::SequenceTooLong),
    ("MixedTypes", ConfigurationFailure::MixedTypes),
    ("CarrierTypeMismatch", ConfigurationFailure::CarrierTypeMismatch),
    ("EvidenceDoesNotPermitVisibility", ConfigurationFailure::EvidenceDoesNotPermitVisibility),
    ("TooManyProperties", ConfigurationFailure::TooManyProperties),
    ("AbsentWithProperties", ConfigurationFailure::AbsentWithProperties),
    ("NotThisRequest", ConfigurationFailure::NotThisRequest),
];

/// Name the fixtures give to the refusals the closed object makes on its own.
const CLOSED_OBJECT: &str = "ClosedObject";

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

/// Returns the rendering the named refusal produces.
fn refusal_rendering(reason: &str) -> Option<String> {
    if reason == CLOSED_OBJECT {
        return None;
    }
    DECLARED_REFUSALS
        .iter()
        .find(|(name, _)| *name == reason)
        .map(|(_, failure)| failure.to_string())
        .or_else(|| panic!("the fixture names a refusal this test does not know: {reason}"))
}

/// Returns every sentence a configuration refusal can render as.
fn every_refusal_rendering() -> Vec<String> {
    DECLARED_REFUSALS.iter().map(|(_, failure)| failure.to_string()).collect()
}

/// Checks one accept-or-refuse vector against the type it names.
fn check<Parsed: serde::de::DeserializeOwned + std::fmt::Debug>(row: &Value) {
    let document = text(row, "document");
    let note = text(row, "note");
    match (row["accepted"].as_bool(), serde_json::from_str::<Parsed>(document)) {
        (Some(true), Ok(_)) => (),
        (Some(false), Err(failure)) => {
            let rendered = failure.to_string();
            match refusal_rendering(text(row, "reason")) {
                Some(expected) => assert!(
                    rendered.contains(&expected),
                    "{note}: refused as {rendered}, not as {expected}"
                ),
                None => assert!(
                    !every_refusal_rendering().contains(&rendered),
                    "{note}: the closed object itself refuses this: {rendered}"
                ),
            }
        }
        (Some(true), Err(failure)) => panic!("{note}: refused as {failure}"),
        (Some(false), Ok(value)) => panic!("{note}: accepted as {value:?}"),
        (None, _) => panic!("{note}: the fixture states whether it is accepted"),
    }
}

#[test]
fn every_identifier_produces_exactly_the_filter_the_fixture_pins() {
    let vectors = rows(IDENTIFIERS);
    assert!(vectors.len() >= 15, "every escape and both bounds are covered");
    for row in &vectors {
        let spelling = text(row, "spelling");
        let note = text(row, "note");
        let built = OpenServiceGatewayInitiativePersistentIdentifier::new(spelling);
        match (row["accepted"].as_bool(), built) {
            (Some(true), Ok(identifier)) => {
                assert_eq!(identifier.as_text(), spelling, "{note}: the spelling was rewritten");
                assert_eq!(
                    identifier.lookup_filter().as_deref(),
                    Ok(text(row, "filter")),
                    "{note}"
                );
            }
            (Some(false), Err(failure)) => assert_eq!(
                Some(failure.to_string()),
                refusal_rendering(text(row, "reason")),
                "{note}"
            ),
            (_, built) => panic!("{note}: the identifier answered {built:?}"),
        }
    }
}

#[test]
fn a_filter_cannot_be_closed_early_by_the_identifier_inside_it() {
    let hostile = OpenServiceGatewayInitiativePersistentIdentifier::new("a)(objectClass=*")
        .expect("a legal identifier");
    let filter = hostile.lookup_filter().expect("a bounded filter");
    assert_eq!(filter, r"(service.pid=a\)\(objectClass=\*)");
    let unescaped = filter.matches(')').count() - filter.matches(r"\)").count();
    assert_eq!(unescaped, 1, "exactly one closing parenthesis is the filter's own");
    let unescaped = filter.matches('(').count() - filter.matches(r"\(").count();
    assert_eq!(unescaped, 1, "and exactly one opening parenthesis is too");
    assert!(!filter.contains("=*)"), "no wildcard reaches the filter as a wildcard");
}

#[test]
fn the_longest_identifier_still_escapes_inside_the_filter_bound() {
    let bound = usize::try_from(maximum_configuration_persistent_identifier_bytes())
        .expect("the bound is addressable");
    let worst = OpenServiceGatewayInitiativePersistentIdentifier::new("\\".repeat(bound))
        .expect("an identifier at its bound");
    let filter = worst.lookup_filter().expect("the worst expansion still fits");
    assert!(
        u64::try_from(filter.len()).expect("addressable")
            <= maximum_configuration_lookup_filter_bytes(),
        "the filter bound accommodates the largest expansion the identifier bound allows"
    );
}

#[test]
fn every_value_vector_lands_where_the_fixture_says_it_does() {
    let vectors = rows(VALUES);
    assert!(vectors.len() >= 50, "every class and every carrier is proved at its edges");
    for row in &vectors {
        check::<OpenServiceGatewayInitiativeConfigurationValue>(row);
    }
}

#[test]
fn every_accepted_value_writes_itself_back_byte_for_byte() {
    for row in rows(VALUES).iter().filter(|row| row["accepted"] == Value::Bool(true)) {
        let document = text(row, "document");
        let value: OpenServiceGatewayInitiativeConfigurationValue =
            serde_json::from_str(document).expect("the fixture says this is accepted");
        assert_eq!(
            serde_json::to_string(&value).expect("a valid value serializes"),
            document,
            "{}: rewritten differently",
            text(row, "note")
        );
    }
}

#[test]
fn every_declared_class_and_carrier_has_an_accepted_vector() {
    let accepted: Vec<OpenServiceGatewayInitiativeConfigurationValue> = rows(VALUES)
        .iter()
        .filter(|row| row["accepted"] == Value::Bool(true))
        .map(|row| {
            serde_json::from_str(text(row, "document")).expect("the fixture says this is accepted")
        })
        .collect();
    for type_name in DECLARED_SCALAR_TYPES {
        assert!(
            accepted.iter().any(|value| value.type_name() == *type_name),
            "{type_name} has no accepted vector"
        );
    }
    for carrier in DECLARED_CARDINALITIES {
        assert!(
            accepted.iter().any(|value| value.cardinality().as_text() == *carrier),
            "{carrier} has no accepted vector"
        );
    }
    assert_eq!(DECLARED_SCALAR_TYPES.len(), 9, "nine classes, and no tenth");
    assert_eq!(DECLARED_CARDINALITIES.len(), 4, "four carriers, and no fifth");
}

#[test]
fn nothing_widens_and_nothing_is_inferred() {
    let one = |type_name: &str| {
        OpenServiceGatewayInitiativeConfigurationValue::new(
            type_name,
            ConfigurationCardinality::Scalar,
            vec![OpenServiceGatewayInitiativeConfigurationScalar::Integer("1".to_owned())],
        )
        .expect("a legal integer value")
    };
    assert_ne!(one("byte"), one("integer"), "a Byte holding one is not an Integer holding one");
    assert_ne!(
        serde_json::to_string(&one("byte")).expect("serializes"),
        serde_json::to_string(&one("long")).expect("serializes"),
        "and the wire keeps them apart"
    );
    let carried = |carrier| {
        OpenServiceGatewayInitiativeConfigurationValue::new(
            "long",
            carrier,
            vec![OpenServiceGatewayInitiativeConfigurationScalar::Integer("1".to_owned())],
        )
        .expect("a legal sequence")
    };
    assert_ne!(
        carried(ConfigurationCardinality::PrimitiveArray),
        carried(ConfigurationCardinality::Collection),
        "an array and a collection are two carriers, not one"
    );
}

#[test]
fn every_observation_vector_lands_where_the_fixture_says_it_does() {
    let vectors = rows(OBSERVATIONS);
    assert!(vectors.len() >= 12, "each evidence is paired with each visibility");
    for row in &vectors {
        check::<ObservedConfigurationProperty>(row);
    }
}

#[test]
fn only_exact_non_password_evidence_ever_makes_a_value_visible() {
    let value = OpenServiceGatewayInitiativeConfigurationValue::new(
        "string",
        ConfigurationCardinality::Scalar,
        vec![OpenServiceGatewayInitiativeConfigurationScalar::Text("ordinary".to_owned())],
    )
    .expect("a legal value");
    for evidence in [MetatypeEvidence::Password, MetatypeEvidence::Unavailable] {
        assert_eq!(
            ObservedConfigurationProperty::visible(evidence, value.clone()),
            Err(ConfigurationFailure::EvidenceDoesNotPermitVisibility),
            "{evidence:?} cannot make a value visible"
        );
        assert!(!evidence.permits_reading());
    }
    assert!(MetatypeEvidence::NonPassword.permits_reading());
    assert!(ObservedConfigurationProperty::visible(MetatypeEvidence::NonPassword, value).is_ok());
}

#[test]
fn a_redaction_carries_its_verdict_and_nothing_that_could_leak_the_value() {
    for evidence in
        [MetatypeEvidence::Password, MetatypeEvidence::Unavailable, MetatypeEvidence::NonPassword]
    {
        let redacted = ObservedConfigurationProperty::redacted(evidence);
        let written = serde_json::to_string(&redacted).expect("a redaction serializes");
        assert!(written.ends_with(r#""observation":{"visibility":"redacted"}}"#), "{written}");
        let observed: Value = serde_json::from_str(&written).expect("one object");
        let members: Vec<&String> = observed["observation"]
            .as_object()
            .expect("the observation is one object")
            .keys()
            .collect();
        assert_eq!(
            members,
            vec!["visibility"],
            "a redaction carries no value, type, carrier, length, digest, or hint"
        );
        assert_eq!(redacted.observation(), &PropertyObservation::Redacted);
    }
}

#[test]
fn the_decision_to_read_is_made_from_the_key_and_the_evidence_alone() {
    let ordinary = OpenServiceGatewayInitiativeConfigurationPropertyKey::new("connection.timeout")
        .expect("a legal key");
    let sensitive = OpenServiceGatewayInitiativeConfigurationPropertyKey::new("adobe.privateKey")
        .expect("a legal key");
    assert_eq!(
        ObservedConfigurationProperty::decide_before_reading(
            &ordinary,
            MetatypeEvidence::NonPassword
        ),
        Some(MetatypeEvidence::NonPassword),
        "an ordinary attribute under an ordinary name is read"
    );
    for evidence in [MetatypeEvidence::Password, MetatypeEvidence::Unavailable] {
        assert_eq!(
            ObservedConfigurationProperty::decide_before_reading(&ordinary, evidence),
            None,
            "{evidence:?} withholds even an ordinary name"
        );
    }
    assert_eq!(
        ObservedConfigurationProperty::decide_before_reading(
            &sensitive,
            MetatypeEvidence::NonPassword
        ),
        None,
        "and an ordinary attribute under a sensitive name is withheld too"
    );
}

#[test]
fn every_name_vector_is_classified_the_way_the_fixture_says() {
    let vectors = rows(NAMES);
    assert!(vectors.len() >= 25, "every literal and its spellings are covered");
    for row in &vectors {
        let spelling = text(row, "spelling");
        let sensitive = row["sensitive"].as_bool().expect("every vector states its verdict");
        let note = text(row, "note");
        match OpenServiceGatewayInitiativeConfigurationPropertyKey::new(spelling) {
            Ok(key) => assert_eq!(key.reads_as_sensitive(), sensitive, "{note}"),
            Err(failure) => {
                assert!(spelling.is_empty(), "{note}: refused as {failure}");
                assert!(sensitive, "{note}: an unnameable key is never read under");
            }
        }
    }
    for literal in SENSITIVE_NAME_LITERALS {
        let key = OpenServiceGatewayInitiativeConfigurationPropertyKey::new(*literal)
            .expect("every literal is itself a legal key");
        assert!(key.reads_as_sensitive(), "{literal} is not treated as sensitive");
    }
}

#[test]
fn a_key_keeps_its_case_while_two_spellings_of_one_key_are_one_key() {
    let mixed = OpenServiceGatewayInitiativeConfigurationPropertyKey::new("Connection.Timeout")
        .expect("a legal key");
    assert_eq!(mixed.as_text(), "Connection.Timeout", "the original case survives");
    let lower = OpenServiceGatewayInitiativeConfigurationPropertyKey::new("connection.timeout")
        .expect("a legal key");
    assert_ne!(mixed, lower, "they are two keys");
    assert_eq!(mixed.folded_identity(), lower.folded_identity(), "with one identity");

    let decomposed = OpenServiceGatewayInitiativeConfigurationPropertyKey::new("cafe\u{301}")
        .expect("a legal key");
    let composed = OpenServiceGatewayInitiativeConfigurationPropertyKey::new("caf\u{e9}")
        .expect("a legal key");
    assert_ne!(decomposed, composed, "two spellings");
    assert_eq!(decomposed.folded_identity(), composed.folded_identity(), "one identity");

    let sharp = OpenServiceGatewayInitiativeConfigurationPropertyKey::new("stra\u{df}e")
        .expect("a legal key");
    let expanded =
        OpenServiceGatewayInitiativeConfigurationPropertyKey::new("strasse").expect("a legal key");
    assert_ne!(
        sharp.folded_identity(),
        expanded.folded_identity(),
        "the documented divergence: full case folding would join these two and this \
         implementation separates them, which accepts a pair the specification refuses \
         and never the other way around"
    );
}

#[test]
fn no_trace_creates_binds_or_mutates_a_configuration() {
    let vectors = rows(LOOKUP);
    let traces: Vec<&Value> = vectors.iter().filter(|row| text(row, "kind") == "trace").collect();
    assert!(traces.len() >= 14, "every lookup outcome is traced");
    let forbidden: Vec<String> = vectors
        .iter()
        .find(|row| text(row, "kind") == "forbidden")
        .expect("the forbidden inventory is present")["calls"]
        .as_array()
        .expect("a list of calls")
        .iter()
        .map(|call| call.as_str().expect("a call name").to_owned())
        .collect();
    assert!(forbidden.contains(&"getConfiguration".to_owned()));
    assert!(forbidden.contains(&"getFactoryConfiguration".to_owned()));
    assert!(forbidden.contains(&"getProcessedProperties".to_owned()));
    for row in &traces {
        let calls: Vec<&str> = row["calls"]
            .as_array()
            .expect("every trace lists its calls")
            .iter()
            .map(|call| call.as_str().expect("a call name"))
            .collect();
        let note = text(row, "note");
        for refused in &forbidden {
            assert!(!calls.contains(&refused.as_str()), "{note}: calls {refused}");
        }
        assert!(
            calls.iter().filter(|call| **call == "keys").count() <= 1,
            "{note}: the keys are enumerated at most once"
        );
        assert!(
            calls.iter().filter(|call| **call == "getProperties").count() <= 1,
            "{note}: the property dictionary is acquired at most once"
        );
    }
}

#[test]
fn a_redacted_property_is_never_fetched_and_a_visible_one_is_fetched_once() {
    for row in rows(LOOKUP).iter().filter(|row| text(row, "kind") == "trace") {
        let calls: Vec<&str> = row["calls"]
            .as_array()
            .expect("every trace lists its calls")
            .iter()
            .map(|call| call.as_str().expect("a call name"))
            .collect();
        let note = text(row, "note");
        let fetches = calls.iter().filter(|call| **call == "get").count();
        if note.contains("never fetched") {
            assert_eq!(fetches, 0, "{note}: a redacted property was fetched");
        }
        if note.contains("read exactly once") {
            assert_eq!(fetches, 1, "{note}: a visible property was not fetched exactly once");
        }
    }
}

#[test]
fn every_failure_is_the_closed_shape_the_contract_declares() {
    let vectors = rows(FAILURES);
    assert_eq!(vectors.len(), 19, "three fieldless, and every budget and reason literal");
    for row in &vectors {
        let document = text(row, "document");
        let note = text(row, "note");
        let refusal: ConfigurationRefusal =
            serde_json::from_str(document).expect("every failure vector is a legal failure");
        assert_eq!(
            serde_json::to_string(&refusal).expect("a failure serializes"),
            document,
            "{note}: rewritten differently"
        );
        let members: Vec<String> = serde_json::from_str::<Value>(document)
            .expect("one object")
            .as_object()
            .expect("an object")
            .keys()
            .cloned()
            .collect();
        let mut declared: Vec<String> = row["members"]
            .as_array()
            .expect("every vector states its members")
            .iter()
            .map(|member| member.as_str().expect("a member name").to_owned())
            .collect();
        declared.sort();
        assert_eq!(members, declared, "{note}: carries other members");
        for channel in ["key", "property", "value", "filter", "persistent_identifier"] {
            assert!(!members.iter().any(|member| member == channel), "{note}: names a {channel}");
        }
    }
}

#[test]
fn a_result_reports_what_it_found_and_correlates_with_its_own_request() {
    let asked = InspectOpenServiceGatewayInitiativeConfigurationCommand {
        persistent_identifier: OpenServiceGatewayInitiativePersistentIdentifier::new(
            "com.example.Service",
        )
        .expect("a legal identifier"),
    };
    let other = OpenServiceGatewayInitiativePersistentIdentifier::new("com.example.Other")
        .expect("a legal identifier");

    let absent = InspectOpenServiceGatewayInitiativeConfigurationResult::new(
        asked.persistent_identifier.clone(),
        false,
        BTreeMap::new(),
    )
    .expect("an absent configuration");
    assert_eq!(absent.require_answers(&asked), Ok(()));
    assert_eq!(
        serde_json::to_string(&absent).expect("a result serializes"),
        r#"{"persistent_identifier":"com.example.Service","present":false,"properties":{}}"#
    );

    let mut properties = BTreeMap::new();
    properties.insert(
        "adobe.privateKey".to_owned(),
        ObservedConfigurationProperty::redacted(MetatypeEvidence::Password),
    );
    assert_eq!(
        InspectOpenServiceGatewayInitiativeConfigurationResult::new(
            asked.persistent_identifier.clone(),
            false,
            properties.clone()
        ),
        Err(ConfigurationFailure::AbsentWithProperties),
        "an absent configuration reports nothing"
    );

    let present =
        InspectOpenServiceGatewayInitiativeConfigurationResult::new(other, true, properties)
            .expect("a present configuration");
    assert_eq!(
        present.require_answers(&asked),
        Err(ConfigurationFailure::NotThisRequest),
        "another configuration's observations are refused before they can be kept"
    );
}

#[test]
fn every_named_bound_comes_from_the_manifest_rather_than_from_here() {
    let contract = slingshot_domain::command::command_identity::CommandContract::embedded();
    assert_eq!(
        maximum_configuration_persistent_identifier_bytes(),
        contract.limit("maximum_configuration_persistent_identifier_bytes")
    );
    assert_eq!(
        maximum_configuration_lookup_filter_bytes(),
        contract.limit("maximum_configuration_lookup_filter_bytes")
    );
    assert_eq!(
        maximum_configuration_lookup_matches(),
        contract.limit("maximum_configuration_lookup_matches")
    );
    assert_eq!(
        maximum_inspected_configuration_properties(),
        contract.limit("maximum_inspected_configuration_properties")
    );
    for row in rows(LOOKUP).iter().filter(|row| text(row, "kind") == "bound") {
        if let Some(matches) = row["matches"].as_u64() {
            assert_eq!(matches, maximum_configuration_lookup_matches());
        }
        if let Some(properties) = row["properties"].as_u64() {
            assert_eq!(properties, maximum_inspected_configuration_properties());
        }
    }
}
