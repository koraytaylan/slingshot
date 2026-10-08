//! Drives the production coordinator over real, independently owned stream endpoints.
//!
//! The controlled runner exposes admission and detachment, so cancellation is
//! observed inside a blocked local call rather than inferred from a silent response.

#![cfg(unix)]

use std::collections::BTreeSet;
use std::io::{BufRead, BufReader, Write};
use std::os::unix::net::UnixStream;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, mpsc};
use std::time::Instant;

use serde_json::{Value, json};
use slingshot_command_line::machine_outcome_envelope::MachineOutcomeEnvelope;
use slingshot_command_line::model_context_protocol::application::{
    RESOURCE_EXHAUSTED_ERROR, process_session,
};
use slingshot_command_line::model_context_protocol::current_stateless_revision::INVALID_REQUEST_ERROR;
use slingshot_command_line::model_context_protocol::operation_execution::ToolRunner;
use slingshot_command_line::model_context_protocol::standard_stream_transport::{
    SUPPORTED_REVISIONS, shutdown_deadline, write_deadline,
};
use slingshot_command_line::model_context_protocol::tool_catalog::ToolDescriptor;

/// Delivers bytes immediately while exposing the writer's later flush boundary.
struct HeldFlush {
    stream: UnixStream,
    entered: mpsc::Sender<()>,
    release: Option<mpsc::Receiver<()>>,
}

impl Write for HeldFlush {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        self.stream.write(bytes)
    }

    fn flush(&mut self) -> std::io::Result<()> {
        if let Some(release) = self.release.take() {
            self.entered.send(()).unwrap();
            let _ignored = release.recv_timeout(write_deadline());
        }
        self.stream.flush()
    }
}

#[test]
fn a_peer_can_reuse_an_identifier_after_reading_its_response() {
    let (entered_sender, entered) = mpsc::channel();
    let (release, released) = mpsc::channel();
    let mut session = Session::over(SUPPORTED_REVISIONS[0], |stream| HeldFlush {
        stream,
        entered: entered_sender,
        release: Some(released),
    });
    session.request("reused", "ping", json!({}));
    entered.recv_timeout(write_deadline()).unwrap();
    assert_eq!(session.response()["id"], "reused");
    session.request("reused", "ping", json!({}));
    release.send(()).unwrap();
    let answered = session.response();
    assert_eq!(answered["id"], "reused");
    assert!(answered.get("error").is_none(), "{answered}");
}

/// Observes cancellation without changing the retained operations it is watching.
struct WaitingRunner {
    /// Coordinator-owned per-request flag.
    cancellation: Arc<AtomicBool>,
    /// Admission rendezvous with the test.
    started: mpsc::Sender<String>,
    /// Confirms the actual local waiter observed cancellation.
    detached: mpsc::Sender<String>,
    /// Remote work is intentionally independent of the local wait's lifetime.
    operations: Arc<Mutex<BTreeSet<String>>>,
}

impl ToolRunner for WaitingRunner {
    fn run(
        &mut self,
        tool: &ToolDescriptor,
        arguments: &Value,
    ) -> Result<MachineOutcomeEnvelope, String> {
        if tool.name == "operation-wait" {
            let operation = arguments["operation_identifier"].as_str().unwrap().to_owned();
            self.operations.lock().unwrap().insert(operation.clone());
            self.started.send(operation.clone()).unwrap();
            let deadline = Instant::now() + write_deadline();
            while !self.cancellation.load(Ordering::SeqCst) {
                if Instant::now() >= deadline {
                    return Err("the fixture's local wait was never cancelled".to_owned());
                }
                std::thread::yield_now();
            }
            let _ignored = self.detached.send(operation);
            return Err("local wait detached".to_owned());
        }
        Ok(MachineOutcomeEnvelope::OperationListPage {
            operations: self.operations.lock().unwrap().iter().cloned().collect(),
            continuation_token: None,
        })
    }
}

/// A real input pipe, output pipe, and controlled operation boundary.
struct Session {
    /// The client owns only the peer of the input reader.
    input: UnixStream,
    /// Lets a test close the output peer while the response reader is waiting.
    output: UnixStream,
    /// Parsed complete output lines.
    responses: mpsc::Receiver<Value>,
    /// Work actually entered the runner.
    started: mpsc::Receiver<String>,
    /// Work actually left its local wait.
    detached: mpsc::Receiver<String>,
    /// The coordinator returned without joining a blocked input thread.
    finished: mpsc::Receiver<()>,
    /// Ensures a failed assertion cannot strand the coordinator.
    stop: Arc<AtomicBool>,
    /// Explicitly independent retained operation state.
    operations: Arc<Mutex<BTreeSet<String>>>,
    /// The era this session uses.
    revision: &'static str,
}

impl Session {
    /// Starts the production coordinator with owned streams and a fresh runner per call.
    fn start(revision: &'static str) -> Self {
        Self::over(revision, |output| output)
    }

    /// A writer adapter can expose the interval between bytes and acknowledgement.
    fn over<Output: Write + Send + 'static>(
        revision: &'static str,
        wrap: impl FnOnce(UnixStream) -> Output,
    ) -> Self {
        let (input, server_input) = UnixStream::pair().unwrap();
        let (output, server_output) = UnixStream::pair().unwrap();
        let server_output = wrap(server_output);
        let (responses_sender, responses) = mpsc::channel();
        let (started_sender, started) = mpsc::channel();
        let (detached_sender, detached) = mpsc::channel();
        let (finished_sender, finished) = mpsc::channel();
        let stop = Arc::new(AtomicBool::new(false));
        let stopping = Arc::clone(&stop);
        let operations = Arc::new(Mutex::new(BTreeSet::new()));
        let retained = Arc::clone(&operations);
        std::thread::spawn(move || {
            process_session::serve(
                BufReader::new(server_input),
                server_output,
                std::io::sink(),
                |cancellation| {
                    Box::new(WaitingRunner {
                        cancellation,
                        started: started_sender.clone(),
                        detached: detached_sender.clone(),
                        operations: Arc::clone(&retained),
                    })
                },
                stopping,
            )
            .unwrap();
            let _ignored = finished_sender.send(());
        });
        let reader = output.try_clone().unwrap();
        std::thread::spawn(move || {
            for line in BufReader::new(reader).lines() {
                let Ok(line) = line else {
                    break;
                };
                if responses_sender.send(serde_json::from_str(&line).unwrap()).is_err() {
                    break;
                }
            }
        });
        let mut session = Self {
            input,
            output,
            responses,
            started,
            detached,
            finished,
            stop,
            operations,
            revision,
        };
        if revision == SUPPORTED_REVISIONS[1] {
            session.send(json!({"id":"initialize", "method":"initialize", "params":{"protocolVersion":revision}}));
            assert_eq!(session.response()["id"], "initialize");
            session.send(json!({"method":"notifications/initialized"}));
        }
        session
    }

    /// Writes a complete protocol frame.
    fn send(&mut self, value: Value) {
        writeln!(self.input, "{value}").unwrap();
    }

    /// Requests one control or tool using the selected era.
    fn request(&mut self, identifier: &str, method: &str, mut parameters: Value) {
        if self.revision == SUPPORTED_REVISIONS[0] {
            parameters["protocolVersion"] = self.revision.into();
        }
        self.send(json!({"id":identifier, "method":method, "params":parameters}));
    }

    /// Starts a local waiter and proves it reached execution before continuing.
    fn wait_for(&mut self, identifier: &str) {
        self.request(
            identifier,
            "tools/call",
            json!({"name":"operation-wait", "arguments":{"operation_identifier":identifier}}),
        );
        assert_eq!(self.started.recv_timeout(write_deadline()).unwrap(), identifier);
    }

    /// Reads an answer under the transport's declared response deadline.
    fn response(&self) -> Value {
        self.responses.recv_timeout(write_deadline()).expect("the coordinator remained responsive")
    }
}

impl Drop for Session {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::SeqCst);
        let _ignored = self.input.shutdown(std::net::Shutdown::Both);
        let _ignored = self.output.shutdown(std::net::Shutdown::Both);
    }
}

#[test]
fn both_revisions_answer_controls_and_independent_calls_while_a_local_wait_is_pending() {
    for revision in SUPPORTED_REVISIONS {
        let mut session = Session::start(revision);
        session.wait_for("waiting");
        session.request("ping", "ping", json!({}));
        assert_eq!(session.response()["id"], "ping");
        session.request("list", "tools/call", json!({"name":"operation-list", "arguments":{}}));
        assert_eq!(session.response()["id"], "list");
        session.send(json!({"method":"notifications/cancelled", "params":{"requestId":"waiting"}}));
        assert_eq!(session.detached.recv_timeout(write_deadline()).unwrap(), "waiting");
        session.request("after", "ping", json!({}));
        assert_eq!(session.response()["id"], "after", "cancelled answers must be suppressed");
        assert!(
            session.operations.lock().unwrap().contains("waiting"),
            "local detachment preserves retained work"
        );
    }
}

#[test]
fn duplicates_and_worker_saturation_leave_original_waiters_reserved() {
    let mut session = Session::start(SUPPORTED_REVISIONS[0]);
    session.wait_for("original");
    session.request("original", "ping", json!({}));
    assert_eq!(session.response()["error"]["code"], INVALID_REQUEST_ERROR);
    for index in 1..process_session::maximum_workers() {
        session.wait_for(&format!("waiting-{index}"));
    }
    session.request(
        "overflow",
        "tools/call",
        json!({"name":"operation-wait", "arguments":{"operation_identifier":"overflow"}}),
    );
    assert_eq!(session.response()["error"]["code"], RESOURCE_EXHAUSTED_ERROR);
    session.request("responsive", "ping", json!({}));
    assert_eq!(session.response()["id"], "responsive");
    assert!(!session.operations.lock().unwrap().contains("overflow"));
    assert!(session.detached.try_recv().is_err(), "refusals do not detach admitted requests");
}

#[test]
fn output_failure_detaches_active_work_without_waiting_for_input_to_end() {
    let mut session = Session::start(SUPPORTED_REVISIONS[0]);
    session.wait_for("waiting");
    session.output.shutdown(std::net::Shutdown::Both).unwrap();
    session.request("broken", "ping", json!({}));
    session.finished.recv_timeout(shutdown_deadline()).expect("output failure stops intake");
    assert_eq!(session.detached.recv_timeout(shutdown_deadline()).unwrap(), "waiting");
}

#[test]
fn input_ending_bounds_the_drain_of_an_active_waiter() {
    let mut session = Session::start(SUPPORTED_REVISIONS[0]);
    session.wait_for("waiting");
    session.input.shutdown(std::net::Shutdown::Write).unwrap();
    session.finished.recv_timeout(write_deadline()).expect("EOF bounds the active drain");
    assert_eq!(session.detached.recv_timeout(shutdown_deadline()).unwrap(), "waiting");
}
