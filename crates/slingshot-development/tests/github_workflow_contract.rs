//! What the hosted workflows are allowed to be.
//!
//! A workflow runs with credentials on a machine nobody here owns, every day,
//! long after anybody read it. So the questions asked of it are asked by a
//! test rather than by a reviewer: is every action pinned to a commit somebody
//! else cannot move, does every job say which permissions it holds, does the
//! checkout leave its credential behind, and does any job hold a write
//! permission it has no business holding.
//!
//! The adapter is also held to adding nothing. A hosted job that ran a narrower
//! gate than a developer runs would be reporting on something other than what a
//! change is held to, so the workflow invokes the repository-local commands and
//! the test checks that it does.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use serde_yaml_ng::Value;
use slingshot_development::github_automation_authority::{AUTHORITY_PATH, parse_authority};

/// The workflows this repository publishes.
const WORKFLOWS: &[&str] =
    &[".github/workflows/quality.yml", ".github/workflows/platform-runtime.yml"];

/// How many characters a full commit is written in.
const FULL_COMMIT_CHARACTERS: usize = 40;

/// The permission every job holds, and the only one most may.
const READ_CONTENT: &str = "read";

/// Returns the workspace root.
fn workspace_root() -> PathBuf {
    slingshot_development::locate_workspace_root(Path::new(env!("CARGO_MANIFEST_DIR")))
        .expect("the development crate lives inside the workspace")
}

/// Returns one repository file's text.
fn read_repository_file(relative: &str) -> String {
    let path = workspace_root().join(relative);
    std::fs::read_to_string(&path)
        .unwrap_or_else(|failure| panic!("{} could not be read: {failure}", path.display()))
}

/// Returns one workflow, parsed.
fn workflow(relative: &str) -> Value {
    serde_yaml_ng::from_str(&read_repository_file(relative))
        .unwrap_or_else(|failure| panic!("{relative} is not a workflow: {failure}"))
}

/// Returns every job one workflow declares, by name.
fn jobs(document: &Value) -> Vec<(String, Value)> {
    document["jobs"]
        .as_mapping()
        .expect("a workflow declares jobs")
        .iter()
        .map(|(name, job)| (name.as_str().unwrap_or_default().to_owned(), job.clone()))
        .collect()
}

/// Returns every step one job declares.
fn steps(job: &Value) -> Vec<Value> {
    job["steps"].as_sequence().cloned().unwrap_or_default()
}

#[test]
fn every_workflow_this_repository_publishes_is_one_the_contract_covers() {
    let published: BTreeSet<String> = std::fs::read_dir(workspace_root().join(".github/workflows"))
        .expect("the workflow directory reads")
        .filter_map(Result::ok)
        .map(|entry| format!(".github/workflows/{}", entry.file_name().to_string_lossy()))
        .collect();
    let covered: BTreeSet<String> = WORKFLOWS.iter().map(|held| (*held).to_owned()).collect();
    assert_eq!(published, covered, "a workflow exists that no assertion holds to anything");
    let authority = parse_authority(&read_repository_file(AUTHORITY_PATH)).expect("it parses");
    for relative in WORKFLOWS {
        assert!(
            relative.starts_with(authority.workflow_root.as_str()),
            "{relative} is outside the root the authority declares"
        );
    }
}

#[test]
fn every_action_is_pinned_to_a_commit_nobody_else_can_move() {
    for relative in WORKFLOWS {
        for (name, job) in jobs(&workflow(relative)) {
            for step in steps(&job) {
                let Some(uses) = step["uses"].as_str() else {
                    continue;
                };
                let (_, reference) =
                    uses.split_once('@').unwrap_or_else(|| panic!("{relative}/{name}: {uses}"));
                assert_eq!(
                    reference.len(),
                    FULL_COMMIT_CHARACTERS,
                    "{relative}/{name}: {uses} is not pinned to a full commit"
                );
                assert!(
                    reference.chars().all(|held| held.is_ascii_hexdigit()),
                    "{relative}/{name}: {uses} is not a commit"
                );
            }
        }
    }
}

#[test]
fn no_checkout_leaves_its_credential_where_a_later_step_can_read_it() {
    let mut checkouts = 0_usize;
    for relative in WORKFLOWS {
        for (name, job) in jobs(&workflow(relative)) {
            for step in steps(&job) {
                let Some(uses) = step["uses"].as_str() else {
                    continue;
                };
                if !uses.starts_with("actions/checkout@") {
                    continue;
                }
                checkouts += 1;
                assert_eq!(
                    step["with"]["persist-credentials"].as_bool(),
                    Some(false),
                    "{relative}/{name}: this checkout persists its credential"
                );
            }
        }
    }
    assert!(checkouts > 0, "the workflows check this repository out");
}

#[test]
fn every_job_says_which_permissions_it_holds_and_holds_no_more() {
    for relative in WORKFLOWS {
        let document = workflow(relative);
        assert_eq!(
            document["permissions"]["contents"].as_str(),
            Some(READ_CONTENT),
            "{relative}: the workflow default is read-only content"
        );
        for (name, job) in jobs(&document) {
            let permissions = job["permissions"]
                .as_mapping()
                .unwrap_or_else(|| panic!("{relative}/{name} declares no permissions of its own"));
            for (held, value) in permissions {
                let held = held.as_str().unwrap_or_default();
                assert_eq!(
                    (held, value.as_str()),
                    ("contents", Some(READ_CONTENT)),
                    "{relative}/{name} holds {held}, which no ordinary job needs"
                );
            }
        }
    }
}

#[test]
fn the_hosted_gate_runs_the_repository_local_commands_rather_than_its_own() {
    let quality = read_repository_file(".github/workflows/quality.yml");
    for command in [
        "scripts/quality",
        "scripts/checkout_pinned_advisory_database",
        "scripts/check_finite_state_machine_compatibility",
        "github-automation-authority",
    ] {
        assert!(quality.contains(command), "the hosted gate does not run {command}");
    }
    for narrower in ["cargo test --lib", "--exact", "--skip"] {
        assert!(
            !quality.contains(narrower),
            "a hosted gate that ran {narrower} would report on a narrower thing"
        );
    }
}

#[test]
fn every_script_the_workflows_name_is_committed() {
    for relative in WORKFLOWS {
        for line in read_repository_file(relative).lines() {
            for word in line.split_whitespace() {
                let named = word.trim_matches('"');
                if !named.starts_with("scripts/") {
                    continue;
                }
                assert!(
                    workspace_root().join(named).is_file(),
                    "{relative} runs {named}, which is not committed"
                );
            }
        }
    }
}

#[test]
fn the_quality_workflow_proves_the_pinned_snapshot_and_claims_no_freshness() {
    let quality = read_repository_file(".github/workflows/quality.yml");
    assert!(
        quality.contains("the exact pinned advisory snapshot"),
        "the job says which snapshot it proves"
    );
    for freshness in ["latest", "--update", "fresh", "up to date"] {
        assert!(
            !quality.contains(freshness),
            "a gate that advanced the snapshot would prove a different one: it names {freshness}"
        );
    }
}

#[test]
fn the_compatibility_job_runs_on_one_row_and_runs_the_unchanged_gate() {
    let document = workflow(".github/workflows/quality.yml");
    let authority = parse_authority(&read_repository_file(AUTHORITY_PATH)).expect("it parses");
    let selected = authority
        .row
        .iter()
        .find(|row| row.finite_state_machine)
        .expect("one row is the compatible one");
    let named = jobs(&document);
    let (_, job) = named
        .iter()
        .find(|(name, _)| name == "pinned-fsm-compatibility")
        .expect("the compatibility job is declared");
    assert_eq!(
        job["runs-on"].as_str(),
        Some(selected.runner_selector.as_str()),
        "the job runs on the one row the owner declared compatible"
    );
    let invocation = steps(job)
        .iter()
        .filter_map(|step| step["run"].as_str().map(str::to_owned))
        .find(|run| run.contains("check_finite_state_machine_compatibility"))
        .expect("it invokes the gate");
    assert!(
        invocation.contains("--finite-state-machine-source"),
        "with the source option the gate declares"
    );
    for narrower in ["--test", "--exact", "cargo test"] {
        assert!(!invocation.contains(narrower), "and substitutes nothing narrower");
    }
}

#[test]
fn the_native_matrix_is_exactly_the_rows_the_authority_maps() {
    let document = workflow(".github/workflows/platform-runtime.yml");
    let authority = parse_authority(&read_repository_file(AUTHORITY_PATH)).expect("it parses");
    let named = jobs(&document);
    let (_, job) = named.first().expect("the workflow declares a native job");
    let included =
        job["strategy"]["matrix"]["include"].as_sequence().expect("the matrix names its rows");
    let mapped: BTreeSet<(String, String)> =
        authority.row.iter().map(|row| (row.triple.clone(), row.runner_selector.clone())).collect();
    let declared: BTreeSet<(String, String)> = included
        .iter()
        .map(|row| {
            (
                row["triple"].as_str().unwrap_or_default().to_owned(),
                row["runner"].as_str().unwrap_or_default().to_owned(),
            )
        })
        .collect();
    assert_eq!(declared, mapped, "the matrix and the authority disagree about the rows");
    assert_eq!(
        job["strategy"]["fail-fast"].as_bool(),
        Some(false),
        "one row failing hides nothing about the others"
    );
}

#[test]
fn no_workflow_interpolates_a_caller_controlled_value_into_a_shell() {
    for relative in WORKFLOWS {
        for (name, job) in jobs(&workflow(relative)) {
            for step in steps(&job) {
                let Some(run) = step["run"].as_str() else {
                    continue;
                };
                assert!(
                    !run.contains("${{"),
                    "{relative}/{name}: an expression in a shell is somebody else's text \
                     becoming this repository's command"
                );
            }
        }
    }
}
