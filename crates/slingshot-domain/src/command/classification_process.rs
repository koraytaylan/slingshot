//! Workflows and Sling jobs.
//!
//! Starting a workflow is a write that replaces nothing; terminating one,
//! suspending one, and cancelling a job each end something that was in effect,
//! which is what destructive means here.

use crate::command::catalog::{
    AccessClassification, DestructiveClassification, IntrinsicIdempotencyClassification,
};
use crate::command::classification::ClassificationRow;

/// Cancel a Sling job.
pub const CANCEL_SLING_JOB: ClassificationRow = ClassificationRow {
    wire_name: "cancel_sling_job",
    title: "Cancel a Sling job",
    description: "Withdraws one queued or running job and reports the state observed \
                  afterwards.",
    access: AccessClassification::Write,
    destructive: DestructiveClassification::Destructive,
    intrinsic_idempotency: IntrinsicIdempotencyClassification::NotIntrinsicallyIdempotent,
    result_bytes_limit: "maximum_mutation_success_result_bytes",
    failure_categories: &[
        "job_not_found",
        "job_not_cancellable",
        "platform_control_rejected",
        "platform_control_outcome_unknown",
    ],
    discovery: false,
};

/// Find Sling jobs.
pub const FIND_SLING_JOBS: ClassificationRow = ClassificationRow {
    wire_name: "find_sling_jobs",
    title: "Find Sling jobs",
    description: "Finds jobs by topic and by a nonempty set of states, and reports each \
                  one's queue and retry count.",
    access: AccessClassification::Read,
    destructive: DestructiveClassification::NonDestructive,
    intrinsic_idempotency: IntrinsicIdempotencyClassification::IntrinsicallyIdempotent,
    result_bytes_limit: "maximum_operational_listing_result_bytes",
    failure_categories: &["job_inventory_failed"],
    discovery: true,
};

/// Find workflow instances.
pub const FIND_WORKFLOW_INSTANCES: ClassificationRow = ClassificationRow {
    wire_name: "find_workflow_instances",
    title: "Find workflow instances",
    description: "Finds workflow instances by model, payload anchor, and a nonempty set of \
                  states, archived ones included.",
    access: AccessClassification::Read,
    destructive: DestructiveClassification::NonDestructive,
    intrinsic_idempotency: IntrinsicIdempotencyClassification::IntrinsicallyIdempotent,
    result_bytes_limit: "maximum_operational_listing_result_bytes",
    failure_categories: &["workflow_inventory_failed"],
    discovery: true,
};

/// Inspect a Sling job.
pub const INSPECT_SLING_JOB: ClassificationRow = ClassificationRow {
    wire_name: "inspect_sling_job",
    title: "Inspect a Sling job",
    description: "Reports one job's topic, state, queue, retry counts, and its property keys \
                  in ascending order, never a value.",
    access: AccessClassification::Read,
    destructive: DestructiveClassification::NonDestructive,
    intrinsic_idempotency: IntrinsicIdempotencyClassification::IntrinsicallyIdempotent,
    result_bytes_limit: "maximum_operational_inspection_result_bytes",
    failure_categories: &["job_not_found", "job_inventory_failed", "result_budget_exceeded"],
    discovery: false,
};

/// Inspect a workflow instance.
pub const INSPECT_WORKFLOW_INSTANCE: ClassificationRow = ClassificationRow {
    wire_name: "inspect_workflow_instance",
    title: "Inspect a workflow instance",
    description: "Reports one instance's model, payload, state, and open work items with the \
                  authorizable each is assigned to.",
    access: AccessClassification::Read,
    destructive: DestructiveClassification::NonDestructive,
    intrinsic_idempotency: IntrinsicIdempotencyClassification::IntrinsicallyIdempotent,
    result_bytes_limit: "maximum_operational_inspection_result_bytes",
    failure_categories: &[
        "instance_not_found",
        "instance_access_denied",
        "workflow_inventory_failed",
        "result_budget_exceeded",
    ],
    discovery: false,
};

/// List Sling job queues.
pub const LIST_SLING_JOB_QUEUES: ClassificationRow = ClassificationRow {
    wire_name: "list_sling_job_queues",
    title: "List Sling job queues",
    description: "Reports every job queue's state with its active and queued job counts kept \
                  separate.",
    access: AccessClassification::Read,
    destructive: DestructiveClassification::NonDestructive,
    intrinsic_idempotency: IntrinsicIdempotencyClassification::IntrinsicallyIdempotent,
    result_bytes_limit: "maximum_operational_listing_result_bytes",
    failure_categories: &["job_inventory_failed"],
    discovery: true,
};

/// List workflow models.
pub const LIST_WORKFLOW_MODELS: ClassificationRow = ClassificationRow {
    wire_name: "list_workflow_models",
    title: "List workflow models",
    description: "Reports the workflow models a deployment has, filtered by title prefix.",
    access: AccessClassification::Read,
    destructive: DestructiveClassification::NonDestructive,
    intrinsic_idempotency: IntrinsicIdempotencyClassification::IntrinsicallyIdempotent,
    result_bytes_limit: "maximum_operational_listing_result_bytes",
    failure_categories: &["workflow_inventory_failed"],
    discovery: true,
};

/// Suspend or resume a workflow instance.
pub const SET_WORKFLOW_INSTANCE_SUSPENSION: ClassificationRow = ClassificationRow {
    wire_name: "set_workflow_instance_suspension",
    title: "Suspend or resume a workflow instance",
    description: "Holds one workflow instance or lets it advance again, and reports the \
                  state observed afterwards.",
    access: AccessClassification::Write,
    destructive: DestructiveClassification::Destructive,
    intrinsic_idempotency: IntrinsicIdempotencyClassification::NotIntrinsicallyIdempotent,
    result_bytes_limit: "maximum_mutation_success_result_bytes",
    failure_categories: &[
        "instance_not_found",
        "instance_access_denied",
        "instance_not_suspendable",
        "platform_control_rejected",
        "platform_control_outcome_unknown",
    ],
    discovery: false,
};

/// Start a workflow.
pub const START_WORKFLOW: ClassificationRow = ClassificationRow {
    wire_name: "start_workflow",
    title: "Start a workflow",
    description: "Starts one workflow model against one payload with the metadata that model \
                  reads.",
    access: AccessClassification::Write,
    destructive: DestructiveClassification::NonDestructive,
    intrinsic_idempotency: IntrinsicIdempotencyClassification::NotIntrinsicallyIdempotent,
    result_bytes_limit: "maximum_mutation_success_result_bytes",
    failure_categories: &[
        "model_not_found",
        "model_invalid",
        "payload_not_found",
        "payload_access_denied",
        "metadata_rejected",
        "platform_control_rejected",
        "platform_control_outcome_unknown",
    ],
    discovery: false,
};

/// Terminate a workflow instance.
pub const TERMINATE_WORKFLOW_INSTANCE: ClassificationRow = ClassificationRow {
    wire_name: "terminate_workflow_instance",
    title: "Terminate a workflow instance",
    description: "Ends one workflow instance and reports the state the author observed \
                  afterwards.",
    access: AccessClassification::Write,
    destructive: DestructiveClassification::Destructive,
    intrinsic_idempotency: IntrinsicIdempotencyClassification::NotIntrinsicallyIdempotent,
    result_bytes_limit: "maximum_mutation_success_result_bytes",
    failure_categories: &[
        "instance_not_found",
        "instance_access_denied",
        "instance_not_terminable",
        "platform_control_rejected",
        "platform_control_outcome_unknown",
    ],
    discovery: false,
};
