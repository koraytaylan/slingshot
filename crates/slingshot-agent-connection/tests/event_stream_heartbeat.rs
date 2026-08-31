//! Telling a quiet stream from a dead one, and nothing else.
//!
//! The boundary is the whole subject. A gap of exactly the timeout is a stream
//! that spoke exactly on time, and one millisecond past it is a stream that did
//! not, so both sides of that single instant are pinned by fixtures rather than
//! left to whichever comparison the code happened to use.
//!
//! The second subject is what a timeout is allowed to mean. It asks for another
//! connection and it does nothing else: no fixture here produces a job state, a
//! failure, or any claim about remote work, because a daemon that concluded a
//! job had failed because its own connection went quiet would be recording its
//! network as a fact about somebody else's system.

use slingshot_agent_connection::author_hypertext_transfer_protocol_policy::ExchangeDeadlines;
use slingshot_agent_connection::author_hypertext_transfer_protocol_policy::ResponseHead;
use slingshot_agent_connection::event_stream_heartbeat::{
    ConnectionState, EventStreamHeartbeat, HeartbeatFault, heartbeat_interval_milliseconds,
    heartbeat_timeout_milliseconds,
};
use slingshot_agent_connection::server_sent_event_decoder::{
    DecoderBounds, EVENT_STREAM_MEDIA_TYPE, ServerSentEventDecoder, StreamExpectation, StreamItem,
};
use slingshot_agent_protocol::wire_contract::ExpectedProvenance;
use slingshot_domain::author_agent_transport_contract::AuthorAgentTransportContract;
use slingshot_domain::command::schema::canonical_contract_digest;
use slingshot_domain::selected_command_contract_identity::SelectedCommandContractIdentity;

/// Where the vectors this suite is driven from live.
const FIXTURES: &str = "tests/fixtures/event-stream-heartbeat.jsonl";

/// The subscription every fixture stream was asked for under.
const SUBSCRIPTION: &str = "daemon-subscription-one";

/// The generation every fixture stream was asked for under.
const GENERATION: u64 = 7;

/// The command these streams carry events about.
const COMMAND: &str = "query_paths";

/// The submission these streams are about.
const SUBMITTED_DIGEST: &str = "1111111111111111111111111111111111111111111111111111111111111111";

/// When a stream attaches, on the injected clock.
const ATTACHED_AT: u64 = 4_000_000;

/// How long after attaching the activity fixtures deliver their bytes.
const ACTIVITY_AT: u64 = 30_000;

/// The protocol version the author is spoken to over.
const SPOKEN_VERSION: &str = "HTTP/1.1";

/// Returns every vector the fixture holds.
fn vectors() -> Vec<serde_json::Value> {
    let text = std::fs::read_to_string(FIXTURES).expect("the vectors are readable");
    text.lines().map(|line| serde_json::from_str(line).expect("each line is one vector")).collect()
}

/// Returns every vector of `kind`.
fn vectors_of(kind: &str) -> Vec<serde_json::Value> {
    vectors().into_iter().filter(|vector| vector["kind"].as_str() == Some(kind)).collect()
}

/// Returns a decoder attached to an acceptable event-stream response.
fn decoder() -> ServerSentEventDecoder {
    let head = ResponseHead {
        alternative_service_offered: false,
        content_coding: None,
        informational: false,
        location: None,
        protocol_version: SPOKEN_VERSION.to_owned(),
        trailers_declared: false,
    };
    ServerSentEventDecoder::attached(
        &head,
        EVENT_STREAM_MEDIA_TYPE,
        DecoderBounds::embedded(),
        StreamExpectation {
            agent_event_store_generation: GENERATION,
            daemon_subscription_identifier: SUBSCRIPTION.to_owned(),
            expected_provenance: ExpectedProvenance {
                canonical_json_contract_digest: canonical_contract_digest(),
                command_contract: SelectedCommandContractIdentity::installed(COMMAND)
                    .expect("the command is published"),
                transport_contract_digest: AuthorAgentTransportContract::embedded_digest(),
            },
            submitted_command_digest: SUBMITTED_DIGEST.to_owned(),
        },
    )
    .expect("this response is one event stream")
}

/// Returns the state `spelling` names.
fn state_named(spelling: &str) -> ConnectionState {
    match spelling {
        "healthy" => ConnectionState::Healthy,
        "timed-out" => ConnectionState::TimedOut,
        other => panic!("{other} is a state this suite does not name"),
    }
}

#[test]
fn the_boundary_belongs_to_health_and_one_millisecond_past_it_does_not() {
    for vector in vectors_of("silence") {
        let name = vector["name"].as_str().expect("a name");
        let quiet_for = vector["quiet_for"].as_u64().expect("a gap");
        let heartbeat = EventStreamHeartbeat::attached_at(ATTACHED_AT);
        assert_eq!(
            heartbeat.state_at(ATTACHED_AT + quiet_for).expect("time moved forward"),
            state_named(vector["state"].as_str().expect("a state")),
            "{name}: a stream that spoke exactly on time is punctual, not dead"
        );
    }
}

#[test]
fn the_timeout_is_the_one_the_transport_contract_states() {
    let heartbeat = EventStreamHeartbeat::attached_at(ATTACHED_AT);
    assert_eq!(heartbeat.timeout_milliseconds(), heartbeat_timeout_milliseconds());
    assert!(
        heartbeat_timeout_milliseconds() > heartbeat_interval_milliseconds(),
        "a timeout no longer than the interval would drop a connection for one late comment"
    );
    assert_eq!(heartbeat.last_activity_milliseconds(), ATTACHED_AT);
}

#[test]
fn a_comment_and_an_event_refresh_liveness_identically() {
    for vector in vectors_of("activity") {
        let name = vector["name"].as_str().expect("a name");
        let bytes = vector["stream"].as_str().expect("a stream").as_bytes();
        let refreshes = vector["refreshes"].as_bool().expect("an expectation");
        let mut heartbeat = EventStreamHeartbeat::attached_at(ATTACHED_AT);
        let mut decoder = decoder();
        let items = decoder.push(bytes).expect("these bytes are well formed so far");
        let observed_at = ATTACHED_AT + ACTIVITY_AT;
        for item in &items {
            assert_eq!(
                heartbeat.observe(item, observed_at).expect("time moved forward"),
                ConnectionState::Healthy,
                "{name}: anything complete means the connection is alive"
            );
        }
        assert_eq!(
            heartbeat.last_activity_milliseconds() == observed_at,
            refreshes,
            "{name}: partial bytes are not activity until the decoder completes something"
        );
        assert_eq!(items.is_empty(), !refreshes, "{name}: one complete unit, one refresh");
    }
}

#[test]
fn partial_bytes_leave_a_connection_quiet_until_they_complete() {
    let mut heartbeat = EventStreamHeartbeat::attached_at(ATTACHED_AT);
    let mut decoder = decoder();
    let quiet = heartbeat.timeout_milliseconds();
    assert!(decoder.push(b":a comment with no line feed").expect("well formed so far").is_empty());
    assert_eq!(
        heartbeat.state_at(ATTACHED_AT + quiet).expect("time moved forward"),
        ConnectionState::Healthy
    );
    assert_eq!(
        heartbeat.state_at(ATTACHED_AT + quiet + 1).expect("time moved forward"),
        ConnectionState::TimedOut,
        "bytes that never completed a comment never said the connection was alive"
    );
    let completed = decoder.push(b"\n").expect("the line feed completes it");
    assert_eq!(completed, vec![StreamItem::Heartbeat]);
    heartbeat.observe(&completed[0], ATTACHED_AT + quiet + 1).expect("time moved forward");
    assert_eq!(
        heartbeat.state_at(ATTACHED_AT + quiet + 1).expect("time moved forward"),
        ConnectionState::Healthy,
        "a completed comment is activity whenever it completes"
    );
}

#[test]
fn a_timeout_asks_for_another_connection_and_asks_for_nothing_else() {
    assert!(!ConnectionState::Healthy.requires_reconnection());
    assert!(ConnectionState::TimedOut.requires_reconnection());
    for vector in vectors() {
        let name = vector["name"].as_str().expect("a name");
        let rendered = vector.to_string();
        for forbidden in ["remote_job", "failed", "Failed", "RemoteJobState"] {
            assert!(
                !rendered.contains(forbidden),
                "{name}: no liveness fixture may carry a claim about remote work"
            );
        }
    }
}

#[test]
fn attaching_is_bounded_and_the_body_that_follows_is_not() {
    let deadlines = EventStreamHeartbeat::attachment_deadlines();
    let contract = AuthorAgentTransportContract::embedded();
    assert_eq!(
        deadlines.connect_milliseconds,
        contract.limit("author_connect_timeout_milliseconds")
    );
    assert_eq!(
        deadlines.response_header_milliseconds,
        contract.limit("author_response_header_timeout_milliseconds")
    );
    assert_eq!(deadlines, ExchangeDeadlines::embedded());
    assert_ne!(
        heartbeat_timeout_milliseconds(),
        contract.limit("finite_response_total_timeout_milliseconds"),
        "liveness is not the finite-body deadline, because a live stream stays open"
    );
    let vector = vectors_of("deadlines");
    assert_eq!(vector.len(), 1, "the deadline expectation is stated once");
}

#[test]
fn a_clock_that_goes_backwards_is_reported_rather_than_absorbed() {
    let regression = vectors_of("regression");
    let quiet_for = regression[0]["quiet_for"].as_u64().expect("a gap");
    let mut heartbeat = EventStreamHeartbeat::attached_at(ATTACHED_AT);
    heartbeat.refresh(ATTACHED_AT + quiet_for).expect("time moved forward");
    let last = heartbeat.last_activity_milliseconds();
    assert_eq!(
        heartbeat.state_at(ATTACHED_AT).expect_err("a monotonic clock does not go backwards"),
        HeartbeatFault::ClockRegressed { last, named: ATTACHED_AT }
    );
    assert_eq!(
        heartbeat.refresh(ATTACHED_AT).expect_err("nor when recording activity"),
        HeartbeatFault::ClockRegressed { last, named: ATTACHED_AT }
    );
    assert_eq!(
        heartbeat.last_activity_milliseconds(),
        last,
        "a rejected instant changes nothing, so no liveness is invented"
    );
}

#[test]
fn a_heartbeat_may_be_held_to_a_timeout_a_test_chooses() {
    let chosen = heartbeat_interval_milliseconds();
    let heartbeat = EventStreamHeartbeat::attached_with_timeout(ATTACHED_AT, chosen);
    assert_eq!(heartbeat.timeout_milliseconds(), chosen);
    assert_eq!(
        heartbeat.state_at(ATTACHED_AT + chosen).expect("time moved forward"),
        ConnectionState::Healthy
    );
    assert_eq!(
        heartbeat.state_at(ATTACHED_AT + chosen + 1).expect("time moved forward"),
        ConnectionState::TimedOut
    );
}
