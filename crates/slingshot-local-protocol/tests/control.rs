//! The surface that keeps working when nothing else does.
//!
//! The property worth proving is negative and easy to lose: no incompatibility
//! takes inspection or an explicit stop away. A client built against another
//! operation-protocol version, or another runtime contract, can still greet the
//! daemon, read its status, ping it, and stop it by its current nonce. Without
//! that, an incompatible client would be left with a daemon it can neither use
//! nor shut down, and the only remaining move would be to find the process and
//! signal it - which every other rule here exists to prevent.

use serde_json::Value;
use slingshot_local_protocol::control::{
    ControlConversation, ControlFailure, DaemonStatusResult, HELLO_METHOD, HelloResult,
    OperationCompatibility, RETAINED_METHODS, hello_required_refusal, operation_compatibility,
};
use slingshot_local_protocol::foundation_contract::FoundationContract;
use slingshot_local_protocol::ping::{PING_METHOD, STOP_METHOD, stop_is_authorized};

/// Greeting vectors this test reads.
const GREETINGS: &str = include_str!("fixtures/control/greetings.jsonl");

/// Status vectors this test reads.
const STATUSES: &str = include_str!("fixtures/control/statuses.jsonl");

/// Ordering vectors this test reads.
const ORDERS: &str = include_str!("fixtures/control/orders.jsonl");

/// Compatibility vectors this test reads.
const COMPATIBILITY: &str = include_str!("fixtures/control/compatibility.jsonl");

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

/// Returns the greeting one fixture row spells.
fn greeting_of(row: &Value) -> HelloResult {
    serde_json::from_str(text(row, "document")).expect("the fixture says this is accepted")
}

#[test]
fn every_greeting_vector_round_trips_to_its_own_bytes() {
    let vectors = rows(GREETINGS);
    assert!(vectors.len() >= 8, "including the shapes a greeting must refuse");
    for row in &vectors {
        let document = text(row, "document");
        let note = text(row, "note");
        match (row["accepted"].as_bool(), serde_json::from_str::<HelloResult>(document)) {
            (Some(true), Ok(hello)) => assert_eq!(
                serde_json::to_string(&hello).expect("a greeting serializes"),
                document,
                "{note}: rewritten differently"
            ),
            (Some(false), Err(_)) => (),
            (_, parsed) => panic!("{note}: the greeting answered {parsed:?}"),
        }
    }
}

#[test]
fn a_greeting_carries_the_opaque_target_and_no_readable_principal() {
    let accepted = rows(GREETINGS);
    let plain = accepted
        .iter()
        .find(|row| text(row, "note") == "an ordinary greeting")
        .expect("one ordinary greeting");
    let hello = greeting_of(plain);
    let written = serde_json::to_string(&hello).expect("a greeting serializes");
    for readable in ["user_name", "organization", "client_identifier", "password", "@"] {
        assert!(
            !written.contains(readable),
            "a greeting is sent to anything that can reach the socket, and carries no {readable}"
        );
    }
    assert_eq!(hello.author_target_identity_digest.len(), DIGEST_CHARACTERS);
    assert!(!hello.selected_environment_revision.is_empty(), "the revision is mandatory");

    let contract = FoundationContract::embedded();
    assert_eq!(hello.require_well_formed(&contract), Ok(()));
    for row in accepted.iter().filter(|row| text(row, "reason") == "DigestNotCanonical") {
        assert_eq!(
            greeting_of(row).require_well_formed(&contract),
            Err(ControlFailure::DigestNotCanonical),
            "{}",
            text(row, "note")
        );
    }
}

#[test]
fn every_status_vector_round_trips_and_stays_bounded() {
    for row in &rows(STATUSES) {
        let document = text(row, "document");
        let note = text(row, "note");
        match (row["accepted"].as_bool(), serde_json::from_str::<DaemonStatusResult>(document)) {
            (Some(true), Ok(status)) => assert_eq!(
                serde_json::to_string(&status).expect("a status serializes"),
                document,
                "{note}: rewritten differently"
            ),
            (Some(false), Err(_)) => (),
            (_, parsed) => panic!("{note}: the status answered {parsed:?}"),
        }
    }
}

#[test]
fn a_connection_greets_before_it_asks_for_anything_else() {
    let vectors = rows(ORDERS);
    assert!(vectors.len() >= 8, "every retained method before and after the greeting");
    for row in &vectors {
        let methods: Vec<&str> = row["methods"]
            .as_array()
            .expect("a method list")
            .iter()
            .map(|method| method.as_str().expect("a method name"))
            .collect();
        let mut conversation = ControlConversation::new();
        let accepted = methods.iter().all(|method| conversation.admit(method).is_ok());
        assert_eq!(
            accepted,
            row["accepted"].as_bool().expect("a verdict"),
            "{}",
            text(row, "note")
        );
    }
    let mut silent = ControlConversation::new();
    assert_eq!(silent.admit(PING_METHOD), Err(ControlFailure::HelloRequired));
    assert!(!silent.has_greeted(), "a refused request does not count as a greeting");
    assert_eq!(hello_required_refusal().code, "hello_required");
}

#[test]
fn every_compatibility_vector_answers_the_way_the_fixture_says() {
    let plain = rows(GREETINGS);
    let base = greeting_of(
        plain
            .iter()
            .find(|row| text(row, "note") == "an ordinary greeting")
            .expect("one ordinary greeting"),
    );
    for row in &rows(COMPATIBILITY) {
        let daemon: Vec<u32> = row["daemon"]
            .as_array()
            .expect("a version list")
            .iter()
            .map(|version| u32::try_from(version.as_u64().expect("a version")).expect("in range"))
            .collect();
        let caller: Vec<u32> = row["caller"]
            .as_array()
            .expect("a version list")
            .iter()
            .map(|version| u32::try_from(version.as_u64().expect("a version")).expect("in range"))
            .collect();
        let hello = HelloResult { supported_operation_protocol_versions: daemon, ..base.clone() };
        let answered = operation_compatibility(&hello, &caller, text(row, "caller_digest"));
        let note = text(row, "note");
        match (text(row, "outcome"), answered) {
            ("compatible", OperationCompatibility::Compatible { version }) => assert_eq!(
                u64::from(version),
                row["version"].as_u64().expect("the agreed version"),
                "{note}"
            ),
            ("no_shared_version", OperationCompatibility::NoSharedVersion) => (),
            ("runtime_contract_differs", OperationCompatibility::RuntimeContractDiffers) => (),
            (expected, actual) => panic!("{note}: expected {expected}, got {actual:?}"),
        }
    }
}

#[test]
fn no_incompatibility_takes_inspection_or_an_explicit_stop_away() {
    let plain = rows(GREETINGS);
    let base = greeting_of(
        plain
            .iter()
            .find(|row| text(row, "note") == "an ordinary greeting")
            .expect("one ordinary greeting"),
    );
    for (note, caller_versions, caller_digest) in [
        ("no shared version", vec![9_u32], base.daemon_runtime_contract_digest.clone()),
        ("another runtime contract", vec![1_u32], "d".repeat(DIGEST_CHARACTERS)),
    ] {
        let answered = operation_compatibility(&base, &caller_versions, &caller_digest);
        assert!(!answered.permits_operations(), "{note}");
        assert!(answered.permits_retained_control(), "{note}: control is never withdrawn");
        let refusal = answered.refusal().unwrap_or_else(|| panic!("{note}: no refusal"));
        assert!(
            refusal.message.contains("stop it explicitly"),
            "{note}: the refusal says what the client can still do"
        );
        let mut conversation = ControlConversation::new();
        for method in RETAINED_METHODS {
            assert_eq!(conversation.admit(method), Ok(()), "{note}: {method} stays reachable");
        }
    }
}

#[test]
fn only_the_exact_live_nonce_stops_this_instance() {
    let live = "0123456789abcdef0123456789abcdef";
    assert!(stop_is_authorized(live, live));
    for (note, supplied) in [
        ("a nonce the prior instance published", "fedcba9876543210fedcba9876543210"),
        ("a nonce one character short", &live[..live.len() - 1]),
        ("an empty nonce", ""),
        ("a nonce differing in one character", "1123456789abcdef0123456789abcdef"),
    ] {
        assert!(!stop_is_authorized(live, supplied), "{note}");
    }
    let namespace = "slingshot-a1b2c3d4";
    let target = "a".repeat(DIGEST_CHARACTERS);
    for substitute in [namespace, target.as_str(), "4321"] {
        assert!(
            !stop_is_authorized(live, substitute),
            "a namespace, a target, and a process identifier are not this instance"
        );
    }
}

#[test]
fn the_retained_methods_are_the_ones_plan_0001_spelled() {
    assert_eq!(RETAINED_METHODS, [HELLO_METHOD, "daemon.ping", "daemon.status", "daemon.stop"]);
    assert_eq!(PING_METHOD, "daemon.ping", "the retained spelling is unchanged");
    assert_eq!(STOP_METHOD, "daemon.stop");
}
