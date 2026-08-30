//! The versioned vocabulary, and the two separations it exists to keep.
//!
//! Bytes travel only in chunk responses. The test walks every response variant
//! and asserts that exactly two carry content, so a caller reading a large
//! artifact does it deliberately rather than by polling a status.
//!
//! Maintenance results are keyed by target and identifier alone. Their requests
//! are offered an operation identifier, an expected digest, a byte offset, and
//! an artifact slot, and each is refused - because a maintenance result outlives
//! the operation that produced it, and a reader that needed the operation could
//! not find it afterwards.
//!
//! Evidence is a union rather than an optional field. A proven remote success
//! that also carried a certainty would be claiming two incompatible things, and
//! the fixture offers exactly that shape to prove it does not decode.

use serde_json::Value;
use slingshot_local_protocol::message::{
    MessageFailure, OperationEnvelope, OperationRequest, OperationResponse,
    TerminalFailureDisposition, digest_is_canonical, write_payload,
};

/// Request vectors this test reads.
const REQUESTS: &str = include_str!("fixtures/messages/requests.jsonl");

/// Shape vectors this test reads.
const SHAPES: &str = include_str!("fixtures/messages/shapes.jsonl");

/// Response vectors this test reads.
const RESPONSES: &str = include_str!("fixtures/messages/responses.jsonl");

/// Evidence vectors this test reads.
const EVIDENCE: &str = include_str!("fixtures/messages/evidence.jsonl");

/// Terminal-failure vectors this test reads.
const TERMINALS: &str = include_str!("fixtures/messages/terminals.jsonl");

/// Characters a rendered digest occupies.
const DIGEST_CHARACTERS: usize = 64;

/// Reads one row's string member.
fn text<'row>(row: &'row Value, member: &str) -> &'row str {
    row[member].as_str().unwrap_or_else(|| panic!("{member} is a string in {row}"))
}

/// Returns every row of one fixture.
fn rows(fixture: &str) -> Vec<Value> {
    fixture
        .lines()
        .map(|line| serde_json::from_str(line).expect("every fixture line is one object"))
        .collect()
}

/// Returns whether one document is exactly what the type it names writes.
///
/// Decoding alone is not enough. Serde's internally tagged enums accept members
/// they do not know rather than refusing them, so a document carrying a surplus
/// field decodes and then writes back without it. Requiring the bytes to
/// survive the round trip is what closes that: a document is accepted when the
/// type reproduces it exactly, and a surplus member makes it fail.
fn accepts<Parsed: serde::de::DeserializeOwned + serde::Serialize>(document: &str) -> bool {
    serde_json::from_str::<Parsed>(document)
        .is_ok_and(|parsed| write_payload(&parsed).is_ok_and(|written| written == document))
}

/// Checks one accept-or-refuse vector against the type it names.
fn check<Parsed: serde::de::DeserializeOwned + serde::Serialize + std::fmt::Debug>(row: &Value) {
    let document = text(row, "document");
    let note = text(row, "note");
    assert_eq!(
        accepts::<Parsed>(document),
        row["accepted"].as_bool().expect("a verdict"),
        "{note}"
    );
}

#[test]
fn every_request_vector_round_trips_to_its_own_bytes() {
    let vectors = rows(REQUESTS);
    assert!(vectors.len() >= 21, "all eleven requests and every refused shape");
    for row in &vectors {
        check::<OperationEnvelope>(row);
    }
}

#[test]
fn every_declared_request_has_an_accepted_vector() {
    let accepted: Vec<OperationEnvelope> = rows(REQUESTS)
        .iter()
        .filter(|row| row["accepted"] == Value::Bool(true))
        .map(|row| serde_json::from_str(text(row, "document")).expect("accepted"))
        .collect();
    let mut named: Vec<String> = accepted
        .iter()
        .map(|envelope| {
            serde_json::to_value(&envelope.request).expect("a request serializes")["request"]
                .as_str()
                .expect("a request name")
                .to_owned()
        })
        .collect();
    named.sort();
    named.dedup();
    let expected = [
        "artifact_read",
        "execute",
        "list_operations",
        "maintenance_result_metadata",
        "maintenance_result_read",
        "operation_status",
        "restore_placeholder",
        "result",
        "resume_operation_recovery",
        "terminal_maintenance_apply",
        "terminal_maintenance_preview",
        "wait",
    ];
    for request in expected.iter().filter(|request| **request != "restore_placeholder") {
        assert!(named.iter().any(|held| held == request), "{request} has no accepted vector");
    }
    assert_eq!(named.len(), 11, "eleven requests, and no twelfth");
}

#[test]
fn only_operation_bearing_requests_name_an_operation() {
    for row in rows(REQUESTS).iter().filter(|row| row["accepted"] == Value::Bool(true)) {
        let envelope: OperationEnvelope =
            serde_json::from_str(text(row, "document")).expect("accepted");
        let names = envelope.request.operation_identifier().is_some();
        let maintenance = envelope.request.is_maintenance_result_request();
        assert!(
            !(names && maintenance),
            "{}: a maintenance result outlives its operation and is not keyed by it",
            text(row, "note")
        );
        if maintenance {
            assert!(
                !text(row, "document").contains("operation_identifier"),
                "{}: nothing in the envelope names an operation",
                text(row, "note")
            );
        }
    }
}

#[test]
fn every_shape_vector_is_judged_the_way_the_fixture_says() {
    for row in &rows(SHAPES) {
        let envelope: OperationEnvelope =
            serde_json::from_str(text(row, "document")).expect("the shape itself decodes");
        let outcome = envelope.require_well_formed();
        let note = text(row, "note");
        match (row["accepted"].as_bool(), &outcome) {
            (Some(true), Ok(())) => (),
            (Some(false), Err(failure)) => {
                let named = match failure {
                    MessageFailure::DigestNotCanonical => "DigestNotCanonical",
                    MessageFailure::IdentifierOutOfBounds => "IdentifierOutOfBounds",
                    MessageFailure::RevisionAbsent => "RevisionAbsent",
                    MessageFailure::DispositionInconsistent => "DispositionInconsistent",
                    MessageFailure::ChunkTooLarge => "ChunkTooLarge",
                };
                assert_eq!(named, text(row, "reason"), "{note}");
            }
            _ => panic!("{note}: answered {outcome:?}"),
        }
    }
    assert!(digest_is_canonical(&"a".repeat(DIGEST_CHARACTERS)));
    assert!(!digest_is_canonical(&"A".repeat(DIGEST_CHARACTERS)));
}

#[test]
fn every_response_vector_round_trips_to_its_own_bytes() {
    let vectors = rows(RESPONSES);
    assert!(vectors.len() >= 34, "every response variant and the shapes they refuse");
    for row in &vectors {
        check::<OperationResponse>(row);
    }
}

#[test]
fn exactly_two_responses_carry_bytes() {
    let carriers: Vec<String> = rows(RESPONSES)
        .iter()
        .filter(|row| row["accepted"] == Value::Bool(true))
        .filter_map(|row| {
            let response: OperationResponse =
                serde_json::from_str(text(row, "document")).expect("accepted");
            response.carries_bytes().then(|| text(row, "note").to_owned())
        })
        .collect();
    assert_eq!(carriers, vec!["an artifact chunk", "a maintenance chunk"]);
    for row in rows(RESPONSES).iter().filter(|row| row["accepted"] == Value::Bool(true)) {
        let document = text(row, "document");
        let response: OperationResponse = serde_json::from_str(document).expect("accepted");
        if !response.carries_bytes() {
            assert!(
                !document.contains("encoded_bytes"),
                "{}: bytes travel only in chunks",
                text(row, "note")
            );
        }
    }
}

#[test]
fn maintenance_metadata_describes_without_naming_a_place_to_read_it_from() {
    let described = rows(RESPONSES)
        .into_iter()
        .find(|row| text(row, "note") == "maintenance metadata")
        .expect("one metadata vector");
    let document = text(&described, "document");
    for absent in ["path", "operation_identifier", "artifact_slot", "encoded_bytes"] {
        assert!(!document.contains(absent), "maintenance metadata carries no {absent}");
    }
    let starting = rows(RESPONSES)
        .into_iter()
        .find(|row| text(row, "note") == "a maintenance result starting")
        .expect("one start vector");
    let start: OperationResponse =
        serde_json::from_str(text(&starting, "document")).expect("accepted");
    let metadata: OperationResponse = serde_json::from_str(document).expect("accepted");
    let described_by = |response: &OperationResponse| match response {
        OperationResponse::MaintenanceResultMetadata { description }
        | OperationResponse::MaintenanceResultStart { description } => description.clone(),
        other => panic!("not a maintenance description: {other:?}"),
    };
    assert_eq!(
        described_by(&metadata),
        described_by(&start),
        "the metadata answer and the transfer's first response describe the same thing"
    );
}

#[test]
fn evidence_says_one_thing_about_execution_or_the_other() {
    for row in &rows(EVIDENCE) {
        let document = text(row, "document");
        let note = text(row, "note");
        assert_eq!(
            accepts::<OperationResponse>(document),
            row["accepted"].as_bool().expect("a verdict"),
            "{note}"
        );
    }
}

#[test]
fn a_terminal_failure_carries_only_the_certainty_its_disposition_allows() {
    let vectors = rows(TERMINALS);
    assert!(vectors.len() >= 7, "every kind, and both ways each disposition can be wrong");
    for row in &vectors {
        let response: OperationResponse =
            serde_json::from_str(text(row, "document")).expect("the shape itself decodes");
        let disposition = match response {
            OperationResponse::TerminalFailure { disposition, .. } => disposition,
            other => panic!("not a terminal failure: {other:?}"),
        };
        assert_eq!(
            disposition.is_consistent(),
            row["consistent"].as_bool().expect("a verdict"),
            "{}",
            text(row, "note")
        );
    }
    assert!(TerminalFailureDisposition::AuthoritativeRemoteFailure.is_consistent());
    assert!(TerminalFailureDisposition::AuthoritativeRemoteSuccess.is_consistent());
}

#[test]
fn no_envelope_field_carries_a_readable_principal() {
    for row in rows(REQUESTS).iter().filter(|row| row["accepted"] == Value::Bool(true)) {
        let document = text(row, "document");
        for readable in ["user_name", "password", "organization", "client_secret", "@"] {
            assert!(
                !document.contains(readable),
                "{}: an envelope crosses a socket anything can reach, and carries no {readable}",
                text(row, "note")
            );
        }
    }
}

#[test]
fn identical_typed_messages_always_write_identical_bytes() {
    for row in rows(REQUESTS).iter().filter(|row| row["accepted"] == Value::Bool(true)) {
        let envelope: OperationEnvelope =
            serde_json::from_str(text(row, "document")).expect("accepted");
        let once = write_payload(&envelope).expect("it writes");
        let again = write_payload(&envelope.clone()).expect("it writes");
        assert_eq!(once, again, "{}: two writes of one message differ", text(row, "note"));
    }
    let request = OperationRequest::Wait { operation_identifier: "operation-1".to_owned() };
    assert_eq!(
        write_payload(&request).expect("it writes"),
        write_payload(&request.clone()).expect("it writes")
    );
}
