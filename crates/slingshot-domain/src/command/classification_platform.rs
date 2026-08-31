//! Configurations, bundles, components, and resource mapping.
//!
//! These rows are where the widened access definition does its work. Nothing here
//! touches a page, and half of it is plainly not a read: updating a configuration,
//! deleting one, and starting or stopping a bundle each leave the author in a
//! different state than they found it.

use crate::command::catalog::{
    AccessClassification, DestructiveClassification, IntrinsicIdempotencyClassification,
};
use crate::command::classification::ClassificationRow;

/// Delete a configuration.
pub const DELETE_CONFIGURATION: ClassificationRow = ClassificationRow {
    wire_name: "delete_open_service_gateway_initiative_configuration",
    title: "Delete a configuration",
    description: "Removes one configuration by its exact persistent identifier, restoring \
                  whatever default the code carries.",
    access: AccessClassification::Write,
    destructive: DestructiveClassification::Destructive,
    intrinsic_idempotency: IntrinsicIdempotencyClassification::NotIntrinsicallyIdempotent,
    result_bytes_limit: "maximum_mutation_success_result_bytes",
    failure_categories: &[
        "configuration_lookup_failed",
        "configuration_lookup_mismatch",
        "configuration_lookup_ambiguous",
        "platform_control_rejected",
        "platform_control_outcome_unknown",
    ],
    discovery: false,
};

/// Find configurations.
pub const FIND_CONFIGURATIONS: ClassificationRow = ClassificationRow {
    wire_name: "find_open_service_gateway_initiative_configurations",
    title: "Find configurations",
    description: "Finds configurations by persistent-identifier prefix and reports \
                  identifiers, binding, and key counts, never a value.",
    access: AccessClassification::Read,
    destructive: DestructiveClassification::NonDestructive,
    intrinsic_idempotency: IntrinsicIdempotencyClassification::IntrinsicallyIdempotent,
    result_bytes_limit: "maximum_operational_listing_result_bytes",
    failure_categories: &["configuration_lookup_failed", "configuration_lookup_budget_exceeded"],
    discovery: true,
};

/// List bundles.
pub const LIST_BUNDLES: ClassificationRow = ClassificationRow {
    wire_name: "list_open_service_gateway_initiative_bundles",
    title: "List bundles",
    description: "Reports installed bundles by symbolic-name prefix and state, ordered by \
                  name and then version.",
    access: AccessClassification::Read,
    destructive: DestructiveClassification::NonDestructive,
    intrinsic_idempotency: IntrinsicIdempotencyClassification::IntrinsicallyIdempotent,
    result_bytes_limit: "maximum_operational_listing_result_bytes",
    failure_categories: &["bundle_inventory_failed"],
    discovery: true,
};

/// List components.
pub const LIST_COMPONENTS: ClassificationRow = ClassificationRow {
    wire_name: "list_open_service_gateway_initiative_components",
    title: "List components",
    description: "Reports declarative service components by name prefix and state, with the \
                  bundle that declares each one.",
    access: AccessClassification::Read,
    destructive: DestructiveClassification::NonDestructive,
    intrinsic_idempotency: IntrinsicIdempotencyClassification::IntrinsicallyIdempotent,
    result_bytes_limit: "maximum_operational_listing_result_bytes",
    failure_categories: &["component_inventory_failed"],
    discovery: true,
};

/// List resource mappings.
pub const LIST_RESOURCE_MAPPINGS: ClassificationRow = ClassificationRow {
    wire_name: "list_resource_mappings",
    title: "List resource mappings",
    description: "Reports the mapping entries in effect, with each one's pattern, kind, \
                  replacements, and redirect status.",
    access: AccessClassification::Read,
    destructive: DestructiveClassification::NonDestructive,
    intrinsic_idempotency: IntrinsicIdempotencyClassification::IntrinsicallyIdempotent,
    result_bytes_limit: "maximum_operational_listing_result_bytes",
    failure_categories: &["mapping_inventory_failed"],
    discovery: true,
};

/// Map a resource path.
pub const MAP_RESOURCE_PATH: ClassificationRow = ClassificationRow {
    wire_name: "map_resource_path",
    title: "Map a resource path",
    description: "Reports the external address the author would emit for one resource, with \
                  the entries that decided it.",
    access: AccessClassification::Read,
    destructive: DestructiveClassification::NonDestructive,
    intrinsic_idempotency: IntrinsicIdempotencyClassification::IntrinsicallyIdempotent,
    result_bytes_limit: "maximum_operational_inspection_result_bytes",
    failure_categories: &["resolution_failed", "resolution_budget_exceeded"],
    discovery: false,
};

/// Resolve a resource path.
pub const RESOLVE_RESOURCE_PATH: ClassificationRow = ClassificationRow {
    wire_name: "resolve_resource_path",
    title: "Resolve a resource path",
    description: "Reports which resource one request address reaches, with its selectors, \
                  extension, suffix, and the entries that decided it.",
    access: AccessClassification::Read,
    destructive: DestructiveClassification::NonDestructive,
    intrinsic_idempotency: IntrinsicIdempotencyClassification::IntrinsicallyIdempotent,
    result_bytes_limit: "maximum_operational_inspection_result_bytes",
    failure_categories: &[
        "resolution_failed",
        "resolution_budget_exceeded",
        "request_address_rejected",
    ],
    discovery: false,
};

/// Set a bundle state.
pub const SET_BUNDLE_STATE: ClassificationRow = ClassificationRow {
    wire_name: "set_open_service_gateway_initiative_bundle_state",
    title: "Set a bundle state",
    description: "Starts, stops, or refreshes one bundle and reports the state the author \
                  observed afterwards.",
    access: AccessClassification::Write,
    destructive: DestructiveClassification::Destructive,
    intrinsic_idempotency: IntrinsicIdempotencyClassification::NotIntrinsicallyIdempotent,
    result_bytes_limit: "maximum_mutation_success_result_bytes",
    failure_categories: &[
        "bundle_not_found",
        "bundle_transition_refused",
        "platform_control_rejected",
        "platform_control_outcome_unknown",
    ],
    discovery: false,
};

/// Update a configuration.
pub const UPDATE_CONFIGURATION: ClassificationRow = ClassificationRow {
    wire_name: "update_open_service_gateway_initiative_configuration",
    title: "Update a configuration",
    description: "Assigns and removes configuration keys and answers with the identifier and \
                  a count, never with a value.",
    access: AccessClassification::Write,
    destructive: DestructiveClassification::Destructive,
    intrinsic_idempotency: IntrinsicIdempotencyClassification::NotIntrinsicallyIdempotent,
    result_bytes_limit: "maximum_mutation_success_result_bytes",
    failure_categories: &[
        "configuration_lookup_failed",
        "configuration_lookup_mismatch",
        "configuration_lookup_ambiguous",
        "configuration_value_unsupported",
        "configuration_value_malformed",
        "platform_control_rejected",
        "platform_control_outcome_unknown",
    ],
    discovery: false,
};
