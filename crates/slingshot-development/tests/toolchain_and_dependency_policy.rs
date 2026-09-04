//! What this workspace builds with, and what it is allowed to depend on.
//!
//! A declared minimum version nobody builds against is a claim rather than a
//! fact: the first dependency that raises its own minimum makes it false, and
//! nothing says so. So the declared minimum and the pinned toolchain are the
//! same value, and the check that builds at it is a script anybody can run.
//!
//! The dependency policy gets the same treatment. Every rule that matters -
//! locked resolution, no source replacement, one version of anything that
//! matters, an advisory database pinned to an exact snapshot - is asserted
//! against the files that carry it rather than described in prose.

use std::path::PathBuf;

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

/// Returns the value one key holds in a document, by its first appearance.
fn first_value(document: &str, key: &str) -> String {
    document
        .lines()
        .find_map(|line| line.strip_prefix(&format!("{key} = ")))
        .map(|held| held.trim().trim_matches('"').to_owned())
        .unwrap_or_else(|| panic!("no {key} is declared"))
}

#[test]
fn the_declared_minimum_version_is_the_one_this_workspace_builds_with() {
    let declared = first_value(&read_repository_file("Cargo.toml"), "rust-version");
    let pinned = first_value(&read_repository_file("rust-toolchain.toml"), "channel");
    assert_eq!(
        declared, pinned,
        "the workspace declares one version and builds with another, so one of them is untrue"
    );
}

#[test]
fn the_toolchain_carries_the_components_the_gate_actually_runs() {
    let toolchain = read_repository_file("rust-toolchain.toml");
    for component in ["clippy", "rustfmt"] {
        assert!(
            toolchain.contains(component),
            "the gate runs {component} and the pinned toolchain does not carry it"
        );
    }
}

#[test]
fn the_script_that_proves_the_minimum_builds_the_whole_workspace_locked() {
    let script = read_repository_file("scripts/check_minimum_supported_rust_version");
    assert!(script.contains("--locked"), "an unlocked build proves nothing about a minimum");
    assert!(script.contains("--workspace"), "and a partial one proves it for part of the product");
    assert!(script.contains("--all-targets"), "including the tests, which use the same language");
    assert!(
        script.contains("so one of them is untrue"),
        "and it refuses when the two values disagree rather than preferring one"
    );
}

#[test]
fn every_dependency_is_resolved_from_the_lockfile_and_from_no_replacement() {
    for (graph, manifest_path, lock_path) in
        [("product", "Cargo.toml", "Cargo.lock"), ("fuzz", "fuzz/Cargo.toml", "fuzz/Cargo.lock")]
    {
        let manifest = read_repository_file(manifest_path);
        assert!(
            !manifest.contains("[source."),
            "{graph}: a replaced source is a dependency nobody pinned"
        );
        assert!(
            !manifest.contains("[patch."),
            "{graph}: a patched one is a dependency nobody reviewed"
        );
        let lock = read_repository_file(lock_path);
        assert!(lock.contains("[[package]]"), "{graph}: the lockfile is committed");
        let unchecked = lock
            .split("[[package]]")
            .filter(|held| held.contains("source = \"registry+"))
            .filter(|held| !held.contains("checksum = "))
            .count();
        assert_eq!(
            unchecked, 0,
            "{graph}: a registry package without a checksum is bytes nobody authenticated"
        );
    }
}

#[test]
fn the_advisory_database_is_pinned_to_one_exact_snapshot() {
    let pin = read_repository_file("compatibility/rustsec-advisory-database.toml");
    assert!(pin.contains("commit"), "an advisory database without a commit is a moving target");
    let commit = first_value(&pin, "commit");
    assert_eq!(commit.len(), COMMIT_CHARACTERS, "a shortened commit can name two things");
    assert!(commit.chars().all(|held| held.is_ascii_digit() || ('a'..='f').contains(&held)));
}

/// How many characters a commit is named by.
const COMMIT_CHARACTERS: usize = 40;

#[test]
fn the_dependency_policy_refuses_what_it_says_it_refuses() {
    let policy = read_repository_file("deny.toml");
    assert!(policy.contains("yanked = \"deny\""), "a withdrawn dependency is still depended on");
    assert!(
        policy.contains("wildcards = \"deny\""),
        "a dependency without a version is any version"
    );
    assert!(
        policy.contains("unknown-registry = \"deny\""),
        "and one from anywhere is from anywhere"
    );
    assert!(policy.contains("unknown-git = \"deny\""));
    assert!(policy.contains("[licenses]"), "and nothing about what may be depended on legally");
    assert!(
        policy.contains("ignore = []"),
        "an advisory nobody has to act on is an advisory nobody acts on"
    );
}

#[test]
fn the_quality_gate_runs_the_minimum_version_check() {
    let gate = read_repository_file("scripts/quality");
    assert!(
        gate.contains("check_minimum_supported_rust_version"),
        "a check nothing runs is a check nobody makes"
    );
}

#[test]
fn the_fuzz_graph_has_one_dated_toolchain_and_the_same_offline_policy() {
    let toolchain = read_repository_file("fuzz/rust-toolchain.toml");
    assert!(toolchain.contains("channel = \"nightly-"), "the fuzz toolchain must not float");
    assert!(toolchain.contains("rust-src"), "the fuzz toolchain records its required input");
    let gate = read_repository_file("scripts/quality");
    assert!(gate.contains("--manifest-path fuzz/Cargo.toml"), "the fuzz graph bypasses cargo-deny");
    assert!(
        gate.contains("cargo +\"$fuzz_toolchain\" deny"),
        "the fuzz graph bypasses its pinned toolchain"
    );
    assert!(gate.contains("--offline"), "dependency policy may not fetch either graph");
    assert!(gate.contains("--disable-fetch"), "dependency policy may not discover either graph");
    assert!(
        gate.contains("locked-graphs.before"),
        "the gate does not retain the pre-policy graph identity"
    );
    assert!(
        gate.contains("locked-graphs.after"),
        "the gate does not compare the post-policy graph identity"
    );
}
