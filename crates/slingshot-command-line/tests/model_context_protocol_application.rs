//! One server, over the lines a client sends it.
//!
//! The composition claims are about counts and ownership: one answer per
//! request, none per notification, exactly one reservation released for each,
//! and nothing at all once input has ended or output has failed. Each of those
//! is the kind of thing that works in the ordinary case and breaks in the one
//! where a client disconnects halfway.

use serde_json::Value;

use slingshot_command_line::application::{Service, service_for};
use slingshot_command_line::invocation::{Invocation, SERVE_LEAF, Selection, parse};
use slingshot_command_line::model_context_protocol::application::{
    RESOURCE_EXHAUSTED_ERROR, Served, ServerApplication,
};
use slingshot_command_line::model_context_protocol::current_stateless_revision::{
    COMPLETE_MEMBER, INVALID_REQUEST_ERROR, PARSE_ERROR, UNSUPPORTED_REVISION_ERROR,
};
use slingshot_command_line::model_context_protocol::legacy_initialized_revision::Lifecycle;
use slingshot_command_line::model_context_protocol::standard_stream_transport::{
    OutputFailure, SUPPORTED_REVISIONS,
};

/// The revision the current era speaks.
const CURRENT: &str = "2026-07-28";

/// Returns the answer one line produced.
fn answered(server: &mut ServerApplication, line: &str) -> Value {
    match server.serve_line(line.as_bytes()) {
        Served::Answered(held) => serde_json::from_str(&held).expect("an answer is one document"),
        other => panic!("{line} produced {other:?}"),
    }
}

#[test]
fn one_request_produces_one_answer_and_releases_one_reservation() {
    let mut server = ServerApplication::new();
    let answer = answered(
        &mut server,
        &format!(r#"{{"id":"one","method":"ping","params":{{"protocolVersion":"{CURRENT}"}}}}"#),
    );
    assert_eq!(answer["id"].as_str(), Some("one"));
    assert_eq!(answer["result"][COMPLETE_MEMBER].as_str(), Some("complete"));
    assert_eq!(server.active(), 0, "an answered request holds nothing");
}

#[test]
fn a_notification_is_answered_never() {
    let mut server = ServerApplication::new();
    let produced =
        server.serve_line(br#"{"method":"notifications/cancelled","params":{"requestId":"one"}}"#);
    assert_eq!(produced, Served::Silent);
    assert_eq!(server.active(), 0);
}

#[test]
fn an_unreadable_line_is_a_parse_error_and_a_readable_one_that_is_not_a_request_is_not() {
    let mut server = ServerApplication::new();
    let unreadable = answered(&mut server, r#"{"id":"one","#);
    assert_eq!(unreadable["error"]["code"].as_i64(), Some(PARSE_ERROR));
    let directionless = answered(&mut server, r#"{"params":{}}"#);
    assert_eq!(directionless["error"]["code"].as_i64(), Some(INVALID_REQUEST_ERROR));
}

#[test]
fn a_request_naming_a_revision_this_build_does_not_serve_is_told_which_it_does() {
    let mut server = ServerApplication::new();
    let answer = answered(
        &mut server,
        r#"{"id":"one","method":"ping","params":{"protocolVersion":"2020-01-01"}}"#,
    );
    assert_eq!(answer["error"]["code"].as_i64(), Some(UNSUPPORTED_REVISION_ERROR));
    let supported: Vec<&str> = answer["error"]["data"]["supported"]
        .as_array()
        .expect("the supported list is a list")
        .iter()
        .filter_map(Value::as_str)
        .collect();
    assert_eq!(supported, SUPPORTED_REVISIONS.to_vec());
}

#[test]
fn an_initialize_begins_a_session_and_the_notification_finishes_it() {
    let mut server = ServerApplication::new();
    assert_eq!(server.lifecycle(), Lifecycle::Fresh);
    let answer = answered(
        &mut server,
        r#"{"id":"one","method":"initialize","params":{"protocolVersion":"2025-06-18"}}"#,
    );
    assert_eq!(answer["result"]["protocolVersion"].as_str(), Some(SUPPORTED_REVISIONS[1]));
    assert_eq!(server.lifecycle(), Lifecycle::Offered);
    server.serve_line(br#"{"method":"notifications/initialized"}"#);
    assert_eq!(server.lifecycle(), Lifecycle::Ready);
}

#[test]
fn a_duplicate_identifier_is_refused_without_disturbing_what_holds_it() {
    let mut server = ServerApplication::new();
    let line = format!(
        r#"{{"id":"one","method":"tools/list","params":{{"protocolVersion":"{CURRENT}"}}}}"#
    );
    answered(&mut server, &line);
    assert_eq!(server.active(), 0, "the first was answered and released");
    let again = answered(&mut server, &line);
    assert!(again["result"].is_object(), "a released identifier is reusable");
    assert_ne!(again["error"]["code"].as_i64(), Some(RESOURCE_EXHAUSTED_ERROR));
}

#[test]
fn nothing_is_served_once_this_server_has_finished() {
    let mut server = ServerApplication::new();
    let detached = server.finish(OutputFailure::SinkFailed);
    assert!(detached.is_empty(), "nothing was watching");
    let produced = server.serve_line(
        format!(r#"{{"id":"one","method":"ping","params":{{"protocolVersion":"{CURRENT}"}}}}"#)
            .as_bytes(),
    );
    assert_eq!(produced, Served::Finished);
    assert!(
        server.finish(OutputFailure::WriteExpired).is_empty(),
        "finishing twice ends nothing new"
    );
}

#[test]
fn the_serve_leaf_takes_a_target_and_nothing_a_caller_writes_a_command_with() {
    let invocation = parse(&[
        SERVE_LEAF.to_owned(),
        "--profile".to_owned(),
        "local".to_owned(),
        "--environment".to_owned(),
        "author".to_owned(),
    ])
    .expect("the serve leaf takes its target");
    assert_eq!(service_for(&invocation), Ok(Service::ModelContextProtocolServer));
    for refused in ["--machine", "--detach", "--operation-key", "--author-target-digest", "--path"]
    {
        let attempted = parse(&[SERVE_LEAF.to_owned(), refused.to_owned(), "value".to_owned()]);
        assert!(attempted.is_err(), "{refused} reached the serve leaf");
    }
}

#[test]
fn the_serve_leaf_is_not_versioned_because_it_starts_no_operation() {
    let invocation = Invocation {
        arguments: std::collections::BTreeMap::new(),
        detached: false,
        operation_key: None,
        output: None,
        selection: Selection {
            environment: Some("author".to_owned()),
            profile: Some("local".to_owned()),
        },
        verb: SERVE_LEAF.to_owned(),
    };
    let service = service_for(&invocation).expect("it routes");
    assert!(!service.is_versioned(), "handing over the streams talks to no daemon by itself");
}
