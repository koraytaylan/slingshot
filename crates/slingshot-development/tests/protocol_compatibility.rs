//! What another program has already read, snapshotted so it cannot move quietly.
//!
//! Every value here is one some other program depends on: a command's wire
//! name, an error code, an outcome tag, a protocol revision, an exit. Changing
//! one is not necessarily wrong - but it is never accidental, and a snapshot is
//! how the decision gets made deliberately and recorded in the same commit that
//! makes it.
//!
//! What is deliberately not snapshotted is anything internal. A snapshot that
//! covered private structure would fail on every refactor and be rewritten
//! without being read, which is worse than having none.

use std::path::PathBuf;

use slingshot_command_line::exit_classification::EVERY_EXIT;
use slingshot_command_line::machine_outcome_envelope::MachineOutcomeEnvelope;
use slingshot_command_line::model_context_protocol::current_stateless_revision::{
    EVERY_ERROR, EVERY_REQUEST,
};
use slingshot_command_line::model_context_protocol::standard_stream_transport::SUPPORTED_REVISIONS;
use slingshot_domain::command::catalog::CommandCatalog;

/// Where the snapshot lives.
const SNAPSHOT: &str = "tests/fixtures/protocol-compatibility/snapshot.json";

/// The variable that arms a rewrite.
const REVIEW_VARIABLE: &str = "SLINGSHOT_REVIEW_PROTOCOL_COMPATIBILITY";

/// The command a reviewer runs to rewrite it.
const REVIEW_COMMAND: &str = "SLINGSHOT_REVIEW_PROTOCOL_COMPATIBILITY=1 \
     cargo test -p slingshot-development --test protocol_compatibility";

/// Returns everything a consumer has already read.
fn observable() -> serde_json::Value {
    let published = CommandCatalog::published();
    let commands: Vec<serde_json::Value> = published
        .descriptors()
        .iter()
        .map(|descriptor| {
            serde_json::json!({
                "arguments_schema_sha256": descriptor.arguments_schema_sha256,
                "command_contract_limits_sha256": descriptor.command_contract_limits_sha256,
                "maximum_result_bytes": descriptor.maximum_result_bytes,
                "result_schema_sha256": descriptor.result_schema_sha256,
                "semantic_version": descriptor.command_semantic_contract_version,
                "wire_name": descriptor.wire_name,
            })
        })
        .collect();
    serde_json::json!({
        "commands": commands,
        "exits": EVERY_EXIT,
        "outcome_tags": MachineOutcomeEnvelope::EVERY_TAG,
        "protocol_errors": EVERY_ERROR,
        "protocol_requests": EVERY_REQUEST,
        "protocol_revisions": SUPPORTED_REVISIONS,
    })
}

/// Returns where the snapshot lives.
fn snapshot_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(SNAPSHOT)
}

#[test]
fn nothing_a_consumer_reads_has_moved_since_it_was_last_agreed() {
    let held = serde_json::to_string_pretty(&observable()).expect("it writes") + "\n";
    if std::env::var(REVIEW_VARIABLE).is_ok() {
        std::fs::write(snapshot_path(), held).expect("the snapshot is written");
        return;
    }
    let committed = std::fs::read_to_string(snapshot_path()).unwrap_or_else(|failure| {
        panic!("the snapshot could not be read: {failure}; write it with `{REVIEW_COMMAND}`")
    });
    assert_eq!(
        held, committed,
        "something a consumer reads moved; review it with `{REVIEW_COMMAND}`"
    );
}

#[test]
fn the_snapshot_covers_every_kind_of_value_a_consumer_reads() {
    let held = observable();
    for named in [
        "commands",
        "exits",
        "outcome_tags",
        "protocol_errors",
        "protocol_requests",
        "protocol_revisions",
    ] {
        assert!(
            held[named].as_array().is_some_and(|values| !values.is_empty()),
            "{named} is empty"
        );
    }
}

#[test]
fn every_command_in_the_snapshot_carries_its_whole_identity() {
    let held = observable();
    for command in held["commands"].as_array().expect("the commands are a list") {
        for member in [
            "arguments_schema_sha256",
            "command_contract_limits_sha256",
            "result_schema_sha256",
            "semantic_version",
            "wire_name",
        ] {
            assert!(
                command[member].as_str().is_some_and(|value| !value.is_empty()),
                "a command omits {member}, and an identity missing a field is not one"
            );
        }
    }
}

#[test]
fn the_snapshot_holds_nothing_that_is_nobodys_business() {
    let rendered = serde_json::to_string(&observable()).expect("it writes");
    for internal in ["/home/", "target/debug", "sqlite", "password", "nonce"] {
        assert!(!rendered.contains(internal), "the snapshot carries {internal}");
    }
}
