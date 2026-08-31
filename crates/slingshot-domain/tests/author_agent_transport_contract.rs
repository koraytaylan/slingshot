//! The transport manifest, and the arithmetic that has to hold inside it.
//!
//! The vectors were computed outside this workspace from primitive operands, so
//! agreement is evidence about the numbers rather than the implementation
//! agreeing with itself. Everything else here is about the manifest being the
//! only place a value lives: no fallback, no default, and a refusal when a
//! build asks for something it does not name.

use serde_json::Value;
use slingshot_domain::author_agent_transport_contract::{
    AGENT_PROTOCOL_VERSION, AuthorAgentTransportContract, CONTRACT_FORMAT, TransportContractFailure,
};

/// Arithmetic vectors this test reads.
const FORMULAS: &str = include_str!("fixtures/author-agent-transport-contract/formulas.jsonl");

/// The committed sidecar.
const SIDECAR: &str = include_str!("../../../policy/author-agent-transport-contract-1.sha256");

/// Characters a sixty-four-character hexadecimal value has.
const DIGEST_CHARACTERS: usize = 64;

/// Values the manifest states outright.
const STATED_LIMITS: usize = 66;

/// Values the manifest computes from others.
const COMPUTED_FORMULAS: usize = 6;

/// Returns one row's string member.
fn text<'row>(row: &'row Value, member: &str) -> &'row str {
    row[member].as_str().unwrap_or_else(|| panic!("{member} is a string in {row}"))
}

/// Returns every arithmetic vector.
fn vectors() -> Vec<Value> {
    FORMULAS
        .lines()
        .map(|line| serde_json::from_str(line).expect("every fixture line is one object"))
        .collect()
}

#[test]
fn the_committed_bytes_are_canonical_and_the_sidecar_digests_them() {
    let manifest = AuthorAgentTransportContract::embedded_manifest();
    assert!(manifest.ends_with('\n'), "one final line feed");
    assert!(!manifest.ends_with("\n\n"), "and only one");
    assert!(!manifest.starts_with('\u{feff}'), "no byte-order mark");
    assert!(!manifest.contains(": "), "no insignificant whitespace between a key and its value");
    assert!(!manifest.contains(", "), "nor between members");

    let value: Value = serde_json::from_str(manifest).expect("the manifest is one value");
    let members: Vec<&String> = value.as_object().expect("an object").keys().collect();
    assert_eq!(
        members,
        vec!["agent_protocol_version", "format", "formulas", "limits"],
        "exactly four members, in byte order"
    );
    let mut sorted = members.clone();
    sorted.sort();
    assert_eq!(members, sorted, "and the keys are byte-lexicographically ordered");

    let digest = AuthorAgentTransportContract::embedded_digest();
    assert_eq!(digest.len(), DIGEST_CHARACTERS);
    assert_eq!(SIDECAR.trim(), digest, "the sidecar is the digest of the whole committed bytes");
    assert!(SIDECAR.ends_with('\n'), "with one final line feed of its own");
}

#[test]
fn the_embedded_contract_is_the_committed_one() {
    let contract = AuthorAgentTransportContract::embedded();
    assert_eq!(contract.format, CONTRACT_FORMAT);
    assert_eq!(contract.agent_protocol_version, AGENT_PROTOCOL_VERSION);
    assert_eq!(contract.limits.len(), STATED_LIMITS, "every value the architecture names");
    assert_eq!(contract.formulas.len(), COMPUTED_FORMULAS);
    contract.require_sidecar(SIDECAR).expect("the sidecar matches");
    contract.require_consistent_formulas().expect("every formula follows from its operands");
}

#[test]
fn every_formula_equals_the_value_its_operands_produce() {
    let contract = AuthorAgentTransportContract::embedded();
    let held = vectors();
    assert!(held.len() >= 5, "every formula the manifest computes has a vector");
    for row in &held {
        let name = text(row, "name");
        let recomputed = row["value"].as_u64().expect("a vector states its result");
        let stated = contract
            .formulas
            .get(name)
            .copied()
            .or_else(|| contract.limits.get(name).copied())
            .unwrap_or_else(|| panic!("the manifest names {name}"));
        assert_eq!(stated, recomputed, "{name}: {}", text(row, "note"));
        assert!(
            !row["operands"].as_object().expect("a vector states its operands").is_empty(),
            "{name} names what it was computed from"
        );
    }
}

#[test]
fn a_manifest_whose_formula_does_not_follow_is_refused() {
    let manifest = AuthorAgentTransportContract::embedded_manifest();
    let mut value: Value = serde_json::from_str(manifest).expect("the manifest is one value");
    value["formulas"]["heartbeat_timeout_milliseconds"] = Value::from(1_u64);
    let text = serde_json::to_string(&value).expect("a manifest renders");
    let contract = AuthorAgentTransportContract::parse(&text).expect("it still parses");
    assert!(
        matches!(
            contract.require_consistent_formulas(),
            Err(TransportContractFailure::FormulaInconsistent { ref name })
                if name == "heartbeat_timeout_milliseconds"
        ),
        "a stated result that does not follow from its operands fails here rather than downstream"
    );
}

#[test]
fn a_lease_renewal_that_is_not_well_inside_its_lease_is_refused() {
    let mut value: Value = serde_json::from_str(AuthorAgentTransportContract::embedded_manifest())
        .expect("the manifest is one value");
    let lease = value["limits"]["worker_execution_lease_milliseconds"].as_u64().expect("a lease");
    value["limits"]["worker_execution_lease_renewal_milliseconds"] = Value::from(lease);
    let text = serde_json::to_string(&value).expect("a manifest renders");
    let contract = AuthorAgentTransportContract::parse(&text).expect("it still parses");
    assert!(
        matches!(
            contract.require_consistent_formulas(),
            Err(TransportContractFailure::FormulaInconsistent { ref name })
                if name == "worker_execution_lease_renewal_milliseconds"
        ),
        "renewing exactly when a lease expires is renewing too late to be sure of keeping it"
    );
}

#[test]
fn a_submission_that_does_not_fit_the_bounds_around_it_is_refused() {
    let mut value: Value = serde_json::from_str(AuthorAgentTransportContract::embedded_manifest())
        .expect("the manifest is one value");
    let document = value["limits"]["maximum_agent_protocol_document_bytes"]
        .as_u64()
        .expect("a document bound");
    value["limits"]["maximum_canonical_submission_bytes"] = Value::from(document);
    let text = serde_json::to_string(&value).expect("a manifest renders");
    let contract = AuthorAgentTransportContract::parse(&text).expect("it still parses");
    assert!(
        matches!(
            contract.require_consistent_formulas(),
            Err(TransportContractFailure::FormulaInconsistent { ref name })
                if name == "maximum_canonical_submission_bytes"
        ),
        "a submission exactly as large as the document that has to hold it leaves no room for the \
         document itself"
    );
}

#[test]
fn a_document_naming_another_format_is_not_this_contract() {
    let mut value: Value = serde_json::from_str(AuthorAgentTransportContract::embedded_manifest())
        .expect("the manifest is one value");
    value["format"] = Value::from("somebody.else/1");
    let text = serde_json::to_string(&value).expect("a manifest renders");
    assert!(
        matches!(
            AuthorAgentTransportContract::parse(&text),
            Err(TransportContractFailure::UnsupportedFormat(_))
        ),
        "changing a value requires a new format rather than silently changing this one"
    );
}

#[test]
fn a_manifest_with_a_member_nobody_declared_is_refused() {
    let mut value: Value = serde_json::from_str(AuthorAgentTransportContract::embedded_manifest())
        .expect("the manifest is one value");
    value["something_else"] = Value::from(1_u64);
    let text = serde_json::to_string(&value).expect("a manifest renders");
    assert!(
        matches!(
            AuthorAgentTransportContract::parse(&text),
            Err(TransportContractFailure::Unreadable(_))
        ),
        "an additional member is a disagreement about what this contract is"
    );
}

#[test]
fn a_value_this_build_does_not_name_is_refused_rather_than_defaulted() {
    let contract = AuthorAgentTransportContract::embedded();
    contract
        .require_limit("maximum_author_response_header_bytes")
        .expect("a value the contract names");
    assert!(
        matches!(
            contract.require_limit("a_value_nobody_declared"),
            Err(TransportContractFailure::ValueAbsent { .. })
        ),
        "a build that picked something reasonable would disagree with the far side about what \
         reasonable means, silently"
    );
}

#[test]
fn the_three_header_bounds_are_three_different_things() {
    let contract = AuthorAgentTransportContract::embedded();
    let field = contract.limit("maximum_author_response_header_bytes");
    let count = contract.limit("maximum_author_response_header_count");
    let head = contract.limit("maximum_author_response_head_bytes");

    assert!(field < head, "one field is smaller than the whole head it sits in");
    assert!(
        count.checked_mul(field).is_some_and(|widest| widest > head),
        "and the head bound binds before the count and field bounds together would, which is why \
         there are three of them rather than one derived from the others"
    );
}
