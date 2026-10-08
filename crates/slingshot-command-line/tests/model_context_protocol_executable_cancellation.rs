//! Executable cancellation through the product runner and a real framed daemon endpoint.
//!
//! The peer keeps a pending operation on disk independently of its observation
//! connections. Closing a wait connection cannot erase that retained operation.

#![cfg(unix)]

use serde_json::{Value, json};
use slingshot_command_line::model_context_protocol::standard_stream_transport::{
    SUPPORTED_REVISIONS, write_deadline,
};
use slingshot_daemon::platform_runtime::endpoint::{EndpointAddress, endpoint_address};
use slingshot_daemon::{local_server, runtime_namespace::RuntimeNamespace};
use slingshot_local_protocol::control::HelloResult;
use slingshot_local_protocol::envelope::{ControlRequest, ControlResponse};
use slingshot_local_protocol::foundation_contract::FoundationContract;
use slingshot_local_protocol::framing;
use slingshot_local_protocol::message::{OperationEnvelope, OperationRequest, OperationResponse};
use std::io::{BufRead, BufReader, Write};
use std::os::unix::fs::PermissionsExt;
use std::path::PathBuf;
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::{Arc, mpsc};
use tokio::io::AsyncReadExt;

/// Canonical target identity of the independent endpoint fixture.
const TARGET: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
/// Explicit namespace names avoid any user configuration lookup.
const PROFILE: &str = "local";
/// Environment addressed by both the endpoint and protocol process.
const ENVIRONMENT: &str = "author";
/// The runtime root is private to its owning account.
const OWNER_DIRECTORY_MODE: u32 = 0o700;

/// A framed daemon peer that exposes actual connection entry and detachment.
struct Peer {
    /// Owns only this fixture's files and endpoint.
    root: tempfile::TempDir,
    /// The exact bytes of the retained pending operation.
    retained: PathBuf,
    /// Wait reached the peer before the test sends cancellation.
    entered: mpsc::Receiver<()>,
    /// The product closed its wait connection after cancellation.
    detached: mpsc::Receiver<()>,
    /// Stops acceptance and aborts outstanding fixture connections on drop.
    stop: Option<tokio::sync::oneshot::Sender<()>>,
    /// Owns the endpoint thread through shutdown.
    thread: Option<std::thread::JoinHandle<()>>,
}

impl Peer {
    /// Creates a real local endpoint and one independent durable status fixture.
    fn start() -> Self {
        let root = tempfile::tempdir().unwrap();
        std::fs::set_permissions(
            root.path(),
            std::fs::Permissions::from_mode(OWNER_DIRECTORY_MODE),
        )
        .unwrap();
        let contract = FoundationContract::embedded();
        let namespace =
            RuntimeNamespace::name(&contract, root.path(), PROFILE, ENVIRONMENT).unwrap();
        let EndpointAddress::UnixDomainSocket(path) =
            endpoint_address(&contract, root.path(), namespace.digest()).unwrap();
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        let listener = std::os::unix::net::UnixListener::bind(path).unwrap();
        listener.set_nonblocking(true).unwrap();
        let retained = root.path().join("pending-operation.json");
        std::fs::write(
            &retained,
            serde_json::to_vec(&OperationResponse::Status {
                lifecycle_state: "running".to_owned(),
                operation_identifier: "pending".to_owned(),
                operation_revision: 1,
            })
            .unwrap(),
        )
        .unwrap();
        let hello = Arc::new(HelloResult {
            author_target_identity_digest: TARGET.to_owned(),
            daemon_runtime_contract_digest:
                slingshot_domain::daemon_runtime_contract::DaemonRuntimeContract::embedded_digest()
                    .as_text()
                    .to_owned(),
            product_version: env!("CARGO_PKG_VERSION").to_owned(),
            readiness_nonce: TARGET.to_owned(),
            runtime_namespace: namespace.key(),
            selected_environment_revision: TARGET.to_owned(),
            supported_operation_protocol_versions: vec![
                slingshot_command_line::daemon_request::spoken_operation_version(),
            ],
        });
        let (entered_sender, entered) = mpsc::channel();
        let (detached_sender, detached) = mpsc::channel();
        let (stop, stopped) = tokio::sync::oneshot::channel();
        let status = retained.clone();
        let thread = std::thread::spawn(move || {
            let runtime =
                tokio::runtime::Builder::new_current_thread().enable_all().build().unwrap();
            runtime.block_on(accept_connections(
                listener,
                hello,
                status,
                entered_sender,
                detached_sender,
                stopped,
            ));
        });
        Self { root, retained, entered, detached, stop: Some(stop), thread: Some(thread) }
    }
}

impl Drop for Peer {
    fn drop(&mut self) {
        if let Some(stop) = self.stop.take() {
            let _ignored = stop.send(());
        }
        if let Some(thread) = self.thread.take() {
            thread.join().unwrap();
        }
    }
}

/// Services independent hello, status and wait connections on one endpoint.
async fn accept_connections(
    listener: std::os::unix::net::UnixListener,
    hello: Arc<HelloResult>,
    retained: PathBuf,
    entered: mpsc::Sender<()>,
    detached: mpsc::Sender<()>,
    mut stopped: tokio::sync::oneshot::Receiver<()>,
) {
    let listener = tokio::net::UnixListener::from_std(listener).unwrap();
    loop {
        tokio::select! {
            _ = &mut stopped => return,
            accepted = listener.accept() => {
                let (stream, _) = accepted.unwrap();
                tokio::spawn(answer(stream, Arc::clone(&hello), retained.clone(), entered.clone(), detached.clone()));
            }
        }
    }
}

/// Wait blocks on the connection; status reads the independent retained file.
async fn answer(
    mut stream: tokio::net::UnixStream,
    hello: Arc<HelloResult>,
    retained: PathBuf,
    entered: mpsc::Sender<()>,
    detached: mpsc::Sender<()>,
) {
    let contract = FoundationContract::embedded();
    let bytes = local_server::read_frame(&mut stream, &contract, true).await.unwrap().unwrap();
    let response = if let Ok(control) = serde_json::from_slice::<ControlRequest>(&bytes) {
        assert_eq!(control.method, "daemon.hello");
        serde_json::to_vec(
            &ControlResponse::served(&contract, &control.request_identifier, hello.as_ref())
                .unwrap(),
        )
        .unwrap()
    } else {
        let envelope: OperationEnvelope = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(envelope.author_target_identity_digest, TARGET);
        match envelope.request {
            OperationRequest::Wait { operation_identifier, .. } => {
                assert_eq!(operation_identifier, "pending");
                entered.send(()).unwrap();
                let mut following = [0_u8];
                assert_eq!(
                    stream.read(&mut following).await.unwrap(),
                    0,
                    "cancellation closes the local wait"
                );
                detached.send(()).unwrap();
                return;
            }
            OperationRequest::OperationStatus { operation_identifier } => {
                assert_eq!(operation_identifier, "pending");
                std::fs::read(retained).unwrap()
            }
            OperationRequest::Execute {
                command, caller_identity, operation_identifier, ..
            } => {
                assert_eq!(command["command"], "list_components");
                std::fs::write(
                    retained.with_extension("producer.json"),
                    serde_json::to_vec(&caller_identity).unwrap(),
                )
                .unwrap();
                serde_json::to_vec(&OperationResponse::ResultInline {
                    operation_identifier,
                    result: json!({"components": []}),
                })
                .unwrap()
            }
            other => panic!("unexpected fixture request: {other:?}"),
        }
    };
    let frame = framing::render(&contract.framing, &response).unwrap();
    local_server::write_frame(&mut stream, &contract, &frame).await.unwrap();
}

/// Retains the actual product child and owns its interactive standard streams.
struct Product {
    /// A child handle is retained until exit or cleanup, never replaced by a PID.
    child: Child,
    /// Interactive input stays open through the pending wait.
    input: ChildStdin,
    /// Complete responses read independently of input.
    responses: mpsc::Receiver<Value>,
    /// Selected revision for this process.
    revision: &'static str,
}

impl Product {
    /// Starts the shipped executable against the fixture endpoint.
    fn start(peer: &Peer, revision: &'static str) -> Self {
        Self::with_producer(peer, revision, None)
    }

    /// Supplies environment only to the child, without mutating the test process.
    fn with_producer(peer: &Peer, revision: &'static str, label: Option<&str>) -> Self {
        let mut command = Command::new(env!("CARGO_BIN_EXE_slingshot"));
        command.env_remove("SLINGSHOT_PRODUCER");
        if let Some(label) = label {
            command.env("SLINGSHOT_PRODUCER", label);
        }
        let mut child = command
            .args([
                "--runtime-root",
                peer.root.path().to_str().unwrap(),
                "--profile",
                PROFILE,
                "--environment",
                ENVIRONMENT,
                "protocol",
                "serve",
            ])
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit())
            .spawn()
            .unwrap();
        let input = child.stdin.take().unwrap();
        let output = child.stdout.take().unwrap();
        let (sender, responses) = mpsc::channel();
        std::thread::spawn(move || {
            for line in BufReader::new(output).lines() {
                let Ok(line) = line else {
                    break;
                };
                if sender.send(serde_json::from_str(&line).unwrap()).is_err() {
                    break;
                }
            }
        });
        let mut product = Self { child, input, responses, revision };
        if revision == SUPPORTED_REVISIONS[1] {
            product.request("initialize", "initialize", json!({"protocolVersion":revision}));
            assert_eq!(product.response()["id"], "initialize");
            product.send(json!({"method":"notifications/initialized"}));
        }
        product
    }

    /// Writes a complete message to the actual executable.
    fn send(&mut self, message: Value) {
        writeln!(self.input, "{message}").unwrap();
    }

    /// Sends a request in the chosen protocol era.
    fn request(&mut self, identifier: &str, method: &str, mut parameters: Value) {
        if self.revision == SUPPORTED_REVISIONS[0] {
            parameters["protocolVersion"] = self.revision.into();
        }
        self.send(json!({"id":identifier, "method":method, "params":parameters}));
    }

    /// Receives under the declared transport response bound.
    fn response(&self) -> Value {
        self.responses.recv_timeout(write_deadline()).unwrap()
    }
}

impl Drop for Product {
    fn drop(&mut self) {
        let _ignored = self.child.kill();
        let _ignored = self.child.wait();
    }
}

#[test]
fn both_executable_revisions_detach_wait_connections_and_preserve_observable_work() {
    for revision in SUPPORTED_REVISIONS {
        let peer = Peer::start();
        let before = std::fs::read(&peer.retained).unwrap();
        let mut product = Product::start(&peer, revision);
        product.request(
            "waiting",
            "tools/call",
            json!({"name":"operation-wait", "arguments":{"operation_identifier":"pending"}}),
        );
        peer.entered
            .recv_timeout(write_deadline())
            .expect("the real runner attached a daemon wait");
        product.request("ping", "ping", json!({}));
        assert_eq!(product.response()["id"], "ping");
        product.request(
            "during",
            "tools/call",
            json!({"name":"operation-status", "arguments":{"operation_identifier":"pending"}}),
        );
        assert_eq!(product.response()["id"], "during");
        product.send(json!({"method":"notifications/cancelled", "params":{"requestId":"waiting"}}));
        peer.detached
            .recv_timeout(write_deadline())
            .expect("cancellation closed the actual daemon exchange");
        product.request(
            "after",
            "tools/call",
            json!({"name":"operation-status", "arguments":{"operation_identifier":"pending"}}),
        );
        let response = product.response();
        assert_eq!(response["id"], "after", "cancelled wait answers are suppressed");
        assert_ne!(response["result"]["isError"], true);
        let text = response["result"]["content"][0]["text"].as_str().unwrap();
        let status: Value = serde_json::from_str(text).unwrap();
        assert_eq!(status["state"], "running");
        assert_eq!(std::fs::read(&peer.retained).unwrap(), before);
    }
}

#[test]
fn product_environment_reaches_execute_as_only_a_digest_in_both_revisions() {
    use slingshot_domain::producer_identity::ProducerIdentity;
    for revision in SUPPORTED_REVISIONS {
        for label in [None, Some("private-build-label"), Some("")] {
            let peer = Peer::start();
            let mut product = Product::with_producer(&peer, revision, label);
            product.request(
                "execute",
                "tools/call",
                json!({"name":"list_components", "arguments":{"root_path":"/apps/acme"}}),
            );
            let response = product.response();
            assert_eq!(response["id"], "execute");
            let captured = peer.retained.with_extension("producer.json");
            if label == Some("") {
                assert_eq!(response["result"]["isError"], true, "{response}");
                assert!(!captured.exists());
            } else {
                assert_ne!(response["result"]["isError"], true, "{response}");
                let bytes = std::fs::read_to_string(captured).unwrap();
                assert!(!bytes.contains("private-build-label"));
                let received: Option<String> = serde_json::from_str(&bytes).unwrap();
                assert_eq!(
                    received,
                    label.map(|label| ProducerIdentity::from_label(label)
                        .unwrap()
                        .as_text()
                        .to_owned())
                );
            }
        }
    }
}
