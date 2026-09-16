//! A pending author exchange must not exclude local daemon control requests.

use std::sync::{Arc, mpsc};

use slingshot_daemon::{
    ownership::{Acquisition, DaemonOwnership},
    runtime_builder::{DurableRuntime, RuntimeBuilder},
    runtime_namespace::RuntimeNamespace,
    service::DaemonService,
};
use slingshot_domain::{
    command_fingerprint::{CommandFingerprint, FingerprintInput},
    daemon_runtime_contract::DaemonRuntimeContract,
};
use slingshot_local_protocol::foundation_contract::FoundationContract;
use slingshot_storage::{database::RequiredSettings, operation_repository::AdmissionRequest};
use tokio_util::sync::CancellationToken;

const OPERATION: &str = "pending-author";
const COMMAND: &str = "query_paths";
const ARGUMENTS: &str = r#"{"root_path":"/content"}"#;

fn runtime_at(root: &std::path::Path, endpoint: &str) -> DurableRuntime {
    let contract = FoundationContract::embedded();
    let namespace =
        RuntimeNamespace::name(&contract, &root.join("runtime"), "remote-site", "staging").unwrap();
    namespace.create_runtime_directory().unwrap();
    let Acquisition::Owned(owner) = DaemonOwnership::acquire(&contract, namespace).unwrap() else {
        panic!("fresh namespace must be owned")
    };
    let limits = DaemonRuntimeContract::embedded();
    RuntimeBuilder::new(
        super::selected_snapshot(endpoint),
        *owner,
        root.join("state"),
        RequiredSettings {
            page_bytes: limits.limit("sqlite_page_bytes"),
            database_pages: limits.limit("maximum_sqlite_database_pages"),
            busy_timeout_milliseconds: limits.limit("database_busy_timeout_milliseconds"),
        },
    )
    .unwrap()
    .establish_durable()
    .unwrap()
}

fn admit(runtime: &DurableRuntime) {
    let target = runtime.context().target();
    let contract = slingshot_domain::selected_command_contract_identity::SelectedCommandContractIdentity::installed(COMMAND).unwrap();
    runtime
        .operations()
        .admit(
            &AdmissionRequest {
                author_target_identity: "fixture".into(),
                author_target_identity_digest: target.author_target_identity_digest.clone(),
                caller_identity: None,
                canonical_command: ARGUMENTS.into(),
                command_fingerprint: CommandFingerprint::derive(&FingerprintInput {
                    author_target_identity_digest: target.author_target_identity_digest.clone(),
                    canonical_command: ARGUMENTS.into(),
                    command_wire_name: COMMAND.into(),
                    command_semantic_contract_version: contract.command_semantic_contract_version,
                    selected_environment_revision: target.selected_environment_revision.clone(),
                })
                .unwrap(),
                command_wire_name: COMMAND.into(),
                daemon_runtime_contract_digest: target.daemon_runtime_contract_digest.clone(),
                installation_identifier: runtime.installation().clone(),
                operation_identifier: OPERATION.into(),
                selected_environment_revision: target.selected_environment_revision.clone(),
                workflow_correlation_identifier: None,
            },
            1,
        )
        .unwrap();
}

#[test]
fn local_ping_and_status_complete_while_the_author_withholds_its_response() {
    let root = tempfile::tempdir().unwrap();
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    listener.set_nonblocking(true).unwrap();
    let endpoint = format!("http://{}", listener.local_addr().unwrap());
    let durable = runtime_at(root.path(), &endpoint);
    admit(&durable);
    let target = durable.context().target().clone();
    let contract = FoundationContract::embedded();
    let service = Arc::new(DaemonService::from_runtime(contract.clone(), durable));
    let shutdown = CancellationToken::new();
    let (started, received) = mpsc::channel();
    let worker_service = Arc::clone(&service);
    let worker_shutdown = shutdown.clone();
    let worker = std::thread::spawn(move || {
        let executor = tokio::runtime::Builder::new_current_thread().enable_all().build().unwrap();
        executor.block_on(tokio::task::LocalSet::new().run_until(async move {
            use tokio::io::AsyncReadExt as _;
            let listener = tokio::net::TcpListener::from_std(listener).unwrap();
            worker_service.start_scheduler(worker_shutdown.clone());
            tokio::select! {
                () = worker_shutdown.cancelled() => {},
                accepted = listener.accept() => {
                    let (mut socket, _) = accepted.unwrap();
                    let mut first_byte = [0_u8; 1];
                    socket.read_exact(&mut first_byte).await.unwrap();
                    started.send(()).unwrap();
                    // The socket remains open and no response is sent until
                    // the test has decided whether the local request finished.
                    worker_shutdown.cancelled().await;
                }
            }
            worker_service.join_scheduler().await;
        }));
    });
    let observed = received.recv_timeout(contract.startup.explicit_start_total());
    let (answered, answer) = mpsc::channel();
    let request_service = Arc::clone(&service);
    let request = serde_json::to_vec(&slingshot_local_protocol::envelope::ControlRequest {
        control_version: contract.control.version,
        request_identifier: "responsive-ping".into(),
        method: slingshot_local_protocol::ping::PING_METHOD.into(),
        arguments: serde_json::json!({}),
    })
    .unwrap();
    let status = serde_json::to_vec(&serde_json::json!({
        "author_target_identity_digest": target.author_target_identity_digest,
        "daemon_runtime_contract_digest": target.daemon_runtime_contract_digest,
        "operation_protocol_version": DaemonRuntimeContract::embedded().operation_protocol_version,
        "request_identifier": "responsive-status",
        "request": { "request": "operation_status", "operation_identifier": OPERATION },
        "selected_environment_revision": target.selected_environment_revision,
    }))
    .unwrap();
    let caller = std::thread::spawn(move || {
        let ping = request_service.answer(&request);
        let status = request_service.answer(&status);
        let _ = answered.send((ping, status));
    });
    let response = answer.recv_timeout(contract.server.response_write());
    // Release every task before asserting, including on the broken path.
    shutdown.cancel();
    worker.join().unwrap();
    caller.join().unwrap();
    observed.expect("the scheduler must reach the real author socket");
    let (response, status) = response.expect("a pending author exchange blocked a local request");
    let body = &response.frame()[std::mem::size_of::<u32>()..];
    let decoded: slingshot_local_protocol::envelope::ControlResponse =
        serde_json::from_slice(body).unwrap();
    assert_eq!(decoded.request_identifier, "responsive-ping");
    assert_eq!(decoded.outcome, slingshot_local_protocol::envelope::ResponseOutcome::Success);
    let body = &status.frame()[std::mem::size_of::<u32>()..];
    let decoded: slingshot_local_protocol::message::OperationResponse =
        serde_json::from_slice(body).unwrap();
    assert!(
        matches!(decoded, slingshot_local_protocol::message::OperationResponse::Status { operation_identifier, .. } if operation_identifier == OPERATION)
    );
}
