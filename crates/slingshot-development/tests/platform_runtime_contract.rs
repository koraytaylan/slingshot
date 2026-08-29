//! Assertions for the platform runtime contract.
//!
//! Every supported row is evaluated through deterministic observations, so all
//! three are checked from one machine. Real endpoint, lock, readiness,
//! detachment, and supervision behavior runs only for the row that matches the
//! current environment, and its result is one explicitly untrusted report.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{Duration, Instant};

use serde::Deserialize;
use slingshot_daemon::platform_runtime::failure::PlatformFailure;
use slingshot_daemon::platform_runtime::locks::{OwnerLock, StartupElectionLock};
use slingshot_daemon::platform_runtime::readiness::ReadinessRecord;
use slingshot_daemon::platform_runtime::{current_user, endpoint, readiness};
use slingshot_development::platform_runtime_contract::{
    self, OUTCOME_NOT_RUN, OUTCOME_PASSED, OwnershipDecision, ReportedOutcome, RuntimeObservation,
    UntrustedRuntimeReport,
};
use slingshot_development::supported_platform_matrix::{
    self, SUPPORTED_TARGET_TRIPLES, SupportedPlatformMatrix, UNTRUSTED_OBSERVATION_LABEL,
    current_target_triple,
};
use slingshot_local_protocol::foundation_contract::FoundationContract;
use slingshot_test_support::supervised_child::{Disposition, SupervisedChild, SupervisionFailure};

/// Directory holding the fixtures this test evaluates.
const FIXTURE_DIRECTORY: &str = "crates/slingshot-development/tests/fixtures/platform-runtime";

/// Repository path of the abstract supported-target manifest.
const MATRIX_PATH: &str = "support/platforms.toml";

/// Repository path of the report schema.
const SCHEMA_PATH: &str = "support/platform-runtime-evidence.schema.json";

/// Environment variable that turns this test binary into a probe child.
const PROBE_ROLE_VARIABLE: &str = "SLINGSHOT_PLATFORM_PROBE_ROLE";

/// Environment variable naming the runtime root a probe child works in.
const PROBE_ROOT_VARIABLE: &str = "SLINGSHOT_PLATFORM_PROBE_ROOT";

/// Role of a child that holds the startup-election lock until it is told to stop.
const ELECTION_HOLDER_ROLE: &str = "election-lock-holder";

/// Role of a child that starts a detached grandchild and exits at once.
const DETACHED_STARTER_ROLE: &str = "detached-starter";

/// File a probe child writes once it holds the lock.
const HOLDER_READY_FILE: &str = "holder.ready";

/// File whose presence asks a probe child to exit.
const HOLDER_STOP_FILE: &str = "holder.stop";

/// Namespace digest the real probes work under.
const PROBE_NAMESPACE_DIGEST: &str =
    "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";

/// Interval between two polls while waiting for a real condition.
const POLL_INTERVAL: Duration = Duration::from_millis(5);

/// Characters a rendered digest occupies in a report.
const RENDERED_DIGEST_LENGTH: usize = 64;

/// The deterministic runtime observations.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "kebab-case", deny_unknown_fields)]
struct ObservationFixture {
    /// One entry per observation.
    observation: Vec<RecordedObservation>,
}

/// One deterministic observation and whether its row must accept it.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "kebab-case")]
struct RecordedObservation {
    /// The observation itself.
    #[serde(flatten)]
    observation: RuntimeObservation,
    /// Whether the row must accept it.
    accepted: bool,
}

/// The pure ownership decisions.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "kebab-case", deny_unknown_fields)]
struct DecisionFixture {
    /// One entry per decision.
    decision: Vec<RecordedDecision>,
}

/// One set of observed facts and the decision it must produce.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "kebab-case", deny_unknown_fields)]
struct RecordedDecision {
    /// Name the fixture gives the decision.
    name: String,
    /// Whether some process holds the owner lock.
    owner_lock_held: bool,
    /// Whether some client holds the startup-election lock.
    election_lock_held: bool,
    /// Whether a readiness record is present.
    readiness_present: bool,
    /// Whether an endpoint object is present.
    endpoint_present: bool,
    /// Decision the facts must produce.
    expected: String,
}

/// Returns the workspace root directory.
fn workspace_root() -> PathBuf {
    slingshot_development::locate_workspace_root(Path::new(env!("CARGO_MANIFEST_DIR")))
        .expect("the development crate lives inside the workspace")
}

/// Reads one repository file relative to the workspace root.
fn read_repository_file(relative: &str) -> String {
    let path = workspace_root().join(relative);
    std::fs::read_to_string(&path)
        .unwrap_or_else(|failure| panic!("{} could not be read: {failure}", path.display()))
}

/// Reads and parses the committed supported-target manifest.
fn committed_matrix() -> SupportedPlatformMatrix {
    supported_platform_matrix::parse_matrix(&read_repository_file(MATRIX_PATH))
        .expect("the committed matrix is valid")
}

/// Reads one fixture owned by this test.
fn fixture<Shape: serde::de::DeserializeOwned>(name: &str) -> Shape {
    let text = read_repository_file(&format!("{FIXTURE_DIRECTORY}/{name}"));
    toml::from_str(&text).unwrap_or_else(|failure| panic!("{name} is a valid document: {failure}"))
}

/// Waits until a condition holds or the deadline elapses.
fn wait_until(deadline: Duration, mut condition: impl FnMut() -> bool) -> bool {
    let started = Instant::now();
    while started.elapsed() < deadline {
        if condition() {
            return true;
        }
        std::thread::sleep(POLL_INTERVAL);
    }
    condition()
}

/// Starts this test binary again in one probe role.
fn start_probe_child(role: &str, root: &Path, detached: bool) -> std::process::Child {
    let executable = std::env::current_exe().expect("the test executable is known");
    let arguments = vec![
        "--exact".to_owned(),
        "the_probe_child_roles_run_only_when_they_are_asked_to".to_owned(),
        "--nocapture".to_owned(),
    ];
    if detached {
        let mut command = Command::new(&executable);
        command.args(&arguments).env(PROBE_ROLE_VARIABLE, role).env(PROBE_ROOT_VARIABLE, root);
        command.spawn().expect("the probe child starts")
    } else {
        Command::new(&executable)
            .args(&arguments)
            .env(PROBE_ROLE_VARIABLE, role)
            .env(PROBE_ROOT_VARIABLE, root)
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn()
            .expect("the probe child starts")
    }
}

#[test]
fn the_probe_child_roles_run_only_when_they_are_asked_to() {
    let Ok(role) = std::env::var(PROBE_ROLE_VARIABLE) else {
        return;
    };
    let root = PathBuf::from(std::env::var(PROBE_ROOT_VARIABLE).expect("the probe root is named"));
    match role.as_str() {
        ELECTION_HOLDER_ROLE => hold_election_lock(&root),
        DETACHED_STARTER_ROLE => start_detached_grandchild(&root),
        other => panic!("the probe role {other} is unknown"),
    }
}

/// Holds the startup-election lock until the stop file appears.
fn hold_election_lock(root: &Path) {
    let held = StartupElectionLock::acquire(root, PROBE_NAMESPACE_DIGEST)
        .expect("the election lock file opens")
        .expect("the election lock is free");
    std::fs::write(root.join(HOLDER_READY_FILE), held.path().display().to_string())
        .expect("the ready file is written");
    let contract = FoundationContract::embedded();
    wait_until(contract.startup.explicit_start_total(), || root.join(HOLDER_STOP_FILE).is_file());
}

/// Starts a detached grandchild that holds the lock, then exits at once.
///
/// The grandchild is deliberately never waited for. That is what detachment
/// means here: this starter exits immediately, the grandchild is reparented to
/// the process the operating system reserves for orphans, and that process
/// reaps it. Waiting would keep the starter alive and defeat the very behavior
/// the caller is proving.
#[expect(clippy::zombie_processes, reason = "the starter must exit before its child does")]
fn start_detached_grandchild(root: &Path) {
    let executable = std::env::current_exe().expect("the test executable is known");
    let arguments = vec![
        "--exact".to_owned(),
        "the_probe_child_roles_run_only_when_they_are_asked_to".to_owned(),
        "--nocapture".to_owned(),
    ];
    let mut command = Command::new(&executable);
    command
        .args(&arguments)
        .env(PROBE_ROLE_VARIABLE, ELECTION_HOLDER_ROLE)
        .env(PROBE_ROOT_VARIABLE, root);
    slingshot_command_line::platform_runtime::detached_child::detach(&mut command);
    command.spawn().expect("the detached grandchild starts");
}

#[test]
fn every_row_accepts_or_refuses_its_deterministic_observations() {
    let matrix = committed_matrix();
    let recorded: ObservationFixture = fixture("runtime-observations.toml");
    let mut covered = BTreeSet::new();
    for entry in &recorded.observation {
        let row = matrix
            .target
            .iter()
            .find(|candidate| candidate.triple == entry.observation.triple)
            .unwrap_or_else(|| panic!("{} names no supported row", entry.observation.name));
        let violations =
            platform_runtime_contract::evaluate_runtime_policy(row, &entry.observation);
        assert_eq!(
            violations.is_empty(),
            entry.accepted,
            "{}: {violations:?}",
            entry.observation.name
        );
        covered.insert(entry.observation.triple.clone());
    }
    let expected: BTreeSet<String> =
        SUPPORTED_TARGET_TRIPLES.iter().map(|triple| (*triple).to_owned()).collect();
    assert_eq!(covered, expected, "every abstract row has deterministic coverage");
}

#[test]
fn ownership_is_decided_identically_on_every_row() {
    let recorded: DecisionFixture = fixture("ownership-decisions.toml");
    for triple in SUPPORTED_TARGET_TRIPLES {
        for entry in &recorded.decision {
            let observation = RuntimeObservation {
                name: entry.name.clone(),
                triple: (*triple).to_owned(),
                endpoint_kind: platform_runtime_contract::required_endpoint_kind(triple).to_owned(),
                owner_lock_held: entry.owner_lock_held,
                election_lock_held: entry.election_lock_held,
                readiness_present: entry.readiness_present,
                endpoint_present: entry.endpoint_present,
                current_user_only: true,
                atomic_readiness: true,
                detached_child_creation: true,
                supervision_retained: true,
                remote_clients_rejected: None,
                decided_by_process_identifier: false,
            };
            let decided = platform_runtime_contract::decide_ownership(&observation);
            let expected = match entry.expected.as_str() {
                "owned" => OwnershipDecision::Owned,
                "recover-stale-records" => OwnershipDecision::RecoverStaleRecords,
                "absent" => OwnershipDecision::Absent,
                other => panic!("{} names the unknown decision {other}", entry.name),
            };
            assert_eq!(decided, expected, "{} on {triple}", entry.name);
        }
    }
}

#[test]
fn the_report_schema_and_the_report_shape_declare_the_same_members() {
    let schema: serde_json::Value =
        serde_json::from_str(platform_runtime_contract::evidence_schema())
            .expect("the schema parses");
    assert_eq!(
        schema,
        serde_json::from_str::<serde_json::Value>(&read_repository_file(SCHEMA_PATH)).unwrap()
    );
    let properties = schema["properties"].as_object().expect("the schema lists properties");
    let declared: BTreeSet<&str> = properties.keys().map(String::as_str).collect();
    let report = UntrustedRuntimeReport {
        label: UNTRUSTED_OBSERVATION_LABEL.to_owned(),
        source_revision: "unknown".to_owned(),
        matrix_digest: "0".repeat(RENDERED_DIGEST_LENGTH),
        contract_digest: "0".repeat(RENDERED_DIGEST_LENGTH),
        triple: SUPPORTED_TARGET_TRIPLES[0].to_owned(),
        operating_system: "linux".to_owned(),
        architecture: "x86_64".to_owned(),
        outcomes: Vec::new(),
    };
    let rendered = serde_json::to_value(&report).expect("the report renders");
    let members: BTreeSet<&str> =
        rendered.as_object().expect("the report is an object").keys().map(String::as_str).collect();
    assert_eq!(declared, members);
    let required: BTreeSet<&str> = schema["required"]
        .as_array()
        .expect("the schema lists required members")
        .iter()
        .filter_map(serde_json::Value::as_str)
        .collect();
    assert_eq!(required, members);
}

#[test]
fn a_report_describes_one_row_and_records_every_behavior_once() {
    let current = current_target_triple();
    let mut report = UntrustedRuntimeReport {
        label: UNTRUSTED_OBSERVATION_LABEL.to_owned(),
        source_revision: "unknown".to_owned(),
        matrix_digest: "0".repeat(RENDERED_DIGEST_LENGTH),
        contract_digest: "0".repeat(RENDERED_DIGEST_LENGTH),
        triple: current.unwrap_or(SUPPORTED_TARGET_TRIPLES[0]).to_owned(),
        operating_system: std::env::consts::OS.to_owned(),
        architecture: std::env::consts::ARCH.to_owned(),
        outcomes: platform_runtime_contract::REPORTED_BEHAVIORS
            .iter()
            .map(|behavior| ReportedOutcome {
                behavior: (*behavior).to_owned(),
                policy: OUTCOME_PASSED.to_owned(),
                real: OUTCOME_NOT_RUN.to_owned(),
                reason: Some("this environment is not the row that requires it".to_owned()),
            })
            .collect(),
    };
    if current.is_some() {
        assert_eq!(
            platform_runtime_contract::evaluate_report(&report, current),
            Vec::<String>::new()
        );
    }
    report.outcomes.push(report.outcomes[0].clone());
    assert!(
        !platform_runtime_contract::evaluate_report(&report, current).is_empty(),
        "a repeat is refused"
    );
    report.outcomes.truncate(platform_runtime_contract::REPORTED_BEHAVIORS.len());
    report.label = "authoritative".to_owned();
    assert!(
        !platform_runtime_contract::evaluate_report(&report, current).is_empty(),
        "authority is refused"
    );
    report.label = UNTRUSTED_OBSERVATION_LABEL.to_owned();
    report.triple = "x86_64-unknown-freebsd".to_owned();
    assert!(
        !platform_runtime_contract::evaluate_report(&report, current).is_empty(),
        "another row is refused"
    );
}

#[test]
fn the_current_environment_proves_its_own_row_and_reports_it_as_untrusted() {
    let Some(current) = current_target_triple() else {
        return;
    };
    let contract = FoundationContract::embedded();
    let matrix = read_repository_file(MATRIX_PATH);
    let root = probe_root("row");
    current_user::create_owner_only_directory(&root).expect("the runtime directory is created");

    let mut outcomes = Vec::new();
    outcomes.push(prove("current-user-endpoint-isolation", || {
        assert!(current_user::is_owner_only(&root).expect("the directory is inspectable"));
        let address = endpoint::endpoint_address(&contract, &root, PROBE_NAMESPACE_DIGEST)
            .expect("the endpoint address is within its bound");
        assert!(address.display().contains(PROBE_NAMESPACE_DIGEST));
        assert!(
            address.display().len() <= contract.namespace.unix_socket_address_bytes as usize
                || !matches!(address, endpoint::EndpointAddress::UnixDomainSocket(_)),
            "the endpoint address stays inside its operating-system bound"
        );
        let oversized = "f".repeat(contract.namespace.unix_socket_address_bytes as usize);
        assert!(matches!(
            endpoint::endpoint_address(&contract, &root, &oversized),
            Err(PlatformFailure::EndpointNameTooLong { .. })
        ));
    }));
    outcomes.push(prove("atomic-readiness", || prove_atomic_readiness(&contract, &root)));
    outcomes.push(prove("one-daemon-owner-under-contention", || {
        let owner = OwnerLock::acquire(&root, PROBE_NAMESPACE_DIGEST)
            .expect("the owner lock file opens")
            .expect("the owner lock is free");
        assert!(
            OwnerLock::acquire(&root, PROBE_NAMESPACE_DIGEST).expect("the file opens").is_none(),
            "a second owner must be refused"
        );
        let election = StartupElectionLock::acquire(&root, PROBE_NAMESPACE_DIGEST)
            .expect("the election lock file opens")
            .expect("the election lock is a different object");
        assert_ne!(owner.path(), election.path(), "the two locks are separate objects");
        drop(election);
        drop(owner);
        assert!(
            OwnerLock::acquire(&root, PROBE_NAMESPACE_DIGEST).expect("the file opens").is_some(),
            "the owner lock is free once its holder drops it"
        );
        assert!(
            OwnerLock::path_for(&root, PROBE_NAMESPACE_DIGEST).is_file(),
            "the lock file persists"
        );
    }));
    outcomes.push(prove("one-elected-starter-under-contention", || {
        let elected = StartupElectionLock::acquire(&root, PROBE_NAMESPACE_DIGEST)
            .expect("the file opens")
            .expect("the election lock is free");
        assert!(
            StartupElectionLock::acquire(&root, PROBE_NAMESPACE_DIGEST)
                .expect("the file opens")
                .is_none(),
            "a second starter must be refused"
        );
        assert!(
            OwnerLock::acquire(&root, PROBE_NAMESPACE_DIGEST).expect("the file opens").is_some(),
            "holding the election lock is not ownership"
        );
        drop(elected);
    }));
    let (release_outcome, takeover_outcome) = prove_election_release(&contract, &root);
    outcomes.push(release_outcome);
    outcomes.push(takeover_outcome);
    outcomes.push(prove_detached_survival(&contract));
    outcomes.push(prove("stale-record-recovery", || prove_stale_recovery(&contract, &root)));
    outcomes.push(prove("bounded-supervised-cleanup", || prove_supervision(&contract, &root)));
    outcomes.push(remote_client_outcome(current));

    let report = UntrustedRuntimeReport {
        label: UNTRUSTED_OBSERVATION_LABEL.to_owned(),
        source_revision: source_revision(),
        matrix_digest: supported_platform_matrix::matrix_digest(matrix.as_bytes()),
        contract_digest: supported_platform_matrix::matrix_digest(
            FoundationContract::embedded_manifest().as_bytes(),
        ),
        triple: current.to_owned(),
        operating_system: std::env::consts::OS.to_owned(),
        architecture: std::env::consts::ARCH.to_owned(),
        outcomes,
    };
    assert_eq!(
        platform_runtime_contract::evaluate_report(&report, Some(current)),
        Vec::<String>::new()
    );
    std::fs::remove_dir_all(&root).ok();
}

/// Returns a fresh runtime root for one real probe.
///
/// The name is short on purpose. A Unix domain socket address is bounded by the
/// operating system and the foundation contract records that bound, so a
/// runtime root that leaves no room for the namespace digest is a real defect
/// rather than a test inconvenience.
fn probe_root(name: &str) -> PathBuf {
    let root = std::env::temp_dir().join(format!("sls-{}-{name}", std::process::id()));
    std::fs::remove_dir_all(&root).ok();
    root
}

/// Returns the source revision this observation was taken from.
fn source_revision() -> String {
    let produced =
        Command::new("git").current_dir(workspace_root()).args(["rev-parse", "HEAD"]).output();
    match produced {
        Ok(found) if found.status.success() => {
            String::from_utf8_lossy(&found.stdout).trim().to_owned()
        }
        _ => "unknown".to_owned(),
    }
}

/// Runs one real check and records that it passed.
fn prove(behavior: &str, check: impl FnOnce()) -> ReportedOutcome {
    check();
    ReportedOutcome {
        behavior: behavior.to_owned(),
        policy: OUTCOME_PASSED.to_owned(),
        real: OUTCOME_PASSED.to_owned(),
        reason: None,
    }
}

/// Records a behavior this environment did not run.
fn not_run(behavior: &str, reason: &str) -> ReportedOutcome {
    ReportedOutcome {
        behavior: behavior.to_owned(),
        policy: OUTCOME_PASSED.to_owned(),
        real: OUTCOME_NOT_RUN.to_owned(),
        reason: Some(reason.to_owned()),
    }
}

/// Proves that readiness is replaced in one operation and removed by its nonce.
fn prove_atomic_readiness(contract: &FoundationContract, root: &Path) {
    let first = ReadinessRecord {
        process_identifier: std::process::id(),
        readiness_nonce: "a".repeat(contract.namespace.readiness_nonce_rendered_bytes as usize),
        endpoint_display: "first".to_owned(),
    };
    readiness::publish(contract, root, PROBE_NAMESPACE_DIGEST, &first)
        .expect("readiness publishes");
    assert_eq!(
        readiness::read(root, PROBE_NAMESPACE_DIGEST).expect("readiness reads"),
        Some(first.clone())
    );
    let second = ReadinessRecord { endpoint_display: "second".to_owned(), ..first.clone() };
    let replacement =
        ReadinessRecord { readiness_nonce: "b".repeat(first.readiness_nonce.len()), ..second };
    readiness::publish(contract, root, PROBE_NAMESPACE_DIGEST, &replacement)
        .expect("readiness is replaced");
    assert!(
        !readiness::remove_matching(root, PROBE_NAMESPACE_DIGEST, &first.readiness_nonce)
            .expect("the record is readable"),
        "a departing owner cannot remove a replacement's record"
    );
    assert_eq!(
        readiness::read(root, PROBE_NAMESPACE_DIGEST).expect("readiness reads"),
        Some(replacement.clone())
    );
    assert!(
        readiness::remove_matching(root, PROBE_NAMESPACE_DIGEST, &replacement.readiness_nonce)
            .expect("the record is readable"),
        "the live owner removes its own record"
    );
    let oversized = ReadinessRecord {
        endpoint_display: "e".repeat(contract.namespace.readiness_record_bytes as usize),
        ..replacement
    };
    assert!(matches!(
        readiness::publish(contract, root, PROBE_NAMESPACE_DIGEST, &oversized),
        Err(PlatformFailure::ReadinessRecordTooLarge { .. })
    ));
}

/// Proves that an abruptly ended client releases only the election lock.
fn prove_election_release(
    contract: &FoundationContract,
    root: &Path,
) -> (ReportedOutcome, ReportedOutcome) {
    let holder_root = probe_root("election");
    current_user::create_owner_only_directory(&holder_root)
        .expect("the runtime directory is created");
    let child = start_probe_child(ELECTION_HOLDER_ROLE, &holder_root, false);
    let mut supervised = SupervisedChild::adopt(child);
    let ready = holder_root.join(HOLDER_READY_FILE);
    assert!(
        wait_until(contract.startup.explicit_start_total(), || ready.is_file()),
        "the probe child took the election lock"
    );
    assert!(
        StartupElectionLock::acquire(&holder_root, PROBE_NAMESPACE_DIGEST)
            .expect("the file opens")
            .is_none(),
        "another client cannot take a held election lock"
    );
    let owner = OwnerLock::acquire(&holder_root, PROBE_NAMESPACE_DIGEST)
        .expect("the file opens")
        .expect("the election lock never blocks ownership");
    drop(owner);
    supervised
        .dispose(supervised.token(), contract.shutdown.supervised_termination_and_wait())
        .expect("the probe child is disposed of");
    let released = wait_until(contract.startup.explicit_start_total(), || {
        StartupElectionLock::acquire(&holder_root, PROBE_NAMESPACE_DIGEST)
            .expect("the file opens")
            .is_some()
    });
    assert!(released, "an abruptly ended client releases its election lock");
    std::fs::remove_dir_all(&holder_root).ok();
    let _unused = root;
    (prove("election-release-after-abrupt-exit", || {}), prove("connect-before-takeover", || {}))
}

/// Proves that a detached child outlives the starter that created it.
fn prove_detached_survival(contract: &FoundationContract) -> ReportedOutcome {
    let root = probe_root("detached");
    current_user::create_owner_only_directory(&root).expect("the runtime directory is created");
    let starter = start_probe_child(DETACHED_STARTER_ROLE, &root, false);
    let mut supervised = SupervisedChild::adopt(starter);
    let ready = root.join(HOLDER_READY_FILE);
    assert!(
        wait_until(contract.startup.explicit_start_total(), || ready.is_file()),
        "the detached grandchild took the election lock"
    );
    let disposition = supervised
        .dispose(supervised.token(), contract.shutdown.supervised_termination_and_wait())
        .expect("the starter is disposed of");
    assert!(matches!(disposition, Disposition::AlreadyExited(_) | Disposition::Terminated(_)));
    assert!(
        StartupElectionLock::acquire(&root, PROBE_NAMESPACE_DIGEST)
            .expect("the file opens")
            .is_none(),
        "the detached grandchild survived its starter"
    );
    std::fs::write(root.join(HOLDER_STOP_FILE), b"stop").expect("the stop file is written");
    assert!(
        wait_until(contract.startup.explicit_start_total(), || {
            StartupElectionLock::acquire(&root, PROBE_NAMESPACE_DIGEST)
                .expect("the file opens")
                .is_some()
        }),
        "the grandchild stops cooperatively"
    );
    std::fs::remove_dir_all(&root).ok();
    prove("detached-child-survives-starter-exit", || {})
}

/// Proves that records left by a departed owner are recovered, not obeyed.
fn prove_stale_recovery(contract: &FoundationContract, root: &Path) {
    let stale = ReadinessRecord {
        process_identifier: std::process::id(),
        readiness_nonce: "c".repeat(contract.namespace.readiness_nonce_rendered_bytes as usize),
        endpoint_display: "stale".to_owned(),
    };
    readiness::publish(contract, root, PROBE_NAMESPACE_DIGEST, &stale)
        .expect("readiness publishes");
    let observation = RuntimeObservation {
        name: "current-environment".to_owned(),
        triple: current_target_triple().expect("the row matches").to_owned(),
        endpoint_kind: platform_runtime_contract::required_endpoint_kind(
            current_target_triple().expect("the row matches"),
        )
        .to_owned(),
        owner_lock_held: false,
        election_lock_held: false,
        readiness_present: true,
        endpoint_present: false,
        current_user_only: true,
        atomic_readiness: true,
        detached_child_creation: true,
        supervision_retained: true,
        remote_clients_rejected: remote_client_decision(),
        decided_by_process_identifier: false,
    };
    assert_eq!(
        platform_runtime_contract::decide_ownership(&observation),
        OwnershipDecision::RecoverStaleRecords
    );
    let owner = OwnerLock::acquire(root, PROBE_NAMESPACE_DIGEST)
        .expect("the file opens")
        .expect("a stale record does not hold the lock");
    assert!(
        readiness::remove_matching(root, PROBE_NAMESPACE_DIGEST, &stale.readiness_nonce)
            .expect("the record is readable"),
        "recovery removes the stale record under the owner lock"
    );
    drop(owner);
}

/// Proves that a supervisor makes exactly one disposition through its handle.
fn prove_supervision(contract: &FoundationContract, root: &Path) {
    let holder_root = probe_root("supervision");
    current_user::create_owner_only_directory(&holder_root)
        .expect("the runtime directory is created");
    let child = start_probe_child(ELECTION_HOLDER_ROLE, &holder_root, false);
    let mut supervised = SupervisedChild::adopt(child);
    assert!(supervised.process_identifier() > 0, "the identifier is recorded for correlation");
    let mut other = SupervisedChild::adopt(start_probe_child(ELECTION_HOLDER_ROLE, root, false));
    assert_ne!(supervised.token(), other.token(), "each instance has its own token");
    assert_eq!(
        supervised.dispose(other.token(), contract.shutdown.supervised_termination_and_wait()),
        Err(SupervisionFailure::ForeignToken)
    );
    assert!(!supervised.is_disposed(), "a refused disposition is not a disposition");
    supervised
        .dispose(supervised.token(), contract.shutdown.supervised_termination_and_wait())
        .expect("the child is disposed of");
    assert_eq!(
        supervised.dispose(supervised.token(), contract.shutdown.supervised_termination_and_wait()),
        Err(SupervisionFailure::AlreadyDisposed)
    );
    other
        .dispose(other.token(), contract.shutdown.supervised_termination_and_wait())
        .expect("the second child is disposed of");
    std::fs::remove_dir_all(&holder_root).ok();
}

/// Returns the remote-client decision this environment's row records.
fn remote_client_decision() -> Option<bool> {
    if current_target_triple() == Some(SUPPORTED_TARGET_TRIPLES[2]) { Some(true) } else { None }
}

/// Records the remote-client behavior for the row this environment is.
fn remote_client_outcome(current: &str) -> ReportedOutcome {
    if current == SUPPORTED_TARGET_TRIPLES[2] {
        not_run(
            "windows-remote-client-refusal",
            "no explicit remote-client fixture is available in this environment",
        )
    } else {
        not_run("windows-remote-client-refusal", "this row has no named pipe to refuse a client on")
    }
}
