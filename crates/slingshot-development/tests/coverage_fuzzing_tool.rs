//! Which fuzzing tool this repository uses, and what it takes to be believed.
//!
//! A tool that fuzzes this product decides which inputs the product is ever
//! tried against, so a bundle is accepted on what it was built from rather than
//! on what it prints when asked its version. Every refusal here is a way a
//! different tool could have answered to the same description.
//!
//! Building one needs the network, which this suite does not use. What it
//! checks is the pin, the schema, the verifier, and the two scripts - which is
//! everything that decides whether a built bundle may be believed.

use std::path::PathBuf;

use serde_json::Value;
use slingshot_development::coverage_fuzzing_tool::{
    BUNDLE_FORMAT, BUNDLE_MANIFEST, BUNDLE_VARIABLE, BundleRefusal, COMMIT_CHARACTERS, PIN_PATH,
    SCHEMA_PATH, parse_pin, verified,
};

/// Where the fixtures live.
const FIXTURES: &str = "tests/fixtures/coverage-fuzzing-tool";

/// The host these cases build for.
const HOST: &str = "x86_64-unknown-linux-gnu";

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

/// Returns one fixture's text.
fn fixture(name: &str) -> String {
    std::fs::read_to_string(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(FIXTURES).join(name))
        .unwrap_or_else(|failure| panic!("{name} could not be read: {failure}"))
}

/// Returns a bundle directory holding `manifest` and the accepted executable.
fn bundle_holding(named: &str, manifest: &str) -> PathBuf {
    let root = std::env::temp_dir().join(format!("fuzz-bundle-{named}-{}", std::process::id()));
    std::fs::remove_dir_all(&root).ok();
    std::fs::create_dir_all(&root).expect("the bundle directory is created");
    std::fs::write(root.join(BUNDLE_MANIFEST), manifest).expect("the manifest is written");
    let executable = fixture("accepted-executable");
    std::fs::write(root.join("cargo-fuzz"), executable).expect("the executable is written");
    root
}

#[test]
fn the_pin_names_one_repository_one_full_commit_and_two_toolchains() {
    let held = parse_pin(&read_repository_file(PIN_PATH)).expect("the pin parses");
    assert_eq!(held.repository, "https://github.com/rust-fuzz/cargo-fuzz");
    assert_eq!(held.commit.len(), COMMIT_CHARACTERS);
    assert_eq!(held.binary, "cargo-fuzz");
    assert!(held.fuzz_toolchain.starts_with("nightly-"), "a fuzz target needs a dated nightly");
    assert!(!held.build_toolchain.starts_with("nightly"), "the tool itself is built on stable");
    assert!(held.policy.requires_locked_resolution);
    assert!(held.policy.requires_checksums);
    assert!(!held.policy.allows_git_dependencies);
    assert!(!held.policy.allows_source_replacement);
}

#[test]
fn the_dated_nightly_the_pin_names_is_the_one_the_fuzz_workspace_uses() {
    let held = parse_pin(&read_repository_file(PIN_PATH)).expect("the pin parses");
    let toolchain = read_repository_file("fuzz/rust-toolchain.toml");
    assert!(
        toolchain.contains(&held.fuzz_toolchain),
        "the fuzz workspace builds with another toolchain than the one pinned"
    );
}

#[test]
fn a_shortened_commit_is_not_a_commit() {
    let committed = read_repository_file(PIN_PATH);
    let shortened = committed.replace("1b34938413a104856042376b285c8d1c1e11b098", "1b34938");
    assert_eq!(
        parse_pin(&shortened),
        Err(BundleRefusal::NotThePinnedTool("the commit".to_owned())),
        "a shortened commit names a prefix, and a prefix can name two things"
    );
}

#[test]
fn a_bundle_built_from_the_pinned_source_is_accepted_and_answers_by_path() {
    let pin = parse_pin(&read_repository_file(PIN_PATH)).expect("the pin parses");
    let bundle = bundle_holding("accepted", &fixture("accepted-bundle.json"));
    let executable = verified(&bundle, &pin, HOST).expect("this bundle is the pinned tool");
    assert_eq!(executable, bundle.join(&pin.binary));
    assert!(executable.is_absolute(), "a consumer receives a path rather than a search");
    std::fs::remove_dir_all(&bundle).ok();
}

/// One declared refusal.
#[derive(Debug, serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct Refusal {
    /// What it is called.
    name: String,
    /// The manifest that is refused.
    manifest: Value,
}

#[test]
fn every_declared_refusal_is_refused_rather_than_run() {
    let pin = parse_pin(&read_repository_file(PIN_PATH)).expect("the pin parses");
    let declared: Vec<Refusal> = fixture("refusals.jsonl")
        .lines()
        .filter(|line| !line.trim().is_empty() && !line.starts_with('#'))
        .map(|line| serde_json::from_str(line).expect("every refusal reads"))
        .collect();
    assert!(!declared.is_empty());
    for refusal in declared {
        let manifest = serde_json::to_string(&refusal.manifest).expect("it writes");
        let bundle = bundle_holding(&refusal.name, &manifest);
        let held = verified(&bundle, &pin, HOST);
        assert!(held.is_err(), "{} was accepted", refusal.name);
        std::fs::remove_dir_all(&bundle).ok();
    }
}

#[test]
fn a_bundle_with_no_manifest_at_all_is_refused_before_anything_else() {
    let pin = parse_pin(&read_repository_file(PIN_PATH)).expect("the pin parses");
    let root = std::env::temp_dir().join(format!("fuzz-bundle-empty-{}", std::process::id()));
    std::fs::remove_dir_all(&root).ok();
    std::fs::create_dir_all(&root).expect("the directory is created");
    assert!(matches!(verified(&root, &pin, HOST), Err(BundleRefusal::Unreadable(_))));
    std::fs::remove_dir_all(&root).ok();
}

#[test]
fn the_schema_declares_every_member_a_manifest_carries() {
    let schema: Value =
        serde_json::from_str(&read_repository_file(SCHEMA_PATH)).expect("the schema reads");
    let required: Vec<&str> = schema["required"]
        .as_array()
        .expect("the schema names what is required")
        .iter()
        .filter_map(Value::as_str)
        .collect();
    let accepted: Value =
        serde_json::from_str(&fixture("accepted-bundle.json")).expect("the manifest reads");
    for member in &required {
        assert!(!accepted[member].is_null(), "the accepted manifest omits {member}");
    }
    assert_eq!(schema["properties"]["format"]["const"].as_str(), Some(BUNDLE_FORMAT));
    assert_eq!(schema["additionalProperties"].as_bool(), Some(false));
}

#[test]
fn the_two_scripts_take_what_they_are_given_and_find_nothing_for_themselves() {
    let verify = read_repository_file("scripts/verify_coverage_fuzzing_tool");
    assert!(verify.contains("finds none for itself"), "the verifier is given its bundle");
    assert!(verify.contains("is not an absolute path"), "and an absolute one");
    assert!(!verify.contains("PATH="), "and searches no path");

    let prepare = read_repository_file("scripts/prepare_coverage_fuzzing_tool");
    assert!(
        prepare.contains("this command reaches the network"),
        "the one command that reaches the network says so"
    );
    assert!(
        prepare.contains("already exists, and this command writes only into a new directory"),
        "and refuses to write into somewhere that already holds something"
    );
}

#[test]
fn a_consumer_receives_the_bundle_in_the_one_variable_that_names_it() {
    let wrapper = read_repository_file("scripts/run_fuzz_target");
    assert!(wrapper.contains(BUNDLE_VARIABLE), "a fuzz run is given a verified bundle");
    assert!(
        !wrapper.contains("cargo fuzz "),
        "and reaches the executable through the verifier rather than by name"
    );
}
