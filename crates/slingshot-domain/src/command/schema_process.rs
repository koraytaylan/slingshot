//! Argument and result shapes for workflows and Sling jobs.
//!
//! One arm per command and role, reached from the dispatch in `schema`. The arms
//! are grouped only so that no single match exceeds the complexity this
//! repository allows; the groups carry no meaning beyond that, which is why they
//! are numbered rather than named after something they are not.
//!
//! What a schema checks is bounded on purpose: types, closed and required
//! members, literal discriminators, counts, and ranges. Serialized member order,
//! raw spelling, minimal integer tokens, and the lexical order of set-like arrays
//! are the byte contract's, and nothing here is offered as proof of them.

use serde_json::{Value, json};

use crate::command::command_identity::CommandContract;
use crate::command::schema::{
    SchemaRole, bounded_string, closed, count, listing_page, nonempty_string, repository_path,
    result_window,
};

/// Returns the body one command role declares, when this leaf declares it.
pub fn body(wire_name: &str, role: SchemaRole, limits: &CommandContract) -> Option<Value> {
    group_1(wire_name, role, limits)
        .or_else(|| group_2(wire_name, role, limits))
        .or_else(|| group_3(wire_name, role, limits))
}

/// Returns the body one of `cancel_sling_job` through `inspect_sling_job` declares.
fn group_1(wire_name: &str, role: SchemaRole, limits: &CommandContract) -> Option<Value> {
    let body = match (wire_name, role) {
        ("cancel_sling_job", SchemaRole::Arguments) => json!({
            "type": "object",
            "additionalProperties": false,
            "required": [
                "job_identifier",
            ],
            "properties": {
                "job_identifier": nonempty_string(limits.limit("maximum_sling_job_identifier_bytes")),
            },
        }),
        ("cancel_sling_job", SchemaRole::Result) => json!({
            "type": "object",
            "additionalProperties": false,
            "required": [
                "job_identifier",
                "observed_state",
            ],
            "properties": {
                "job_identifier": nonempty_string(limits.limit("maximum_sling_job_identifier_bytes")),
                "observed_state": closed(&["active", "cancelled", "dropped", "error", "queued", "succeeded"]),
            },
        }),
        ("find_sling_jobs", SchemaRole::Arguments) => json!({
            "type": "object",
            "additionalProperties": false,
            "required": [
                "states",
            ],
            "properties": {
                "result_window": result_window(limits),
                "states": {
                    "type": "array",
                    "minItems": 1,
                    "uniqueItems": true,
                    "maxItems": limits.limit("maximum_sling_job_states"),
                    "items": closed(&["active", "cancelled", "dropped", "error", "queued", "succeeded"]),
                },
                "topic": nonempty_string(limits.limit("maximum_sling_job_topic_bytes")),
            },
        }),
        ("find_sling_jobs", SchemaRole::Result) => listing_page(
            limits,
            json!({
                "type": "object",
                "additionalProperties": false,
                "required": [
                    "job_identifier",
                    "retry_count",
                    "state",
                    "topic",
                ],
                "properties": {
                    "job_identifier": nonempty_string(limits.limit("maximum_sling_job_identifier_bytes")),
                    "queue_name": nonempty_string(limits.limit("maximum_sling_job_queue_name_bytes")),
                    "retry_count": count(),
                    "state": closed(&["active", "cancelled", "dropped", "error", "queued", "succeeded"]),
                    "topic": nonempty_string(limits.limit("maximum_sling_job_topic_bytes")),
                },
            }),
        ),
        ("find_workflow_instances", SchemaRole::Arguments) => json!({
            "type": "object",
            "additionalProperties": false,
            "required": [
                "states",
            ],
            "properties": {
                "model_identifier": nonempty_string(limits.limit("maximum_workflow_model_identifier_bytes")),
                "payload_prefix": repository_path(limits),
                "result_window": result_window(limits),
                "states": {
                    "type": "array",
                    "minItems": 1,
                    "uniqueItems": true,
                    "maxItems": limits.limit("maximum_workflow_instance_states"),
                    "items": closed(&["aborted", "completed", "running", "suspended", "stale"]),
                },
            },
        }),
        ("find_workflow_instances", SchemaRole::Result) => listing_page(
            limits,
            json!({
                "type": "object",
                "additionalProperties": false,
                "required": [
                    "instance_identifier",
                    "model_identifier",
                    "payload_path",
                    "state",
                ],
                "properties": {
                    "instance_identifier": nonempty_string(limits.limit("maximum_workflow_instance_identifier_bytes")),
                    "model_identifier": nonempty_string(limits.limit("maximum_workflow_model_identifier_bytes")),
                    "payload_path": repository_path(limits),
                    "started_at": nonempty_string(limits.limit("maximum_property_string_bytes")),
                    "state": closed(&["aborted", "completed", "running", "suspended", "stale"]),
                },
            }),
        ),
        ("inspect_sling_job", SchemaRole::Arguments) => json!({
            "type": "object",
            "additionalProperties": false,
            "required": [
                "job_identifier",
            ],
            "properties": {
                "job_identifier": nonempty_string(limits.limit("maximum_sling_job_identifier_bytes")),
            },
        }),
        ("inspect_sling_job", SchemaRole::Result) => json!({
            "type": "object",
            "additionalProperties": false,
            "required": [
                "job_identifier",
                "maximum_retry_count",
                "property_keys",
                "retry_count",
                "state",
                "topic",
            ],
            "properties": {
                "job_identifier": nonempty_string(limits.limit("maximum_sling_job_identifier_bytes")),
                "maximum_retry_count": count(),
                "property_keys": {
                    "type": "array",
                    "uniqueItems": true,
                    "maxItems": limits.limit("maximum_sling_job_property_keys"),
                    "items": nonempty_string(limits.limit("maximum_property_name_bytes")),
                },
                "queue_name": nonempty_string(limits.limit("maximum_sling_job_queue_name_bytes")),
                "retry_count": count(),
                "state": closed(&["active", "cancelled", "dropped", "error", "queued", "succeeded"]),
                "topic": nonempty_string(limits.limit("maximum_sling_job_topic_bytes")),
            },
        }),
        _ => return None,
    };
    Some(body)
}

/// Returns the body one of `inspect_workflow_instance` through `set_workflow_instance_suspension` declares.
fn group_2(wire_name: &str, role: SchemaRole, limits: &CommandContract) -> Option<Value> {
    let body = match (wire_name, role) {
        ("inspect_workflow_instance", SchemaRole::Arguments) => json!({
            "type": "object",
            "additionalProperties": false,
            "required": [
                "instance_identifier",
            ],
            "properties": {
                "instance_identifier": nonempty_string(limits.limit("maximum_workflow_instance_identifier_bytes")),
            },
        }),
        ("inspect_workflow_instance", SchemaRole::Result) => json!({
            "type": "object",
            "additionalProperties": false,
            "required": [
                "instance_identifier",
                "model_identifier",
                "payload_path",
                "state",
                "work_items",
            ],
            "properties": {
                "instance_identifier": nonempty_string(limits.limit("maximum_workflow_instance_identifier_bytes")),
                "model_identifier": nonempty_string(limits.limit("maximum_workflow_model_identifier_bytes")),
                "payload_path": repository_path(limits),
                "state": closed(&["aborted", "completed", "running", "suspended", "stale"]),
                "work_items": {
                    "type": "array",
                    "maxItems": limits.limit("maximum_workflow_work_items"),
                    "items": {
                        "type": "object",
                        "additionalProperties": false,
                        "required": [
                            "node_title",
                            "work_item_identifier",
                        ],
                        "properties": {
                            "assignee": nonempty_string(limits.limit("maximum_authorizable_identifier_bytes")),
                            "node_title": bounded_string(limits.limit("maximum_page_title_bytes")),
                            "work_item_identifier": nonempty_string(limits.limit("maximum_work_item_identifier_bytes")),
                        },
                    },
                },
            },
        }),
        ("list_sling_job_queues", SchemaRole::Arguments) => json!({
            "type": "object",
            "additionalProperties": false,
            "required": [],
            "properties": {
                "result_window": result_window(limits),
            },
        }),
        ("list_sling_job_queues", SchemaRole::Result) => listing_page(
            limits,
            json!({
                "type": "object",
                "additionalProperties": false,
                "required": [
                    "active_job_count",
                    "queue_name",
                    "queued_job_count",
                    "state",
                ],
                "properties": {
                    "active_job_count": {
                        "type": "integer",
                        "minimum": 0,
                        "maximum": limits.limit("maximum_operational_candidate_records"),
                    },
                    "queue_name": nonempty_string(limits.limit("maximum_sling_job_queue_name_bytes")),
                    "queued_job_count": {
                        "type": "integer",
                        "minimum": 0,
                        "maximum": limits.limit("maximum_operational_candidate_records"),
                    },
                    "state": closed(&["running", "suspended"]),
                },
            }),
        ),
        ("list_workflow_models", SchemaRole::Arguments) => json!({
            "type": "object",
            "additionalProperties": false,
            "required": [],
            "properties": {
                "result_window": result_window(limits),
                "title_prefix": bounded_string(limits.limit("maximum_page_title_bytes")),
            },
        }),
        ("list_workflow_models", SchemaRole::Result) => listing_page(
            limits,
            json!({
                "type": "object",
                "additionalProperties": false,
                "required": [
                    "model_identifier",
                    "title",
                ],
                "properties": {
                    "model_identifier": nonempty_string(limits.limit("maximum_workflow_model_identifier_bytes")),
                    "title": bounded_string(limits.limit("maximum_page_title_bytes")),
                    "version": nonempty_string(limits.limit("maximum_bundle_version_bytes")),
                },
            }),
        ),
        ("set_workflow_instance_suspension", SchemaRole::Arguments) => json!({
            "type": "object",
            "additionalProperties": false,
            "required": [
                "instance_identifier",
                "requested_state",
            ],
            "properties": {
                "instance_identifier": nonempty_string(limits.limit("maximum_workflow_instance_identifier_bytes")),
                "requested_state": closed(&["running", "suspended"]),
            },
        }),
        ("set_workflow_instance_suspension", SchemaRole::Result) => json!({
            "type": "object",
            "additionalProperties": false,
            "required": [
                "instance_identifier",
                "observed_state",
            ],
            "properties": {
                "instance_identifier": nonempty_string(limits.limit("maximum_workflow_instance_identifier_bytes")),
                "observed_state": closed(&["aborted", "completed", "running", "suspended", "stale"]),
            },
        }),
        _ => return None,
    };
    Some(body)
}

/// Returns the body one of `start_workflow` through `terminate_workflow_instance` declares.
fn group_3(wire_name: &str, role: SchemaRole, limits: &CommandContract) -> Option<Value> {
    let body = match (wire_name, role) {
        ("start_workflow", SchemaRole::Arguments) => json!({
            "type": "object",
            "additionalProperties": false,
            "required": [
                "model_identifier",
                "payload_path",
            ],
            "properties": {
                "comment": bounded_string(limits.limit("maximum_workflow_comment_bytes")),
                "metadata": {
                    "type": "object",
                    "maxProperties": limits.limit("maximum_workflow_metadata_entries"),
                    "additionalProperties": bounded_string(limits.limit("maximum_property_string_bytes")),
                },
                "model_identifier": nonempty_string(limits.limit("maximum_workflow_model_identifier_bytes")),
                "payload_path": repository_path(limits),
                "title": bounded_string(limits.limit("maximum_page_title_bytes")),
            },
        }),
        ("start_workflow", SchemaRole::Result) => json!({
            "type": "object",
            "additionalProperties": false,
            "required": [
                "instance_identifier",
                "model_identifier",
                "state",
            ],
            "properties": {
                "instance_identifier": nonempty_string(limits.limit("maximum_workflow_instance_identifier_bytes")),
                "model_identifier": nonempty_string(limits.limit("maximum_workflow_model_identifier_bytes")),
                "state": closed(&["aborted", "completed", "running", "suspended", "stale"]),
            },
        }),
        ("terminate_workflow_instance", SchemaRole::Arguments) => json!({
            "type": "object",
            "additionalProperties": false,
            "required": [
                "instance_identifier",
            ],
            "properties": {
                "instance_identifier": nonempty_string(limits.limit("maximum_workflow_instance_identifier_bytes")),
            },
        }),
        ("terminate_workflow_instance", SchemaRole::Result) => json!({
            "type": "object",
            "additionalProperties": false,
            "required": [
                "instance_identifier",
                "observed_state",
            ],
            "properties": {
                "instance_identifier": nonempty_string(limits.limit("maximum_workflow_instance_identifier_bytes")),
                "observed_state": closed(&["aborted", "completed", "running", "suspended", "stale"]),
            },
        }),
        _ => return None,
    };
    Some(body)
}
