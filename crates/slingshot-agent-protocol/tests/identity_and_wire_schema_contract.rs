//! What crosses the wire, and the order it is checked in.
//!
//! The ordering claim is the one worth testing, because getting it wrong
//! produces a system that accepts documents it meant to refuse. A schema cannot
//! see whether the octets it was handed are the canonical spelling of the value
//! they parse to - two byte strings can parse to one value, and only one is the
//! one this system agreed on. So bytes are checked first, and by the time typed
//! code runs, nothing remembers the bytes.

use serde::Serialize as _;
use serde_json::Value;
use slingshot_agent_protocol::identity::{
    AGENT_FORMAT, DocumentProvenance, WireContractIdentity, WireOperationIdentity,
};
use slingshot_agent_protocol::wire_contract::{
    ExpectedProvenance, VALIDATION_ORDER, ValidationStage, WireRefusal,
};
use slingshot_domain::agent_identity::AgentEventStoreGeneration;
use slingshot_domain::command::schema::canonical_contract_digest;
use slingshot_domain::selected_command_contract_identity::SelectedCommandContractIdentity;

/// The generated schema manifest this test reads.
const MANIFEST: &str = include_str!("fixtures/identity-and-wire-schema/manifest.json");

/// Two-character pairs in a sixty-four-character hexadecimal value.
const DIGEST_PAIRS: usize = 32;

/// Directories between this crate's manifest and the workspace root.
const WORKSPACE_ROOT_ANCESTORS: usize = 2;

/// Documents the generated agent-schema families hold together.
const GENERATED_SCHEMAS: usize = 5;

/// The generation an agent event store reaches once it has been rebuilt.
const REBUILT_GENERATION: u64 = 2;

/// The command these fixtures submit.
const COMMAND: &str = "query_paths";

/// The target these fixtures serve.
const TARGET: &str = "1d";

/// The environment revision these fixtures run at.
const REVISION: &str = "revision-1";

/// Returns a sixty-four-character value made of one repeated pair.
fn digest(pair: &str) -> String {
    pair.repeat(DIGEST_PAIRS)
}

/// Returns what this build expects every document to say.
fn expected() -> ExpectedProvenance {
    ExpectedProvenance {
        canonical_json_contract_digest: canonical_contract_digest(),
        command_contract: SelectedCommandContractIdentity::installed(COMMAND)
            .expect("an installed command"),
        transport_contract_digest: digest("70"),
    }
}

/// Returns the canonical spelling of one value.
///
/// Compared against the committed bytes, which is exact. A scan for
/// insignificant whitespace would find the colon in a description string and
/// report a file that is perfectly canonical.
fn recanonicalized(value: &Value) -> String {
    let mut written = Vec::new();
    let mut serializer =
        serde_json::Serializer::with_formatter(&mut written, serde_json::ser::CompactFormatter);
    sorted(value).serialize(&mut serializer).expect("a schema serializes");
    String::from_utf8(written).expect("a schema is text") + "\n"
}

/// Returns `value` with every object key in byte order.
fn sorted(value: &Value) -> Value {
    match value {
        Value::Object(members) => Value::Object(
            members
                .iter()
                .map(|(name, held)| (name.clone(), sorted(held)))
                .collect::<serde_json::Map<String, Value>>(),
        ),
        Value::Array(items) => Value::Array(items.iter().map(sorted).collect()),
        other => other.clone(),
    }
}

/// Returns the repository root.
fn repository_root() -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(WORKSPACE_ROOT_ANCESTORS)
        .expect("the workspace root")
        .to_path_buf()
}

#[test]
fn the_gates_run_in_the_order_that_makes_each_one_possible() {
    assert_eq!(
        VALIDATION_ORDER,
        [
            ValidationStage::RawCanonicalBytes,
            ValidationStage::DecodedShape,
            ValidationStage::TypedConversion,
        ],
        "bytes before shape before types, because each gate cannot do the next one's job"
    );
    assert!(
        ValidationStage::RawCanonicalBytes < ValidationStage::DecodedShape,
        "and the ordering is in the type, not only in a comment"
    );
    assert!(ValidationStage::DecodedShape < ValidationStage::TypedConversion);
}

#[test]
fn a_document_naming_exactly_what_this_build_has_is_accepted() {
    let expected = expected();
    let provenance = expected.provenance();
    expected.require_matching(&provenance).expect("a document this build wrote");
    assert_eq!(provenance.format, AGENT_FORMAT);
    assert_eq!(provenance.canonical_json_contract_digest, canonical_contract_digest());
}

#[test]
fn transport_drift_and_canonical_contract_drift_are_two_different_failures() {
    let expected = expected();
    let transport_drifted =
        DocumentProvenance { transport_contract_digest: digest("71"), ..expected.provenance() };
    assert!(
        matches!(
            expected.require_matching(&transport_drifted),
            Err(WireRefusal::TransportContractDrift { .. })
        ),
        "a disagreement about how to talk"
    );

    let contract_drifted = DocumentProvenance {
        canonical_json_contract_digest: digest("c1"),
        ..expected.provenance()
    };
    assert!(
        matches!(
            expected.require_matching(&contract_drifted),
            Err(WireRefusal::CanonicalContractDrift { .. })
        ),
        "and a disagreement about what a canonical document looks like, which has a different fix"
    );
}

#[test]
fn a_document_naming_another_command_contract_is_refused_whichever_field_differs() {
    let expected = expected();
    let installed = WireContractIdentity::from(&expected.command_contract);
    for changed in [
        WireContractIdentity { argument_schema_digest: digest("ff"), ..installed.clone() },
        WireContractIdentity { result_schema_digest: digest("ff"), ..installed.clone() },
        WireContractIdentity { command_contract_limits_digest: digest("ff"), ..installed.clone() },
        WireContractIdentity {
            command_semantic_contract_version: "2.0.0".to_owned(),
            ..installed.clone()
        },
        WireContractIdentity { command_wire_name: "create_page".to_owned(), ..installed.clone() },
    ] {
        let drifted = DocumentProvenance { command_contract: changed, ..expected.provenance() };
        assert_eq!(
            expected.require_matching(&drifted),
            Err(WireRefusal::CommandContractDrift),
            "all five fields, or the submission is refused"
        );
    }
}

#[test]
fn a_document_written_under_another_format_never_reaches_a_digest_comparison() {
    let expected = expected();
    let foreign = DocumentProvenance {
        format: "somebody.else/1".to_owned(),
        transport_contract_digest: digest("71"),
        canonical_json_contract_digest: digest("c1"),
        ..expected.provenance()
    };
    assert!(
        matches!(expected.require_matching(&foreign), Err(WireRefusal::FormatDrift { .. })),
        "the first thing wrong is what a caller is told, not whichever the code noticed"
    );
}

#[test]
fn the_wire_identity_and_the_domain_identity_are_the_same_five_fields() {
    let installed =
        SelectedCommandContractIdentity::installed(COMMAND).expect("an installed command");
    let crossed = WireContractIdentity::from(&installed);
    let returned = SelectedCommandContractIdentity::from(&crossed);
    assert!(
        installed.is_the_same_contract_as(&returned),
        "crossing the wire and coming back changes nothing about which contract this is"
    );
}

#[test]
fn an_operation_identity_carries_its_partition_and_its_generation() {
    let identity = WireOperationIdentity::of(
        &digest(TARGET),
        REVISION,
        "operation-1",
        AgentEventStoreGeneration::first(),
    );
    assert_eq!(identity.author_target_identity_digest, digest(TARGET));
    assert_eq!(identity.selected_environment_revision, REVISION);
    assert_eq!(identity.agent_event_store_generation, 1);

    let rebuilt = WireOperationIdentity::of(
        &digest(TARGET),
        REVISION,
        "operation-1",
        AgentEventStoreGeneration::of(REBUILT_GENERATION),
    );
    assert_ne!(
        identity.agent_operation_identifier, rebuilt.agent_operation_identifier,
        "a store that was rebuilt does not answer to the names from before it"
    );
}

#[test]
fn every_generated_schema_regenerates_to_the_bytes_the_manifest_records() {
    let manifest: Value = serde_json::from_str(MANIFEST).expect("the manifest is one value");
    assert_eq!(
        manifest["canonical_json_contract_sha256"].as_str(),
        Some(canonical_contract_digest().as_str()),
        "the manifest names the canonical contract this build has"
    );
    let schemas = manifest["schemas"].as_object().expect("a schema list");
    assert!(schemas.len() >= GENERATED_SCHEMAS, "both families, and every document in them");

    let root = repository_root().join("schemas/agent-protocol");
    for (relative, recorded) in schemas {
        let bytes = std::fs::read(root.join(relative)).expect("a generated schema reads");
        let digest: String = <sha2::Sha256 as sha2::Digest>::digest(&bytes)
            .iter()
            .map(|octet| format!("{octet:02x}"))
            .collect();
        assert_eq!(
            Some(digest.as_str()),
            recorded.as_str(),
            "{relative} is not the bytes the manifest records"
        );
        let value: Value = serde_json::from_slice(&bytes).expect("a schema is one value");
        assert_eq!(
            value["$schema"].as_str(),
            Some("https://json-schema.org/draft/2020-12/schema"),
            "{relative} names the dialect this build validates against"
        );
        assert_eq!(
            recanonicalized(&value),
            String::from_utf8(bytes).expect("a schema is text"),
            "{relative} is not the canonical spelling of its own value; a scan for whitespace \
             would have read inside its description strings instead of checking this"
        );
    }
}

#[test]
fn no_generated_schema_claims_to_prove_that_bytes_are_canonical() {
    let root = repository_root().join("schemas/agent-protocol");
    let manifest: Value = serde_json::from_str(MANIFEST).expect("the manifest is one value");
    for relative in manifest["schemas"].as_object().expect("a schema list").keys() {
        let text = std::fs::read_to_string(root.join(relative)).expect("a schema reads");
        assert!(
            !text.contains("canonical bytes") && !text.contains("proves canonical"),
            "{relative} claims something a schema cannot see: whether the octets it was handed \
             are the canonical spelling of the value they parse to"
        );
    }
    let provenance = std::fs::read_to_string(root.join("common/provenance.json"))
        .expect("the provenance schema reads");
    assert!(
        provenance.contains("checked before this schema is consulted"),
        "and the one that names the contract says where canonicality is actually decided"
    );
}
