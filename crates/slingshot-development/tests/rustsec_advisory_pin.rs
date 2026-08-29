//! Assertions for the exact advisory-database snapshot.
//!
//! The verifier is proved against repositories this assertion builds, so every
//! refusal is observed rather than described. Nothing here reaches the network,
//! and nothing here reads a clock: an assertion that changed a Git author time
//! must still reach the same verdict.

use std::path::{Path, PathBuf};
use std::process::Command;

use slingshot_development::rustsec_advisory_pin::{
    self, AdvisoryDatabasePin, CHECKOUT_VARIABLE, EXACT_SNAPSHOT_LABEL, PIN_PATH, PinFailure,
};

/// Directory holding the pin documents this assertion evaluates.
const FIXTURE_DIRECTORY: &str = "crates/slingshot-development/tests/fixtures/rustsec-advisory-pin";

/// Pin documents the schema must refuse.
const REFUSED_PINS: &[&str] = &[
    "rejected-with-a-timestamp.toml",
    "rejected-with-an-age.toml",
    "rejected-with-a-freshness-flag.toml",
    "rejected-with-a-review-assertion.toml",
    "rejected-with-a-mutable-location.toml",
    "rejected-branch-instead-of-a-commit.toml",
    "rejected-short-identifier.toml",
    "rejected-other-format.toml",
];

/// An author time an assertion sets to prove no verdict depends on it.
const AUTHORED_AT: &str = "2001-02-03T04:05:06+00:00";

/// Another author time, years apart from the first.
const REAUTHORED_AT: &str = "2031-02-03T04:05:06+00:00";

/// Returns the workspace root directory.
fn workspace_root() -> PathBuf {
    slingshot_development::locate_workspace_root(Path::new(env!("CARGO_MANIFEST_DIR")))
        .expect("the development crate lives inside the workspace")
}

/// Reads one pin document owned by this assertion.
fn fixture(name: &str) -> String {
    let path = workspace_root().join(FIXTURE_DIRECTORY).join(name);
    std::fs::read_to_string(&path)
        .unwrap_or_else(|failure| panic!("{} could not be read: {failure}", path.display()))
}

/// Runs one Git command inside a checkout and requires it to succeed.
fn run_git(checkout: &Path, arguments: &[&str], authored_at: &str) {
    let produced = Command::new("git")
        .arg("-C")
        .arg(checkout)
        .args(arguments)
        .env("GIT_AUTHOR_NAME", "Probe")
        .env("GIT_AUTHOR_EMAIL", "probe@example.invalid")
        .env("GIT_COMMITTER_NAME", "Probe")
        .env("GIT_COMMITTER_EMAIL", "probe@example.invalid")
        .env("GIT_AUTHOR_DATE", authored_at)
        .env("GIT_COMMITTER_DATE", authored_at)
        .output()
        .expect("git runs");
    assert!(
        produced.status.success(),
        "{arguments:?}: {}",
        String::from_utf8_lossy(&produced.stderr)
    );
}

/// Builds one checkout with one commit, detached, and returns its directory.
fn build_checkout(directory: &tempfile::TempDir, origin: &str, authored_at: &str) -> PathBuf {
    let checkout = directory.path().to_path_buf();
    run_git(&checkout, &["init", "--quiet", "--initial-branch=main"], authored_at);
    run_git(&checkout, &["remote", "add", "origin", origin], authored_at);
    std::fs::write(checkout.join("advisory.md"), b"one advisory").expect("the advisory is written");
    run_git(&checkout, &["add", "advisory.md"], authored_at);
    run_git(&checkout, &["commit", "--quiet", "--message", "one advisory"], authored_at);
    run_git(&checkout, &["checkout", "--quiet", "--detach", "HEAD"], authored_at);
    checkout
}

/// Builds a checkout and the pin that names exactly it.
fn build_pinned(authored_at: &str) -> (tempfile::TempDir, AdvisoryDatabasePin) {
    let directory = tempfile::tempdir().expect("a temporary checkout is created");
    let checkout =
        build_checkout(&directory, "https://github.com/rustsec/advisory-db.git", authored_at);
    let snapshot = rustsec_advisory_pin::read_snapshot(&checkout).expect("the checkout reads");
    (directory, snapshot)
}

#[test]
fn the_schema_refuses_every_value_no_verifier_can_authenticate() {
    rustsec_advisory_pin::parse_pin(&fixture("accepted-well-formed.toml"))
        .expect("a well-formed pin is accepted");
    for name in REFUSED_PINS {
        assert!(rustsec_advisory_pin::parse_pin(&fixture(name)).is_err(), "{name} must be refused");
    }
}

#[test]
fn the_committed_pin_names_one_exact_snapshot() {
    let pin = rustsec_advisory_pin::parse_pin(
        &std::fs::read_to_string(workspace_root().join(PIN_PATH)).unwrap(),
    )
    .expect("the committed pin is well formed");
    assert!(rustsec_advisory_pin::is_full_identifier(&pin.commit));
    assert!(rustsec_advisory_pin::is_full_identifier(&pin.tree));
    assert_eq!(rustsec_advisory_pin::normalize_origin(&pin.origin), pin.origin);
    let committed =
        std::fs::read_to_string(workspace_root().join(PIN_PATH)).expect("the pin reads");
    for forbidden in ["timestamp", "age", "fresh", "review", "branch", "tag"] {
        let declared = committed
            .lines()
            .filter(|line| !line.trim_start().starts_with('#'))
            .any(|line| line.split('=').next().unwrap_or_default().contains(forbidden));
        assert!(!declared, "the committed pin declares a {forbidden} field");
    }
}

#[test]
fn an_exact_checkout_verifies_and_says_only_which_snapshot_it_is() {
    let (_directory, pin) = build_pinned(AUTHORED_AT);
    let checkout = _directory.path();
    let verified =
        rustsec_advisory_pin::verify(&pin, checkout).expect("the checkout is the snapshot");
    assert_eq!(verified.label, EXACT_SNAPSHOT_LABEL);
    assert_eq!(verified.commit, pin.commit);
    assert_eq!(verified.tree, pin.tree);
    let rendered = serde_json::to_string(&verified).expect("the result renders");
    for absent in ["time", "age", "fresh", "current"] {
        assert!(!rendered.contains(absent), "{rendered} claims something about {absent}");
    }
}

#[test]
fn a_wrong_origin_commit_or_tree_is_refused() {
    let (directory, pin) = build_pinned(AUTHORED_AT);
    let checkout = directory.path();
    let other_origin =
        AdvisoryDatabasePin { origin: "https://example.invalid/other".to_owned(), ..pin.clone() };
    assert!(matches!(
        rustsec_advisory_pin::verify(&other_origin, checkout),
        Err(PinFailure::Mismatch { field: "origin", .. })
    ));
    let other_commit = AdvisoryDatabasePin { commit: "b".repeat(pin.commit.len()), ..pin.clone() };
    assert!(matches!(
        rustsec_advisory_pin::verify(&other_commit, checkout),
        Err(PinFailure::Mismatch { field: "commit", .. })
    ));
    let other_tree = AdvisoryDatabasePin { tree: "c".repeat(pin.tree.len()), ..pin };
    assert!(matches!(
        rustsec_advisory_pin::verify(&other_tree, checkout),
        Err(PinFailure::Mismatch { field: "tree", .. })
    ));
}

#[test]
fn a_checkout_that_is_dirty_untracked_attached_or_absent_is_refused() {
    let (directory, pin) = build_pinned(AUTHORED_AT);
    let checkout = directory.path().to_path_buf();

    std::fs::write(checkout.join("advisory.md"), b"changed").expect("the advisory changes");
    assert!(matches!(
        rustsec_advisory_pin::verify(&pin, &checkout),
        Err(PinFailure::UnusableState(_))
    ));
    std::fs::write(checkout.join("advisory.md"), b"one advisory")
        .expect("the advisory is restored");
    rustsec_advisory_pin::verify(&pin, &checkout).expect("a restored checkout verifies");

    std::fs::write(checkout.join("stray.md"), b"stray").expect("a stray path is created");
    assert!(matches!(
        rustsec_advisory_pin::verify(&pin, &checkout),
        Err(PinFailure::UnusableState(_))
    ));
    std::fs::remove_file(checkout.join("stray.md")).expect("the stray path is removed");

    run_git(&checkout, &["checkout", "--quiet", "main"], AUTHORED_AT);
    assert!(matches!(
        rustsec_advisory_pin::verify(&pin, &checkout),
        Err(PinFailure::UnusableState(_))
    ));

    let absent = tempfile::tempdir().expect("a temporary directory is created");
    assert!(matches!(
        rustsec_advisory_pin::verify(&pin, absent.path()),
        Err(PinFailure::NotARepository { .. })
    ));
}

#[test]
fn an_author_time_years_later_reaches_the_same_verdict() {
    let (first_directory, first) = build_pinned(AUTHORED_AT);
    let (second_directory, second) = build_pinned(REAUTHORED_AT);
    assert_ne!(first.commit, second.commit, "an author time changes the commit identifier");
    assert_eq!(first.tree, second.tree, "an author time never changes the content tree");
    rustsec_advisory_pin::verify(&first, first_directory.path())
        .expect("the first snapshot verifies");
    rustsec_advisory_pin::verify(&second, second_directory.path())
        .expect("the second snapshot verifies");
    assert!(
        rustsec_advisory_pin::verify(&first, second_directory.path()).is_err(),
        "a snapshot only ever proves itself"
    );
}

#[test]
fn the_review_command_proposes_bytes_without_changing_or_claiming_anything() {
    let (directory, snapshot) = build_pinned(AUTHORED_AT);
    let before = std::fs::read(directory.path().join("advisory.md")).expect("the advisory reads");
    let proposed = rustsec_advisory_pin::propose_pin(&snapshot);
    assert!(proposed.contains(&snapshot.commit));
    assert!(proposed.contains(&snapshot.tree));
    let declared: String =
        proposed.lines().filter(|line| !line.trim_start().starts_with('#')).collect();
    for absent in ["fresh", "age", "reviewed", "current"] {
        assert!(!declared.contains(absent), "{declared} declares something about {absent}");
    }
    rustsec_advisory_pin::parse_pin(&proposed).expect("the proposal is a pin the schema accepts");
    let after = std::fs::read(directory.path().join("advisory.md")).expect("the advisory reads");
    assert_eq!(before, after, "proposing a pin changes nothing in the candidate");
}

#[test]
fn the_verifier_reads_only_the_checkout_the_environment_names() {
    let produced = Command::new(slingshot_development::cargo_executable())
        .current_dir(workspace_root())
        .args(["run", "--locked", "--quiet", "--package", "slingshot-development", "--"])
        .arg("rustsec-advisory-pin")
        .env_remove(CHECKOUT_VARIABLE)
        .output()
        .expect("the repository command runs");
    assert!(!produced.status.success(), "an unnamed checkout is refused");
    let diagnostic = String::from_utf8_lossy(&produced.stderr);
    assert!(diagnostic.contains(CHECKOUT_VARIABLE), "{diagnostic}");
    assert!(produced.stdout.is_empty(), "a refused verification writes no result");
}
