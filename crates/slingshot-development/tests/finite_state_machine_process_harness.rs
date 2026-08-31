//! What a multi-process scenario is allowed to see, and what it must leave alone.
//!
//! The processes themselves need executables this environment does not always
//! have, so the suite is written in two halves. Everything about isolation,
//! supply, and refusal is decided here and always runs. The scenarios that need
//! the pinned executor run when it is supplied and say plainly when it is not,
//! rather than passing quietly - a compatibility suite that reports success
//! having run nothing is worse than one that fails.

use std::path::PathBuf;

use slingshot_development::finite_state_machine_process_harness::{
    EXECUTOR_VARIABLE, HarnessRefusal, PRODUCT_VARIABLE, REMOVED_VARIABLES, Role, SENTINEL_CONTENT,
    SENTINEL_FILE, ScenarioRoots, closed_environment, every_role_supplied, is_removed, supplied,
};

/// Where the roles live.
const ROLE_FIXTURE: &str = "tests/fixtures/finite-state-machine-process-harness/roles.jsonl";

/// One declared role.
#[derive(Debug, serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct DeclaredRole {
    /// What it is called.
    name: String,
    /// Which variable supplies it.
    variable: String,
    /// What part it plays.
    why: String,
}

/// Returns every declared role.
fn declared_roles() -> Vec<DeclaredRole> {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(ROLE_FIXTURE);
    let text = std::fs::read_to_string(&path)
        .unwrap_or_else(|failure| panic!("{} could not be read: {failure}", path.display()));
    text.lines()
        .filter(|line| !line.trim().is_empty() && !line.starts_with('#'))
        .map(|line| serde_json::from_str(line).expect("every role reads"))
        .collect()
}

/// Returns a temporary directory this case owns.
fn temporary_root(named: &str) -> PathBuf {
    let root = std::env::temp_dir().join(format!("fsm-{named}-{}", std::process::id()));
    std::fs::remove_dir_all(&root).ok();
    std::fs::create_dir_all(&root).expect("the temporary root is created");
    root
}

#[test]
fn every_declared_role_is_supplied_by_a_variable_and_never_found() {
    let declared = declared_roles();
    assert_eq!(declared.len(), EVERY_ROLE.len(), "the fixture and the build name the same roles");
    for (row, role) in declared.iter().zip(EVERY_ROLE) {
        assert_eq!(row.variable, role.variable(), "{} is supplied elsewhere", row.name);
        assert!(!row.why.is_empty(), "{} says what part it plays", row.name);
    }
    assert_eq!(Role::Executor.variable(), EXECUTOR_VARIABLE);
    assert_eq!(Role::ProtocolServer.variable(), PRODUCT_VARIABLE);
    assert_eq!(Role::Daemon.variable(), PRODUCT_VARIABLE);
}

/// Every role a scenario may run.
const EVERY_ROLE: &[Role] = &[Role::Executor, Role::ProtocolServer, Role::Daemon];

#[test]
fn a_role_nobody_supplied_refuses_rather_than_finding_something() {
    let unsupplied = supplied(Role::Executor);
    match unsupplied {
        Err(HarnessRefusal::Unsupplied(named)) => assert_eq!(named, EXECUTOR_VARIABLE),
        _ => {
            assert!(
                std::env::var(EXECUTOR_VARIABLE).is_ok(),
                "a refusal that is not about supply means something was supplied"
            );
        }
    }
}

#[test]
fn a_supplied_path_that_names_nothing_runnable_is_refused_by_name() {
    let root = temporary_root("unusable");
    let missing = root.join("nothing-here");
    let refusal =
        slingshot_test_support::finite_state_machine_executable::FiniteStateMachineExecutable::at(
            missing.clone(),
        )
        .expect_err("nothing is there");
    assert!(format!("{refusal}").contains("regular file"), "{refusal}");
    let relative =
        slingshot_test_support::finite_state_machine_executable::FiniteStateMachineExecutable::at(
            "fsm",
        )
        .expect_err("a relative path depends on where it is read");
    assert!(format!("{relative}").contains("absolute"), "{relative}");
    std::fs::remove_dir_all(&root).ok();
}

#[test]
fn a_scenario_acts_under_a_root_it_made_and_leaves_the_other_one_alone() {
    let root = temporary_root("roots");
    let roots = ScenarioRoots::create(&root).expect("the roots are created");
    assert!(roots.private().is_dir());
    assert!(roots.decoy().join(SENTINEL_FILE).is_file());
    roots.require_untouched().expect("nothing has touched the decoy yet");

    std::fs::write(roots.private().join("whatever-a-scenario-writes"), "held")
        .expect("a scenario writes under its own root");
    roots.require_untouched().expect("writing under the private root touches nothing else");

    std::fs::write(roots.decoy().join("something-else"), "held").expect("the decoy is writable");
    assert_eq!(
        roots.require_untouched(),
        Err(HarnessRefusal::ProductionRootTouched),
        "a scenario that reached the production root proved something about this machine"
    );
    std::fs::remove_dir_all(&root).ok();
}

#[test]
fn a_sentinel_that_was_read_is_still_the_sentinel_that_was_written() {
    let root = temporary_root("sentinel");
    let roots = ScenarioRoots::create(&root).expect("the roots are created");
    let held = std::fs::read_to_string(roots.decoy().join(SENTINEL_FILE)).expect("it is readable");
    assert_eq!(held, SENTINEL_CONTENT);
    roots.require_untouched().expect("reading is not touching");
    std::fs::remove_dir_all(&root).ok();
}

#[test]
fn every_child_sees_a_built_environment_and_none_of_the_removed_variables() {
    let root = temporary_root("environment");
    let roots = ScenarioRoots::create(&root).expect("the roots are created");
    let held = closed_environment(&roots);
    assert_eq!(held.get("HOME").map(String::as_str), roots.private().to_str());
    for named in REMOVED_VARIABLES {
        assert!(!held.contains_key(*named), "{named} reached a child");
        assert!(is_removed(named));
    }
    assert!(!is_removed("HOME"), "what a scenario sets is not what it removes");
    std::fs::remove_dir_all(&root).ok();
}

#[test]
fn the_scenarios_that_need_real_processes_say_whether_they_ran() {
    let ran = every_role_supplied(EVERY_ROLE);
    if ran {
        for role in EVERY_ROLE {
            let path = supplied(*role).expect("every role was supplied");
            assert!(path.is_absolute(), "{role:?} was supplied a relative path");
        }
        return;
    }
    assert!(
        supplied(Role::Executor).is_err() || supplied(Role::ProtocolServer).is_err(),
        "either every role is supplied or at least one refuses, and never neither"
    );
}
