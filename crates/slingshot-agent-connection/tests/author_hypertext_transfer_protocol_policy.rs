//! What this daemon refuses from an author, and when it refuses it.
//!
//! Every test here is really about timing rather than about the rule. A header
//! limit checked after the headers were collected is not a limit; a protocol
//! check made after a body arrived is not a check. So the assertions are that
//! the refusal happens on the field that crosses the bound, and that a
//! conversation this daemon is not having is refused before anything in it is
//! interpreted.

use slingshot_agent_connection::author_hypertext_transfer_protocol_policy::{
    ExchangeDeadlines, HeadBounds, HeadReader, PERMITTED_CONTENT_CODINGS,
    PERMITTED_PROTOCOL_VERSIONS, ResponseHead, ResponseRefusal, retry_delay_milliseconds,
};
use slingshot_domain::author_agent_transport_contract::AuthorAgentTransportContract;

/// Bytes one header field may occupy, from the transport contract.
const FIELD_BYTES: u64 = 8_192;

/// Fields one head may carry, from the transport contract.
const FIELD_COUNT: u64 = 64;

/// Bytes one head may occupy, from the transport contract.
const HEAD_BYTES: u64 = 32_768;

/// Milliseconds a retry waits when the server asked for nothing.
const RETRY_BASE_MILLISECONDS: u64 = 250;

/// Milliseconds the longest honoured `Retry-After` lasts.
const RETRY_CAP_MILLISECONDS: u64 = 60_000;

/// A wait far longer than the cap, as a server might ask for.
const UNREASONABLE_WAIT_MILLISECONDS: u64 = 86_400_000;

/// Returns a head that the policy accepts.
fn acceptable() -> ResponseHead {
    ResponseHead {
        content_coding: Some("identity".to_owned()),
        location: None,
        protocol_version: "HTTP/1.1".to_owned(),
        trailers_declared: false,
        alternative_service_offered: false,
        informational: false,
    }
}

#[test]
fn the_bounds_and_deadlines_are_the_manifest_s_own() {
    let bounds = HeadBounds::embedded();
    assert_eq!(bounds.field_bytes, FIELD_BYTES);
    assert_eq!(bounds.field_count, FIELD_COUNT);
    assert_eq!(bounds.head_bytes, HEAD_BYTES);

    let deadlines = ExchangeDeadlines::embedded();
    let contract = AuthorAgentTransportContract::embedded();
    assert_eq!(
        deadlines.connect_milliseconds,
        contract.limit("author_connect_timeout_milliseconds")
    );
    assert_eq!(
        deadlines.transport_layer_security_milliseconds,
        contract.limit("author_tls_timeout_milliseconds")
    );
    assert!(
        deadlines.finite_idle_milliseconds < deadlines.finite_total_milliseconds,
        "a gap between bytes is noticed sooner than a whole response running long, which is why \
         they are two deadlines rather than one"
    );
}

#[test]
fn a_head_is_refused_on_the_field_that_crosses_a_bound() {
    let mut reader = HeadReader::new(HeadBounds::embedded());
    let field = usize::try_from(FIELD_BYTES).expect("a countable bound");
    reader.read_field("name", &"v".repeat(field - "name".len())).expect("the largest field");
    assert_eq!(reader.fields(), 1);

    let refused = reader.read_field("name", &"v".repeat(field));
    assert!(
        matches!(refused, Err(ResponseRefusal::FieldTooLong { .. })),
        "one byte further is refused as it arrives, not once the head is in hand: {refused:?}"
    );
    assert_eq!(reader.fields(), 1, "and the refused field was not counted");
}

#[test]
fn the_count_bound_and_the_head_bound_each_bind_on_their_own() {
    let mut reader = HeadReader::new(HeadBounds::embedded());
    let count = usize::try_from(FIELD_COUNT).expect("a countable bound");
    for index in 0..count {
        reader.read_field(&format!("name-{index}"), "v").expect("a field below the count");
    }
    let refused = reader.read_field("name-extra", "v");
    assert!(
        matches!(refused, Err(ResponseRefusal::TooManyFields { .. })),
        "one field past the count: {refused:?}"
    );

    let mut reader =
        HeadReader::new(HeadBounds { field_count: u64::MAX, ..HeadBounds::embedded() });
    let field = usize::try_from(FIELD_BYTES).expect("a countable bound");
    let head = usize::try_from(HEAD_BYTES).expect("a countable bound");
    let mut written = 0;
    while written + field <= head {
        reader.read_field("n", &"v".repeat(field - 1)).expect("a field below the head bound");
        written += field;
    }
    let refused = reader.read_field("n", &"v".repeat(field - 1));
    assert!(
        matches!(refused, Err(ResponseRefusal::HeadTooLong { .. })),
        "and the head bound binds even where the count would not: {refused:?}"
    );
}

#[test]
fn a_conversation_this_daemon_is_not_having_is_refused_before_anything_in_it() {
    let over_http_three = ResponseHead {
        protocol_version: "HTTP/3".to_owned(),
        location: Some("https://elsewhere.example".to_owned()),
        trailers_declared: true,
        ..acceptable()
    };
    assert!(
        matches!(
            over_http_three.require_acceptable(),
            Err(ResponseRefusal::ProtocolVersion { .. })
        ),
        "nothing about a conversation this daemon is not having is worth interpreting"
    );
    for version in PERMITTED_PROTOCOL_VERSIONS {
        ResponseHead { protocol_version: (*version).to_owned(), ..acceptable() }
            .require_acceptable()
            .expect("a protocol this daemon speaks");
    }
}

#[test]
fn every_way_a_server_can_move_the_conversation_is_refused() {
    let informational = ResponseHead { informational: true, ..acceptable() };
    assert!(
        matches!(
            informational.require_acceptable(),
            Err(ResponseRefusal::MigrationAttempted { mechanism: "an informational head" })
        ),
        "a head preceding a real one is a chance to apply the policy to the wrong response"
    );
    let alternative = ResponseHead { alternative_service_offered: true, ..acceptable() };
    assert!(
        matches!(
            alternative.require_acceptable(),
            Err(ResponseRefusal::MigrationAttempted { mechanism: "an alternative service" })
        ),
        "and an alternative service moves the next request somewhere unchecked"
    );
}

#[test]
fn a_redirect_is_refused_even_when_it_points_at_the_same_origin() {
    let elsewhere = ResponseHead {
        location: Some("https://elsewhere.example/other".to_owned()),
        ..acceptable()
    };
    assert!(matches!(elsewhere.require_acceptable(), Err(ResponseRefusal::RedirectOffered { .. })));

    let same_origin =
        ResponseHead { location: Some("/bin/slingshot/agent/other".to_owned()), ..acceptable() };
    assert!(
        matches!(same_origin.require_acceptable(), Err(ResponseRefusal::RedirectOffered { .. })),
        "selecting one author origin means asking nowhere else, including elsewhere on it"
    );
}

#[test]
fn a_coding_this_daemon_did_not_ask_for_is_refused() {
    for coding in PERMITTED_CONTENT_CODINGS {
        ResponseHead { content_coding: Some((*coding).to_owned()), ..acceptable() }
            .require_acceptable()
            .expect("the coding that means none");
    }
    let compressed = ResponseHead { content_coding: Some("gzip".to_owned()), ..acceptable() };
    assert!(
        matches!(
            compressed.require_acceptable(),
            Err(ResponseRefusal::UnexpectedContentCoding { .. })
        ),
        "a body of unknown decoded length has no bound, so a bound on it would be a fiction"
    );
    ResponseHead { content_coding: None, ..acceptable() }
        .require_acceptable()
        .expect("while declaring no coding is ordinary");
}

#[test]
fn declared_trailers_are_refused_because_they_arrive_too_late_to_matter() {
    let trailing = ResponseHead { trailers_declared: true, ..acceptable() };
    assert!(
        matches!(trailing.require_acceptable(), Err(ResponseRefusal::TrailersDeclared)),
        "a field that arrives after a body this daemon has already acted on cannot change what \
         it did"
    );
}

#[test]
fn a_server_asking_for_a_longer_wait_is_honoured_up_to_a_cap() {
    assert_eq!(
        retry_delay_milliseconds(None),
        RETRY_BASE_MILLISECONDS,
        "a server that asked for nothing gets the base wait"
    );
    assert_eq!(
        retry_delay_milliseconds(Some(RETRY_CAP_MILLISECONDS / 2)),
        RETRY_CAP_MILLISECONDS / 2,
        "a modest request is honoured exactly"
    );
    assert_eq!(
        retry_delay_milliseconds(Some(UNREASONABLE_WAIT_MILLISECONDS)),
        RETRY_CAP_MILLISECONDS,
        "without a cap a server could park a client indefinitely by asking it to"
    );
    assert_eq!(retry_delay_milliseconds(Some(RETRY_CAP_MILLISECONDS)), RETRY_CAP_MILLISECONDS);
}
