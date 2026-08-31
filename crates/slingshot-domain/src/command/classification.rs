//! What the registry knows about each command, and where each row is written.
//!
//! Plan 0003 kept one table of twelve rows in one file. Sixty-four rows do not
//! fit one file under this repository's own size rule, so the rows live beside
//! the families they describe and the table lives here: one array, one ascending
//! order, one place a reader goes to see the whole surface at once.
//!
//! What did not change is that a row is data rather than a rule. Whether
//! packaging is a read, whether cancelling a job is destructive, whether a
//! deletion is idempotent - each is a judgement somebody made, and each is
//! written down where it can be read and argued with rather than inferred from a
//! name.
//!
//! Four groups of failure categories are shared, because they are the same
//! answer to the same question wherever they appear: a repository write can fail
//! to commit or leave nobody able to tell; a command addressed by one path can
//! find nothing or be refused; a command that changes retained state outside the
//! repository can be refused or leave its outcome unknown; and any windowed
//! command can exhaust a budget or be handed a continuation token it cannot use.

use crate::command::catalog::{
    AccessClassification, DestructiveClassification, IntrinsicIdempotencyClassification,
};
use crate::command::{
    classification_administration, classification_authoring, classification_foundation,
    classification_platform, classification_process,
};

/// One row of the closed classification table.
///
/// Data rather than a rule, because there is no rule: whether packaging is a
/// read is a judgement somebody made, and it belongs written down where it can
/// be read and argued with.
pub struct ClassificationRow {
    /// Stable name.
    pub wire_name: &'static str,
    /// Human title.
    pub title: &'static str,
    /// Present-state description.
    pub description: &'static str,
    /// Whether it changes state the author retains.
    pub access: AccessClassification,
    /// Whether a success can replace or end something already in effect.
    pub destructive: DestructiveClassification,
    /// Whether running it twice is running it once.
    pub intrinsic_idempotency: IntrinsicIdempotencyClassification,
    /// Limit naming its largest canonical success result.
    pub result_bytes_limit: &'static str,
    /// Failure categories this version allows beside the shared ones.
    pub failure_categories: &'static [&'static str],
    /// Whether the shared discovery categories apply.
    pub discovery: bool,
}

/// Failure categories every windowed command allows.
pub const DISCOVERY_FAILURES: &[&str] = &[
    "discovery_budget_exceeded",
    "continuation_token_malformed",
    "continuation_token_integrity_invalid",
    "continuation_token_wrong_target",
    "continuation_token_wrong_query",
    "continuation_token_expired",
];

/// Anchor failures the rooted discovery commands allow.
pub const ROOT_ANCHOR_FAILURES: &[&str] = &["root_not_found", "root_access_denied"];

/// The closed sixty-four-row table, in ascending wire-name order.
pub const CLASSIFICATIONS: &[ClassificationRow] = &[
    classification_foundation::ADD_COMPONENT,
    classification_administration::ADD_GROUP_MEMBER,
    classification_process::CANCEL_SLING_JOB,
    classification_authoring::CREATE_ASSET,
    classification_authoring::CREATE_ASSET_FOLDER,
    classification_authoring::CREATE_CONTENT_FRAGMENT,
    classification_authoring::CREATE_EXPERIENCE_FRAGMENT,
    classification_administration::CREATE_GROUP,
    classification_foundation::CREATE_PAGE,
    classification_administration::CREATE_USER,
    classification_authoring::DELETE_ASSET,
    classification_administration::DELETE_AUTHORIZABLE,
    classification_authoring::DELETE_COMPONENT,
    classification_authoring::DELETE_CONTENT_FRAGMENT,
    classification_authoring::DELETE_EXPERIENCE_FRAGMENT,
    classification_platform::DELETE_CONFIGURATION,
    classification_authoring::DELETE_PAGE,
    classification_foundation::DOWNLOAD_CONTENT_PACKAGE,
    classification_foundation::FIND_ASSETS_BY_METADATA,
    classification_foundation::FIND_ASSETS_REFERENCED_BY_PAGE,
    classification_platform::FIND_CONFIGURATIONS,
    classification_foundation::FIND_PAGES_BY_TEMPLATE,
    classification_foundation::FIND_PAGES_CONTAINING_PHRASE,
    classification_foundation::FIND_PAGES_USING_COMPONENTS,
    classification_process::FIND_SLING_JOBS,
    classification_process::FIND_WORKFLOW_INSTANCES,
    classification_administration::FLUSH_REPLICATION_QUEUE,
    classification_foundation::INSPECT_CONFIGURATION,
    classification_administration::INSPECT_REPLICATION_AGENT,
    classification_administration::INSPECT_REPLICATION_QUEUE,
    classification_process::INSPECT_SLING_JOB,
    classification_process::INSPECT_WORKFLOW_INSTANCE,
    classification_authoring::LIST_ASSET_RENDITIONS,
    classification_authoring::LIST_CHILD_PAGES,
    classification_administration::LIST_GROUP_MEMBERS,
    classification_platform::LIST_BUNDLES,
    classification_platform::LIST_COMPONENTS,
    classification_administration::LIST_REPLICATION_AGENTS,
    classification_platform::LIST_RESOURCE_MAPPINGS,
    classification_process::LIST_SLING_JOB_QUEUES,
    classification_process::LIST_WORKFLOW_MODELS,
    classification_foundation::LOAD_CONTENT_AS_JSON,
    classification_platform::MAP_RESOURCE_PATH,
    classification_authoring::MOVE_ASSET,
    classification_authoring::MOVE_PAGE,
    classification_foundation::QUERY_PATHS,
    classification_authoring::READ_CONTENT_FRAGMENT,
    classification_administration::REMOVE_GROUP_MEMBER,
    classification_authoring::REORDER_COMPONENT,
    classification_foundation::REPLICATE_CONTENT,
    classification_platform::RESOLVE_RESOURCE_PATH,
    classification_administration::RETRY_REPLICATION_QUEUE_ENTRY,
    classification_platform::SET_BUNDLE_STATE,
    classification_administration::SET_USER_DISABLED,
    classification_process::SET_WORKFLOW_INSTANCE_SUSPENSION,
    classification_process::START_WORKFLOW,
    classification_process::TERMINATE_WORKFLOW_INSTANCE,
    classification_authoring::UPDATE_ASSET_METADATA,
    classification_authoring::UPDATE_COMPONENT,
    classification_authoring::UPDATE_CONTENT_FRAGMENT,
    classification_authoring::UPDATE_EXPERIENCE_FRAGMENT,
    classification_platform::UPDATE_CONFIGURATION,
    classification_authoring::UPDATE_PAGE,
    classification_administration::UPDATE_USER_PROFILE,
];
