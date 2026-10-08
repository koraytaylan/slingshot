//! Durable local scheduler leases and no-return checkpoints.

use crate::database::OperationDatabase;
use crate::operation_repository::RepositoryFailure;
use crate::sqlite_statement_inventory::STATEMENTS;
use rusqlite::OptionalExtension as _;

fn statement(purpose: &str) -> &'static str {
    STATEMENTS
        .iter()
        .find(|item| item.purpose == purpose)
        .map(|item| item.text)
        .unwrap_or_else(|| panic!("scheduler statement is inventoried: {purpose}"))
}

/// Reads a nonnegative durable count without inventing a value on corruption.
fn count(row: &rusqlite::Row<'_>, column: &str) -> rusqlite::Result<u64> {
    let index = row.as_ref().column_index(column)?;
    let value: i64 = row.get(index)?;
    u64::try_from(value).map_err(|_| rusqlite::Error::IntegralValueOutOfRange(index, value))
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

/// One queued operation considered inside the atomic claim transaction.
#[derive(Debug, Clone)]
pub struct QueuedCandidate {
    /// Persisted producer identity; absence denotes the shared default queue.
    pub caller_identity: Option<String>,
    /// Durable operation identity.
    pub operation_identifier: String,
    /// Compare-and-set lifecycle value.
    pub lifecycle: String,
    /// Compare-and-set revision value.
    pub revision: u64,
    /// Persisted retry observation, used only to reconstruct a local deadline.
    pub retry_observed_at_unix_milliseconds: u64,
    /// Original maximum delay for the observed retry.
    pub retry_delay_milliseconds: u64,
    /// Distinguishes successive retries with identical clock readings.
    pub attempt_count: u64,
}

/// One queued operation a person was left to resume.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PausedQueueRow {
    /// The operation.
    pub operation_identifier: String,
    /// The revision a terminal fact must name.
    pub revision: u64,
    /// The fence still holding the claim, when one remains.
    pub fence: Option<u64>,
    /// The stored evidence kind.
    pub evidence_kind: String,
}

/// Returns queued operations whose recovery is waiting on a person.
///
/// # Errors
/// Returns [`RepositoryFailure`] when the inventory statement cannot be run.
pub fn paused_queued(
    database: &OperationDatabase,
    target: &str,
) -> Result<Vec<PausedQueueRow>, RepositoryFailure> {
    let mut statement = database
        .connection()
        .prepare(statement("select queued operations paused for manual recovery"))
        .map_err(|_| RepositoryFailure::NoSuchOperation { identifier: target.to_owned() })?;
    let rows = statement
        .query_map(rusqlite::params![target], |row| {
            Ok(PausedQueueRow {
                operation_identifier: row.get(0)?,
                revision: u64::try_from(row.get::<_, i64>(1)?).unwrap_or(0),
                fence: row
                    .get::<_, Option<i64>>("scheduler_fence")?
                    .and_then(|fence| u64::try_from(fence).ok()),
                evidence_kind: row.get("evidence_kind")?,
            })
        })
        .map_err(|_| RepositoryFailure::NoSuchOperation { identifier: target.to_owned() })?;
    rows.collect::<Result<Vec<_>, _>>()
        .map_err(|_| RepositoryFailure::NoSuchOperation { identifier: target.to_owned() })
}

/// Selects and claims a queued operation at a fixed fixture clock reading.
///
/// This is the scheduler's process-safe handoff: two ticks may select at the
/// same time, but only one transaction can update the eligible row and receive
/// a `SelectedClaim`.
///
/// # Errors
///
/// Returns a repository failure if candidate selection or persistence fails.
#[cfg(test)]
fn claim_next_queued(
    database: &OperationDatabase,
    target: &str,
    fence: u64,
    lease_expires_at_unix_milliseconds: u64,
    now_unix_milliseconds: u64,
) -> Result<Option<SelectedClaim>, RepositoryFailure> {
    claim_next_queued_with(
        database,
        target,
        fence,
        lease_expires_at_unix_milliseconds,
        now_unix_milliseconds,
        |candidates| {
            candidates.iter().position(|candidate| {
                now_unix_milliseconds.saturating_sub(candidate.retry_observed_at_unix_milliseconds)
                    >= candidate.retry_delay_milliseconds
            })
        },
    )
}

/// Selects ready work from durable round-robin producer order.
///
/// Selection and claim run in one immediate transaction. The callback receives
/// unclaimed queued candidates in producer-turn order and enqueue order within
/// each producer. The callback returns the first ready candidate using its
/// scheduling clock. Only a successful claim rotates the producer, atomically.
/// Local retry durations belong to the daemon's monotonic clock, while the
/// persisted lease timestamps continue to use cross-process wall-clock evidence.
///
/// # Errors
///
/// Returns [`RepositoryFailure`] if reading or claiming a candidate fails.
pub fn claim_next_queued_with(
    database: &OperationDatabase,
    target: &str,
    fence: u64,
    lease_expires_at_unix_milliseconds: u64,
    now_unix_milliseconds: u64,
    choose: impl FnOnce(&[QueuedCandidate]) -> Option<usize>,
) -> Result<Option<SelectedClaim>, RepositoryFailure> {
    let now = i64::try_from(now_unix_milliseconds).unwrap_or(i64::MAX);
    let transaction = rusqlite::Transaction::new_unchecked(
        database.connection(),
        rusqlite::TransactionBehavior::Immediate,
    )?;
    let mut candidates = {
        let mut query =
            transaction.prepare(statement("select queued candidates for scheduler claim"))?;
        query
            .query_map(rusqlite::params![target, now], |row| {
                Ok(QueuedCandidate {
                    caller_identity: row.get("caller_identity")?,
                    operation_identifier: row.get(0)?,
                    lifecycle: row.get(1)?,
                    revision: count(row, "operation_revision")?,
                    retry_observed_at_unix_milliseconds: count(
                        row,
                        "retry_observed_at_unix_milliseconds",
                    )?,
                    retry_delay_milliseconds: count(row, "retry_delay_milliseconds")?,
                    attempt_count: count(row, "attempt_count")?,
                })
            })?
            .collect::<Result<Vec<_>, _>>()?
    };
    let mut turns =
        super::producer_turns::ProducerTurns::order(&transaction, target, &mut candidates)?;
    let Some(candidate) = choose(&candidates).and_then(|index| candidates.get(index)) else {
        transaction.commit()?;
        return Ok(None);
    };
    let outcome = claim_in_transaction(
        &transaction,
        target,
        &candidate.operation_identifier,
        &candidate.lifecycle,
        i64::try_from(candidate.revision).unwrap_or(i64::MAX),
        fence,
        lease_expires_at_unix_milliseconds,
        now_unix_milliseconds,
    )?;
    if outcome != ClaimOutcome::Claimed {
        transaction.commit()?;
        return Ok(None);
    }
    turns.claimed(&transaction, target, candidate, &candidates)?;
    transaction.commit()?;
    Ok(Some(SelectedClaim {
        operation_identifier: candidate.operation_identifier.clone(),
        expected_revision: candidate.revision,
    }))
}

/// Claims one operation only if lifecycle and revision are unchanged.
///
/// # Errors
///
/// Returns a repository failure if the row is missing, its counts are invalid, or persistence fails.
pub fn claim(
    database: &OperationDatabase,
    target: &str,
    operation: &str,
    expected_lifecycle: &str,
    expected_revision: u64,
    fence: u64,
    lease_expires_at_unix_milliseconds: u64,
    now_unix_milliseconds: u64,
) -> Result<ClaimOutcome, RepositoryFailure> {
    let expected_revision = i64::try_from(expected_revision).map_err(|_| {
        RepositoryFailure::RevisionMoved { expected: expected_revision, stored: i64::MAX as u64 }
    })?;
    let fence = i64::try_from(fence).map_err(|_| RepositoryFailure::RevisionMoved {
        expected: fence,
        stored: i64::MAX as u64,
    })?;
    let transaction = rusqlite::Transaction::new_unchecked(
        database.connection(),
        rusqlite::TransactionBehavior::Immediate,
    )?;
    let current: Option<(String, i64, Option<String>)> = transaction
        .query_row(
            statement("read one scheduler claim candidate"),
            rusqlite::params![target, operation],
            |row| Ok((row.get(0)?, row.get(1)?, row.get("scheduler_checkpoint")?)),
        )
        .optional()?;
    let Some((lifecycle, revision, checkpoint)) = current else {
        return Err(RepositoryFailure::NoSuchOperation { identifier: operation.to_owned() });
    };
    if revision != expected_revision || lifecycle != expected_lifecycle {
        return Ok(ClaimOutcome::RevisionMoved);
    }
    if checkpoint.is_some() {
        return Ok(ClaimOutcome::AlreadyStarted);
    }
    let outcome = claim_in_transaction(
        &transaction,
        target,
        operation,
        expected_lifecycle,
        expected_revision,
        fence as u64,
        lease_expires_at_unix_milliseconds,
        now_unix_milliseconds,
    )?;
    if outcome != ClaimOutcome::Claimed {
        return Ok(outcome);
    }
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
        rusqlite::params![
            fence,
            expiry,
            target,
            operation,
            expected_lifecycle,
            expected_revision,
            now,
            fence
        ],
    )?;
    Ok(if changed == 1 { ClaimOutcome::Claimed } else { ClaimOutcome::Fenced })
}

/// Records the no-return point only for the current fence.
///
/// # Errors
///
/// Returns a repository failure if the checkpoint write fails.
pub fn checkpoint(
    database: &OperationDatabase,
    target: &str,
    operation: &str,
    fence: u64,
    marker: &str,
) -> Result<bool, RepositoryFailure> {
    let changed = database.connection().execute(
        statement("checkpoint one retained operation execution"),
        rusqlite::params![marker, target, operation, i64::try_from(fence).unwrap_or(i64::MAX)],
    )?;
    Ok(changed == 1)
}

/// Renews a lease without allowing a stale worker to revive itself.
///
/// # Errors
///
/// Returns a repository failure if the lease write fails.
pub fn renew(
    database: &OperationDatabase,
    target: &str,
    operation: &str,
    fence: u64,
    expiry: u64,
    now: u64,
) -> Result<bool, RepositoryFailure> {
    let changed = database.connection().execute(
        statement("renew one retained operation execution lease"),
        rusqlite::params![
            i64::try_from(expiry).unwrap_or(i64::MAX),
            target,
            operation,
            i64::try_from(fence).unwrap_or(i64::MAX),
            i64::try_from(now).unwrap_or(i64::MAX)
        ],
    )?;
    Ok(changed == 1)
}

/// Releases a fence and lease this side proved but never checkpointed,
/// abandoning a claim immediately rather than leaving it to lapse only once
/// the lease it never used against a real attempt expires.
///
/// # Errors
///
/// Returns a repository failure if the claim release fails.
pub fn release(
    database: &OperationDatabase,
    target: &str,
    operation: &str,
    fence: u64,
) -> Result<bool, RepositoryFailure> {
    let changed = database.connection().execute(
        statement("release a scheduler claim this attempt proved but left the operation unchanged"),
        rusqlite::params![target, operation, i64::try_from(fence).unwrap_or(i64::MAX)],
    )?;
    Ok(changed == 1)
}

/// Clears every scheduler claim a previous instance left on nonterminal work.
///
/// A claim survives the claim query until its checkpoint is released, and a
/// process that died holding one never gets to release it. The startup sweep
/// is what turns those rows back into claimable work, because no later tick
/// will ever select a checkpointed operation. A recovery fact paused for a
/// person is left alone: its checkpoint is the hold that keeps the scheduler
/// from retrying it ahead of that person, and manual resume releases it.
///
/// # Errors
///
/// Returns [`RepositoryFailure`] when the database refuses the write.
pub fn recover_abandoned_claims(
    database: &OperationDatabase,
    target: &str,
) -> Result<usize, RepositoryFailure> {
    Ok(database.connection().execute(
        statement("clear every scheduler claim a dead instance left behind"),
        rusqlite::params![target],
    )?)
}

/// Reads lease/checkpoint facts without granting execution authority.
///
/// # Errors
///
/// Returns a repository failure if the stored claim cannot be read.
pub fn facts(
    database: &OperationDatabase,
    target: &str,
    operation: &str,
) -> Result<Option<ClaimFacts>, RepositoryFailure> {
    let row = database
        .connection()
        .query_row(
            statement("read one retained operation scheduler claim"),
            rusqlite::params![target, operation],
            |row| {
                Ok((
                    row.get::<_, Option<i64>>(0)?,
                    row.get::<_, Option<i64>>(1)?,
                    row.get::<_, Option<String>>("scheduler_checkpoint")?,
                ))
            },
        )
        .optional()?;
    Ok(row.map(|(fence, expiry, checkpoint)| ClaimFacts {
        scheduler_fence: fence.map(|value| u64::try_from(value).unwrap_or_default()),
        lease_expires_at_unix_milliseconds: expiry
            .map(|value| u64::try_from(value).unwrap_or_default()),
        checkpoint,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::database::RequiredSettings;

    const FIXTURE_PAGE_BYTES: u64 = 4096;
    const FIXTURE_DATABASE_PAGES: u64 = 262_144;
    const FIXTURE_BUSY_MILLISECONDS: u64 = 5000;
    const FIXTURE_DIGEST_CHARACTERS: usize = 64;

    #[test]
    fn release_abandons_a_claim_that_never_reached_its_checkpoint() {
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("release.sqlite");
        let settings = RequiredSettings {
            page_bytes: FIXTURE_PAGE_BYTES,
            database_pages: FIXTURE_DATABASE_PAGES,
            busy_timeout_milliseconds: FIXTURE_BUSY_MILLISECONDS,
        };
        let database = OperationDatabase::open(&path, settings).unwrap();
        let value = "a".repeat(FIXTURE_DIGEST_CHARACTERS);
        database
            .connection()
            .execute(
                statement("admit one operation"),
                rusqlite::params![
                    "identity",
                    value,
                    Option::<String>::None,
                    "{}",
                    value,
                    "query_paths",
                    value,
                    1,
                    value,
                    "queued",
                    "operation",
                    1,
                    1,
                    value,
                    Option::<String>::None
                ],
            )
            .unwrap();
        assert_eq!(
            claim(&database, &value, "operation", "queued", 1, 1, 10, 1).unwrap(),
            ClaimOutcome::Claimed
        );
        assert!(
            release(&database, &value, "operation", 1).unwrap(),
            "a fence this call proved was not released"
        );
        let facts = facts(&database, &value, "operation").unwrap().unwrap();
        assert!(facts.scheduler_fence.is_none(), "the released fence is still held");
        assert!(
            facts.lease_expires_at_unix_milliseconds.is_none(),
            "the released lease is still held"
        );
        assert!(facts.checkpoint.is_none(), "a claim that never checkpointed now carries one");
        assert_eq!(
            claim_next_queued(&database, &value, 2, 10, 2).unwrap(),
            Some(SelectedClaim {
                operation_identifier: "operation".to_owned(),
                expected_revision: 1
            }),
            "the released operation is not immediately reclaimable"
        );
        assert!(
            !release(&database, &value, "operation", 1).unwrap(),
            "a fence that has since moved on was released as if it were still held"
        );
    }

    #[test]
    fn claim_is_revision_bound_and_checkpoint_survives_lease_expiry() {
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("claims.sqlite");
        let settings = RequiredSettings {
            page_bytes: FIXTURE_PAGE_BYTES,
            database_pages: FIXTURE_DATABASE_PAGES,
            busy_timeout_milliseconds: FIXTURE_BUSY_MILLISECONDS,
        };
        let database = OperationDatabase::open(&path, settings).unwrap();
        let contender = OperationDatabase::open(&path, settings).unwrap();
        let value = "a".repeat(FIXTURE_DIGEST_CHARACTERS);
        database
            .connection()
            .execute(
                statement("admit one operation"),
                rusqlite::params![
                    "identity",
                    value,
                    Option::<String>::None,
                    "{}",
                    value,
                    "query_paths",
                    value,
                    1,
                    value,
                    "queued",
                    "operation",
                    1,
                    1,
                    value,
                    Option::<String>::None
                ],
            )
            .unwrap();
        assert_eq!(
            claim(&database, &value, "operation", "queued", 1, 1, 10, 1).unwrap(),
            ClaimOutcome::Claimed
        );
        assert_eq!(
            claim(&contender, &value, "operation", "queued", 1, 2, 10, 2).unwrap(),
            ClaimOutcome::Fenced
        );
        assert!(!renew(&database, &value, "operation", 2, 20, 2).unwrap());
        assert!(checkpoint(&database, &value, "operation", 1, "sent").unwrap());
        assert_eq!(
            claim(&database, &value, "operation", "queued", 1, 2, 30, 11).unwrap(),
            ClaimOutcome::AlreadyStarted
        );
        let facts = facts(&database, &value, "operation").unwrap().unwrap();
        assert_eq!(facts.scheduler_fence, Some(1));
        assert_eq!(facts.checkpoint.as_deref(), Some("sent"));
        assert_eq!(
            claim(&database, &value, "operation", "running", 1, 3, 30, 11).unwrap(),
            ClaimOutcome::RevisionMoved
        );

        let second = "b".repeat(FIXTURE_DIGEST_CHARACTERS);
        database
            .connection()
            .execute(
                statement("admit one operation"),
                rusqlite::params![
                    "identity",
                    second,
                    Option::<String>::None,
                    "{}",
                    second,
                    "query_paths",
                    second,
                    2,
                    second,
                    "queued",
                    "operation-2",
                    1,
                    1,
                    second,
                    Option::<String>::None
                ],
            )
            .unwrap();
        assert_eq!(
            claim_next_queued(&contender, &second, 4, 20, 2).unwrap(),
            Some(SelectedClaim {
                operation_identifier: "operation-2".to_owned(),
                expected_revision: 1
            })
        );
    }
}
