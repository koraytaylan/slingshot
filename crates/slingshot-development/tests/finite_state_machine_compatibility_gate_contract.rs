//! What the compatibility gate is, checked without running it.
//!
//! Running it needs the pinned external source. What can be checked here is
//! everything about the gate that could drift silently: that the inventory
//! names every integration target this plan has and each of them once, that
//! every named target exists, that the gate script actually runs each one, and
//! that it refuses in the places where reporting success would be worse than
//! failing.
//!
//! A gate that reported success having run nothing is the failure the whole
//! arrangement exists to prevent, so its refusals are asserted as carefully as
//! its steps.

use std::collections::BTreeSet;
use std::path::PathBuf;

/// Where the inventory lives.
const INVENTORY: &str = "compatibility/finite-state-machine-test-targets.toml";

/// Where the gate lives.
const GATE: &str = "scripts/check_finite_state_machine_compatibility";

/// The format the inventory declares.
const INVENTORY_FORMAT: &str = "slingshot.finite-state-machine-test-targets/1";

/// How many integration targets this plan has.
const EVERY_TARGET_COUNT: usize = 12;

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

/// The inventory, as it is committed.
#[derive(Debug, serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct Inventory {
    /// The format it declares.
    format: String,
    /// Every target, in order.
    target: Vec<Target>,
}

/// One integration target.
#[derive(Debug, serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct Target {
    /// Which suite.
    name: String,
    /// Which task owns it.
    task: String,
}

/// Returns the committed inventory.
fn inventory() -> Inventory {
    toml::from_str(&read_repository_file(INVENTORY)).expect("the inventory parses")
}

#[test]
fn the_inventory_names_every_target_once_and_declares_its_format() {
    let held = inventory();
    assert_eq!(held.format, INVENTORY_FORMAT);
    assert_eq!(held.target.len(), EVERY_TARGET_COUNT, "this plan has that many targets");
    let named: BTreeSet<&str> = held.target.iter().map(|target| target.name.as_str()).collect();
    assert_eq!(
        named.len(),
        held.target.len(),
        "a target is named twice, so evidence is counted twice"
    );
    let owning: BTreeSet<&str> = held.target.iter().map(|target| target.task.as_str()).collect();
    assert_eq!(owning.len(), held.target.len(), "two targets claim one owner");
}

#[test]
fn every_named_target_is_a_suite_that_exists_and_a_task_that_exists() {
    let tasks = workspace_root().join("docs/plans/0008-fsm-workflow-integration/tasks");
    for target in inventory().target {
        let suite = workspace_root()
            .join("crates/slingshot-development/tests")
            .join(format!("{}.rs", target.name));
        assert!(suite.is_file(), "{} names a suite that does not exist", target.name);
        let owned = std::fs::read_dir(&tasks)
            .expect("the tasks are committed")
            .filter_map(Result::ok)
            .any(|entry| entry.file_name().to_string_lossy().contains(&target.task));
        assert!(owned, "{} names a task that does not exist", target.task);
    }
}

#[test]
fn the_gate_runs_every_target_and_names_none_of_them_itself() {
    let gate = read_repository_file(GATE);
    assert!(
        gate.contains("--test \"$TARGET\""),
        "the gate runs what the inventory names rather than a list of its own"
    );
    assert!(
        gate.contains("--test \"$FIRST_TARGET\""),
        "and validates the manifest before it builds anything against it"
    );
    for target in inventory().target {
        assert!(
            !gate.contains(&target.name),
            "{} is named in the gate as well as the inventory, so the two can disagree",
            target.name
        );
    }
    assert!(!gate.contains("--tests"), "a package-wide run would let later plans enter this gate");
}

#[test]
fn the_gate_refuses_where_reporting_success_would_be_worse_than_failing() {
    let gate = read_repository_file(GATE);
    for refusal in [
        "this gate is given the pinned source explicitly and finds none for itself",
        "the inventory names no target, so this gate would prove nothing",
        "the build produced no executable where one was expected",
        "the pinned source did not build",
    ] {
        assert!(gate.contains(refusal), "the gate does not refuse: {refusal:?}");
    }
    assert!(gate.contains("set -eu"), "the gate stops at the first step that does not hold");
}

#[test]
fn a_supplied_seed_forces_frozen_offline_resolution_and_an_absent_one_says_so() {
    let gate = read_repository_file(GATE);
    assert!(gate.contains("--frozen --offline"), "a seed is only useful if it is the only source");
    assert!(
        gate.contains("dependency acquisition may use the network"),
        "an absent seed is recorded rather than assumed away"
    );
}

#[test]
fn the_gate_claims_no_provider_no_runner_and_no_provenance() {
    let gate = read_repository_file(GATE);
    for absent in ["github", "runs-on", "workflow_dispatch", "provenance", "reproducib"] {
        assert!(
            !gate.to_lowercase().contains(absent),
            "the gate mentions {absent}, which is a claim it does not establish"
        );
    }
}
