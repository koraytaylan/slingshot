#![cfg(not(windows))]

//! Assertions for explicit daemon start and existing-only ping.
//!
//! Every assertion runs against real detached daemon processes inside an
//! injected temporary runtime root, so convergence, election, and absence are
//! observed rather than simulated.

use std::path::{Path, PathBuf};
use std::process::Command;
#[cfg(target_os = "linux")]
use std::time::Duration;

#[cfg(target_os = "linux")]
#[path = "support/runtime_fixture.rs"]
mod runtime_fixture;

use slingshot_command_line::command_line::{EXIT_SUCCESS, EXIT_TARGET_UNUSABLE};
#[cfg(target_os = "linux")]
use slingshot_command_line::daemon_connection;
use slingshot_command_line::explicit_daemon_start::{self, StartFailure, TargetRuntime};
#[cfg(target_os = "linux")]
use slingshot_command_line::explicit_daemon_start::{StartDisposition, StartReport};
#[cfg(target_os = "linux")]
use slingshot_daemon::platform_runtime::endpoint;
#[cfg(target_os = "linux")]
use slingshot_daemon::platform_runtime::locks::{OwnerLock, StartupElectionLock};
use slingshot_daemon::runtime_namespace::NamespaceFailure;
#[cfg(target_os = "linux")]
use slingshot_daemon::runtime_namespace::RuntimeNamespace;
#[cfg(target_os = "linux")]
use slingshot_local_protocol::envelope::{ControlRequest, ResponseOutcome};
use slingshot_local_protocol::foundation_contract::FoundationContract;
#[cfg(target_os = "linux")]
use slingshot_local_protocol::ping::STOP_METHOD;
use slingshot_test_support::runtime_harness::runtime_root_path;

/// Profile the assertions name their target with.
const PROFILE: &str = "local";

/// Environment the assertions name their target with.
const ENVIRONMENT: &str = "author";

/// Number of clients one convergence assertion releases at once.
#[cfg(target_os = "linux")]
const CONVERGING_CLIENT_COUNT: usize = 12;

/// Interval between two polls while waiting for a real condition.
#[cfg(target_os = "linux")]
const POLL_INTERVAL: Duration = Duration::from_millis(10);

/// Returns the product executable this assertion drives.
fn product_executable() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_slingshot-runtime-test-host"))
}

/// Creates an injected temporary runtime root that no other assertion shares.
///
/// The name is short because a Unix domain socket address is bounded and the
/// namespace digest takes most of that bound.
fn temporary_runtime_root(name: &str) -> PathBuf {
    let root = runtime_root_path(name);
    std::fs::remove_dir_all(&root).ok();
    root
}

/// Names one target inside an injected runtime root.
fn target(root: &Path, environment: &str) -> TargetRuntime {
    TargetRuntime {
        runtime_root: root.to_path_buf(),
        profile: PROFILE.to_owned(),
        environment: environment.to_owned(),
    }
}

/// Stops the daemon that owns one target, if one is running.
#[cfg(target_os = "linux")]
async fn stop_daemon(target: &TargetRuntime) {
    let contract = FoundationContract::embedded();
    let Ok(report) = explicit_daemon_start::existing_only_ping(&contract, target, "cleanup").await
    else {
        return;
    };
    let Some(nonce) = report.readiness_nonce else {
        return;
    };
    let namespace = RuntimeNamespace::name(
        &contract,
        &target.runtime_root,
        &target.profile,
        &target.environment,
    )
    .expect("the target names a namespace");
    let address = endpoint::endpoint_address(&contract, &target.runtime_root, namespace.digest())
        .expect("the endpoint is named");
    let request = ControlRequest {
        control_version: contract.control.version,
        request_identifier: "cleanup".to_owned(),
        method: STOP_METHOD.to_owned(),
        arguments: serde_json::json!({ "readiness_nonce": nonce }),
    };
    let response = daemon_connection::exchange(&contract, &address, &request)
        .await
        .expect("the cooperative stop is answered");
    assert_eq!(response.outcome, ResponseOutcome::Success);
    wait_until(contract.shutdown.cooperative_stop(), || {
        OwnerLock::acquire(&target.runtime_root, namespace.digest())
            .expect("the lock file opens")
            .is_some()
    })
    .await;
}

/// Waits until a condition holds or the deadline elapses.
#[cfg(target_os = "linux")]
async fn wait_until(deadline: Duration, mut condition: impl FnMut() -> bool) -> bool {
    let started = tokio::time::Instant::now();
    while started.elapsed() < deadline {
        if condition() {
            return true;
        }
        tokio::time::sleep(POLL_INTERVAL).await;
    }
    condition()
}

/// Starts one daemon and returns the report the caller received.
#[cfg(target_os = "linux")]
async fn start(target: &TargetRuntime, identifier: &str) -> StartReport {
    explicit_daemon_start::explicit_start(
        &FoundationContract::embedded(),
        target,
        &product_executable(),
        identifier,
    )
    .await
    .expect("the start converges")
}

#[cfg(target_os = "linux")]
#[tokio::test(flavor = "multi_thread")]
async fn an_already_responsive_daemon_is_joined_by_start_and_reported_by_ping() {
    let root = temporary_runtime_root("a");
    runtime_fixture::prepare(&root, PROFILE, &[ENVIRONMENT]);
    let addressed = target(&root, ENVIRONMENT);
    let created = start(&addressed, "first").await;
    assert_eq!(created.disposition, StartDisposition::Started);

    let joined = start(&addressed, "second").await;
    assert_eq!(joined.disposition, StartDisposition::Joined);
    assert_eq!(joined.readiness_nonce, created.readiness_nonce);
    assert_eq!(joined.process_identifier, created.process_identifier);

    let probed = explicit_daemon_start::existing_only_ping(
        &FoundationContract::embedded(),
        &addressed,
        "probe",
    )
    .await
    .expect("the probe finishes");
    assert!(probed.running);
    assert_eq!(probed.readiness_nonce.as_deref(), Some(created.readiness_nonce.as_str()));
    assert_eq!(probed.process_identifier, Some(created.process_identifier));

    stop_daemon(&addressed).await;
    std::fs::remove_dir_all(&root).ok();
}

#[cfg(target_os = "linux")]
#[tokio::test(flavor = "multi_thread")]
async fn concurrent_starts_against_absence_create_one_daemon_and_share_one_nonce() {
    let root = temporary_runtime_root("c");
    runtime_fixture::prepare(&root, PROFILE, &[ENVIRONMENT]);
    let addressed = target(&root, ENVIRONMENT);
    let mut pending = Vec::new();
    for index in 0..CONVERGING_CLIENT_COUNT {
        let addressed = addressed.clone();
        pending
            .push(tokio::spawn(async move { start(&addressed, &format!("client-{index}")).await }));
    }
    let mut reports = Vec::new();
    for handle in pending {
        reports.push(handle.await.expect("the client finishes"));
    }
    let first = reports.first().expect("a client reported").clone();
    for report in &reports {
        assert_eq!(
            report.readiness_nonce, first.readiness_nonce,
            "every client reached one daemon"
        );
        assert_eq!(report.process_identifier, first.process_identifier);
    }
    let created =
        reports.iter().filter(|report| report.disposition == StartDisposition::Started).count();
    assert_eq!(created, 1, "exactly one client created the daemon");
    stop_daemon(&addressed).await;
    std::fs::remove_dir_all(&root).ok();
}

#[tokio::test(flavor = "multi_thread")]
async fn a_probe_against_absence_creates_nothing_and_takes_no_election_lock() {
    let root = temporary_runtime_root("p");
    let addressed = target(&root, ENVIRONMENT);
    let report = explicit_daemon_start::existing_only_ping(
        &FoundationContract::embedded(),
        &addressed,
        "probe",
    )
    .await
    .expect("the probe finishes");
    assert!(!report.running);
    assert_eq!(report.readiness_nonce, None);
    assert_eq!(report.process_identifier, None);
    assert!(!root.exists(), "a probe against absence creates no runtime state");
}

#[cfg(target_os = "linux")]
#[tokio::test(flavor = "multi_thread")]
async fn a_successor_starts_one_daemon_once_an_abandoned_election_is_released() {
    let contract = FoundationContract::embedded();
    let root = temporary_runtime_root("e");
    runtime_fixture::prepare(&root, PROFILE, &[ENVIRONMENT]);
    std::fs::create_dir_all(&root).expect("the runtime root is created");
    let addressed = target(&root, ENVIRONMENT);
    let namespace =
        RuntimeNamespace::name(&contract, &root, PROFILE, ENVIRONMENT).expect("it names");
    let held = StartupElectionLock::acquire(&root, namespace.digest())
        .expect("the lock file opens")
        .expect("the election lock is free");
    assert!(
        StartupElectionLock::acquire(&root, namespace.digest()).expect("the file opens").is_none(),
        "a second client cannot take a held election lock"
    );
    assert!(
        OwnerLock::acquire(&root, namespace.digest()).expect("the file opens").is_some(),
        "holding the election lock is never ownership"
    );
    drop(held);

    let created = start(&addressed, "successor").await;
    assert_eq!(created.disposition, StartDisposition::Started);
    stop_daemon(&addressed).await;
    std::fs::remove_dir_all(&root).ok();
}

#[test]
fn help_and_version_create_no_runtime_state() {
    let root = temporary_runtime_root("h");
    for arguments in [vec!["--version"], vec!["--help"]] {
        let produced =
            Command::new(product_executable()).args(&arguments).output().expect("it runs");
        assert!(produced.status.success(), "{arguments:?}");
        assert!(!produced.stdout.is_empty(), "{arguments:?}");
    }
    assert!(!root.exists(), "help and version touch no runtime namespace");
}

#[test]
fn an_invocation_that_names_no_target_exits_distinctly() {
    let root = temporary_runtime_root("t");
    let produced = Command::new(product_executable())
        .args(["--runtime-root"])
        .arg(&root)
        .args(["daemon", "ping"])
        .output()
        .expect("the executable runs");
    assert_eq!(produced.status.code(), Some(i32::from(EXIT_TARGET_UNUSABLE)));
    assert!(produced.stdout.is_empty(), "a refused invocation writes no result");
    assert!(!produced.stderr.is_empty(), "a refused invocation explains itself");
    assert!(!root.exists());

    let produced = Command::new(product_executable())
        .args(["--runtime-root"])
        .arg(&root)
        .args(["--profile", PROFILE, "--environment", ENVIRONMENT, "daemon", "ping"])
        .output()
        .expect("the executable runs");
    assert_eq!(produced.status.code(), Some(i32::from(EXIT_SUCCESS)));
    let rendered = String::from_utf8(produced.stdout).expect("the result is text");
    assert_eq!(rendered.lines().count(), 1, "{rendered}");
    assert!(rendered.contains("daemon-ping: absent"), "{rendered}");
    assert!(produced.stderr.is_empty(), "a served probe writes no diagnostic");
    std::fs::remove_dir_all(&root).ok();
}

/// Permission bits of a directory its owner's group and everybody else may enter.
const SHARED_DIRECTORY_MODE: u32 = 0o755;

/// Permission bits that grant anything to anybody but the owner.
#[cfg(target_os = "linux")]
const NOT_OWNER_PERMISSION_BITS: u32 = 0o077;

#[tokio::test(flavor = "multi_thread")]
async fn a_runtime_root_others_may_enter_is_refused_before_anything_is_contended_for() {
    use std::os::unix::fs::PermissionsExt as _;
    // A daemon refuses to own a namespace in such a root. Were the client to
    // accept it, the refusal would happen in a child with no terminal and the
    // caller would see only the whole start deadline elapse.
    let root = temporary_runtime_root("s");
    std::fs::create_dir_all(&root).expect("the runtime root is created");
    std::fs::set_permissions(&root, std::fs::Permissions::from_mode(SHARED_DIRECTORY_MODE))
        .expect("the runtime root is shared");
    let refused = explicit_daemon_start::explicit_start(
        &FoundationContract::embedded(),
        &target(&root, ENVIRONMENT),
        &product_executable(),
        "shared",
    )
    .await
    .expect_err("a shared runtime root is refused");
    assert!(
        matches!(refused, StartFailure::RuntimeRoot(NamespaceFailure::RootNotPrivate { .. })),
        "{refused}"
    );
    assert!(refused.to_string().contains("0700"), "the refusal says what to change: {refused}");
    let left = std::fs::read_dir(&root).expect("the root is readable").count();
    assert_eq!(left, 0, "no lock, log, or child was created for a refused root");
    std::fs::remove_dir_all(&root).ok();
}

#[tokio::test(flavor = "multi_thread")]
async fn a_daemon_that_exits_while_starting_is_reported_with_what_it_wrote() {
    // No fixture configuration is prepared, so the child refuses to start. The
    // client must say so, and quote the child, rather than wait out the whole
    // deadline and report only that nothing answered.
    let root = temporary_runtime_root("x");
    let refused = explicit_daemon_start::explicit_start(
        &FoundationContract::embedded(),
        &target(&root, ENVIRONMENT),
        &product_executable(),
        "exiting",
    )
    .await
    .expect_err("a daemon without configuration does not start");
    let StartFailure::DaemonExited { diagnostic, log, .. } = &refused else {
        panic!("the early exit is reported as one: {refused}")
    };
    assert!(diagnostic.contains("configuration could not be established"), "{refused}");
    assert_eq!(
        &explicit_daemon_start::startup_log_tail(log),
        diagnostic,
        "the quoted diagnostic is what the log holds"
    );
    assert!(refused.to_string().contains(&log.display().to_string()), "{refused}");
    std::fs::remove_dir_all(&root).ok();
}

#[cfg(target_os = "linux")]
#[tokio::test(flavor = "multi_thread")]
async fn a_daemon_that_starts_leaves_its_startup_log_private_to_its_owner() {
    use std::os::unix::fs::PermissionsExt as _;
    let root = temporary_runtime_root("l");
    runtime_fixture::prepare(&root, PROFILE, &[ENVIRONMENT]);
    let addressed = target(&root, ENVIRONMENT);
    start(&addressed, "logged").await;
    let namespace =
        RuntimeNamespace::name(&FoundationContract::embedded(), &root, PROFILE, ENVIRONMENT)
            .expect("it names");
    let log = explicit_daemon_start::startup_log_path(&root, namespace.digest());
    let mode = std::fs::metadata(&log).expect("the startup log exists").permissions().mode();
    assert_eq!(
        mode & NOT_OWNER_PERMISSION_BITS,
        0,
        "nobody but the owner may read what the daemon wrote"
    );
    stop_daemon(&addressed).await;
    std::fs::remove_dir_all(&root).ok();
}

#[cfg(target_os = "linux")]
#[tokio::test(flavor = "multi_thread")]
async fn a_start_that_names_no_root_owns_a_directory_of_its_own_beside_shared_data() {
    use std::os::unix::fs::PermissionsExt as _;
    // An earlier installation left the product's data directory readable by
    // everybody, which is what a platform without a per-login runtime
    // directory falls back to. The default root must not be that directory.
    let home = temporary_runtime_root("d");
    let data = home.join("data");
    let shared = data.join("slingshot");
    std::fs::create_dir_all(&shared).expect("the shared data directory is created");
    std::fs::set_permissions(&shared, std::fs::Permissions::from_mode(SHARED_DIRECTORY_MODE))
        .expect("the data directory is shared");
    let expected = shared.join(slingshot_command_line::command_line::DEFAULT_RUNTIME_DIRECTORY);
    runtime_fixture::prepare(&expected, PROFILE, &[ENVIRONMENT]);
    let produced = Command::new(product_executable())
        .env("HOME", &home)
        .env("XDG_DATA_HOME", &data)
        .env_remove("XDG_RUNTIME_DIR")
        .args(["--profile", PROFILE, "--environment", ENVIRONMENT, "daemon", "start"])
        .output()
        .expect("the executable runs");
    let diagnostics = String::from_utf8_lossy(&produced.stderr);
    assert_eq!(produced.status.code(), Some(i32::from(EXIT_SUCCESS)), "{diagnostics}");
    stop_daemon(&target(&expected, ENVIRONMENT)).await;
    std::fs::remove_dir_all(&home).ok();
}

#[test]
fn a_command_line_that_names_nothing_says_so_and_where_to_look() {
    let produced = Command::new(product_executable()).output().expect("the executable runs");
    assert_eq!(produced.status.code(), Some(i32::from(EXIT_TARGET_UNUSABLE)));
    let diagnostics = String::from_utf8(produced.stderr).expect("the diagnostic is text");
    assert!(diagnostics.contains("no command was given"), "{diagnostics}");
    assert!(diagnostics.contains("slingshot help"), "{diagnostics}");
}

#[test]
fn help_about_one_leaf_lists_the_options_that_leaf_takes() {
    let produced = Command::new(product_executable())
        .args(["help", "daemon", "start"])
        .output()
        .expect("the executable runs");
    assert!(produced.status.success(), "{}", String::from_utf8_lossy(&produced.stderr));
    let rendered = String::from_utf8(produced.stdout).expect("help is text");
    assert!(rendered.starts_with("daemon-start"), "{rendered}");
    for option in ["--profile", "--environment", "--runtime-root"] {
        assert!(rendered.contains(option), "{option}: {rendered}");
    }
    assert!(!rendered.contains("--operation-key"), "a start takes no key: {rendered}");
}
