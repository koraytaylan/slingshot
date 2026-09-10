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
const WORKFLOWS: &[&str] = &[
    ".github/workflows/quality.yml",
    ".github/workflows/platform-runtime.yml",
    ".github/workflows/release.yml",
];

/// The one job that may hold a permission beyond reading content.
const ATTESTATION_JOB: &str = "release-binary-provenance";

/// The permissions that job alone may add.
const ATTESTATION_PERMISSIONS: &[&str] = &["attestations", "id-token"];

/// What a job holding one of those permissions writes.
const WRITE_PERMISSION: &str = "write";

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
                if held == "contents" {
                    assert_eq!(value.as_str(), Some(READ_CONTENT), "{relative}/{name}");
                    continue;
                }
                assert_eq!(
                    name, ATTESTATION_JOB,
                    "{relative}/{name} holds {held}, and one job alone may"
                );
                assert!(
                    ATTESTATION_PERMISSIONS.contains(&held),
                    "{relative}/{name} holds {held}, which is not one of the two"
                );
                assert_eq!(value.as_str(), Some(WRITE_PERMISSION), "{relative}/{name}/{held}");
            }
        }
    }
}

#[test]
fn the_hosted_gate_runs_the_repository_local_commands_rather_than_its_own() {
    let quality = read_repository_file(".github/workflows/quality.yml");
    for command in [
        "scripts/quality",
        "scripts/prepare_native_dependencies",
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
    assert!(
        document["on"].as_mapping().is_some_and(|triggers| triggers.contains_key("pull_request")),
        "every supported native row gates pull requests as well as pushes"
    );
    let native_steps = steps(job);
    let bootstrap_invocations: Vec<&str> = native_steps
        .iter()
        .filter_map(|step| step["run"].as_str())
        .filter(|run| run.contains("scripts/prepare_native_dependencies"))
        .collect();
    assert_eq!(
        bootstrap_invocations,
        vec!["scripts/prepare_native_dependencies"],
        "the native job seeds the exact graph before entering its offline gate"
    );
    let local_capture = read_repository_file("scripts/capture_release_gates");
    assert!(
        local_capture.contains("scripts/prepare_native_dependencies"),
        "the local release capture must seed the same graph before its offline gate"
    );
    let native_invocations: Vec<&str> = native_steps
        .iter()
        .filter_map(|step| step["run"].as_str())
        .filter(|run| run.contains("scripts/platform_quality"))
        .collect();
    assert_eq!(
        native_invocations,
        vec!["scripts/platform_quality"],
        "the native job invokes exactly the argument-free repository-local gate"
    );
    let gate = read_repository_file("scripts/platform_quality");
    for required in [
        "cargo check --locked --offline --workspace --all-targets --all-features",
        "cargo test --locked --offline --workspace --all-targets --all-features",
        "this gate takes no arguments",
    ] {
        assert!(gate.contains(required), "the native gate omits {required}");
    }
    for narrower in ["platform_runtime_contract", "--exact", "--skip"] {
        assert!(
            !native_invocations[0].contains(narrower),
            "the workflow substitutes the narrower {narrower} check"
        );
    }
    assert!(
        workspace_root()
            .join("crates/slingshot-configuration/tests/macos_native_sentinel.rs")
            .is_file(),
        "a macOS-only sentinel outside platform_runtime_contract is compiled by the gate"
    );
}

#[test]
fn native_rows_have_the_repository_gate_timeout_budget() {
    let native = workflow(".github/workflows/platform-runtime.yml");
    let native_jobs = jobs(&native);
    let (_, native_job) = native_jobs.first().expect("the native workflow declares a job");
    let native_timeout =
        native_job["timeout-minutes"].as_i64().expect("the native job declares a timeout");

    let quality = workflow(".github/workflows/quality.yml");
    let (_, gate) = jobs(&quality)
        .into_iter()
        .find(|(name, _)| name == "gate")
        .expect("the quality workflow declares its repository gate");
    let gate_timeout =
        gate["timeout-minutes"].as_i64().expect("the repository gate declares a timeout");

    assert_eq!(
        native_timeout, gate_timeout,
        "native rows and the repository gate must have the same cold-run budget"
    );
    assert!(
        native_timeout >= 30,
        "the all-target/all-feature gate needs enough budget for a cold hosted runner"
    );
}

/// The action that attests, which composes the provenance itself.
const ATTESTING_ACTION: &str = "actions/attest-build-provenance@";

#[test]
fn exactly_one_job_attests_and_it_does_so_over_named_files() {
    let document = workflow(".github/workflows/release.yml");
    let attesting: Vec<String> = jobs(&document)
        .into_iter()
        .filter(|(_, job)| {
            steps(job).iter().any(|step| {
                step["uses"].as_str().is_some_and(|uses| uses.starts_with(ATTESTING_ACTION))
            })
        })
        .map(|(name, _)| name)
        .collect();
    assert_eq!(attesting, vec![ATTESTATION_JOB.to_owned()], "one job attests, and one only");
    let named = jobs(&document);
    let (_, job) = named.iter().find(|(name, _)| name == ATTESTATION_JOB).expect("it is there");
    let attest = steps(job)
        .into_iter()
        .find(|step| step["uses"].as_str().is_some_and(|uses| uses.starts_with(ATTESTING_ACTION)))
        .expect("the step is there");
    assert!(attest["with"]["subject-path"].as_str().is_some(), "it names the files it attests");
    // The provider composes the provenance, because the policy reads the builder
    // and the workflow out of it and only the provider can state those. So the
    // workflow composes no predicate, and the type this repository verifies is
    // pinned where the verification happens.
    assert!(
        attest["with"]["predicate-type"].is_null() && attest["with"]["predicate-path"].is_null(),
        "a workflow that composed the predicate would be attesting its own account of itself"
    );
    let policy = read_repository_file("support/release-attestation-policy.toml");
    assert!(
        policy.contains("predicate-type = \"https://slsa.dev/provenance/v1\""),
        "and the policy pins the provenance version the attesting action produces"
    );
    for discovery in ["subject-digest", "subject-checksums", "push-to-registry"] {
        assert!(
            attest["with"][discovery].is_null(),
            "automatic discovery would attest whatever happened to be there: {discovery}"
        );
    }
}

#[test]
fn every_attested_archive_keeps_its_bundle_in_the_uploaded_row() {
    let document = workflow(".github/workflows/release.yml");
    let named = jobs(&document);
    let (_, job) =
        named.iter().find(|(name, _)| name == ATTESTATION_JOB).expect("the attestation job");
    let steps = steps(job);
    let attest = steps
        .iter()
        .find(|step| step["uses"].as_str().is_some_and(|uses| uses.starts_with(ATTESTING_ACTION)))
        .expect("the archive attestation step");
    let subject = attest["with"]["subject-path"].as_str().expect("a subject path");
    assert_eq!(
        subject, "${{ runner.temp }}/release/*.${{ matrix.archive_profile }}",
        "the attestation names the archive profile selected by each row"
    );
    let rows = named
        .iter()
        .find(|(name, _)| name == ATTESTATION_JOB)
        .and_then(|(_, job)| job["strategy"]["matrix"]["include"].as_sequence())
        .expect("the attestation job declares native rows");
    for row in rows {
        let triple = row["triple"].as_str().expect("each row names a target");
        let profile = row["archive_profile"].as_str().expect("each row names an archive profile");
        let expected = match triple {
            "x86_64-pc-windows-msvc" => "zip",
            "aarch64-apple-darwin" | "x86_64-unknown-linux-gnu" => "tar.gz",
            other => panic!("the workflow declares an unsupported release row: {other}"),
        };
        assert_eq!(profile, expected, "the row profile must match its archive format");
    }
    let copy = steps
        .iter()
        .find(|step| step["run"].as_str().is_some_and(|run| run.contains("attestation.jsonl")))
        .and_then(|step| step["run"].as_str())
        .expect("the bundle is copied into the row");
    assert!(copy.contains("$RUNNER_TEMP/release/attestation.jsonl"));
    let upload = steps
        .iter()
        .find(|step| {
            step["uses"].as_str().is_some_and(|uses| uses.starts_with("actions/upload-artifact@"))
        })
        .expect("the row is uploaded");
    assert_eq!(upload["with"]["path"].as_str(), Some("${{ runner.temp }}/release"));
    assert!(upload["with"]["name"].as_str().is_some_and(|name| name.contains("matrix.triple")));
}

#[test]
fn the_release_reviews_the_advisory_pin_in_a_protected_environment_first() {
    let document = workflow(".github/workflows/release.yml");
    let named = jobs(&document);
    let (_, review) =
        named.iter().find(|(name, _)| name == "rustsec-owner-review").expect("the review job");
    assert_eq!(
        review["environment"].as_str(),
        Some("release-rustsec-review"),
        "the review runs where approval is required"
    );
    let (_, provenance) =
        named.iter().find(|(name, _)| name == ATTESTATION_JOB).expect("the build job");
    let needs = provenance["needs"].as_sequence().expect("every build waits for its inputs");
    assert!(
        needs.iter().any(|need| need.as_str() == Some("rustsec-owner-review")),
        "and every build waits for the protected review"
    );
    let recorded = read_repository_file("scripts/record_rustsec_owner_review");
    for authored in ["date +", "$(date", "\"timestamp\"", "\"fresh\""] {
        assert!(
            !recorded.contains(authored),
            "a record carrying {authored} would be recording a claim rather than a fact"
        );
    }
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
