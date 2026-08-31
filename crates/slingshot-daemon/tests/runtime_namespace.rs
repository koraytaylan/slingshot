//! Naming one runtime namespace, over two roots that answer to different lifetimes.
//!
//! The digest vectors were computed outside this workspace, so agreement with
//! them is evidence about the rule rather than the implementation agreeing with
//! itself. Two of the legal vectors escape to the same readable text on purpose:
//! that pair is what proves the digest, and not the readable part, is what tells
//! two namespaces apart.

use std::collections::BTreeSet;

use serde_json::Value;
use slingshot_daemon::runtime_namespace::{NamespaceFailure, RuntimeNamespace, readable_component};
use slingshot_local_protocol::foundation_contract::FoundationContract;

/// Namespace vectors this test reads.
const VECTORS: &str = include_str!("fixtures/runtime_namespace/vectors.jsonl");

/// Bytes a profile name may occupy, from the foundation contract.
const PROFILE_BYTES: usize = 128;

/// Bytes an environment name may occupy, from the foundation contract.
const ENVIRONMENT_BYTES: usize = 128;

/// Returns one row's string member.
fn text<'row>(row: &'row Value, member: &str) -> &'row str {
    row[member].as_str().unwrap_or_else(|| panic!("{member} is a string in {row}"))
}

/// Returns every row of the fixture.
fn rows() -> Vec<Value> {
    VECTORS
        .lines()
        .map(|line| serde_json::from_str(line).expect("every fixture line is one object"))
        .collect()
}

/// Returns a root inside `directory` for this daemon to create itself.
///
/// A temporary directory is made with whatever the process umask allows, which
/// on an ordinary account is readable by the group. Handing one straight to the
/// daemon would be handing it a root it is right to refuse, so the tests name a
/// child of it and let the daemon create that child with the protection it
/// requires - which is what happens in production too.
fn root(directory: &tempfile::TempDir, name: &str) -> std::path::PathBuf {
    directory.path().join(name)
}

/// Returns the namespace one pair names inside `root`.
fn named(
    root: &std::path::Path,
    profile: &str,
    environment: &str,
) -> Result<RuntimeNamespace, NamespaceFailure> {
    RuntimeNamespace::name(&FoundationContract::embedded(), root, profile, environment)
}

#[test]
fn every_vector_names_the_namespace_the_fixture_committed() {
    let directory = tempfile::tempdir().expect("a directory");
    let directory = root(&directory, "runtime");
    let vectors = rows();
    assert!(vectors.len() >= 14, "ordinary, Unicode, punctuation, bounds, and every escape");
    for row in &vectors {
        let named = named(&directory, text(row, "profile"), text(row, "environment"));
        let legal = row["legal"].as_bool().expect("a vector states whether it is legal");
        match (legal, named) {
            (true, Ok(namespace)) => {
                assert_eq!(namespace.digest(), text(row, "digest"), "{}", text(row, "note"));
                assert_eq!(namespace.key(), text(row, "key"), "{}", text(row, "note"));
            }
            (false, Err(_)) => (),
            (true, Err(failure)) => panic!("{}: refused as {failure}", text(row, "note")),
            (false, Ok(namespace)) => {
                panic!("{}: named {}", text(row, "note"), namespace.digest())
            }
        }
    }
}

#[test]
fn names_that_escape_alike_still_name_two_namespaces() {
    let directory = tempfile::tempdir().expect("a directory");
    let directory = root(&directory, "runtime");
    let held: Vec<Value> =
        rows().into_iter().filter(|row| row["legal"] == Value::Bool(true)).collect();

    let digests: BTreeSet<String> = held.iter().map(|row| text(row, "digest").to_owned()).collect();
    assert_eq!(digests.len(), held.len(), "every legal vector names its own namespace");

    let readable: Vec<String> = held
        .iter()
        .map(|row| {
            format!(
                "{}-{}",
                readable_component(text(row, "profile")),
                readable_component(text(row, "environment"))
            )
        })
        .collect();
    let distinct: BTreeSet<&String> = readable.iter().collect();
    assert!(
        distinct.len() < readable.len(),
        "the fixture deliberately holds two pairs that escape to one readable text"
    );

    let alike: Vec<&Value> = held
        .iter()
        .filter(|row| {
            readable_component(text(row, "profile")) == readable_component("production")
                && readable_component(text(row, "environment")) == readable_component("publish")
        })
        .collect();
    assert!(alike.len() >= 2, "and those two are in it");
    let one = named(&directory, text(alike[0], "profile"), text(alike[0], "environment"))
        .expect("a legal pair");
    let other = named(&directory, text(alike[1], "profile"), text(alike[1], "environment"))
        .expect("another legal pair");
    assert_ne!(one.digest(), other.digest(), "which name two namespaces all the same");
    assert_ne!(one.key(), other.key(), "and two directories");
}

#[test]
fn a_name_at_each_bound_is_legal_and_one_byte_further_is_not() {
    let held = tempfile::tempdir().expect("a directory");
    let directory = root(&held, "runtime");
    let profile = "a".repeat(PROFILE_BYTES);
    let environment = "b".repeat(ENVIRONMENT_BYTES);
    assert!(named(&directory, &profile, &environment).is_ok(), "both names at their bound");

    let over_profile = named(&directory, &"a".repeat(PROFILE_BYTES + 1), &environment);
    assert!(
        matches!(
            over_profile,
            Err(NamespaceFailure::TooLong { name: "profile", limit, .. }) if limit == PROFILE_BYTES
        ),
        "one byte further in the profile: {over_profile:?}"
    );
    let over_environment = named(&directory, &profile, &"b".repeat(ENVIRONMENT_BYTES + 1));
    assert!(
        matches!(
            over_environment,
            Err(NamespaceFailure::TooLong { name: "environment", limit, .. })
                if limit == ENVIRONMENT_BYTES
        ),
        "and one byte further in the environment: {over_environment:?}"
    );
}

#[test]
fn nothing_a_caller_types_reaches_a_path_component_on_its_own() {
    let runtime = tempfile::tempdir().expect("a directory");
    let state = tempfile::tempdir().expect("another directory");
    let (runtime, state) = (root(&runtime, "runtime"), root(&state, "state"));
    for attempt in ["../escape", "/absolute", "a/b", "a\\b", "a\0b", "\u{7f}delete"] {
        let refused = named(&runtime, attempt, "publish");
        assert!(refused.is_err(), "{attempt:?} cannot name a namespace: {refused:?}");
    }

    let namespace = named(&runtime, "prod uction", "pub..lish").expect("a legal pair");
    let paths = namespace.beneath(&state);
    for path in [
        paths.target_root().to_path_buf(),
        paths.database_path(),
        paths.artifact_root(),
        paths.maintenance_root(),
        paths.diagnostic_root(),
    ] {
        assert!(path.starts_with(&state), "{} escaped the state root", path.display());
        assert!(
            !path.components().any(|part| part == std::path::Component::ParentDir),
            "{} carries a parent marker",
            path.display()
        );
    }
    assert!(
        namespace.readiness_path().starts_with(&runtime),
        "and the readiness record stays under the runtime root"
    );
}

#[test]
fn two_namespaces_share_no_path_at_all() {
    let runtime_directory = tempfile::tempdir().expect("a directory");
    let state_directory = tempfile::tempdir().expect("another directory");
    let runtime = root(&runtime_directory, "runtime");
    let state = root(&state_directory, "state");
    let one = named(&runtime, "production", "publish").expect("a legal pair");
    let other = named(&runtime, "staging", "publish").expect("another legal pair");

    one.create_runtime_directory().expect("a runtime directory");
    other.create_runtime_directory().expect("the same runtime directory, shared by design");
    let held = one.beneath(&state);
    let neighbour = other.beneath(&state);
    held.create().expect("one target's state");
    neighbour.create().expect("another target's state");

    assert_ne!(held.target_root(), neighbour.target_root());
    assert_ne!(held.database_path(), neighbour.database_path());
    assert_ne!(held.artifact_root(), neighbour.artifact_root());
    assert_ne!(one.readiness_path(), other.readiness_path());
    assert_eq!(
        held.installation_record_path(),
        neighbour.installation_record_path(),
        "while one installation identity covers every target this user has"
    );
    assert!(held.database_path().parent().expect("a parent").exists(), "and both exist");
    assert!(neighbour.database_path().parent().expect("a parent").exists());
}

#[test]
fn replacing_the_runtime_root_is_a_new_login_and_loses_no_durable_state() {
    let state_directory = tempfile::tempdir().expect("a state root");
    let state = root(&state_directory, "state");
    let before = {
        let runtime_directory = tempfile::tempdir().expect("a runtime root");
        let runtime = root(&runtime_directory, "runtime");
        let namespace = named(&runtime, "production", "publish").expect("a legal pair");
        let paths = namespace.beneath(&state);
        paths.create().expect("one target's state");
        std::fs::write(paths.database_path(), b"durable").expect("a database that exists");
        (paths.database_path(), namespace.digest().to_owned())
    };

    let runtime_directory =
        tempfile::tempdir().expect("a second runtime root, as after a new login");
    let runtime = root(&runtime_directory, "runtime");
    let namespace = named(&runtime, "production", "publish").expect("the same pair");
    assert_eq!(namespace.digest(), before.1, "which names the same namespace");
    let paths = namespace.beneath(&state);
    assert_eq!(paths.database_path(), before.0, "and the same durable paths");
    assert_eq!(
        std::fs::read(paths.database_path()).expect("the database reads"),
        b"durable",
        "holding what the previous login left there"
    );
    assert!(
        !namespace.readiness_path().exists(),
        "while nothing from the previous session's runtime root came with it"
    );
}

#[test]
fn only_the_two_names_reach_the_digest() {
    let runtime_directory = tempfile::tempdir().expect("a runtime root");
    let elsewhere_directory = tempfile::tempdir().expect("another runtime root");
    let runtime = root(&runtime_directory, "runtime");
    let elsewhere = root(&elsewhere_directory, "runtime");
    let one = named(&runtime, "production", "publish").expect("a legal pair");
    let other = named(&elsewhere, "production", "publish").expect("the same pair");
    assert_eq!(
        one.digest(),
        other.digest(),
        "not even the root a namespace sits in changes what it is called"
    );
    assert_eq!(one.display(), "production/publish", "and the display value is the names alone");
    assert!(
        !one.key().contains(one.runtime_root().to_string_lossy().as_ref()),
        "a key carries no path"
    );
}

#[test]
fn a_state_root_that_anyone_can_reach_is_refused() {
    let state_directory = tempfile::tempdir().expect("a state root");
    let runtime_directory = tempfile::tempdir().expect("a runtime root");
    let state = root(&state_directory, "state");
    let runtime = root(&runtime_directory, "runtime");
    let namespace = named(&runtime, "production", "publish").expect("a legal pair");
    let paths = namespace.beneath(&state);
    paths.create().expect("one target's state");
    make_reachable_by_others(paths.target_root());

    let refused = paths.create();
    assert!(
        matches!(refused, Err(NamespaceFailure::RootNotPrivate { .. })),
        "state anyone can reach is not state this daemon will use: {refused:?}"
    );
}

/// Widens one directory's permissions so the namespace should refuse it.
#[cfg(unix)]
fn make_reachable_by_others(path: &std::path::Path) {
    use std::os::unix::fs::PermissionsExt as _;

    /// Owner everything, and read and traverse for everyone else.
    const WIDE_OPEN: u32 = 0o755;

    std::fs::set_permissions(path, std::fs::Permissions::from_mode(WIDE_OPEN))
        .expect("the permissions change");
}

/// Widens one directory's permissions so the namespace should refuse it.
#[cfg(not(unix))]
fn make_reachable_by_others(_path: &std::path::Path) {}
