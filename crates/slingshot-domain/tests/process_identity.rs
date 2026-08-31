//! Assertions for the values the workflow and Sling job families address.
//!
//! The asymmetry is deliberate and is what these assertions pin. A model or an
//! instance identifier is opaque, so a path-shaped one and a plainly non-path
//! one are both accepted; a topic has a grammar, so a topic that a Sling
//! deployment would refuse is refused here.

use slingshot_domain::command::command_identity::CommandContract;
use slingshot_domain::command::operational_listing::ListingResultFailure;
use slingshot_domain::command::process_identity::{
    RequestedSlingJobStates, RequestedWorkflowInstanceStates, SLING_JOB_QUEUE_STATE_COUNT,
    SLING_JOB_STATE_COUNT, SlingJobIdentifier, SlingJobQueueName, SlingJobQueueState,
    SlingJobState, SlingJobTopic, WORKFLOW_INSTANCE_STATE_COUNT, WorkItemIdentifier,
    WorkflowInstanceIdentifier, WorkflowInstanceState, WorkflowModelIdentifier,
};

/// Returns one limit by name.
fn limit(name: &str) -> usize {
    usize::try_from(CommandContract::embedded().limit(name)).expect("the bound fits")
}

#[test]
fn a_model_or_instance_identifier_is_opaque_rather_than_a_path() {
    for accepted in [
        "/var/workflow/models/dam/update_asset/jcr:content/model",
        "/conf/global/settings/workflow/models/request-for-activation",
        "request-for-activation",
        "model:17",
    ] {
        assert!(WorkflowModelIdentifier::parse(accepted).is_ok(), "{accepted} was refused");
        assert!(WorkflowInstanceIdentifier::parse(accepted).is_ok(), "{accepted} was refused");
        assert!(WorkItemIdentifier::parse(accepted).is_ok(), "{accepted} was refused");
    }
    for refused in ["", " model", "model ", "mo\u{0}del"] {
        assert!(WorkflowModelIdentifier::parse(refused).is_err(), "{refused:?} was accepted");
        assert!(WorkflowInstanceIdentifier::parse(refused).is_err(), "{refused:?} was accepted");
        assert!(WorkItemIdentifier::parse(refused).is_err(), "{refused:?} was accepted");
    }
}

#[test]
fn every_process_identifier_is_accepted_at_its_bound_and_refused_one_byte_past_it() {
    for (bound_name, parse) in [
        (
            "maximum_workflow_model_identifier_bytes",
            (|value: &str| WorkflowModelIdentifier::parse(value).is_ok()) as fn(&str) -> bool,
        ),
        ("maximum_workflow_instance_identifier_bytes", |value| {
            WorkflowInstanceIdentifier::parse(value).is_ok()
        }),
        ("maximum_work_item_identifier_bytes", |value| WorkItemIdentifier::parse(value).is_ok()),
        ("maximum_sling_job_identifier_bytes", |value| SlingJobIdentifier::parse(value).is_ok()),
        ("maximum_sling_job_queue_name_bytes", |value| SlingJobQueueName::parse(value).is_ok()),
        ("maximum_sling_job_topic_bytes", |value| SlingJobTopic::parse(value).is_ok()),
    ] {
        let exact = "a".repeat(limit(bound_name));
        assert!(parse(&exact), "{bound_name}: the bound itself was refused");
        assert!(!parse(&format!("{exact}a")), "{bound_name}: one byte past the bound was accepted");
    }
}

#[test]
fn a_topic_is_solidus_separated_segments_over_its_own_alphabet() {
    let topic = SlingJobTopic::parse("com/example/jobs/reindex").expect("a legal topic");
    assert_eq!(topic.segments(), vec!["com", "example", "jobs", "reindex"]);
    for accepted in ["single", "com/example", "com.example/job-1", "a_b/c.d"] {
        assert!(SlingJobTopic::parse(accepted).is_ok(), "{accepted} was refused");
    }
    for refused in
        ["", "/com/example", "com/example/", "com//example", "com/exa mple", "com/ex:ample"]
    {
        assert!(SlingJobTopic::parse(refused).is_err(), "{refused:?} was accepted");
    }
}

#[test]
fn every_closed_state_set_has_exactly_the_members_the_contract_names() {
    assert_eq!(WorkflowInstanceState::every().len(), WORKFLOW_INSTANCE_STATE_COUNT);
    assert_eq!(SlingJobState::every().len(), SLING_JOB_STATE_COUNT);
    assert_eq!(SlingJobQueueState::every().len(), SLING_JOB_QUEUE_STATE_COUNT);
    assert!(WORKFLOW_INSTANCE_STATE_COUNT <= limit("maximum_workflow_instance_states"));
    assert!(SLING_JOB_STATE_COUNT <= limit("maximum_sling_job_states"));
}

#[test]
fn an_archived_instance_is_a_state_rather_than_another_subject() {
    assert!(WorkflowInstanceState::Completed.has_ended());
    assert!(WorkflowInstanceState::Aborted.has_ended());
    assert!(!WorkflowInstanceState::Running.has_ended());
    assert!(!WorkflowInstanceState::Suspended.has_ended());
    assert!(!WorkflowInstanceState::Stale.has_ended());
}

#[test]
fn only_a_queued_or_active_job_can_still_be_cancelled() {
    assert!(SlingJobState::Queued.is_cancellable());
    assert!(SlingJobState::Active.is_cancellable());
    for ended in [SlingJobState::Succeeded, SlingJobState::Cancelled, SlingJobState::Dropped] {
        assert!(!ended.is_cancellable(), "{ended:?} was reported as cancellable");
    }
}

#[test]
fn a_requested_state_set_is_nonempty_ascending_and_distinct() {
    let asked = RequestedWorkflowInstanceStates::new(vec![
        WorkflowInstanceState::Completed,
        WorkflowInstanceState::Running,
    ])
    .expect("a legal set");
    assert!(asked.contains(WorkflowInstanceState::Running));
    assert!(!asked.contains(WorkflowInstanceState::Suspended));
    assert_eq!(
        serde_json::to_string(&asked).expect("a set serializes"),
        "[\"completed\",\"running\"]"
    );
    assert_eq!(
        RequestedWorkflowInstanceStates::new(Vec::new()),
        Err(ListingResultFailure::NotAscendingDistinct)
    );
    assert_eq!(
        RequestedSlingJobStates::new(vec![SlingJobState::Queued, SlingJobState::Active]),
        Err(ListingResultFailure::NotAscendingDistinct)
    );
    assert!(serde_json::from_str::<RequestedSlingJobStates>("[\"queued\",\"active\"]").is_err());
    assert!(serde_json::from_str::<RequestedSlingJobStates>("[]").is_err());
    assert!(serde_json::from_str::<RequestedSlingJobStates>("[\"unknown\"]").is_err());
}
