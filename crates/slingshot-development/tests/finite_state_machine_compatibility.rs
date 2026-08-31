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
    PinRefusal, SeedLimits, SeedRefusal, canonical_origin, verify_seed,
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

/// How many files the seeds that are counted by file hold.
const COUNTED_FILES: u64 = 2;

/// How many directories a seed with one file two below the root holds.
const NESTED_DIRECTORIES: u64 = 3;

/// How far below the root that file sits.
const NESTED_DEPTH: u64 = 3;

/// How many bytes the deliberately long names in these seeds hold.
const NAME_BYTES: usize = 32;

/// How many bytes the two halves of the aggregate seed hold together.
const TOGETHER_BYTES: u64 = 10;

/// Returns a directory nothing else in this suite writes into.
fn seed_named(named: &str) -> PathBuf {
    let root = std::env::temp_dir().join(format!("seed-{named}-{}", std::process::id()));
    std::fs::remove_dir_all(&root).ok();
    std::fs::create_dir_all(&root).expect("the seed root is created");
    root
}

/// Writes one file into a seed, creating whatever it lives in.
fn write_into(seed: &std::path::Path, relative: &str, content: &str) {
    let path = seed.join(relative);
    std::fs::create_dir_all(path.parent().expect("a file has a parent"))
        .expect("the directory is created");
    std::fs::write(&path, content).expect("the file is written");
}

/// Returns the committed limits with one dimension tightened by `tighten`.
fn limits_with(tighten: impl FnOnce(&mut SeedLimits)) -> SeedLimits {
    let mut limits = pin().cargo_home_seed;
    tighten(&mut limits);
    limits
}

#[test]
fn an_ordinary_bounded_seed_is_accepted_and_counted() {
    let seed = seed_named("ordinary");
    write_into(&seed, "registry/cache/example/first.crate", "the first crate");
    write_into(&seed, "registry/cache/example/second.crate", "the second crate");
    let survey = verify_seed(&seed, &pin().cargo_home_seed).expect("this seed is ordinary");
    assert_eq!(survey.files, COUNTED_FILES);
    assert_eq!(survey.directories, NESTED_DIRECTORIES + 1, "the root, registry, cache, example");
    let written = "the first crate".len() as u64 * COUNTED_FILES + 1;
    assert_eq!(survey.aggregate_file_bytes, written, "the second crate is one byte longer");
    std::fs::remove_dir_all(&seed).ok();
}

#[test]
fn a_seed_holding_exactly_as_many_files_as_it_may_is_accepted_and_one_more_is_not() {
    let seed = seed_named("file-count");
    write_into(&seed, "first", "a");
    write_into(&seed, "second", "b");
    let exact = limits_with(|limits| limits.maximum_files = COUNTED_FILES);
    assert_eq!(
        verify_seed(&seed, &exact).expect("exactly as many is as many").files,
        COUNTED_FILES
    );
    let next = limits_with(|limits| limits.maximum_files = COUNTED_FILES - 1);
    assert_eq!(
        verify_seed(&seed, &next),
        Err(SeedRefusal::TooManyFiles { held: COUNTED_FILES, limit: COUNTED_FILES - 1 }),
        "the count is decided as it is reached, not after everything is read"
    );
    std::fs::remove_dir_all(&seed).ok();
}

#[test]
fn a_seed_holding_exactly_as_many_directories_as_it_may_is_accepted_and_one_more_is_not() {
    let seed = seed_named("directory-count");
    write_into(&seed, "one/two/held", "a");
    let exact = limits_with(|limits| limits.maximum_directories = NESTED_DIRECTORIES);
    let held = verify_seed(&seed, &exact).expect("the root and two more");
    assert_eq!(held.directories, NESTED_DIRECTORIES);
    let next = limits_with(|limits| limits.maximum_directories = NESTED_DIRECTORIES - 1);
    assert_eq!(
        verify_seed(&seed, &next),
        Err(SeedRefusal::TooManyDirectories {
            held: NESTED_DIRECTORIES,
            limit: NESTED_DIRECTORIES - 1
        })
    );
    std::fs::remove_dir_all(&seed).ok();
}

#[test]
fn a_seed_exactly_as_deep_as_it_may_be_is_accepted_and_one_deeper_is_not() {
    let seed = seed_named("depth");
    write_into(&seed, "one/two/held", "a");
    let exact = limits_with(|limits| limits.maximum_depth = NESTED_DEPTH);
    assert!(verify_seed(&seed, &exact).is_ok(), "the file sits that far below the root");
    let next = limits_with(|limits| limits.maximum_depth = NESTED_DEPTH - 1);
    let failure = verify_seed(&seed, &next).expect_err("one deeper than it may be");
    let expected =
        SeedRefusal::TooDeep { held: NESTED_DEPTH, limit: NESTED_DEPTH - 1, path: String::new() };
    assert_eq!(std::mem::discriminant(&failure), std::mem::discriminant(&expected), "{failure}");
    std::fs::remove_dir_all(&seed).ok();
}

#[test]
fn a_file_exactly_as_large_as_it_may_be_is_accepted_and_one_byte_more_is_not() {
    let seed = seed_named("file-bytes");
    let held = "0123456789";
    write_into(&seed, "held", held);
    let exact = limits_with(|limits| limits.maximum_file_bytes = held.len() as u64);
    assert!(verify_seed(&seed, &exact).is_ok());
    let next = limits_with(|limits| limits.maximum_file_bytes = held.len() as u64 - 1);
    let failure = verify_seed(&seed, &next).expect_err("one byte over");
    assert!(matches!(failure, SeedRefusal::FileTooLarge { .. }), "{failure}");
    std::fs::remove_dir_all(&seed).ok();
}

#[test]
fn files_holding_exactly_as_much_as_they_may_together_are_accepted_and_one_byte_more_is_not() {
    let seed = seed_named("aggregate");
    write_into(&seed, "first", "01234");
    write_into(&seed, "second", "56789");
    let together = TOGETHER_BYTES;
    let exact = limits_with(|limits| limits.maximum_aggregate_file_bytes = together);
    assert_eq!(
        verify_seed(&seed, &exact).expect("exactly together").aggregate_file_bytes,
        together
    );
    let next = limits_with(|limits| limits.maximum_aggregate_file_bytes = together - 1);
    assert_eq!(
        verify_seed(&seed, &next),
        Err(SeedRefusal::TooLargeAltogether { held: together, limit: together - 1 })
    );
    std::fs::remove_dir_all(&seed).ok();
}

#[test]
fn a_component_exactly_as_long_as_it_may_be_is_accepted_and_one_byte_longer_is_not() {
    let seed = seed_named("component");
    let named = "n".repeat(NAME_BYTES);
    write_into(&seed, &named, "a");
    let exact = limits_with(|limits| limits.maximum_component_utf8_bytes = named.len() as u64);
    assert!(verify_seed(&seed, &exact).is_ok());
    let next = limits_with(|limits| limits.maximum_component_utf8_bytes = named.len() as u64 - 1);
    let failure = verify_seed(&seed, &next).expect_err("one byte longer");
    assert!(matches!(failure, SeedRefusal::ComponentTooLong { .. }), "{failure}");
    std::fs::remove_dir_all(&seed).ok();
}

#[test]
fn a_relative_path_exactly_as_long_as_it_may_be_is_accepted_and_one_byte_longer_is_not() {
    let seed = seed_named("relative-path");
    write_into(&seed, "one/two", "a");
    let relative = "one/two".len() as u64;
    let exact = limits_with(|limits| limits.maximum_relative_path_utf8_bytes = relative);
    assert!(verify_seed(&seed, &exact).is_ok());
    let next = limits_with(|limits| limits.maximum_relative_path_utf8_bytes = relative - 1);
    let failure = verify_seed(&seed, &next).expect_err("one byte longer");
    assert!(matches!(failure, SeedRefusal::PathTooLong { .. }), "{failure}");
    std::fs::remove_dir_all(&seed).ok();
}

#[test]
fn a_seed_holding_anything_but_ordinary_files_and_directories_is_refused() {
    let seed = seed_named("not-ordinary");
    write_into(&seed, "held", "a");
    std::os::unix::fs::symlink(seed.join("held"), seed.join("also-held"))
        .expect("the link is made");
    let failure = verify_seed(&seed, &pin().cargo_home_seed).expect_err("a link is not a file");
    assert_eq!(
        failure,
        SeedRefusal::NotOrdinary("also-held".to_owned()),
        "a link is followed by whatever reads the seed, and it points wherever it likes"
    );
    std::fs::remove_dir_all(&seed).ok();
}

#[test]
fn one_path_breaking_several_rules_earns_the_first_of_them() {
    let seed = seed_named("precedence");
    let named = "n".repeat(NAME_BYTES);
    write_into(&seed, &format!("one/{named}"), "0123456789");
    let tightened = limits_with(|limits| {
        limits.maximum_component_utf8_bytes = 1;
        limits.maximum_relative_path_utf8_bytes = 1;
        limits.maximum_depth = 1;
        limits.maximum_file_bytes = 1;
    });
    let failure = verify_seed(&seed, &tightened).expect_err("it breaks four rules");
    assert!(
        matches!(failure, SeedRefusal::ComponentTooLong { .. }),
        "the first rule in the declared order decides, and this is {failure}"
    );
    std::fs::remove_dir_all(&seed).ok();
}

#[test]
fn two_paths_breaking_the_same_rule_are_decided_in_sorted_order() {
    let seed = seed_named("sorted");
    write_into(&seed, "aardvark", "0123456789");
    write_into(&seed, "zebra", "0123456789");
    let tightened = limits_with(|limits| limits.maximum_file_bytes = 1);
    let failure = verify_seed(&seed, &tightened).expect_err("both are too large");
    assert_eq!(
        failure.to_string(),
        SeedRefusal::FileTooLarge { held: 10, limit: 1, path: "aardvark".to_owned() }.to_string(),
        "the same seed earns the same diagnostic on every machine that walks it"
    );
    std::fs::remove_dir_all(&seed).ok();
}

#[test]
fn a_seed_that_is_not_there_is_refused_rather_than_treated_as_empty() {
    let seed = seed_named("absent");
    std::fs::remove_dir_all(&seed).ok();
    let failure = verify_seed(&seed, &pin().cargo_home_seed).expect_err("there is nothing to walk");
    assert!(matches!(failure, SeedRefusal::Unwalkable(_)), "{failure}");
}
