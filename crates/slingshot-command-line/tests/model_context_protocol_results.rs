//! What a client receives for one outcome, against what a command line receives.
//!
//! One document, two surfaces. The suite compares the bytes rather than the
//! shapes, because a projection that produced an equivalent document with
//! members in another order would still be a second rendering, and the second
//! rendering is what drifts.

use serde_json::json;

use slingshot_command_line::machine_outcome_envelope::{Interruption, MachineOutcomeEnvelope};
use slingshot_command_line::machine_readable_renderer;
use slingshot_command_line::model_context_protocol::result_projection::{
    FAILING_TAGS, ProjectionRefusal, SUPPRESSED_TAGS, observation_succeeded, projected,
};

/// The revision an operation in these cases stands at.
const REVISION: u64 = 7;

/// Returns one receipt outcome.
fn receipt() -> MachineOutcomeEnvelope {
    MachineOutcomeEnvelope::OperationReceipt {
        operation_identifier: "one".to_owned(),
        replayed: false,
        revision: REVISION,
    }
}

/// Returns one terminal-failure outcome.
fn terminal() -> MachineOutcomeEnvelope {
    MachineOutcomeEnvelope::OperationTerminalError {
        disposition: "AuthoritativeRemoteFailure".to_owned(),
        failure: json!({ "category": "not_found" }),
        kind: "RemoteFailed".to_owned(),
    }
}

#[test]
fn the_text_and_the_structured_content_are_one_document() {
    let result = projected(&receipt(), true, Vec::new()).expect("a receipt is projected");
    let reparsed: serde_json::Value =
        serde_json::from_str(&result.text).expect("the text is the document");
    assert_eq!(reparsed, result.structured_content, "two renderings would be two documents");
}

#[test]
fn a_protocol_client_and_a_command_line_read_the_same_bytes() {
    for envelope in [receipt(), terminal()] {
        let rendered = machine_readable_renderer::render(&envelope).expect("it renders");
        let result = projected(&envelope, true, Vec::new()).expect("it projects");
        assert_eq!(result.text, rendered, "the two surfaces disagree about the same outcome");
    }
}

#[test]
fn an_attached_call_whose_operation_ended_badly_is_an_error_to_its_caller() {
    let attached = projected(&terminal(), true, Vec::new()).expect("it projects");
    assert!(attached.is_error, "the caller waiting on it is told their call failed");
    assert!(FAILING_TAGS.contains(&terminal().tag().as_str()));
}

#[test]
fn an_observation_that_reports_a_failure_has_not_itself_failed() {
    let observed = projected(&terminal(), false, Vec::new()).expect("it projects");
    assert!(!observed.is_error, "the read succeeded, and what it found is the answer");
    assert!(observation_succeeded(&observed));
    let status =
        MachineOutcomeEnvelope::OperationStatus { revision: REVISION, state: "failed".to_owned() };
    let read = projected(&status, false, Vec::new()).expect("it projects");
    assert!(!read.is_error, "a client told otherwise would retry the read rather than the work");
}

#[test]
fn an_outcome_describing_a_command_line_stopping_reaches_no_client() {
    for interruption in [
        Interruption::PreReceipt { retry_identifier: "one".to_owned() },
        Interruption::PostReceipt { operation_identifier: "one".to_owned(), revision: REVISION },
        Interruption::ArtifactTransfer {
            artifact_identifier: "one".to_owned(),
            operation_identifier: "two".to_owned(),
        },
        Interruption::MaintenanceResultTransfer {
            author_target_identity_digest: "one".to_owned(),
            maintenance_result_identifier: "two".to_owned(),
        },
    ] {
        let envelope = MachineOutcomeEnvelope::LocalApplicationError { interruption };
        let refusal = projected(&envelope, true, Vec::new())
            .expect_err("this server has no terminal for somebody to interrupt");
        assert_eq!(refusal, ProjectionRefusal::Suppressed(envelope.tag()));
        assert!(SUPPRESSED_TAGS.contains(&envelope.tag().as_str()));
    }
}

#[test]
fn a_link_is_an_affordance_beside_the_answer_rather_than_part_of_it() {
    let address = "slingshot://profiles/local/environments/author/targets/one/operations/two";
    let result =
        projected(&receipt(), true, vec![address.to_owned()]).expect("a receipt is projected");
    assert_eq!(result.resource_links, vec![address.to_owned()]);
    assert!(!result.text.contains(address), "the answer is the outcome, and the link is beside it");
}
