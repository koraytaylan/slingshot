//! The tools this server offers, against the registry that decides them.
//!
//! Every count here is asserted rather than sampled, because the interesting
//! failure is not a wrong tool but a missing one: a command that exists and a
//! tool that does not is a capability a model host cannot reach, and nothing
//! else in the system notices.
//!
//! The safety annotations get the same treatment. They are what a host uses to
//! decide whether to call something twice or at all, so each one is compared
//! against the registry row it is derived from rather than against a list
//! somebody wrote down beside it.

use std::collections::BTreeSet;

use slingshot_command_line::model_context_protocol::tool_catalog::{
    CatalogRefusal, EVERY_CONTROL, KeyPresence, PROJECTED_VERSION, Provenance, ToolDescriptor,
    derive,
};
use slingshot_domain::command::catalog::CommandCatalog;

/// How many commands the registry publishes.
const PUBLISHED_COMMANDS: usize = 64;

/// How many of them are read-only.
const READ_ONLY_COMMANDS: usize = 28;

/// How many of them change something.
const CHANGING_COMMANDS: usize = 36;

/// How many of them may remove something.
const DESTRUCTIVE_COMMANDS: usize = 25;

/// How many of them are the same request twice over.
const IDEMPOTENT_COMMANDS: usize = 26;

/// How many of them require the caller's key.
const KEY_REQUIRING_COMMANDS: usize = 38;

/// The one command that may remove something.
const DESTRUCTIVE_COMMAND: &str = "replicate_content";

/// The read-only command that is nonetheless not the same request twice.
const READ_BUT_NOT_IDEMPOTENT: &str = "load_content_as_json";

/// Returns every tool, against this build's own provenance.
fn tools() -> Vec<ToolDescriptor> {
    derive(&Provenance::recomputed()).expect("this build's own provenance agrees with itself")
}

/// Returns the tool one name belongs to.
fn tool(named: &str) -> ToolDescriptor {
    tools()
        .into_iter()
        .find(|held| held.name == named)
        .unwrap_or_else(|| panic!("{named} is a tool this server offers"))
}

/// Returns every tool that comes from the registry rather than from a control.
fn command_tools() -> Vec<ToolDescriptor> {
    tools().into_iter().filter(|held| !EVERY_CONTROL.contains(&held.name.as_str())).collect()
}

#[test]
fn one_tool_exists_for_every_published_command_and_for_nothing_else() {
    let published: BTreeSet<String> = CommandCatalog::published()
        .descriptors()
        .iter()
        .map(|descriptor| descriptor.wire_name.clone())
        .collect();
    let derived: BTreeSet<String> = command_tools().into_iter().map(|held| held.name).collect();
    assert_eq!(derived, published, "a command and its tool disagree about existing");
    assert_eq!(published.len(), PUBLISHED_COMMANDS);
}

#[test]
fn the_controls_are_exactly_the_eight_this_server_fixes() {
    let derived: Vec<String> = tools()
        .into_iter()
        .map(|held| held.name)
        .filter(|name| EVERY_CONTROL.contains(&name.as_str()))
        .collect();
    assert_eq!(derived, EVERY_CONTROL.to_vec());
    assert!(
        !derived.iter().any(|name| name == "maintenance-result"),
        "a maintenance result is read as a resource rather than called as a tool"
    );
}

#[test]
fn every_safety_annotation_is_the_registry_row_it_comes_from() {
    for descriptor in CommandCatalog::published().descriptors() {
        let derived = tool(&descriptor.wire_name);
        assert_eq!(
            derived.read_only_hint,
            descriptor.access.read_only_hint(),
            "{} disagrees about being read-only",
            descriptor.wire_name
        );
        assert_eq!(
            derived.destructive_hint,
            descriptor.destructive.destructive_hint(),
            "{} disagrees about removing something",
            descriptor.wire_name
        );
        assert_eq!(
            derived.idempotent_hint,
            !descriptor.intrinsic_idempotency.requires_operation_key(),
            "{} disagrees about being the same request twice",
            descriptor.wire_name
        );
    }
}

#[test]
fn the_annotation_matrix_is_the_one_the_registry_produces() {
    let derived = command_tools();
    assert_eq!(derived.len(), PUBLISHED_COMMANDS);
    assert_eq!(derived.iter().filter(|held| held.read_only_hint).count(), READ_ONLY_COMMANDS);
    assert_eq!(derived.iter().filter(|held| !held.read_only_hint).count(), CHANGING_COMMANDS);
    assert_eq!(derived.iter().filter(|held| held.destructive_hint).count(), DESTRUCTIVE_COMMANDS);
    assert_eq!(derived.iter().filter(|held| held.idempotent_hint).count(), IDEMPOTENT_COMMANDS);
    assert_eq!(
        derived.iter().filter(|held| held.operation_key == KeyPresence::Required).count(),
        KEY_REQUIRING_COMMANDS
    );
    assert_eq!(
        derived.iter().filter(|held| held.operation_key == KeyPresence::Optional).count(),
        IDEMPOTENT_COMMANDS
    );
}

#[test]
fn the_two_rows_that_are_easy_to_get_wrong_are_the_ones_the_registry_says() {
    let destructive = tool(DESTRUCTIVE_COMMAND);
    assert!(destructive.destructive_hint, "a destructive command is a destructive tool");
    assert!(!destructive.read_only_hint);
    assert_eq!(destructive.operation_key, KeyPresence::Required);
    assert_eq!(
        command_tools().iter().filter(|held| held.destructive_hint).count(),
        DESTRUCTIVE_COMMANDS,
        "and every destructive row is one"
    );

    let read_but_not_idempotent = tool(READ_BUT_NOT_IDEMPOTENT);
    assert!(read_but_not_idempotent.read_only_hint, "reading changes nothing");
    assert!(!read_but_not_idempotent.destructive_hint);
    assert!(
        !read_but_not_idempotent.idempotent_hint,
        "two reads of a moving repository are not one read"
    );
    assert_eq!(read_but_not_idempotent.operation_key, KeyPresence::Required);
}

#[test]
fn a_control_starts_no_work_and_therefore_takes_no_key() {
    for named in EVERY_CONTROL {
        let control = tool(named);
        assert_eq!(control.operation_key, KeyPresence::Absent, "{named}");
        assert!(control.idempotent_hint, "{named} is the same control twice over");
        assert!(!control.description.is_empty(), "{named} says what it is");
    }
    assert!(tool("maintenance-apply").destructive_hint, "an apply removes what a preview showed");
    assert!(!tool("operation-list").destructive_hint);
}

#[test]
fn provenance_drift_offers_no_tools_at_all() {
    let named = [
        "author_agent_transport_contract_digest",
        "canonical_contract_digest",
        "command_schema_manifest_digest",
        "role_annotations",
    ];
    for member in named {
        let mut drifted = Provenance::recomputed();
        match member {
            "author_agent_transport_contract_digest" => {
                drifted.author_agent_transport_contract_digest = DRIFTED.to_owned();
            }
            "canonical_contract_digest" => drifted.canonical_contract_digest = DRIFTED.to_owned(),
            "command_schema_manifest_digest" => {
                drifted.command_schema_manifest_digest = DRIFTED.to_owned();
            }
            _ => drifted.role_annotations = vec![DRIFTED.to_owned()],
        }
        let refusal = derive(&drifted).expect_err("{member} drifted");
        assert!(
            matches!(refusal, CatalogRefusal::ProvenanceDrifted(_)),
            "{member} was refused as {refusal:?}"
        );
    }
}

/// A digest no build carries.
const DRIFTED: &str = "1111111111111111111111111111111111111111111111111111111111111111";

#[test]
fn every_published_command_declares_the_one_version_this_build_projects() {
    for descriptor in CommandCatalog::published().descriptors() {
        assert_eq!(
            descriptor.command_semantic_contract_version, PROJECTED_VERSION,
            "{} declares another version",
            descriptor.wire_name
        );
    }
}

#[test]
fn provenance_is_recomputed_rather_than_remembered() {
    let recomputed = Provenance::recomputed();
    assert_eq!(recomputed, Provenance::recomputed(), "two computations agree");
    assert!(recomputed.agrees_with(&Provenance::recomputed()));
    assert_eq!(recomputed.role_annotations.len(), ROLE_COUNT);
    for annotation in &recomputed.role_annotations {
        assert_eq!(
            *annotation, recomputed.canonical_contract_digest,
            "a role schema names the contract it was written against"
        );
    }
}

/// How many roles a command's schemas are written in.
const ROLE_COUNT: usize = 2;
