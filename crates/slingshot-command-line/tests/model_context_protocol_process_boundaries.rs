//! What the server process does when the world around it misbehaves.
//!
//! Every case here is about a boundary the server does not control: input
//! ending, the reader of its output going away, the reader of its diagnostics
//! going away, and somebody ending the process itself. What must hold through
//! all of them is the same short list - it stops, it stops once, it stops
//! inside a bounded time, and it leaves nothing behind.
//!
//! A process that hung while shutting down would hold the terminal it was asked
//! to give back, which is worse than the failure that started the shutdown.

use std::path::PathBuf;
use std::time::Duration;

use serde_json::Value;
use slingshot_test_support::process_harness::{
    DeliverableSignal, ExecutablePath, ProcessHarness, ProcessRequest,
};

/// Where the sentinels live.
const FIXTURES: &str =
    "../slingshot-test-support/fixtures/model-context-protocol/process-boundaries";

/// The revision these cases speak.
const REVISION: &str = "2026-07-28";

/// How long a case waits for a server that should finish at once.
const PROMPT_DEADLINE: Duration = Duration::from_secs(30);

/// How long a case waits for a server whose output has been taken away.
const SHUTDOWN_DEADLINE: Duration = Duration::from_secs(30);

/// How long a case waits to be sure a server is waiting rather than finished.
const SETTLING_DEADLINE: Duration = Duration::from_millis(400);

/// How many requests a flooding case sends.
const FLOODED_REQUESTS: usize = 512;

/// Returns the product executable these cases drive.
fn product_executable() -> ExecutablePath {
    ExecutablePath::new(PathBuf::from(env!("CARGO_BIN_EXE_slingshot")))
        .expect("the product executable was built")
}

/// Returns the words that hand the streams to the server.
fn serving() -> ProcessRequest {
    ProcessRequest::new(&["--profile", "local", "--environment", "author", "protocol", "serve"])
}

/// Returns one request line.
fn request(identifier: usize) -> String {
    format!(
        r#"{{"id":"{identifier}","method":"ping","params":{{"protocolVersion":"{REVISION}"}}}}"#
    )
}

/// One declared sentinel.
#[derive(Debug, serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct Sentinel {
    /// What class of value it stands for.
    name: String,
    /// The distinct value searched for.
    value: String,
    /// What it stands for.
    why: String,
}

/// Returns every declared sentinel.
fn sentinels() -> Vec<Sentinel> {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(FIXTURES).join("sentinels.jsonl");
    let text = std::fs::read_to_string(&path)
        .unwrap_or_else(|failure| panic!("{} could not be read: {failure}", path.display()));
    text.lines()
        .filter(|line| !line.trim().is_empty() && !line.starts_with('#'))
        .map(|line| serde_json::from_str(line).expect("every sentinel reads"))
        .collect()
}

#[test]
fn input_ending_finishes_the_server_cleanly_and_at_once() {
    let harness = ProcessHarness::new();
    let produced = harness
        .run_within(&product_executable(), &serving(), PROMPT_DEADLINE)
        .expect("the server finishes when its input ends");
    assert!(produced.status.success());
    assert!(produced.standard_output.is_empty(), "nothing was asked, so nothing is answered");
}

#[test]
fn every_answer_is_one_whole_line_and_the_capture_ends_on_one() {
    let input = (0..FLOODED_REQUESTS).map(request).collect::<Vec<String>>().join("\n") + "\n";
    let harness = ProcessHarness::new();
    let produced = harness
        .run_within(&product_executable(), &serving().reading(input), PROMPT_DEADLINE)
        .expect("the server answers everything it was asked");
    assert!(produced.standard_output.ends_with('\n'), "a complete capture ends on a whole line");
    let answered = produced.standard_output.lines().count();
    assert_eq!(answered, FLOODED_REQUESTS, "one answer each, and no more");
    for line in produced.standard_output.lines() {
        let parsed: Value = serde_json::from_str(line).expect("every line is one message");
        assert!(parsed["id"].is_string(), "every answer names what it answers");
    }
}

#[test]
fn a_reader_that_goes_away_stops_the_server_inside_its_deadline() {
    let input = (0..FLOODED_REQUESTS).map(request).collect::<Vec<String>>().join("\n") + "\n";
    let harness = ProcessHarness::new();
    let mut child = harness
        .start_retained(&product_executable(), &serving().reading(input))
        .expect("the server starts");
    drop(child.take_output());
    let ended = child.wait_within(SHUTDOWN_DEADLINE).expect("the server stopped when nobody read");
    assert!(ended.success() || ended.code().is_some(), "it stopped rather than being killed");
    assert!(child.is_reaped());
}

#[test]
fn a_diagnostic_reader_that_goes_away_delays_no_answer() {
    let input = (0..FLOODED_REQUESTS).map(request).collect::<Vec<String>>().join("\n") + "\n";
    let harness = ProcessHarness::new();
    let mut child = harness
        .start_retained(&product_executable(), &serving().reading(input))
        .expect("the server starts");
    drop(child.take_diagnostics());
    let produced = child
        .capture_within(SHUTDOWN_DEADLINE)
        .expect("the server finished without its diagnostics");
    assert!(produced.status.success());
    assert_eq!(
        produced.standard_output.lines().count(),
        FLOODED_REQUESTS,
        "every answer arrived while nobody was reading the other stream"
    );
}

#[test]
fn a_server_waiting_on_its_input_is_ended_through_its_retained_handle() {
    let harness = ProcessHarness::new();
    let mut child = harness
        .start_retained(&product_executable(), &serving().on_terminal())
        .expect("the server starts");
    assert!(
        child.wait_within(SETTLING_DEADLINE).is_err(),
        "a server whose input stays open waits for it"
    );
    let identifier = child.identifier();
    child.deliver(DeliverableSignal::Terminate).expect("the signal is delivered");
    child.wait_within(PROMPT_DEADLINE).expect("the server ended");
    assert!(child.is_reaped());
    assert!(child.deliver(DeliverableSignal::Kill).is_err(), "a reaped handle reaches nothing");
    assert_ne!(identifier, 0, "the identifier is recorded as a diagnostic and used as nothing");
}

#[test]
fn no_sentinel_reaches_either_stream_however_it_arrives() {
    let searched = sentinels();
    assert!(!searched.is_empty());
    let mut input = String::new();
    for (index, sentinel) in searched.iter().enumerate() {
        input.push_str(&format!(
            r#"{{"id":"{index}","method":"tools/call","params":{{"protocolVersion":"{REVISION}","name":"{}","arguments":{{"operation_key":"{}"}}}}}}"#,
            sentinel.value, sentinel.value
        ));
        input.push('\n');
    }
    let harness = ProcessHarness::new();
    let produced = harness
        .run_within(&product_executable(), &serving().reading(input), PROMPT_DEADLINE)
        .expect("the server answers");
    for sentinel in &searched {
        assert!(
            !produced.standard_output.contains(&sentinel.value),
            "{} ({}) reached standard output",
            sentinel.name,
            sentinel.why
        );
        assert!(
            !produced.standard_error.contains(&sentinel.value),
            "{} reached standard error",
            sentinel.name
        );
    }
}
