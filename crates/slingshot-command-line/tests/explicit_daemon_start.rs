//! Assertions for explicit daemon start and existing-only ping.
//!
//! Every assertion runs against real detached daemon processes inside an
//! injected temporary runtime root, so convergence, election, and absence are
//! observed rather than simulated.

use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Duration;

use slingshot_command_line::command_line::{EXIT_SUCCESS, EXIT_TARGET_UNUSABLE};
use slingshot_command_line::daemon_connection;
use slingshot_command_line::explicit_daemon_start::{
    self, StartDisposition, StartReport, TargetRuntime,
};
use slingshot_daemon::platform_runtime::endpoint;
use slingshot_daemon::platform_runtime::locks::{OwnerLock, StartupElectionLock};
use slingshot_daemon::runtime_namespace::RuntimeNamespace;
use slingshot_local_protocol::envelope::{ControlRequest, ResponseOutcome};
use slingshot_local_protocol::foundation_contract::FoundationContract;
use slingshot_local_protocol::ping::STOP_METHOD;

/// Profile the assertions name their target with.
const PROFILE: &str = "local";

/// Environment the assertions name their target with.
const ENVIRONMENT: &str = "author";

/// Number of clients one convergence assertion releases at once.
const CONVERGING_CLIENT_COUNT: usize = 12;

/// Interval between two polls while waiting for a real condition.
const POLL_INTERVAL: Duration = Duration::from_millis(10);

/// Returns the product executable this assertion drives.
fn product_executable() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_slingshot"))
}

/// Creates an injected temporary runtime root that no other assertion shares.
///
/// The name is short because a Unix domain socket address is bounded and the
/// namespace digest takes most of that bound.
fn temporary_runtime_root(name: &str) -> PathBuf {
    let root = std::env::temp_dir().join(format!("x{}{name}", std::process::id()));
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

#[tokio::test(flavor = "multi_thread")]
async fn an_already_responsive_daemon_is_joined_by_start_and_reported_by_ping() {
    let root = temporary_runtime_root("a");
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

#[tokio::test(flavor = "multi_thread")]
async fn concurrent_starts_against_absence_create_one_daemon_and_share_one_nonce() {
    let root = temporary_runtime_root("c");
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

#[tokio::test(flavor = "multi_thread")]
async fn a_successor_starts_one_daemon_once_an_abandoned_election_is_released() {
    let contract = FoundationContract::embedded();
    let root = temporary_runtime_root("e");
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
        let produced = Command::new(product_executable())
            .args(&arguments)
            .arg("--runtime-root")
            .arg(&root)
            .output()
            .expect("the executable runs");
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
    assert!(rendered.contains("\"running\":false"), "{rendered}");
    assert!(produced.stderr.is_empty(), "a served probe writes no diagnostic");
    std::fs::remove_dir_all(&root).ok();
}
