//! What the current revision accepts, answers, and refuses.
//!
//! Every inbound case is validated against a committed shape declaration before
//! it is dispatched, and every outbound message is validated against the same
//! document afterwards. The declaration is digest-pinned and the digest is
//! recomputed here, so an oracle that changed without anybody choosing the
//! change fails before it is trusted.
//!
//! What that document is, and what it is not, is stated beside it: this
//! environment cannot retrieve the official revision artifact, and a
//! hand-written file presented as that artifact would be a forgery. The
//! mechanism is real and the authority is this build's own until the artifact
//! is retrieved.

use std::collections::BTreeSet;
use std::path::PathBuf;

use serde_json::{Value, json};
use slingshot_command_line::model_context_protocol::current_stateless_revision::{
    CACHE_SCOPE_MEMBER, COMPLETE_MEMBER, COMPLETE_VALUE, EVERY_CAPABILITY, EVERY_ERROR,
    EVERY_INBOUND_NOTIFICATION, EVERY_OUTBOUND_NOTIFICATION, EVERY_REQUEST,
    INVALID_PARAMETERS_ERROR, LIFETIME_MEMBER, METHOD_NOT_FOUND_ERROR, REVISION_MEMBER, Refusal,
    UNSUPPORTED_REVISION_ERROR, decorated, discovery, require_answerable,
};
use slingshot_command_line::model_context_protocol::standard_stream_transport::SUPPORTED_REVISIONS;

/// Where the revision's committed shapes live.
const SHAPES: &str =
    "../slingshot-test-support/fixtures/model-context-protocol/official-schemas/2026-07-28";

/// The revision this suite speaks.
const REVISION: &str = "2026-07-28";

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
    let observed = sha256_of(&text);
    assert_eq!(observed, pinned.trim(), "the oracle changed without its digest changing");
    serde_json::from_str(&text).expect("the declaration reads")
}

/// Returns the lowercase hexadecimal digest of one text.
fn sha256_of(text: &str) -> String {
    use sha2::Digest;
    let mut digest = sha2::Sha256::new();
    digest.update(text.as_bytes());
    digest.finalize().iter().map(|byte| format!("{byte:02x}")).collect()
}

/// Requires one object to satisfy the declared shape of `named` under `section`.
fn require_shaped(shapes: &Value, section: &str, named: &str, held: &Value) {
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

/// Returns whether one object satisfies the declared shape of `named`.
fn is_shaped(shapes: &Value, section: &str, named: &str, held: &Value) -> bool {
    let shape = &shapes[section][named];
    let Some(object) = held.as_object() else {
        return false;
    };
    let holds_required = shape["required"].as_array().is_some_and(|members| {
        members.iter().all(|member| member.as_str().is_some_and(|named| object.contains_key(named)))
    });
    let avoids_forbidden = shape["forbidden"].as_array().is_some_and(|members| {
        members
            .iter()
            .all(|member| member.as_str().is_some_and(|named| !object.contains_key(named)))
    });
    holds_required && avoids_forbidden
}

#[test]
fn the_declaration_is_the_one_that_was_pinned_and_names_this_revision() {
    let shapes = shapes();
    assert_eq!(shapes["revision"].as_str(), Some(REVISION));
    let provenance = artifact("PROVENANCE.md");
    assert!(provenance.contains("no copy of that document"), "the directory says what it holds");
}

#[test]
fn the_declaration_and_the_build_name_the_same_requests_and_notifications() {
    let shapes = shapes();
    let declared: BTreeSet<String> = shapes["requests"]
        .as_object()
        .expect("the requests are an object")
        .keys()
        .cloned()
        .collect();
    let offered: BTreeSet<String> = EVERY_REQUEST.iter().map(|held| (*held).to_owned()).collect();
    assert_eq!(declared, offered, "the declaration and the build offer different requests");

    let inbound: BTreeSet<String> = shapes["inbound_notifications"]
        .as_object()
        .expect("the inbound notifications are an object")
        .keys()
        .cloned()
        .collect();
    assert_eq!(inbound, EVERY_INBOUND_NOTIFICATION.iter().map(|held| (*held).to_owned()).collect());
    let outbound: BTreeSet<String> = shapes["outbound_notifications"]
        .as_object()
        .expect("the outbound notifications are an object")
        .keys()
        .cloned()
        .collect();
    assert_eq!(
        outbound,
        EVERY_OUTBOUND_NOTIFICATION.iter().map(|held| (*held).to_owned()).collect()
    );
}

#[test]
fn discovery_advertises_both_revisions_in_order_and_two_capabilities() {
    let answered = decorated("server/discover", discovery());
    require_shaped(&shapes(), "results", "server/discover", &answered);
    let versions: Vec<&str> = answered["supportedVersions"]
        .as_array()
        .expect("the versions are a list")
        .iter()
        .map(|held| held.as_str().expect("a version is text"))
        .collect();
    assert_eq!(versions, SUPPORTED_REVISIONS.to_vec());
    let capabilities: BTreeSet<String> = answered["capabilities"]
        .as_object()
        .expect("the capabilities are an object")
        .keys()
        .cloned()
        .collect();
    assert_eq!(capabilities, EVERY_CAPABILITY.iter().map(|held| (*held).to_owned()).collect());
}

#[test]
fn every_positive_request_satisfies_its_declared_shape_and_is_answerable() {
    let shapes = shapes();
    for method in EVERY_REQUEST {
        let request = positive_request(method);
        require_shaped(&shapes, "requests", method, &request);
        require_answerable(method, REVISION).unwrap_or_else(|refusal| {
            panic!("{method} is answerable: {refusal:?}");
        });
    }
}

/// Returns one well-formed request for `method`.
fn positive_request(method: &str) -> Value {
    let mut request = json!({ REVISION_MEMBER: REVISION });
    let object = request.as_object_mut().expect("a request is an object");
    match method {
        "tools/call" => {
            object.insert("name".to_owned(), json!("load_content_as_json"));
            object.insert("arguments".to_owned(), json!({ "path": "/content/site" }));
        }
        "resources/read" => {
            object.insert("uri".to_owned(), json!("slingshot://targets/one/operations/two"));
        }
        _ => {}
    }
    request
}

#[test]
fn a_malformed_request_fails_the_same_oracle_a_positive_one_satisfies() {
    let shapes = shapes();
    let missing_name = json!({ REVISION_MEMBER: REVISION, "arguments": {} });
    assert!(!is_shaped(&shapes, "requests", "tools/call", &missing_name));
    let missing_uri = json!({ REVISION_MEMBER: REVISION });
    assert!(!is_shaped(&shapes, "requests", "resources/read", &missing_uri));
    let not_an_object = json!(["tools/call"]);
    assert!(!is_shaped(&shapes, "requests", "tools/call", &not_an_object));
    let cancelled_with_identifier = json!({ "requestId": "one", "id": "one" });
    assert!(!is_shaped(
        &shapes,
        "inbound_notifications",
        "notifications/cancelled",
        &cancelled_with_identifier
    ));
}

#[test]
fn an_unsupported_revision_is_refused_with_the_ordered_list_and_nothing_else() {
    let refusal = require_answerable("tools/list", "2020-01-01")
        .expect_err("this build serves no such revision");
    assert_eq!(refusal, Refusal::RevisionUnsupported { requested: "2020-01-01".to_owned() });
    let rendered = refusal.rendered();
    assert_eq!(rendered["code"].as_i64(), Some(UNSUPPORTED_REVISION_ERROR));
    assert_eq!(rendered["data"]["requested"].as_str(), Some("2020-01-01"));
    let supported: Vec<&str> = rendered["data"]["supported"]
        .as_array()
        .expect("the supported list is a list")
        .iter()
        .map(|held| held.as_str().expect("a revision is text"))
        .collect();
    assert_eq!(supported, SUPPORTED_REVISIONS.to_vec());
    require_shaped(&shapes(), "errors", "", &rendered);
}

#[test]
fn the_legacy_revision_is_not_this_handlers_to_answer() {
    let refusal = require_answerable("tools/list", SUPPORTED_REVISIONS[1])
        .expect_err("the other era answers that one");
    assert_eq!(refusal.code(), UNSUPPORTED_REVISION_ERROR);
}

#[test]
fn a_method_this_server_does_not_offer_is_refused_as_that_and_nothing_else() {
    let refusal = require_answerable("tools/invent", REVISION).expect_err("no such method");
    assert_eq!(refusal, Refusal::MethodUnavailable { named: "tools/invent".to_owned() });
    assert_eq!(refusal.code(), METHOD_NOT_FOUND_ERROR);
    let rendered = refusal.rendered();
    assert!(rendered["data"].is_null(), "a missing method needs no data to explain it");
}

#[test]
fn every_successful_result_says_it_is_whole_and_a_listing_says_how_long_for() {
    let shapes = shapes();
    for method in EVERY_REQUEST {
        let answered = decorated(method, semantic_payload(method));
        assert_eq!(answered[COMPLETE_MEMBER].as_str(), Some(COMPLETE_VALUE), "{method}");
        require_shaped(&shapes, "results", method, &answered);
    }
    let failing_call = decorated("tools/call", json!({ "content": [], "isError": true }));
    assert_eq!(
        failing_call[COMPLETE_MEMBER].as_str(),
        Some(COMPLETE_VALUE),
        "a call that completed and reported a failure is still a complete result"
    );
    let ping = decorated("ping", json!({}));
    assert!(ping[LIFETIME_MEMBER].is_null(), "a ping is not a listing");
    assert!(ping[CACHE_SCOPE_MEMBER].is_null());
}

/// Returns the semantic payload one method answers with.
fn semantic_payload(method: &str) -> Value {
    match method {
        "server/discover" => discovery(),
        "tools/list" => json!({ "tools": [] }),
        "tools/call" => json!({ "content": [] }),
        "resources/list" => json!({ "resources": [] }),
        "resources/templates/list" => json!({ "resourceTemplates": [] }),
        "resources/read" => json!({ "contents": [] }),
        _ => json!({}),
    }
}

#[test]
fn eligibility_depends_on_the_request_in_hand_and_not_on_what_came_before() {
    let refused = require_answerable("tools/invent", REVISION);
    assert!(refused.is_err());
    require_answerable("tools/list", REVISION).expect("the next request is read on its own");
    let wrong_revision = require_answerable("tools/list", "2020-01-01");
    assert!(wrong_revision.is_err());
    require_answerable("tools/list", REVISION).expect("and so is the one after that");
}

#[test]
fn every_error_this_surface_reaches_is_one_the_declaration_names() {
    let declared: Vec<i64> = shapes()["errors"]["codes"]
        .as_array()
        .expect("the codes are a list")
        .iter()
        .map(|held| held.as_i64().expect("a code is a number"))
        .collect();
    assert_eq!(declared, EVERY_ERROR.to_vec());
    let unusable =
        Refusal::ParametersUnusable { detail: "a call names the tool it calls".to_owned() };
    assert_eq!(unusable.code(), INVALID_PARAMETERS_ERROR);
    require_shaped(&shapes(), "errors", "", &unusable.rendered());
}
