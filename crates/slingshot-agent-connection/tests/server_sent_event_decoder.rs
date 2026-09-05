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

#[test]
fn decoded_events_preserve_missing_counters_without_accepting_explicit_regression() {
    use slingshot_agent_connection::job_event_reducer::{
        AssociationBinding, JobDisposition, ReducerRefusal, RetainedJob, reduce_decoded,
    };
    use slingshot_domain::remote_job::{AgentJobState, JobEventSequence, RemoteJobObservation};
    let held = RetainedJob {
        observation: RemoteJobObservation {
            applied_sequence: JobEventSequence::of(3), attempt: 2, progress: 40,
            state: AgentJobState::Running,
        },
        snapshot_watermark: JobEventSequence::of(1),
    };
    let binding = AssociationBinding {
        expected_provenance: installed_provenance(),
        selected_environment_revision: "retained-revision".into(),
        submitted_command_digest: SUBMITTED_DIGEST.into(),
    };
    for (attempt, progress) in [(None, None), (Some(3), None), (None, Some(41)), (Some(0), None), (None, Some(0))] {
        for sequence in [2, 3, 4, 5] {
            let mut document = serde_json::json!({
                "agent_event_store_generation": GENERATION,
                "agent_operation_identifier": TERMINAL_OPERATION,
                "daemon_subscription_identifier": SUBSCRIPTION,
                "sling_job_identifier": "job-fixture", "kind": "progress",
                "state": "running", "sequence": sequence
            });
            if let Some(attempt) = attempt { document["attempt"] = attempt.into(); }
            if let Some(progress) = progress { document["progress"] = progress.into(); }
            let bytes = format!("id:cursor\ndata:{document}\n\n");
            let items = decode_in_chunks(bytes.as_bytes(), 1).unwrap();
            let StreamItem::Event(event) = &items[0] else { panic!("event missing") };
            for (generation, operation) in [(GENERATION + 1, TERMINAL_OPERATION), (GENERATION, "wrong-operation")] {
                assert_eq!(reduce_decoded(&held, &binding, generation, operation, event), Err(ReducerRefusal::AnotherJob));
            }
            let result = reduce_decoded(&held, &binding, GENERATION, TERMINAL_OPERATION, event);
            match sequence {
                2 => assert_eq!(result.unwrap(), (JobDisposition::StaleCursorOnly, None)),
                3 => assert_eq!(result.unwrap(), (if attempt.is_none() && progress.is_none() {
                    JobDisposition::ExactReplay
                } else { JobDisposition::IntegrityConflictNeedsReconciliation }, None)),
                4 if attempt == Some(0) || progress == Some(0) => assert!(matches!(result, Err(ReducerRefusal::Job(_)))),
                4 => {
                    let (disposition, observation) = result.unwrap();
                    assert_eq!(disposition, JobDisposition::Applied);
                    let observation = observation.unwrap();
                    assert_eq!(observation.attempt, attempt.unwrap_or(2));
                    assert_eq!(observation.progress, progress.unwrap_or(40));
                    assert_eq!(observation.applied_sequence, JobEventSequence::of(4));
                }
                5 => assert_eq!(result.unwrap(), (JobDisposition::NeedsSnapshot, None)),
                _ => unreachable!(),
            }
        }
    }
}

#[test]
fn canonical_event_identity_covers_the_envelope_and_preserves_omission() {
    use slingshot_domain::command::canonical_json::{canonical_digest, write_canonical};
    let base = serde_json::json!({
        "agent_event_store_generation": GENERATION,
        "agent_operation_identifier": TERMINAL_OPERATION,
        "daemon_subscription_identifier": SUBSCRIPTION,
        "sling_job_identifier": "job-é", "kind": "progress",
        "state": "running", "sequence": 3
    });
    let decode = |payload: &str, framing: &str| {
        let bytes = format!("{framing}data:{payload}\n\n");
        let mut items = decode_in_chunks(bytes.as_bytes(), 1).unwrap();
        let StreamItem::Event(event) = items.remove(0) else { panic!("event missing") };
        event
    };
    let first = decode(&base.to_string(), "id:first\nevent:job-event\n");
    let canonical = write_canonical(&base).unwrap();
    assert_eq!(first.canonical_digest, canonical_digest(&canonical));
    assert_eq!(first.canonical_bytes, canonical.len() as u64);
    // Reverse member order and add insignificant JSON whitespace. SSE cursor
    // and event-name framing are not part of the event document's identity.
    let reversed = base.as_object().unwrap().iter().rev()
        .map(|(key, value)| format!("{} : {value}", serde_json::to_string(key).unwrap()))
        .collect::<Vec<_>>().join(", ");
    let equivalent = decode(&format!("{{ {reversed} }}"), "id:second\nevent:other-name\n");
    assert_eq!(first.canonical_digest, equivalent.canonical_digest);
    assert_eq!(first.canonical_bytes, equivalent.canonical_bytes);
    for (field, value) in [
        ("attempt", serde_json::json!(0)),
        ("progress", serde_json::json!(0)),
        ("sling_job_identifier", serde_json::json!("other-job")),
        ("sequence", serde_json::json!(4)),
        ("agent_operation_identifier", serde_json::json!("b".repeat(64))),
    ] {
        let mut changed = base.clone();
        changed[field] = value;
        let event = decode(&changed.to_string(), "id:first\n");
        assert_ne!(first.canonical_digest, event.canonical_digest, "{field}");
        assert_eq!(event.canonical_bytes, write_canonical(&changed).unwrap().len() as u64);
    }
}

#[test]
fn incremental_delivery_preserves_valid_items_before_a_later_failure_at_every_split() {
    let bytes = b": valid heartbeat\n\ndata: not-json\n\n";
    for split in 0..=bytes.len() {
        let mut decoder = attached();
        let mut delivered = Vec::new();
        let mut consume = |item| {
            delivered.push(item);
            Ok(())
        };
        let first = decoder.push_each(&bytes[..split], &mut consume);
        let result =
            if first.is_ok() { decoder.push_each(&bytes[split..], &mut consume) } else { first };
        assert!(result.is_err());
        assert_eq!(delivered, [StreamItem::Heartbeat]);
        assert!(!decoder.has_partial_event());
        assert_eq!(
            decoder.push_each(b": later\n", |_| panic!("closed decoder delivered")),
            Err(StreamRefusal::Closed)
        );
    }
}

#[test]
fn physical_event_state_and_optional_counters_are_validated_and_preserved() {
    let base = serde_json::json!({"agent_event_store_generation":GENERATION,
        "agent_operation_identifier":TERMINAL_OPERATION, "daemon_subscription_identifier":SUBSCRIPTION,
        "sling_job_identifier":"job-fixture", "state":"running", "kind":"progress", "sequence":1});
    let decode = |document: &serde_json::Value| attached().push(format!("data:{document}\n\n").as_bytes());
    for (kind, expected) in [("accepted", "queued"), ("started", "running"), ("progress", "running"), ("succeeded", "succeeded"), ("failed", "failed")] {
        for state in ["queued", "running", "succeeded", "failed"] {
            let mut document = base.clone(); document["kind"] = kind.into(); document["state"] = state.into();
            if ["succeeded", "failed"].contains(&kind) {
                document["terminal"] = serde_json::json!({"provenance":installed_provenance().provenance(), "submitted_command_digest":SUBMITTED_DIGEST});
            }
            assert_eq!(decode(&document).is_ok(), state == expected, "{kind}/{state}");
        }
    }
    for (physical, valid) in [("job /?é".into(), true), (String::new(), false), ("a".repeat(1024), true), ("a".repeat(1025), false), ("é".repeat(512), true), ("é".repeat(513), false)] {
        let mut document = base.clone(); document["sling_job_identifier"] = physical.clone().into();
        let result = decode(&document);
        assert_eq!(result.is_ok(), valid);
        if valid { let StreamItem::Event(event) = &result.unwrap()[0] else {panic!("event missing");}; assert_eq!(event.sling_job_identifier, physical); }
    }
    for field in ["sling_job_identifier", "state"] {
        let mut document = base.clone(); document.as_object_mut().unwrap().remove(field);
        assert!(decode(&document).is_err());
    }
    for counter in [None, Some(0), Some(17), Some(u64::MAX)] {
        let mut document = base.clone();
        if let Some(counter) = counter { document["attempt"] = counter.into(); document["progress"] = counter.into(); }
        let items = decode(&document).unwrap(); let StreamItem::Event(event) = &items[0] else {panic!("event missing");};
        assert_eq!(event.attempt, counter); assert_eq!(event.progress, counter);
    }
    for field in ["attempt", "progress"] {
        for value in [serde_json::Value::Null, serde_json::json!(-1), serde_json::json!(1.5), serde_json::json!("1"), serde_json::json!(true)] {
            let mut document = base.clone(); document[field] = value; assert!(decode(&document).is_err());
        }
    }
}

#[test]
fn event_operation_identifiers_obey_the_closed_wire_grammar() {
    let valid = "a".repeat(64);
    for identifier in [String::new(), "operation-placeholder".into(), "a".repeat(63), "a".repeat(65),
        "A".repeat(64), "g".repeat(64), format!(" {}", "a".repeat(63)), "é".repeat(32),
        valid.clone(), "0".repeat(64), "f".repeat(64)] {
        let mut decoder = attached();
        let document = serde_json::json!({"agent_event_store_generation":GENERATION,
            "agent_operation_identifier":identifier, "daemon_subscription_identifier":SUBSCRIPTION,
            "kind":"progress", "sequence":1, "sling_job_identifier":"job-fixture", "state":"running"});
        let mut delivered = Vec::new();
        let result = decoder.push_each(format!(": prefix\ndata:{document}\n\n").as_bytes(), |item| {delivered.push(item); Ok(())});
        let acceptable = identifier.len() == 64 && identifier.bytes().all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte));
        if acceptable {
            assert!(result.is_ok()); assert_eq!(delivered.len(), 2);
        } else {
            assert_eq!(result, Err(StreamRefusal::Malformed {field:"agent_operation_identifier"}));
            assert_eq!(delivered.len(), 1, "only the prior heartbeat may escape");
            assert_eq!(decoder.push(b": later\n"), Err(StreamRefusal::Closed));
        }
    }
}

#[test]
fn event_and_terminal_debug_views_do_not_expose_wire_identity() {
    let items = attached().push(&terminal_stream(&installed_provenance().provenance(), SUBMITTED_DIGEST)).unwrap();
    let StreamItem::Event(event) = &items[0] else { panic!("expected terminal event"); };
    assert_eq!(format!("{event:?}"), "DecodedEvent([redacted])");
    assert_eq!(format!("{:?}", event.terminal.as_ref().unwrap()), "TerminalCorrelation([redacted])");
}

#[test]
fn explicitly_null_terminal_data_is_not_an_absent_member() {
    for kind in ["progress", "succeeded"] {
        let document = serde_json::json!({"agent_event_store_generation":GENERATION,
            "agent_operation_identifier":TERMINAL_OPERATION, "daemon_subscription_identifier":SUBSCRIPTION,
            "kind":kind, "sequence":1, "terminal":null, "sling_job_identifier":"job-fixture", "state":if kind == "progress" {"running"} else {"succeeded"}});
        assert_eq!(attached().push(format!("data:{document}\n\n").as_bytes()), Err(StreamRefusal::Malformed {field:"payload"}));
    }
}

#[test]
fn incremental_consumer_refusal_never_delivers_later_items_or_reopens() {
    let mut decoder = attached();
    let mut delivered = 0;
    assert_eq!(
        decoder.push_each(b": first\n: refused\n: never\n", |_| {
            delivered += 1;
            if delivered == 2 { Err(StreamRefusal::Consumer) } else { Ok(()) }
        }),
        Err(StreamRefusal::Consumer)
    );
    assert_eq!(delivered, 2);
    assert_eq!(decoder.push(b": never\n"), Err(StreamRefusal::Closed));
}

#[test]
fn incremental_delivery_does_not_collect_a_chunk_sized_batch() {
    let mut decoder = attached();
    let chunk = b": heartbeat\n".repeat(100_000);
    let mut delivered = 0;
    decoder
        .push_each(&chunk, |item| {
            assert_eq!(item, StreamItem::Heartbeat);
            delivered += 1;
            Ok(())
        })
        .unwrap();
    assert_eq!(delivered, 100_000);
    assert!(!decoder.has_partial_event());
}

#[test]
fn a_caught_consumer_panic_cannot_reopen_the_decoder_or_expose_its_buffers() {
    let mut decoder = attached();
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let _ = decoder.push_each(b": heartbeat\n", |_| panic!("consumer stopped"));
    }));
    assert!(result.is_err());
    assert_eq!(decoder.push(b": later\n"), Err(StreamRefusal::Closed));
    assert_eq!(format!("{decoder:?}"), "ServerSentEventDecoder([redacted])");
}

#[test]
fn subscription_terminals_resolve_each_operations_own_contract_and_digest() {
    use slingshot_agent_connection::server_sent_event_decoder::OperationStreamExpectation;
    let first = installed_provenance();
    let second = ExpectedProvenance {
        command_contract: SelectedCommandContractIdentity::installed("create_asset").unwrap(),
        ..installed_provenance()
    };
    let first_bytes = terminal_stream(&first.provenance(), SUBMITTED_DIGEST);
    let second_bytes = String::from_utf8(terminal_stream(&second.provenance(), SUBSTITUTED_DIGEST))
        .unwrap()
        .replace(TERMINAL_OPERATION, "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb")
        .replace("cursor-terminal", "cursor-second")
        .into_bytes();
    let bytes = [first_bytes, second_bytes].concat();
    for chunk in CHUNK_SIZES {
        let mut resolved = Vec::new();
        let resolver = |operation: &str| {
            resolved.push(operation.to_owned());
            let (provenance, digest) = match operation {
                TERMINAL_OPERATION => (first.clone(), SUBMITTED_DIGEST),
                "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb" => (second.clone(), SUBSTITUTED_DIGEST),
                _ => return Err(StreamRefusal::AnotherSubmission),
            };
            Ok(OperationStreamExpectation {
                agent_operation_identifier: operation.to_owned(),
                daemon_subscription_identifier: SUBSCRIPTION.to_owned(),
                agent_event_store_generation: GENERATION,
                expected_provenance: provenance,
                submitted_command_digest: digest.to_owned(),
            })
        };
        let mut decoder = ServerSentEventDecoder::attached_subscription(
            &clean_head(),
            EVENT_STREAM_MEDIA_TYPE,
            DecoderBounds::embedded(),
            SUBSCRIPTION.to_owned(),
            GENERATION,
            resolver,
        )
        .unwrap();
        let mut delivered = Vec::new();
        for part in bytes.chunks(*chunk) {
            decoder
                .push_each(part, |item| {
                    delivered.push(item);
                    Ok(())
                })
                .unwrap();
        }
        assert_eq!(delivered.len(), 2);
        drop(decoder);
        assert_eq!(resolved, [TERMINAL_OPERATION, "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb"]);
    }
}

#[test]
fn wrong_retained_key_digest_contract_or_resolution_failure_prevents_delivery() {
    use slingshot_agent_connection::server_sent_event_decoder::OperationStreamExpectation;
    for defect in
        ["key", "digest", "contract", "missing", "generation", "subscription", "malformed_digest"]
    {
        let resolver = |operation: &str| {
            if defect == "missing" {
                return Err(StreamRefusal::AnotherSubmission);
            }
            let mut provenance = installed_provenance();
            if defect == "contract" {
                provenance.transport_contract_digest = SUBSTITUTED_DIGEST.to_owned();
            }
            Ok(OperationStreamExpectation {
                daemon_subscription_identifier: if defect == "subscription" {
                    "other"
                } else {
                    SUBSCRIPTION
                }
                .to_owned(),
                agent_event_store_generation: if defect == "generation" {
                    GENERATION + 1
                } else {
                    GENERATION
                },
                agent_operation_identifier: if defect == "key" {
                    "another".to_owned()
                } else {
                    operation.to_owned()
                },
                expected_provenance: provenance,
                submitted_command_digest: if defect == "digest" {
                    SUBSTITUTED_DIGEST
                } else if defect == "malformed_digest" {
                    "not a digest"
                } else {
                    SUBMITTED_DIGEST
                }
                .to_owned(),
            })
        };
        let mut decoder = ServerSentEventDecoder::attached_subscription(
            &clean_head(),
            EVENT_STREAM_MEDIA_TYPE,
            DecoderBounds::embedded(),
            SUBSCRIPTION.to_owned(),
            GENERATION,
            resolver,
        )
        .unwrap();
        assert!(
            decoder
                .push_each(
                    &terminal_stream(&installed_provenance().provenance(), SUBMITTED_DIGEST),
                    |_| panic!("uncorrelated terminal was delivered")
                )
                .is_err()
        );
        assert_eq!(decoder.push(b": later\n"), Err(StreamRefusal::Closed));
    }
}

#[test]
fn subscription_and_generation_are_checked_before_retained_operation_resolution() {
    use slingshot_agent_connection::server_sent_event_decoder::OperationStreamExpectation;
    for bytes in [
        String::from_utf8(terminal_stream(&installed_provenance().provenance(), SUBMITTED_DIGEST))
            .unwrap()
            .replace(SUBSCRIPTION, "other"),
        String::from_utf8(terminal_stream(&installed_provenance().provenance(), SUBMITTED_DIGEST))
            .unwrap()
            .replace("\"agent_event_store_generation\":7", "\"agent_event_store_generation\":8"),
    ] {
        let resolver = |_: &str| -> Result<OperationStreamExpectation, StreamRefusal> {
            panic!("wrong stream caused a storage lookup");
        };
        let mut decoder = ServerSentEventDecoder::attached_subscription(
            &clean_head(),
            EVENT_STREAM_MEDIA_TYPE,
            DecoderBounds::embedded(),
            SUBSCRIPTION.to_owned(),
            GENERATION,
            resolver,
        )
        .unwrap();
        assert!(
            decoder.push_each(bytes.as_bytes(), |_| panic!("wrong stream was delivered")).is_err()
        );
    }
}

#[test]
fn matching_remote_and_retained_drift_is_not_installed_contract_evidence() {
    use slingshot_agent_connection::server_sent_event_decoder::OperationStreamExpectation;
    for defect in ["transport", "canonical", "digest"] {
        let mut retained = installed_provenance();
        if defect == "transport" {
            retained.transport_contract_digest = SUBSTITUTED_DIGEST.to_owned();
        }
        if defect == "canonical" {
            retained.canonical_json_contract_digest = SUBSTITUTED_DIGEST.to_owned();
        }
        let digest = if defect == "digest" { "invalid" } else { SUBMITTED_DIGEST };
        let bytes = terminal_stream(&retained.provenance(), digest);
        let resolver = |operation: &str| {
            Ok(OperationStreamExpectation {
                daemon_subscription_identifier: SUBSCRIPTION.to_owned(),
                agent_event_store_generation: GENERATION,
                agent_operation_identifier: operation.to_owned(),
                expected_provenance: retained.clone(),
                submitted_command_digest: digest.to_owned(),
            })
        };
        let mut decoder = ServerSentEventDecoder::attached_subscription(
            &clean_head(),
            EVENT_STREAM_MEDIA_TYPE,
            DecoderBounds::embedded(),
            SUBSCRIPTION.to_owned(),
            GENERATION,
            resolver,
        )
        .unwrap();
        assert!(
            decoder
                .push_each(&bytes, |_| panic!("matching invalid records were delivered"))
                .is_err()
        );
    }
}

/// The operation a terminal event ends.
const TERMINAL_OPERATION: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";

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
        "sling_job_identifier": "job-fixture", "state": "succeeded",
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
                 {DATA_FIELD}: \"agent_operation_identifier\":\"dddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddd\",\n\
                 {DATA_FIELD}:\"daemon_subscription_identifier\":\"{SUBSCRIPTION}\",\n\
                 {DATA_FIELD}:\"kind\":\"progress\",\"sequence\":1,\"sling_job_identifier\":\"job-fixture\",\"state\":\"running\"}}\n\n"
            )
            .as_bytes(),
        )
        .expect("the joined payload is one document");
    let StreamItem::Event(decoded) = &items[0] else {
        panic!("a data field joined across lines is still one event")
    };
    let (cursor, event) = (&decoded.cursor, &decoded.event);
    assert_eq!(event.agent_operation_identifier, "dddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddd");
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
        if event.agent_operation_identifier == "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa" {
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
        "agent_operation_identifier": "eeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeee",
        "daemon_subscription_identifier": SUBSCRIPTION,
        "kind": "progress",
        "sling_job_identifier": "job-fixture", "state": "running",
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
        "sling_job_identifier": "job-fixture", "state": "running",
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
