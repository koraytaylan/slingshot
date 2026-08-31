//! The twelve rows Plan 0003 published.
//!
//! They are here unchanged. Plan 0010 widened what `Read` and `Write` mean, from
//! repository content to any state the author retains, and that widening changes
//! no answer for any of these - which is the point of writing it down rather than
//! rederiving it.

use crate::command::catalog::{
    AccessClassification, DestructiveClassification, IntrinsicIdempotencyClassification,
};
use crate::command::classification::{ClassificationRow, ROOT_ANCHOR_FAILURES};

/// Add a component.
pub const ADD_COMPONENT: ClassificationRow = ClassificationRow {
    wire_name: "add_component",
    title: "Add a component",
    description: "Creates one component under a page's content resource and appends it \
                  last in its orderable parent.",
    access: AccessClassification::Write,
    destructive: DestructiveClassification::NonDestructive,
    intrinsic_idempotency: IntrinsicIdempotencyClassification::NotIntrinsicallyIdempotent,
    result_bytes_limit: "maximum_mutation_success_result_bytes",
    failure_categories: &[
        "page_not_found",
        "page_invalid",
        "parent_not_found",
        "parent_access_denied",
        "parent_not_orderable",
        "target_already_exists",
        "property_rejected",
        "repository_commit_failed",
        "mutation_outcome_unknown",
    ],
    discovery: false,
};

/// Create a page.
pub const CREATE_PAGE: ClassificationRow = ClassificationRow {
    wire_name: "create_page",
    title: "Create a page",
    description: "Creates one page from a template and applies its title and initial \
                  properties to the new page's content resource.",
    access: AccessClassification::Write,
    destructive: DestructiveClassification::NonDestructive,
    intrinsic_idempotency: IntrinsicIdempotencyClassification::NotIntrinsicallyIdempotent,
    result_bytes_limit: "maximum_mutation_success_result_bytes",
    failure_categories: &[
        "target_already_exists",
        "parent_not_found",
        "parent_access_denied",
        "template_not_found",
        "template_invalid",
        "property_rejected",
        "repository_commit_failed",
        "mutation_outcome_unknown",
    ],
    discovery: false,
};

/// Download a content package.
pub const DOWNLOAD_CONTENT_PACKAGE: ClassificationRow = ClassificationRow {
    wire_name: "download_content_package",
    title: "Download a content package",
    description: "Builds one FileVault content package from roots and ordered selection \
                  filters and returns its artifact metadata.",
    access: AccessClassification::Read,
    destructive: DestructiveClassification::NonDestructive,
    intrinsic_idempotency: IntrinsicIdempotencyClassification::NotIntrinsicallyIdempotent,
    result_bytes_limit: "maximum_command_result_bytes",
    failure_categories: &[
        "pattern_rejected",
        "filevault_profile_unsupported",
        "filevault_filter_unrepresentable",
        "root_not_found",
        "root_access_denied",
        "repository_read_failed",
        "filevault_package_failed",
        "staging_cleanup_failed",
        "artifact_publication_failed",
        "artifact_publication_outcome_unknown",
        "evaluation_budget_exceeded",
    ],
    discovery: false,
};

/// Find assets by metadata.
pub const FIND_ASSETS_BY_METADATA: ClassificationRow = ClassificationRow {
    wire_name: "find_assets_by_metadata",
    title: "Find assets by metadata",
    description: "Finds assets under an anchor by media format, original-rendition size, \
                  tags, and property predicates.",
    access: AccessClassification::Read,
    destructive: DestructiveClassification::NonDestructive,
    intrinsic_idempotency: IntrinsicIdempotencyClassification::IntrinsicallyIdempotent,
    result_bytes_limit: "maximum_discovery_result_bytes",
    failure_categories: ROOT_ANCHOR_FAILURES,
    discovery: true,
};

/// Find assets referenced by a page.
pub const FIND_ASSETS_REFERENCED_BY_PAGE: ClassificationRow = ClassificationRow {
    wire_name: "find_assets_referenced_by_page",
    title: "Find assets referenced by a page",
    description: "Reports the assets one page refers to and the relative property paths \
                  it refers to them from.",
    access: AccessClassification::Read,
    destructive: DestructiveClassification::NonDestructive,
    intrinsic_idempotency: IntrinsicIdempotencyClassification::IntrinsicallyIdempotent,
    result_bytes_limit: "maximum_discovery_result_bytes",
    failure_categories: &["page_not_found", "page_access_denied", "page_invalid"],
    discovery: true,
};

/// Find pages by template.
pub const FIND_PAGES_BY_TEMPLATE: ClassificationRow = ClassificationRow {
    wire_name: "find_pages_by_template",
    title: "Find pages by template",
    description: "Finds pages under an anchor whose recorded template equals one \
                  repository address.",
    access: AccessClassification::Read,
    destructive: DestructiveClassification::NonDestructive,
    intrinsic_idempotency: IntrinsicIdempotencyClassification::IntrinsicallyIdempotent,
    result_bytes_limit: "maximum_discovery_result_bytes",
    failure_categories: ROOT_ANCHOR_FAILURES,
    discovery: true,
};

/// Find pages containing a phrase.
pub const FIND_PAGES_CONTAINING_PHRASE: ClassificationRow = ClassificationRow {
    wire_name: "find_pages_containing_phrase",
    title: "Find pages containing a phrase",
    description: "Finds pages under an anchor holding one exact phrase as a contiguous \
                  sequence of Unicode scalar values.",
    access: AccessClassification::Read,
    destructive: DestructiveClassification::NonDestructive,
    intrinsic_idempotency: IntrinsicIdempotencyClassification::IntrinsicallyIdempotent,
    result_bytes_limit: "maximum_discovery_result_bytes",
    failure_categories: ROOT_ANCHOR_FAILURES,
    discovery: true,
};

/// Find pages using components.
pub const FIND_PAGES_USING_COMPONENTS: ClassificationRow = ClassificationRow {
    wire_name: "find_pages_using_components",
    title: "Find pages using components",
    description: "Finds pages under an anchor whose subtree uses any or all of the \
                  requested component resource types.",
    access: AccessClassification::Read,
    destructive: DestructiveClassification::NonDestructive,
    intrinsic_idempotency: IntrinsicIdempotencyClassification::IntrinsicallyIdempotent,
    result_bytes_limit: "maximum_discovery_result_bytes",
    failure_categories: ROOT_ANCHOR_FAILURES,
    discovery: true,
};

/// Inspect a configuration.
pub const INSPECT_CONFIGURATION: ClassificationRow = ClassificationRow {
    wire_name: "inspect_open_service_gateway_initiative_configuration",
    title: "Inspect a configuration",
    description: "Reads one effective configuration by its exact persistent identifier, \
                  redacting every value the evidence does not clear.",
    access: AccessClassification::Read,
    destructive: DestructiveClassification::NonDestructive,
    intrinsic_idempotency: IntrinsicIdempotencyClassification::IntrinsicallyIdempotent,
    result_bytes_limit: "maximum_inspected_configuration_result_bytes",
    failure_categories: &[
        "configuration_lookup_failed",
        "configuration_lookup_mismatch",
        "configuration_lookup_ambiguous",
        "configuration_lookup_budget_exceeded",
        "configuration_value_unsupported",
        "configuration_value_malformed",
        "configuration_value_budget_exceeded",
        "configuration_result_budget_exceeded",
    ],
    discovery: false,
};

/// Load content as JSON.
pub const LOAD_CONTENT_AS_JSON: ClassificationRow = ClassificationRow {
    wire_name: "load_content_as_json",
    title: "Load content as JSON",
    description: "Reads one repository subtree to a bounded depth and returns it inline \
                  or as an artifact, decided by the document's own canonical bytes.",
    access: AccessClassification::Read,
    destructive: DestructiveClassification::NonDestructive,
    intrinsic_idempotency: IntrinsicIdempotencyClassification::NotIntrinsicallyIdempotent,
    result_bytes_limit: "maximum_command_result_bytes",
    failure_categories: &[
        "not_found",
        "access_denied",
        "unsupported_repository_value",
        "load_budget_exceeded",
    ],
    discovery: false,
};

/// Query paths.
pub const QUERY_PATHS: ClassificationRow = ClassificationRow {
    wire_name: "query_paths",
    title: "Query paths",
    description: "Finds nodes under an anchor by primary type and a bounded collection \
                  of property predicates.",
    access: AccessClassification::Read,
    destructive: DestructiveClassification::NonDestructive,
    intrinsic_idempotency: IntrinsicIdempotencyClassification::IntrinsicallyIdempotent,
    result_bytes_limit: "maximum_discovery_result_bytes",
    failure_categories: ROOT_ANCHOR_FAILURES,
    discovery: true,
};

/// Replicate content.
pub const REPLICATE_CONTENT: ClassificationRow = ClassificationRow {
    wire_name: "replicate_content",
    title: "Replicate content",
    description: "Offers one path, or a path and its descendants, to the author \
                  replication service and reports what was admitted.",
    access: AccessClassification::Write,
    destructive: DestructiveClassification::Destructive,
    intrinsic_idempotency: IntrinsicIdempotencyClassification::NotIntrinsicallyIdempotent,
    result_bytes_limit: "maximum_replication_result_bytes",
    failure_categories: &[
        "source_not_found",
        "source_access_denied",
        "candidate_limit_exceeded",
        "traversal_budget_exceeded",
        "admission_rejected",
        "admission_budget_exceeded",
        "admission_outcome_unknown",
    ],
    discovery: false,
};
