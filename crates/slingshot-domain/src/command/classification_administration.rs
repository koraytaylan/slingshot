//! Authorizables and replication queues.
//!
//! Two subjects that have nothing to do with each other and one property in
//! common: both are administration of the author rather than of its content, and
//! both carry rows whose refusals prove no effect precisely because somebody will
//! run them in a hurry.

use crate::command::catalog::{
    AccessClassification, DestructiveClassification, IntrinsicIdempotencyClassification,
};
use crate::command::classification::ClassificationRow;

/// Add a group member.
pub const ADD_GROUP_MEMBER: ClassificationRow = ClassificationRow {
    wire_name: "add_group_member",
    title: "Add a group member",
    description: "Adds one authorizable to one group and reports whether the membership was \
                  already there.",
    access: AccessClassification::Write,
    destructive: DestructiveClassification::NonDestructive,
    intrinsic_idempotency: IntrinsicIdempotencyClassification::NotIntrinsicallyIdempotent,
    result_bytes_limit: "maximum_mutation_success_result_bytes",
    failure_categories: &[
        "group_not_found",
        "member_not_found",
        "authorizable_kind_mismatch",
        "authorizable_access_denied",
        "membership_cycle_refused",
        "repository_commit_failed",
        "mutation_outcome_unknown",
    ],
    discovery: false,
};

/// Create a group.
pub const CREATE_GROUP: ClassificationRow = ClassificationRow {
    wire_name: "create_group",
    title: "Create a group",
    description: "Creates one group under the authorizable root and reports where the author \
                  placed it.",
    access: AccessClassification::Write,
    destructive: DestructiveClassification::NonDestructive,
    intrinsic_idempotency: IntrinsicIdempotencyClassification::NotIntrinsicallyIdempotent,
    result_bytes_limit: "maximum_mutation_success_result_bytes",
    failure_categories: &[
        "authorizable_already_exists",
        "identifier_rejected",
        "intermediate_path_rejected",
        "property_rejected",
        "authorizable_access_denied",
        "repository_commit_failed",
        "mutation_outcome_unknown",
    ],
    discovery: false,
};

/// Create a user.
pub const CREATE_USER: ClassificationRow = ClassificationRow {
    wire_name: "create_user",
    title: "Create a user",
    description: "Creates one user under the authorizable root, carrying no credential, and \
                  reports where the author placed it.",
    access: AccessClassification::Write,
    destructive: DestructiveClassification::NonDestructive,
    intrinsic_idempotency: IntrinsicIdempotencyClassification::NotIntrinsicallyIdempotent,
    result_bytes_limit: "maximum_mutation_success_result_bytes",
    failure_categories: &[
        "authorizable_already_exists",
        "identifier_rejected",
        "intermediate_path_rejected",
        "property_rejected",
        "authorizable_access_denied",
        "repository_commit_failed",
        "mutation_outcome_unknown",
    ],
    discovery: false,
};

/// Delete an authorizable.
pub const DELETE_AUTHORIZABLE: ClassificationRow = ClassificationRow {
    wire_name: "delete_authorizable",
    title: "Delete an authorizable",
    description: "Removes one user or group, refusing a kind other than the one the request \
                  expects.",
    access: AccessClassification::Write,
    destructive: DestructiveClassification::Destructive,
    intrinsic_idempotency: IntrinsicIdempotencyClassification::NotIntrinsicallyIdempotent,
    result_bytes_limit: "maximum_mutation_success_result_bytes",
    failure_categories: &[
        "authorizable_not_found",
        "authorizable_kind_mismatch",
        "authorizable_access_denied",
        "group_has_members",
        "repository_commit_failed",
        "mutation_outcome_unknown",
    ],
    discovery: false,
};

/// Flush a replication queue.
pub const FLUSH_REPLICATION_QUEUE: ClassificationRow = ClassificationRow {
    wire_name: "flush_replication_queue",
    title: "Flush a replication queue",
    description: "Empties one agent's queue, refusing before it removes anything when the \
                  queue is not the length the request expects.",
    access: AccessClassification::Write,
    destructive: DestructiveClassification::Destructive,
    intrinsic_idempotency: IntrinsicIdempotencyClassification::NotIntrinsicallyIdempotent,
    result_bytes_limit: "maximum_mutation_success_result_bytes",
    failure_categories: &[
        "agent_not_found",
        "agent_access_denied",
        "queue_expectation_mismatch",
        "platform_control_rejected",
        "platform_control_outcome_unknown",
    ],
    discovery: false,
};

/// Inspect a replication agent.
pub const INSPECT_REPLICATION_AGENT: ClassificationRow = ClassificationRow {
    wire_name: "inspect_replication_agent",
    title: "Inspect a replication agent",
    description: "Reports one agent's state, transport kind, queue depth, and retry delay, \
                  and never its transport address.",
    access: AccessClassification::Read,
    destructive: DestructiveClassification::NonDestructive,
    intrinsic_idempotency: IntrinsicIdempotencyClassification::IntrinsicallyIdempotent,
    result_bytes_limit: "maximum_operational_inspection_result_bytes",
    failure_categories: &["agent_not_found", "agent_access_denied", "agent_inventory_failed"],
    discovery: false,
};

/// Inspect a replication queue.
pub const INSPECT_REPLICATION_QUEUE: ClassificationRow = ClassificationRow {
    wire_name: "inspect_replication_queue",
    title: "Inspect a replication queue",
    description: "Reports one agent's queued entries, their actions and attempt counts, and \
                  whether the queue is blocked.",
    access: AccessClassification::Read,
    destructive: DestructiveClassification::NonDestructive,
    intrinsic_idempotency: IntrinsicIdempotencyClassification::IntrinsicallyIdempotent,
    result_bytes_limit: "maximum_operational_listing_result_bytes",
    failure_categories: &["agent_not_found", "agent_access_denied", "queue_inventory_failed"],
    discovery: true,
};

/// List group members.
pub const LIST_GROUP_MEMBERS: ClassificationRow = ClassificationRow {
    wire_name: "list_group_members",
    title: "List group members",
    description: "Reports one group's members, saying of each whether the membership is held \
                  on that group itself.",
    access: AccessClassification::Read,
    destructive: DestructiveClassification::NonDestructive,
    intrinsic_idempotency: IntrinsicIdempotencyClassification::IntrinsicallyIdempotent,
    result_bytes_limit: "maximum_operational_listing_result_bytes",
    failure_categories: &[
        "group_not_found",
        "authorizable_kind_mismatch",
        "authorizable_access_denied",
    ],
    discovery: true,
};

/// List replication agents.
pub const LIST_REPLICATION_AGENTS: ClassificationRow = ClassificationRow {
    wire_name: "list_replication_agents",
    title: "List replication agents",
    description: "Reports every replication agent's state, transport kind, and queue depth, \
                  and never a transport address.",
    access: AccessClassification::Read,
    destructive: DestructiveClassification::NonDestructive,
    intrinsic_idempotency: IntrinsicIdempotencyClassification::IntrinsicallyIdempotent,
    result_bytes_limit: "maximum_operational_listing_result_bytes",
    failure_categories: &["agent_inventory_failed"],
    discovery: true,
};

/// Remove a group member.
pub const REMOVE_GROUP_MEMBER: ClassificationRow = ClassificationRow {
    wire_name: "remove_group_member",
    title: "Remove a group member",
    description: "Removes one authorizable from one group and reports whether the membership \
                  existed at all.",
    access: AccessClassification::Write,
    destructive: DestructiveClassification::Destructive,
    intrinsic_idempotency: IntrinsicIdempotencyClassification::NotIntrinsicallyIdempotent,
    result_bytes_limit: "maximum_mutation_success_result_bytes",
    failure_categories: &[
        "group_not_found",
        "member_not_found",
        "authorizable_kind_mismatch",
        "authorizable_access_denied",
        "membership_cycle_refused",
        "repository_commit_failed",
        "mutation_outcome_unknown",
    ],
    discovery: false,
};

/// Retry a replication queue entry.
pub const RETRY_REPLICATION_QUEUE_ENTRY: ClassificationRow = ClassificationRow {
    wire_name: "retry_replication_queue_entry",
    title: "Retry a replication queue entry",
    description: "Puts one queued entry back to be tried again and reports whether it was \
                  actually resubmitted.",
    access: AccessClassification::Write,
    destructive: DestructiveClassification::NonDestructive,
    intrinsic_idempotency: IntrinsicIdempotencyClassification::NotIntrinsicallyIdempotent,
    result_bytes_limit: "maximum_mutation_success_result_bytes",
    failure_categories: &[
        "agent_not_found",
        "agent_access_denied",
        "entry_not_found",
        "platform_control_rejected",
        "platform_control_outcome_unknown",
    ],
    discovery: false,
};

/// Disable or enable a user.
pub const SET_USER_DISABLED: ClassificationRow = ClassificationRow {
    wire_name: "set_user_disabled",
    title: "Disable or enable a user",
    description: "Disables or enables one user, carrying a reason only when it disables.",
    access: AccessClassification::Write,
    destructive: DestructiveClassification::Destructive,
    intrinsic_idempotency: IntrinsicIdempotencyClassification::NotIntrinsicallyIdempotent,
    result_bytes_limit: "maximum_mutation_success_result_bytes",
    failure_categories: &[
        "authorizable_not_found",
        "authorizable_kind_mismatch",
        "authorizable_access_denied",
        "platform_control_rejected",
        "platform_control_outcome_unknown",
    ],
    discovery: false,
};

/// Update a user profile.
pub const UPDATE_USER_PROFILE: ClassificationRow = ClassificationRow {
    wire_name: "update_user_profile",
    title: "Update a user profile",
    description: "Applies a property document and a removal list to one user's profile \
                  resource.",
    access: AccessClassification::Write,
    destructive: DestructiveClassification::Destructive,
    intrinsic_idempotency: IntrinsicIdempotencyClassification::NotIntrinsicallyIdempotent,
    result_bytes_limit: "maximum_mutation_success_result_bytes",
    failure_categories: &[
        "authorizable_not_found",
        "authorizable_kind_mismatch",
        "authorizable_access_denied",
        "property_rejected",
        "property_not_removable",
        "repository_commit_failed",
        "mutation_outcome_unknown",
    ],
    discovery: false,
};
