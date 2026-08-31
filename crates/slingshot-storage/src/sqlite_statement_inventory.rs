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
        purpose: "reconstruct one target's operations in enqueue order",
        text: "SELECT operation_identifier FROM operation \
               WHERE author_target_identity_digest = ? \
               ORDER BY enqueue_sequence, operation_identifier",
        parameters: 1,
        maximum_rows: LISTING_ROWS,
    },
    InventoriedStatement {
        purpose: "list one target's operations, newest first",
        text: "SELECT operation_identifier, lifecycle_state, enqueue_sequence \
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
];

/// Returns whether `text` is a statement this crate may run.
#[must_use]
pub fn is_inventoried(text: &str) -> bool {
    STATEMENTS.iter().any(|statement| statement.text == text)
}
