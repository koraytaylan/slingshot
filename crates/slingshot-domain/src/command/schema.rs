//! Draft 2020-12 schemas for every command, in both roles.
//!
//! These are ordinary schemas that any conforming validator can run. What they
//! check is deliberately bounded: types, closed and required members, literal
//! discriminators, counts, ranges, and document-local alternatives. What they
//! do not check is everything a decoded tree cannot show - serialized member
//! order, raw UTF-8 spelling, minimal integer tokens, escape spelling, and the
//! lexical order of set-like arrays. Those belong to the byte contract beside
//! this one, and no schema result is ever offered as proof of them.
//!
//! The identifier carries the semantic contract version, so a version change
//! changes both role schemas' bytes and therefore both digests, even when every
//! other keyword is identical. That is the point: compatibility is a comparison
//! of digests, and two versions must never share one.
//!
//! Each root also carries the byte contract's own digest as an annotation. A
//! validator may ignore it; its presence binds the separately executed byte
//! contract into both role digests, so a change to the byte rules regenerates
//! the schemas that depend on them.

use serde_json::{Value, json};

use crate::command::canonical_json::{canonical_digest, write_canonical};
use crate::command::command_identity::{CommandContract, INITIAL_COMMAND_VERSION};
use crate::command::inspect_open_service_gateway_initiative_configuration::DECLARED_SCALAR_TYPES;

/// Dialect every schema declares.
pub const SCHEMA_DIALECT: &str = "https://json-schema.org/draft/2020-12/schema";

/// Format the schema manifest declares.
pub const SCHEMA_MANIFEST_FORMAT: &str = "slingshot.command-schema/1";

/// Annotation binding the byte contract into both role digests.
pub const CANONICAL_CONTRACT_ANNOTATION: &str = "x-slingshot-canonical-json-contract-sha256";

/// Roles every command has.
pub const ROLE_COUNT: usize = 2;

/// Prefix every schema identifier begins with.
pub const SCHEMA_IDENTIFIER_PREFIX: &str = "urn:slingshot:command";

/// Every command wire name, in the order the catalog returns them.
pub const COMMAND_WIRE_NAMES: &[&str] = &[
    "add_component",
    "add_group_member",
    "cancel_sling_job",
    "create_asset",
    "create_asset_folder",
    "create_content_fragment",
    "create_experience_fragment",
    "create_group",
    "create_page",
    "create_user",
    "delete_asset",
    "delete_authorizable",
    "delete_component",
    "delete_content_fragment",
    "delete_experience_fragment",
    "delete_open_service_gateway_initiative_configuration",
    "delete_page",
    "download_content_package",
    "find_assets_by_metadata",
    "find_assets_referenced_by_page",
    "find_open_service_gateway_initiative_configurations",
    "find_pages_by_template",
    "find_pages_containing_phrase",
    "find_pages_using_components",
    "find_sling_jobs",
    "find_workflow_instances",
    "flush_replication_queue",
    "inspect_open_service_gateway_initiative_configuration",
    "inspect_replication_agent",
    "inspect_replication_queue",
    "inspect_sling_job",
    "inspect_workflow_instance",
    "list_asset_renditions",
    "list_child_pages",
    "list_group_members",
    "list_open_service_gateway_initiative_bundles",
    "list_open_service_gateway_initiative_components",
    "list_replication_agents",
    "list_resource_mappings",
    "list_sling_job_queues",
    "list_workflow_models",
    "load_content_as_json",
    "map_resource_path",
    "move_asset",
    "move_page",
    "query_paths",
    "read_content_fragment",
    "remove_group_member",
    "reorder_component",
    "replicate_content",
    "resolve_resource_path",
    "retry_replication_queue_entry",
    "set_open_service_gateway_initiative_bundle_state",
    "set_user_disabled",
    "set_workflow_instance_suspension",
    "start_workflow",
    "terminate_workflow_instance",
    "update_asset_metadata",
    "update_component",
    "update_content_fragment",
    "update_experience_fragment",
    "update_open_service_gateway_initiative_configuration",
    "update_page",
    "update_user_profile",
];

/// Which side of one command a schema describes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SchemaRole {
    /// What a caller sends.
    Arguments,
    /// What the command answers with.
    Result,
}

impl SchemaRole {
    /// Returns the wire spelling of this role.
    #[must_use]
    pub fn as_text(self) -> &'static str {
        match self {
            Self::Arguments => "arguments",
            Self::Result => "result",
        }
    }

    /// Returns both roles, in the order they are written.
    #[must_use]
    pub fn both() -> [Self; ROLE_COUNT] {
        [Self::Arguments, Self::Result]
    }
}

/// Returns the identifier one command role's schema declares.
///
/// The version goes in literally. Its alphabet is safe in a URN segment, so
/// there is no second escaping convention to disagree about.
#[must_use]
pub fn schema_identifier(wire_name: &str, role: SchemaRole) -> String {
    format!("{SCHEMA_IDENTIFIER_PREFIX}:{wire_name}:{}:{INITIAL_COMMAND_VERSION}", role.as_text())
}

/// Returns the file one command role's schema is committed as.
#[must_use]
pub fn schema_file_name(wire_name: &str, role: SchemaRole) -> String {
    format!("{wire_name}-{}.json", role.as_text())
}

/// Returns a bounded string schema.
pub(crate) fn bounded_string(maximum: u64) -> Value {
    json!({"type": "string", "maxLength": maximum})
}

/// Returns a nonempty bounded string schema.
pub(crate) fn nonempty_string(maximum: u64) -> Value {
    json!({"type": "string", "minLength": 1, "maxLength": maximum})
}

/// Returns the schema one absolute repository path satisfies.
///
/// Anchored to a leading separator and bounded. The rest of the grammar - the
/// refused punctuation, the sibling index, the normalization form - is not
/// schema-expressible without a pattern nobody could read, so the typed
/// constructor owns it and the schema says so by saying less.
pub(crate) fn repository_path(limits: &CommandContract) -> Value {
    json!({
        "type": "string",
        "pattern": "^/",
        "minLength": 1,
        "maxLength": limits.limit("maximum_repository_path_bytes"),
    })
}

/// Returns the schema one relative repository path satisfies.
fn relative_path(limits: &CommandContract) -> Value {
    json!({
        "type": "string",
        "pattern": "^[^/]",
        "minLength": 1,
        "maxLength": limits.limit("maximum_relative_property_path_bytes"),
    })
}

/// Returns the schema one result window satisfies.
///
/// Two closed alternatives with a literal discriminator, which is exactly the
/// kind of thing a standard validator does well.
pub(crate) fn result_window(limits: &CommandContract) -> Value {
    json!({
        "oneOf": [
            {
                "type": "object",
                "additionalProperties": false,
                "required": ["mode", "offset", "limit"],
                "properties": {
                    "mode": {"const": "initial"},
                    "offset": {
                        "type": "integer",
                        "minimum": 0,
                        "maximum": limits.limit("maximum_result_offset"),
                    },
                    "limit": {
                        "type": "integer",
                        "minimum": 1,
                        "maximum": limits.limit("maximum_result_limit"),
                    },
                },
            },
            {
                "type": "object",
                "additionalProperties": false,
                "required": ["mode", "continuation_token"],
                "properties": {
                    "mode": {"const": "continuation"},
                    "continuation_token":
                        nonempty_string(limits.limit("maximum_continuation_token_bytes")),
                },
            },
        ],
    })
}

/// Returns the schema one property scalar satisfies.
fn property_scalar(limits: &CommandContract) -> Value {
    json!({
        "type": "object",
        "additionalProperties": false,
        "required": ["type", "value"],
        "properties": {
            "type": {
                "enum": [
                    "string", "boolean", "integer", "decimal", "date_time", "repository_path",
                ],
            },
            "value": {
                "anyOf": [
                    bounded_string(limits.limit("maximum_property_string_bytes")),
                    {"type": "boolean"},
                ],
            },
        },
    })
}

/// Returns the schema one property value satisfies.
pub(crate) fn property_value(limits: &CommandContract) -> Value {
    json!({
        "oneOf": [
            {
                "type": "object",
                "additionalProperties": false,
                "required": ["cardinality", "value"],
                "properties": {
                    "cardinality": {"const": "single"},
                    "value": property_scalar(limits),
                },
            },
            {
                "type": "object",
                "additionalProperties": false,
                "required": ["cardinality", "values"],
                "properties": {
                    "cardinality": {"const": "multiple"},
                    "values": {
                        "type": "array",
                        "minItems": 1,
                        "maxItems": limits.limit("maximum_property_value_items"),
                        "items": property_scalar(limits),
                    },
                },
            },
        ],
    })
}

/// Returns the schema one predicate collection satisfies.
fn property_predicates(limits: &CommandContract) -> Value {
    json!({
        "type": "array",
        "maxItems": limits.limit("maximum_property_predicates"),
        "items": {
            "type": "object",
            "additionalProperties": false,
            "required": ["operator", "property_path"],
            "properties": {
                "operator": {
                    "enum": [
                        "exists", "equals", "not_equals", "scalar_in", "list_contains_any",
                        "list_contains_all", "less_than", "less_than_or_equal", "greater_than",
                        "greater_than_or_equal",
                    ],
                },
                "property_path": relative_path(limits),
                "value": {},
                "values": {
                    "type": "array",
                    "minItems": 1,
                    "maxItems": limits.limit("maximum_property_predicate_values"),
                    "items": property_scalar(limits),
                },
            },
        },
    })
}

/// Returns the schema one artifact descriptor satisfies.
fn artifact_descriptor(limits: &CommandContract) -> Value {
    json!({
        "type": "object",
        "additionalProperties": false,
        "required": [
            "identifier", "slot", "media_type", "byte_length", "digest", "suggested_file_name",
        ],
        "properties": {
            "identifier": nonempty_string(limits.limit("maximum_artifact_identifier_bytes")),
            "slot": nonempty_string(limits.limit("maximum_artifact_slot_bytes")),
            "media_type": nonempty_string(limits.limit("maximum_artifact_media_type_bytes")),
            "byte_length": {"type": "integer", "minimum": 0},
            "digest": {"type": "string", "pattern": "^[0-9a-f]{64}$"},
            "suggested_file_name":
                nonempty_string(limits.limit("maximum_artifact_suggested_file_name_bytes")),
        },
    })
}

/// Returns the schema one page of discovery matches satisfies.
pub(crate) fn discovery_page(limits: &CommandContract, match_schema: Value) -> Value {
    json!({
        "type": "object",
        "additionalProperties": false,
        "required": ["matches"],
        "properties": {
            "matches": {
                "type": "array",
                "maxItems": limits.limit("maximum_result_limit"),
                "items": match_schema,
            },
            "next_continuation_token":
                nonempty_string(limits.limit("maximum_continuation_token_bytes")),
        },
    })
}

/// Returns the schema one page match satisfies.
pub(crate) fn page_match(limits: &CommandContract) -> Value {
    json!({
        "type": "object",
        "additionalProperties": false,
        "required": ["repository_path"],
        "properties": {
            "repository_path": repository_path(limits),
            "title": bounded_string(limits.limit("maximum_page_title_bytes")),
        },
    })
}

/// Returns the schema a discovery command's arguments satisfy.
fn discovery_arguments(limits: &CommandContract, extra: Value, required: Value) -> Value {
    let mut properties = json!({
        "root_path": repository_path(limits),
        "result_window": result_window(limits),
    });
    if let (Some(target), Some(source)) = (properties.as_object_mut(), extra.as_object()) {
        for (name, member) in source {
            target.insert(name.clone(), member.clone());
        }
    }
    json!({
        "type": "object",
        "additionalProperties": false,
        "required": required,
        "properties": properties,
    })
}

/// Returns one bounded nonnegative count.
pub(crate) fn count() -> Value {
    json!({"type": "integer", "minimum": 0})
}

/// Returns the schema one closed spelling set satisfies.
pub(crate) fn closed(spellings: &[&str]) -> Value {
    json!({"enum": spellings})
}

/// Returns the schema one page of text-keyed rows satisfies.
///
/// The listing counterpart of `discovery_page`: same window, same token, rows
/// keyed by something other than a repository path.
pub(crate) fn listing_page(limits: &CommandContract, row: Value) -> Value {
    json!({
        "type": "object",
        "additionalProperties": false,
        "required": ["matches"],
        "properties": {
            "matches": {
                "type": "array",
                "maxItems": limits.limit("maximum_result_limit"),
                "items": row,
            },
            "next_continuation_token":
                nonempty_string(limits.limit("maximum_continuation_token_bytes")),
        },
    })
}

/// Returns the schema a removal list satisfies.
pub(crate) fn removed_property_names(limits: &CommandContract) -> Value {
    json!({
        "type": "array",
        "minItems": 1,
        "uniqueItems": true,
        "maxItems": limits.limit("maximum_removed_property_names"),
        "items": nonempty_string(limits.limit("maximum_property_name_bytes")),
    })
}

/// Returns the schema the shared mutation result satisfies.
pub(crate) fn mutation_result(limits: &CommandContract) -> Value {
    json!({
        "type": "object",
        "additionalProperties": false,
        "required": ["repository_path"],
        "properties": {"repository_path": repository_path(limits)},
    })
}

/// Returns the schema the shared deletion result satisfies.
pub(crate) fn deleted_result(limits: &CommandContract) -> Value {
    json!({
        "type": "object",
        "additionalProperties": false,
        "required": ["removed_node_count", "repository_path"],
        "properties": {
            "removed_node_count": {
                "type": "integer",
                "minimum": 0,
                "maximum": limits.limit("maximum_deleted_nodes"),
            },
            "repository_path": repository_path(limits),
        },
    })
}

/// Returns the schema the shared move result satisfies.
pub(crate) fn moved_result(limits: &CommandContract) -> Value {
    json!({
        "type": "object",
        "additionalProperties": false,
        "required": ["adjusted_reference_count", "destination_path", "source_path"],
        "properties": {
            "adjusted_reference_count": {
                "type": "integer",
                "minimum": 0,
                "maximum": limits.limit("maximum_adjusted_references"),
            },
            "destination_path": repository_path(limits),
            "source_path": repository_path(limits),
        },
    })
}

/// Returns the schema one inline binary payload satisfies.
pub(crate) fn inline_binary_payload(limits: &CommandContract) -> Value {
    json!({
        "type": "object",
        "additionalProperties": false,
        "required": ["encoded_content", "media_type"],
        "properties": {
            "encoded_content": {
                "type": "string",
                "maxLength": limits.limit("maximum_inline_binary_encoded_bytes"),
                "pattern": "^[A-Za-z0-9+/]*={0,2}$",
            },
            "media_type": nonempty_string(limits.limit("maximum_inline_binary_media_type_bytes")),
        },
    })
}

/// Returns the schema one configuration value satisfies.
///
/// The class and the carrier are both stated, because writing a value back needs
/// to know whether the framework wants a primitive array, a wrapper array, or a
/// collection - three different things to construct from the same items.
pub(crate) fn configuration_value(limits: &CommandContract) -> Value {
    let item = {
        let text = bounded_string(limits.limit("maximum_configuration_scalar_string_bytes"));
        json!({"anyOf": [text, {"type": "boolean"}]})
    };
    json!({
        "oneOf": [
            {
                "type": "object",
                "additionalProperties": false,
                "required": ["cardinality", "type", "value"],
                "properties": {
                    "cardinality": {"const": "scalar"},
                    "type": {"enum": DECLARED_SCALAR_TYPES},
                    "value": item,
                },
            },
            {
                "type": "object",
                "additionalProperties": false,
                "required": ["cardinality", "type", "values"],
                "properties": {
                    "cardinality":
                        {"enum": ["primitive_array", "scalar_array", "collection"]},
                    "type": {"enum": DECLARED_SCALAR_TYPES},
                    "values": {
                        "type": "array",
                        "maxItems": limits.limit("maximum_configuration_sequence_items"),
                        "items": item,
                    },
                },
            },
        ],
    })
}

/// Returns the schema one content fragment element document satisfies.
///
/// Each element holds either one bounded text value or a bounded ordered list of
/// them, and neither form is a rewriting of the other.
pub(crate) fn content_fragment_elements(limits: &CommandContract) -> Value {
    let value = bounded_string(limits.limit("maximum_property_string_bytes"));
    json!({
        "type": "object",
        "maxProperties": limits.limit("maximum_content_fragment_elements"),
        "additionalProperties": {
            "oneOf": [
                value,
                {
                    "type": "array",
                    "minItems": 1,
                    "maxItems": limits.limit("maximum_content_fragment_element_values"),
                    "items": bounded_string(limits.limit("maximum_property_string_bytes")),
                },
            ],
        },
    })
}

/// Returns both role schemas for one command.
///
/// # Panics
///
/// Panics when asked for a wire name this plan does not define, which is a
/// defect in the caller rather than in any input.
#[must_use]
pub fn command_schema(wire_name: &str, role: SchemaRole) -> Value {
    let limits = CommandContract::embedded();
    let body = page_search_body(wire_name, role, limits)
        .or_else(|| asset_search_body(wire_name, role, limits))
        .or_else(|| inspection_body(wire_name, role, limits))
        .or_else(|| action_body(wire_name, role, limits))
        .or_else(|| crate::command::schema_authoring::body(wire_name, role, limits))
        .or_else(|| crate::command::schema_platform::body(wire_name, role, limits))
        .or_else(|| crate::command::schema_process::body(wire_name, role, limits))
        .or_else(|| crate::command::schema_administration::body(wire_name, role, limits))
        .unwrap_or_else(|| panic!("this registry declares no command named {wire_name}"));
    root(wire_name, role, body)
}

/// Returns the body the four page searches declare, when this is one of them.
fn page_search_body(wire_name: &str, role: SchemaRole, limits: &CommandContract) -> Option<Value> {
    let body = match (wire_name, role) {
        ("query_paths", SchemaRole::Arguments) => discovery_arguments(
            limits,
            json!({
                "primary_node_type":
                    nonempty_string(limits.limit("maximum_primary_node_type_name_bytes")),
                "property_predicates": property_predicates(limits),
            }),
            json!(["root_path"]),
        ),
        ("query_paths", SchemaRole::Result) => discovery_page(
            limits,
            json!({
                "type": "object",
                "additionalProperties": false,
                "required": ["repository_path"],
                "properties": {"repository_path": repository_path(limits)},
            }),
        ),
        ("find_pages_containing_phrase", SchemaRole::Arguments) => discovery_arguments(
            limits,
            json!({"phrase": nonempty_string(limits.limit("maximum_search_phrase_bytes"))}),
            json!(["phrase", "root_path"]),
        ),
        ("find_pages_by_template", SchemaRole::Arguments) => discovery_arguments(
            limits,
            json!({"template_path": repository_path(limits)}),
            json!(["root_path", "template_path"]),
        ),
        ("find_pages_using_components", SchemaRole::Arguments) => discovery_arguments(
            limits,
            json!({
                "match_mode": {"enum": ["any", "all"]},
                "resource_types": {
                    "type": "array",
                    "minItems": 1,
                    "uniqueItems": true,
                    "maxItems": limits.limit("maximum_requested_component_resource_types"),
                    "items": nonempty_string(limits.limit("maximum_component_resource_type_bytes")),
                },
            }),
            json!(["match_mode", "resource_types", "root_path"]),
        ),
        (
            "find_pages_containing_phrase"
            | "find_pages_by_template"
            | "find_pages_using_components",
            SchemaRole::Result,
        ) => discovery_page(limits, page_match(limits)),
        _ => return None,
    };
    Some(body)
}

/// Returns the body the two asset searches declare, when this is one of them.
fn asset_search_body(wire_name: &str, role: SchemaRole, limits: &CommandContract) -> Option<Value> {
    let body = match (wire_name, role) {
        ("find_assets_by_metadata", SchemaRole::Arguments) => {
            find_assets_by_metadata_arguments(limits)
        }
        ("find_assets_by_metadata", SchemaRole::Result) => {
            discovery_page(limits, asset_match(limits))
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

/// Returns the body the two inspection commands declare, when this is one of them.
fn inspection_body(wire_name: &str, role: SchemaRole, limits: &CommandContract) -> Option<Value> {
    let body = match (wire_name, role) {
        ("load_content_as_json", SchemaRole::Arguments) => json!({
            "type": "object",
            "additionalProperties": false,
            "required": ["path"],
            "properties": {
                "path": repository_path(limits),
                "depth": {
                    "type": "integer",
                    "minimum": 0,
                    "maximum": limits.limit("maximum_load_depth"),
                },
            },
        }),
        ("load_content_as_json", SchemaRole::Result) => json!({
            "oneOf": [
                {
                    "type": "object",
                    "additionalProperties": false,
                    "required": ["disposition", "document", "path"],
                    "properties": {
                        "disposition": {"const": "inline"},
                        "document": {"type": "object"},
                        "path": repository_path(limits),
                    },
                },
                {
                    "type": "object",
                    "additionalProperties": false,
                    "required": ["artifact", "disposition", "path"],
                    "properties": {
                        "artifact": artifact_descriptor(limits),
                        "disposition": {"const": "artifact"},
                        "path": repository_path(limits),
                    },
                },
            ],
        }),
        ("inspect_open_service_gateway_initiative_configuration", SchemaRole::Arguments) => json!({
            "type": "object",
            "additionalProperties": false,
            "required": ["persistent_identifier"],
            "properties": {
                "persistent_identifier": nonempty_string(
                    limits.limit("maximum_configuration_persistent_identifier_bytes"),
                ),
            },
        }),
        ("inspect_open_service_gateway_initiative_configuration", SchemaRole::Result) => json!({
            "type": "object",
            "additionalProperties": false,
            "required": ["persistent_identifier", "present", "properties"],
            "properties": {
                "persistent_identifier": nonempty_string(
                    limits.limit("maximum_configuration_persistent_identifier_bytes"),
                ),
                "present": {"type": "boolean"},
                "properties": {
                    "type": "object",
                    "maxProperties": limits.limit("maximum_inspected_configuration_properties"),
                    "additionalProperties": {
                        "type": "object",
                        "additionalProperties": false,
                        "required": ["metatype_evidence", "observation"],
                        "properties": {
                            "metatype_evidence":
                                {"enum": ["password", "non_password", "unavailable"]},
                            "observation": {"type": "object"},
                        },
                    },
                },
            },
        }),
        _ => return None,
    };
    Some(body)
}

/// Returns the body the four action commands declare, when this is one of them.
fn action_body(wire_name: &str, role: SchemaRole, limits: &CommandContract) -> Option<Value> {
    let body = match (wire_name, role) {
        ("replicate_content", SchemaRole::Arguments) => json!({
            "type": "object",
            "additionalProperties": false,
            "required": ["path", "recursive"],
            "properties": {
                "path": repository_path(limits),
                "recursive": {"type": "boolean"},
            },
        }),
        ("replicate_content", SchemaRole::Result) => json!({
            "type": "object",
            "additionalProperties": false,
            "required": ["accepted_item_count"],
            "properties": {
                "accepted_item_count": {
                    "type": "integer",
                    "minimum": 0,
                    "maximum": limits.limit("maximum_replication_candidate_paths"),
                },
            },
        }),
        ("download_content_package", SchemaRole::Arguments) => {
            download_content_package_arguments(limits)
        }
        ("download_content_package", SchemaRole::Result) => json!({
            "type": "object",
            "additionalProperties": false,
            "required": ["artifact"],
            "properties": {"artifact": artifact_descriptor(limits)},
        }),
        ("create_page", SchemaRole::Arguments) => json!({
            "type": "object",
            "additionalProperties": false,
            "required": ["page_name", "parent_path", "template_path", "title"],
            "properties": {
                "page_name": nonempty_string(limits.limit("maximum_page_name_bytes")),
                "parent_path": repository_path(limits),
                "template_path": repository_path(limits),
                "title": bounded_string(limits.limit("maximum_property_string_bytes")),
                "initial_properties": mutation_properties(limits),
            },
        }),
        ("add_component", SchemaRole::Arguments) => json!({
            "type": "object",
            "additionalProperties": false,
            "required": ["component_name", "content_parent", "page_path", "resource_type"],
            "properties": {
                "component_name": nonempty_string(limits.limit("maximum_component_name_bytes")),
                "content_parent": {
                    "anyOf": [{"const": "content_root"}, relative_path(limits)],
                },
                "page_path": repository_path(limits),
                "properties": mutation_properties(limits),
                "resource_type":
                    nonempty_string(limits.limit("maximum_component_resource_type_bytes")),
            },
        }),
        ("create_page" | "add_component", SchemaRole::Result) => json!({
            "type": "object",
            "additionalProperties": false,
            "required": ["target_path"],
            "properties": {"target_path": repository_path(limits)},
        }),
        _ => return None,
    };
    Some(body)
}

/// Returns the schema one mutation property map satisfies.
pub(crate) fn mutation_properties(limits: &CommandContract) -> Value {
    json!({
        "type": "object",
        "maxProperties": limits.limit("maximum_mutation_properties"),
        "additionalProperties": property_value(limits),
    })
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

/// Returns the schema the package command's arguments satisfy.
fn download_content_package_arguments(limits: &CommandContract) -> Value {
    let expressions = |maximum: u64| {
        json!({
            "type": "array",
            "maxItems": maximum,
            "items": nonempty_string(limits.limit("maximum_package_selection_expression_bytes")),
        })
    };
    json!({
        "type": "object",
        "additionalProperties": false,
        "required": ["package_name", "roots"],
        "properties": {
            "exclusion_filters": expressions(limits.limit("maximum_package_exclusion_expressions")),
            "inclusion_filters": expressions(limits.limit("maximum_package_inclusion_expressions")),
            "package_name": {
                "type": "string",
                "pattern": "^[A-Za-z0-9_-]+$",
                "maxLength": limits.limit("maximum_package_name_bytes"),
            },
            "roots": {
                "type": "array",
                "minItems": 1,
                "uniqueItems": true,
                "maxItems": limits.limit("maximum_package_roots"),
                "items": repository_path(limits),
            },
        },
    })
}

/// Wraps one command body in the root keywords every schema carries.
fn root(wire_name: &str, role: SchemaRole, body: Value) -> Value {
    let mut document = json!({
        "$schema": SCHEMA_DIALECT,
        "$id": schema_identifier(wire_name, role),
        CANONICAL_CONTRACT_ANNOTATION: canonical_contract_digest(),
    });
    if let (Some(target), Some(source)) = (document.as_object_mut(), body.as_object()) {
        for (name, member) in source {
            target.insert(name.clone(), member.clone());
        }
    }
    document
}

/// Returns the digest of the committed byte contract.
///
/// # Panics
///
/// Panics when the committed contract is not itself canonical, which is a
/// defect in this repository rather than in any input.
#[must_use]
pub fn canonical_contract_digest() -> String {
    /// Bytes of the committed byte contract, embedded at compile time.
    const CONTRACT: &str = include_str!("../../../../schemas/command-canonical-json-1.json");

    let value: Value = serde_json::from_str(CONTRACT).expect("the byte contract is one value");
    canonical_digest(&write_canonical(&value).expect("the byte contract is canonical"))
}

/// Returns the manifest recording every schema's digest.
///
/// # Panics
///
/// Panics when a schema cannot be written canonically, which is a defect here.
#[must_use]
pub fn schema_manifest() -> Value {
    let mut roles = serde_json::Map::new();
    for wire_name in COMMAND_WIRE_NAMES {
        let mut command = serde_json::Map::new();
        for role in SchemaRole::both() {
            let written =
                write_canonical(&command_schema(wire_name, role)).expect("a schema is canonical");
            command.insert(role.as_text().to_owned(), Value::from(canonical_digest(&written)));
        }
        roles.insert((*wire_name).to_owned(), Value::Object(command));
    }
    let limits_digest = canonical_digest(
        &write_canonical(
            &serde_json::from_str::<Value>(CommandContract::embedded_manifest())
                .expect("the limits manifest is one value"),
        )
        .expect("the limits manifest is canonical"),
    );
    json!({
        "canonical_json_contract_sha256": canonical_contract_digest(),
        "command_contract_limits_sha256": limits_digest,
        "command_semantic_contract_version": INITIAL_COMMAND_VERSION,
        "format": SCHEMA_MANIFEST_FORMAT,
        "schemas": Value::Object(roles),
    })
}
