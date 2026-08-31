//! Assertions for putting content into a workflow.
//!
//! The instance identifier is the author's to mint, so nothing here compares it.
//! What is compared is the model: a result about another model is a result about
//! another request, whatever instance it carries.

use serde_json::Value;
use slingshot_domain::command::command_identity::CommandContract;
use slingshot_domain::command::process_identity::{
    WorkflowInstanceIdentifier, WorkflowInstanceState, WorkflowModelIdentifier,
};
use slingshot_domain::command::resource_mutation::MutationResultFailure;
use slingshot_domain::command::start_workflow::{
    StartWorkflowCommand, StartWorkflowFailure, StartWorkflowRefusal, StartWorkflowResult,
    WorkflowMetadata, WorkflowMetadataKey,
};

/// Commands this test reads.
const COMMANDS: &str = include_str!("fixtures/commands/start_workflow/commands.jsonl");

/// Failures this test reads.
const FAILURES: &str = include_str!("fixtures/commands/start_workflow/failures.jsonl");

/// Model every vector starts.
const MODEL: &str = "/var/workflow/models/request-for-activation/jcr:content/model";

/// Reads one row's string member.
fn text<'row>(row: &'row Value, member: &str) -> &'row str {
    row[member].as_str().unwrap_or_else(|| panic!("{member} is a string in {row}"))
}

/// Returns every row of one fixture.
fn rows(fixture: &str) -> Vec<Value> {
    fixture
        .lines()
        .map(|line| serde_json::from_str::<Value>(line).expect("every fixture line is one object"))
        .collect()
}

/// Returns one model identifier.
fn model(value: &str) -> WorkflowModelIdentifier {
    WorkflowModelIdentifier::parse(value).expect("a legal identifier")
}

/// Returns one legal request.
fn command() -> StartWorkflowCommand {
    serde_json::from_str(&format!(
        "{{\"model_identifier\":\"{MODEL}\",\"payload_path\":\"/content/example/en/report\"}}"
    ))
    .expect("a legal command")
}

#[test]
fn every_command_vector_parses_exactly_as_the_fixture_says() {
    let vectors = rows(COMMANDS);
    assert!(vectors.len() >= 6, "every document shape and every refusal");
    for row in &vectors {
        let document = text(row, "document");
        let note = text(row, "note");
        match (row["accepted"].as_bool(), serde_json::from_str::<StartWorkflowCommand>(document)) {
            (Some(true), Ok(parsed)) => {
                assert_eq!(
                    serde_json::to_string(&parsed).expect("a command serializes"),
                    document,
                    "{note}: rewritten differently"
                );
                assert_eq!(parsed.require_usable(), Ok(()), "{note}: refused as unusable");
            }
            (Some(false), Err(_)) => (),
            (_, parsed) => panic!("{note}: the command answered {parsed:?}"),
        }
    }
}

#[test]
fn the_instance_identifier_is_the_authors_and_the_model_is_the_requests() {
    let asked = command();
    let answered = StartWorkflowResult {
        instance_identifier: WorkflowInstanceIdentifier::parse("anything-the-author-minted")
            .expect("a legal identifier"),
        model_identifier: model(MODEL),
        state: WorkflowInstanceState::Running,
    };
    assert_eq!(answered.require_answers(&asked), Ok(()));
    let elsewhere = StartWorkflowResult {
        instance_identifier: WorkflowInstanceIdentifier::parse("anything-the-author-minted")
            .expect("a legal identifier"),
        model_identifier: model("/var/workflow/models/other"),
        state: WorkflowInstanceState::Running,
    };
    assert_eq!(elsewhere.require_answers(&asked), Err(MutationResultFailure::NotThisRequest));
}

#[test]
fn metadata_is_accepted_at_its_entry_bound_and_refused_one_entry_past_it() {
    let bound =
        usize::try_from(CommandContract::embedded().limit("maximum_workflow_metadata_entries"))
            .expect("the bound fits");
    let build = |count: usize| {
        let mut entries = std::collections::BTreeMap::new();
        for index in 0..count {
            entries.insert(
                WorkflowMetadataKey::parse(&format!("k{index:04}")).expect("a legal key"),
                "value".to_owned(),
            );
        }
        WorkflowMetadata::new(entries)
    };
    assert!(build(bound).is_ok(), "the bound itself was refused");
    assert!(build(bound + 1).is_err(), "one entry past the bound was accepted");
}

#[test]
fn a_comment_is_accepted_at_its_bound_and_refused_one_byte_past_it() {
    let bound =
        usize::try_from(CommandContract::embedded().limit("maximum_workflow_comment_bytes"))
            .expect("the bound fits");
    let build = |length: usize| StartWorkflowCommand {
        comment: Some("a".repeat(length)),
        metadata: None,
        model_identifier: model(MODEL),
        payload_path: slingshot_domain::command::repository_path::RepositoryPath::parse(
            "/content/example/en/report",
        )
        .expect("a legal path"),
        title: None,
    };
    assert_eq!(build(bound).require_usable(), Ok(()));
    assert_eq!(build(bound + 1).require_usable(), Err(MutationResultFailure::CountTooLarge));
}

#[test]
fn a_refusal_answers_only_the_request_that_named_its_model() {
    let refusal = StartWorkflowRefusal {
        failure: StartWorkflowFailure::PayloadNotFound,
        model_identifier: model(MODEL),
    };
    assert_eq!(refusal.require_answers(&command()), Ok(()));
    let elsewhere = StartWorkflowRefusal {
        failure: StartWorkflowFailure::PayloadNotFound,
        model_identifier: model("/var/workflow/models/other"),
    };
    assert_eq!(elsewhere.require_answers(&command()), Err(MutationResultFailure::NotThisRequest));
}

#[test]
fn every_failure_document_carries_its_members_and_says_what_it_proves() {
    let vectors = rows(FAILURES);
    assert_eq!(vectors.len(), 7, "one for each category this command allows");
    for row in &vectors {
        let note = text(row, "note");
        let document = text(row, "document");
        let refusal: StartWorkflowRefusal =
            serde_json::from_str(document).unwrap_or_else(|failure| panic!("{note}: {failure}"));
        assert_eq!(
            serde_json::to_string(&refusal).expect("a refusal serializes"),
            document,
            "{note}: rewritten differently"
        );
        assert_eq!(
            refusal.proves_no_effect(),
            row["proves_no_effect"].as_bool().expect("a verdict"),
            "{note}"
        );
    }
    assert!(
        serde_json::from_str::<StartWorkflowRefusal>(r#"{"failure":"model_not_found","model_identifier":"/var/workflow/models/request-for-activation/jcr:content/model","extra":1}"#).is_err(),
        "a surplus member was accepted"
    );
}
