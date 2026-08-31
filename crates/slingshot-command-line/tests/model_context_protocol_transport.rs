//! What the transport accepts, what it writes, and how it stops.
//!
//! Every claim here is about a boundary: the longest line, the deepest
//! nesting, the most requests in flight, the fullest queue, and the one
//! transition that happens when standard output stops working. A boundary
//! asserted only in prose is a boundary that moves.
//!
//! The revision authority is compared against a fixture written independently
//! of the source, because the exact list and its exact order are what two era
//! handlers agree on, and a list either of them could restate is a list they
//! can disagree about.

use std::time::Duration;

use slingshot_command_line::model_context_protocol::active_request_registry::{
    ActiveRequestRegistry, AdmissionRefusal, MAXIMUM_ACTIVE_REQUESTS, Standing,
};
use slingshot_command_line::model_context_protocol::protocol_diagnostics::{
    MAXIMUM_HELD_RECORDS, ProtocolDiagnosticSink, Recorded,
};
use slingshot_command_line::model_context_protocol::standard_stream_transport::{
    LineSink, MAXIMUM_QUEUED_BYTES, MAXIMUM_QUEUED_MESSAGES, Message, MessageRefusal,
    OutputFailure, OutputQueue, ProtocolRevision, QUEUE_PRESSURE_DEADLINE, QueueRefusal,
    SUPPORTED_REVISIONS, WRITE_DEADLINE, Written, maximum_line_bytes, maximum_nesting_depth,
    read_message,
};

/// Where the transport fixtures live.
const FIXTURES: &str = "../slingshot-test-support/fixtures/model-context-protocol/transport";

/// A duration no deadline treats as elapsed.
const NO_WAIT: Duration = Duration::from_millis(0);

/// Returns one fixture's text.
fn fixture(name: &str) -> String {
    let path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(FIXTURES).join(name);
    std::fs::read_to_string(&path)
        .unwrap_or_else(|failure| panic!("{} could not be read: {failure}", path.display()))
}

/// One declared line case.
#[derive(Debug, serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct Case {
    /// What the case is called.
    name: String,
    /// What the transport is given.
    line: String,
    /// What it makes of it.
    outcome: String,
    /// Which identifier or which refusal.
    detail: String,
}

/// Returns every declared line case.
fn cases() -> Vec<Case> {
    fixture("lines.jsonl")
        .lines()
        .filter(|line| !line.trim().is_empty() && !line.starts_with('#'))
        .map(|line| serde_json::from_str(line).expect("every case reads"))
        .collect()
}

/// A sink that takes everything.
#[derive(Debug, Default)]
struct WorkingSink {
    /// Every complete line it took, in order.
    taken: Vec<String>,
}

impl LineSink for WorkingSink {
    fn write_line(&mut self, line: &str) -> Written {
        self.taken.push(line.to_owned());
        Written::Complete
    }
}

/// A sink that takes some lines and then fails partway through one.
#[derive(Debug)]
struct FailingSink {
    /// How many lines it takes before it fails.
    takes: usize,
    /// Every complete line it took, in order.
    taken: Vec<String>,
    /// How many bytes of the failing line it accepted.
    prefix: usize,
}

impl LineSink for FailingSink {
    fn write_line(&mut self, line: &str) -> Written {
        if self.taken.len() < self.takes {
            self.taken.push(line.to_owned());
            return Written::Complete;
        }
        Written::Prefix(self.prefix)
    }
}

#[test]
fn the_revision_authority_is_exactly_the_two_the_fixture_names_in_that_order() {
    let named: Vec<String> =
        serde_json::from_str(&fixture("supported-revisions.json")).expect("the fixture reads");
    assert_eq!(SUPPORTED_REVISIONS.to_vec(), named);
    assert_eq!(ProtocolRevision::preferred().as_text(), named[0]);
    assert_eq!(ProtocolRevision::Legacy.as_text(), named[1]);
    assert_eq!(ProtocolRevision::named(&named[0]), Some(ProtocolRevision::Current));
    assert_eq!(ProtocolRevision::named("2020-01-01"), None);
}

#[test]
fn this_builds_preference_decides_the_revision_and_not_the_peers_order() {
    let both = vec![SUPPORTED_REVISIONS[1].to_owned(), SUPPORTED_REVISIONS[0].to_owned()];
    assert_eq!(ProtocolRevision::negotiated(&both), Some(ProtocolRevision::Current));
    let legacy_alone = vec![SUPPORTED_REVISIONS[1].to_owned()];
    assert_eq!(ProtocolRevision::negotiated(&legacy_alone), Some(ProtocolRevision::Legacy));
    let neither = vec!["2020-01-01".to_owned()];
    assert_eq!(ProtocolRevision::negotiated(&neither), None);
}

#[test]
fn every_declared_line_is_read_the_way_the_fixture_says() {
    for case in cases() {
        let read = read_message(case.line.as_bytes());
        match case.outcome.as_str() {
            "request" => {
                let Ok(Message::Request { identifier, .. }) = read else {
                    panic!("{} is a request: {read:?}", case.name)
                };
                assert_eq!(identifier, case.detail, "{}", case.name);
            }
            "notification" => {
                assert!(matches!(read, Ok(Message::Notification { .. })), "{}", case.name);
            }
            _ => {
                let refusal = read.expect_err(&format!("{} is refused", case.name));
                assert!(
                    format!("{refusal:?}").starts_with(&case.detail),
                    "{} was refused as {refusal:?}",
                    case.name
                );
            }
        }
    }
}

#[test]
fn a_line_past_a_bound_is_refused_before_it_is_parsed() {
    let padding = "a".repeat(maximum_line_bytes());
    let long = format!("{{\"id\":\"{padding}\",\"method\":\"tools/list\"}}");
    assert_eq!(read_message(long.as_bytes()), Err(MessageRefusal::LineTooLong(long.len())));

    let mut deep = String::new();
    for _ in 0..=maximum_nesting_depth() {
        deep.push('[');
    }
    deep.push('1');
    for _ in 0..=maximum_nesting_depth() {
        deep.push(']');
    }
    let nested = format!("{{\"id\":\"one\",\"method\":\"tools/list\",\"params\":{deep}}}");
    assert_eq!(read_message(nested.as_bytes()), Err(MessageRefusal::TooDeep));

    assert_eq!(read_message(&[0xff, 0xfe]), Err(MessageRefusal::EncodingInvalid));
}

#[test]
fn a_duplicate_identifier_is_refused_and_the_original_is_left_alone() {
    let mut registry = ActiveRequestRegistry::new();
    registry.reserve("one").expect("the first is admitted");
    assert_eq!(
        registry.reserve("one"),
        Err(AdmissionRefusal::Duplicate("one".to_owned())),
        "the second names a request already in flight"
    );
    assert_eq!(registry.standing("one"), Some(Standing::Reserved), "the original is untouched");
    assert_eq!(registry.active(), 1);
    assert_eq!(registry.released(), 0);
}

#[test]
fn requests_saturate_exactly_at_the_bound_and_not_before_it() {
    let mut registry = ActiveRequestRegistry::new();
    for index in 0..MAXIMUM_ACTIVE_REQUESTS {
        registry.reserve(&format!("request-{index}")).expect("every request up to the bound");
    }
    assert_eq!(registry.active(), MAXIMUM_ACTIVE_REQUESTS);
    assert_eq!(registry.reserve("one-too-many"), Err(AdmissionRefusal::Saturated));
    assert_eq!(registry.active(), MAXIMUM_ACTIVE_REQUESTS, "a refusal admits nothing");
}

#[test]
fn an_identifier_is_reusable_only_once_its_answer_has_gone_out() {
    let mut registry = ActiveRequestRegistry::new();
    registry.reserve("one").expect("it is admitted");
    registry.answered("one");
    assert_eq!(registry.standing("one"), Some(Standing::Answered));
    assert_eq!(
        registry.reserve("one"),
        Err(AdmissionRefusal::Duplicate("one".to_owned())),
        "a queued answer is not a delivered one"
    );
    assert!(registry.acknowledged("one"), "the writer says the line went out in full");
    assert_eq!(registry.standing("one"), None);
    registry.reserve("one").expect("the identifier is reusable now");
    assert_eq!(registry.released(), 1);
}

#[test]
fn a_cancelled_request_is_released_only_after_it_is_suppressed_and_detached() {
    let mut registry = ActiveRequestRegistry::new();
    registry.reserve("one").expect("it is admitted");
    assert!(registry.cancelling("one"));
    assert_eq!(registry.standing("one"), Some(Standing::Cancelling));
    assert!(!registry.acknowledged("one"), "a cancelled request has no answer to acknowledge");
    assert!(registry.cancelled("one"), "suppression and detachment are done");
    assert_eq!(registry.standing("one"), None);
    assert!(!registry.cancelled("one"), "one release, not two");
    assert_eq!(registry.released(), 1);
}

#[test]
fn end_of_input_releases_everything_once_and_names_what_to_detach() {
    let mut registry = ActiveRequestRegistry::new();
    for identifier in ["one", "two", "three"] {
        registry.reserve(identifier).expect("it is admitted");
    }
    registry.answered("two");
    let detaching = registry.release_all();
    assert_eq!(detaching, vec!["one".to_owned(), "three".to_owned(), "two".to_owned()]);
    assert_eq!(registry.active(), 0);
    assert_eq!(registry.released(), detaching.len());
    assert!(registry.release_all().is_empty(), "a second release finds nothing");
}

#[test]
fn the_queue_holds_what_it_says_and_refuses_the_rest() {
    let mut queue = OutputQueue::new();
    let line = "{\"id\":\"one\"}";
    for _ in 0..MAXIMUM_QUEUED_MESSAGES {
        queue.enqueue(line).expect("every line up to the bound");
    }
    assert_eq!(queue.waiting(), MAXIMUM_QUEUED_MESSAGES);
    assert_eq!(queue.enqueue(line), Err(QueueRefusal::Full));
    assert!(queue.waiting_bytes() <= MAXIMUM_QUEUED_BYTES);

    let mut sink = WorkingSink::default();
    let written = queue.write_waiting(&mut sink, NO_WAIT);
    assert_eq!(written, MAXIMUM_QUEUED_MESSAGES);
    assert_eq!(queue.waiting(), 0);
    assert_eq!(queue.waiting_bytes(), 0);
    assert_eq!(sink.taken.len(), MAXIMUM_QUEUED_MESSAGES);
}

#[test]
fn a_line_longer_than_the_transport_writes_is_refused_rather_than_split() {
    let mut queue = OutputQueue::new();
    let long = "a".repeat(maximum_line_bytes() + 1);
    assert_eq!(queue.enqueue(&long), Err(QueueRefusal::TooLong(long.len())));
    assert_eq!(queue.waiting(), 0);
}

#[test]
fn one_failure_wins_and_nothing_is_written_after_it() {
    let mut queue = OutputQueue::new();
    for index in 0..EXPECTED_LINES {
        queue.enqueue(&format!("{{\"id\":\"{index}\"}}")).expect("it is queued");
    }
    let mut sink = FailingSink { takes: TAKEN_LINES, taken: Vec::new(), prefix: ACCEPTED_PREFIX };
    let written = queue.write_waiting(&mut sink, NO_WAIT);
    assert_eq!(written, TAKEN_LINES, "every line before the failure is complete");
    assert_eq!(queue.failure(), Some(OutputFailure::SinkFailed));
    assert_eq!(queue.waiting(), 0, "what had not started being written is discarded");
    assert!(!queue.accepts_more());
    assert_eq!(queue.enqueue("{}"), Err(QueueRefusal::Stopped));

    queue.fail(OutputFailure::WriteExpired);
    assert_eq!(queue.failure(), Some(OutputFailure::SinkFailed), "the first reason wins");
}

/// How many lines the partial-sink case queues.
const EXPECTED_LINES: usize = 5;

/// How many of them the sink takes before it fails.
const TAKEN_LINES: usize = 2;

/// How many bytes of the failing line the sink accepted.
const ACCEPTED_PREFIX: usize = 3;

#[test]
fn a_wait_past_the_pressure_deadline_stops_output_rather_than_one_message() {
    let mut queue = OutputQueue::new();
    queue.waited_for_room(QUEUE_PRESSURE_DEADLINE - Duration::from_millis(1));
    assert_eq!(queue.failure(), None, "a wait inside the deadline is only a wait");
    queue.waited_for_room(QUEUE_PRESSURE_DEADLINE);
    assert_eq!(queue.failure(), Some(OutputFailure::PressureExpired));
}

#[test]
fn a_line_past_the_write_deadline_stops_output_before_it_is_attempted() {
    let mut queue = OutputQueue::new();
    queue.enqueue("{\"id\":\"one\"}").expect("it is queued");
    let mut sink = WorkingSink::default();
    let written = queue.write_waiting(&mut sink, WRITE_DEADLINE);
    assert_eq!(written, 0);
    assert!(sink.taken.is_empty(), "nothing is written after the deadline elapses");
    assert_eq!(queue.failure(), Some(OutputFailure::WriteExpired));
}

#[test]
fn a_diagnostic_is_dropped_and_counted_rather_than_waited_for() {
    let mut sink = ProtocolDiagnosticSink::closed();
    assert_eq!(sink.record("a stream nobody reads"), Recorded::Dropped);
    assert_eq!(sink.dropped(), 1);
    assert!(sink.held().is_empty());

    let mut working = ProtocolDiagnosticSink::new();
    for index in 0..MAXIMUM_HELD_RECORDS {
        assert_eq!(working.record(&format!("record {index}")), Recorded::Kept);
    }
    assert_eq!(working.record("one too many"), Recorded::Dropped);
    assert_eq!(working.dropped(), 1);
    assert_eq!(working.held().len(), MAXIMUM_HELD_RECORDS);
}
