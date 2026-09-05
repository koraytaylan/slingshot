//! Believing a result, in an order that cannot be shortened.
//!
//! A terminal result is the one document that turns remote work into local
//! truth, so the checks it passes are ordered and none of them is skippable.
//! The order is not stylistic: a bound applied after deserialization is a bound
//! on memory already spent, and a correlation checked after persistence is a
//! correlation checked too late.
//!
//! The check a schema cannot make is the one this suite spends most of its
//! effort on. A result produced by the same command with different arguments
//! satisfies the variant, the shape, and every echoed fact the domain can
//! compare. Only the submitted digest says it is wrong, so the digest is
//! checked, and it is checked before anything is written down.
//!
//! The second subject is secrecy. A configuration value that is read before it
//! is classified has already been read by the time anybody decides it should
//! not have been, so the whole observation is discarded rather than the
//! offending step, and no fixture here lets a classified canary near a value
//! access.

use slingshot_agent_connection::structured_job_result::{
    ArtifactEcho, Classification, DictionaryStep, LocalDisposition, ResultExpectation,
    ResultRefusal, STAGE_ORDER, STRUCTURED_RESULT_MEDIA_TYPE, STRUCTURED_RESULT_SLOT,
    TerminalResultDocument, TraceRefusal, ValidationStage, declared_slots, decode_terminal_result,
    loading_command, local_disposition, maximum_agent_inline_result_bytes, maximum_document_bytes,
    maximum_inline_machine_result_bytes, package_slot_and_media_type, require_declared_artifacts,
    require_two_phase_access, require_valid,
};
use slingshot_agent_protocol::wire_contract::ExpectedProvenance;
use slingshot_domain::author_agent_transport_contract::AuthorAgentTransportContract;
use slingshot_domain::command::artifact::{
    ArtifactRequirement, CONTENT_PACKAGE_MEDIA_TYPE, CONTENT_PACKAGE_SLOT,
    LOADED_CONTENT_MEDIA_TYPE, LOADED_CONTENT_SLOT,
};
use slingshot_domain::command::load_content_as_javascript_object_notation::maximum_agent_inline_loaded_document_bytes;
use slingshot_domain::command::schema::canonical_contract_digest;
use slingshot_domain::selected_command_contract_identity::SelectedCommandContractIdentity;

#[test]
fn published_terminal_envelope_rejects_invalid_shapes_even_if_expectations_match() {
    let mut source = document("query_paths", 0, &[]);
    source.canonical_result = r#"{"matches":[]}"#.to_owned();
    let expected = expectation("query_paths");
    assert!(decode_terminal_result(&serde_json::to_vec(&source).unwrap(), &expected).is_ok());
    for subscription in [String::new(), "x".repeat(97)] {
        let mut changed = source.clone();
        let mut matching = expected.clone();
        changed.daemon_subscription_identifier = subscription.clone();
        matching.daemon_subscription_identifier = subscription;
        assert!(decode_terminal_result(&serde_json::to_vec(&changed).unwrap(), &matching).is_err());
    }
    let mut changed = source.clone();
    let mut matching = expected.clone();
    changed.operation.agent_event_store_generation = 0;
    matching.operation.agent_event_store_generation = 0;
    assert!(decode_terminal_result(&serde_json::to_vec(&changed).unwrap(), &matching).is_err());
    for artifacts in [
        vec![echo(CONTENT_PACKAGE_SLOT, 1), echo(CONTENT_PACKAGE_SLOT, 1)],
        vec![ArtifactEcho { slot: String::new(), ..echo(CONTENT_PACKAGE_SLOT, 1) }],
        vec![ArtifactEcho { media_type: "x".repeat(129), ..echo(CONTENT_PACKAGE_SLOT, 1) }],
        vec![ArtifactEcho { suggested_name: "x".repeat(256), ..echo(CONTENT_PACKAGE_SLOT, 1) }],
    ] {
        let mut changed = source.clone();
        changed.declared_artifacts = artifacts;
        assert!(decode_terminal_result(&serde_json::to_vec(&changed).unwrap(), &expected).is_err());
    }
}

#[test]
fn command_result_gate_checks_schema_order_types_and_retained_request() {
    use slingshot_agent_connection::structured_job_result::decode_result_for_command;
    use slingshot_domain::command::catalog::Command;
    let command: Command = serde_json::from_value(serde_json::json!({
        "command": "query_paths", "root_path": "/content/retained"
    }))
    .unwrap();
    let expected = expectation("query_paths");
    let mut source = document("query_paths", 0, &[]);
    source.canonical_result =
        r#"{"matches":[{"repository_path":"/content/retained/a"}]}"#.to_owned();
    let checked =
        decode_result_for_command(&serde_json::to_vec(&source).unwrap(), &expected, &command)
            .unwrap();
    assert_eq!(checked.canonical_result, source.canonical_result);
    for payload in [
        // Missing and surplus members, including an injected catalog tag.
        r#"{}"#,
        r#"{"matches":[],"private-canary":true}"#,
        r#"{"command":"query_paths","matches":[]}"#,
        // Serde accepts null for Option; the installed schema does not.
        r#"{"matches":[],"next_continuation_token":null}"#,
        // Canonical object bytes, but unordered or repeated set members.
        r#"{"matches":[{"repository_path":"/content/retained/b"},{"repository_path":"/content/retained/a"}]}"#,
        r#"{"matches":[{"repository_path":"/content/retained/a"},{"repository_path":"/content/retained/a"}]}"#,
        // Schema permits this prefix; the typed repository path refuses it.
        r#"{"matches":[{"repository_path":"/content/retained/../a"}]}"#,
        // A valid typed result, but not for the retained root.
        r#"{"matches":[{"repository_path":"/content/another-request/a"}]}"#,
    ] {
        source.canonical_result = payload.to_owned();
        let refusal =
            decode_result_for_command(&serde_json::to_vec(&source).unwrap(), &expected, &command)
                .unwrap_err();
        assert!(!format!("{refusal:?} {refusal}").contains("private-canary"));
    }
    source.canonical_result = r#"{"matches":[]}"#.to_owned();
    let mut fabricated = expected.clone();
    fabricated.expected_provenance.command_contract.result_schema_digest =
        SUBSTITUTED_DIGEST.to_owned();
    source.provenance.command_contract.result_schema_digest = SUBSTITUTED_DIGEST.to_owned();
    assert!(
        decode_result_for_command(&serde_json::to_vec(&source).unwrap(), &fabricated, &command)
            .is_err()
    );
}

#[test]
fn every_installed_result_schema_compiles_without_external_resolution() {
    use slingshot_domain::command::schema::{COMMAND_WIRE_NAMES, SchemaRole, command_schema};
    for name in COMMAND_WIRE_NAMES {
        assert!(
            jsonschema::draft202012::new(&command_schema(name, SchemaRole::Result)).is_ok(),
            "{name}"
        );
    }
}

#[test]
fn loaded_result_gate_checks_document_structure_not_only_outer_echo() {
    use slingshot_agent_connection::structured_job_result::decode_result_for_command;
    use slingshot_domain::command::{canonical_json::write_canonical, catalog::Command};
    let wire = "load_content_as_json";
    let command: Command = serde_json::from_value(serde_json::json!({
        "command":wire, "path":"/content/example", "depth":1
    }))
    .unwrap();
    let expected = expectation(wire);
    let leaf = |path: &str| serde_json::json!({"children":[],"children_truncated":false,"path":path,"properties":{}});
    let mut valid = leaf("/content/example");
    valid["children"] = serde_json::json!([
        leaf("/content/example/a"),
        leaf("/content/example/a[2]"),
        leaf("/content/example/a[10]")
    ]);
    for mutation in 0..6 {
        let mut tree = valid.clone();
        match mutation {
            0 => {}
            1 => tree["path"] = "/content/other".into(),
            2 => tree["children"][0]["path"] = "/content/other/a".into(),
            3 => tree["children"].as_array_mut().unwrap().swap(1, 2),
            4 => {
                tree["children"][0]["children"] =
                    serde_json::json!([leaf("/content/example/a/deeper")])
            }
            5 => tree["children_truncated"] = true.into(),
            _ => unreachable!(),
        }
        let mut source = document(wire, 0, &[]);
        source.canonical_result = write_canonical(
            &serde_json::json!({"disposition":"inline","path":"/content/example","document":tree}),
        )
        .unwrap();
        let checked =
            decode_result_for_command(&serde_json::to_vec(&source).unwrap(), &expected, &command);
        assert_eq!(checked.is_ok(), mutation == 0, "mutation {mutation}");
    }
}

#[test]
fn typed_artifact_results_require_exact_matching_envelope_metadata() {
    use slingshot_agent_connection::structured_job_result::decode_result_for_command;
    use slingshot_domain::command::canonical_json::write_canonical;
    use slingshot_domain::command::catalog::Command;
    for (wire, request, slot, media, name, length) in [
        (
            "load_content_as_json",
            serde_json::json!({"path":"/content/example"}),
            LOADED_CONTENT_SLOT,
            LOADED_CONTENT_MEDIA_TYPE,
            "loaded-content.json",
            262_145,
        ),
        (
            "download_content_package",
            serde_json::json!({"package_name":"example-pages","roots":["/content/example"]}),
            CONTENT_PACKAGE_SLOT,
            CONTENT_PACKAGE_MEDIA_TYPE,
            "example-pages.zip",
            4096,
        ),
    ] {
        let mut tagged = request;
        tagged["command"] = wire.into();
        let command: Command = serde_json::from_value(tagged).unwrap();
        let descriptor = serde_json::json!({
            "identifier":"expected-artifact-1", "slot":slot, "media_type":media,
            "byte_length":length, "digest":SUBMITTED_DIGEST, "suggested_file_name":name
        });
        let mut result = serde_json::json!({"artifact":descriptor});
        if wire == "load_content_as_json" {
            result["disposition"] = "artifact".into();
            result["path"] = "/content/example".into();
        }
        let mut source = document(wire, 0, &[]);
        source.canonical_result = write_canonical(&result).unwrap();
        source.declared_artifacts = vec![ArtifactEcho {
            byte_length: length,
            media_type: media.to_owned(),
            slot: slot.to_owned(),
            suggested_name: name.to_owned(),
        }];
        let expected = expectation(wire);
        let bytes = serde_json::to_vec(&source).unwrap();
        let checked = decode_result_for_command(&bytes, &expected, &command).unwrap();
        assert_eq!(checked.canonical_result, source.canonical_result);
        assert_eq!(checked.declared_artifacts, source.declared_artifacts);
        let retained = checked.remote_artifact.as_ref().unwrap();
        assert_eq!(retained.identifier.as_text(), "expected-artifact-1");
        assert_eq!(retained.digest.as_text(), SUBMITTED_DIGEST);
        assert_eq!(retained.byte_length, length);
        for mutation in 0..6 {
            let mut changed = source.clone();
            match mutation {
                0 => changed.declared_artifacts.clear(),
                1 => changed.declared_artifacts.push(changed.declared_artifacts[0].clone()),
                2 => changed.declared_artifacts[0].byte_length += 1,
                3 => {
                    changed.declared_artifacts[0].media_type = "application/octet-stream".to_owned()
                }
                4 => changed.declared_artifacts[0].slot = "structured_result".to_owned(),
                _ => changed.declared_artifacts[0].suggested_name = "another-name.json".to_owned(),
            }
            assert!(
                decode_result_for_command(
                    &serde_json::to_vec(&changed).unwrap(),
                    &expected,
                    &command
                )
                .is_err()
            );
        }
    }
}

/// Where the vectors this suite is driven from live.
const FIXTURES: &str = "tests/fixtures/structured-job-results";

/// The submission these results end.
const SUBMITTED_DIGEST: &str = "1111111111111111111111111111111111111111111111111111111111111111";

/// A digest substituted where a real one belongs.
const SUBSTITUTED_DIGEST: &str = "0000000000000000000000000000000000000000000000000000000000000000";

/// A key whose metatype says it is ordinary.
const ORDINARY_KEY: &str = "ordinary";

/// A key whose metatype says it is a password.
const SECRET_KEY: &str = "secret";

/// The value that key holds, which nothing may ever print.
const CANARY: &str = "a-classified-value-nothing-may-print";

/// How large one artifact echo says it is.
const ECHO_BYTES: u64 = 4_096;

/// Returns every vector one fixture holds.
fn vectors(name: &str) -> Vec<serde_json::Value> {
    let path = format!("{FIXTURES}/{name}");
    let text = std::fs::read_to_string(&path).unwrap_or_else(|_| panic!("{path} is readable"));
    text.lines().map(|line| serde_json::from_str(line).expect("each line is one vector")).collect()
}

/// Returns what this build has, for `wire_name`.
fn installed(wire_name: &str) -> ExpectedProvenance {
    ExpectedProvenance {
        canonical_json_contract_digest: canonical_contract_digest(),
        command_contract: SelectedCommandContractIdentity::installed(wire_name)
            .unwrap_or_else(|_| panic!("{wire_name} is published")),
        transport_contract_digest: AuthorAgentTransportContract::embedded_digest(),
    }
}

/// Returns what this daemon expects a result for `wire_name` to say.
fn expectation(wire_name: &str) -> ResultExpectation {
    ResultExpectation {
        operation: slingshot_agent_protocol::identity::WireOperationIdentity::of(
            &"a".repeat(64),
            &"b".repeat(64),
            "retained-operation",
            slingshot_domain::agent_identity::AgentEventStoreGeneration::of(7),
        ),
        daemon_subscription_identifier: "retained-subscription".to_owned(),
        expected_provenance: installed(wire_name),
        submitted_command_digest: SUBMITTED_DIGEST.to_owned(),
        wire_name: wire_name.to_owned(),
    }
}

/// Returns one echo filling `slot`.
fn echo(slot: &str, bytes: u64) -> ArtifactEcho {
    let (media_type, suggested_name) = if slot == LOADED_CONTENT_SLOT {
        (LOADED_CONTENT_MEDIA_TYPE, "loaded-content.json")
    } else {
        (CONTENT_PACKAGE_MEDIA_TYPE, "content-package.zip")
    };
    ArtifactEcho {
        byte_length: bytes,
        media_type: media_type.to_owned(),
        slot: slot.to_owned(),
        suggested_name: suggested_name.to_owned(),
    }
}

/// Returns one result document for `wire_name`.
fn document(wire_name: &str, inline_bytes: usize, slots: &[String]) -> TerminalResultDocument {
    TerminalResultDocument {
        operation: expectation(wire_name).operation,
        daemon_subscription_identifier: "retained-subscription".to_owned(),
        canonical_result: "r".repeat(inline_bytes),
        declared_artifacts: slots.iter().map(|slot| echo(slot, ECHO_BYTES)).collect(),
        provenance: installed(wire_name).provenance(),
        submitted_command_digest: SUBMITTED_DIGEST.to_owned(),
    }
}

#[test]
fn wire_decoding_preserves_canonical_bytes_without_claiming_command_validation() {
    let mut source = document("query_paths", 0, &[]);
    // Canonical JSON is not necessarily a valid query_paths command result.
    source.canonical_result = r#"{"a":"private-λ","b":[]}"#.to_owned();
    let bytes = serde_json::to_vec(&source).unwrap();
    let decoded = decode_terminal_result(&bytes, &expectation("query_paths")).unwrap();
    assert_eq!(decoded, source);
    assert_eq!(format!("{decoded:?}"), "TerminalResultDocument([redacted])");
    assert_eq!(format!("{decoded:#?}"), "TerminalResultDocument([redacted])");
    let metadata = require_valid(&expectation("query_paths"), &decoded).unwrap();
    assert_eq!(format!("{metadata:?}"), "ValidatedResult([redacted])");
    assert_eq!(format!("{metadata:#?}"), "ValidatedResult([redacted])");
}

#[test]
fn wire_decoding_refuses_noncanonical_payloads_and_unbound_envelopes() {
    let expected = expectation("query_paths");
    let mut source = document("query_paths", 0, &[]);
    for payload in [" {}", "{ }", "{\"b\":0,\"a\":0}", "{\"a\":0,\"a\":1}", "not-json"] {
        source.canonical_result = payload.to_owned();
        assert!(decode_terminal_result(&serde_json::to_vec(&source).unwrap(), &expected).is_err());
    }
    source.canonical_result = "{}".to_owned();
    let good = serde_json::to_value(&source).unwrap();
    for field in [
        "canonical_result",
        "declared_artifacts",
        "provenance",
        "submitted_command_digest",
        "operation",
        "daemon_subscription_identifier",
    ] {
        let mut missing = good.clone();
        missing.as_object_mut().unwrap().remove(field);
        assert!(decode_terminal_result(&serde_json::to_vec(&missing).unwrap(), &expected).is_err());
    }
    let mut surplus = good.clone();
    surplus["private-canary"] = true.into();
    let refusal =
        decode_terminal_result(&serde_json::to_vec(&surplus).unwrap(), &expected).unwrap_err();
    assert!(!format!("{refusal:?} {refusal}").contains("private-canary"));
    let bytes = serde_json::to_string(&source).unwrap();
    let duplicate = bytes.replacen('{', "{\"canonical_result\":\"{}\",", 1);
    assert!(decode_terminal_result(duplicate.as_bytes(), &expected).is_err());
    source.submitted_command_digest = SUBSTITUTED_DIGEST.to_owned();
    assert!(decode_terminal_result(&serde_json::to_vec(&source).unwrap(), &expected).is_err());
    source.submitted_command_digest = SUBMITTED_DIGEST.to_owned();
    source.provenance.transport_contract_digest = SUBSTITUTED_DIGEST.to_owned();
    assert!(decode_terminal_result(&serde_json::to_vec(&source).unwrap(), &expected).is_err());
    let mut wrong_command = expected.clone();
    wrong_command.wire_name = "another_command".to_owned();
    assert!(decode_terminal_result(bytes.as_bytes(), &wrong_command).is_err());
    let mut artifact = good;
    artifact["declared_artifacts"] = serde_json::json!([{
        "byte_length": 1, "media_type": "application/json", "slot": "slot",
        "suggested_name": "result.json", "unknown": true
    }]);
    assert!(decode_terminal_result(&serde_json::to_vec(&artifact).unwrap(), &expected).is_err());
}

#[test]
fn wire_decoding_bounds_both_envelope_and_inline_result() {
    let expected = expectation("query_paths");
    assert!(
        decode_terminal_result(&vec![b' '; maximum_document_bytes() as usize + 1], &expected)
            .is_err()
    );
    let mut source = document("query_paths", 0, &[]);
    source.canonical_result =
        format!("\"{}\"", "x".repeat(maximum_agent_inline_result_bytes() as usize - 2));
    assert!(decode_terminal_result(&serde_json::to_vec(&source).unwrap(), &expected).is_ok());
    source.canonical_result.insert(1, 'x');
    assert!(decode_terminal_result(&serde_json::to_vec(&source).unwrap(), &expected).is_err());
}

#[test]
fn identical_command_results_cannot_cross_operation_contexts() {
    let expected = expectation("query_paths");
    let mut source = document("query_paths", 0, &[]);
    source.canonical_result = "{}".to_owned();
    for field in 0..5 {
        let mut changed = source.clone();
        match field {
            0 => changed.operation.agent_event_store_generation += 1,
            1 => changed.operation.agent_operation_identifier = "another-operation".to_owned(),
            2 => changed.operation.author_target_identity_digest = "c".repeat(64),
            3 => changed.operation.selected_environment_revision = "d".repeat(64),
            _ => changed.daemon_subscription_identifier = "another-subscription".to_owned(),
        }
        assert_eq!(changed.submitted_command_digest, source.submitted_command_digest);
        assert!(decode_terminal_result(&serde_json::to_vec(&changed).unwrap(), &expected).is_err());
        assert_eq!(require_valid(&expected, &changed), Err(ResultRefusal::AnotherSubmission));
    }
}

/// Returns how `key` is classified.
fn classification_of(key: &str) -> Classification {
    match key {
        ORDINARY_KEY => Classification::NonPassword,
        SECRET_KEY => Classification::Password,
        _ => Classification::Unavailable,
    }
}

/// Returns the steps one trace vector describes.
fn steps_of(vector: &serde_json::Value) -> Vec<DictionaryStep> {
    vector["steps"]
        .as_array()
        .expect("a list")
        .iter()
        .map(|step| match step["step"].as_str().expect("a step") {
            "inventory" => DictionaryStep::KeyInventory {
                keys: step["keys"]
                    .as_array()
                    .expect("a list")
                    .iter()
                    .map(|key| key.as_str().expect("a key").to_owned())
                    .collect(),
            },
            "planned" => DictionaryStep::RedactionPlanned,
            "read" => {
                DictionaryStep::ValueAccess { key: step["key"].as_str().expect("a key").to_owned() }
            }
            other => panic!("{other} is a step this suite does not stage"),
        })
        .collect()
}

/// Returns how one trace refusal is spelled in the vectors.
fn trace_spelling(refusal: &TraceRefusal) -> &'static str {
    match refusal {
        TraceRefusal::InventoryNotFirst => "inventory-not-first",
        TraceRefusal::ValueReadBeforePlanning { .. } => "value-read-before-planning",
        TraceRefusal::ForbiddenValueRead { .. } => "forbidden-value-read",
        TraceRefusal::RepeatedValueRead { .. } => "repeated-value-read",
        TraceRefusal::KeyNotInventoried { .. } => "key-not-inventoried",
        TraceRefusal::DuplicateInventoryKey { .. } => "duplicate-inventory-key",
    }
}

#[test]
fn the_stages_are_ordered_and_the_bound_comes_before_the_parse() {
    assert_eq!(STAGE_ORDER.first(), Some(&ValidationStage::TransportBound));
    assert_eq!(STAGE_ORDER.last(), Some(&ValidationStage::RequestCorrelation));
    let digest_at = STAGE_ORDER
        .iter()
        .position(|stage| *stage == ValidationStage::SubmittedDigest)
        .expect("the digest is a stage");
    let shape_at = STAGE_ORDER
        .iter()
        .position(|stage| *stage == ValidationStage::DecodedShape)
        .expect("the shape is a stage");
    assert!(
        digest_at < shape_at,
        "the check a schema cannot make comes before the schema, so nothing is spent on a \
         document that ends another submission"
    );
    let raw_at = STAGE_ORDER
        .iter()
        .position(|stage| *stage == ValidationStage::RawCanonicalBytes)
        .expect("canonicality is a stage");
    assert!(
        raw_at < shape_at,
        "a schema applied to bytes nobody agreed were canonical proves little"
    );
}

#[test]
fn a_result_ending_another_submission_is_refused_before_anything_is_written() {
    let mut elsewhere = document("query_paths", ECHO_BYTES as usize, &[]);
    elsewhere.submitted_command_digest = SUBSTITUTED_DIGEST.to_owned();
    assert_eq!(
        require_valid(&expectation("query_paths"), &elsewhere),
        Err(ResultRefusal::AnotherSubmission),
        "the variant, the shape, and every echoed fact can all agree and it is still wrong"
    );
    let mut contract_moved = document("query_paths", ECHO_BYTES as usize, &[]);
    contract_moved.provenance.transport_contract_digest = SUBSTITUTED_DIGEST.to_owned();
    assert!(matches!(
        require_valid(&expectation("query_paths"), &contract_moved),
        Err(ResultRefusal::Provenance(_))
    ));
}

#[test]
fn a_document_larger_than_one_may_be_is_refused_before_it_is_read() {
    let allowed = maximum_document_bytes();
    let oversized = TerminalResultDocument {
        operation: expectation("query_paths").operation,
        daemon_subscription_identifier: "retained-subscription".to_owned(),
        canonical_result: "r".repeat(allowed as usize + 1),
        declared_artifacts: Vec::new(),
        provenance: installed("query_paths").provenance(),
        submitted_command_digest: SUBMITTED_DIGEST.to_owned(),
    };
    assert!(matches!(
        require_valid(&expectation("query_paths"), &oversized),
        Err(ResultRefusal::TooLarge { .. })
    ));
}

#[test]
fn every_size_lands_where_its_vector_says_it_does() {
    for vector in vectors("sizes.jsonl") {
        let name = vector["name"].as_str().expect("a name");
        let bytes = vector["bytes"].as_u64().expect("a size");
        let expected = vector["disposition"].as_str().expect("a disposition");
        match local_disposition(bytes) {
            Ok(LocalDisposition::Inline) => assert_eq!(expected, "inline", "{name}"),
            Ok(LocalDisposition::LocalArtifact { media_type, slot }) => {
                assert_eq!(expected, "local-artifact", "{name}");
                assert_eq!(slot, STRUCTURED_RESULT_SLOT);
                assert_eq!(media_type, STRUCTURED_RESULT_MEDIA_TYPE);
            }
            Err(ResultRefusal::TooLarge { .. }) => assert_eq!(expected, "too-large", "{name}"),
            Err(other) => panic!("{name}: {other}"),
        }
    }
    assert!(
        maximum_inline_machine_result_bytes() < maximum_agent_inline_result_bytes(),
        "the machine bound is about one local response and the transport bound about arrival"
    );
}

#[test]
fn every_artifact_echo_is_accepted_exactly_when_its_vector_says() {
    for vector in vectors("artifact-echoes.jsonl") {
        let name = vector["name"].as_str().expect("a name");
        let command = vector["command"].as_str().expect("a command");
        let slots: Vec<String> = vector["slots"]
            .as_array()
            .expect("a list")
            .iter()
            .map(|slot| slot.as_str().expect("a slot").to_owned())
            .collect();
        let inline_bytes = vector["inline_bytes"].as_u64().expect("a size") as usize;
        let produced =
            require_valid(&expectation(command), &document(command, inline_bytes, &slots));
        assert_eq!(
            produced.is_ok(),
            vector["accepted"].as_bool().expect("an expectation"),
            "{name}: {produced:?}"
        );
    }
}

#[test]
fn a_loaded_document_travels_inline_only_through_its_own_contract_bound() {
    let allowed = maximum_agent_inline_loaded_document_bytes();
    let command = loading_command();
    require_valid(&expectation(command), &document(command, allowed as usize, &[]))
        .expect("exactly at the bound is inline");
    assert_eq!(
        require_valid(&expectation(command), &document(command, allowed as usize + 1, &[])),
        Err(ResultRefusal::InlineLoadTooLarge { allowed, actual: allowed + 1 }),
        "the general transport ceiling governs arrival and does not widen this form"
    );
    let alternative = vec![LOADED_CONTENT_SLOT.to_owned()];
    require_valid(&expectation(command), &document(command, allowed as usize + 1, &alternative))
        .expect("past the bound the declared alternative is the only form");
    assert_eq!(
        require_valid(&expectation(command), &document(command, allowed as usize, &alternative)),
        Err(ResultRefusal::BothForms),
        "a result carrying its data twice has two copies that can disagree"
    );
    assert!(allowed < maximum_agent_inline_result_bytes());
}

#[test]
fn a_package_result_fills_the_slot_its_command_requires() {
    let (slot, media_type) = package_slot_and_media_type();
    assert_eq!((slot, media_type), (CONTENT_PACKAGE_SLOT, CONTENT_PACKAGE_MEDIA_TYPE));
    let declared = declared_slots("download_content_package");
    assert_eq!(declared.len(), 1);
    assert_eq!(declared[0].requirement, ArtifactRequirement::Required);
    assert_eq!(
        require_declared_artifacts("download_content_package", &[]),
        Err(ResultRefusal::RequiredSlotOmitted {
            command: "download_content_package".to_owned(),
            slot: CONTENT_PACKAGE_SLOT.to_owned()
        })
    );
    let wrong_media = ArtifactEcho {
        media_type: "application/octet-stream".to_owned(),
        ..echo(CONTENT_PACKAGE_SLOT, ECHO_BYTES)
    };
    assert!(matches!(
        require_declared_artifacts("download_content_package", &[wrong_media]),
        Err(ResultRefusal::EchoDrifted { .. })
    ));
    let empty = ArtifactEcho { byte_length: 0, ..echo(CONTENT_PACKAGE_SLOT, ECHO_BYTES) };
    assert!(matches!(
        require_declared_artifacts("download_content_package", &[empty]),
        Err(ResultRefusal::EchoDrifted { .. })
    ));
}

#[test]
fn a_command_that_declares_no_artifact_may_not_be_handed_one() {
    assert!(declared_slots("query_paths").is_empty(), "an empty manifest forbids remote artifacts");
    assert_eq!(
        require_declared_artifacts("query_paths", &[echo(CONTENT_PACKAGE_SLOT, ECHO_BYTES)]),
        Err(ResultRefusal::UndeclaredSlot {
            command: "query_paths".to_owned(),
            slot: CONTENT_PACKAGE_SLOT.to_owned()
        }),
        "a result filling a slot the command never declared is answering something else"
    );
}

#[test]
fn every_dictionary_trace_is_accepted_exactly_when_its_vector_says() {
    for vector in vectors("dictionary-traces.jsonl") {
        let name = vector["name"].as_str().expect("a name");
        let produced = require_two_phase_access(&steps_of(&vector), &classification_of);
        match (produced, vector["refusal"].as_str()) {
            (Ok(()), None) => {}
            (Err(refusal), Some(expected)) => {
                assert_eq!(trace_spelling(&refusal), expected, "{name}");
            }
            (produced, expected) => panic!("{name}: produced {produced:?}, expected {expected:?}"),
        }
    }
}

#[test]
fn an_unclassified_value_is_treated_as_a_password() {
    assert!(Classification::NonPassword.permits_value_access());
    assert!(!Classification::Password.permits_value_access());
    assert!(
        !Classification::Unavailable.permits_value_access(),
        "the absence of evidence that something is safe is not evidence that it is"
    );
}

#[test]
fn no_refusal_or_fixture_carries_a_classified_value() {
    let steps = vec![
        DictionaryStep::KeyInventory { keys: vec![ORDINARY_KEY.to_owned(), SECRET_KEY.to_owned()] },
        DictionaryStep::RedactionPlanned,
        DictionaryStep::ValueAccess { key: SECRET_KEY.to_owned() },
    ];
    let refusal =
        require_two_phase_access(&steps, &classification_of).expect_err("a password is not read");
    let rendered = format!("{refusal}");
    assert!(rendered.contains(SECRET_KEY), "a refusal names the key it refused");
    assert!(!rendered.contains(CANARY), "and never the value, which it never had: {rendered}");
    for fixture in ["sizes.jsonl", "artifact-echoes.jsonl", "dictionary-traces.jsonl"] {
        let path = format!("{FIXTURES}/{fixture}");
        let text = std::fs::read_to_string(&path).expect("it reads");
        assert!(!text.contains(CANARY), "{fixture} carries no classified value either");
    }
}
