//! What survives a daemon going away in the middle of a workflow.
//!
//! Everything that identifies the work. The key comes from the occurrence,
//! which no restart changes; the operation it names is durable, so recovering
//! it is a lookup rather than a repeat; and the effect fence is part of that
//! durable state, so the work that already ran does not run again because the
//! process that started it went away.
//!
//! A restart is therefore transparent inside the handler deadline: the workflow
//! sees a call that took longer, not an outcome that changed.

use std::path::PathBuf;

use serde_json::Value;
use slingshot_development::finite_state_machine_handler_validation::{
    LEAST_HANDLER_TIMEOUT_MILLISECONDS, MOST_HANDLER_TIMEOUT_MILLISECONDS,
    workflow_effect_operation_key,
};

/// Where the restart rows live.
const FIXTURE: &str = "tests/fixtures/finite-state-machine-daemon-restart/restarts.jsonl";

/// Where the machine a person runs lives.
const MACHINE: &str = "../../examples/finite-state-machine/daemon-restart.machine.json";

/// The store these cases act in.
const NAMESPACE: &str = "store-one";

/// The instance request these cases act under.
const INSTANCE: &str = "instance-one";

/// The occurrence these cases act on.
const OCCURRENCE: u64 = 0;

/// One declared restart.
#[derive(Debug, serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct Row {
    /// What it is called.
    name: String,
    /// When the daemon went away.
    restarted_at: String,
    /// Whether the key is the same afterwards.
    same_key: bool,
    /// Whether the operation is the same afterwards.
    same_operation: bool,
    /// Why.
    why: String,
}

/// Returns every declared restart.
fn rows() -> Vec<Row> {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(FIXTURE);
    let text = std::fs::read_to_string(&path)
        .unwrap_or_else(|failure| panic!("{} could not be read: {failure}", path.display()));
    text.lines()
        .filter(|line| !line.trim().is_empty() && !line.starts_with('#'))
        .map(|line| serde_json::from_str(line).expect("every row reads"))
        .collect()
}

/// Returns the key this occurrence derives.
fn key() -> String {
    workflow_effect_operation_key(NAMESPACE, INSTANCE, OCCURRENCE, "")
        .expect("the occurrence derives a key")
}

#[test]
fn a_restart_at_any_moment_leaves_the_key_and_the_operation_alone() {
    let before = key();
    for row in rows() {
        assert!(row.same_key, "{}: {}", row.name, row.why);
        assert!(row.same_operation, "{}: {}", row.name, row.why);
        assert_eq!(key(), before, "{} derived another key after a restart", row.name);
    }
}

#[test]
fn every_moment_a_daemon_can_go_away_is_covered_once() {
    let declared = rows();
    let moments: std::collections::BTreeSet<String> =
        declared.iter().map(|row| row.restarted_at.clone()).collect();
    assert_eq!(moments.len(), declared.len(), "a moment is covered twice");
    for moment in ["before", "waiting", "after"] {
        assert!(moments.contains(moment), "no case restarts {moment} the call");
    }
}

#[test]
fn a_restart_is_transparent_inside_the_handler_deadline_and_not_beyond_it() {
    assert_eq!(
        LEAST_HANDLER_TIMEOUT_MILLISECONDS.min(MOST_HANDLER_TIMEOUT_MILLISECONDS),
        LEAST_HANDLER_TIMEOUT_MILLISECONDS,
        "a deadline that admitted nothing would make every restart visible"
    );
    assert_ne!(LEAST_HANDLER_TIMEOUT_MILLISECONDS, MOST_HANDLER_TIMEOUT_MILLISECONDS);
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(MACHINE);
    let machine: Value =
        serde_json::from_str(&std::fs::read_to_string(&path).expect("it is committed"))
            .expect("the machine reads");
    let retry = &machine["states"]["replicating"]["on"]["mcp_error"];
    assert_eq!(
        retry["effect"].as_str(),
        Some("replicate"),
        "a call that outlived the deadline is retried, and the retry finds the same operation"
    );
}

#[test]
fn the_machine_has_no_state_for_a_daemon_and_therefore_none_to_get_wrong() {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(MACHINE);
    let rendered = std::fs::read_to_string(&path).expect("it is committed");
    for named in ["daemon", "restart", "reconnect"] {
        assert!(
            !rendered.contains(&format!("\"{named}\"")),
            "the machine models {named}, which is not its business"
        );
    }
}
