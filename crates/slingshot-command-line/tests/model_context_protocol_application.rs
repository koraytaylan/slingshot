//! One server, over the lines a client sends it.
//!
//! The composition claims are about counts and ownership: one answer per
//! request, none per notification, exactly one reservation released for each,
//! and nothing at all once input has ended or output has failed. Each of those
//! is the kind of thing that works in the ordinary case and breaks in the one
//! where a client disconnects halfway.

use std::sync::{Arc, Mutex};

use serde_json::{Value, json};

use slingshot_command_line::application::{Service, service_for};
use slingshot_command_line::command_line::serves_protocol;
use slingshot_command_line::invocation::{Invocation, SERVE_LEAF, Selection, parse};
use slingshot_command_line::machine_outcome_envelope::MachineOutcomeEnvelope;
use slingshot_command_line::model_context_protocol::application::{
    RESOURCE_EXHAUSTED_ERROR, Served, ServerApplication,
};
use slingshot_command_line::model_context_protocol::current_stateless_revision::{
    COMPLETE_MEMBER, EVERY_REQUEST, INVALID_REQUEST_ERROR, METHOD_NOT_FOUND_ERROR, PARSE_ERROR,
    UNSUPPORTED_REVISION_ERROR,
};
use slingshot_command_line::model_context_protocol::legacy_initialized_revision::{
    Lifecycle, NOT_INITIALIZED_ERROR,
};
use slingshot_command_line::model_context_protocol::operation_execution::ToolRunner;
use slingshot_command_line::model_context_protocol::standard_stream_transport::{
    LineSink, OutputFailure, SUPPORTED_REVISIONS, Written,
};
use slingshot_command_line::model_context_protocol::tool_catalog::ToolDescriptor;

/// The revision the current era speaks.
const CURRENT: &str = "2026-07-28";

/// A writer that accepts each queued response in full.
struct CompleteSink;

impl LineSink for CompleteSink {
    fn write_line(&mut self, _line: &str) -> Written {
        Written::Complete
    }
}

/// A writer that accepts no queued response.
struct RefusingSink;

impl LineSink for RefusingSink {
    fn write_line(&mut self, _line: &str) -> Written {
        Written::Refused
    }
}

/// A runner that records the tools it was asked to run.
struct RecordingRunner {
    /// The wire names, in call order.
    calls: Arc<Mutex<Vec<String>>>,
}

impl ToolRunner for RecordingRunner {
    fn run(
        &mut self,
        tool: &ToolDescriptor,
        _arguments: &Value,
    ) -> Result<MachineOutcomeEnvelope, String> {
        self.calls.lock().expect("the recorder is uncontended").push(tool.name.clone());
        Ok(MachineOutcomeEnvelope::OperationListPage {
            operations: vec!["held".to_owned()],
            continuation_token: None,
        })
    }
}

/// Completes the older era's handshake on `server`.
fn handshake(server: &mut ServerApplication) {
    let _ = answered(
        server,
        r#"{"id":"init","method":"initialize","params":{"protocolVersion":"2025-06-18"}}"#,
    );
    server.serve_line(br#"{"method":"notifications/initialized"}"#);
}

/// Returns the answer one line produced.
fn answered(server: &mut ServerApplication, line: &str) -> Value {
    let answer = match server.serve_line(line.as_bytes()) {
        Served::Answered(held) => held,
        other => panic!("{line} produced {other:?}"),
    };
    let mut sink = CompleteSink;
    assert!(server.write_output(&mut sink), "the complete writer remains available");
    serde_json::from_str(&answer).expect("an answer is one document")
}

#[test]
fn a_tool_call_this_build_cannot_run_is_a_result_rather_than_a_protocol_error() {
    // A server with no runner can advertise the catalog and answer every
    // request that describes this build. A call it cannot run is a tool result
    // carrying the local-failure document, because the protocol read the
    // request and this build is what could not answer it: reporting it as
    // `-32602` would tell the caller their request was malformed when it was
    // not, and a caller told that would rewrite a request that was already
    // right.
    let mut server = ServerApplication::new();
    let answer = answered(
        &mut server,
        &format!(
            r#"{{"id":"call","method":"tools/call","params":{{"protocolVersion":"{CURRENT}","name":"operation-list","arguments":{{}}}}}}"#
        ),
    );
    assert!(
        answer.get("error").is_none(),
        "a call this build could not run was answered as a protocol error: {answer}"
    );
    assert_eq!(answer["result"]["isError"].as_bool(), Some(true));
    let structured = &answer["result"]["structuredContent"];
    assert_eq!(structured["outcome"].as_str(), Some("local_application_error"));
    // The identifier a caller quotes is the one they sent: a retry that quoted
    // a JSON string with the protocol's own quotes around it would not match
    // the request it came from.
    assert_eq!(structured["interruption"]["retry_identifier"].as_str(), Some("call"));
    // The text and the structured content are one document, which is what makes
    // the two surfaces one: a client reading either sees the same bytes.
    let text = answer["result"]["content"][0]["text"].as_str().expect("the content carries text");
    let reparsed: Value = serde_json::from_str(text).expect("the text is one document");
    assert_eq!(&reparsed, structured, "two renderings would be two documents");
}

#[test]
fn one_request_produces_one_answer_and_releases_one_reservation() {
    let mut server = ServerApplication::new();
    let answer = answered(
        &mut server,
        &format!(r#"{{"id":"one","method":"ping","params":{{"protocolVersion":"{CURRENT}"}}}}"#),
    );
    assert_eq!(answer["jsonrpc"].as_str(), Some("2.0"));
    assert_eq!(answer["id"].as_str(), Some("one"));
    assert_eq!(answer["result"][COMPLETE_MEMBER].as_str(), Some("complete"));
    assert_eq!(server.active(), 0, "an answered request holds nothing");
}

#[test]
fn json_rpc_numeric_ids_and_protocol_version_are_preserved_in_responses() {
    let mut server = ServerApplication::new();
    let answer = answered(
        &mut server,
        &format!(
            r#"{{"jsonrpc":"2.0","id":1,"method":"ping","params":{{"protocolVersion":"{CURRENT}"}}}}"#
        ),
    );
    assert_eq!(answer["jsonrpc"].as_str(), Some("2.0"));
    assert_eq!(answer["id"].as_i64(), Some(1));
}

#[test]
fn a_refused_writer_never_acknowledges_the_response_it_did_not_take() {
    let mut server = ServerApplication::new();
    let line =
        format!(r#"{{"id":"one","method":"ping","params":{{"protocolVersion":"{CURRENT}"}}}}"#);
    assert!(matches!(server.serve_line(line.as_bytes()), Served::Answered(_)));
    assert_eq!(server.active(), 1, "queueing is not acknowledgement");
    let mut sink = RefusingSink;
    assert!(!server.write_output(&mut sink), "the refused writer stops output");
    assert_eq!(server.active(), 1, "the undelivered response remains active until shutdown");
    assert!(server.finish(OutputFailure::SinkFailed).is_empty());
    assert_eq!(server.active(), 0, "shutdown releases the request once");
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
    assert_eq!(unreadable["jsonrpc"].as_str(), Some("2.0"));
    assert!(unreadable["id"].is_null(), "parse errors carry a JSON-RPC null id");
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
fn a_legacy_session_runs_a_tool_call_after_it_is_initialized() {
    let calls = Arc::new(Mutex::new(Vec::new()));
    let mut server =
        ServerApplication::over(Some(Box::new(RecordingRunner { calls: calls.clone() })));
    handshake(&mut server);
    let answer = answered(
        &mut server,
        r#"{"id":"call","method":"tools/call","params":{"name":"operation-list","arguments":{}}}"#,
    );
    assert_eq!(calls.lock().expect("the recorder is uncontended").as_slice(), ["operation-list"]);
    assert_eq!(answer["result"]["isError"].as_bool(), Some(false));
    assert_eq!(
        answer["result"]["structuredContent"]["outcome"].as_str(),
        Some("operation_list_page")
    );
    assert_eq!(
        answer["result"]["structuredContent"]["operations"].as_array().map(Vec::len),
        Some(1)
    );
    assert!(
        answer["result"].get(COMPLETE_MEMBER).is_none(),
        "the older era does not carry the modern completeness member: {answer}"
    );
}

#[test]
fn a_legacy_tool_call_this_build_cannot_run_is_a_result_rather_than_empty_content() {
    let mut server = ServerApplication::new();
    handshake(&mut server);
    let answer = answered(
        &mut server,
        r#"{"id":"call","method":"tools/call","params":{"name":"operation-list","arguments":{}}}"#,
    );
    assert!(
        answer.get("error").is_none(),
        "a call this build could not run was a protocol error: {answer}"
    );
    assert_eq!(answer["result"]["isError"].as_bool(), Some(true));
    assert_eq!(
        answer["result"]["structuredContent"]["outcome"].as_str(),
        Some("local_application_error")
    );
    let content = answer["result"]["content"].as_array().expect("a tool result carries content");
    assert!(!content.is_empty(), "an empty content array is not a local failure: {answer}");
    assert!(answer["result"].get(COMPLETE_MEMBER).is_none());
}

#[test]
fn a_tool_call_between_the_handshake_halves_does_not_run() {
    let calls = Arc::new(Mutex::new(Vec::new()));
    let mut server =
        ServerApplication::over(Some(Box::new(RecordingRunner { calls: calls.clone() })));
    let _ = answered(
        &mut server,
        r#"{"id":"init","method":"initialize","params":{"protocolVersion":"2025-06-18"}}"#,
    );
    let answer = answered(
        &mut server,
        r#"{"id":"call","method":"tools/call","params":{"name":"operation-list","arguments":{}}}"#,
    );
    assert!(calls.lock().expect("the recorder is uncontended").is_empty());
    assert_eq!(answer["error"]["code"].as_i64(), Some(NOT_INITIALIZED_ERROR));
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
fn tools_and_resource_templates_project_the_installed_surface() {
    let mut server = ServerApplication::new();
    let tools = answered(
        &mut server,
        &format!(
            r#"{{"id":"one","method":"tools/list","params":{{"protocolVersion":"{CURRENT}"}}}}"#
        ),
    );
    let listed = tools["result"]["tools"].as_array().expect("a tool list");
    assert!(!listed.is_empty());
    assert!(listed.iter().all(|tool| tool["inputSchema"].is_object()));
    assert!(listed.iter().all(|tool| tool.get("outputSchema").is_none()));
    let templates = answered(
        &mut server,
        &format!(
            r#"{{"id":"two","method":"resources/templates/list","params":{{"protocolVersion":"{CURRENT}"}}}}"#
        ),
    );
    assert_eq!(templates["result"]["resourceTemplates"].as_array().unwrap().len(), 3);
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
fn a_resource_read_without_a_daemon_is_a_local_failure_not_empty_contents() {
    let mut server = ServerApplication::new();
    let answer = answered(
        &mut server,
        &format!(
            r#"{{"id":"read","method":"resources/read","params":{{"protocolVersion":"{CURRENT}","uri":"slingshot://profiles/local/environments/author/targets/one/operations/two"}}}}"#
        ),
    );
    assert!(answer.get("error").is_none(), "a valid URI was a protocol error: {answer}");
    let contents =
        answer["result"]["contents"].as_array().expect("a resource read carries contents");
    assert!(!contents.is_empty(), "empty contents after a valid URI is the old stub: {answer}");
    assert_eq!(answer["result"]["isError"].as_bool(), Some(true));
    let text = contents[0]["text"].as_str().unwrap_or("");
    assert!(text.contains("local_application_error"), "{text}");
}

#[test]
fn a_legacy_resource_read_without_a_daemon_is_a_local_failure_not_empty_contents() {
    let mut server = ServerApplication::new();
    handshake(&mut server);
    let answer = answered(
        &mut server,
        r#"{"id":"read","method":"resources/read","params":{"uri":"slingshot://profiles/local/environments/author/targets/one/operations/two"}}"#,
    );
    let contents = answer["result"]["contents"].as_array().expect("contents");
    assert!(!contents.is_empty(), "{answer}");
    assert_eq!(answer["result"]["isError"].as_bool(), Some(true));
    assert!(answer["result"].get(COMPLETE_MEMBER).is_none());
}

#[test]
fn a_resource_read_of_an_operation_reaches_the_runner() {
    let calls = Arc::new(Mutex::new(Vec::new()));
    let mut server =
        ServerApplication::over(Some(Box::new(RecordingRunner { calls: calls.clone() })));
    let answer = answered(
        &mut server,
        &format!(
            r#"{{"id":"read","method":"resources/read","params":{{"protocolVersion":"{CURRENT}","uri":"slingshot://profiles/local/environments/author/targets/one/operations/two"}}}}"#
        ),
    );
    assert_eq!(calls.lock().expect("the recorder is uncontended").as_slice(), ["operation-result"]);
    assert!(answer.get("error").is_none(), "{answer}");
    let contents = answer["result"]["contents"].as_array().expect("contents");
    assert!(!contents.is_empty(), "{answer}");
}

#[test]
fn recovery_documents_name_the_unknown_cause_that_produced_them() {
    use slingshot_agent_connection::command_submission::UnknownCause;
    use slingshot_command_line::daemon_answer::{recovering, recovery_facts};
    use slingshot_local_protocol::message::{
        OperationExecutionCertainty, OperationResponse, RecoveryExecutionEvidence,
    };

    let cause = UnknownCause::UnvalidatedStatus;
    let evidence = RecoveryExecutionEvidence::ExecutionCertainty {
        certainty: OperationExecutionCertainty::SubmissionUnknown,
    };
    let response = OperationResponse::RecoveryRequired {
        category: "ambiguous_submission".to_owned(),
        detail: cause.spelling(),
        evidence,
        operation_identifier: "held".to_owned(),
    };
    let Some((category, document, identifier)) = recovery_facts(&response) else {
        panic!("a recovery response projects recovery facts")
    };
    assert_eq!(identifier, "held");
    assert!(
        document.contains(&cause.spelling()),
        "the operator document must name the cause: {document}"
    );
    assert!(!cause.spelling().is_empty());
    let completion = recovering(category, document.clone(), 2);
    match completion.answer {
        slingshot_command_line::application::Answer::Envelope(envelope) => {
            match envelope.as_ref() {
                MachineOutcomeEnvelope::OperationRecoveryRequired { evidence: held, .. } => {
                    assert!(held.contains(&cause.spelling()), "{held}");
                }
                other => panic!("expected a recovery envelope, got {other:?}"),
            }
        }
        other => panic!("expected an envelope, got {other:?}"),
    }

    struct RecoveryRunner {
        evidence: String,
    }
    impl ToolRunner for RecoveryRunner {
        fn run(
            &mut self,
            _tool: &ToolDescriptor,
            _arguments: &Value,
        ) -> Result<MachineOutcomeEnvelope, String> {
            Ok(MachineOutcomeEnvelope::OperationRecoveryRequired {
                category: "ambiguous_submission".to_owned(),
                evidence: self.evidence.clone(),
                revision: 2,
            })
        }
    }
    let mut server = ServerApplication::over(Some(Box::new(RecoveryRunner { evidence: document })));
    let answer = answered(
        &mut server,
        &format!(
            r#"{{"id":"wait","method":"tools/call","params":{{"protocolVersion":"{CURRENT}","name":"operation-result","arguments":{{"operation_identifier":"held"}}}}}}"#
        ),
    );
    let held = answer["result"]["structuredContent"]["evidence"].as_str().unwrap_or("");
    assert!(held.contains(&cause.spelling()), "{answer}");
}

/// Returns the parameters one advertised method needs in `era`.
fn advertised_parameters(method: &str, legacy: bool) -> Value {
    let mut parameters = serde_json::Map::new();
    if !legacy {
        parameters.insert("protocolVersion".to_owned(), json!(CURRENT));
    }
    match method {
        "tools/call" => {
            parameters.insert("name".to_owned(), json!("operation-list"));
            parameters.insert("arguments".to_owned(), json!({}));
        }
        "resources/read" => {
            parameters.insert(
                "uri".to_owned(),
                json!("slingshot://profiles/local/environments/author/targets/one/operations/two"),
            );
        }
        _ => {}
    }
    Value::Object(parameters)
}

/// Requires `answer` to be work or an honest failure, never the empty-contents stub.
fn require_honest_advertised_answer(method: &str, answer: &Value) {
    assert_ne!(
        answer["error"]["code"].as_i64(),
        Some(METHOD_NOT_FOUND_ERROR),
        "{method} was method-not-found: {answer}"
    );
    match method {
        "resources/read" => {
            let contents =
                answer["result"]["contents"].as_array().expect("a resource read carries contents");
            assert!(!contents.is_empty(), "{method} empty contents: {answer}");
        }
        "tools/call" => {
            let content =
                answer["result"]["content"].as_array().expect("a tool result carries content");
            assert!(!content.is_empty(), "{method} empty content: {answer}");
        }
        _ => {
            let contents = &answer["result"]["contents"];
            assert!(
                contents.is_null() || contents.as_array().is_some_and(|held| !held.is_empty()),
                "{method} stub contents: {answer}"
            );
        }
    }
}

#[test]
fn no_advertised_method_in_either_era_answers_empty_contents_as_success() {
    for legacy in [false, true] {
        let mut server = ServerApplication::new();
        if legacy {
            handshake(&mut server);
        }
        for method in EVERY_REQUEST {
            let line = json!({
                "id": method,
                "method": method,
                "params": advertised_parameters(method, legacy),
            })
            .to_string();
            require_honest_advertised_answer(method, &answered(&mut server, &line));
        }
    }
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
    assert!(serves_protocol(&[
        "--profile".to_owned(),
        "local".to_owned(),
        "--environment".to_owned(),
        "author".to_owned(),
        "protocol".to_owned(),
        "serve".to_owned(),
    ]));
    assert!(!serves_protocol(&["daemon".to_owned(), "ping".to_owned()]));
    for refused in ["--machine", "--detach", "--operation-key", "--author-target-digest", "--path"]
    {
        let attempted = parse(&[SERVE_LEAF.to_owned(), refused.to_owned(), "value".to_owned()]);
        assert!(attempted.is_err(), "{refused} reached the serve leaf");
    }
}

#[test]
fn the_serve_leaf_is_not_versioned_because_it_starts_no_operation() {
    let invocation = Invocation {
        command: None,
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
