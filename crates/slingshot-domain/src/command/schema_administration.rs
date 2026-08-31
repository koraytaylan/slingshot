//! Argument and result shapes for authorizables and replication queues.
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
    SchemaRole, bounded_string, closed, count, listing_page, mutation_properties, nonempty_string,
    removed_property_names, repository_path, result_window,
};

/// Returns the body one command role declares, when this leaf declares it.
pub fn body(wire_name: &str, role: SchemaRole, limits: &CommandContract) -> Option<Value> {
    group_1(wire_name, role, limits)
        .or_else(|| group_2(wire_name, role, limits))
        .or_else(|| group_3(wire_name, role, limits))
        .or_else(|| group_4(wire_name, role, limits))
}

/// Returns the body one of `add_group_member` through `delete_authorizable` declares.
fn group_1(wire_name: &str, role: SchemaRole, limits: &CommandContract) -> Option<Value> {
    let body = match (wire_name, role) {
        ("add_group_member", SchemaRole::Arguments) => json!({
            "type": "object",
            "additionalProperties": false,
            "required": [
                "group_identifier",
                "member_identifier",
            ],
            "properties": {
                "group_identifier": nonempty_string(limits.limit("maximum_authorizable_identifier_bytes")),
                "member_identifier": nonempty_string(limits.limit("maximum_authorizable_identifier_bytes")),
            },
        }),
        ("add_group_member", SchemaRole::Result) => json!({
            "type": "object",
            "additionalProperties": false,
            "required": [
                "already_a_member",
                "group_identifier",
                "member_identifier",
            ],
            "properties": {
                "already_a_member": {
                    "type": "boolean",
                },
                "group_identifier": nonempty_string(limits.limit("maximum_authorizable_identifier_bytes")),
                "member_identifier": nonempty_string(limits.limit("maximum_authorizable_identifier_bytes")),
            },
        }),
        ("create_group", SchemaRole::Arguments) => json!({
            "type": "object",
            "additionalProperties": false,
            "required": [
                "authorizable_identifier",
            ],
            "properties": {
                "authorizable_identifier": nonempty_string(limits.limit("maximum_authorizable_identifier_bytes")),
                "intermediate_path": nonempty_string(limits.limit("maximum_authorizable_intermediate_path_bytes")),
                "properties": mutation_properties(limits),
            },
        }),
        ("create_group", SchemaRole::Result) => json!({
            "type": "object",
            "additionalProperties": false,
            "required": [
                "authorizable_identifier",
                "kind",
                "repository_path",
            ],
            "properties": {
                "authorizable_identifier": nonempty_string(limits.limit("maximum_authorizable_identifier_bytes")),
                "kind": json!({"const": "group"}),
                "repository_path": repository_path(limits),
            },
        }),
        ("create_user", SchemaRole::Arguments) => json!({
            "type": "object",
            "additionalProperties": false,
            "required": [
                "authorizable_identifier",
            ],
            "properties": {
                "authorizable_identifier": nonempty_string(limits.limit("maximum_authorizable_identifier_bytes")),
                "intermediate_path": nonempty_string(limits.limit("maximum_authorizable_intermediate_path_bytes")),
                "properties": mutation_properties(limits),
            },
        }),
        ("create_user", SchemaRole::Result) => json!({
            "type": "object",
            "additionalProperties": false,
            "required": [
                "authorizable_identifier",
                "kind",
                "repository_path",
            ],
            "properties": {
                "authorizable_identifier": nonempty_string(limits.limit("maximum_authorizable_identifier_bytes")),
                "kind": json!({"const": "user"}),
                "repository_path": repository_path(limits),
            },
        }),
        ("delete_authorizable", SchemaRole::Arguments) => json!({
            "type": "object",
            "additionalProperties": false,
            "required": [
                "authorizable_identifier",
                "expected_kind",
            ],
            "properties": {
                "authorizable_identifier": nonempty_string(limits.limit("maximum_authorizable_identifier_bytes")),
                "expected_kind": closed(&["group", "user"]),
            },
        }),
        ("delete_authorizable", SchemaRole::Result) => json!({
            "type": "object",
            "additionalProperties": false,
            "required": [
                "authorizable_identifier",
                "kind",
                "repository_path",
            ],
            "properties": {
                "authorizable_identifier": nonempty_string(limits.limit("maximum_authorizable_identifier_bytes")),
                "kind": closed(&["group", "user"]),
                "repository_path": repository_path(limits),
            },
        }),
        _ => return None,
    };
    Some(body)
}

/// Returns the body one of `flush_replication_queue` through `list_group_members` declares.
fn group_2(wire_name: &str, role: SchemaRole, limits: &CommandContract) -> Option<Value> {
    let body = match (wire_name, role) {
        ("flush_replication_queue", SchemaRole::Arguments) => json!({
            "type": "object",
            "additionalProperties": false,
            "required": [
                "agent_identifier",
            ],
            "properties": {
                "agent_identifier": nonempty_string(limits.limit("maximum_replication_agent_identifier_bytes")),
                "expected_entry_count": {
                    "type": "integer",
                    "minimum": 0,
                    "maximum": limits.limit("maximum_replication_queue_entries"),
                },
            },
        }),
        ("flush_replication_queue", SchemaRole::Result) => json!({
            "type": "object",
            "additionalProperties": false,
            "required": [
                "agent_identifier",
                "removed_entry_count",
            ],
            "properties": {
                "agent_identifier": nonempty_string(limits.limit("maximum_replication_agent_identifier_bytes")),
                "removed_entry_count": {
                    "type": "integer",
                    "minimum": 0,
                    "maximum": limits.limit("maximum_replication_queue_entries"),
                },
            },
        }),
        ("inspect_replication_agent", SchemaRole::Arguments) => json!({
            "type": "object",
            "additionalProperties": false,
            "required": [
                "agent_identifier",
            ],
            "properties": {
                "agent_identifier": nonempty_string(limits.limit("maximum_replication_agent_identifier_bytes")),
            },
        }),
        ("inspect_replication_agent", SchemaRole::Result) => json!({
            "type": "object",
            "additionalProperties": false,
            "required": [
                "agent_identifier",
                "enabled",
                "queue_blocked",
                "queued_entry_count",
                "repository_path",
                "retry_delay_milliseconds",
                "title",
                "transport_kind",
            ],
            "properties": {
                "agent_identifier": nonempty_string(limits.limit("maximum_replication_agent_identifier_bytes")),
                "enabled": {
                    "type": "boolean",
                },
                "queue_blocked": {
                    "type": "boolean",
                },
                "queued_entry_count": {
                    "type": "integer",
                    "minimum": 0,
                    "maximum": limits.limit("maximum_replication_queue_entries"),
                },
                "repository_path": repository_path(limits),
                "retry_delay_milliseconds": count(),
                "title": bounded_string(limits.limit("maximum_page_title_bytes")),
                "transport_kind": closed(&["flush", "publish", "reverse", "static"]),
            },
        }),
        ("inspect_replication_queue", SchemaRole::Arguments) => json!({
            "type": "object",
            "additionalProperties": false,
            "required": [
                "agent_identifier",
            ],
            "properties": {
                "agent_identifier": nonempty_string(limits.limit("maximum_replication_agent_identifier_bytes")),
                "result_window": result_window(limits),
            },
        }),
        ("inspect_replication_queue", SchemaRole::Result) => json!({
            "type": "object",
            "additionalProperties": false,
            "required": [
                "blocked",
                "entries",
            ],
            "properties": {
                "blocked": {
                    "type": "boolean",
                },
                "entries": {
                    "type": "array",
                    "maxItems": limits.limit("maximum_result_limit"),
                    "items": {
                        "type": "object",
                        "additionalProperties": false,
                        "required": [
                            "action",
                            "attempt_count",
                            "content_path",
                            "entry_identifier",
                        ],
                        "properties": {
                            "action": closed(&["activate", "deactivate", "delete", "test"]),
                            "attempt_count": count(),
                            "content_path": repository_path(limits),
                            "entry_identifier": nonempty_string(limits.limit("maximum_replication_queue_entry_identifier_bytes")),
                            "last_failure_category": nonempty_string(limits.limit("maximum_repository_name_bytes")),
                        },
                    },
                },
                "next_continuation_token": nonempty_string(limits.limit("maximum_continuation_token_bytes")),
            },
        }),
        ("list_group_members", SchemaRole::Arguments) => json!({
            "type": "object",
            "additionalProperties": false,
            "required": [
                "group_identifier",
                "include_indirect",
            ],
            "properties": {
                "group_identifier": nonempty_string(limits.limit("maximum_authorizable_identifier_bytes")),
                "include_indirect": {
                    "type": "boolean",
                },
                "result_window": result_window(limits),
            },
        }),
        ("list_group_members", SchemaRole::Result) => listing_page(
            limits,
            json!({
                "type": "object",
                "additionalProperties": false,
                "required": [
                    "authorizable_identifier",
                    "direct",
                    "kind",
                    "repository_path",
                ],
                "properties": {
                    "authorizable_identifier": nonempty_string(limits.limit("maximum_authorizable_identifier_bytes")),
                    "direct": {
                        "type": "boolean",
                    },
                    "kind": closed(&["group", "user"]),
                    "repository_path": repository_path(limits),
                },
            }),
        ),
        _ => return None,
    };
    Some(body)
}

/// Returns the body one of `list_replication_agents` through `set_user_disabled` declares.
fn group_3(wire_name: &str, role: SchemaRole, limits: &CommandContract) -> Option<Value> {
    let body = match (wire_name, role) {
        ("list_replication_agents", SchemaRole::Arguments) => json!({
            "type": "object",
            "additionalProperties": false,
            "required": [],
            "properties": {
                "result_window": result_window(limits),
            },
        }),
        ("list_replication_agents", SchemaRole::Result) => listing_page(
            limits,
            json!({
                "type": "object",
                "additionalProperties": false,
                "required": [
                    "agent_identifier",
                    "enabled",
                    "queue_blocked",
                    "queued_entry_count",
                    "repository_path",
                    "title",
                    "transport_kind",
                ],
                "properties": {
                    "agent_identifier": nonempty_string(limits.limit("maximum_replication_agent_identifier_bytes")),
                    "enabled": {
                        "type": "boolean",
                    },
                    "queue_blocked": {
                        "type": "boolean",
                    },
                    "queued_entry_count": {
                        "type": "integer",
                        "minimum": 0,
                        "maximum": limits.limit("maximum_replication_queue_entries"),
                    },
                    "repository_path": repository_path(limits),
                    "title": bounded_string(limits.limit("maximum_page_title_bytes")),
                    "transport_kind": closed(&["flush", "publish", "reverse", "static"]),
                },
            }),
        ),
        ("remove_group_member", SchemaRole::Arguments) => json!({
            "type": "object",
            "additionalProperties": false,
            "required": [
                "group_identifier",
                "member_identifier",
            ],
            "properties": {
                "group_identifier": nonempty_string(limits.limit("maximum_authorizable_identifier_bytes")),
                "member_identifier": nonempty_string(limits.limit("maximum_authorizable_identifier_bytes")),
            },
        }),
        ("remove_group_member", SchemaRole::Result) => json!({
            "type": "object",
            "additionalProperties": false,
            "required": [
                "group_identifier",
                "member_identifier",
                "was_a_member",
            ],
            "properties": {
                "group_identifier": nonempty_string(limits.limit("maximum_authorizable_identifier_bytes")),
                "member_identifier": nonempty_string(limits.limit("maximum_authorizable_identifier_bytes")),
                "was_a_member": {
                    "type": "boolean",
                },
            },
        }),
        ("retry_replication_queue_entry", SchemaRole::Arguments) => json!({
            "type": "object",
            "additionalProperties": false,
            "required": [
                "agent_identifier",
                "entry_identifier",
            ],
            "properties": {
                "agent_identifier": nonempty_string(limits.limit("maximum_replication_agent_identifier_bytes")),
                "entry_identifier": nonempty_string(limits.limit("maximum_replication_queue_entry_identifier_bytes")),
            },
        }),
        ("retry_replication_queue_entry", SchemaRole::Result) => json!({
            "type": "object",
            "additionalProperties": false,
            "required": [
                "agent_identifier",
                "entry_identifier",
                "resubmitted",
            ],
            "properties": {
                "agent_identifier": nonempty_string(limits.limit("maximum_replication_agent_identifier_bytes")),
                "entry_identifier": nonempty_string(limits.limit("maximum_replication_queue_entry_identifier_bytes")),
                "resubmitted": {
                    "type": "boolean",
                },
            },
        }),
        ("set_user_disabled", SchemaRole::Arguments) => json!({
            "type": "object",
            "additionalProperties": false,
            "required": [
                "authorizable_identifier",
                "disabled",
            ],
            "properties": {
                "authorizable_identifier": nonempty_string(limits.limit("maximum_authorizable_identifier_bytes")),
                "disabled": {
                    "type": "boolean",
                },
                "reason": bounded_string(limits.limit("maximum_authorizable_disabled_reason_bytes")),
            },
        }),
        ("set_user_disabled", SchemaRole::Result) => json!({
            "type": "object",
            "additionalProperties": false,
            "required": [
                "authorizable_identifier",
                "disabled",
            ],
            "properties": {
                "authorizable_identifier": nonempty_string(limits.limit("maximum_authorizable_identifier_bytes")),
                "disabled": {
                    "type": "boolean",
                },
            },
        }),
        _ => return None,
    };
    Some(body)
}

/// Returns the body one of `update_user_profile` through `update_user_profile` declares.
fn group_4(wire_name: &str, role: SchemaRole, limits: &CommandContract) -> Option<Value> {
    let body = match (wire_name, role) {
        ("update_user_profile", SchemaRole::Arguments) => json!({
            "type": "object",
            "additionalProperties": false,
            "required": [
                "authorizable_identifier",
            ],
            "properties": {
                "authorizable_identifier": nonempty_string(limits.limit("maximum_authorizable_identifier_bytes")),
                "properties": mutation_properties(limits),
                "removed_property_names": removed_property_names(limits),
            },
        }),
        ("update_user_profile", SchemaRole::Result) => json!({
            "type": "object",
            "additionalProperties": false,
            "required": [
                "authorizable_identifier",
                "repository_path",
            ],
            "properties": {
                "authorizable_identifier": nonempty_string(limits.limit("maximum_authorizable_identifier_bytes")),
                "repository_path": repository_path(limits),
            },
        }),
        _ => return None,
    };
    Some(body)
}
