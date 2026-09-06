//! Durable local scheduler leases and no-return checkpoints.

use rusqlite::OptionalExtension as _;
use crate::database::OperationDatabase;
use crate::operation_repository::RepositoryFailure;
use crate::sqlite_statement_inventory::STATEMENTS;

fn statement(purpose: &str) -> &'static str {
    STATEMENTS.iter().find(|item| item.purpose == purpose).map(|item| item.text)
        .unwrap_or_else(|| panic!("scheduler statement is inventoried: {purpose}"))
}

/// What one claim attempt observed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ClaimOutcome {
    /// This worker now holds the lease.
    Claimed,
    /// A newer unexpired worker fence owns the lease.
    Fenced,
    /// Execution crossed the no-return checkpoint.
    AlreadyStarted,
    /// Lifecycle or revision no longer matches the scheduler snapshot.
    RevisionMoved,
}

/// Durable state used to fence executor and settlement calls.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClaimFacts {
    /// Current worker fence, if one exists.
    pub scheduler_fence: Option<u64>,
    /// Current lease expiry, if one exists.
    pub lease_expires_at_unix_milliseconds: Option<u64>,
    /// No-return checkpoint, if execution crossed it.
    pub checkpoint: Option<String>,
}

/// The durable identity and compare-and-set facts selected for one tick.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SelectedClaim {
    /// The operation selected in the same transaction that claims it.
    pub operation_identifier: String,
    /// The revision the scheduler claimed.
    pub expected_revision: u64,
}

/// Selects and claims the oldest queued operation atomically.
///
/// This is the scheduler's process-safe handoff: two ticks may select at the
/// same time, but only one transaction can update the eligible row and receive
/// a `SelectedClaim`.
pub fn claim_next_queued(
    database: &OperationDatabase,
    target: &str,
    fence: u64,
    lease_expires_at_unix_milliseconds: u64,
    now_unix_milliseconds: u64,
) -> Result<Option<SelectedClaim>, RepositoryFailure> {
    let now = i64::try_from(now_unix_milliseconds).unwrap_or(i64::MAX);
    let transaction = rusqlite::Transaction::new_unchecked(
        database.connection(),
        rusqlite::TransactionBehavior::Immediate,
    )?;
    let candidate: Option<(String, String, i64)> = transaction
        .query_row(
            statement("select one queued operation for scheduler claim"),
            rusqlite::params![target, now],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )
        .optional()?;
    let Some((operation_identifier, lifecycle, revision)) = candidate else {
        transaction.commit()?;
        return Ok(None);
    };
    let outcome = claim_in_transaction(
        &transaction,
        target,
        &operation_identifier,
        &lifecycle,
        revision,
        fence,
        lease_expires_at_unix_milliseconds,
        now_unix_milliseconds,
    )?;
    if outcome != ClaimOutcome::Claimed {
        transaction.commit()?;
        return Ok(None);
    }
    transaction.commit()?;
    Ok(Some(SelectedClaim {
        operation_identifier,
        expected_revision: u64::try_from(revision).unwrap_or_default(),
    }))
}

/// Claims one operation only if lifecycle and revision are unchanged.
pub fn claim(
    database: &OperationDatabase, target: &str, operation: &str,
    expected_lifecycle: &str, expected_revision: u64, fence: u64,
    lease_expires_at_unix_milliseconds: u64, now_unix_milliseconds: u64,
) -> Result<ClaimOutcome, RepositoryFailure> {
    let expected_revision = i64::try_from(expected_revision).map_err(|_| RepositoryFailure::RevisionMoved { expected: expected_revision, stored: i64::MAX as u64 })?;
    let fence = i64::try_from(fence).map_err(|_| RepositoryFailure::RevisionMoved { expected: fence, stored: i64::MAX as u64 })?;
    let transaction = rusqlite::Transaction::new_unchecked(database.connection(), rusqlite::TransactionBehavior::Immediate)?;
    let current: Option<(String, i64, Option<String>)> = transaction.query_row(statement("read one scheduler claim candidate"), rusqlite::params![target, operation], |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?))).optional()?;
    let Some((lifecycle, revision, checkpoint)) = current else { return Err(RepositoryFailure::NoSuchOperation { identifier: operation.to_owned() }); };
    if revision != expected_revision || lifecycle != expected_lifecycle { return Ok(ClaimOutcome::RevisionMoved); }
    if checkpoint.is_some() { return Ok(ClaimOutcome::AlreadyStarted); }
    let outcome = claim_in_transaction(&transaction, target, operation, expected_lifecycle, expected_revision, fence as u64, lease_expires_at_unix_milliseconds, now_unix_milliseconds)?;
    if outcome != ClaimOutcome::Claimed { return Ok(outcome); }
    transaction.commit()?;
    Ok(ClaimOutcome::Claimed)
}

fn claim_in_transaction(
    transaction: &rusqlite::Transaction<'_>,
    target: &str,
    operation: &str,
    expected_lifecycle: &str,
    expected_revision: i64,
    fence: u64,
    lease_expires_at_unix_milliseconds: u64,
    now_unix_milliseconds: u64,
) -> Result<ClaimOutcome, RepositoryFailure> {
    let fence = i64::try_from(fence).unwrap_or(i64::MAX);
    let expiry = i64::try_from(lease_expires_at_unix_milliseconds).unwrap_or(i64::MAX);
    let now = i64::try_from(now_unix_milliseconds).unwrap_or(i64::MAX);
    let changed = transaction.execute(
        statement("claim one retained operation for execution"),
        rusqlite::params![fence, expiry, target, operation, expected_lifecycle, expected_revision, now, fence],
    )?;
    Ok(if changed == 1 { ClaimOutcome::Claimed } else { ClaimOutcome::Fenced })
}

/// Records the no-return point only for the current fence.
pub fn checkpoint(database: &OperationDatabase, target: &str, operation: &str, fence: u64, marker: &str) -> Result<bool, RepositoryFailure> {
    let changed = database.connection().execute(statement("checkpoint one retained operation execution"), rusqlite::params![marker, target, operation, i64::try_from(fence).unwrap_or(i64::MAX)])?;
    Ok(changed == 1)
}

/// Renews a lease without allowing a stale worker to revive itself.
pub fn renew(database: &OperationDatabase, target: &str, operation: &str, fence: u64, expiry: u64, now: u64) -> Result<bool, RepositoryFailure> {
    let changed = database.connection().execute(statement("renew one retained operation execution lease"), rusqlite::params![i64::try_from(expiry).unwrap_or(i64::MAX), target, operation, i64::try_from(fence).unwrap_or(i64::MAX), i64::try_from(now).unwrap_or(i64::MAX)])?;
    Ok(changed == 1)
}

/// Reads lease/checkpoint facts without granting execution authority.
pub fn facts(database: &OperationDatabase, target: &str, operation: &str) -> Result<Option<ClaimFacts>, RepositoryFailure> {
    let row = database.connection().query_row(statement("read one retained operation scheduler claim"), rusqlite::params![target, operation], |row| Ok((row.get::<_, Option<i64>>(0)?, row.get::<_, Option<i64>>(1)?, row.get::<_, Option<String>>(2)?))).optional()?;
    Ok(row.map(|(fence, expiry, checkpoint)| ClaimFacts { scheduler_fence: fence.map(|value| u64::try_from(value).unwrap_or_default()), lease_expires_at_unix_milliseconds: expiry.map(|value| u64::try_from(value).unwrap_or_default()), checkpoint }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::database::RequiredSettings;

    #[test]
    fn claim_is_revision_bound_and_checkpoint_survives_lease_expiry() {
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("claims.sqlite");
        let settings = RequiredSettings { page_bytes: 4096, database_pages: 262_144, busy_timeout_milliseconds: 5000 };
        let database = OperationDatabase::open(&path, settings).unwrap();
        let contender = OperationDatabase::open(&path, settings).unwrap();
        let value = "a".repeat(64);
        database.connection().execute(
            statement("admit one operation"),
            rusqlite::params!["identity", value, Option::<String>::None, "{}", value, "query_paths", value, 1, value, "queued", "operation", 1, 1, value, Option::<String>::None],
        ).unwrap();
        assert_eq!(claim(&database, &value, "operation", "queued", 1, 1, 10, 1).unwrap(), ClaimOutcome::Claimed);
        assert_eq!(claim(&contender, &value, "operation", "queued", 1, 2, 10, 2).unwrap(), ClaimOutcome::Fenced);
        assert!(renew(&database, &value, "operation", 2, 20, 2).unwrap() == false);
        assert!(checkpoint(&database, &value, "operation", 1, "sent").unwrap());
        assert_eq!(claim(&database, &value, "operation", "queued", 1, 2, 30, 11).unwrap(), ClaimOutcome::AlreadyStarted);
        let facts = facts(&database, &value, "operation").unwrap().unwrap();
        assert_eq!(facts.scheduler_fence, Some(1));
        assert_eq!(facts.checkpoint.as_deref(), Some("sent"));
        assert_eq!(claim(&database, &value, "operation", "running", 1, 3, 30, 11).unwrap(), ClaimOutcome::RevisionMoved);

        let second = "b".repeat(64);
        database.connection().execute(
            statement("admit one operation"),
            rusqlite::params!["identity", second, Option::<String>::None, "{}", second, "query_paths", second, 2, second, "queued", "operation-2", 1, 1, second, Option::<String>::None],
        ).unwrap();
        assert_eq!(claim_next_queued(&contender, &second, 4, 20, 2).unwrap(), Some(SelectedClaim { operation_identifier: "operation-2".to_owned(), expected_revision: 1 }));
    }
}
