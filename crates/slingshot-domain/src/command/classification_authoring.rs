//! Pages, components, assets, and fragments.
//!
//! Every row here is a write except the two listings, and every write is
//! destructive except a creation: creating something that was not there replaces
//! nothing, while changing, moving, or removing something replaces or ends what
//! was already visible.

use crate::command::catalog::{
    AccessClassification, DestructiveClassification, IntrinsicIdempotencyClassification,
};
use crate::command::classification::{ClassificationRow, ROOT_ANCHOR_FAILURES};

/// Create an asset.
pub const CREATE_ASSET: ClassificationRow = ClassificationRow {
    wire_name: "create_asset",
    title: "Create an asset",
    description: "Creates one asset from bytes the request carries inline and reports the \
                  stored original rendition's length.",
    access: AccessClassification::Write,
    destructive: DestructiveClassification::NonDestructive,
    intrinsic_idempotency: IntrinsicIdempotencyClassification::NotIntrinsicallyIdempotent,
    result_bytes_limit: "maximum_mutation_success_result_bytes",
    failure_categories: &[
        "parent_not_found",
        "parent_access_denied",
        "target_already_exists",
        "payload_rejected",
        "payload_too_large",
        "media_type_unsupported",
        "repository_commit_failed",
        "mutation_outcome_unknown",
    ],
    discovery: false,
};

/// Create an asset folder.
pub const CREATE_ASSET_FOLDER: ClassificationRow = ClassificationRow {
    wire_name: "create_asset_folder",
    title: "Create an asset folder",
    description: "Creates one asset folder under a parent and applies the title it was \
                  given.",
    access: AccessClassification::Write,
    destructive: DestructiveClassification::NonDestructive,
    intrinsic_idempotency: IntrinsicIdempotencyClassification::NotIntrinsicallyIdempotent,
    result_bytes_limit: "maximum_mutation_success_result_bytes",
    failure_categories: &[
        "parent_not_found",
        "parent_access_denied",
        "target_already_exists",
        "property_rejected",
        "repository_commit_failed",
        "mutation_outcome_unknown",
    ],
    discovery: false,
};

/// Create a content fragment.
pub const CREATE_CONTENT_FRAGMENT: ClassificationRow = ClassificationRow {
    wire_name: "create_content_fragment",
    title: "Create a content fragment",
    description: "Creates one content fragment under a model and writes its initial element \
                  values.",
    access: AccessClassification::Write,
    destructive: DestructiveClassification::NonDestructive,
    intrinsic_idempotency: IntrinsicIdempotencyClassification::NotIntrinsicallyIdempotent,
    result_bytes_limit: "maximum_mutation_success_result_bytes",
    failure_categories: &[
        "parent_not_found",
        "parent_access_denied",
        "target_already_exists",
        "model_not_found",
        "model_invalid",
        "element_unknown",
        "element_value_rejected",
        "repository_commit_failed",
        "mutation_outcome_unknown",
    ],
    discovery: false,
};

/// Create an experience fragment.
pub const CREATE_EXPERIENCE_FRAGMENT: ClassificationRow = ClassificationRow {
    wire_name: "create_experience_fragment",
    title: "Create an experience fragment",
    description: "Creates one experience fragment from a template together with its first \
                  variation.",
    access: AccessClassification::Write,
    destructive: DestructiveClassification::NonDestructive,
    intrinsic_idempotency: IntrinsicIdempotencyClassification::NotIntrinsicallyIdempotent,
    result_bytes_limit: "maximum_mutation_success_result_bytes",
    failure_categories: &[
        "parent_not_found",
        "parent_access_denied",
        "target_already_exists",
        "template_not_found",
        "template_invalid",
        "repository_commit_failed",
        "mutation_outcome_unknown",
    ],
    discovery: false,
};

/// Delete an asset.
pub const DELETE_ASSET: ClassificationRow = ClassificationRow {
    wire_name: "delete_asset",
    title: "Delete an asset",
    description: "Removes one asset and its subtree under the reference policy the request \
                  states.",
    access: AccessClassification::Write,
    destructive: DestructiveClassification::Destructive,
    intrinsic_idempotency: IntrinsicIdempotencyClassification::NotIntrinsicallyIdempotent,
    result_bytes_limit: "maximum_mutation_success_result_bytes",
    failure_categories: &[
        "asset_not_found",
        "asset_access_denied",
        "asset_invalid",
        "asset_is_referenced",
        "deletion_budget_exceeded",
        "repository_commit_failed",
        "mutation_outcome_unknown",
    ],
    discovery: false,
};

/// Delete a component.
pub const DELETE_COMPONENT: ClassificationRow = ClassificationRow {
    wire_name: "delete_component",
    title: "Delete a component",
    description: "Removes one component resource and the subtree it owns.",
    access: AccessClassification::Write,
    destructive: DestructiveClassification::Destructive,
    intrinsic_idempotency: IntrinsicIdempotencyClassification::NotIntrinsicallyIdempotent,
    result_bytes_limit: "maximum_mutation_success_result_bytes",
    failure_categories: &[
        "component_not_found",
        "component_access_denied",
        "component_invalid",
        "repository_commit_failed",
        "mutation_outcome_unknown",
    ],
    discovery: false,
};

/// Delete a content fragment.
pub const DELETE_CONTENT_FRAGMENT: ClassificationRow = ClassificationRow {
    wire_name: "delete_content_fragment",
    title: "Delete a content fragment",
    description: "Removes one content fragment and every variation it holds, under the \
                  reference policy the request states.",
    access: AccessClassification::Write,
    destructive: DestructiveClassification::Destructive,
    intrinsic_idempotency: IntrinsicIdempotencyClassification::NotIntrinsicallyIdempotent,
    result_bytes_limit: "maximum_mutation_success_result_bytes",
    failure_categories: &[
        "fragment_not_found",
        "fragment_access_denied",
        "fragment_invalid",
        "fragment_is_referenced",
        "deletion_budget_exceeded",
        "repository_commit_failed",
        "mutation_outcome_unknown",
    ],
    discovery: false,
};

/// Delete an experience fragment.
pub const DELETE_EXPERIENCE_FRAGMENT: ClassificationRow = ClassificationRow {
    wire_name: "delete_experience_fragment",
    title: "Delete an experience fragment",
    description: "Removes one experience fragment and every variation it holds, under the \
                  reference policy the request states.",
    access: AccessClassification::Write,
    destructive: DestructiveClassification::Destructive,
    intrinsic_idempotency: IntrinsicIdempotencyClassification::NotIntrinsicallyIdempotent,
    result_bytes_limit: "maximum_mutation_success_result_bytes",
    failure_categories: &[
        "fragment_not_found",
        "fragment_access_denied",
        "fragment_invalid",
        "fragment_is_referenced",
        "deletion_budget_exceeded",
        "repository_commit_failed",
        "mutation_outcome_unknown",
    ],
    discovery: false,
};

/// Delete a page.
pub const DELETE_PAGE: ClassificationRow = ClassificationRow {
    wire_name: "delete_page",
    title: "Delete a page",
    description: "Removes one page and its subtree under the reference policy the request \
                  states, and reports how much went.",
    access: AccessClassification::Write,
    destructive: DestructiveClassification::Destructive,
    intrinsic_idempotency: IntrinsicIdempotencyClassification::NotIntrinsicallyIdempotent,
    result_bytes_limit: "maximum_mutation_success_result_bytes",
    failure_categories: &[
        "target_not_found",
        "target_access_denied",
        "target_not_a_page",
        "target_is_referenced",
        "deletion_budget_exceeded",
        "repository_commit_failed",
        "mutation_outcome_unknown",
    ],
    discovery: false,
};

/// List asset renditions.
pub const LIST_ASSET_RENDITIONS: ClassificationRow = ClassificationRow {
    wire_name: "list_asset_renditions",
    title: "List asset renditions",
    description: "Reports one asset's renditions in ascending name order with each one's \
                  media type and byte length.",
    access: AccessClassification::Read,
    destructive: DestructiveClassification::NonDestructive,
    intrinsic_idempotency: IntrinsicIdempotencyClassification::IntrinsicallyIdempotent,
    result_bytes_limit: "maximum_discovery_result_bytes",
    failure_categories: &["asset_not_found", "asset_access_denied", "asset_invalid"],
    discovery: true,
};

/// List child pages.
pub const LIST_CHILD_PAGES: ClassificationRow = ClassificationRow {
    wire_name: "list_child_pages",
    title: "List child pages",
    description: "Reports the pages that are immediate children of one anchor, and no \
                  deeper.",
    access: AccessClassification::Read,
    destructive: DestructiveClassification::NonDestructive,
    intrinsic_idempotency: IntrinsicIdempotencyClassification::IntrinsicallyIdempotent,
    result_bytes_limit: "maximum_discovery_result_bytes",
    failure_categories: ROOT_ANCHOR_FAILURES,
    discovery: true,
};

/// Move an asset.
pub const MOVE_ASSET: ClassificationRow = ClassificationRow {
    wire_name: "move_asset",
    title: "Move an asset",
    description: "Moves one asset, adjusting or abandoning the references to its old address \
                  as the request states.",
    access: AccessClassification::Write,
    destructive: DestructiveClassification::Destructive,
    intrinsic_idempotency: IntrinsicIdempotencyClassification::NotIntrinsicallyIdempotent,
    result_bytes_limit: "maximum_mutation_success_result_bytes",
    failure_categories: &[
        "source_not_found",
        "source_access_denied",
        "destination_parent_not_found",
        "destination_already_exists",
        "destination_inside_source",
        "reference_adjustment_budget_exceeded",
        "repository_commit_failed",
        "mutation_outcome_unknown",
    ],
    discovery: false,
};

/// Move a page.
pub const MOVE_PAGE: ClassificationRow = ClassificationRow {
    wire_name: "move_page",
    title: "Move a page",
    description: "Moves one page and its subtree, adjusting or abandoning the references to \
                  its old address as the request states.",
    access: AccessClassification::Write,
    destructive: DestructiveClassification::Destructive,
    intrinsic_idempotency: IntrinsicIdempotencyClassification::NotIntrinsicallyIdempotent,
    result_bytes_limit: "maximum_mutation_success_result_bytes",
    failure_categories: &[
        "source_not_found",
        "source_access_denied",
        "destination_parent_not_found",
        "destination_already_exists",
        "destination_inside_source",
        "reference_adjustment_budget_exceeded",
        "repository_commit_failed",
        "mutation_outcome_unknown",
    ],
    discovery: false,
};

/// Read a content fragment.
pub const READ_CONTENT_FRAGMENT: ClassificationRow = ClassificationRow {
    wire_name: "read_content_fragment",
    title: "Read a content fragment",
    description: "Reports one fragment's model, title, variation, and elements as the \
                  fragment's own vocabulary rather than as storage.",
    access: AccessClassification::Read,
    destructive: DestructiveClassification::NonDestructive,
    intrinsic_idempotency: IntrinsicIdempotencyClassification::IntrinsicallyIdempotent,
    result_bytes_limit: "maximum_operational_inspection_result_bytes",
    failure_categories: &[
        "fragment_not_found",
        "fragment_access_denied",
        "fragment_invalid",
        "variation_not_found",
        "result_budget_exceeded",
    ],
    discovery: false,
};

/// Reorder a component.
pub const REORDER_COMPONENT: ClassificationRow = ClassificationRow {
    wire_name: "reorder_component",
    title: "Reorder a component",
    description: "Moves one component within its orderable parent, before a named sibling or \
                  after every one.",
    access: AccessClassification::Write,
    destructive: DestructiveClassification::Destructive,
    intrinsic_idempotency: IntrinsicIdempotencyClassification::NotIntrinsicallyIdempotent,
    result_bytes_limit: "maximum_mutation_success_result_bytes",
    failure_categories: &[
        "component_not_found",
        "component_access_denied",
        "parent_not_orderable",
        "sibling_not_found",
        "repository_commit_failed",
        "mutation_outcome_unknown",
    ],
    discovery: false,
};

/// Update asset metadata.
pub const UPDATE_ASSET_METADATA: ClassificationRow = ClassificationRow {
    wire_name: "update_asset_metadata",
    title: "Update asset metadata",
    description: "Applies a property document and a removal list to one asset's metadata \
                  resource.",
    access: AccessClassification::Write,
    destructive: DestructiveClassification::Destructive,
    intrinsic_idempotency: IntrinsicIdempotencyClassification::NotIntrinsicallyIdempotent,
    result_bytes_limit: "maximum_mutation_success_result_bytes",
    failure_categories: &[
        "asset_not_found",
        "asset_access_denied",
        "asset_invalid",
        "property_rejected",
        "property_not_removable",
        "repository_commit_failed",
        "mutation_outcome_unknown",
    ],
    discovery: false,
};

/// Update a component.
pub const UPDATE_COMPONENT: ClassificationRow = ClassificationRow {
    wire_name: "update_component",
    title: "Update a component",
    description: "Applies a property document and a removal list to one component resource.",
    access: AccessClassification::Write,
    destructive: DestructiveClassification::Destructive,
    intrinsic_idempotency: IntrinsicIdempotencyClassification::NotIntrinsicallyIdempotent,
    result_bytes_limit: "maximum_mutation_success_result_bytes",
    failure_categories: &[
        "component_not_found",
        "component_access_denied",
        "component_invalid",
        "property_rejected",
        "property_not_removable",
        "repository_commit_failed",
        "mutation_outcome_unknown",
    ],
    discovery: false,
};

/// Update a content fragment.
pub const UPDATE_CONTENT_FRAGMENT: ClassificationRow = ClassificationRow {
    wire_name: "update_content_fragment",
    title: "Update a content fragment",
    description: "Applies a title and element values to one variation of one content \
                  fragment.",
    access: AccessClassification::Write,
    destructive: DestructiveClassification::Destructive,
    intrinsic_idempotency: IntrinsicIdempotencyClassification::NotIntrinsicallyIdempotent,
    result_bytes_limit: "maximum_mutation_success_result_bytes",
    failure_categories: &[
        "fragment_not_found",
        "fragment_access_denied",
        "fragment_invalid",
        "variation_not_found",
        "element_unknown",
        "element_value_rejected",
        "repository_commit_failed",
        "mutation_outcome_unknown",
    ],
    discovery: false,
};

/// Update an experience fragment.
pub const UPDATE_EXPERIENCE_FRAGMENT: ClassificationRow = ClassificationRow {
    wire_name: "update_experience_fragment",
    title: "Update an experience fragment",
    description: "Applies a title, a property document, and a removal list to one experience \
                  fragment variation.",
    access: AccessClassification::Write,
    destructive: DestructiveClassification::Destructive,
    intrinsic_idempotency: IntrinsicIdempotencyClassification::NotIntrinsicallyIdempotent,
    result_bytes_limit: "maximum_mutation_success_result_bytes",
    failure_categories: &[
        "variation_not_found",
        "variation_access_denied",
        "variation_invalid",
        "property_rejected",
        "property_not_removable",
        "repository_commit_failed",
        "mutation_outcome_unknown",
    ],
    discovery: false,
};

/// Update a page.
pub const UPDATE_PAGE: ClassificationRow = ClassificationRow {
    wire_name: "update_page",
    title: "Update a page",
    description: "Applies a title, a property document, and a removal list to one page's \
                  content resource.",
    access: AccessClassification::Write,
    destructive: DestructiveClassification::Destructive,
    intrinsic_idempotency: IntrinsicIdempotencyClassification::NotIntrinsicallyIdempotent,
    result_bytes_limit: "maximum_mutation_success_result_bytes",
    failure_categories: &[
        "page_not_found",
        "page_access_denied",
        "page_invalid",
        "property_rejected",
        "property_not_removable",
        "repository_commit_failed",
        "mutation_outcome_unknown",
    ],
    discovery: false,
};
