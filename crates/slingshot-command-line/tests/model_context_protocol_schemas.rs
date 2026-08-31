//! What a tool declares it takes, what it declares it answers, and the order
//! in which a document is refused.
//!
//! Which check said no is as much a part of the contract as whether one did. A
//! caller told "invalid" learns nothing; a caller told the bytes were not
//! canonical fixes their serializer, one told the shape was wrong fixes their
//! document, and one told the values could not be constructed fixes their
//! values. So every negative case here asserts the stage as well as the
//! refusal.
//!
//! The strongest claim is the ordering one: a document whose bytes are not
//! canonical is refused before its shape is examined. Accepting it because it
//! parses would accept a document the language-neutral validator rejects, and
//! the agent on the other side runs that validator.

use serde_json::{Value, json};

use slingshot_command_line::machine_outcome_envelope::MachineOutcomeEnvelope;
use slingshot_command_line::model_context_protocol::schema_projection::{
    DETACHED_MEMBER, LEAST_OPERATION_KEY_BYTES, MOST_OPERATION_KEY_BYTES, OPERATION_KEY_MEMBER,
    Stage, answerable_tags, input_schema, output_schema, require_acceptable,
};
use slingshot_command_line::model_context_protocol::tool_catalog::{
    EVERY_CONTROL, KeyPresence, Provenance, ToolDescriptor, derive,
};
use slingshot_domain::command::canonical_json::write_canonical;
use slingshot_domain::command::catalog::CommandCatalog;
use slingshot_domain::command::schema::CANONICAL_CONTRACT_ANNOTATION;

/// Returns every tool this server offers.
fn tools() -> Vec<ToolDescriptor> {
    derive(&Provenance::recomputed()).expect("this build's provenance agrees with itself")
}

/// Returns the tool one name belongs to.
fn tool(named: &str) -> ToolDescriptor {
    tools().into_iter().find(|held| held.name == named).expect("the tool exists")
}

/// Returns one document's canonical bytes.
fn canonical(value: &Value) -> Vec<u8> {
    write_canonical(value).expect("the document is canonical").into_bytes()
}

#[test]
fn every_tool_declares_an_input_schema_that_names_the_contract_it_was_written_against() {
    let annotation = slingshot_domain::command::schema::canonical_contract_digest();
    for held in tools() {
        let declared = input_schema(&held).expect("every tool projects");
        assert_eq!(
            declared[CANONICAL_CONTRACT_ANNOTATION].as_str(),
            Some(annotation.as_str()),
            "{} does not name the contract it was written against",
            held.name
        );
        assert_eq!(declared["type"].as_str(), Some("object"), "{}", held.name);
    }
}

#[test]
fn a_command_tool_declares_the_key_and_the_detachment_this_protocol_adds() {
    for descriptor in CommandCatalog::published().descriptors() {
        let held = tool(&descriptor.wire_name);
        let declared = input_schema(&held).expect("it projects");
        let properties = &declared["properties"];
        assert!(!properties[OPERATION_KEY_MEMBER].is_null(), "{}", held.name);
        assert!(!properties[DETACHED_MEMBER].is_null(), "{}", held.name);
        let required: Vec<&str> = declared["required"]
            .as_array()
            .map(|members| members.iter().filter_map(Value::as_str).collect())
            .unwrap_or_default();
        let names_key = required.contains(&OPERATION_KEY_MEMBER);
        assert_eq!(
            names_key,
            held.operation_key == KeyPresence::Required,
            "{} disagrees with itself about requiring a key",
            held.name
        );
    }
}

#[test]
fn a_control_declares_what_it_needs_and_takes_no_operation_key() {
    for named in EVERY_CONTROL {
        let held = tool(named);
        let declared = input_schema(&held).expect("it projects");
        assert!(declared["properties"][OPERATION_KEY_MEMBER].is_null(), "{named}");
        assert_eq!(declared["additionalProperties"].as_bool(), Some(false), "{named}");
    }
    let restart = input_schema(&tool("operation-restart")).expect("it projects");
    let required: Vec<&str> = restart["required"]
        .as_array()
        .expect("a resume names what it needs")
        .iter()
        .filter_map(Value::as_str)
        .collect();
    assert!(required.contains(&"operation_identifier"));
    assert!(required.contains(&"expected_recovery_category"));
}

#[test]
fn an_omitted_key_is_accepted_by_exactly_the_tools_that_may_omit_it() {
    for held in tools() {
        let bytes = canonical(&json!({}));
        let accepted = require_acceptable(&held, &bytes);
        match held.operation_key {
            KeyPresence::Required => {
                let refusal = accepted.expect_err("a required key cannot be omitted");
                assert_eq!(refusal.stage, Stage::DecodedShape, "{}", held.name);
            }
            KeyPresence::Optional | KeyPresence::Absent => {
                accepted.unwrap_or_else(|refusal| panic!("{}: {refusal}", held.name));
            }
        }
    }
}

#[test]
fn a_key_outside_its_bounds_is_refused_for_every_tool_that_takes_one() {
    let empty = canonical(&json!({ OPERATION_KEY_MEMBER: "" }));
    let long = canonical(
        &json!({ OPERATION_KEY_MEMBER: "k".repeat(usize::try_from(MOST_OPERATION_KEY_BYTES).unwrap_or_default() + 1) }),
    );
    for held in tools().into_iter().filter(|held| held.operation_key != KeyPresence::Absent) {
        for bytes in [&empty, &long] {
            let refusal =
                require_acceptable(&held, bytes).expect_err("a key outside its bounds is refused");
            assert_eq!(refusal.stage, Stage::DecodedShape, "{}", held.name);
        }
        let inside = canonical(&json!({ OPERATION_KEY_MEMBER: "k" }));
        require_acceptable(&held, &inside).expect("the smallest key is inside the bound");
        assert_eq!(LEAST_OPERATION_KEY_BYTES, 1, "the smallest key is one byte");
    }
}

#[test]
fn a_control_that_is_given_a_key_is_refused_rather_than_ignored() {
    for named in EVERY_CONTROL {
        let bytes = canonical(&json!({ OPERATION_KEY_MEMBER: "one" }));
        let refusal = require_acceptable(&tool(named), &bytes)
            .expect_err("a control starts no work, so a key means the caller expects something");
        assert_eq!(refusal.stage, Stage::DecodedShape, "{named}");
    }
}

#[test]
fn noncanonical_bytes_are_refused_before_their_shape_is_examined() {
    let held = tool("find_pages_by_template");
    let reordered = br#"{"operation_key":"one","detached":false}"#;
    let canonical_form = canonical(&json!({ DETACHED_MEMBER: false, OPERATION_KEY_MEMBER: "one" }));
    assert_ne!(reordered.to_vec(), canonical_form, "the two orderings differ");
    require_acceptable(&held, &canonical_form).expect("the canonical form is accepted");
    let refusal =
        require_acceptable(&held, reordered).expect_err("a noncanonical document is refused");
    assert_eq!(
        refusal.stage,
        Stage::RawBytes,
        "a shape that would have passed cannot excuse bytes that did not"
    );
}

#[test]
fn a_documents_own_shape_cannot_rescue_bytes_that_are_not_canonical() {
    let held = tool("operation-status");
    let spaced = b"{ \"operation_identifier\": \"one\" }";
    let refusal = require_acceptable(&held, spaced).expect_err("whitespace is not canonical");
    assert_eq!(refusal.stage, Stage::RawBytes);
}

#[test]
fn every_tool_answers_only_with_tags_the_envelope_declares() {
    for held in tools() {
        let answerable = answerable_tags(&held);
        assert!(!answerable.is_empty(), "{} answers with something", held.name);
        for tag in &answerable {
            assert!(
                MachineOutcomeEnvelope::EVERY_TAG.contains(tag),
                "{} answers with {tag}, which is not a tag",
                held.name
            );
        }
        let declared = output_schema(&held);
        let enumerated: Vec<&str> = declared["properties"]["outcome"]["enum"]
            .as_array()
            .expect("the outcomes are a list")
            .iter()
            .filter_map(Value::as_str)
            .collect();
        assert_eq!(enumerated, answerable, "{}", held.name);
    }
}

#[test]
fn a_tool_does_not_declare_an_answer_it_cannot_produce() {
    let listing = answerable_tags(&tool("operation-list"));
    assert_eq!(listing, vec!["operation_list_page"]);
    assert!(
        !answerable_tags(&tool("operation-list")).contains(&"operation_receipt"),
        "a listing admits nothing, so it issues no receipt"
    );
    assert!(
        !answerable_tags(&tool("load_content_as_json")).contains(&"maintenance_preview"),
        "a content read previews no maintenance"
    );
    for held in tools() {
        assert!(
            !answerable_tags(&held).contains(&"local_application_error"),
            "{} cannot answer with a command line's own interruption",
            held.name
        );
        assert!(
            !answerable_tags(&held).contains(&"configuration_report"),
            "{} answers about operations rather than about this machine",
            held.name
        );
        assert!(!answerable_tags(&held).contains(&"daemon_control"), "{}", held.name);
    }
}

/// Where the projected-schema digests live.
const DIGEST_FIXTURE: &str =
    "../slingshot-test-support/fixtures/model-context-protocol/schemas/digests.jsonl";

/// The variable that arms a rewrite of the digests.
const REVIEW_VARIABLE: &str = "SLINGSHOT_REVIEW_PROJECTED_SCHEMAS";

/// The command a reviewer runs to rewrite them.
const REVIEW_COMMAND: &str = "SLINGSHOT_REVIEW_PROJECTED_SCHEMAS=1 \
     cargo test -p slingshot-command-line --test model_context_protocol_schemas";

/// Returns the digest of one projected schema.
fn digest_of(schema: &Value) -> String {
    use sha2::Digest;
    let canonical = write_canonical(schema).expect("a projected schema is canonical");
    let mut digest = sha2::Sha256::new();
    digest.update(canonical.as_bytes());
    digest.finalize().iter().map(|byte| format!("{byte:02x}")).collect()
}

#[test]
fn every_projected_schema_digests_to_what_was_committed_for_it() {
    let path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(DIGEST_FIXTURE);
    let mut rows = Vec::new();
    for held in tools() {
        let declared = input_schema(&held).expect("it projects");
        rows.push(json!({
            "tool": held.name,
            "input": digest_of(&declared),
            "output": digest_of(&output_schema(&held)),
        }));
    }
    let rendered = rows
        .iter()
        .map(|row| serde_json::to_string(row).expect("a row writes"))
        .collect::<Vec<String>>()
        .join("\n")
        + "\n";
    if std::env::var(REVIEW_VARIABLE).is_ok() {
        std::fs::write(&path, rendered).expect("the digests are written");
        return;
    }
    let committed = std::fs::read_to_string(&path).unwrap_or_else(|failure| {
        panic!("{} could not be read: {failure}; write it with `{REVIEW_COMMAND}`", path.display())
    });
    assert_eq!(rendered, committed, "a projection changed; review it with `{REVIEW_COMMAND}`");
}
