//! One page of a target's operation history.
//!
//! Ordered so a client can walk a target's whole history without missing or
//! repeating a row.
//!
//! Paging is by arrival sequence rather than by time, and that is the whole
//! design. A timestamp is not a position - two operations can share one, and a
//! clock can move - while the sequence a row was given when it was admitted
//! never changes. So a page boundary named by a sequence stays exactly where it
//! was however much work arrives afterwards, which is what lets a client
//! reading page after page be sure it saw each row once.
//!
//! No command payload is read. A listing answers what happened to work; a
//! client that wants the work itself asks about one operation.

use slingshot_domain::operation::{OperationListing, TerminalFailureKind};

use crate::database::OperationDatabase;
use crate::operation_repository::RepositoryFailure;
use crate::sqlite_statement_inventory::statement_text;

/// Purpose of the statement one listing page runs.
pub const LISTING_STATEMENT: &str = "list one target's operations, newest first";

/// Returns one page of a target's operations, newest first.
///
/// # Errors
///
/// Returns [`RepositoryFailure`] when a stored value does not decode or the
/// database refuses.
pub fn list(
    database: &OperationDatabase,
    author_target_identity_digest: &str,
    before_enqueue_sequence: u64,
    page_size: u64,
) -> Result<Vec<OperationListing>, RepositoryFailure> {
    let mut prepared = database.connection().prepare(statement_text(LISTING_STATEMENT))?;
    let rows = prepared.query_map(
        rusqlite::params![
            author_target_identity_digest,
            i64::try_from(before_enqueue_sequence).unwrap_or(i64::MAX),
            i64::try_from(page_size).unwrap_or(i64::MAX),
        ],
        |row| Ok(listing_from(row)),
    )?;
    rows.collect::<Result<Vec<_>, _>>()?.into_iter().collect()
}

/// Returns the listing row one result row holds.
fn listing_from(row: &rusqlite::Row<'_>) -> Result<OperationListing, RepositoryFailure> {
    let unsigned = |column: &str| -> Result<u64, rusqlite::Error> {
        Ok(u64::try_from(row.get::<_, i64>(column)?).unwrap_or_default())
    };
    let settled: Option<i64> = row.get("settled_at_unix_milliseconds")?;
    let kind: Option<String> = row.get("terminal_failure_kind")?;
    Ok(OperationListing {
        caller_identity: row.get("caller_identity")?,
        enqueue_sequence: unsigned("enqueue_sequence")?,
        lifecycle_state: decode_word("lifecycle_state", &row.get::<_, String>("lifecycle_state")?)?,
        operation_identifier: row.get("operation_identifier")?,
        revision: unsigned("operation_revision")?,
        settled_at_unix_milliseconds: settled.map(|at| u64::try_from(at).unwrap_or_default()),
        terminal_failure_kind: kind
            .map(|spelling| decode_word::<TerminalFailureKind>("terminal_failure_kind", &spelling))
            .transpose()?,
        workflow_correlation_identifier: row.get("workflow_correlation_identifier")?,
    })
}

/// Decodes one column's bare word back into the domain value it is.
fn decode_word<Value: serde::de::DeserializeOwned>(
    column: &'static str,
    word: &str,
) -> Result<Value, RepositoryFailure> {
    serde_json::from_str(&format!("\"{word}\""))
        .map_err(|failure| RepositoryFailure::NotDecodable { column, detail: failure.to_string() })
}
