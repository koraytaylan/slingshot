//! The walking proof: real processes, one daemon, and nothing left behind.
//!
//! Every claim here is made with independent operating-system processes. The
//! product dispatcher is run in an explicit test host, the daemon it creates is a real
//! detached child, and every deadline comes from the foundation contract and is
//! waited for against the monotonic clock rather than slept through.
//!
//! This suite also creates a private configuration tree and exercises the
//! descriptor-bound POSIX ACL policy. That fixture is intentionally limited to
//! Linux, where the hosted gate exposes the ACL namespace the policy verifies.

#![cfg(target_os = "linux")]

const DIGEST_HEX_CHARACTERS: usize = 64;

use std::path::PathBuf;
use std::sync::mpsc;
use std::thread;

use slingshot_command_line::explicit_daemon_start::DAEMON_SERVE_COMMAND;
use slingshot_daemon::platform_runtime::endpoint;
use slingshot_daemon::platform_runtime::locks::{OwnerLock, StartupElectionLock};
use slingshot_daemon::platform_runtime::readiness;
use slingshot_daemon::runtime_namespace::RuntimeNamespace;
use slingshot_local_protocol::foundation_contract::FoundationContract;
use slingshot_test_support::process_harness::{
    CapturedProcess, ExecutablePath, ProcessHarness, ProcessRequest, ReleaseBarrier,
};
use slingshot_test_support::runtime_harness::{TemporaryRuntimeRoot, wait_until};
use slingshot_test_support::supervised_child::SupervisedChild;

/// Directory holding the hand-authored normalized outputs.
const FIXTURE_DIRECTORY: &str = "tests/fixtures/walking-skeleton";

/// Profile every assertion names its target with.
const PROFILE: &str = "local";

/// Environment every assertion names its first target with.
const ENVIRONMENT: &str = "author";

/// Every line a start may write, because either is one correct answer.
///
/// A cohort released together contains clients that found nothing and clients
/// that found what a neighbour had just created, and both reached the one
/// daemon that ends up owning the namespace. Which line a given client wrote is
/// a race; that there is exactly one owner is not.
const EVERY_START_LINE: &[&str] = &["daemon-start: created", "daemon-start: adopted"];

/// Environment the second target uses, to prove two owners coexist.
const SECOND_ENVIRONMENT: &str = "publish";

/// Returns the product executable this proof drives.
fn product_executable() -> ExecutablePath {
    ExecutablePath::new(PathBuf::from(env!("CARGO_BIN_EXE_slingshot-runtime-test-host")))
        .expect("the product executable was built")
}

/// A real private, digest-inventoried configuration; no account files change.
#[path = "support/runtime_fixture.rs"]
mod runtime_fixture;

fn configured_runtime_root(name: &str) -> TemporaryRuntimeRoot {
    let root = TemporaryRuntimeRoot::create(name).unwrap();
    runtime_fixture::prepare(root.path(), PROFILE, &[ENVIRONMENT, SECOND_ENVIRONMENT]);
    root
}

/// Reads one hand-authored normalized output.
fn fixture(name: &str) -> String {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(FIXTURE_DIRECTORY).join(name);
    std::fs::read_to_string(&path)
        .unwrap_or_else(|failure| panic!("{} could not be read: {failure}", path.display()))
        .trim_end()
        .to_owned()
}

/// Runs the product executable with one target and command.
fn run_product(root: &TemporaryRuntimeRoot, environment: &str, action: &str) -> CapturedProcess {
    let harness = ProcessHarness::new();
    let request = ProcessRequest::new(&[
        "--runtime-root",
        root.path().to_str().expect("the root is text"),
        "--profile",
        PROFILE,
        "--environment",
        environment,
        "daemon",
        action,
    ]);
    harness.run(&product_executable(), &request).expect("the product executable runs")
}

/// Returns the readiness nonce the daemon owning one target published.
///
/// Read from the record the daemon publishes rather than from what the command
/// printed. A nonce authorizes a stop, so it belongs in the runtime state a
/// daemon owns and not on a stream a caller may log, and a proof that needs one
/// reads it where it actually lives.
fn published_nonce(root: &TemporaryRuntimeRoot, environment: &str) -> Option<String> {
    let target = namespace(root, environment);
    readiness::read(target.runtime_root(), target.digest())
        .expect("the record is readable")
        .map(|record| record.readiness_nonce)
}

/// Returns the process identifier the daemon owning one target published.
fn published_identifier(root: &TemporaryRuntimeRoot, environment: &str) -> Option<u32> {
    let target = namespace(root, environment);
    readiness::read(target.runtime_root(), target.digest())
        .expect("the record is readable")
        .map(|record| record.process_identifier)
}

/// Names the runtime namespace of one target inside a temporary root.
fn namespace(root: &TemporaryRuntimeRoot, environment: &str) -> RuntimeNamespace {
    RuntimeNamespace::name(&FoundationContract::embedded(), root.path(), PROFILE, environment)
        .expect("the target names a namespace")
}

/// Reports whether no process owns one target.
fn owner_is_absent(namespace: &RuntimeNamespace) -> bool {
    OwnerLock::acquire(namespace.runtime_root(), namespace.digest())
        .expect("the lock file opens")
        .is_some()
}

/// Sends one nonce-bound cooperative stop and reports whether it was accepted.
fn stop_over_endpoint(
    contract: &FoundationContract,
    address: &endpoint::EndpointAddress,
    nonce: &str,
) -> bool {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("the runtime builds");
    runtime.block_on(async {
        let request = slingshot_local_protocol::envelope::ControlRequest {
            control_version: contract.control.version,
            request_identifier: "walking-cleanup".to_owned(),
            method: slingshot_local_protocol::ping::STOP_METHOD.to_owned(),
            arguments: serde_json::json!({ "readiness_nonce": nonce }),
        };
        slingshot_command_line::daemon_connection::exchange(contract, address, &request)
            .await
            .is_ok_and(|response| {
                response.outcome == slingshot_local_protocol::envelope::ResponseOutcome::Success
            })
    })
}

/// Stops the daemon that owns one target through its live nonce.
fn cooperatively_stop(root: &TemporaryRuntimeRoot, environment: &str) {
    let contract = FoundationContract::embedded();
    let target = namespace(root, environment);
    let Some(record) =
        readiness::read(target.runtime_root(), target.digest()).expect("the record is readable")
    else {
        return;
    };
    let address = endpoint::endpoint_address(&contract, root.path(), target.digest())
        .expect("the endpoint is named");
    assert!(
        stop_over_endpoint(&contract, &address, &record.readiness_nonce),
        "the daemon acknowledged its cooperative stop"
    );
    assert!(
        wait_until(contract.shutdown.cooperative_stop(), || owner_is_absent(&target)),
        "the daemon released its owner lock"
    );
}

#[test]
fn compiled_startup_publishes_selected_durable_identity() {
    let root = configured_runtime_root("identity");
    let target = namespace(&root, ENVIRONMENT);
    let mut child = std::process::Command::new(product_executable().path())
        .env(
            "SSL_CERT_FILE",
            root.path().join("fixture-home/.config/slingshot/profiles/local.toml"),
        )
        .env("SSL_CERT_DIR", root.path().join("fixture-home/.config/slingshot/profiles"))
        .args([
            "--runtime-root",
            root.path().to_str().unwrap(),
            "--profile",
            PROFILE,
            "--environment",
            ENVIRONMENT,
            "daemon",
            DAEMON_SERVE_COMMAND,
        ])
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .unwrap();
    let reached = wait_until(FoundationContract::embedded().startup.explicit_start_total(), || {
        target.readiness_path().is_file() || child.try_wait().unwrap().is_some()
    });
    if !reached || !target.readiness_path().is_file() {
        let _ = child.kill();
        let output = child.wait_with_output().unwrap();
        panic!("compiled startup failed: {}", String::from_utf8_lossy(&output.stderr));
    }
    let record = readiness::read(target.runtime_root(), target.digest()).unwrap().unwrap();
    let address =
        endpoint::endpoint_address(&FoundationContract::embedded(), root.path(), target.digest())
            .unwrap();
    let runtime = tokio::runtime::Builder::new_current_thread().enable_all().build().unwrap();
    for arguments in [
        serde_json::json!({}),
        serde_json::json!({"unexpected":"not-to-be-echoed"}),
        serde_json::Value::Null,
    ] {
        let valid = arguments == serde_json::json!({});
        let response = runtime
            .block_on(slingshot_command_line::daemon_connection::exchange(
                &FoundationContract::embedded(),
                &address,
                &slingshot_local_protocol::envelope::ControlRequest {
                    control_version: FoundationContract::embedded().control.version,
                    request_identifier: "startup-hello".into(),
                    method: slingshot_local_protocol::control::HELLO_METHOD.into(),
                    arguments,
                },
            ))
            .unwrap();
        assert_eq!(response.request_identifier, "startup-hello");
        if valid {
            let hello: slingshot_local_protocol::control::HelloResult =
                serde_json::from_value(response.result.unwrap()).unwrap();
            let published = record.identity.as_ref().unwrap();
            assert_eq!(
                hello.author_target_identity_digest,
                published.author_target_identity_digest
            );
            assert_eq!(
                hello.selected_environment_revision,
                published.selected_environment_revision
            );
            assert_eq!(
                hello.daemon_runtime_contract_digest,
                published.daemon_runtime_contract_digest
            );
            assert_eq!(hello.readiness_nonce, record.readiness_nonce);
            assert_eq!(hello.runtime_namespace, target.display());
            assert_eq!(
                hello.supported_operation_protocol_versions,
                vec![
                    slingshot_domain::daemon_runtime_contract::DaemonRuntimeContract::embedded()
                        .operation_protocol_version as u32
                ]
            );
        } else {
            let error = response.error.unwrap();
            assert_eq!(error.code, slingshot_local_protocol::envelope::MALFORMED_REQUEST_CODE);
            assert!(!error.message.contains("not-to-be-echoed"));
        }
    }
    cooperatively_stop(&root, ENVIRONMENT);
    assert!(child.wait().unwrap().success());
    let identity = record.identity.unwrap();
    assert_eq!(identity.retained_control_version, FoundationContract::embedded().control.version);
    assert_eq!(
        identity.daemon_runtime_contract_digest,
        slingshot_domain::daemon_runtime_contract::DaemonRuntimeContract::embedded_digest()
            .as_text()
    );
    assert_eq!(
        identity.supported_operation_versions,
        vec![
            slingshot_domain::daemon_runtime_contract::DaemonRuntimeContract::embedded()
                .operation_protocol_version,
        ]
    );
    let ledger =
        slingshot_storage::installation_state::InstallationState::at(root.path().join("state"))
            .read()
            .unwrap();
    assert_eq!(
        ledger.registration(&target.key()),
        Some(slingshot_domain::installation::TargetRegistration::Registered)
    );
    let path = target.beneath(&root.path().join("state")).database_path();
    let database =
        slingshot_storage::database::OperationDatabase::open_live(&path, database_settings())
            .unwrap();
    assert_eq!(database.installation_identifier().unwrap(), Some(ledger.installation_identifier));
    drop(database);
    let restarted = run_product(&root, ENVIRONMENT, "start");
    assert!(restarted.status.success(), "{restarted:?}");
    let again = readiness::read(target.runtime_root(), target.digest()).unwrap().unwrap();
    cooperatively_stop(&root, ENVIRONMENT);
    assert_eq!(
        again.identity,
        Some(identity),
        "ambient certificate selectors changed the selected revision"
    );
    assert_ne!(again.readiness_nonce, record.readiness_nonce);
}

fn database_settings() -> slingshot_storage::database::RequiredSettings {
    let limits = slingshot_domain::daemon_runtime_contract::DaemonRuntimeContract::embedded();
    slingshot_storage::database::RequiredSettings {
        page_bytes: limits.limit("sqlite_page_bytes"),
        database_pages: limits.limit("maximum_sqlite_database_pages"),
        busy_timeout_milliseconds: limits.limit("database_busy_timeout_milliseconds"),
    }
}

#[test]
fn compiled_startup_refusals_leave_no_readiness_or_owner() {
    for (index, defect) in [
        "configuration",
        "installation",
        "database",
        "diagnostics",
        "stage",
        "endpoint",
        "foreign-revision",
    ]
    .into_iter()
    .enumerate()
    {
        let root = configured_runtime_root(&format!("fail{index}"));
        let target = namespace(&root, ENVIRONMENT);
        let started = run_product(&root, ENVIRONMENT, "start");
        assert!(started.status.success(), "{defect}: {started:?}");
        let identity = readiness::read(target.runtime_root(), target.digest())
            .unwrap()
            .unwrap()
            .identity
            .unwrap();
        cooperatively_stop(&root, ENVIRONMENT);
        let state_root = root.path().join("state");
        let paths = target.beneath(&state_root);
        let ledger = slingshot_storage::installation_state::InstallationState::at(&state_root);
        match defect {
            "configuration" => std::fs::remove_file(
                root.path().join("fixture-home/.config/slingshot/configuration-snapshot.toml"),
            )
            .unwrap(),
            "installation" => {
                let mut record = ledger.read().unwrap();
                record.installation_identifier =
                    slingshot_domain::installation::InstallationIdentifier::parse(
                        &"f".repeat(DIGEST_HEX_CHARACTERS),
                    )
                    .unwrap();
                ledger.replace(&record).unwrap();
            }
            "database" => std::fs::remove_file(paths.database_path()).unwrap(),
            "diagnostics" => {
                std::fs::remove_dir(paths.diagnostic_root()).unwrap();
                std::fs::write(paths.diagnostic_root(), b"preserved obstacle").unwrap();
            }
            "stage" => {
                let path = paths
                    .artifact_root()
                    .join(slingshot_storage::artifact_store::CONTENT_DIRECTORY)
                    .join("not-a-uuid.partial");
                std::fs::write(path, b"preserved partial bytes").unwrap();
            }
            "endpoint" => {
                let endpoint::EndpointAddress::UnixDomainSocket(path) = endpoint::endpoint_address(
                    &FoundationContract::embedded(),
                    root.path(),
                    target.digest(),
                )
                .unwrap();
                std::fs::write(path, b"preserved endpoint obstacle").unwrap();
            }
            "foreign-revision" => {
                use slingshot_domain::command_fingerprint::{CommandFingerprint, FingerprintInput};
                let database = slingshot_storage::database::OperationDatabase::open_live(
                    &paths.database_path(),
                    database_settings(),
                )
                .unwrap();
                let installation = database.installation_identifier().unwrap().unwrap();
                let operations =
                    slingshot_storage::operation_repository::OperationRepository::new(database);
                let canonical = "{\"root_path\":\"/retained\"}";
                let revision = "a".repeat(DIGEST_HEX_CHARACTERS);
                let fingerprint = CommandFingerprint::derive(&FingerprintInput {
                    author_target_identity_digest: identity.author_target_identity_digest.clone(),
                    canonical_command: canonical.into(),
                    command_wire_name: "query_paths".into(),
                    command_semantic_contract_version: "1".into(),
                    selected_environment_revision: revision.clone(),
                })
                .unwrap();
                operations
                    .admit(
                        &slingshot_storage::operation_repository::AdmissionRequest {
                            author_target_identity: "fixture".into(),
                            author_target_identity_digest: identity.author_target_identity_digest,
                            caller_identity: Some("fixture".into()),
                            canonical_command: canonical.into(),
                            command_fingerprint: fingerprint,
                            command_wire_name: "query_paths".into(),
                            daemon_runtime_contract_digest: identity.daemon_runtime_contract_digest,
                            installation_identifier: installation,
                            operation_identifier: "retained-foreign".into(),
                            selected_environment_revision: revision,
                            workflow_correlation_identifier: None,
                        },
                        1000,
                    )
                    .unwrap();
            }
            _ => unreachable!(),
        }
        let database_before = std::fs::read(paths.database_path()).ok();
        let ledger_before = std::fs::read(ledger.record_path()).unwrap();
        let refused = run_product(&root, ENVIRONMENT, DAEMON_SERVE_COMMAND);
        assert_eq!(
            refused.status.code(),
            Some(i32::from(slingshot_command_line::command_line::EXIT_RUNTIME_UNUSABLE)),
            "{defect}: {refused:?}"
        );
        assert!(refused.standard_output.is_empty());
        assert!(!target.readiness_path().exists(), "{defect} published readiness");
        assert!(owner_is_absent(&target), "{defect} retained ownership");
        let endpoint::EndpointAddress::UnixDomainSocket(endpoint_path) =
            endpoint::endpoint_address(
                &FoundationContract::embedded(),
                root.path(),
                target.digest(),
            )
            .unwrap();
        if defect == "endpoint" {
            assert_eq!(std::fs::read(endpoint_path).unwrap(), b"preserved endpoint obstacle");
        } else {
            assert!(!endpoint_path.exists(), "{defect} left an endpoint");
        }
        assert_eq!(
            std::fs::read(paths.database_path()).ok(),
            database_before,
            "{defect} changed the database"
        );
        assert_eq!(
            std::fs::read(ledger.record_path()).unwrap(),
            ledger_before,
            "{defect} changed the ledger"
        );
    }
}

#[tokio::test]
async fn readiness_publication_failure_and_cancelled_startup_unwind_the_endpoint() {
    use slingshot_command_line::daemon_entry::{
        DaemonEntryArguments, DaemonEntryOutcome, run_daemon_entry_for_test,
    };
    use slingshot_configuration::configuration_root::{
        AccountResolver as _, ConfigurationRoot, OperatingSystemAccountResolver,
    };
    for cancelled in [false, true] {
        let root = configured_runtime_root(if cancelled { "cancel" } else { "publish" });
        let target = namespace(&root, ENVIRONMENT);
        let account = OperatingSystemAccountResolver.resolve().unwrap();
        let configuration =
            ConfigurationRoot::at_explicit_home(account.identity, root.path().join("fixture-home"));
        let mut contract = FoundationContract::embedded();
        contract.namespace.readiness_record_bytes = 1;
        let cancellation = tokio_util::sync::CancellationToken::new();
        if cancelled {
            cancellation.cancel();
        }
        let result = run_daemon_entry_for_test(
            &contract,
            &DaemonEntryArguments::new(root.path(), PROFILE, ENVIRONMENT),
            cancellation,
            configuration,
            root.path().join("state"),
        )
        .await;
        if cancelled {
            assert_eq!(result.unwrap(), DaemonEntryOutcome::Served);
        } else {
            assert!(matches!(
                result.unwrap_err(),
                slingshot_command_line::daemon_entry::DaemonEntryFailure::Runtime(_)
            ));
        }
        assert!(!target.readiness_path().exists());
        let endpoint::EndpointAddress::UnixDomainSocket(path) =
            endpoint::endpoint_address(&contract, root.path(), target.digest()).unwrap();
        assert!(!path.exists());
        assert!(owner_is_absent(&target));
    }
}

#[test]
fn twenty_barrier_released_clients_converge_on_one_daemon() {
    let contract = FoundationContract::embedded();
    let root = configured_runtime_root("w");
    let target = namespace(&root, ENVIRONMENT);

    let absent = run_product(&root, ENVIRONMENT, "ping");
    assert!(absent.status.success());
    assert_eq!(absent.single_result_line(), fixture("ping-absent.txt"));
    assert!(owner_is_absent(&target), "an existing-only probe started nothing");

    let cohort = contract.process_harness.walking_start_client_count as usize;
    let barrier = ReleaseBarrier::new(cohort);
    let (reporting, reported) = mpsc::channel();
    let mut clients = Vec::new();
    for _ in 0..cohort {
        let barrier = barrier.clone();
        let reporting = reporting.clone();
        let path = root.path().to_path_buf();
        clients.push(thread::spawn(move || {
            let harness = ProcessHarness::new();
            let request = ProcessRequest::new(&[
                "--runtime-root",
                path.to_str().expect("the root is text"),
                "--profile",
                PROFILE,
                "--environment",
                ENVIRONMENT,
                "daemon",
                "start",
            ]);
            barrier.release();
            let produced = harness.run(&product_executable(), &request).expect("the client runs");
            reporting.send(produced).expect("the report is sent");
        }));
    }
    drop(reporting);
    for client in clients {
        client.join().expect("the client finishes");
    }

    let mut received = 0_usize;
    for produced in reported {
        received += 1;
        assert!(produced.status.success(), "{produced:?}");
        assert!(produced.standard_error.is_empty(), "a served start writes no diagnostic");
        let reached = produced.single_result_line().to_owned();
        assert!(EVERY_START_LINE.contains(&reached.as_str()), "{reached}");
    }
    assert_eq!(received, cohort, "every client reported");
    let nonce = published_nonce(&root, ENVIRONMENT).expect("the owner published a nonce");
    assert!(published_identifier(&root, ENVIRONMENT).is_some(), "one process owns the namespace");

    let running = run_product(&root, ENVIRONMENT, "ping");
    assert_eq!(running.single_result_line(), fixture("ping-serving.txt"));

    let second = run_product(&root, SECOND_ENVIRONMENT, "start");
    assert!(second.status.success());
    let second_nonce =
        published_nonce(&root, SECOND_ENVIRONMENT).expect("the second owner published a nonce");
    assert_ne!(second_nonce, nonce, "two namespaces are two owners");

    let first_again = run_product(&root, ENVIRONMENT, "ping");
    assert_eq!(first_again.single_result_line(), fixture("ping-serving.txt"));
    assert_eq!(published_nonce(&root, ENVIRONMENT), Some(nonce.clone()));
    let nonces = [nonce];

    cooperatively_stop(&root, SECOND_ENVIRONMENT);
    cooperatively_stop(&root, ENVIRONMENT);

    let stale_nonce = nonces[0].clone();
    let after = run_product(&root, ENVIRONMENT, "ping");
    assert_eq!(after.single_result_line(), fixture("ping-absent.txt"));

    let recovered = run_product(&root, ENVIRONMENT, "start");
    assert!(recovered.status.success(), "{recovered:?}");
    let fresh = published_nonce(&root, ENVIRONMENT).expect("the replacement published a nonce");
    assert_ne!(fresh, stale_nonce, "a new cohort recovered a fresh nonce");
    let address = endpoint::endpoint_address(&contract, root.path(), target.digest())
        .expect("the endpoint is named");
    assert!(
        !stop_over_endpoint(&contract, &address, &stale_nonce),
        "a stale nonce cannot stop the replacement"
    );
    cooperatively_stop(&root, ENVIRONMENT);
    assert!(owner_is_absent(&target), "nothing owns the target once the proof finishes");
}

#[test]
fn a_supervised_daemon_is_ended_through_its_own_handle_and_leaves_nothing_behind() {
    let contract = FoundationContract::embedded();
    let root = configured_runtime_root("v");
    let target = namespace(&root, ENVIRONMENT);
    let child = std::process::Command::new(product_executable().path())
        .args(["--runtime-root", root.path().to_str().expect("the root is text")])
        .args(["--profile", PROFILE, "--environment", ENVIRONMENT, "daemon", DAEMON_SERVE_COMMAND])
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
        .expect("the daemon child starts");
    let mut supervised = SupervisedChild::adopt(child);
    assert!(
        wait_until(contract.startup.explicit_start_total(), || !owner_is_absent(&target)),
        "the supervised daemon took ownership"
    );
    let other = SupervisedChild::adopt(
        std::process::Command::new(product_executable().path())
            .arg("--version")
            .stdout(std::process::Stdio::null())
            .spawn()
            .expect("a second child starts"),
    );
    assert!(
        supervised
            .dispose(other.token(), contract.shutdown.supervised_termination_and_wait())
            .is_err(),
        "a token from another instance cannot redirect a disposition"
    );
    supervised
        .dispose(supervised.token(), contract.shutdown.supervised_termination_and_wait())
        .expect("the daemon is ended through its own handle");
    assert!(
        supervised
            .dispose(supervised.token(), contract.shutdown.supervised_termination_and_wait())
            .is_err(),
        "a child accepts exactly one disposition"
    );
    assert!(
        wait_until(contract.shutdown.cooperative_stop(), || owner_is_absent(&target)),
        "the ended daemon released its owner lock"
    );
    let after = run_product(&root, ENVIRONMENT, "ping");
    assert_eq!(after.single_result_line(), fixture("ping-absent.txt"));
    assert!(
        OwnerLock::path_for(root.path(), target.digest()).is_file(),
        "the lock file is persistent"
    );
}

#[test]
fn an_abandoned_election_never_blocks_the_cohort_that_follows_it() {
    let root = configured_runtime_root("u");
    let target = namespace(&root, ENVIRONMENT);
    let held = StartupElectionLock::acquire(root.path(), target.digest())
        .expect("the lock file opens")
        .expect("the election lock is free");
    assert!(owner_is_absent(&target), "an election is never ownership");
    drop(held);
    let created = run_product(&root, ENVIRONMENT, "start");
    assert!(created.status.success(), "{created:?}");
    assert_eq!(created.single_result_line(), fixture("start-created.txt"));
    cooperatively_stop(&root, ENVIRONMENT);
}

#[test]
fn a_refused_invocation_writes_one_diagnostic_and_no_result() {
    let root = configured_runtime_root("q");
    let harness = ProcessHarness::new();
    let request = ProcessRequest::new(&[
        "--runtime-root",
        root.path().to_str().expect("the root is text"),
        "daemon",
        "ping",
    ]);
    let produced = harness.run(&product_executable(), &request).expect("the executable runs");
    assert!(!produced.status.success());
    assert!(produced.standard_output.is_empty(), "a refused invocation writes no result");
    assert_eq!(produced.standard_error.trim_end(), fixture("target-unusable-diagnostic.txt"));
    assert_eq!(harness.owned_count(), 0, "a completed run leaves no owned child");
}
