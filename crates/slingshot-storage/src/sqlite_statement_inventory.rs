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
        text: "SELECT command_fingerprint, lifecycle_state, operation_revision, \
                      selected_environment_revision \
               FROM operation \
               WHERE author_target_identity_digest = ? AND operation_identifier = ?",
        parameters: 2,
        maximum_rows: SINGLE_ROW,
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
        purpose: "record one lifecycle advance under compare-and-set",
        text: "UPDATE operation \
               SET lifecycle_state = ?, operation_revision = ? \
               WHERE author_target_identity_digest = ? AND operation_identifier = ? \
                 AND operation_revision = ?",
        parameters: 5,
        maximum_rows: 0,
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
                      selected_environment_revision \
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
