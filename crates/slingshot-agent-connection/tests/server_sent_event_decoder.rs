//! Reading somebody else's stream without letting them decide what it costs.
//!
//! Two properties are proved here. The first is that decoding does not depend
//! on how the transport split the bytes: every valid stream is decoded at every
//! chunk size and must produce the identical items, because a decoder whose
//! answer moves with the packet boundaries is a decoder whose answer is not
//! about the stream.
//!
//! The second is that every quantity the far side chooses is bounded by name,
//! and that each bound admits its exact value and refuses one byte past it. A
//! bound that were checked after the bytes were collected would be a bound on
//! nothing, so the failing cases are the ones that matter.
//!
//! Nothing is inferred from anything else: a cursor is the identifier field and
//! never a sequence, a sequence is the document's and never a cursor, and an
//! ending is authenticated in full - transport contract, canonical byte
//! contract, both role schemas, the limits, the version, the wire name, and the
//! submitted digest - before it is exposed as an ending at all.

use slingshot_agent_connection::author_hypertext_transfer_protocol_policy::ResponseHead;
use slingshot_agent_connection::server_sent_event_decoder::{
    DATA_FIELD, DecoderBounds, EVENT_STREAM_MEDIA_TYPE, EventStreamCursor, IDENTIFIER_FIELD,
    RETRY_FIELD, ServerSentEventDecoder, StreamExpectation, StreamItem, StreamRefusal,
    require_event_stream,
};
use slingshot_agent_protocol::identity::{AGENT_FORMAT, DocumentProvenance, WireContractIdentity};
use slingshot_agent_protocol::job_contract::JobEventKind;
use slingshot_agent_protocol::wire_contract::{ExpectedProvenance, WireRefusal};
use slingshot_domain::author_agent_transport_contract::AuthorAgentTransportContract;
use slingshot_domain::command::schema::canonical_contract_digest;
use slingshot_domain::selected_command_contract_identity::SelectedCommandContractIdentity;

/// Where the streams this suite is driven from live.
const FIXTURES: &str = "tests/fixtures/server-sent-events";

/// The subscription every fixture stream was asked for under.
const SUBSCRIPTION: &str = "daemon-subscription-one";

/// The generation every fixture stream was asked for under.
const GENERATION: u64 = 7;

/// The command these streams carry events about.
const COMMAND: &str = "query_paths";

/// The submission these streams are about.
const SUBMITTED_DIGEST: &str = "1111111111111111111111111111111111111111111111111111111111111111";

/// A digest substituted where a real one belongs.
const SUBSTITUTED_DIGEST: &str = "0000000000000000000000000000000000000000000000000000000000000000";

/// The operation a terminal event ends.
const TERMINAL_OPERATION: &str = "operation-alpha";

/// Where a terminal event sits in that operation's own sequence.
const TERMINAL_SEQUENCE: u64 = 9;

/// The chunk sizes every valid stream is decoded at.
const CHUNK_SIZES: &[usize] = &[1, 2, 3, 7, 64, 4096];

/// The protocol version the author is spoken to over.
const SPOKEN_VERSION: &str = "HTTP/1.1";

/// A protocol version this daemon does not speak.
const UNSPOKEN_VERSION: &str = "HTTP/1.0";

/// Where a redirect would send a stream that followed one.
const REDIRECT_TARGET: &str = "https://elsewhere.example/events";

/// A content coding that hides a stream's decoded length.
const UNEXPECTED_CODING: &str = "gzip";

/// Media types that are not exactly one event stream.
const REFUSED_MEDIA_TYPES: &[&str] = &[
    "application/json",
    "text/event-stream, text/event-stream",
    "text/event-stream, application/json",
    "text/event-streamx",
];

/// Media parameters that are not an absent or UTF-8 character set.
const REFUSED_MEDIA_PARAMETERS: &[&str] = &[
    "text/event-stream; charset=iso-8859-1",
    "text/event-stream; charset=utf-8; charset=utf-8",
    "text/event-stream; boundary=something",
];

/// Returns the expectations manifest.
fn expectations() -> serde_json::Value {
    let path = format!("{FIXTURES}/expectations.json");
    let text = std::fs::read_to_string(&path).unwrap_or_else(|_| panic!("{path} is readable"));
    serde_json::from_str(&text).unwrap_or_else(|_| panic!("{path} is one value"))
}

/// Returns the exact bytes `file` holds.
fn stream_bytes(file: &str) -> Vec<u8> {
    let path = format!("{FIXTURES}/{file}");
    std::fs::read(&path).unwrap_or_else(|_| panic!("{path} is readable"))
}

/// Returns what this build has, for the command these streams are about.
fn installed_provenance() -> ExpectedProvenance {
    ExpectedProvenance {
        canonical_json_contract_digest: canonical_contract_digest(),
        command_contract: SelectedCommandContractIdentity::installed(COMMAND)
            .expect("the command is published"),
        transport_contract_digest: AuthorAgentTransportContract::embedded_digest(),
    }
}

/// Returns what the request every fixture answers asked for.
fn expectation() -> StreamExpectation {
    StreamExpectation {
        agent_event_store_generation: GENERATION,
        daemon_subscription_identifier: SUBSCRIPTION.to_owned(),
        expected_provenance: installed_provenance(),
        submitted_command_digest: SUBMITTED_DIGEST.to_owned(),
    }
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

/// Returns a decoder attached to an acceptable event-stream response.
fn attached() -> ServerSentEventDecoder {
    ServerSentEventDecoder::attached(
        &clean_head(),
        EVENT_STREAM_MEDIA_TYPE,
        DecoderBounds::embedded(),
        expectation(),
    )
    .expect("this response is one event stream")
}

/// Returns everything `bytes` decode to when pushed `chunk` bytes at a time.
fn decode_in_chunks(bytes: &[u8], chunk: usize) -> Result<Vec<StreamItem>, StreamRefusal> {
    let mut decoder = attached();
    let mut items = Vec::new();
    for part in bytes.chunks(chunk) {
        items.extend(decoder.push(part)?);
    }
    Ok(items)
}

/// Returns how each closed refusal is spelled in the manifest.
fn refusal_named(refusal: &StreamRefusal) -> &'static str {
    match refusal {
        StreamRefusal::Malformed { .. } => "malformed",
        StreamRefusal::AnotherSubscription => "another-subscription",
        StreamRefusal::AnotherGeneration { .. } => "another-generation",
        StreamRefusal::TerminalWithoutCorrelation => "terminal-without-correlation",
        StreamRefusal::CorrelationOnNonTerminal => "correlation-on-non-terminal",
        StreamRefusal::AnotherSubmission => "another-submission",
        other => panic!("{other} is a refusal this suite does not name"),
    }
}

/// Returns the kind `spelling` names.
fn kind_named(spelling: &str) -> JobEventKind {
    match spelling {
        "accepted" => JobEventKind::Accepted,
        "started" => JobEventKind::Started,
        "progress" => JobEventKind::Progress,
        "succeeded" => JobEventKind::Succeeded,
        "failed" => JobEventKind::Failed,
        other => panic!("{other} is a kind this suite does not name"),
    }
}

/// Returns the stream one terminal event with `provenance` and `digest` makes.
fn terminal_stream(provenance: &DocumentProvenance, digest: &str) -> Vec<u8> {
    let document = serde_json::json!({
        "agent_event_store_generation": GENERATION,
        "agent_operation_identifier": TERMINAL_OPERATION,
        "daemon_subscription_identifier": SUBSCRIPTION,
        "kind": "succeeded",
        "sequence": TERMINAL_SEQUENCE,
        "terminal": { "provenance": provenance, "submitted_command_digest": digest },
    });
    format!("event:job-event\nid:cursor-terminal\ndata:{document}\n\n").into_bytes()
}

/// Returns one substitution per field a terminal correlation is checked on.
fn correlation_substitutions() -> Vec<(&'static str, DocumentProvenance)> {
    let installed = installed_provenance().provenance();
    let substituted = |mutate: &dyn Fn(&mut DocumentProvenance)| {
        let mut named = installed.clone();
        mutate(&mut named);
        named
    };
    vec![
        ("format", substituted(&|named| named.format = "slingshot.agent/2".to_owned())),
        (
            "transport contract",
            substituted(&|named| {
                named.transport_contract_digest = SUBSTITUTED_DIGEST.to_owned();
            }),
        ),
        (
            "canonical byte contract",
            substituted(&|named| {
                named.canonical_json_contract_digest = SUBSTITUTED_DIGEST.to_owned();
            }),
        ),
        (
            "argument schema",
            substituted(&|named| {
                named.command_contract.argument_schema_digest = SUBSTITUTED_DIGEST.to_owned();
            }),
        ),
        (
            "result schema",
            substituted(&|named| {
                named.command_contract.result_schema_digest = SUBSTITUTED_DIGEST.to_owned();
            }),
        ),
        (
            "contract limits",
            substituted(&|named| {
                named.command_contract.command_contract_limits_digest =
                    SUBSTITUTED_DIGEST.to_owned();
            }),
        ),
        (
            "semantic version",
            substituted(&|named| {
                named.command_contract.command_semantic_contract_version = "second".to_owned();
            }),
        ),
        (
            "wire name",
            substituted(&|named| {
                named.command_contract.command_wire_name = "create_page".to_owned();
            }),
        ),
    ]
}

#[test]
fn every_valid_stream_decodes_the_same_however_the_bytes_are_split() {
    let manifest = expectations();
    for stream in manifest["streams"].as_array().expect("streams are a list") {
        let name = stream["name"].as_str().expect("a name");
        let bytes = stream_bytes(stream["file"].as_str().expect("a file"));
        let whole = decode_in_chunks(&bytes, bytes.len().max(1))
            .unwrap_or_else(|refusal| panic!("{name}: {refusal}"));
        for chunk in CHUNK_SIZES {
            let split = decode_in_chunks(&bytes, *chunk)
                .unwrap_or_else(|refusal| panic!("{name} at {chunk}: {refusal}"));
            assert_eq!(
                split, whole,
                "{name}: a decoder whose answer moves with the packet boundaries is not \
                 answering about the stream"
            );
        }
    }
}

#[test]
fn every_valid_stream_produces_exactly_the_items_its_manifest_names() {
    let manifest = expectations();
    for stream in manifest["streams"].as_array().expect("streams are a list") {
        let name = stream["name"].as_str().expect("a name");
        let bytes = stream_bytes(stream["file"].as_str().expect("a file"));
        let mut decoder = attached();
        let items = decoder.push(&bytes).unwrap_or_else(|refusal| panic!("{name}: {refusal}"));
        let heartbeats = items.iter().filter(|item| matches!(item, StreamItem::Heartbeat)).count();
        assert_eq!(
            heartbeats as u64,
            stream["heartbeats"].as_u64().expect("a count"),
            "{name}: a comment says the connection is alive and nothing else"
        );
        let decoded: Vec<&StreamItem> =
            items.iter().filter(|item| !matches!(item, StreamItem::Heartbeat)).collect();
        let named = stream["events"].as_array().expect("events are a list");
        assert_eq!(decoded.len(), named.len(), "{name}: one blank line, one event");
        for (item, expected) in decoded.iter().zip(named) {
            let StreamItem::Event(decoded) = item else {
                panic!("{name}: a heartbeat was filtered out already")
            };
            let (cursor, event, event_name, terminal) =
                (&decoded.cursor, &decoded.event, &decoded.name, &decoded.terminal);
            assert_eq!(event.agent_operation_identifier, expected["operation"].as_str().unwrap());
            assert_eq!(event.sequence, expected["sequence"].as_u64().expect("a sequence"));
            assert_eq!(event.kind, kind_named(expected["kind"].as_str().expect("a kind")));
            assert_eq!(event.agent_event_store_generation, GENERATION);
            assert_eq!(event_name, expected["name"].as_str().expect("a name"));
            assert_eq!(terminal, &None, "{name}: nothing here ends anything");
            assert_eq!(
                cursor.as_ref().map(EventStreamCursor::as_text),
                expected["cursor"].as_str(),
                "{name}: a cursor is the identifier field, present only when the agent sent one"
            );
        }
        assert_eq!(
            decoder.has_partial_event(),
            stream["partial"].as_bool().expect("an expectation"),
            "{name}: bytes without a blank line after them are half a sentence"
        );
    }
}

#[test]
fn every_refused_stream_names_its_one_closed_refusal() {
    let manifest = expectations();
    for refusal in manifest["refusals"].as_array().expect("refusals are a list") {
        let name = refusal["name"].as_str().expect("a name");
        let bytes = stream_bytes(refusal["file"].as_str().expect("a file"));
        let mut decoder = attached();
        let produced = decoder.push(&bytes).expect_err(&format!("{name} is refused"));
        assert_eq!(
            refusal_named(&produced),
            refusal["refusal"].as_str().expect("a refusal"),
            "{name}: {produced}"
        );
        assert!(
            !decoder.has_partial_event(),
            "{name}: state accumulated before a protocol error is state nobody can vouch for"
        );
    }
}

#[test]
fn data_fields_join_with_a_newline_and_the_fields_this_build_ignores_are_ignored() {
    let mut decoder = attached();
    let items = decoder
        .push(
            format!(
                "{RETRY_FIELD}:5000\nunheard-of:something\n{IDENTIFIER_FIELD}:cursor-joined\n\
                 {DATA_FIELD}:{{\"agent_event_store_generation\":{GENERATION},\n\
                 {DATA_FIELD}: \"agent_operation_identifier\":\"operation-joined\",\n\
                 {DATA_FIELD}:\"daemon_subscription_identifier\":\"{SUBSCRIPTION}\",\n\
                 {DATA_FIELD}:\"kind\":\"progress\",\"sequence\":1}}\n\n"
            )
            .as_bytes(),
        )
        .expect("the joined payload is one document");
    let StreamItem::Event(decoded) = &items[0] else {
        panic!("a data field joined across lines is still one event")
    };
    let (cursor, event) = (&decoded.cursor, &decoded.event);
    assert_eq!(event.agent_operation_identifier, "operation-joined");
    assert_eq!(
        cursor.as_ref().map(EventStreamCursor::as_text),
        Some("cursor-joined"),
        "a retry suggestion and an unknown field are skipped, not refused"
    );
}

#[test]
fn interleaved_jobs_keep_independent_sequences_and_distinct_cursors() {
    let bytes = stream_bytes("interleaved-jobs.sse");
    let mut decoder = attached();
    let items = decoder.push(&bytes).expect("interleaving is ordinary");
    let mut cursors: Vec<String> = Vec::new();
    let mut alpha: Vec<u64> = Vec::new();
    let mut beta: Vec<u64> = Vec::new();
    for item in &items {
        let StreamItem::Event(decoded) = item else { panic!("no comments here") };
        let (cursor, event) = (&decoded.cursor, &decoded.event);
        cursors.push(cursor.as_ref().expect("each carries one").as_text().to_owned());
        if event.agent_operation_identifier == "operation-alpha" {
            alpha.push(event.sequence);
        } else {
            beta.push(event.sequence);
        }
    }
    let mut distinct = cursors.clone();
    distinct.dedup();
    assert_eq!(distinct, cursors, "one subscription's cursors advance once per event");
    assert!(cursors.is_sorted(), "the stream's own order is monotonic across every job");
    assert_eq!(alpha, vec![6, 7], "one job's sequence is its own and skips nothing");
    assert_eq!(beta, vec![1, 2], "another job's sequence starts where that job starts");
}

#[test]
fn each_named_bound_admits_its_exact_value_and_refuses_one_byte_past_it() {
    let bounds = DecoderBounds::embedded();
    let comment = format!(":{}\n", "c".repeat(bounds.line_bytes as usize - 1));
    assert_eq!(attached().push(comment.as_bytes()).expect("exactly one line").len(), 1);
    let overlong = format!(":{}\n", "c".repeat(bounds.line_bytes as usize));
    assert!(matches!(attached().push(overlong.as_bytes()), Err(StreamRefusal::LineTooLong { .. })));

    let identifier = format!("id:{}\n", "i".repeat(bounds.identifier_bytes as usize));
    assert!(attached().push(identifier.as_bytes()).is_ok());
    let overlong = format!("id:{}\n", "i".repeat(bounds.identifier_bytes as usize + 1));
    assert!(matches!(
        attached().push(overlong.as_bytes()),
        Err(StreamRefusal::IdentifierTooLong { .. })
    ));

    let exact = padded_event(bounds, 0);
    assert_eq!(attached().push(exact.as_bytes()).expect("exactly one event").len(), 1);
    let beyond = padded_event(bounds, 1);
    assert!(
        matches!(attached().push(beyond.as_bytes()), Err(StreamRefusal::EventTooLarge { .. })),
        "a bound applied to a buffer that is already full is a bound on nothing"
    );
}

/// Returns one event whose field lines come to the event bound plus `surplus`.
fn padded_event(bounds: DecoderBounds, surplus: usize) -> String {
    let document = serde_json::json!({
        "agent_event_store_generation": GENERATION,
        "agent_operation_identifier": "operation-padded",
        "daemon_subscription_identifier": SUBSCRIPTION,
        "kind": "progress",
        "sequence": 1,
    })
    .to_string();
    let line_bytes = bounds.line_bytes as usize;
    let prefix = format!("{DATA_FIELD}:");
    let room = line_bytes - prefix.len();
    let lines = bounds.event_bytes as usize / line_bytes;
    let mut stream = format!("{prefix}{document}{}\n", " ".repeat(room - document.len()));
    for _ in 1..lines {
        stream.push_str(&format!("{prefix}{}\n", " ".repeat(room)));
    }
    stream.push_str(&"d".repeat(surplus));
    if surplus > 0 {
        stream.push('\n');
    }
    stream.push('\n');
    stream
}

#[test]
fn only_one_event_stream_attaches_and_only_with_a_character_set_this_build_reads() {
    require_event_stream(EVENT_STREAM_MEDIA_TYPE).expect("the bare type attaches");
    require_event_stream("text/event-stream; charset=utf-8").expect("an explicit UTF-8 attaches");
    require_event_stream("Text/Event-Stream; Charset=UTF-8").expect("case decides nothing");
    for named in REFUSED_MEDIA_TYPES {
        assert!(
            matches!(require_event_stream(named), Err(StreamRefusal::MediaType { .. })),
            "{named}: a server that has not decided what it is sending is not decided for"
        );
    }
    for named in REFUSED_MEDIA_PARAMETERS {
        assert!(matches!(require_event_stream(named), Err(StreamRefusal::MediaParameters { .. })));
    }
}

#[test]
fn a_head_the_shared_policy_refuses_never_becomes_a_stream() {
    let mut informational = clean_head();
    informational.informational = true;
    let mut declared = clean_head();
    declared.trailers_declared = true;
    let mut unspoken = clean_head();
    unspoken.protocol_version = UNSPOKEN_VERSION.to_owned();
    let mut redirected = clean_head();
    redirected.location = Some(REDIRECT_TARGET.to_owned());
    let mut coded = clean_head();
    coded.content_coding = Some(UNEXPECTED_CODING.to_owned());
    let mut migrating = clean_head();
    migrating.alternative_service_offered = true;
    for head in [informational, declared, unspoken, redirected, coded, migrating] {
        assert!(
            matches!(
                ServerSentEventDecoder::attached(
                    &head,
                    EVENT_STREAM_MEDIA_TYPE,
                    DecoderBounds::embedded(),
                    expectation()
                ),
                Err(StreamRefusal::Head(_))
            ),
            "a stream refused after decoding is a stream already paid for"
        );
    }
}

#[test]
fn an_undeclared_trailer_is_a_lost_connection_and_never_a_cursor_fact() {
    assert_eq!(
        ServerSentEventDecoder::undeclared_trailer(),
        StreamRefusal::UndeclaredTrailer,
        "letting the framing layer write a cursor would let it decide where a stream resumes"
    );
}

#[test]
fn a_terminal_event_is_exposed_only_when_its_whole_correlation_authenticates() {
    let installed = installed_provenance().provenance();
    let mut decoder = attached();
    let items = decoder
        .push(&terminal_stream(&installed, SUBMITTED_DIGEST))
        .expect("a fully correlated ending is an ending");
    let StreamItem::Event(decoded) = &items[0] else { panic!("one event") };
    let (event, terminal) = (&decoded.event, &decoded.terminal);
    assert_eq!(event.kind, JobEventKind::Succeeded);
    assert_eq!(event.sequence, TERMINAL_SEQUENCE);
    let correlation = terminal.as_ref().expect("an ending carries its correlation");
    assert_eq!(correlation.submitted_command_digest, SUBMITTED_DIGEST);
    assert_eq!(correlation.provenance, installed);
    assert_eq!(correlation.provenance.format, AGENT_FORMAT);
}

#[test]
fn a_terminal_event_correlating_to_anything_else_is_refused_one_field_at_a_time() {
    for (label, substituted) in correlation_substitutions() {
        let produced = attached()
            .push(&terminal_stream(&substituted, SUBMITTED_DIGEST))
            .expect_err("a substituted correlation authenticates nothing");
        assert!(
            matches!(produced, StreamRefusal::Provenance(_)),
            "{label}: an ending naming another contract ends another submission, and got {produced}"
        );
    }
    let installed = installed_provenance().provenance();
    assert!(matches!(
        attached().push(&terminal_stream(&installed, SUBSTITUTED_DIGEST)),
        Err(StreamRefusal::AnotherSubmission)
    ));
    let identity: WireContractIdentity =
        (&SelectedCommandContractIdentity::installed(COMMAND).expect("published")).into();
    assert_eq!(
        installed.command_contract, identity,
        "the correlation is checked against all five fields, not a subset of them"
    );
}

#[test]
fn a_correlation_on_an_event_that_ends_nothing_correlates_nothing() {
    let document = serde_json::json!({
        "agent_event_store_generation": GENERATION,
        "agent_operation_identifier": TERMINAL_OPERATION,
        "daemon_subscription_identifier": SUBSCRIPTION,
        "kind": "progress",
        "sequence": 1,
        "terminal": {
            "provenance": installed_provenance().provenance(),
            "submitted_command_digest": SUBMITTED_DIGEST,
        },
    });
    let stream = format!("event:job-event\ndata:{document}\n\n");
    assert!(matches!(
        attached().push(stream.as_bytes()),
        Err(StreamRefusal::CorrelationOnNonTerminal)
    ));
}

#[test]
fn a_refusal_is_stable_and_says_nothing_a_remote_server_wrote() {
    let bytes = stream_bytes("malformed-data.sse");
    let first = attached().push(&bytes).expect_err("malformed stays malformed");
    let second = attached().push(&bytes).expect_err("malformed stays malformed");
    assert_eq!(first, second, "a stable refusal is one a caller can act on");
    let rendered = format!("{first}");
    assert!(
        !rendered.contains("not a document"),
        "a bounded error names what could not be read, not what was sent: {rendered}"
    );
    let refusals = [
        StreamRefusal::AnotherSubscription,
        StreamRefusal::AnotherGeneration { expected: GENERATION, named: GENERATION + 1 },
        StreamRefusal::TerminalWithoutCorrelation,
        StreamRefusal::AnotherSubmission,
        StreamRefusal::UndeclaredTrailer,
    ];
    for refusal in refusals {
        assert!(!format!("{refusal}").is_empty(), "every refusal says why");
    }
    assert!(matches!(
        installed_provenance().require_matching(&DocumentProvenance {
            format: "slingshot.agent/2".to_owned(),
            ..installed_provenance().provenance()
        }),
        Err(WireRefusal::FormatDrift { .. })
    ));
}

#[test]
fn a_stream_that_ends_mid_sentence_emits_nothing_it_did_not_receive() {
    let bytes = stream_bytes("missing-final-blank-line.sse");
    let mut decoder = attached();
    assert!(decoder.push(&bytes).expect("the bytes are well formed").is_empty());
    assert!(decoder.has_partial_event(), "an event is what arrives before a blank line");
    assert!(decoder.push(b"\n").expect("the blank line completes it").len() == 1);
    assert!(!decoder.has_partial_event());
}
