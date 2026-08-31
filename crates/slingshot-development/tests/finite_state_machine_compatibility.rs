//! What this repository claims to be compatible with, and how tightly.
//!
//! The manifest is the sole authority, so the suite reads it rather than
//! restating it, and every field it declares is required to be there. What is
//! restated here on purpose is the set of origin spellings, because a grammar
//! is only as closed as the list of things somebody tried against it.
//!
//! The recorded contract digests are recomputed from the bytes beside them. Two
//! recorded strings agreeing proves only that somebody wrote the same thing
//! twice, which is exactly the failure a pin exists to catch.

use std::path::PathBuf;

use slingshot_development::finite_state_machine_compatibility::{
    COMMIT_CHARACTERS, FiniteStateMachineCompatibilityPin, MANIFEST_FORMAT, MANIFEST_PATH,
    PinRefusal, canonical_origin,
};

/// Where the origin spellings live.
const ORIGIN_FIXTURE: &str = "tests/fixtures/finite-state-machine-compatibility/origins.jsonl";

/// How many directories separate this crate from the workspace root.
const CRATE_DEPTH: usize = 2;

/// Returns the workspace root.
fn workspace_root() -> PathBuf {
    let mut root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    for _ in 0..CRATE_DEPTH {
        root = root.parent().expect("the crate is inside the workspace").to_path_buf();
    }
    root
}

/// Reads one file from the workspace.
fn read_repository_file(relative: &str) -> String {
    let path = workspace_root().join(relative);
    std::fs::read_to_string(&path)
        .unwrap_or_else(|failure| panic!("{} could not be read: {failure}", path.display()))
}

/// Returns the committed pin.
fn pin() -> FiniteStateMachineCompatibilityPin {
    FiniteStateMachineCompatibilityPin::parse(&read_repository_file(MANIFEST_PATH))
        .expect("the committed manifest parses")
}

/// One declared origin spelling.
#[derive(Debug, serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct Origin {
    /// What it is called.
    name: String,
    /// What is written.
    spelling: String,
    /// What it canonicalizes to, or nothing when it is refused.
    canonical: String,
}

/// Returns every declared origin spelling.
fn origins() -> Vec<Origin> {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(ORIGIN_FIXTURE);
    let text = std::fs::read_to_string(&path)
        .unwrap_or_else(|failure| panic!("{} could not be read: {failure}", path.display()));
    text.lines()
        .filter(|line| !line.trim().is_empty() && !line.starts_with('#'))
        .map(|line| serde_json::from_str(line).expect("every origin reads"))
        .collect()
}

#[test]
fn the_committed_manifest_declares_every_value_this_integration_is_bound_to() {
    let held = pin();
    assert_eq!(held.format, MANIFEST_FORMAT);
    assert_eq!(held.repository, "https://github.com/koraytaylan/fsm");
    assert_eq!(held.commit.len(), COMMIT_CHARACTERS);
    assert_eq!(held.model_context_protocol_revision, "2025-06-18");
    assert_eq!(held.handler_format, "fsm.handlers/1");
    assert_eq!(held.daemon_runtime_contract_format, "slingshot.daemon-runtime-contract/1");
    assert_eq!(
        held.author_agent_transport_contract_format,
        "slingshot.author-agent-transport-contract/1"
    );
}

#[test]
fn the_recorded_digests_are_what_the_committed_contracts_produce() {
    let held = pin();
    let runtime = read_repository_file("policy/daemon-runtime-contract-1.json");
    let transport = read_repository_file("policy/author-agent-transport-contract-1.json");
    held.require_contract_digests(runtime.as_bytes(), transport.as_bytes())
        .expect("the manifest records what the bytes beside it produce");

    let drifted = held.require_contract_digests(b"something else", transport.as_bytes());
    assert_eq!(
        drifted,
        Err(PinRefusal::DigestDrifted("the daemon runtime contract".to_owned())),
        "a contract that moved is caught before anything runs against it"
    );
    let other = held.require_contract_digests(runtime.as_bytes(), b"something else");
    assert_eq!(
        other,
        Err(PinRefusal::DigestDrifted("the author-agent transport contract".to_owned()))
    );
}

#[test]
fn the_recorded_digests_are_also_what_the_sidecars_say() {
    let held = pin();
    let runtime = read_repository_file("policy/daemon-runtime-contract-1.sha256");
    let transport = read_repository_file("policy/author-agent-transport-contract-1.sha256");
    assert_eq!(held.daemon_runtime_contract_sha256, runtime.trim());
    assert_eq!(held.author_agent_transport_contract_sha256, transport.trim());
}

#[test]
fn the_identity_is_the_whole_tuple_and_not_some_of_it() {
    let held = pin();
    let identity = held.identity();
    assert_eq!(identity.commit, held.commit);
    assert_eq!(identity.repository, held.repository);
    assert_eq!(identity.model_context_protocol_revision, held.model_context_protocol_revision);
    assert_eq!(identity.handler_format, held.handler_format);
    assert_eq!(identity.daemon_runtime_contract.1, held.daemon_runtime_contract_sha256);
    assert_eq!(
        identity.author_agent_transport_contract.1,
        held.author_agent_transport_contract_sha256
    );

    let mut moved = held.clone();
    moved.commit = "0".repeat(COMMIT_CHARACTERS);
    assert_ne!(moved.identity(), identity, "a different commit is a different compatibility");
}

#[test]
fn every_declared_origin_spelling_is_canonicalized_or_refused() {
    let declared = origins();
    assert!(!declared.is_empty());
    for origin in declared {
        let held = canonical_origin(&origin.spelling);
        if origin.canonical.is_empty() {
            assert!(
                held.is_err(),
                "{} ({}) was repaired rather than refused",
                origin.name,
                origin.spelling
            );
            continue;
        }
        assert_eq!(
            held.expect("this spelling is admitted"),
            origin.canonical,
            "{} canonicalizes elsewhere",
            origin.name
        );
    }
}

#[test]
fn the_three_admitted_spellings_all_reach_the_pinned_repository() {
    let held = pin();
    for spelling in [
        "https://github.com/koraytaylan/fsm.git",
        "ssh://git@github.com/koraytaylan/fsm",
        "git@github.com:koraytaylan/fsm.git",
    ] {
        assert_eq!(
            canonical_origin(spelling).expect("it is admitted"),
            held.repository,
            "{spelling} does not reach the pinned repository"
        );
    }
}

#[test]
fn a_manifest_that_declares_another_format_is_refused() {
    let held = read_repository_file(MANIFEST_PATH).replace(MANIFEST_FORMAT, "something.else/1");
    let refusal = FiniteStateMachineCompatibilityPin::parse(&held).expect_err("another format");
    assert_eq!(refusal, PinRefusal::ForeignFormat("something.else/1".to_owned()));
}

#[test]
fn a_manifest_missing_or_repeating_a_field_is_refused() {
    let committed = read_repository_file(MANIFEST_PATH);
    let without = committed
        .lines()
        .filter(|line| !line.starts_with("handler_format"))
        .collect::<Vec<&str>>()
        .join("\n");
    assert!(
        matches!(
            FiniteStateMachineCompatibilityPin::parse(&without),
            Err(PinRefusal::Unreadable(_))
        ),
        "a missing field is a manifest this build refuses"
    );
    let surplus = format!("{committed}\nsomething_else = \"held\"\n");
    assert!(matches!(
        FiniteStateMachineCompatibilityPin::parse(&surplus),
        Err(PinRefusal::Unreadable(_))
    ));
}

#[test]
fn a_commit_that_is_not_forty_hexadecimal_characters_is_refused() {
    let committed = read_repository_file(MANIFEST_PATH);
    for held in ["7d183e4d", &"g".repeat(COMMIT_CHARACTERS), &"A".repeat(COMMIT_CHARACTERS)] {
        let altered = committed.replace("7d183e4d7a6b130343ea7d88897e0d029f604813", held);
        assert_eq!(
            FiniteStateMachineCompatibilityPin::parse(&altered),
            Err(PinRefusal::CommitUnusable),
            "{held} is not a commit"
        );
    }
}

#[test]
fn every_seed_dimension_is_bounded_and_the_key_contract_is_closed() {
    let limits = pin().cargo_home_seed;
    assert_eq!(limits.maximum_files, 65_536);
    assert_eq!(limits.maximum_directories, 8_192);
    assert_eq!(limits.maximum_component_utf8_bytes, 255);
    assert_eq!(limits.maximum_relative_path_utf8_bytes, 4_096);
    assert_eq!(limits.maximum_depth, 64);
    assert_eq!(limits.maximum_file_bytes, 536_870_912);
    assert_eq!(limits.maximum_aggregate_file_bytes, 8_589_934_592);

    let contract = pin().workflow_effect_operation_key;
    assert_eq!(contract.preimage_format, "slingshot.workflow-effect-operation-key/1");
    assert_eq!(contract.key_prefix, "slingshot-workflow-effect-1-");
    assert_eq!(contract.maximum_input_utf8_bytes, 128);
    assert_eq!(contract.maximum_suffix_bytes, 15);
    assert_eq!(contract.maximum_key_bytes, 107);
    assert_eq!(contract.suffixes, vec![String::new(), "-backup-restore".to_owned()]);
}
