//! Whether one source revision is releasable, and what makes that answerable.
//!
//! Two properties carry this suite. The isolation has to actually deny what it
//! says it denies - so every flag that makes a denial real is weakened one at a
//! time and each weakening is refused by name, because a contract that admitted
//! one of them would be a description rather than a boundary. And the gate
//! inventory has to be complete, ordered, and unanimous: a gate that did not
//! run is not a gate that passed, and the five ways a record can be wrong are
//! five different defects with one consequence.
//!
//! The runner is held to preparing nothing. Acceptance that fetched a missing
//! input, installed a missing tool, or repaired a dirty tree would be accepting
//! whatever it managed to assemble rather than what it was given.

use std::path::{Path, PathBuf};

use serde_json::{Value, json};
use slingshot_development::github_automation_authority::{AUTHORITY_PATH, parse_authority};
use slingshot_development::release_acceptance::{
    AcceptanceGate, AcceptanceManifest, AcceptanceRefusal, CACHE_MOUNT, CONTAINER_PATH,
    FINITE_STATE_MACHINE_MOUNT, GateRun, GateSubject, HELD, MANIFEST_FORMAT, NETWORK_NONE,
    PLATFORM_EVIDENCE_MOUNT, PROVIDER_RUN_VARIABLE, REFUSED, RELEASABLE, REQUIRED_GATES,
    REVIEW_RECORD_MOUNT, RunBinding, RunIdentity, SCHEMA_PATH, SOURCE_COMMIT_VARIABLE,
    SOURCE_TREE_VARIABLE, conclude, parse_container, parse_manifest, require_complete,
    require_revision, run_gate, told,
};

/// Where the fixtures live.
const FIXTURES: &str = "tests/fixtures/release-acceptance";

/// The revision a fixture acceptance run is about.
const SOURCE_COMMIT: &str = "1111111111111111111111111111111111111111";

/// How many characters a digest is written in.
const DIGEST_CHARACTERS: usize = 64;

/// How many characters a commit is written in.
const COMMIT_CHARACTERS: usize = 40;

/// A gate whose program this repository does not have and never had.
const ABSENT_PROGRAM: &str = "scripts/no_such_gate_lives_here";

/// The provider run a fixture acceptance run was produced by.
const PROVIDER_RUN: &str = ".github/workflows/release.yml@refs/heads/main";

/// The row a fixture acceptance run was coordinated by.
const COORDINATOR_ROW: &str = "x86_64-unknown-linux-gnu";

/// Where the runner that starts the container lives.
const RUNNER_PATH: &str = "scripts/release_acceptance";

/// Where the module that makes the decision lives.
const DECISION_PATH: &str = "crates/slingshot-development/src/release_acceptance.rs";

/// Where the workflow that asks for the decision lives.
const WORKFLOW_PATH: &str = ".github/workflows/release.yml";

/// The variable a workflow job reports which run it is through.
const REPORTED_WORKFLOW_VARIABLE: &str = "SLINGSHOT_REPORTED_WORKFLOW";

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

/// Returns the rows one fixture states.
fn fixture_rows(name: &str) -> Vec<Value> {
    std::fs::read_to_string(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(FIXTURES).join(name))
        .unwrap_or_else(|failure| panic!("{name} could not be read: {failure}"))
        .lines()
        .filter(|line| !line.trim().is_empty() && !line.starts_with('#'))
        .map(|line| serde_json::from_str(line).expect("every row reads"))
        .collect()
}

/// Returns which refusal one failure is.
///
/// Read off the refusal itself rather than matched name by name. A table of
/// names beside the refusals needs a row adding whenever a refusal is, and the
/// row nobody adds is the one a fixture then cannot tell from another.
fn refusal_name(failure: &AcceptanceRefusal) -> String {
    let rendered = format!("{failure:?}");
    rendered
        .split(|character: char| !character.is_alphanumeric())
        .next()
        .unwrap_or_default()
        .to_owned()
}

/// Returns what a fixture acceptance run binds its decision to.
fn binding() -> RunBinding {
    let digest = "a".repeat(DIGEST_CHARACTERS);
    RunBinding {
        coordinator_row: COORDINATOR_ROW.to_owned(),
        isolation_sha256: digest.clone(),
        platform_evidence_sha256: digest.clone(),
        rustsec_review_record_sha256: digest,
        identity: RunIdentity {
            source_commit: SOURCE_COMMIT.to_owned(),
            source_tree: SOURCE_COMMIT.to_owned(),
            provider_run: PROVIDER_RUN.to_owned(),
        },
    }
}

/// Returns one run of every gate in which exactly the named ones refused.
fn runs_refusing(refused: &[&str]) -> Vec<GateRun> {
    REQUIRED_GATES
        .iter()
        .map(|gate| GateRun {
            name: gate.name.to_owned(),
            held: !refused.contains(&gate.name),
            report: format!("{} ran\n", gate.name).into_bytes(),
        })
        .collect()
}

/// Returns a gate that runs one command of this repository's own executable.
fn probe_gate(passed: &'static [&'static str]) -> AcceptanceGate {
    AcceptanceGate { name: REQUIRED_GATES[0].name, subject: GateSubject::RepositoryCommand(passed) }
}

/// Returns one complete acceptance manifest, as a run does write it.
///
/// Produced by the thing that produces one rather than written out beside it.
/// A fixture manifest assembled by hand would be a second statement of the
/// document's shape, and the two could disagree for as long as nobody looked.
fn complete_manifest() -> Value {
    serde_json::to_value(conclude(&binding(), &runs_refusing(&[])))
        .expect("what a run writes is JSON")
}

/// Returns the manifest one value parses into.
fn parsed(held: &Value) -> AcceptanceManifest {
    parse_manifest(&held.to_string()).expect("the manifest parses")
}

#[test]
fn the_committed_isolation_denies_everything_it_says_it_denies() {
    let held = parse_container(&read_repository_file(CONTAINER_PATH)).expect("it parses");
    assert_eq!(held.isolation.network, NETWORK_NONE);
    assert!(!held.isolation.privileged);
    assert!(held.isolation.no_new_privileges);
    assert!(held.isolation.read_only_root);
    assert!(!held.isolation.host_engine_socket);
    assert!(held.isolation.host_namespaces.is_empty());
    assert!(held.isolation.host_devices.is_empty());
    assert!(held.isolation.add_capabilities.is_empty());
    assert!(held.runtime.rootless && held.runtime.daemonless);
    assert!(!held.image.pull, "the image is loaded from what was transferred");
    assert!(!held.mounts.read_only.is_empty(), "every input is mounted read-only");
    assert!(!held.mounts.writable_output_root.is_empty(), "one writable root, and one only");
}

#[test]
fn every_weakening_of_the_isolation_is_refused_for_its_own_reason() {
    let committed = read_repository_file(CONTAINER_PATH);
    let declared = fixture_rows("weakened-isolation.jsonl");
    assert!(!declared.is_empty());
    for row in declared {
        let name = row["name"].as_str().expect("a name");
        let find = row["find"].as_str().expect("a find");
        assert!(committed.contains(find), "{name}: the contract has no {find:?}");
        let altered = committed.replacen(find, row["replace"].as_str().expect("a replacement"), 1);
        let failure = parse_container(&altered).expect_err(&format!("{name} was accepted"));
        assert_eq!(
            refusal_name(&failure),
            row["refusal"].as_str().expect("a refusal"),
            "{name}: {failure}"
        );
    }
}

#[test]
fn the_coordinator_is_the_row_the_owner_declared() {
    let container = parse_container(&read_repository_file(CONTAINER_PATH)).expect("it parses");
    let authority = parse_authority(&read_repository_file(AUTHORITY_PATH)).expect("it parses");
    let coordinator =
        authority.row.iter().find(|row| row.coordinator).expect("one row is the coordinator");
    assert_eq!(container.coordinator.triple, coordinator.triple);
    assert_eq!(container.coordinator.runner_selector, coordinator.runner_selector);
}

#[test]
fn a_complete_run_records_every_gate_once_in_order_and_all_holding() {
    let held = parsed(&complete_manifest());
    require_complete(&held).expect("this run is complete");
    require_revision(&held, SOURCE_COMMIT).expect("and about this revision");
    assert_eq!(held.gates.len(), REQUIRED_GATES.len());
}

#[test]
fn a_gate_that_did_not_run_is_not_a_gate_that_passed() {
    let mut manifest = complete_manifest();
    let gates = manifest["gates"].as_array_mut().expect("the gates");
    let removed = gates.remove(0);
    let failure = require_complete(&parsed(&manifest)).expect_err("one gate is missing");
    assert_eq!(refusal_name(&failure), "GateMissing");
    assert!(
        failure.to_string().contains(removed["name"].as_str().expect("a name")),
        "and the refusal names which"
    );
}

#[test]
fn a_gate_recorded_twice_decided_nothing_the_second_time() {
    let mut manifest = complete_manifest();
    let gates = manifest["gates"].as_array_mut().expect("the gates");
    let first = gates[0].clone();
    gates.push(first);
    let failure = require_complete(&parsed(&manifest)).expect_err("one gate is repeated");
    assert_eq!(refusal_name(&failure), "GateRepeated");
}

#[test]
fn gates_recorded_out_of_the_order_they_run_in_are_refused() {
    let mut manifest = complete_manifest();
    let gates = manifest["gates"].as_array_mut().expect("the gates");
    gates.swap(0, 1);
    let failure = require_complete(&parsed(&manifest)).expect_err("they are out of order");
    assert_eq!(refusal_name(&failure), "GateOutOfOrder");
    assert!(
        failure.to_string().contains(REQUIRED_GATES[0].name),
        "and the refusal names what belongs there"
    );
}

#[test]
fn one_refused_gate_makes_the_revision_unreleasable() {
    for position in 0..REQUIRED_GATES.len() {
        let mut manifest = complete_manifest();
        manifest["gates"][position]["outcome"] = json!("refused");
        let failure = require_complete(&parsed(&manifest)).expect_err("a gate refused");
        assert_eq!(refusal_name(&failure), "GateRefused");
        assert!(failure.to_string().contains(REQUIRED_GATES[position].name), "and names which");
    }
}

#[test]
fn a_gate_acceptance_does_not_require_cannot_be_counted_toward_it() {
    let mut manifest = complete_manifest();
    let digest = "a".repeat(DIGEST_CHARACTERS);
    manifest["gates"]
        .as_array_mut()
        .expect("the gates")
        .push(json!({ "name": "something-else", "outcome": HELD, "report_sha256": digest }));
    let failure = require_complete(&parsed(&manifest)).expect_err("no such gate");
    assert_eq!(refusal_name(&failure), "GateUnknown");
}

#[test]
fn a_run_that_concludes_more_than_its_gates_support_is_refused() {
    let mut manifest = complete_manifest();
    manifest["outcome"] = json!("refused");
    let failure = require_complete(&parsed(&manifest)).expect_err("the outcome is not supported");
    assert_eq!(refusal_name(&failure), "OutcomeUnsupported");
}

#[test]
fn a_manifest_about_another_revision_is_about_another_revision() {
    let held = parsed(&complete_manifest());
    let failure =
        require_revision(&held, &"0".repeat(COMMIT_CHARACTERS)).expect_err("another revision");
    assert_eq!(refusal_name(&failure), "RevisionDrift");
}

#[test]
fn the_schema_and_the_manifest_a_run_writes_agree() {
    let schema: Value =
        serde_json::from_str(&read_repository_file(SCHEMA_PATH)).expect("the schema reads");
    let manifest = complete_manifest();
    for member in schema["required"].as_array().expect("the schema names what is required") {
        let named = member.as_str().expect("a member is named");
        assert!(!manifest[named].is_null(), "a run writes no {named}");
    }
    let properties = schema["properties"].as_object().expect("the schema names its members");
    for named in manifest.as_object().expect("the manifest is an object").keys() {
        assert!(properties.contains_key(named), "the schema describes no {named}");
    }
    assert_eq!(schema["properties"]["format"]["const"].as_str(), Some(MANIFEST_FORMAT));
    let outcomes = schema["properties"]["outcome"]["enum"]
        .as_array()
        .expect("the schema names what a run may conclude");
    let answers = [RELEASABLE, REFUSED];
    assert_eq!(
        outcomes.len(),
        answers.len(),
        "a run is releasable or it is refused, and there is no third answer"
    );
    for answer in answers {
        assert!(
            outcomes.iter().any(|held| held.as_str() == Some(answer)),
            "a run writes {answer} and the schema admits no such answer"
        );
    }
}

#[test]
fn acceptance_prepares_nothing_and_says_so_when_something_is_missing() {
    let runner = read_repository_file("scripts/release_acceptance");
    for preparing in ["cargo install", "curl", "git clone", "git fetch", "pull"] {
        assert!(
            !runner.contains(preparing),
            "acceptance that ran {preparing} would accept whatever it assembled"
        );
    }
    for refused in [
        "name the verified pinned source",
        "name the verified advisory database",
        "name the same-run owner review record",
        "name the verified coordinator cache member",
        "name the authenticated platform evidence",
    ] {
        assert!(runner.contains(refused), "a missing input is refused: {refused}");
    }
    assert!(
        runner.contains("git diff --quiet HEAD"),
        "and a tree that differs from the commit is refused before a gate runs"
    );
}

#[test]
fn every_denial_the_contract_names_appears_in_the_invocation_that_makes_it() {
    let runner = read_repository_file("scripts/release_acceptance");
    for enforced in
        ["--network none", "--read-only", "--cap-drop ALL", "--security-opt no-new-privileges"]
    {
        assert!(runner.contains(enforced), "the invocation does not apply {enforced}");
    }
    let read_only = runner.matches(":ro").count();
    let container = parse_container(&read_repository_file(CONTAINER_PATH)).expect("it parses");
    assert_eq!(
        read_only,
        container.mounts.read_only.len(),
        "every input the contract names read-only is mounted read-only"
    );
    // The writable roots the contract declares: the one evidence leaves through
    // and the one a build works in. A mount beyond those is a mount nothing
    // named, which is the thing this whole document exists to prevent.
    let writable = [&container.mounts.writable_output_root, &container.mounts.writable_build_root];
    assert_eq!(
        runner.matches("--volume").count(),
        container.mounts.read_only.len() + writable.len(),
        "and exactly the writable roots the contract declares, and no other mount"
    );
    for root in writable {
        assert!(runner.contains(root.as_str()), "the invocation does not mount {root}");
    }
}

#[test]
fn the_gates_run_inside_the_container_and_the_host_starts_nothing_after_it() {
    let inside = read_repository_file("scripts/run_acceptance_gates");
    assert!(inside.contains("--frozen --offline"), "the gates resolve nothing");
    assert!(
        inside.contains("SLINGSHOT_ACCEPTANCE_OUTPUT"),
        "and write into the one root that leaves the container"
    );
    let runner = read_repository_file("scripts/release_acceptance");
    let container = runner.find("run --rm").expect("it starts the container");
    let manifest = runner.find("verify-release-acceptance").expect("it verifies the manifest");
    assert!(container < manifest, "the manifest is read after the run that produced it");
    assert!(
        !runner.contains("scripts/quality"),
        "the host runs no gate of its own after the container exits"
    );
}

#[test]
fn what_a_run_decides_is_what_the_verifier_reads_back_out_of_it() {
    let declared = fixture_rows("decisions.jsonl");
    assert!(!declared.is_empty());
    for row in declared {
        let name = row["name"].as_str().expect("a name");
        let refused: Vec<&str> = row["refused"]
            .as_array()
            .expect("the gates that refused")
            .iter()
            .map(|held| held.as_str().expect("a gate name"))
            .collect();
        let manifest = conclude(&binding(), &runs_refusing(&refused));
        assert_eq!(manifest.outcome, row["outcome"].as_str().expect("an outcome"), "{name}");
        let written =
            parse_manifest(&serde_json::to_string(&manifest).expect("what a run writes is JSON"))
                .unwrap_or_else(|failure| panic!("{name}: a run wrote no manifest: {failure}"));
        assert_eq!(written, manifest, "{name}: what was written is what is read back");
        require_revision(&written, SOURCE_COMMIT)
            .unwrap_or_else(|failure| panic!("{name}: {failure}"));
        let Some(expected) = row["refusal"].as_str().filter(|held| !held.is_empty()) else {
            require_complete(&written).unwrap_or_else(|failure| panic!("{name}: {failure}"));
            continue;
        };
        let failure = require_complete(&written).expect_err(&format!("{name} was accepted"));
        assert_eq!(refusal_name(&failure), expected, "{name}: {failure}");
        assert!(
            failure.to_string().contains(row["names"].as_str().expect("a gate")),
            "{name}: the refusal does not name which gate stopped it: {failure}"
        );
    }
}

#[test]
fn a_gate_that_could_not_run_at_all_is_not_a_gate_that_held() {
    let gate = AcceptanceGate {
        name: REQUIRED_GATES[0].name,
        subject: GateSubject::Script { path: ABSENT_PROGRAM, arguments: &[] },
    };
    let run = run_gate(&gate, &workspace_root());
    assert!(!run.held, "a gate that never started is not a gate that held");
    let report = String::from_utf8_lossy(&run.report).into_owned();
    assert!(report.contains(ABSENT_PROGRAM), "and its report names what could not start: {report}");
    let manifest = conclude(&binding(), &[run]);
    assert_eq!(manifest.gates[0].outcome, REFUSED);
    assert_eq!(manifest.outcome, REFUSED, "and the revision it was about is not releasable");
}

#[test]
fn a_gate_is_recorded_by_what_its_own_run_concluded() {
    let root = workspace_root();
    let held = run_gate(&probe_gate(&["workspace-metadata"]), &root);
    let report = String::from_utf8_lossy(&held.report).into_owned();
    assert!(held.held, "a gate whose run succeeded held: {report}");
    assert!(!held.report.is_empty(), "and what it wrote is what its digest is over");
    let refused = run_gate(&probe_gate(&["no-such-repository-command"]), &root);
    assert!(!refused.held, "and a gate whose run failed refused");
    assert_ne!(
        conclude(&binding(), &[held]).gates[0].report_sha256,
        conclude(&binding(), &[refused]).gates[0].report_sha256,
        "two gates that wrote different things do not digest the same"
    );
}

#[test]
fn no_gate_reaches_for_anything_the_container_was_not_given() {
    let runner = read_repository_file("scripts/release_acceptance");
    let given =
        [CACHE_MOUNT, FINITE_STATE_MACHINE_MOUNT, PLATFORM_EVIDENCE_MOUNT, REVIEW_RECORD_MOUNT];
    for mount in given {
        assert!(
            runner.contains(&format!(":{mount}:ro")),
            "the invocation mounts no {mount} for the decision to read"
        );
    }
    for gate in REQUIRED_GATES {
        let program = gate.program(&workspace_root());
        if let GateSubject::Script { path, .. } = gate.subject {
            assert!(program.is_file(), "{}: this repository commits no {path}", gate.name);
        }
        for argument in gate.arguments() {
            if !argument.starts_with('/') {
                continue;
            }
            assert!(
                given.contains(&argument.as_str()),
                "{}: {argument} is not something the container is given",
                gate.name
            );
        }
    }
}

#[test]
fn every_gate_the_inventory_names_is_named_once_and_run_by_something() {
    let mut seen = std::collections::BTreeSet::new();
    for gate in REQUIRED_GATES {
        assert!(seen.insert(gate.name), "{} is in the inventory twice", gate.name);
        assert!(!gate.arguments().is_empty(), "{} is run by nothing", gate.name);
    }
    assert_eq!(seen.len(), REQUIRED_GATES.len());
}

/// Returns everything one run is told about itself.
fn everything_told() -> std::collections::BTreeMap<String, String> {
    [
        (SOURCE_COMMIT_VARIABLE, SOURCE_COMMIT),
        (SOURCE_TREE_VARIABLE, SOURCE_COMMIT),
        (PROVIDER_RUN_VARIABLE, PROVIDER_RUN),
    ]
    .into_iter()
    .map(|(variable, held)| (variable.to_owned(), held.to_owned()))
    .collect()
}

/// Returns every variable the runner tells the container.
fn variables_the_runner_passes(runner: &str) -> Vec<String> {
    runner
        .split("--env ")
        .skip(1)
        .filter_map(|held| held.split('=').next())
        .map(|held| held.trim().trim_matches('"').to_owned())
        .collect()
}

#[test]
fn everything_the_manifest_binds_arrives_through_the_declared_environment() {
    let container = parse_container(&read_repository_file(CONTAINER_PATH)).expect("it parses");
    let runner = read_repository_file(RUNNER_PATH);
    let passed = variables_the_runner_passes(&runner);
    for variable in [SOURCE_COMMIT_VARIABLE, SOURCE_TREE_VARIABLE, PROVIDER_RUN_VARIABLE] {
        assert!(
            container.environment.allowed.iter().any(|held| held == variable),
            "the contract admits no {variable}, so the run would never have it"
        );
        assert!(
            passed.iter().any(|held| held == variable),
            "the contract admits {variable} and the invocation tells the container no such thing"
        );
    }
    assert!(
        runner.contains(&format!("{SOURCE_COMMIT_VARIABLE}=$SOURCE_COMMIT")),
        "the revision told to the container is the one read from the checkout"
    );
    assert!(runner.contains(&format!("{SOURCE_TREE_VARIABLE}=$SOURCE_TREE")), "and so is the tree");
}

#[test]
fn the_container_is_told_nothing_the_contract_does_not_admit() {
    let container = parse_container(&read_repository_file(CONTAINER_PATH)).expect("it parses");
    let passed = variables_the_runner_passes(&read_repository_file(RUNNER_PATH));
    assert!(!passed.is_empty());
    for variable in passed {
        assert!(
            container.environment.allowed.contains(&variable),
            "the invocation tells the container {variable} and the contract admits no such thing"
        );
    }
}

#[test]
fn a_run_told_nothing_about_itself_decides_nothing() {
    let complete = everything_told();
    told(&complete).expect("a run told everything knows what it is deciding");
    for variable in [SOURCE_COMMIT_VARIABLE, SOURCE_TREE_VARIABLE, PROVIDER_RUN_VARIABLE] {
        let mut absent = complete.clone();
        absent.remove(variable);
        let failure = told(&absent).expect_err("a run missing a value it binds decides nothing");
        assert_eq!(refusal_name(&failure), "RunUnbound", "{failure}");
        assert!(failure.to_string().contains(variable), "and the refusal names which: {failure}");

        let mut blank = complete.clone();
        blank.insert(variable.to_owned(), " ".to_owned());
        let failure = told(&blank).expect_err("told a blank is told nothing");
        assert_eq!(refusal_name(&failure), "RunUnbound", "{failure}");
        assert!(failure.to_string().contains(variable), "{failure}");
    }
    assert!(
        told(&std::collections::BTreeMap::new()).is_err(),
        "and a run told nothing at all decides nothing at all"
    );
}

#[test]
fn nothing_the_manifest_binds_is_worked_out_from_the_containers_surroundings() {
    let decision = read_repository_file(DECISION_PATH);
    assert_eq!(
        decision.matches("std::env::").count(),
        1,
        "the decision reads its environment in one place, or a value could come from another"
    );
    for inferred in ["rev-parse", "GITHUB_", "hostname", "SLINGSHOT_REPORTED_"] {
        assert!(
            !decision.contains(inferred),
            "a run that could work {inferred} out could be persuaded it was a run it is not"
        );
    }
    let runner = read_repository_file(RUNNER_PATH);
    assert!(
        runner.contains("SOURCE_COMMIT=$(git rev-parse HEAD)"),
        "the revision is read on the host, from the checkout being accepted"
    );
    assert!(
        runner.contains(&format!("PROVIDER_RUN=${{{REPORTED_WORKFLOW_VARIABLE}:-}}")),
        "and the provider run comes from the one variable a run reports itself through"
    );
}

#[test]
fn the_run_that_asks_for_a_decision_says_which_run_it_is() {
    let runner = read_repository_file(RUNNER_PATH);
    assert!(
        runner.contains(&format!("report_refusal 'set {REPORTED_WORKFLOW_VARIABLE}")),
        "a host that reports no run is refused before a container starts"
    );
    let workflow = read_repository_file(WORKFLOW_PATH);
    let asks = workflow.find(RUNNER_PATH).expect("the workflow asks for the decision");
    let reports = workflow[..asks]
        .rfind(REPORTED_WORKFLOW_VARIABLE)
        .expect("the job that asks for the decision reports which run it is");
    let step = workflow[..asks].rfind("      - name:").unwrap_or_default();
    assert!(
        reports > step,
        "the step that asks for the decision reports the run itself, and an environment does not \
         travel between steps"
    );
}
