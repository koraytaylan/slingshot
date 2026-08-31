//! Argument and result shapes for pages, components, assets, and fragments.
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
    SchemaRole, bounded_string, closed, content_fragment_elements, deleted_result, discovery_page,
    inline_binary_payload, listing_page, moved_result, mutation_properties, mutation_result,
    nonempty_string, page_match, removed_property_names, repository_path, result_window,
};

/// Returns the body one command role declares, when this leaf declares it.
pub fn body(wire_name: &str, role: SchemaRole, limits: &CommandContract) -> Option<Value> {
    group_1(wire_name, role, limits)
        .or_else(|| group_2(wire_name, role, limits))
        .or_else(|| group_3(wire_name, role, limits))
        .or_else(|| group_4(wire_name, role, limits))
        .or_else(|| group_5(wire_name, role, limits))
}

/// Returns the body one of `create_asset` through `create_experience_fragment` declares.
fn group_1(wire_name: &str, role: SchemaRole, limits: &CommandContract) -> Option<Value> {
    let body = match (wire_name, role) {
        ("create_asset", SchemaRole::Arguments) => json!({
            "type": "object",
            "additionalProperties": false,
            "required": [
                "name",
                "parent_path",
                "payload",
            ],
            "properties": {
                "metadata": mutation_properties(limits),
                "name": nonempty_string(limits.limit("maximum_repository_name_bytes")),
                "parent_path": repository_path(limits),
                "payload": inline_binary_payload(limits),
            },
        }),
        ("create_asset", SchemaRole::Result) => json!({
            "type": "object",
            "additionalProperties": false,
            "required": [
                "original_rendition_byte_length",
                "repository_path",
            ],
            "properties": {
                "original_rendition_byte_length": {
                    "type": "integer",
                    "minimum": 0,
                    "maximum": limits.limit("maximum_asset_byte_length"),
                },
                "repository_path": repository_path(limits),
            },
        }),
        ("create_asset_folder", SchemaRole::Arguments) => json!({
            "type": "object",
            "additionalProperties": false,
            "required": [
                "name",
                "parent_path",
            ],
            "properties": {
                "name": nonempty_string(limits.limit("maximum_repository_name_bytes")),
                "parent_path": repository_path(limits),
                "title": bounded_string(limits.limit("maximum_page_title_bytes")),
            },
        }),
        ("create_asset_folder", SchemaRole::Result) => mutation_result(limits),
        ("create_content_fragment", SchemaRole::Arguments) => json!({
            "type": "object",
            "additionalProperties": false,
            "required": [
                "model_path",
                "name",
                "parent_path",
            ],
            "properties": {
                "elements": content_fragment_elements(limits),
                "model_path": repository_path(limits),
                "name": nonempty_string(limits.limit("maximum_repository_name_bytes")),
                "parent_path": repository_path(limits),
                "title": bounded_string(limits.limit("maximum_page_title_bytes")),
            },
        }),
        ("create_content_fragment", SchemaRole::Result) => mutation_result(limits),
        ("create_experience_fragment", SchemaRole::Arguments) => json!({
            "type": "object",
            "additionalProperties": false,
            "required": [
                "name",
                "parent_path",
                "template_path",
                "variation_name",
            ],
            "properties": {
                "name": nonempty_string(limits.limit("maximum_repository_name_bytes")),
                "parent_path": repository_path(limits),
                "template_path": repository_path(limits),
                "title": bounded_string(limits.limit("maximum_page_title_bytes")),
                "variation_name": nonempty_string(limits.limit("maximum_experience_fragment_variation_name_bytes")),
            },
        }),
        ("create_experience_fragment", SchemaRole::Result) => json!({
            "type": "object",
            "additionalProperties": false,
            "required": [
                "repository_path",
                "variation_path",
            ],
            "properties": {
                "repository_path": repository_path(limits),
                "variation_path": repository_path(limits),
            },
        }),
        _ => return None,
    };
    Some(body)
}

/// Returns the body one of `delete_asset` through `delete_experience_fragment` declares.
fn group_2(wire_name: &str, role: SchemaRole, limits: &CommandContract) -> Option<Value> {
    let body = match (wire_name, role) {
        ("delete_asset", SchemaRole::Arguments) => json!({
            "type": "object",
            "additionalProperties": false,
            "required": [
                "asset_path",
                "reference_policy",
            ],
            "properties": {
                "asset_path": repository_path(limits),
                "reference_policy": closed(&["ignore_references", "refuse_when_referenced"]),
            },
        }),
        ("delete_asset", SchemaRole::Result) => deleted_result(limits),
        ("delete_component", SchemaRole::Arguments) => json!({
            "type": "object",
            "additionalProperties": false,
            "required": [
                "component_path",
            ],
            "properties": {
                "component_path": repository_path(limits),
            },
        }),
        ("delete_component", SchemaRole::Result) => deleted_result(limits),
        ("delete_content_fragment", SchemaRole::Arguments) => json!({
            "type": "object",
            "additionalProperties": false,
            "required": [
                "fragment_path",
                "reference_policy",
            ],
            "properties": {
                "fragment_path": repository_path(limits),
                "reference_policy": closed(&["ignore_references", "refuse_when_referenced"]),
            },
        }),
        ("delete_content_fragment", SchemaRole::Result) => deleted_result(limits),
        ("delete_experience_fragment", SchemaRole::Arguments) => json!({
            "type": "object",
            "additionalProperties": false,
            "required": [
                "fragment_path",
                "reference_policy",
            ],
            "properties": {
                "fragment_path": repository_path(limits),
                "reference_policy": closed(&["ignore_references", "refuse_when_referenced"]),
            },
        }),
        ("delete_experience_fragment", SchemaRole::Result) => deleted_result(limits),
        _ => return None,
    };
    Some(body)
}

/// Returns the body one of `delete_page` through `move_asset` declares.
fn group_3(wire_name: &str, role: SchemaRole, limits: &CommandContract) -> Option<Value> {
    let body = match (wire_name, role) {
        ("delete_page", SchemaRole::Arguments) => json!({
            "type": "object",
            "additionalProperties": false,
            "required": [
                "page_path",
                "reference_policy",
            ],
            "properties": {
                "page_path": repository_path(limits),
                "reference_policy": closed(&["ignore_references", "refuse_when_referenced"]),
            },
        }),
        ("delete_page", SchemaRole::Result) => deleted_result(limits),
        ("list_asset_renditions", SchemaRole::Arguments) => json!({
            "type": "object",
            "additionalProperties": false,
            "required": [
                "asset_path",
            ],
            "properties": {
                "asset_path": repository_path(limits),
                "result_window": result_window(limits),
            },
        }),
        ("list_asset_renditions", SchemaRole::Result) => listing_page(
            limits,
            json!({
                "type": "object",
                "additionalProperties": false,
                "required": [
                    "byte_length",
                    "media_type",
                    "name",
                    "repository_path",
                ],
                "properties": {
                    "byte_length": {
                        "type": "integer",
                        "minimum": 0,
                        "maximum": limits.limit("maximum_asset_byte_length"),
                    },
                    "media_type": nonempty_string(limits.limit("maximum_inline_binary_media_type_bytes")),
                    "name": nonempty_string(limits.limit("maximum_rendition_name_bytes")),
                    "repository_path": repository_path(limits),
                },
            }),
        ),
        ("list_child_pages", SchemaRole::Arguments) => json!({
            "type": "object",
            "additionalProperties": false,
            "required": [
                "root_path",
            ],
            "properties": {
                "result_window": result_window(limits),
                "root_path": repository_path(limits),
            },
        }),
        ("list_child_pages", SchemaRole::Result) => discovery_page(limits, page_match(limits)),
        ("move_asset", SchemaRole::Arguments) => json!({
            "type": "object",
            "additionalProperties": false,
            "required": [
                "adjust_references",
                "destination_path",
                "source_path",
            ],
            "properties": {
                "adjust_references": {
                    "type": "boolean",
                },
                "destination_path": repository_path(limits),
                "source_path": repository_path(limits),
            },
        }),
        ("move_asset", SchemaRole::Result) => moved_result(limits),
        _ => return None,
    };
    Some(body)
}

/// Returns the body one of `move_page` through `update_asset_metadata` declares.
fn group_4(wire_name: &str, role: SchemaRole, limits: &CommandContract) -> Option<Value> {
    let body = match (wire_name, role) {
        ("move_page", SchemaRole::Arguments) => json!({
            "type": "object",
            "additionalProperties": false,
            "required": [
                "adjust_references",
                "destination_path",
                "source_path",
            ],
            "properties": {
                "adjust_references": {
                    "type": "boolean",
                },
                "destination_path": repository_path(limits),
                "source_path": repository_path(limits),
            },
        }),
        ("move_page", SchemaRole::Result) => moved_result(limits),
        ("read_content_fragment", SchemaRole::Arguments) => json!({
            "type": "object",
            "additionalProperties": false,
            "required": [
                "fragment_path",
            ],
            "properties": {
                "fragment_path": repository_path(limits),
                "variation_name": nonempty_string(limits.limit("maximum_content_fragment_variation_name_bytes")),
            },
        }),
        ("read_content_fragment", SchemaRole::Result) => json!({
            "type": "object",
            "additionalProperties": false,
            "required": [
                "elements",
                "model_path",
                "repository_path",
                "variation_name",
            ],
            "properties": {
                "elements": content_fragment_elements(limits),
                "model_path": repository_path(limits),
                "repository_path": repository_path(limits),
                "title": bounded_string(limits.limit("maximum_page_title_bytes")),
                "variation_name": nonempty_string(limits.limit("maximum_content_fragment_variation_name_bytes")),
            },
        }),
        ("reorder_component", SchemaRole::Arguments) => json!({
            "type": "object",
            "additionalProperties": false,
            "required": [
                "component_path",
                "placement",
            ],
            "properties": {
                "component_path": repository_path(limits),
                "placement": {
                    "oneOf": [
                        {
                            "type": "object",
                            "additionalProperties": false,
                            "required": [
                                "mode",
                                "sibling_name",
                            ],
                            "properties": {
                                "mode": {
                                    "const": "before",
                                },
                                "sibling_name": nonempty_string(limits.limit("maximum_component_name_bytes")),
                            },
                        },
                        {
                            "type": "object",
                            "additionalProperties": false,
                            "required": [
                                "mode",
                            ],
                            "properties": {
                                "mode": {
                                    "const": "last",
                                },
                            },
                        },
                    ],
                },
            },
        }),
        ("reorder_component", SchemaRole::Result) => json!({
            "type": "object",
            "additionalProperties": false,
            "required": [
                "repository_path",
            ],
            "properties": {
                "preceding_sibling_name": nonempty_string(limits.limit("maximum_component_name_bytes")),
                "repository_path": repository_path(limits),
            },
        }),
        ("update_asset_metadata", SchemaRole::Arguments) => json!({
            "type": "object",
            "additionalProperties": false,
            "required": [
                "asset_path",
            ],
            "properties": {
                "asset_path": repository_path(limits),
                "properties": mutation_properties(limits),
                "removed_property_names": removed_property_names(limits),
            },
        }),
        ("update_asset_metadata", SchemaRole::Result) => mutation_result(limits),
        _ => return None,
    };
    Some(body)
}

/// Returns the body one of `update_component` through `update_page` declares.
fn group_5(wire_name: &str, role: SchemaRole, limits: &CommandContract) -> Option<Value> {
    let body = match (wire_name, role) {
        ("update_component", SchemaRole::Arguments) => json!({
            "type": "object",
            "additionalProperties": false,
            "required": [
                "component_path",
            ],
            "properties": {
                "component_path": repository_path(limits),
                "properties": mutation_properties(limits),
                "removed_property_names": removed_property_names(limits),
            },
        }),
        ("update_component", SchemaRole::Result) => mutation_result(limits),
        ("update_content_fragment", SchemaRole::Arguments) => json!({
            "type": "object",
            "additionalProperties": false,
            "required": [
                "fragment_path",
            ],
            "properties": {
                "elements": content_fragment_elements(limits),
                "fragment_path": repository_path(limits),
                "title": bounded_string(limits.limit("maximum_page_title_bytes")),
                "variation_name": nonempty_string(limits.limit("maximum_content_fragment_variation_name_bytes")),
            },
        }),
        ("update_content_fragment", SchemaRole::Result) => mutation_result(limits),
        ("update_experience_fragment", SchemaRole::Arguments) => json!({
            "type": "object",
            "additionalProperties": false,
            "required": [
                "variation_path",
            ],
            "properties": {
                "properties": mutation_properties(limits),
                "removed_property_names": removed_property_names(limits),
                "title": bounded_string(limits.limit("maximum_page_title_bytes")),
                "variation_path": repository_path(limits),
            },
        }),
        ("update_experience_fragment", SchemaRole::Result) => mutation_result(limits),
        ("update_page", SchemaRole::Arguments) => json!({
            "type": "object",
            "additionalProperties": false,
            "required": [
                "page_path",
            ],
            "properties": {
                "page_path": repository_path(limits),
                "properties": mutation_properties(limits),
                "removed_property_names": removed_property_names(limits),
                "title": bounded_string(limits.limit("maximum_page_title_bytes")),
            },
        }),
        ("update_page", SchemaRole::Result) => mutation_result(limits),
        _ => return None,
    };
    Some(body)
}
