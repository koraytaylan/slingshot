//! Getting one lost stream back without inventing anything while it is gone.
//!
//! Three things are proved. The delay sequence is deterministic given the
//! injected samples and never leaves the named full-jitter interval, so a fleet
//! that all lost the same author returns spread out rather than together, and
//! nothing here sleeps to find that out.
//!
//! The position resumed from is the one durably committed and never the last
//! one seen, over a route built from persisted facts with exactly two members
//! in canonical order. A server that offers another route is refused by name.
//!
//! And the one case reconnecting cannot fix is separated from the ones it can:
//! a position arriving twice with different contents leaves the ledger exactly
//! as it was, degrades the subscription, and blocks streaming until a full
//! high-water reset. Throughout, no disconnect, no reconnect, no clock jump,
//! and no reset touches anything about a remote job.

use slingshot_agent_connection::author_hypertext_transfer_protocol_policy::ResponseHead;
use slingshot_agent_connection::command_submission::{StatusClass, classify_status};
use slingshot_agent_connection::event_stream_reconnection::{
    CommitOutcome, DELAY_MULTIPLIER, EVENT_ROUTE, EventStreamReconnection, FIRST_ATTEMPT,
    GENERATION_QUERY_MEMBER, LAST_EVENT_IDENTIFIER_HEADER, RECONNECT_CATEGORY, ReconnectCause,
    ReconnectionRefusal, ReconnectionSchedule, RecoveryRoute, ResetReason,
    SUBSCRIPTION_QUERY_MEMBER, StreamHealth, SubscriptionLedger, bounded_retry_after_milliseconds,
    initial_delay_milliseconds, jitter_ceiling_milliseconds, maximum_attempts,
    maximum_delay_milliseconds, reset_route, resumed_delay_milliseconds, schedule_for,
    undeclared_trailer,
};
use slingshot_agent_connection::server_sent_event_decoder::{DecoderBounds, EventStreamCursor};
use slingshot_domain::author_agent_transport_contract::AuthorAgentTransportContract;

/// Where the vectors this suite is driven from live.
const FIXTURES: &str = "tests/fixtures/event-stream-reconnection.jsonl";

/// The subscription every vector is about.
const SUBSCRIPTION: &str = "daemon-subscription-one";

/// The generation every vector is about.
const GENERATION: u64 = 7;

/// A later generation, after the store has been rebuilt.
const LATER_GENERATION: u64 = 8;

/// One wall-clock instant, for the vectors that need one.
const NOW: u64 = 1_700_000_000_000;

/// The protocol version the author is spoken to over.
const SPOKEN_VERSION: &str = "HTTP/1.1";

/// The other protocol version the author is spoken to over.
const OTHER_SPOKEN_VERSION: &str = "HTTP/2";

/// A protocol version this daemon does not speak.
const UNSPOKEN_VERSION: &str = "HTTP/1.0";

/// Where a server would rather this stream went.
const OFFERED_ROUTE: &str = "https://elsewhere.example/events";

/// A content coding that hides a stream's decoded length.
const UNEXPECTED_CODING: &str = "gzip";

/// What one event at a position hashes to.
const FIRST_CONTENTS: &str = "contents-of-the-first-position";

/// What a second, disagreeing account of that same position hashes to.
const OTHER_CONTENTS: &str = "another-account-of-the-first-position";

/// Returns every vector the fixture holds.
fn vectors() -> Vec<serde_json::Value> {
    let text = std::fs::read_to_string(FIXTURES).expect("the vectors are readable");
    text.lines().map(|line| serde_json::from_str(line).expect("each line is one vector")).collect()
}

/// Returns every vector of `kind`.
fn vectors_of(kind: &str) -> Vec<serde_json::Value> {
    vectors().into_iter().filter(|vector| vector["kind"].as_str() == Some(kind)).collect()
}

/// Returns a head with nothing wrong with it.
fn clean_head() -> ResponseHead {
    ResponseHead {
        alternative_service_offered: false,
        content_coding: None,
        informational: false,
        location: None,
        protocol_version: SPOKEN_VERSION.to_owned(),
        trailers_declared: false,
    }
}

/// Returns a head with one named defect.
fn head_with(defect: &str) -> ResponseHead {
    let mut head = clean_head();
    match defect {
        "informational-head" => head.informational = true,
        "declared-trailer" => head.trailers_declared = true,
        "unsupported-protocol-version" => head.protocol_version = UNSPOKEN_VERSION.to_owned(),
        "alternative-service-offered" => head.alternative_service_offered = true,
        "unexpected-content-coding" => head.content_coding = Some(UNEXPECTED_CODING.to_owned()),
        "server-offered-route" => head.location = Some(OFFERED_ROUTE.to_owned()),
        other => panic!("{other} is a defect this suite does not stage"),
    }
    head
}

/// Returns the cursor `spelling` names.
fn cursor(spelling: &str) -> EventStreamCursor {
    EventStreamCursor::new(spelling, DecoderBounds::embedded().identifier_bytes)
        .expect("these cursors are short")
}

/// Returns the reason `spelling` names.
fn reason_named(spelling: &str) -> ResetReason {
    match spelling {
        "cursor-expired" => ResetReason::CursorExpired,
        "generation-changed" => ResetReason::GenerationChanged,
        "equal-cursor-digest-conflict" => ResetReason::EqualCursorDigestConflict,
        other => panic!("{other} is a reason this suite does not name"),
    }
}

/// Returns a subscription that has just opened.
fn opening() -> EventStreamReconnection {
    EventStreamReconnection::opening(SUBSCRIPTION, GENERATION)
}

#[test]
fn the_delay_sequence_follows_the_samples_and_never_leaves_its_named_interval() {
    for vector in vectors_of("jitter") {
        let name = vector["name"].as_str().expect("a name");
        let attempt = vector["attempt"].as_u64().expect("an attempt");
        let sample = vector["sample"].as_u64().expect("a sample");
        assert_eq!(
            jitter_ceiling_milliseconds(attempt),
            vector["ceiling"].as_u64().expect("a ceiling"),
            "{name}: the interval doubles until the cap decides it"
        );
        let schedule = schedule_for(attempt, ReconnectCause::TransportFailure, sample, NOW);
        assert_eq!(
            schedule.chosen_delay_milliseconds,
            vector["delay"].as_u64().expect("a delay"),
            "{name}: the same sample chooses the same delay every time"
        );
        assert!(
            schedule.chosen_delay_milliseconds <= schedule.jitter_ceiling_milliseconds,
            "{name}: full jitter chooses within the interval, not the interval itself"
        );
        assert!(schedule.jitter_ceiling_milliseconds <= maximum_delay_milliseconds());
        assert_eq!(schedule.category, RECONNECT_CATEGORY);
        assert_eq!(
            schedule.eligible_at_unix_milliseconds,
            NOW + schedule.chosen_delay_milliseconds
        );
    }
}

#[test]
fn distinct_samples_spread_across_the_interval_rather_than_landing_together() {
    let attempt = maximum_attempts();
    let ceiling = jitter_ceiling_milliseconds(attempt);
    let chosen: Vec<u64> = [0, ceiling / DELAY_MULTIPLIER, ceiling]
        .iter()
        .map(|sample| {
            schedule_for(attempt, ReconnectCause::CleanClose, *sample, NOW)
                .chosen_delay_milliseconds
        })
        .collect();
    assert_eq!(chosen, vec![0, ceiling / DELAY_MULTIPLIER, ceiling]);
    let mut distinct = chosen.clone();
    distinct.dedup();
    assert_eq!(
        distinct.len(),
        chosen.len(),
        "a fleet that returns together loses the author again"
    );
    assert_eq!(jitter_ceiling_milliseconds(FIRST_ATTEMPT), initial_delay_milliseconds());
}

#[test]
fn a_connection_the_shared_policy_accepts_resets_the_next_delay_to_the_initial_one() {
    let mut stream = opening();
    for _ in 0..maximum_attempts() {
        stream.schedule_next(ReconnectCause::HeartbeatTimeout, 0, NOW).expect("attempts remain");
    }
    assert!(stream.attempt() > FIRST_ATTEMPT);
    stream.connected(&clean_head()).expect("a clean head is a connection");
    assert_eq!(stream.attempt(), FIRST_ATTEMPT);
    assert_eq!(jitter_ceiling_milliseconds(stream.attempt()), initial_delay_milliseconds());
    let mut other = opening();
    let mut head = clean_head();
    head.protocol_version = OTHER_SPOKEN_VERSION.to_owned();
    other.connected(&head).expect("both spoken versions are connections");
}

#[test]
fn reconnecting_is_attempted_a_bounded_number_of_times() {
    let mut stream = opening();
    let allowed = maximum_attempts();
    for attempt in FIRST_ATTEMPT..=allowed {
        let schedule = stream
            .schedule_next(ReconnectCause::TransportFailure, 0, NOW)
            .expect("attempts remain");
        assert_eq!(schedule.attempt, attempt);
    }
    assert!(matches!(
        stream.schedule_next(ReconnectCause::TransportFailure, 0, NOW),
        Err(ReconnectionRefusal::AttemptsExhausted { .. })
    ));
}

#[test]
fn the_route_carries_exactly_the_persisted_pair_in_canonical_order() {
    let vector = vectors_of("route").remove(0);
    let route = opening().route().expect("the persisted facts make one route");
    assert_eq!(route, vector["route"].as_str().expect("a route"));
    assert!(route.starts_with(EVENT_ROUTE), "a fixed route is not one a server chooses");
    let query = route.split_once('?').expect("one query").1;
    let members: Vec<&str> = query.split('&').map(|pair| pair.split('=').next().unwrap()).collect();
    assert_eq!(
        members,
        vec![GENERATION_QUERY_MEMBER, SUBSCRIPTION_QUERY_MEMBER],
        "a member more or fewer, or in another order, is a different subscription"
    );
    let allowed = AuthorAgentTransportContract::embedded().limit("maximum_route_query_bytes");
    let overlong = EventStreamReconnection::opening(&"s".repeat(allowed as usize), GENERATION);
    assert!(matches!(overlong.route(), Err(ReconnectionRefusal::QueryTooLong { .. })));
}

#[test]
fn a_position_is_resumed_from_only_after_it_has_been_committed() {
    let mut stream = opening();
    assert_eq!(
        stream.last_event_identifier(),
        None,
        "the first connection resumes from nothing, because nothing has been applied"
    );
    assert_eq!(
        stream.commit_cursor(&cursor("cursor-0001"), FIRST_CONTENTS),
        CommitOutcome::Advanced
    );
    assert_eq!(stream.last_event_identifier(), Some("cursor-0001"));
    assert_eq!(
        LAST_EVENT_IDENTIFIER_HEADER, "Last-Event-ID",
        "the header is the protocol's, spelled exactly"
    );
    let received_but_uncommitted = cursor("cursor-0002");
    assert_eq!(
        stream.last_event_identifier(),
        Some("cursor-0001"),
        "a position that arrived and was not written down describes events nobody applied"
    );
    assert_eq!(received_but_uncommitted.as_text(), "cursor-0002");
}

#[test]
fn one_ledger_advances_once_per_position_however_the_jobs_interleave() {
    let mut ledger = SubscriptionLedger::empty(GENERATION);
    for (position, contents) in [
        ("cursor-0001", "alpha-progress"),
        ("cursor-0002", "beta-accepted"),
        ("cursor-0003", "alpha-progress-again"),
        ("cursor-0004", "beta-started"),
    ] {
        assert_eq!(ledger.commit(&cursor(position), contents), CommitOutcome::Advanced);
    }
    assert_eq!(
        ledger.last_event_identifier(),
        Some("cursor-0004"),
        "cursor progress is the subscription's and is not conditioned on any local job"
    );
    assert_eq!(
        ledger.commit(&cursor("cursor-0004"), "beta-started"),
        CommitOutcome::Unchanged,
        "a replayed event moves nothing, which is what replay should do"
    );
}

#[test]
fn one_position_with_two_accounts_degrades_the_subscription_and_demands_a_reset() {
    let mut stream = opening();
    stream.commit_cursor(&cursor("cursor-0001"), FIRST_CONTENTS);
    assert_eq!(stream.health(), StreamHealth::Healthy);
    assert_eq!(
        stream.commit_cursor(&cursor("cursor-0001"), OTHER_CONTENTS),
        CommitOutcome::Conflicted
    );
    assert_eq!(
        stream.last_event_identifier(),
        Some("cursor-0001"),
        "choosing between two disagreeing accounts of one position is what cannot be done here"
    );
    assert_eq!(stream.health(), StreamHealth::Degraded);
    assert_eq!(stream.outstanding_reset(), Some(ResetReason::EqualCursorDigestConflict));
    assert!(matches!(stream.route(), Err(ReconnectionRefusal::ResetRequired)));
    assert!(matches!(stream.connected(&clean_head()), Err(ReconnectionRefusal::ResetRequired)));
    stream.reset_completed(GENERATION);
    assert_eq!(stream.health(), StreamHealth::Healthy);
    assert_eq!(stream.last_event_identifier(), None);
    assert!(stream.route().is_ok());
}

#[test]
fn every_reset_reason_invents_no_advance_and_routes_to_the_one_recovery() {
    for vector in vectors_of("reset") {
        let name = vector["name"].as_str().expect("a name");
        let reason = reason_named(vector["reason"].as_str().expect("a reason"));
        let mut stream = opening();
        stream.commit_cursor(&cursor("cursor-0001"), FIRST_CONTENTS);
        let route = stream.require_reset(reason);
        assert_eq!(route, RecoveryRoute::HighWaterSnapshotReset, "{name}");
        assert_eq!(reset_route(reason), route);
        assert_eq!(
            stream.last_event_identifier(),
            Some("cursor-0001"),
            "{name}: a position that means nothing is not replaced by an invented one"
        );
        assert_eq!(stream.health(), StreamHealth::Degraded);
        stream.reset_completed(LATER_GENERATION);
        assert_eq!(stream.ledger().generation(), LATER_GENERATION);
        assert_eq!(stream.last_event_identifier(), None);
    }
}

#[test]
fn a_restart_reconstructs_only_the_wait_it_persisted() {
    for vector in vectors_of("restart") {
        let name = vector["name"].as_str().expect("a name");
        let schedule = ReconnectionSchedule {
            attempt: FIRST_ATTEMPT,
            category: RECONNECT_CATEGORY,
            chosen_delay_milliseconds: vector["chosen"].as_u64().expect("a delay"),
            cause: ReconnectCause::TransportFailure,
            eligible_at_unix_milliseconds: vector["eligible_at"].as_u64().expect("an instant"),
            jitter_ceiling_milliseconds: maximum_delay_milliseconds(),
        };
        assert_eq!(
            resumed_delay_milliseconds(&schedule, vector["now"].as_u64().expect("an instant")),
            vector["resumed"].as_u64().expect("what remains"),
            "{name}: wall-clock movement cannot lengthen a wait beyond the one that was chosen"
        );
    }
}

#[test]
fn the_status_policy_is_the_shared_one_and_a_retry_after_is_bounded() {
    for vector in vectors_of("status") {
        let name = vector["name"].as_str().expect("a name");
        let status = u16::try_from(vector["status"].as_u64().expect("a status")).expect("small");
        assert_eq!(
            matches!(classify_status(status), StatusClass::Retryable),
            vector["retryable"].as_bool().expect("an expectation"),
            "{name}: one status policy, read the same way wherever it is read"
        );
    }
    for vector in vectors_of("retry-after") {
        let name = vector["name"].as_str().expect("a name");
        assert_eq!(
            bounded_retry_after_milliseconds(vector["requested"].as_u64().expect("a request")),
            vector["honoured"].as_u64().expect("what is honoured"),
            "{name}: without a cap the worst a server can do to a stream is unbounded"
        );
    }
}

#[test]
fn a_head_the_shared_policy_refuses_never_resets_the_backoff() {
    for vector in vectors_of("head") {
        let name = vector["name"].as_str().expect("a name");
        let mut stream = opening();
        stream.schedule_next(ReconnectCause::TransportFailure, 0, NOW).expect("attempts remain");
        let attempt = stream.attempt();
        let refusal = stream
            .connected(&head_with(vector["defect"].as_str().expect("a defect")))
            .expect_err("a refused head is not a connection");
        let named = match refusal {
            ReconnectionRefusal::Head(_) => "head",
            ReconnectionRefusal::ServerRouteOffered { .. } => "server-route",
            other => panic!("{name}: {other} is not a refusal this suite stages"),
        };
        assert_eq!(named, vector["refusal"].as_str().expect("a refusal"));
        assert_eq!(
            stream.attempt(),
            attempt,
            "{name}: a server that answers quickly and wrongly has not answered"
        );
    }
}

#[test]
fn an_undeclared_trailer_reconnects_without_retracting_or_inventing_a_position() {
    let mut stream = opening();
    stream.commit_cursor(&cursor("cursor-0001"), FIRST_CONTENTS);
    assert_eq!(undeclared_trailer(), ReconnectionRefusal::UndeclaredTrailer);
    let schedule = stream
        .schedule_next(ReconnectCause::ProtocolLoss, 0, NOW)
        .expect("protocol loss is reconnected from");
    assert_eq!(schedule.cause, ReconnectCause::ProtocolLoss);
    assert_eq!(
        stream.last_event_identifier(),
        Some("cursor-0001"),
        "a framing failure retracts no event that was independently committed"
    );
    assert_eq!(stream.health(), StreamHealth::Healthy);
    assert_eq!(stream.outstanding_reset(), None);
}

#[test]
fn nothing_a_connection_does_touches_what_is_known_about_remote_work() {
    let mut stream = opening();
    stream.commit_cursor(&cursor("cursor-0001"), FIRST_CONTENTS);
    let before = stream.ledger().clone();
    for cause in [
        ReconnectCause::CleanClose,
        ReconnectCause::HeartbeatTimeout,
        ReconnectCause::ProtocolLoss,
        ReconnectCause::RetryableStatus { status: 503 },
        ReconnectCause::TransportFailure,
    ] {
        stream.schedule_next(cause, 0, NOW).expect("attempts remain");
        assert_eq!(
            stream.ledger(),
            &before,
            "losing a connection is this daemon's problem and says nothing about the work"
        );
    }
    stream.connected(&clean_head()).expect("reconnecting works");
    assert_eq!(stream.ledger(), &before);
    assert_eq!(stream.ledger().generation(), GENERATION, "a connection changes no generation");
}
