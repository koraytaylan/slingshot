//! What happens between a tool call arriving and an operation existing.
//!
//! The claim worth proving twice is the one about invented keys. A command that
//! may omit its key gets one invented, and the same one every time the request
//! is retried or reconnected - because a second identifier would turn a retry
//! into a second operation, and the caller would have started work twice by
//! asking once.

use serde_json::json;

use slingshot_command_line::model_context_protocol::operation_execution::{
    ExecutionRefusal, ExecutionState, KeySource, ResumeBelief, ResumeOutcome, ResumeReality,
    require_complete_belief, require_runnable, resumed, supplied_key,
};
use slingshot_command_line::model_context_protocol::schema_projection::{
    OPERATION_KEY_MEMBER, Stage,
};
use slingshot_command_line::model_context_protocol::tool_catalog::{
    KeyPresence, Provenance, ToolDescriptor, derive,
};
use slingshot_domain::command::canonical_json::write_canonical;

/// The protocol request identifier every case is made under.
const REQUEST: &str = "request-one";

/// The revision an operation in these cases stands at.
const REVISION: u64 = 9;

/// A revision it does not stand at.
const OTHER_REVISION: u64 = 10;

/// The recovery category an operation in these cases waits in.
const CATEGORY: &str = "submission_unknown";

/// Returns the tool one name belongs to.
fn tool(named: &str) -> ToolDescriptor {
    derive(&Provenance::recomputed())
        .expect("this build's provenance agrees with itself")
        .into_iter()
        .find(|held| held.name == named)
        .expect("the tool exists")
}

/// Returns one document's canonical bytes.
fn canonical(value: &serde_json::Value) -> Vec<u8> {
    write_canonical(value).expect("the document is canonical").into_bytes()
}

#[test]
fn a_supplied_key_is_preserved_exactly_and_nothing_is_invented() {
    let mut state = ExecutionState::new();
    let held = state.operation_key(REQUEST, &tool("replicate_content"), Some("mine"), || {
        panic!("nothing is invented when the caller supplied one")
    });
    assert_eq!(held, KeySource::Supplied("mine".to_owned()));
    assert_eq!(state.holding(), 0, "nothing invented is nothing to hold");
}

#[test]
fn an_omitted_key_is_invented_once_and_reused_for_every_retry() {
    let mut state = ExecutionState::new();
    let mut invented = 0;
    let mut generate = || {
        invented += 1;
        format!("invented-{invented}")
    };
    let first = state.operation_key(REQUEST, &tool("query_paths"), None, &mut generate);
    let reconnected = state.operation_key(REQUEST, &tool("query_paths"), None, &mut generate);
    let retried = state.operation_key(REQUEST, &tool("query_paths"), None, &mut generate);
    assert_eq!(first, reconnected, "a reconnect is the same operation");
    assert_eq!(first, retried, "so is a retry");
    assert_eq!(invented, 1, "a second identifier would be a second operation");
    assert_eq!(first, KeySource::GeneratedOnce("invented-1".to_owned()));
}

#[test]
fn an_invented_key_is_not_the_protocol_request_identifier() {
    let mut state = ExecutionState::new();
    let held = state.operation_key(REQUEST, &tool("query_paths"), None, || "invented".to_owned());
    assert_ne!(
        held.identifier(),
        Some(REQUEST),
        "a client may reuse its request identifier the moment its answer arrives"
    );
}

#[test]
fn a_released_request_holds_nothing_and_a_later_one_invents_afresh() {
    let mut state = ExecutionState::new();
    state.operation_key(REQUEST, &tool("query_paths"), None, || "first".to_owned());
    assert_eq!(state.holding(), 1);
    assert!(state.release(REQUEST));
    assert_eq!(state.holding(), 0);
    let after = state.operation_key(REQUEST, &tool("query_paths"), None, || "second".to_owned());
    assert_eq!(after, KeySource::GeneratedOnce("second".to_owned()));
    assert!(!state.release("never-seen"));
}

#[test]
fn a_control_invents_nothing_because_it_starts_nothing() {
    let mut state = ExecutionState::new();
    let held = state.operation_key(REQUEST, &tool("operation-list"), None, || {
        panic!("a control starts no work")
    });
    assert_eq!(held, KeySource::Absent);
    assert_eq!(tool("operation-list").operation_key, KeyPresence::Absent);
}

#[test]
fn nothing_runs_until_provenance_the_tool_and_the_arguments_all_pass() {
    let arguments = canonical(&json!({ OPERATION_KEY_MEMBER: "mine" }));
    let (held, decoded) =
        require_runnable("replicate_content", &arguments, &Provenance::recomputed())
            .expect("a well-formed call is runnable");
    assert_eq!(held.name, "replicate_content");
    assert_eq!(supplied_key(&decoded), Some("mine"));

    let unknown = require_runnable("tools/invent", &arguments, &Provenance::recomputed())
        .expect_err("no such tool");
    assert_eq!(unknown, ExecutionRefusal::ToolUnknown("tools/invent".to_owned()));

    let drifted = Provenance {
        canonical_contract_digest:
            "1111111111111111111111111111111111111111111111111111111111111111".to_owned(),
        ..Provenance::recomputed()
    };
    let refused = require_runnable("replicate_content", &arguments, &drifted)
        .expect_err("a drifted build runs nothing");
    assert!(matches!(refused, ExecutionRefusal::ProvenanceDrifted(_)));
}

#[test]
fn an_argument_refusal_names_the_check_that_made_it() {
    let missing = canonical(&json!({}));
    let refusal = require_runnable("replicate_content", &missing, &Provenance::recomputed())
        .expect_err("a required key cannot be omitted");
    let ExecutionRefusal::ArgumentsRefused(held) = refusal else {
        panic!("the arguments were refused")
    };
    assert_eq!(held.stage, Stage::DecodedShape);

    let noncanonical = br#"{ "operation_key": "mine" }"#;
    let refusal = require_runnable("replicate_content", noncanonical, &Provenance::recomputed())
        .expect_err("noncanonical bytes are refused");
    let ExecutionRefusal::ArgumentsRefused(held) = refusal else {
        panic!("the arguments were refused")
    };
    assert_eq!(held.stage, Stage::RawBytes, "the bytes are judged before the shape");
}

#[test]
fn a_resume_says_what_it_believes_before_it_is_sent() {
    let complete = json!({
        "operation_identifier": "one",
        "expected_recovery_category": CATEGORY,
        "expected_operation_revision": REVISION,
    });
    let belief = require_complete_belief(&complete).expect("it says what it believes");
    assert_eq!(belief.operation_identifier, "one");
    for missing in
        ["operation_identifier", "expected_recovery_category", "expected_operation_revision"]
    {
        let mut partial = complete.clone();
        partial.as_object_mut().expect("it is an object").remove(missing);
        assert_eq!(
            require_complete_belief(&partial),
            Err(ExecutionRefusal::ResumeIncomplete),
            "a resume without {missing} believes nothing"
        );
    }
}

#[test]
fn a_resume_whose_belief_is_true_releases_the_recovery_once() {
    let belief = ResumeBelief {
        expected_recovery_category: CATEGORY.to_owned(),
        expected_operation_revision: REVISION,
        operation_identifier: "one".to_owned(),
    };
    let waiting = ResumeReality {
        recovery_category: Some(CATEGORY.to_owned()),
        revision: REVISION,
        receipted: false,
    };
    assert_eq!(resumed(&belief, &waiting), ResumeOutcome::Applied);

    let already = ResumeReality { receipted: true, ..waiting.clone() };
    assert_eq!(
        resumed(&belief, &already),
        ResumeOutcome::Replayed,
        "the second send finds the receipt and runs nothing"
    );
}

#[test]
fn a_resume_whose_belief_is_stale_schedules_nothing() {
    let belief = ResumeBelief {
        expected_recovery_category: CATEGORY.to_owned(),
        expected_operation_revision: REVISION,
        operation_identifier: "one".to_owned(),
    };
    let moved_on = ResumeReality {
        recovery_category: Some(CATEGORY.to_owned()),
        revision: OTHER_REVISION,
        receipted: false,
    };
    assert!(matches!(resumed(&belief, &moved_on), ResumeOutcome::Refused { .. }));

    let another_category = ResumeReality {
        recovery_category: Some("remote_outcome_unknown".to_owned()),
        revision: REVISION,
        receipted: false,
    };
    assert!(matches!(resumed(&belief, &another_category), ResumeOutcome::Refused { .. }));

    let running = ResumeReality { recovery_category: None, revision: REVISION, receipted: false };
    assert!(
        matches!(resumed(&belief, &running), ResumeOutcome::Refused { .. }),
        "an operation that waits in no recovery has nothing to release"
    );
}
