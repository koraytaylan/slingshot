//! What the older revision accepts, when it accepts it, and what it never says.
//!
//! Two things separate this era from the current one, and both are proved here:
//! nothing is dispatched before the handshake finishes, and no result carries a
//! member that belongs to the other era. A client from this era is entitled to
//! reject a message carrying one, so producing it would be a compatibility
//! failure rather than a harmless extra.
//!
//! The oracle is the digest-pinned shape declaration committed beside this
//! revision, recomputed before it is used. What that document is - and what it
//! is not, in an environment that cannot retrieve the official artifact - is
//! stated in the note beside it.

use std::path::PathBuf;

use serde_json::{Value, json};
use slingshot_command_line::model_context_protocol::current_stateless_revision::{
    EVERY_REQUEST, METHOD_NOT_FOUND_ERROR,
};
use slingshot_command_line::model_context_protocol::legacy_initialized_revision::{
    INITIALIZE_REQUEST, LegacyRefusal, LegacySession, Lifecycle, MODERN_ONLY_MEMBERS,
    NOT_INITIALIZED_ERROR, undecorated,
};
use slingshot_command_line::model_context_protocol::standard_stream_transport::SUPPORTED_REVISIONS;

/// Where this revision's committed shapes live.
const SHAPES: &str =
    "../slingshot-test-support/fixtures/model-context-protocol/official-schemas/2025-06-18";

/// The revision this era speaks.
const REVISION: &str = "2025-06-18";

/// Returns one file from the revision's directory.
fn artifact(name: &str) -> String {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(SHAPES).join(name);
    std::fs::read_to_string(&path)
        .unwrap_or_else(|failure| panic!("{} could not be read: {failure}", path.display()))
}

/// Returns the shape declaration, after proving it is the one that was pinned.
fn shapes() -> Value {
    let text = artifact("served-shapes.json");
    let pinned = artifact("served-shapes.sha256");
    use sha2::Digest;
    let mut digest = sha2::Sha256::new();
    digest.update(text.as_bytes());
    let observed: String = digest.finalize().iter().map(|byte| format!("{byte:02x}")).collect();
    assert_eq!(observed, pinned.trim(), "the oracle changed without its digest changing");
    serde_json::from_str(&text).expect("the declaration reads")
}

/// Requires one object to satisfy the declared shape of `named` under `section`.
fn require_shaped(section: &str, named: &str, held: &Value) {
    let shapes = shapes();
    let shape = if named.is_empty() { &shapes[section] } else { &shapes[section][named] };
    assert!(!shape.is_null(), "the declaration says nothing about {section}/{named}");
    let object = held.as_object().unwrap_or_else(|| panic!("{named} is an object"));
    for member in shape["required"].as_array().expect("required members are a list") {
        let member = member.as_str().expect("a member is named");
        assert!(object.contains_key(member), "{section}/{named} omits {member}");
    }
    for member in shape["forbidden"].as_array().expect("forbidden members are a list") {
        let member = member.as_str().expect("a member is named");
        assert!(!object.contains_key(member), "{section}/{named} carries {member}");
    }
}

#[test]
fn the_declaration_is_the_one_that_was_pinned_and_names_this_revision() {
    assert_eq!(shapes()["revision"].as_str(), Some(REVISION));
    assert!(artifact("PROVENANCE.md").contains("no copy of that document"));
}

#[test]
fn an_initialize_naming_this_revision_is_echoed() {
    let mut session = LegacySession::new();
    let answered = session.initialize(REVISION);
    assert_eq!(answered["protocolVersion"].as_str(), Some(REVISION));
    assert!(session.echoed_the_request(), "the client asked for what it was offered");
    assert_eq!(session.lifecycle(), Lifecycle::Offered);
    require_shaped("results", INITIALIZE_REQUEST, &answered);
}

#[test]
fn an_initialize_naming_another_revision_is_offered_this_one_rather_than_refused() {
    for requested in [SUPPORTED_REVISIONS[0], "2020-01-01"] {
        let mut session = LegacySession::new();
        let answered = session.initialize(requested);
        assert_eq!(
            answered["protocolVersion"].as_str(),
            Some(REVISION),
            "an older client asking for {requested} is asking whether there is common ground"
        );
        assert!(!session.echoed_the_request());
        assert!(answered["error"].is_null(), "the answer is a successful one");
        require_shaped("results", INITIALIZE_REQUEST, &answered);
    }
}

#[test]
fn nothing_is_dispatched_before_the_client_says_it_is_initialized() {
    let mut session = LegacySession::new();
    let refusal =
        session.require_actionable("tools/call").expect_err("the handshake has not begun");
    assert_eq!(refusal, LegacyRefusal::NotInitialized { named: "tools/call".to_owned() });
    assert_eq!(refusal.code(), NOT_INITIALIZED_ERROR);
    require_shaped("errors", "", &refusal.rendered());

    session.initialize(REVISION);
    assert!(
        session.require_actionable("tools/call").is_err(),
        "the answer is half a handshake, not a session"
    );

    assert!(session.initialized());
    assert_eq!(session.lifecycle(), Lifecycle::Ready);
    session.require_actionable("tools/call").expect("the session is established");
}

#[test]
fn an_initialized_notification_out_of_order_establishes_nothing() {
    let mut session = LegacySession::new();
    assert!(!session.initialized(), "nothing was offered to be initialized");
    assert_eq!(session.lifecycle(), Lifecycle::Fresh);
    assert!(session.require_actionable("tools/list").is_err());
}

#[test]
fn an_initialize_is_answerable_at_any_point_in_a_session() {
    let mut session = LegacySession::new();
    session.require_actionable(INITIALIZE_REQUEST).expect("a fresh session may be initialized");
    session.initialize(REVISION);
    session.initialized();
    session.require_actionable(INITIALIZE_REQUEST).expect("and so may an established one");
}

#[test]
fn a_method_this_server_does_not_offer_is_refused_as_that_even_before_the_handshake() {
    let session = LegacySession::new();
    let refusal = session.require_actionable("tools/invent").expect_err("no such method");
    assert_eq!(refusal, LegacyRefusal::MethodUnavailable { named: "tools/invent".to_owned() });
    assert_eq!(refusal.code(), METHOD_NOT_FOUND_ERROR);
}

#[test]
fn no_result_this_era_writes_carries_a_member_from_the_other_one() {
    for method in EVERY_REQUEST {
        let modern = json!({
            "resultType": "complete",
            "ttlMs": 60_000,
            "cacheScope": "session",
            "tools": [],
            "content": [],
            "resources": [],
            "resourceTemplates": [],
            "contents": [],
        });
        let answered = undecorated(modern);
        for member in MODERN_ONLY_MEMBERS {
            assert!(answered[member].is_null(), "{method} carries {member}");
        }
        require_shaped("results", method, &answered);
    }
}

#[test]
fn a_real_client_sequence_reaches_a_direct_call_only_after_the_handshake() {
    for requested in [REVISION, SUPPORTED_REVISIONS[0]] {
        let mut session = LegacySession::new();
        let mut dispatched = Vec::new();
        for (method, is_notification) in EVERY_STEP {
            if *is_notification {
                session.initialized();
                continue;
            }
            match session.require_actionable(method) {
                Ok(()) if *method != INITIALIZE_REQUEST => dispatched.push(*method),
                Ok(()) => {
                    session.initialize(requested);
                }
                Err(refusal) => assert_eq!(
                    refusal,
                    LegacyRefusal::NotInitialized { named: (*method).to_owned() },
                    "only the handshake stops a step in this sequence"
                ),
            }
        }
        assert_eq!(dispatched, vec!["tools/call"], "{requested}");
        assert_eq!(session.lifecycle(), Lifecycle::Ready);
    }
}

/// The sequence a real client of this era sends, in order.
const EVERY_STEP: &[(&str, bool)] = &[
    ("tools/call", false),
    (INITIALIZE_REQUEST, false),
    ("tools/call", false),
    ("notifications/initialized", true),
    ("tools/call", false),
];
