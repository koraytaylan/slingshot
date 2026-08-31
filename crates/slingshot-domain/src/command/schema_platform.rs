//! Argument and result shapes for configurations, bundles, components, and mapping.
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
    SchemaRole, closed, configuration_value, count, listing_page, nonempty_string, repository_path,
    result_window,
};

/// Returns the body one command role declares, when this leaf declares it.
pub fn body(wire_name: &str, role: SchemaRole, limits: &CommandContract) -> Option<Value> {
    group_1(wire_name, role, limits)
        .or_else(|| group_2(wire_name, role, limits))
        .or_else(|| group_3(wire_name, role, limits))
}

/// Returns the body one of `delete_open_service_gateway_initiative_configuration` through `list_open_service_gateway_initiative_components` declares.
fn group_1(wire_name: &str, role: SchemaRole, limits: &CommandContract) -> Option<Value> {
    let body = match (wire_name, role) {
        ("delete_open_service_gateway_initiative_configuration", SchemaRole::Arguments) => json!({
            "type": "object",
            "additionalProperties": false,
            "required": [
                "persistent_identifier",
            ],
            "properties": {
                "persistent_identifier": nonempty_string(limits.limit("maximum_configuration_persistent_identifier_bytes")),
            },
        }),
        ("delete_open_service_gateway_initiative_configuration", SchemaRole::Result) => json!({
            "type": "object",
            "additionalProperties": false,
            "required": [
                "persistent_identifier",
                "was_a_factory_instance",
            ],
            "properties": {
                "persistent_identifier": nonempty_string(limits.limit("maximum_configuration_persistent_identifier_bytes")),
                "was_a_factory_instance": {
                    "type": "boolean",
                },
            },
        }),
        ("find_open_service_gateway_initiative_configurations", SchemaRole::Arguments) => json!({
            "type": "object",
            "additionalProperties": false,
            "required": [],
            "properties": {
                "persistent_identifier_prefix": nonempty_string(limits.limit("maximum_configuration_persistent_identifier_bytes")),
                "result_window": result_window(limits),
            },
        }),
        ("find_open_service_gateway_initiative_configurations", SchemaRole::Result) => {
            listing_page(
                limits,
                json!({
                    "type": "object",
                    "additionalProperties": false,
                    "required": [
                        "bound_to_a_bundle_location",
                        "persistent_identifier",
                        "property_key_count",
                    ],
                    "properties": {
                        "bound_to_a_bundle_location": {
                            "type": "boolean",
                        },
                        "factory_persistent_identifier": nonempty_string(limits.limit("maximum_configuration_persistent_identifier_bytes")),
                        "persistent_identifier": nonempty_string(limits.limit("maximum_configuration_persistent_identifier_bytes")),
                        "property_key_count": {
                            "type": "integer",
                            "minimum": 0,
                            "maximum": limits.limit("maximum_inspected_configuration_properties"),
                        },
                    },
                }),
            )
        }
        ("list_open_service_gateway_initiative_bundles", SchemaRole::Arguments) => json!({
            "type": "object",
            "additionalProperties": false,
            "required": [],
            "properties": {
                "result_window": result_window(limits),
                "states": {
                    "type": "array",
                    "minItems": 1,
                    "uniqueItems": true,
                    "maxItems": limits.limit("maximum_bundle_states"),
                    "items": closed(&["active", "installed", "resolved", "starting", "stopping", "uninstalled"]),
                },
                "symbolic_name_prefix": nonempty_string(limits.limit("maximum_bundle_symbolic_name_bytes")),
            },
        }),
        ("list_open_service_gateway_initiative_bundles", SchemaRole::Result) => listing_page(
            limits,
            json!({
                "type": "object",
                "additionalProperties": false,
                "required": [
                    "bundle_identifier",
                    "state",
                    "symbolic_name",
                    "version",
                ],
                "properties": {
                    "bundle_identifier": count(),
                    "state": closed(&["active", "installed", "resolved", "starting", "stopping", "uninstalled"]),
                    "symbolic_name": nonempty_string(limits.limit("maximum_bundle_symbolic_name_bytes")),
                    "version": nonempty_string(limits.limit("maximum_bundle_version_bytes")),
                },
            }),
        ),
        ("list_open_service_gateway_initiative_components", SchemaRole::Arguments) => json!({
            "type": "object",
            "additionalProperties": false,
            "required": [],
            "properties": {
                "name_prefix": nonempty_string(limits.limit("maximum_declarative_service_component_name_bytes")),
                "result_window": result_window(limits),
                "states": {
                    "type": "array",
                    "minItems": 1,
                    "uniqueItems": true,
                    "maxItems": limits.limit("maximum_component_states"),
                    "items": closed(&["active", "disabled", "satisfied", "unsatisfied"]),
                },
            },
        }),
        ("list_open_service_gateway_initiative_components", SchemaRole::Result) => listing_page(
            limits,
            json!({
                "type": "object",
                "additionalProperties": false,
                "required": [
                    "bundle_symbolic_name",
                    "name",
                    "state",
                ],
                "properties": {
                    "bundle_symbolic_name": nonempty_string(limits.limit("maximum_bundle_symbolic_name_bytes")),
                    "name": nonempty_string(limits.limit("maximum_declarative_service_component_name_bytes")),
                    "service_persistent_identifier": nonempty_string(limits.limit("maximum_configuration_persistent_identifier_bytes")),
                    "state": closed(&["active", "disabled", "satisfied", "unsatisfied"]),
                },
            }),
        ),
        _ => return None,
    };
    Some(body)
}

/// Returns the body one of `list_resource_mappings` through `set_open_service_gateway_initiative_bundle_state` declares.
fn group_2(wire_name: &str, role: SchemaRole, limits: &CommandContract) -> Option<Value> {
    let body = match (wire_name, role) {
        ("list_resource_mappings", SchemaRole::Arguments) => json!({
            "type": "object",
            "additionalProperties": false,
            "required": [],
            "properties": {
                "result_window": result_window(limits),
            },
        }),
        ("list_resource_mappings", SchemaRole::Result) => json!({
            "type": "object",
            "additionalProperties": false,
            "required": [
                "entries",
            ],
            "properties": {
                "entries": {
                    "type": "array",
                    "maxItems": limits.limit("maximum_result_limit"),
                    "items": {
                        "type": "object",
                        "additionalProperties": false,
                        "required": [
                            "entry_path",
                            "kind",
                            "pattern",
                            "replacements",
                        ],
                        "properties": {
                            "entry_path": repository_path(limits),
                            "kind": closed(&["alias", "internal_redirect", "map", "redirect"]),
                            "pattern": nonempty_string(limits.limit("maximum_resource_mapping_pattern_bytes")),
                            "replacements": {
                                "type": "array",
                                "maxItems": limits.limit("maximum_resource_mapping_replacements"),
                                "items": nonempty_string(limits.limit("maximum_resource_mapping_pattern_bytes")),
                            },
                            "status_code": count(),
                        },
                    },
                },
                "next_continuation_token": nonempty_string(limits.limit("maximum_continuation_token_bytes")),
            },
        }),
        ("map_resource_path", SchemaRole::Arguments) => json!({
            "type": "object",
            "additionalProperties": false,
            "required": [
                "include_trace",
                "repository_path",
            ],
            "properties": {
                "include_trace": {
                    "type": "boolean",
                },
                "repository_path": repository_path(limits),
                "request_authority": nonempty_string(limits.limit("maximum_repository_name_bytes")),
            },
        }),
        ("map_resource_path", SchemaRole::Result) => json!({
            "type": "object",
            "additionalProperties": false,
            "required": [
                "mapped_address",
                "repository_path",
            ],
            "properties": {
                "mapped_address": nonempty_string(limits.limit("maximum_request_address_bytes")),
                "repository_path": repository_path(limits),
                "trace": {
                    "type": "array",
                    "maxItems": limits.limit("maximum_resolution_trace_entries"),
                    "items": repository_path(limits),
                },
            },
        }),
        ("resolve_resource_path", SchemaRole::Arguments) => json!({
            "type": "object",
            "additionalProperties": false,
            "required": [
                "include_trace",
                "request_address",
            ],
            "properties": {
                "include_trace": {
                    "type": "boolean",
                },
                "request_address": nonempty_string(limits.limit("maximum_request_address_bytes")),
            },
        }),
        ("resolve_resource_path", SchemaRole::Result) => json!({
            "type": "object",
            "additionalProperties": false,
            "required": [
                "request_address",
                "selectors",
            ],
            "properties": {
                "extension": nonempty_string(limits.limit("maximum_repository_name_bytes")),
                "request_address": nonempty_string(limits.limit("maximum_request_address_bytes")),
                "resolved_path": repository_path(limits),
                "resource_type": nonempty_string(limits.limit("maximum_component_resource_type_bytes")),
                "selectors": {
                    "type": "array",
                    "maxItems": limits.limit("maximum_repository_path_segments"),
                    "items": nonempty_string(limits.limit("maximum_repository_name_bytes")),
                },
                "suffix": nonempty_string(limits.limit("maximum_repository_path_bytes")),
                "trace": {
                    "type": "array",
                    "maxItems": limits.limit("maximum_resolution_trace_entries"),
                    "items": repository_path(limits),
                },
            },
        }),
        ("set_open_service_gateway_initiative_bundle_state", SchemaRole::Arguments) => json!({
            "type": "object",
            "additionalProperties": false,
            "required": [
                "symbolic_name",
                "transition",
            ],
            "properties": {
                "symbolic_name": nonempty_string(limits.limit("maximum_bundle_symbolic_name_bytes")),
                "transition": closed(&["refresh", "start", "stop"]),
            },
        }),
        ("set_open_service_gateway_initiative_bundle_state", SchemaRole::Result) => json!({
            "type": "object",
            "additionalProperties": false,
            "required": [
                "observed_state",
                "symbolic_name",
            ],
            "properties": {
                "observed_state": closed(&["active", "installed", "resolved", "starting", "stopping", "uninstalled"]),
                "symbolic_name": nonempty_string(limits.limit("maximum_bundle_symbolic_name_bytes")),
            },
        }),
        _ => return None,
    };
    Some(body)
}

/// Returns the body one of `update_open_service_gateway_initiative_configuration` through `update_open_service_gateway_initiative_configuration` declares.
fn group_3(wire_name: &str, role: SchemaRole, limits: &CommandContract) -> Option<Value> {
    let body = match (wire_name, role) {
        ("update_open_service_gateway_initiative_configuration", SchemaRole::Arguments) => json!({
            "type": "object",
            "additionalProperties": false,
            "required": [
                "persistent_identifier",
            ],
            "properties": {
                "assignments": {
                    "type": "object",
                    "maxProperties": limits.limit("maximum_inspected_configuration_properties"),
                    "additionalProperties": configuration_value(limits),
                },
                "persistent_identifier": nonempty_string(limits.limit("maximum_configuration_persistent_identifier_bytes")),
                "removed_property_keys": {
                    "type": "array",
                    "minItems": 1,
                    "uniqueItems": true,
                    "maxItems": limits.limit("maximum_inspected_configuration_properties"),
                    "items": nonempty_string(limits.limit("maximum_configuration_property_key_bytes")),
                },
            },
        }),
        ("update_open_service_gateway_initiative_configuration", SchemaRole::Result) => json!({
            "type": "object",
            "additionalProperties": false,
            "required": [
                "changed_property_key_count",
                "persistent_identifier",
            ],
            "properties": {
                "changed_property_key_count": {
                    "type": "integer",
                    "minimum": 0,
                    "maximum": limits.limit("maximum_inspected_configuration_properties"),
                },
                "persistent_identifier": nonempty_string(limits.limit("maximum_configuration_persistent_identifier_bytes")),
            },
        }),
        _ => return None,
    };
    Some(body)
}
