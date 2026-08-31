//! How a workflow names one command effect, and what that naming guarantees.
//!
//! One property carries everything else: the same intended occurrence always
//! derives the same key, and two different occurrences never do. That is what
//! makes a retry attach to work that already exists and a deliberate second run
//! start work that does not - and every vector here is one way of getting that
//! wrong.
//!
//! The exact preimage bytes are pinned as well as the keys. Two implementations
//! agreeing on which members go in and differing on their order would derive
//! different keys for the same occurrence, and the retry meant to attach to
//! existing work would quietly start new work instead.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use slingshot_development::finite_state_machine_handler_validation::{
    EVERY_SUFFIX, KEY_PREFIX, KeyRefusal, MOST_INPUT_UTF8_BYTES, MOST_KEY_BYTES, MOST_SUFFIX_BYTES,
    key_preimage, workflow_effect_operation_key,
};

/// Where the derivation vectors live.
const VECTOR_FIXTURE: &str = "tests/fixtures/finite-state-machine-handler/keys.jsonl";

/// Where the examples a person copies live.
const EXAMPLES: &str = "../../examples/finite-state-machine";

/// How many characters a digest is written in.
const DIGEST_CHARACTERS: usize = 64;

/// One declared derivation.
#[derive(Debug, serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct Vector {
    /// What it is called.
    name: String,
    /// Which store.
    workflow_operation_namespace: String,
    /// Which instance request.
    instance_request_identifier: String,
    /// Which occurrence.
    occurrence: u64,
    /// Which suffix.
    suffix: String,
    /// The exact bytes hashed.
    preimage: String,
    /// The exact key derived.
    key: String,
}

/// Returns every declared derivation.
fn vectors() -> Vec<Vector> {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(VECTOR_FIXTURE);
    let text = std::fs::read_to_string(&path)
        .unwrap_or_else(|failure| panic!("{} could not be read: {failure}", path.display()));
    text.lines()
        .filter(|line| !line.trim().is_empty() && !line.starts_with('#'))
        .map(|line| serde_json::from_str(line).expect("every vector reads"))
        .collect()
}

/// Returns the key one vector derives.
fn derived(vector: &Vector) -> String {
    workflow_effect_operation_key(
        &vector.workflow_operation_namespace,
        &vector.instance_request_identifier,
        vector.occurrence,
        &vector.suffix,
    )
    .unwrap_or_else(|refusal| panic!("{} derives a key: {refusal}", vector.name))
}

#[test]
fn every_vector_hashes_the_exact_bytes_it_says_and_derives_the_exact_key() {
    for vector in vectors() {
        let held = key_preimage(
            &vector.workflow_operation_namespace,
            &vector.instance_request_identifier,
            vector.occurrence,
        )
        .unwrap_or_else(|refusal| panic!("{} has a preimage: {refusal}", vector.name));
        assert_eq!(held, vector.preimage, "{} hashes other bytes", vector.name);
        assert!(!held.contains(' '), "{} hashes whitespace", vector.name);
        assert!(!held.contains('\n'), "{} hashes a newline", vector.name);
        assert_eq!(derived(&vector), vector.key, "{} derives another key", vector.name);
    }
}

#[test]
fn the_same_occurrence_derives_the_same_key_however_often_it_is_asked() {
    let vectors = vectors();
    let first = vectors.iter().find(|held| held.name == "ordinary").expect("it is declared");
    let again = vectors
        .iter()
        .find(|held| held.name == "the-same-occurrence-again")
        .expect("it is declared");
    assert_eq!(derived(first), derived(again), "a retry attaches to work that already exists");
}

#[test]
fn a_different_occurrence_or_a_different_store_derives_a_different_key() {
    let vectors = vectors();
    let held =
        |name: &str| derived(vectors.iter().find(|row| row.name == name).expect("it is declared"));
    let ordinary = held("ordinary");
    assert_ne!(ordinary, held("a-second-occurrence"), "two deliberate runs are two operations");
    assert_ne!(
        ordinary,
        held("another-store-same-instance"),
        "two stores with the same instance request are two operations"
    );
    assert_ne!(ordinary, held("the-compensating-effect"), "compensating is not the same effect");
    assert_ne!(
        held("composed-unicode"),
        held("decomposed-unicode"),
        "no normalization happens, so two spellings are two names"
    );
}

#[test]
fn every_key_is_the_prefix_a_digest_and_at_most_one_admitted_suffix() {
    for vector in vectors() {
        let held = derived(&vector);
        let digest = held
            .strip_prefix(KEY_PREFIX)
            .unwrap_or_else(|| panic!("{} does not begin where a key begins", vector.name));
        let (digest, suffix) = digest.split_at(DIGEST_CHARACTERS);
        assert!(
            digest
                .chars()
                .all(|character| character.is_ascii_digit() || ('a'..='f').contains(&character)),
            "{} is not lowercase hexadecimal",
            vector.name
        );
        assert!(EVERY_SUFFIX.contains(&suffix), "{} carries {suffix}", vector.name);
        assert!(held.len() <= MOST_KEY_BYTES, "{} is {} bytes", vector.name, held.len());
    }
    assert_eq!(KEY_PREFIX.len() + DIGEST_CHARACTERS + MOST_SUFFIX_BYTES, MOST_KEY_BYTES);
}

#[test]
fn an_input_the_contract_refuses_derives_no_key_at_all() {
    let refused = |namespace: &str, instance: &str| {
        workflow_effect_operation_key(namespace, instance, 0, "").expect_err("it is refused")
    };
    assert_eq!(
        refused("", "instance"),
        KeyRefusal::Empty("workflow_operation_namespace".to_owned())
    );
    assert_eq!(refused("store", ""), KeyRefusal::Empty("instance_request_identifier".to_owned()));
    let long = "n".repeat(MOST_INPUT_UTF8_BYTES + 1);
    assert_eq!(
        refused(&long, "instance"),
        KeyRefusal::TooLong {
            held: MOST_INPUT_UTF8_BYTES + 1,
            named: "workflow_operation_namespace".to_owned(),
        }
    );
    assert_eq!(
        refused("store\u{7}", "instance"),
        KeyRefusal::ControlCodePoint("workflow_operation_namespace".to_owned())
    );
    assert_eq!(
        refused("store\u{9f}", "instance"),
        KeyRefusal::ControlCodePoint("workflow_operation_namespace".to_owned())
    );
    assert_eq!(
        workflow_effect_operation_key("store", "instance", 0, "-something-else"),
        Err(KeyRefusal::SuffixUnknown("-something-else".to_owned()))
    );
}

#[test]
fn the_examples_a_person_copies_pass_this_products_own_validation() {
    use slingshot_command_line::model_context_protocol::tool_catalog::{Provenance, derive};
    use slingshot_development::finite_state_machine_handler_validation::{Handler, kind_of};

    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(EXAMPLES);
    let template: serde_json::Value = serde_json::from_str(
        &std::fs::read_to_string(path.join("slingshot.handlers.template.json"))
            .expect("the template is committed"),
    )
    .expect("the template reads");
    assert_eq!(template["format"].as_str(), Some("fsm.handlers/1"));

    let offered: BTreeSet<String> = derive(&Provenance::recomputed())
        .expect("this build's provenance agrees")
        .into_iter()
        .map(|tool| tool.name)
        .collect();
    let handlers = template["handlers"].as_object().expect("the template declares handlers");
    assert!(!handlers.is_empty());
    for (named, held) in handlers {
        let handler: Handler =
            serde_json::from_value(held.clone()).expect("every example handler reads");
        let kind =
            kind_of(&handler.tool, &offered).unwrap_or_else(|refusal| panic!("{named}: {refusal}"));
        let carries_key = !handler.arguments["operation_key"].is_null();
        let expects_key = matches!(
            kind,
            slingshot_development::finite_state_machine_handler_validation::ToolKind::RegistryCommand
        );
        assert_eq!(carries_key, expects_key, "{named} carries the wrong kind of identity");
    }
    assert!(Path::new(&path.join("operation-key.machine.json")).is_file());
}
