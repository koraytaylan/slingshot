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
    SchemaRole, bounded_string, closed, component_listing_match, content_fragment_elements,
    deleted_result, discovery_arguments, discovery_page, inline_binary_payload, listing_page,
    moved_result, mutation_properties, mutation_result, nonempty_string, page_match,
    property_predicates, relative_path, removed_property_names, repository_path, result_window,
};

/// Returns the body one command role declares, when this leaf declares it.
pub fn body(wire_name: &str, role: SchemaRole, limits: &CommandContract) -> Option<Value> {
    group_1(wire_name, role, limits)
        .or_else(|| group_2(wire_name, role, limits))
        .or_else(|| group_3(wire_name, role, limits))
        .or_else(|| group_4(wire_name, role, limits))
        .or_else(|| group_5(wire_name, role, limits))
        .or_else(|| group_6(wire_name, role, limits))
        .or_else(|| group_7(wire_name, role, limits))
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

/// Returns the body one of the three child listings declares.
///
/// The three share an anchor and a window and differ only in what they admit:
/// every child, the children of one named primary type, or the pages. A typed
/// listing reports the type each child has, because the type is what
/// distinguishes a page from the folder beside it.
fn group_6(wire_name: &str, role: SchemaRole, limits: &CommandContract) -> Option<Value> {
    let body = match (wire_name, role) {
        ("list_child_nodes", SchemaRole::Arguments)
        | ("list_child_pages", SchemaRole::Arguments)
        | ("list_content_fragment_models", SchemaRole::Arguments)
        | ("list_page_templates", SchemaRole::Arguments) => {
            json!({
                "type": "object",
                "additionalProperties": false,
                "required": [
                    "root_path",
                ],
                "properties": {
                    "result_window": result_window(limits),
                    "root_path": repository_path(limits),
                },
            })
        }
        ("list_child_nodes_by_type", SchemaRole::Arguments) => json!({
            "type": "object",
            "additionalProperties": false,
            "required": [
                "primary_node_type",
                "root_path",
            ],
            "properties": {
                "primary_node_type":
                    nonempty_string(limits.limit("maximum_primary_node_type_name_bytes")),
                "result_window": result_window(limits),
                "root_path": repository_path(limits),
            },
        }),
        ("list_child_nodes", SchemaRole::Result)
        | ("list_child_nodes_by_type", SchemaRole::Result) => {
            discovery_page(limits, child_node_match(limits))
        }
        ("list_child_pages", SchemaRole::Result)
        | ("list_content_fragment_models", SchemaRole::Result)
        | ("list_page_templates", SchemaRole::Result) => discovery_page(limits, page_match(limits)),
        _ => return None,
    };
    Some(body)
}

/// Returns the body the four content catalogues declare.
///
/// Each takes an anchor and a window. Component definitions and component
/// instances answer with a resource type; content fragments and experience
/// fragments answer with a path and a title.
fn group_7(wire_name: &str, role: SchemaRole, limits: &CommandContract) -> Option<Value> {
    let body = match (wire_name, role) {
        ("list_component_definitions", SchemaRole::Arguments)
        | ("list_components", SchemaRole::Arguments)
        | ("list_content_fragments", SchemaRole::Arguments)
        | ("list_experience_fragments", SchemaRole::Arguments) => json!({
            "type": "object",
            "additionalProperties": false,
            "required": ["root_path"],
            "properties": {
                "result_window": result_window(limits),
                "root_path": repository_path(limits),
            },
        }),
        ("list_component_definitions", SchemaRole::Result)
        | ("list_components", SchemaRole::Result) => {
            crate::command::incremental_discovery::page_schema(
                limits,
                component_listing_match(limits),
            )
        }
        ("list_content_fragments", SchemaRole::Result)
        | ("list_experience_fragments", SchemaRole::Result) => {
            crate::command::incremental_discovery::page_schema(limits, page_match(limits))
        }
        _ => return None,
    };
    Some(body)
}

/// Returns the schema one child-node match satisfies.
fn child_node_match(limits: &CommandContract) -> Value {
    json!({
        "type": "object",
        "additionalProperties": false,
        "required": ["primary_node_type", "repository_path"],
        "properties": {
            "primary_node_type":
                nonempty_string(limits.limit("maximum_primary_node_type_name_bytes")),
            "repository_path": repository_path(limits),
            "title": bounded_string(limits.limit("maximum_page_title_bytes")),
        },
    })
}

/// Returns the body the two asset searches declare, when this is one of them.
pub(crate) fn asset_search_body(
    wire_name: &str,
    role: SchemaRole,
    limits: &CommandContract,
) -> Option<Value> {
    let body = match (wire_name, role) {
        ("find_assets_by_metadata", SchemaRole::Arguments) => {
            find_assets_by_metadata_arguments(limits)
        }
        ("find_assets_by_metadata", SchemaRole::Result) => {
            crate::command::incremental_discovery::page_schema(limits, asset_match(limits))
        }
        ("find_assets_referenced_by_page", SchemaRole::Arguments) => json!({
            "type": "object",
            "additionalProperties": false,
            "required": ["page_path"],
            "properties": {
                "page_path": repository_path(limits),
                "result_window": result_window(limits),
            },
        }),
        ("find_assets_referenced_by_page", SchemaRole::Result) => discovery_page(
            limits,
            json!({
                "type": "object",
                "additionalProperties": false,
                "required": ["reference_paths", "repository_path"],
                "properties": {
                    "reference_paths": {
                        "type": "array",
                        "minItems": 1,
                        "uniqueItems": true,
                        "maxItems": limits.limit("maximum_asset_reference_paths"),
                        "items": relative_path(limits),
                    },
                    "repository_path": repository_path(limits),
                },
            }),
        ),
        _ => return None,
    };
    Some(body)
}

/// Returns the schema one asset match satisfies.
fn asset_match(limits: &CommandContract) -> Value {
    json!({
        "type": "object",
        "additionalProperties": false,
        "required": ["repository_path"],
        "properties": {
            "byte_length": {
                "type": "integer",
                "minimum": 0,
                "maximum": limits.limit("maximum_asset_byte_length"),
            },
            "media_format": nonempty_string(limits.limit("maximum_media_format_bytes")),
            "repository_path": repository_path(limits),
            "tags": {
                "type": "array",
                "uniqueItems": true,
                "maxItems": limits.limit("maximum_requested_asset_tags"),
                "items": nonempty_string(limits.limit("maximum_asset_tag_bytes")),
            },
        },
    })
}

/// Returns the schema the asset search's arguments satisfy.
fn find_assets_by_metadata_arguments(limits: &CommandContract) -> Value {
    let byte_length = json!({
        "type": "integer",
        "minimum": 0,
        "maximum": limits.limit("maximum_asset_byte_length"),
    });
    discovery_arguments(
        limits,
        json!({
            "maximum_byte_length": byte_length,
            "minimum_byte_length": byte_length,
            "media_formats": {
                "type": "array",
                "uniqueItems": true,
                "maxItems": limits.limit("maximum_requested_media_formats"),
                "items": nonempty_string(limits.limit("maximum_media_format_bytes")),
            },
            "property_predicates": property_predicates(limits),
            "tag_match_mode": {"enum": ["any", "all"]},
            "tags": {
                "type": "array",
                "uniqueItems": true,
                "maxItems": limits.limit("maximum_requested_asset_tags"),
                "items": nonempty_string(limits.limit("maximum_asset_tag_bytes")),
            },
        }),
        json!(["root_path"]),
    )
}
