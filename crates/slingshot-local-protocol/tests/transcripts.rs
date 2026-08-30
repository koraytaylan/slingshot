//! Whole sessions, byte for byte, at every place a transport could split them.
//!
//! A transcript is worth more than a per-message test because the interesting
//! failures are between messages: a refusal that also closes a connection, a
//! stale nonce that stops the wrong instance, a replay that reports the state
//! it had rather than the state it has. Each session below is a sequence of
//! exact payloads, and the test replays each one through the frame codec at
//! every fragmentation boundary a transport could produce.
//!
//! What the transcripts are *for* is mostly negative. The incompatible sessions
//! exist to show the connection survives; the mismatch sessions exist to show
//! nothing was consumed; the hostile inputs exist to show one bad frame ends its
//! own transcript and nothing else.

use serde_json::Value;
use slingshot_local_protocol::foundation_contract::FoundationContract;
use slingshot_local_protocol::framing::{FrameReader, render};
use slingshot_local_protocol::message::{OperationEnvelope, OperationResponse, write_payload};

/// Sessions this test replays.
const SESSIONS: &str = include_str!("fixtures/transcripts/sessions.jsonl");

/// Hostile inputs this test feeds a reader.
const HOSTILE: &str = include_str!("fixtures/transcripts/hostile.jsonl");

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

/// Returns the framing limits the embedded contract declares.
fn limits() -> slingshot_local_protocol::foundation_contract::FramingLimits {
    FoundationContract::embedded().framing
}

/// Returns the bytes one hexadecimal member spells.
fn octets(row: &Value, member: &str) -> Vec<u8> {
    /// Radix the fixture writes bytes in.
    const HEXADECIMAL_RADIX: u32 = 16;
    /// Characters one byte occupies.
    const CHARACTERS_PER_OCTET: usize = 2;

    let spelling = text(row, member);
    (0..spelling.len() / CHARACTERS_PER_OCTET)
        .map(|position| {
            let pair = &spelling[position * CHARACTERS_PER_OCTET
                ..position * CHARACTERS_PER_OCTET + CHARACTERS_PER_OCTET];
            u8::from_str_radix(pair, HEXADECIMAL_RADIX).expect("a hexadecimal pair")
        })
        .collect()
}

/// Returns every payload one session sends and receives, in order.
fn payloads(session: &Value) -> Vec<String> {
    session["exchanges"]
        .as_array()
        .expect("a session lists its exchanges")
        .iter()
        .flat_map(|exchange| {
            [text(exchange, "sent").to_owned(), text(exchange, "received").to_owned()]
        })
        .filter(|payload| !payload.is_empty())
        .collect()
}

#[test]
fn every_session_survives_every_place_a_transport_could_split_it() {
    let sessions = rows(SESSIONS);
    assert!(sessions.len() >= 11, "compatible, incompatible, refused, replayed, and streamed");
    for session in &sessions {
        let name = text(session, "name");
        for payload in payloads(session) {
            let Ok(frame) = render(&limits(), payload.as_bytes()) else {
                continue;
            };
            for cut in 0..=frame.len() {
                let mut reader = FrameReader::new();
                let first = reader
                    .absorb(&limits(), &frame[..cut])
                    .unwrap_or_else(|failure| panic!("{name}: split at {cut}: {failure}"));
                let read = match first {
                    Some(payload) => payload,
                    None => reader
                        .absorb(&limits(), &frame[cut..])
                        .unwrap_or_else(|failure| panic!("{name}: split at {cut}: {failure}"))
                        .unwrap_or_else(|| panic!("{name}: split at {cut} completed nothing")),
                };
                assert_eq!(read, payload.as_bytes(), "{name}: split at {cut} changed the payload");
            }
        }
    }
}

#[test]
fn every_operation_payload_in_every_session_is_exactly_what_its_type_writes() {
    for session in &rows(SESSIONS) {
        let name = text(session, "name");
        for exchange in session["exchanges"].as_array().expect("exchanges") {
            if text(exchange, "kind") != "operation" && text(exchange, "kind") != "continued" {
                continue;
            }
            let sent = text(exchange, "sent");
            if !sent.is_empty() {
                let envelope: OperationEnvelope = serde_json::from_str(sent)
                    .unwrap_or_else(|failure| panic!("{name}: {failure}"));
                assert_eq!(write_payload(&envelope).expect("it writes"), sent, "{name}");
                assert_eq!(envelope.require_well_formed(), Ok(()), "{name}");
            }
            let received = text(exchange, "received");
            let response: OperationResponse = serde_json::from_str(received)
                .unwrap_or_else(|failure| panic!("{name}: {failure}"));
            assert_eq!(write_payload(&response).expect("it writes"), received, "{name}");
        }
    }
}

#[test]
fn an_incompatible_session_refuses_once_and_keeps_the_connection() {
    for name in ["incompatible-operation-version", "incompatible-runtime-contract"] {
        let sessions = rows(SESSIONS);
        let session = sessions
            .iter()
            .find(|session| text(session, "name") == name)
            .unwrap_or_else(|| panic!("{name} is a session"));
        let exchanges = session["exchanges"].as_array().expect("exchanges");
        let refusals = exchanges
            .iter()
            .filter(|exchange| {
                text(exchange, "received").contains("incompatible_operation_protocol")
                    || text(exchange, "received").contains("runtime_contract_digest_mismatch")
            })
            .count();
        assert_eq!(refusals, 1, "{name}: exactly one refusal");
        assert!(
            exchanges.iter().any(|exchange| text(exchange, "kind") == "control"),
            "{name}: the connection is still used afterwards"
        );
    }
    let sessions = rows(SESSIONS);
    let stopping = sessions
        .iter()
        .find(|session| text(session, "name") == "incompatible-operation-version")
        .expect("that session");
    assert!(
        stopping["exchanges"]
            .as_array()
            .expect("exchanges")
            .iter()
            .any(|exchange| text(exchange, "sent") == "daemon.stop"),
        "an incompatible client can still stop the daemon it cannot use"
    );
}

#[test]
fn a_stale_nonce_changes_nothing_and_the_live_one_still_works() {
    let sessions = rows(SESSIONS);
    let session = sessions
        .iter()
        .find(|session| text(session, "name") == "stale-nonce")
        .expect("the stale-nonce session");
    let exchanges = session["exchanges"].as_array().expect("exchanges");
    let stops: Vec<&Value> =
        exchanges.iter().filter(|exchange| text(exchange, "kind") == "stop").collect();
    assert_eq!(stops.len(), 2, "one stale attempt and one live one");
    assert!(
        text(stops[0], "received").contains("stale_daemon_instance"),
        "the stale attempt is refused"
    );
    assert!(
        text(stops[1], "received").contains("acknowledged"),
        "and the live one still works afterwards, so nothing was disabled"
    );
}

#[test]
fn a_mismatch_answers_with_what_the_daemon_actually_has() {
    for (name, member) in [
        ("target-mismatch", "author_target_identity_digest"),
        ("revision-mismatch", "selected_environment_revision"),
    ] {
        let sessions = rows(SESSIONS);
        let session = sessions
            .iter()
            .find(|session| text(session, "name") == name)
            .unwrap_or_else(|| panic!("{name} is a session"));
        let refusal = session["exchanges"]
            .as_array()
            .expect("exchanges")
            .iter()
            .find(|exchange| text(exchange, "received").contains("mismatch"))
            .unwrap_or_else(|| panic!("{name} refuses something"));
        let answered: Value =
            serde_json::from_str(text(refusal, "received")).expect("one response");
        let sent: Value = serde_json::from_str(text(refusal, "sent")).expect("one request");
        assert_ne!(answered[member], sent[member], "{name}: the two differ, which is the point");
        assert!(
            !text(refusal, "received").contains("operation_identifier"),
            "{name}: a refusal before any work names no operation"
        );
    }
}

#[test]
fn no_transcript_carries_a_readable_principal() {
    for session in &rows(SESSIONS) {
        let name = text(session, "name");
        for payload in payloads(session) {
            for readable in ["user_name", "password", "client_secret", "@AdobeOrg"] {
                assert!(!payload.contains(readable), "{name}: carries {readable}");
            }
        }
    }
}

#[test]
fn a_replay_reports_where_the_operation_is_now() {
    let sessions = rows(SESSIONS);
    for (name, first, second) in [
        ("recovery-resume-replay", "recovery_resume_applied", "recovery_resume_replayed"),
        ("maintenance-apply-replay", "maintenance_applied", "maintenance_replayed"),
    ] {
        let session = sessions
            .iter()
            .find(|session| text(session, "name") == name)
            .unwrap_or_else(|| panic!("{name} is a session"));
        let received: Vec<&str> = session["exchanges"]
            .as_array()
            .expect("exchanges")
            .iter()
            .map(|exchange| text(exchange, "received"))
            .collect();
        assert!(received.iter().any(|payload| payload.contains(first)), "{name}: applied once");
        assert!(received.iter().any(|payload| payload.contains(second)), "{name}: then replayed");
    }
    let resume = sessions
        .iter()
        .find(|session| text(session, "name") == "recovery-resume-replay")
        .expect("that session");
    let replayed = resume["exchanges"]
        .as_array()
        .expect("exchanges")
        .iter()
        .find(|exchange| text(exchange, "received").contains("recovery_resume_replayed"))
        .expect("a replay");
    assert!(
        text(replayed, "received").contains("succeeded"),
        "a replay reports the state the operation is in now, not the one it had"
    );
}

#[test]
fn every_hostile_input_ends_its_own_transcript_and_nothing_else() {
    let vectors = rows(HOSTILE);
    assert!(vectors.len() >= 6, "every way a frame can be wrong");
    for row in &vectors {
        let note = text(row, "note");
        let mut reader = FrameReader::new();
        let outcome = reader.absorb(&limits(), &octets(row, "frame"));
        match outcome {
            Err(_) => assert!(reader.is_poisoned(), "{note}: the reader carried on"),
            Ok(None) => {
                assert!(!reader.is_poisoned(), "{note}: an incomplete frame is not a malformed one")
            }
            Ok(Some(payload)) => panic!("{note}: yielded {payload:?}"),
        }
        let mut fresh = FrameReader::new();
        let good = render(&limits(), b"{\"a\":1}").expect("a good frame");
        assert!(
            fresh.absorb(&limits(), &good).expect("a good frame").is_some(),
            "{note}: the next connection is unaffected"
        );
    }
}

#[test]
fn canonical_output_does_not_depend_on_how_a_map_was_built() {
    let scrambled = r#"{"selected_environment_revision":"cccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccc","request_identifier":"request-1","request":{"operation_identifier":"operation-1","request":"wait"},"operation_protocol_version":1,"daemon_runtime_contract_digest":"bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb","author_target_identity_digest":"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"}"#;
    let envelope: OperationEnvelope =
        serde_json::from_str(scrambled).expect("member order does not change meaning");
    let written = write_payload(&envelope).expect("it writes");
    assert_ne!(written, scrambled, "the input was not canonical");
    let again: OperationEnvelope = serde_json::from_str(&written).expect("its own bytes parse");
    assert_eq!(write_payload(&again).expect("it writes"), written, "and writing is stable");
}
