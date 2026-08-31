//! Every statement this crate may run, written down once.
//!
//! A closed inventory is worth the bother because the ways a database can
//! surprise you are mostly statements nobody reviewed: a dynamic string built
//! from a caller's value, an `ATTACH` that reaches another file, a `VACUUM` that
//! writes a whole second database beside this one, a sort large enough to spill.
//! A statement that is not here cannot ship.
//!
//! Each entry carries its own bounded shape, so reviewing the list is reviewing
//! the database's whole behaviour rather than reading the code that calls it.

/// One statement this crate may run.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct InventoriedStatement {
    /// What it is for.
    pub purpose: &'static str,
    /// The exact text, with bind markers and no interpolation.
    pub text: &'static str,
    /// How many parameters it binds.
    pub parameters: usize,
    /// How many rows it can return.
    pub maximum_rows: u64,
}

/// Constructs this crate may never use.
///
/// Each of them either reaches a file outside the whitelist, writes one the
/// accounting does not know about, or takes its text from somewhere other than
/// this inventory.
pub const FORBIDDEN_CONSTRUCTS: &[&str] = &[
    "ATTACH",
    "DETACH",
    "VACUUM",
    "CREATE TEMP",
    "CREATE TEMPORARY",
    "PRAGMA TEMP_STORE_DIRECTORY",
];

/// One row the listing statement can return.
const LISTING_ROWS: u64 = 256;

/// One row a keyed lookup can return.
const SINGLE_ROW: u64 = 1;

/// Physical Sling jobs one logical submission may be carried by.
const PHYSICAL_JOB_ROWS: u64 = 32;

/// Every statement, in the order a reader would want to read them.
pub const STATEMENTS: &[InventoriedStatement] = &[
    InventoriedStatement {
        purpose: "record this installation's identifier once",
        text: "INSERT INTO installation \
               (singleton, installation_identifier, recorded_at_unix_milliseconds) \
               VALUES (0, ?, ?)",
        parameters: 2,
        maximum_rows: 0,
    },
    InventoriedStatement {
        purpose: "read this installation's identifier",
        text: "SELECT installation_identifier FROM installation WHERE singleton = 0",
        parameters: 0,
        maximum_rows: SINGLE_ROW,
    },
    InventoriedStatement {
        purpose: "admit one operation",
        text: "INSERT INTO operation \
               (author_target_identity, author_target_identity_digest, caller_identity, \
                canonical_command, command_fingerprint, command_wire_name, \
                daemon_runtime_contract_digest, enqueue_sequence, installation_identifier, \
                lifecycle_state, operation_identifier, operation_revision, \
                recorded_at_unix_milliseconds, selected_environment_revision, \
                workflow_correlation_identifier) \
               VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
        parameters: 15,
        maximum_rows: 0,
    },
    InventoriedStatement {
        purpose: "read one operation inside its target partition",
        // A summary is not a payload: the canonical command, the opaque author
        // identity, and the contract digest stay in the row rather than
        // travelling with every lookup that only wants to know where the work
        // has got to.
        text: "SELECT caller_identity, command_fingerprint, command_wire_name, \
                      enqueue_sequence, installation_identifier, latest_progress, \
                      lifecycle_state, operation_revision, recorded_at_unix_milliseconds, \
                      result_disposition, selected_environment_revision, \
                      settled_at_unix_milliseconds, terminal_failure_disposition, \
                      terminal_failure_kind, terminal_failure_metadata, \
                      workflow_correlation_identifier \
               FROM operation \
               WHERE author_target_identity_digest = ? AND operation_identifier = ?",
        parameters: 2,
        maximum_rows: SINGLE_ROW,
    },
    InventoriedStatement {
        purpose: "reserve the next enqueue sequence inside one target partition",
        text: "SELECT COALESCE(MAX(enqueue_sequence), 0) + 1 FROM operation \
               WHERE author_target_identity_digest = ?",
        parameters: 1,
        maximum_rows: SINGLE_ROW,
    },
    InventoriedStatement {
        purpose: "list every partition holding work that has not ended",
        // Startup audits these before it binds anything. Terminal rows are
        // deliberately excluded: history from a target this daemon no longer
        // serves is something to keep and answer questions about, while
        // unfinished work under another identity is something no daemon may
        // quietly adopt.
        text: "SELECT DISTINCT author_target_identity_digest, selected_environment_revision \
               FROM operation \
               WHERE lifecycle_state NOT IN ('succeeded', 'failed') \
               ORDER BY author_target_identity_digest, selected_environment_revision",
        parameters: 0,
        maximum_rows: LISTING_ROWS,
    },
    InventoriedStatement {
        purpose: "reconstruct one target's operations in enqueue order",
        text: "SELECT operation_identifier FROM operation \
               WHERE author_target_identity_digest = ? \
               ORDER BY enqueue_sequence, operation_identifier",
        parameters: 1,
        maximum_rows: LISTING_ROWS,
    },
    InventoriedStatement {
        purpose: "list one target's operations, newest first",
        // Ordered by the stable arrival sequence rather than by a timestamp, so
        // a cursor names a position that cannot move. No command payload is
        // read: a listing says what happened to work, not what the work was.
        text: "SELECT enqueue_sequence, lifecycle_state, operation_identifier, \
                      operation_revision, caller_identity, workflow_correlation_identifier, \
                      terminal_failure_kind, settled_at_unix_milliseconds \
               FROM operation \
               WHERE author_target_identity_digest = ? AND enqueue_sequence < ? \
               ORDER BY enqueue_sequence DESC, operation_identifier \
               LIMIT ?",
        parameters: 3,
        maximum_rows: LISTING_ROWS,
    },
    InventoriedStatement {
        purpose: "record one folded operation under compare-and-set",
        text: "UPDATE operation \
               SET latest_progress = ?, lifecycle_state = ?, operation_revision = ?, \
                   result_disposition = ?, settled_at_unix_milliseconds = ?, \
                   terminal_failure_disposition = ?, terminal_failure_kind = ?, \
                   terminal_failure_metadata = ? \
               WHERE author_target_identity_digest = ? AND operation_identifier = ? \
                 AND operation_revision = ?",
        parameters: 11,
        maximum_rows: 0,
    },
    InventoriedStatement {
        purpose: "record the one recovery fact an operation is waiting on",
        text: "INSERT OR REPLACE INTO recovery_fact \
               (attempt_count, author_target_identity_digest, category, detail, \
                evidence_certainty, evidence_kind, manual_resume_eligible, \
                operation_identifier, retry_delay_milliseconds, \
                retry_observed_at_unix_milliseconds) \
               VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
        parameters: 10,
        maximum_rows: 0,
    },
    InventoriedStatement {
        purpose: "read the one recovery fact an operation is waiting on",
        text: "SELECT attempt_count, category, detail, evidence_certainty, evidence_kind, \
                      manual_resume_eligible, retry_delay_milliseconds, \
                      retry_observed_at_unix_milliseconds \
               FROM recovery_fact \
               WHERE author_target_identity_digest = ? AND operation_identifier = ?",
        parameters: 2,
        maximum_rows: SINGLE_ROW,
    },
    InventoriedStatement {
        purpose: "clear the recovery fact an operation is no longer waiting on",
        text: "DELETE FROM recovery_fact \
               WHERE author_target_identity_digest = ? AND operation_identifier = ?",
        parameters: 2,
        maximum_rows: 0,
    },
    InventoriedStatement {
        purpose: "record one recovery-resume receipt",
        text: "INSERT INTO recovery_resume_receipt \
               (applied_operation_revision, author_target_identity_digest, \
                operation_identifier, recorded_at_unix_milliseconds, \
                selected_environment_revision, source_fingerprint) \
               VALUES (?, ?, ?, ?, ?, ?)",
        parameters: 6,
        maximum_rows: 0,
    },
    InventoriedStatement {
        purpose: "count one operation's recovery-resume receipts",
        text: "SELECT COUNT(*) FROM recovery_resume_receipt \
               WHERE author_target_identity_digest = ? AND operation_identifier = ?",
        parameters: 2,
        maximum_rows: SINGLE_ROW,
    },
    InventoriedStatement {
        purpose: "count this namespace's retained operation rows",
        text: "SELECT COUNT(*) FROM operation",
        parameters: 0,
        maximum_rows: SINGLE_ROW,
    },
    InventoriedStatement {
        purpose: "count one target's maintenance-application receipts",
        text: "SELECT COUNT(*) FROM maintenance_application_receipt \
               WHERE author_target_identity_digest = ?",
        parameters: 1,
        maximum_rows: SINGLE_ROW,
    },
    InventoriedStatement {
        purpose: "count one target's maintenance-result associations",
        text: "SELECT COUNT(*) FROM maintenance_result_association \
               WHERE author_target_identity_digest = ?",
        parameters: 1,
        maximum_rows: SINGLE_ROW,
    },
    InventoriedStatement {
        purpose: "measure the bytes this namespace's committed content occupies",
        text: "SELECT COALESCE(SUM(byte_length), 0) FROM artifact_blob",
        parameters: 0,
        maximum_rows: SINGLE_ROW,
    },
    InventoriedStatement {
        purpose: "read one artifact blob's recorded length",
        text: "SELECT byte_length FROM artifact_blob WHERE content_digest = ?",
        parameters: 1,
        maximum_rows: SINGLE_ROW,
    },
    InventoriedStatement {
        purpose: "record one artifact's content, once per digest",
        text: "INSERT OR IGNORE INTO artifact_blob \
               (byte_length, content_digest, recorded_at_unix_milliseconds) \
               VALUES (?, ?, ?)",
        parameters: 3,
        maximum_rows: 0,
    },
    InventoriedStatement {
        purpose: "associate one artifact with the operation slot it fills",
        text: "INSERT INTO artifact_association \
               (artifact_identifier, artifact_slot, author_target_identity_digest, \
                byte_length, content_digest, media_type, operation_identifier) \
               VALUES (?, ?, ?, ?, ?, ?, ?)",
        parameters: 7,
        maximum_rows: 0,
    },
    InventoriedStatement {
        purpose: "read the artifact one operation slot holds",
        text: "SELECT artifact_identifier, byte_length, content_digest, media_type \
               FROM artifact_association \
               WHERE author_target_identity_digest = ? AND operation_identifier = ? \
                 AND artifact_slot = ?",
        parameters: 3,
        maximum_rows: SINGLE_ROW,
    },
    InventoriedStatement {
        purpose: "record one maintenance-application receipt",
        text: "INSERT INTO maintenance_application_receipt \
               (application_receipt_identifier, author_target_identity_digest, \
                recorded_at_unix_milliseconds, reviewed_manifest_digest) \
               VALUES (?, ?, ?, ?)",
        parameters: 4,
        maximum_rows: 0,
    },
    InventoriedStatement {
        purpose: "select one target's operations that ended before a cutoff",
        // Terminal only, and never a nonterminal row under any criteria. Work
        // that has not ended is work somebody may still be waiting on, and no
        // amount of age makes it safe to remove.
        text: "SELECT operation_identifier, operation_revision, settled_at_unix_milliseconds \
               FROM operation \
               WHERE author_target_identity_digest = ? \
                 AND lifecycle_state IN ('succeeded', 'failed') \
                 AND settled_at_unix_milliseconds IS NOT NULL \
                 AND settled_at_unix_milliseconds < ? \
               ORDER BY settled_at_unix_milliseconds, operation_identifier \
               LIMIT ?",
        parameters: 3,
        maximum_rows: LISTING_ROWS,
    },
    InventoriedStatement {
        purpose: "remove one terminal operation and everything hanging off it",
        // The children go with it through the schema's own cascades rather than
        // through a list this statement has to keep in step with the schema.
        text: "DELETE FROM operation \
               WHERE author_target_identity_digest = ? AND operation_identifier = ? \
                 AND lifecycle_state IN ('succeeded', 'failed')",
        parameters: 2,
        maximum_rows: 0,
    },
    InventoriedStatement {
        purpose: "count what still references one artifact's content",
        text: "SELECT (SELECT COUNT(*) FROM artifact_association WHERE content_digest = ?) \
                      + (SELECT COUNT(*) FROM maintenance_result_association \
                         WHERE content_digest = ?)",
        parameters: 2,
        maximum_rows: SINGLE_ROW,
    },
    InventoriedStatement {
        purpose: "remove one artifact's content, once nothing references it",
        text: "DELETE FROM artifact_blob WHERE content_digest = ?",
        parameters: 1,
        maximum_rows: 0,
    },
    InventoriedStatement {
        purpose: "read one target's maintenance-application receipt",
        text: "SELECT recorded_at_unix_milliseconds, reviewed_manifest_digest \
               FROM maintenance_application_receipt \
               WHERE author_target_identity_digest = ? AND application_receipt_identifier = ?",
        parameters: 2,
        maximum_rows: SINGLE_ROW,
    },
    InventoriedStatement {
        purpose: "read one maintenance result by target and identifier alone",
        text: "SELECT association_revision, byte_length, content_digest, kind, media_type, \
                      owning_application_receipt_identifier, reviewed_source_digest \
               FROM maintenance_result_association \
               WHERE author_target_identity_digest = ? \
                 AND maintenance_result_identifier = ?",
        parameters: 2,
        maximum_rows: SINGLE_ROW,
    },
    InventoriedStatement {
        purpose: "read one recovery-resume receipt by its source fingerprint",
        text: "SELECT applied_operation_revision, operation_identifier, \
                      recorded_at_unix_milliseconds, selected_environment_revision \
               FROM recovery_resume_receipt \
               WHERE author_target_identity_digest = ? AND source_fingerprint = ?",
        parameters: 2,
        maximum_rows: SINGLE_ROW,
    },
    InventoriedStatement {
        purpose: "admit one agent submission",
        text: "INSERT INTO agent_operation \
               (agent_event_store_generation, agent_operation_identifier, applied_sequence, \
                argument_schema_digest, attempt, author_agent_transport_contract_digest, \
                author_target_identity_digest, canonical_submission, \
                command_canonical_json_contract_digest, command_contract_limits_digest, \
                command_semantic_contract_version, command_wire_name, \
                daemon_subscription_identifier, job_state, operation_identifier, progress, \
                recorded_at_unix_milliseconds, remaining_retention_milliseconds, \
                request_start_unix_milliseconds, result_schema_digest, \
                selected_environment_revision, snapshot_watermark, submitted_command_digest) \
               VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
        parameters: 23,
        maximum_rows: 0,
    },
    InventoriedStatement {
        purpose: "read one agent submission inside its target partition",
        // Every derivation input comes back, because a restart re-derives the
        // identity from what this build has and compares it with what is here.
        // A summary that dropped the contract columns would make that
        // comparison impossible and the resumption dishonest.
        text: "SELECT agent_event_store_generation, applied_sequence, argument_schema_digest, \
                      attempt, author_agent_transport_contract_digest, canonical_submission, \
                      command_canonical_json_contract_digest, command_contract_limits_digest, \
                      command_semantic_contract_version, command_wire_name, \
                      daemon_subscription_identifier, job_state, operation_identifier, progress, \
                      recorded_at_unix_milliseconds, remaining_retention_milliseconds, \
                      request_start_unix_milliseconds, result_schema_digest, \
                      selected_environment_revision, snapshot_watermark, \
                      submitted_command_digest, terminal_disposition \
               FROM agent_operation \
               WHERE author_target_identity_digest = ? AND agent_operation_identifier = ?",
        parameters: 2,
        maximum_rows: SINGLE_ROW,
    },
    InventoriedStatement {
        purpose: "count the agent submissions one target holds",
        text: "SELECT COUNT(*) FROM agent_operation WHERE author_target_identity_digest = ?",
        parameters: 1,
        maximum_rows: SINGLE_ROW,
    },
    InventoriedStatement {
        purpose: "record one physical Sling job for one agent submission",
        // Several physical records for one logical submission is ordinary, so a
        // repeat is not an error. It records the same name again and changes
        // nothing, which is what at-least-once delivery looks like when it is
        // handled rather than merely survived.
        text: "INSERT OR IGNORE INTO agent_physical_job \
               (agent_operation_identifier, author_target_identity_digest, \
                recorded_at_unix_milliseconds, sling_job_identifier) \
               VALUES (?, ?, ?, ?)",
        parameters: 4,
        maximum_rows: 0,
    },
    InventoriedStatement {
        purpose: "read one agent submission's physical Sling jobs in order",
        text: "SELECT sling_job_identifier FROM agent_physical_job \
               WHERE author_target_identity_digest = ? AND agent_operation_identifier = ? \
               ORDER BY sling_job_identifier",
        parameters: 2,
        maximum_rows: PHYSICAL_JOB_ROWS,
    },
    InventoriedStatement {
        purpose: "fold one believed event into one agent submission",
        // The applied sequence is in the predicate as well as the assignment,
        // so two folds racing on one row cannot both succeed and neither can
        // apply an event to a row that has already moved past it.
        text: "UPDATE agent_operation \
               SET applied_sequence = ?, attempt = ?, job_state = ?, progress = ? \
               WHERE author_target_identity_digest = ? AND agent_operation_identifier = ? \
                 AND applied_sequence = ? AND terminal_disposition IS NULL",
        parameters: 7,
        maximum_rows: 0,
    },
    InventoriedStatement {
        purpose: "record one snapshot watermark on one agent submission",
        // Never backwards. A snapshot that covered less than one already
        // applied would make settled events look unsettled again.
        text: "UPDATE agent_operation SET snapshot_watermark = ? \
               WHERE author_target_identity_digest = ? AND agent_operation_identifier = ? \
                 AND snapshot_watermark <= ?",
        parameters: 4,
        maximum_rows: 0,
    },
    InventoriedStatement {
        purpose: "settle one agent submission",
        text: "UPDATE agent_operation \
               SET applied_sequence = ?, attempt = ?, job_state = ?, progress = ?, \
                   remaining_retention_milliseconds = ?, terminal_disposition = ? \
               WHERE author_target_identity_digest = ? AND agent_operation_identifier = ? \
                 AND terminal_disposition IS NULL",
        parameters: 8,
        maximum_rows: 0,
    },
    InventoriedStatement {
        purpose: "open one subscription ledger",
        text: "INSERT INTO subscription_ledger \
               (agent_event_store_generation, author_target_identity_digest, \
                daemon_subscription_identifier, event_bytes, event_rows, \
                recorded_at_unix_milliseconds, unresolved_incident_count) \
               VALUES (?, ?, ?, 0, 0, ?, 0)",
        parameters: 4,
        maximum_rows: 0,
    },
    InventoriedStatement {
        purpose: "read one subscription ledger",
        text: "SELECT agent_event_store_generation, canonical_digest, compacted_below_cursor, \
                      cursor, event_bytes, event_rows, high_water_cursor, \
                      recorded_at_unix_milliseconds, unresolved_incident, \
                      unresolved_incident_count \
               FROM subscription_ledger \
               WHERE author_target_identity_digest = ? AND daemon_subscription_identifier = ?",
        parameters: 2,
        maximum_rows: SINGLE_ROW,
    },
    InventoriedStatement {
        purpose: "count the subscription ledgers one target holds",
        text: "SELECT COUNT(*) FROM subscription_ledger WHERE author_target_identity_digest = ?",
        parameters: 1,
        maximum_rows: SINGLE_ROW,
    },
    InventoriedStatement {
        purpose: "advance one subscription ledger to a later position",
        // The predicate is the whole idempotency story. A position that is not
        // strictly later changes nothing, so a replayed event and a stale one
        // both leave the ledger where it was without the caller having to
        // decide which it was looking at.
        text: "UPDATE subscription_ledger \
               SET canonical_digest = ?, cursor = ?, event_bytes = event_bytes + ?, \
                   event_rows = event_rows + 1 \
               WHERE author_target_identity_digest = ? AND daemon_subscription_identifier = ? \
                 AND (cursor IS NULL OR cursor < ?)",
        parameters: 6,
        maximum_rows: 0,
    },
    InventoriedStatement {
        purpose: "record one unresolved integrity incident on a subscription",
        // One slot, and the counter moves only when the slot was empty.
        // Repeated conflicts about one subscription are one disagreement being
        // reported again, and charging capacity for each report would let a
        // misbehaving agent exhaust it by repeating itself.
        text: "UPDATE subscription_ledger \
               SET unresolved_incident = COALESCE(unresolved_incident, ?), \
                   unresolved_incident_count = \
                       unresolved_incident_count + (unresolved_incident IS NULL) \
               WHERE author_target_identity_digest = ? AND daemon_subscription_identifier = ?",
        parameters: 3,
        maximum_rows: 0,
    },
    InventoriedStatement {
        purpose: "install a captured high-water position on a subscription",
        text: "UPDATE subscription_ledger \
               SET agent_event_store_generation = ?, canonical_digest = ?, cursor = ?, \
                   high_water_cursor = ?, unresolved_incident = NULL, \
                   unresolved_incident_count = 0 \
               WHERE author_target_identity_digest = ? AND daemon_subscription_identifier = ?",
        parameters: 6,
        maximum_rows: 0,
    },
    InventoriedStatement {
        purpose: "record one subscription event",
        text: "INSERT INTO subscription_event \
               (agent_operation_identifier, author_target_identity_digest, canonical_digest, \
                cursor, daemon_subscription_identifier, disposition, event_bytes, job_sequence, \
                recorded_at_unix_milliseconds) \
               VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)",
        parameters: 9,
        maximum_rows: 0,
    },
    InventoriedStatement {
        purpose: "read one subscription event by its position",
        text: "SELECT agent_operation_identifier, canonical_digest, disposition, event_bytes, \
                      job_sequence, recorded_at_unix_milliseconds \
               FROM subscription_event \
               WHERE author_target_identity_digest = ? AND daemon_subscription_identifier = ? \
                 AND cursor = ?",
        parameters: 3,
        maximum_rows: SINGLE_ROW,
    },
    InventoriedStatement {
        purpose: "measure one subscription's retained events",
        text: "SELECT COUNT(*), COALESCE(SUM(event_bytes), 0) FROM subscription_event \
               WHERE author_target_identity_digest = ? AND daemon_subscription_identifier = ?",
        parameters: 2,
        maximum_rows: SINGLE_ROW,
    },
    InventoriedStatement {
        purpose: "compact one subscription's events below a position",
        text: "DELETE FROM subscription_event \
               WHERE author_target_identity_digest = ? AND daemon_subscription_identifier = ? \
                 AND cursor < ?",
        parameters: 3,
        maximum_rows: 0,
    },
    InventoriedStatement {
        purpose: "record one subscription's compaction floor",
        text: "UPDATE subscription_ledger \
               SET compacted_below_cursor = ?, event_bytes = ?, event_rows = ? \
               WHERE author_target_identity_digest = ? AND daemon_subscription_identifier = ?",
        parameters: 5,
        maximum_rows: 0,
    },
    InventoriedStatement {
        purpose: "select the agent submissions one maintenance run would remove",
        // Ended work only, and named in one fixed order so two previews of the
        // same target under the same window digest alike.
        text: "SELECT agent_operation_identifier, submitted_command_digest, \
                      terminal_disposition \
               FROM agent_operation \
               WHERE author_target_identity_digest = ? AND terminal_disposition IS NOT NULL \
                 AND recorded_at_unix_milliseconds < ? \
               ORDER BY agent_operation_identifier LIMIT ?",
        parameters: 3,
        maximum_rows: LISTING_ROWS,
    },
    InventoriedStatement {
        purpose: "remove one ended agent submission",
        text: "DELETE FROM agent_operation \
               WHERE author_target_identity_digest = ? AND agent_operation_identifier = ? \
                 AND terminal_disposition IS NOT NULL",
        parameters: 2,
        maximum_rows: 0,
    },
    InventoriedStatement {
        purpose: "select the subscriptions no retained agent submission needs",
        text: "SELECT daemon_subscription_identifier FROM subscription_ledger \
               WHERE author_target_identity_digest = ? \
                 AND daemon_subscription_identifier NOT IN ( \
                     SELECT daemon_subscription_identifier FROM agent_operation \
                     WHERE author_target_identity_digest = ?) \
               ORDER BY daemon_subscription_identifier LIMIT ?",
        parameters: 3,
        maximum_rows: LISTING_ROWS,
    },
    InventoriedStatement {
        purpose: "retire one subscription no retained agent submission needs",
        text: "DELETE FROM subscription_ledger \
               WHERE author_target_identity_digest = ? AND daemon_subscription_identifier = ? \
                 AND daemon_subscription_identifier NOT IN ( \
                     SELECT daemon_subscription_identifier FROM agent_operation \
                     WHERE author_target_identity_digest = ?)",
        parameters: 3,
        maximum_rows: 0,
    },
];

/// Returns the text of the statement with `purpose`.
///
/// The inventory is the single place a statement exists, so every runner looks
/// its text up here rather than holding a copy. A statement that is not in the
/// list is therefore not reachable at all.
///
/// # Panics
///
/// Panics when no statement carries `purpose`, which is a programming mistake
/// rather than a runtime condition: the purposes are constants in this file.
#[must_use]
pub fn statement_text(purpose: &str) -> &'static str {
    STATEMENTS
        .iter()
        .find(|inventoried| inventoried.purpose == purpose)
        .map(|inventoried| inventoried.text)
        .unwrap_or_else(|| panic!("the inventory holds a statement for {purpose}"))
}

/// Returns whether `text` is a statement this crate may run.
#[must_use]
pub fn is_inventoried(text: &str) -> bool {
    STATEMENTS.iter().any(|statement| statement.text == text)
}
