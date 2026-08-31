//! What this product insists on before it acts on a handler table.
//!
//! Each case is one thing that would otherwise be discovered at the first call,
//! by which time the workflow has started and the mistake costs an operation
//! rather than a validation.
//!
//! The division of labour is asserted as much as the rules: the executor owns
//! whether a table is a valid table, and this owns whether the tool exists,
//! whether the identity is the kind that tool has, and whether the executable
//! named can actually be run here.

use std::collections::BTreeSet;

use serde_json::json;
use slingshot_command_line::model_context_protocol::tool_catalog::{Provenance, derive};
use slingshot_development::finite_state_machine_handler_validation::{
    ADVANCE_MEMBERS, Handler, HandlerRefusal, LEAST_HANDLER_TIMEOUT_MILLISECONDS,
    MOST_HANDLER_TIMEOUT_MILLISECONDS, MOST_RETRY_ATTEMPTS, OPERATION_KEY_MEMBER,
    REFUSED_ARGUMENTS, REFUSED_MAINTENANCE_MEMBERS, RETRY_MEMBERS, ToolKind, kind_of,
    require_actionable,
};

/// The key a registry command handler passes.
const KEY: &str = "slingshot-workflow-effect-1-\
     0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";

/// A handler deadline inside its bounds.
const DEADLINE: u64 = 30_000;

/// How many attempts a handler declares.
const ATTEMPTS: u64 = 3;

/// Returns every tool this build offers.
fn offered() -> BTreeSet<String> {
    derive(&Provenance::recomputed())
        .expect("this build's provenance agrees with itself")
        .into_iter()
        .map(|tool| tool.name)
        .collect()
}

/// Returns an executable this machine really can run.
fn runnable() -> String {
    std::env::current_exe().expect("this test binary has a path").to_string_lossy().into_owned()
}

/// Returns one handler for `tool`, with everything spelled out.
fn handler(tool: &str, arguments: serde_json::Value) -> Handler {
    Handler {
        arguments,
        effect: "one".to_owned(),
        argv: vec![runnable(), "--json".to_owned()],
        kind: "mcp".to_owned(),
        on_failed: "failed".to_owned(),
        on_ok: "succeeded".to_owned(),
        retry: json!({
            "attempts": ATTEMPTS,
            "backoff_ms": 500,
            "initial_delay_ms": 500,
            "maximum_delay_ms": 5_000,
        }),
        timeout_ms: DEADLINE,
        tool: tool.to_owned(),
        advances: vec![json!({ "payload": {}, "stamps": [] })],
    }
}

#[test]
fn a_registry_command_handler_that_spells_everything_out_is_actionable() {
    let held = handler("replicate_content", json!({ OPERATION_KEY_MEMBER: KEY }));
    assert_eq!(require_actionable(&held, &offered()), Ok(ToolKind::RegistryCommand));
}

#[test]
fn a_registry_command_without_a_key_is_refused_before_anything_runs() {
    let held = handler("replicate_content", json!({}));
    assert_eq!(
        require_actionable(&held, &offered()),
        Err(HandlerRefusal::KeyAbsent("replicate_content".to_owned())),
        "a rerun of that occurrence is the same operation only with a key"
    );
}

#[test]
fn a_maintenance_control_that_carries_an_operation_identity_is_refused() {
    for member in REFUSED_MAINTENANCE_MEMBERS {
        let held = handler("maintenance-apply", json!({ *member: "held" }));
        assert_eq!(
            require_actionable(&held, &offered()),
            Err(HandlerRefusal::IdentityInvented {
                member: (*member).to_owned(),
                named: "maintenance-apply".to_owned(),
            }),
            "{member} invents an identity a maintenance control has none of"
        );
    }
    let keyless = handler("maintenance-apply", json!({ "reviewed_manifest_digest": "held" }));
    assert_eq!(require_actionable(&keyless, &offered()), Ok(ToolKind::MaintenanceControl));
}

#[test]
fn a_tool_this_build_does_not_offer_is_refused_rather_than_attempted() {
    let held = handler("tools/invent", json!({ OPERATION_KEY_MEMBER: KEY }));
    assert_eq!(
        require_actionable(&held, &offered()),
        Err(HandlerRefusal::ToolUnknown("tools/invent".to_owned()))
    );
    assert_eq!(
        kind_of("operation-status", &offered()),
        Ok(ToolKind::Observation),
        "an observation starts nothing and needs no key"
    );
}

#[test]
fn an_executable_this_machine_cannot_run_is_refused_by_this_side() {
    let mut held = handler("replicate_content", json!({ OPERATION_KEY_MEMBER: KEY }));
    held.argv = vec!["fsm".to_owned()];
    let refusal = require_actionable(&held, &offered()).expect_err("a relative path names nothing");
    assert!(matches!(refusal, HandlerRefusal::ExecutableUnusable { .. }), "{refusal:?}");

    held.argv = Vec::new();
    assert!(matches!(
        require_actionable(&held, &offered()),
        Err(HandlerRefusal::ExecutableUnusable { .. })
    ));
}

#[test]
fn every_retry_member_and_every_advance_member_is_written_down() {
    for member in RETRY_MEMBERS {
        let mut held = handler("replicate_content", json!({ OPERATION_KEY_MEMBER: KEY }));
        held.retry.as_object_mut().expect("the retry is an object").remove(*member);
        assert_eq!(
            require_actionable(&held, &offered()),
            Err(HandlerRefusal::LeftToADefault(format!("retry.{member}"))),
            "two places deciding what an omitted {member} means would disagree"
        );
    }
    for member in ADVANCE_MEMBERS {
        let mut held = handler("replicate_content", json!({ OPERATION_KEY_MEMBER: KEY }));
        held.advances = vec![json!({})];
        let refusal = require_actionable(&held, &offered()).expect_err("an advance is spelled out");
        assert!(
            matches!(refusal, HandlerRefusal::LeftToADefault(ref named) if named.contains(member)
                || named.contains("payload")),
            "{member}: {refusal:?}"
        );
    }
}

#[test]
fn a_deadline_or_an_attempt_count_outside_its_bound_is_refused() {
    for held in [LEAST_HANDLER_TIMEOUT_MILLISECONDS - 1, MOST_HANDLER_TIMEOUT_MILLISECONDS + 1] {
        let mut handler = handler("replicate_content", json!({ OPERATION_KEY_MEMBER: KEY }));
        handler.timeout_ms = held;
        assert_eq!(
            require_actionable(&handler, &offered()),
            Err(HandlerRefusal::OutsideItsBound { held, named: "timeout_ms".to_owned() })
        );
    }
    for held in [0, MOST_RETRY_ATTEMPTS + 1] {
        let mut handler = handler("replicate_content", json!({ OPERATION_KEY_MEMBER: KEY }));
        handler.retry["attempts"] = json!(held);
        assert_eq!(
            require_actionable(&handler, &offered()),
            Err(HandlerRefusal::OutsideItsBound { held, named: "retry.attempts".to_owned() })
        );
    }
    let mut exactly = handler("replicate_content", json!({ OPERATION_KEY_MEMBER: KEY }));
    exactly.timeout_ms = LEAST_HANDLER_TIMEOUT_MILLISECONDS;
    require_actionable(&exactly, &offered()).expect("the bound itself is inside it");
}

#[test]
fn a_wait_time_from_a_handler_is_refused_because_the_executor_owns_that() {
    for refused in REFUSED_ARGUMENTS {
        let held =
            handler("replicate_content", json!({ OPERATION_KEY_MEMBER: KEY, *refused: 1_000 }));
        assert_eq!(
            require_actionable(&held, &offered()),
            Err(HandlerRefusal::ArgumentRefused((*refused).to_owned())),
            "a second timer would make one of them a lie"
        );
    }
}
