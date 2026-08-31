//! What a real client of the older revision sends, and what really comes back.
//!
//! These are the sequences the fsm executor actually sends: initialize, the
//! initialized notification, and then work. Both orders are replayed - the one
//! that asks for this revision and the one that asks for another and is offered
//! this one - because the executor may do either and the difference must not
//! reach anything past the handshake.
//!
//! The claim that matters is negative: nothing runs before the notification. A
//! server that answered a tool call arriving between the two halves would be
//! acting on a session neither side had agreed on, and the transcript is the
//! only place that is visible end to end.

use std::path::PathBuf;
use std::time::Duration;

use serde_json::Value;
use slingshot_test_support::process_harness::{
    CapturedProcess, ExecutablePath, ProcessHarness, ProcessRequest,
};

/// Where the conversations live.
const FIXTURES: &str = "../slingshot-test-support/fixtures/model-context-protocol/legacy-revision";

/// The variable that arms a rewrite of the expected transcripts.
const REVIEW_VARIABLE: &str = "SLINGSHOT_REVIEW_LEGACY_REVISION_TRANSCRIPTS";

/// The command a reviewer runs to rewrite them.
const REVIEW_COMMAND: &str = "SLINGSHOT_REVIEW_LEGACY_REVISION_TRANSCRIPTS=1 \
     cargo test -p slingshot-command-line --test legacy_revision_transcripts";

/// How long a conversation waits for the server to finish.
const PROMPT_DEADLINE: Duration = Duration::from_secs(30);

/// Returns the product executable these conversations drive.
fn product_executable() -> ExecutablePath {
    ExecutablePath::new(PathBuf::from(env!("CARGO_BIN_EXE_slingshot")))
        .expect("the product executable was built")
}

/// Returns one file from the fixture directory.
fn fixture_path(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(FIXTURES).join(name)
}

/// One conversation, as the source declares it.
#[derive(Debug, serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct Conversation {
    /// What it is called, and what its expected transcript is named.
    name: String,
    /// Why it is here.
    intent: String,
    /// The lines a client sends, in order.
    sends: Vec<Value>,
}

/// Returns every declared conversation.
fn conversations() -> Vec<Conversation> {
    let path = fixture_path("conversations.jsonl");
    let text = std::fs::read_to_string(&path)
        .unwrap_or_else(|failure| panic!("{} could not be read: {failure}", path.display()));
    text.lines()
        .filter(|line| !line.trim().is_empty() && !line.starts_with('#'))
        .map(|line| serde_json::from_str(line).expect("every conversation reads"))
        .collect()
}

/// Runs one conversation against the compiled server.
fn held(conversation: &Conversation) -> CapturedProcess {
    let input = conversation
        .sends
        .iter()
        .map(|line| serde_json::to_string(line).expect("a line writes"))
        .collect::<Vec<String>>()
        .join("\n")
        + "\n";
    let harness = ProcessHarness::new();
    let request = ProcessRequest::new(&[
        "--profile",
        "local",
        "--environment",
        "author",
        "protocol",
        "serve",
    ])
    .reading(input);
    harness.run_within(&product_executable(), &request, PROMPT_DEADLINE).expect("the server runs")
}

/// Compares one conversation's transcript with the bytes committed for it.
fn matches_fixture(name: &str, produced: &str) {
    let path = fixture_path(&format!("{name}.txt"));
    if std::env::var(REVIEW_VARIABLE).is_ok() {
        std::fs::write(&path, produced).expect("the transcript is written");
        return;
    }
    let expected = std::fs::read_to_string(&path).unwrap_or_else(|failure| {
        panic!("{} could not be read: {failure}; write it with `{REVIEW_COMMAND}`", path.display())
    });
    assert_eq!(produced, expected, "{name} changed; review it with `{REVIEW_COMMAND}`");
}

#[test]
fn every_conversation_produces_the_transcript_committed_for_it() {
    for conversation in conversations() {
        let produced = held(&conversation);
        matches_fixture(&conversation.name, &produced.standard_output);
        assert!(produced.status.success(), "{} exited badly", conversation.name);
    }
}

#[test]
fn standard_output_carries_protocol_messages_and_nothing_else() {
    for conversation in conversations() {
        let produced = held(&conversation);
        for line in produced.standard_output.lines() {
            let parsed: Value = serde_json::from_str(line).unwrap_or_else(|failure| {
                panic!("{}: {line} is not a message: {failure}", conversation.name)
            });
            assert!(parsed.is_object(), "{}: every line is one object", conversation.name);
            assert!(
                !parsed["id"].is_null() || !parsed["error"].is_null(),
                "{}: every line answers something",
                conversation.name
            );
        }
    }
}

#[test]
fn no_answer_precedes_the_initialized_notification() {
    for requested in ["2025-06-18", "2026-07-28"] {
        let conversation = Conversation {
            name: format!("handshake-{requested}"),
            intent: "an executor that asks for one revision and is offered another".to_owned(),
            sends: vec![
                serde_json::json!({
                    "id": "one",
                    "method": "initialize",
                    "params": { "protocolVersion": requested },
                }),
                serde_json::json!({ "method": "notifications/initialized" }),
                serde_json::json!({
                    "id": "two",
                    "method": "tools/list",
                    "params": { "protocolVersion": "2026-07-28" },
                }),
            ],
        };
        let produced = held(&conversation);
        let lines: Vec<&str> = produced.standard_output.lines().collect();
        assert_eq!(lines.len(), ANSWERS_IN_A_HANDSHAKE, "{requested}");
        let offered: serde_json::Value =
            serde_json::from_str(lines[0]).expect("the first answer is one document");
        assert_eq!(
            offered["result"]["protocolVersion"].as_str(),
            Some("2025-06-18"),
            "this era offers its own revision whichever one was asked for"
        );
    }
}

/// How many answers a handshake and one request produce.
const ANSWERS_IN_A_HANDSHAKE: usize = 2;

#[test]
fn a_notification_is_answered_with_silence() {
    let conversation = Conversation {
        name: "notification-only".to_owned(),
        intent: "a client that only notifies is answered nothing".to_owned(),
        sends: vec![serde_json::json!({
            "method": "notifications/cancelled",
            "params": { "requestId": "one" },
        })],
    };
    let produced = held(&conversation);
    assert!(produced.standard_output.is_empty(), "silence is the answer to a notification");
    assert!(produced.status.success());
}

#[test]
fn every_conversation_says_why_it_is_here_and_every_line_names_a_method() {
    let declared = conversations();
    assert!(!declared.is_empty());
    for conversation in &declared {
        assert!(!conversation.intent.is_empty(), "{} says why", conversation.name);
        let every_line_names_a_method =
            conversation.sends.iter().all(|line| line["method"].as_str().is_some());
        assert!(
            every_line_names_a_method,
            "{} sends a line that names a method",
            conversation.name
        );
    }
}
