//! Removing work that has ended, and never anything else.
//!
//! Maintenance is the only path by which durable state shrinks. Nothing here
//! runs on a timer, on pressure, or on age alone: a person previews what would
//! be removed, reads it, and applies exactly that. Automatic pruning is absent
//! by construction, because the record of what a system did is not something to
//! reclaim space from without being asked.
//!
//! Two rules bound everything below. Only terminal operations are ever
//! selected, whatever the criteria - work that has not ended is work somebody
//! may still be waiting on, and no amount of age makes it safe to remove. And
//! content is deleted only when nothing references it any more, checked at the
//! moment of deletion rather than assumed from the manifest, because a
//! reference may have appeared since the preview was taken.
//!
//! Apply is phased so an interruption is recoverable rather than ambiguous. The
//! rows go first, in one transaction, and the receipt goes with them; only then
//! is unreferenced content deleted. An interruption after the transaction
//! leaves a receipt saying the database part is done and content that is merely
//! unreferenced - which a retry finishes, and which nothing else mistakes for a
//! result. The reverse order would leave rows pointing at bytes that are gone.

use sha2::{Digest as _, Sha256};
use slingshot_domain::daemon_runtime_contract::DaemonRuntimeContract;

use crate::database::OperationDatabase;
use crate::operation_repository::RepositoryFailure;
use crate::sqlite_statement_inventory::statement_text;

/// Version marker every maintenance manifest is digested under.
pub const MANIFEST_VERSION: &str = "slingshot.terminal-maintenance-manifest/1";

/// Separator between the fields a manifest digest is taken over.
pub const FIELD_SEPARATOR: u8 = 0;

/// Columns the selection statement returns, in its own order.
mod column {
    /// The identifier its caller chose.
    pub const IDENTIFIER: usize = 0;
    /// The revision it settled at.
    pub const REVISION: usize = 1;
    /// When it settled.
    pub const SETTLED_AT: usize = 2;
}

/// One operation a manifest proposes to remove.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct ProposedRemoval {
    /// The identifier its caller chose.
    pub operation_identifier: String,
    /// The revision it was at when the preview was taken.
    pub operation_revision: u64,
    /// When it settled.
    pub settled_at_unix_milliseconds: u64,
}

/// One remote submission a manifest proposes to remove.
///
/// Named beside the operation that submitted it rather than folded into it,
/// because the two are removed for the same reason and reviewed as one list.
/// A reviewer who saw only the local half would be approving the removal of
/// remote correlation they were never shown.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct ProposedAgentRemoval {
    /// What the submission is called at the agent.
    pub agent_operation_identifier: String,
    /// Which submission it was.
    pub submitted_command_digest: String,
    /// How it ended.
    pub terminal_disposition: String,
}

/// What one maintenance run would remove, exactly.
///
/// Whole operations only. An operation's children go with it, so a manifest
/// never proposes to remove half of one - a partly removed operation would be a
/// row whose history had holes in it, which is worse than either keeping it or
/// removing it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TerminalMaintenanceManifest {
    /// The remote submissions it proposes to remove, in a fixed order.
    pub agent_removals: Vec<ProposedAgentRemoval>,
    /// The partition this run is about.
    pub author_target_identity_digest: String,
    /// Operations settled before this instant were eligible.
    pub before_unix_milliseconds: u64,
    /// How many operations the preview was allowed to take.
    ///
    /// Part of the manifest, and part of its digest, because two previews of
    /// one target under different limits are different requests - and because
    /// the freshness check has to re-derive under the same window the preview
    /// used. Re-deriving under a window sized to what the manifest happens to
    /// contain would find the same rows however much arrived since.
    pub limit: u64,
    /// What it proposes to remove, in a fixed order.
    pub removals: Vec<ProposedRemoval>,
    /// The shared subscriptions no retained submission would still need.
    ///
    /// Reviewed rather than reclaimed. A subscription outlives the submissions
    /// that opened it and may still be carrying another one's events, so it is
    /// retired only when the same review that removes its last submission says
    /// so, and only if nothing retained still names it at the moment of
    /// removal.
    pub retired_subscriptions: Vec<String>,
}

impl TerminalMaintenanceManifest {
    /// Returns the digest of this manifest's canonical form.
    ///
    /// Every field is length-prefixed and separated, so two manifests digest
    /// alike exactly when they propose the same removals for the same target
    /// under the same cutoff. An apply quotes this digest, which is how the
    /// daemon knows the person applying reviewed what is about to happen rather
    /// than something that has since changed.
    #[must_use]
    pub fn digest(&self) -> String {
        let mut hasher = Sha256::new();
        hasher.update(MANIFEST_VERSION.as_bytes());
        hasher.update([FIELD_SEPARATOR]);
        hasher.update(self.author_target_identity_digest.as_bytes());
        hasher.update([FIELD_SEPARATOR]);
        hasher.update(self.before_unix_milliseconds.to_be_bytes());
        hasher.update([FIELD_SEPARATOR]);
        hasher.update(self.limit.to_be_bytes());
        for removal in &self.removals {
            hasher.update(removal.operation_identifier.as_bytes());
            hasher.update([FIELD_SEPARATOR]);
            hasher.update(removal.operation_revision.to_be_bytes());
            hasher.update(removal.settled_at_unix_milliseconds.to_be_bytes());
        }
        for removal in &self.agent_removals {
            hasher.update(removal.agent_operation_identifier.as_bytes());
            hasher.update([FIELD_SEPARATOR]);
            hasher.update(removal.submitted_command_digest.as_bytes());
            hasher.update([FIELD_SEPARATOR]);
            hasher.update(removal.terminal_disposition.as_bytes());
            hasher.update([FIELD_SEPARATOR]);
        }
        for subscription in &self.retired_subscriptions {
            hasher.update(subscription.as_bytes());
            hasher.update([FIELD_SEPARATOR]);
        }
        hasher.finalize().iter().map(|octet| format!("{octet:02x}")).collect()
    }

    /// Returns how many operation rows applying this would release.
    #[must_use]
    pub fn released_operation_rows(&self) -> u64 {
        u64::try_from(self.removals.len()).unwrap_or(u64::MAX)
    }

    /// Returns how many remote submissions applying this would release.
    #[must_use]
    pub fn released_agent_rows(&self) -> u64 {
        u64::try_from(self.agent_removals.len()).unwrap_or(u64::MAX)
    }
}

/// How far one apply has got.
///
/// Two states rather than one, and the reason is recovery. A receipt that only
/// said "applied" would leave a retry unable to tell a finished run from one
/// interrupted between the rows and the bytes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReceiptStage {
    /// The rows are gone and the receipt is committed; content may remain.
    DatabaseApplied,
    /// Everything the manifest listed is done.
    Completed,
}

/// What one applied maintenance run did.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ApplicationReceipt {
    /// The receipt's own identifier, which is the manifest digest.
    pub application_receipt_identifier: String,
    /// The partition it was about.
    pub author_target_identity_digest: String,
    /// When it was recorded.
    pub recorded_at_unix_milliseconds: u64,
    /// Operation rows it removed.
    pub released_operation_rows: u64,
    /// How far it has got.
    pub stage: ReceiptStage,
}

/// Selects what a maintenance run would remove, changing nothing.
///
/// Mutation-free on purpose: a preview a person is going to read must not be
/// the thing that changes what it describes.
///
/// # Errors
///
/// Returns [`RepositoryFailure`] when the database refuses.
pub fn preview(
    database: &OperationDatabase,
    author_target_identity_digest: &str,
    before_unix_milliseconds: u64,
    limit: u64,
) -> Result<TerminalMaintenanceManifest, RepositoryFailure> {
    let mut prepared = database
        .connection()
        .prepare(statement_text("select one target's operations that ended before a cutoff"))?;
    let rows = prepared.query_map(
        rusqlite::params![
            author_target_identity_digest,
            i64::try_from(before_unix_milliseconds).unwrap_or(i64::MAX),
            i64::try_from(limit).unwrap_or(i64::MAX),
        ],
        |row| {
            Ok(ProposedRemoval {
                operation_identifier: row.get(column::IDENTIFIER)?,
                operation_revision: u64::try_from(row.get::<_, i64>(column::REVISION)?)
                    .unwrap_or_default(),
                settled_at_unix_milliseconds: u64::try_from(row.get::<_, i64>(column::SETTLED_AT)?)
                    .unwrap_or_default(),
            })
        },
    )?;
    let removals = rows.collect::<Result<Vec<ProposedRemoval>, _>>()?;
    Ok(TerminalMaintenanceManifest {
        agent_removals: proposed_agent_removals(
            database,
            author_target_identity_digest,
            before_unix_milliseconds,
            limit,
        )?,
        author_target_identity_digest: author_target_identity_digest.to_owned(),
        before_unix_milliseconds,
        limit,
        removals,
        retired_subscriptions: retirable_subscriptions(
            database,
            author_target_identity_digest,
            limit,
        )?,
    })
}

/// Returns the remote submissions the same window would remove.
fn proposed_agent_removals(
    database: &OperationDatabase,
    author_target_identity_digest: &str,
    before_unix_milliseconds: u64,
    limit: u64,
) -> Result<Vec<ProposedAgentRemoval>, RepositoryFailure> {
    let connection = database.connection();
    let mut prepared = connection
        .prepare(statement_text("select the agent submissions one maintenance run would remove"))?;
    let rows = prepared.query_map(
        rusqlite::params![
            author_target_identity_digest,
            i64::try_from(before_unix_milliseconds).unwrap_or(i64::MAX),
            i64::try_from(limit).unwrap_or(i64::MAX),
        ],
        |row| {
            Ok(ProposedAgentRemoval {
                agent_operation_identifier: row.get("agent_operation_identifier")?,
                submitted_command_digest: row.get("submitted_command_digest")?,
                terminal_disposition: row.get("terminal_disposition")?,
            })
        },
    )?;
    Ok(rows.collect::<Result<Vec<ProposedAgentRemoval>, _>>()?)
}

/// Returns the subscriptions no retained submission still needs.
fn retirable_subscriptions(
    database: &OperationDatabase,
    author_target_identity_digest: &str,
    limit: u64,
) -> Result<Vec<String>, RepositoryFailure> {
    let connection = database.connection();
    let mut prepared = connection
        .prepare(statement_text("select the subscriptions no retained agent submission needs"))?;
    let rows = prepared.query_map(
        rusqlite::params![
            author_target_identity_digest,
            author_target_identity_digest,
            i64::try_from(limit).unwrap_or(i64::MAX),
        ],
        |row| row.get(0),
    )?;
    Ok(rows.collect::<Result<Vec<String>, _>>()?)
}

/// Returns how many operations one maintenance run may remove.
#[must_use]
pub fn maximum_removals() -> u64 {
    DaemonRuntimeContract::embedded().limit("maximum_terminal_maintenance_operations")
}

/// Reason a maintenance run could not be applied.
#[derive(Debug, thiserror::Error)]
pub enum MaintenanceFailure {
    /// The manifest being applied is not the one that was reviewed.
    #[error(
        "the reviewed manifest digest is {reviewed}, and this target's rows now digest to {current}"
    )]
    ManifestChanged {
        /// What the rows digest to now.
        current: String,
        /// What the person reviewed.
        reviewed: String,
    },
    /// The manifest proposes more than one run may remove.
    #[error("one run removes at most {allowed} operations, and this proposes {wanted}")]
    TooManyRemovals {
        /// How many one run may remove.
        allowed: u64,
        /// How many this proposes.
        wanted: u64,
    },
    /// The database refused.
    #[error(transparent)]
    Repository(#[from] RepositoryFailure),
}

/// What applying one manifest did.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ApplyOutcome {
    /// The rows were removed and the receipt committed.
    Applied(Box<ApplicationReceipt>),
    /// This exact manifest was already applied, and this is its receipt.
    Replayed(Box<ApplicationReceipt>),
}

/// Applies exactly the manifest a person reviewed.
///
/// The digest is re-derived from the rows as they are now and compared against
/// what was reviewed. A manifest that no longer describes the target is refused
/// rather than partly applied, because "remove these thirty operations" is not
/// a request that still means anything once the thirty have changed.
///
/// The removal and the receipt commit together. An interruption therefore
/// leaves either nothing done or a receipt saying the rows are gone, and never
/// rows gone with no record of why.
///
/// # Errors
///
/// Returns [`MaintenanceFailure::ManifestChanged`] when the target has moved on,
/// [`MaintenanceFailure::TooManyRemovals`] past the contract's bound, or a
/// repository failure.
pub fn apply(
    database: &OperationDatabase,
    reviewed: &TerminalMaintenanceManifest,
    now_unix_milliseconds: u64,
) -> Result<ApplyOutcome, MaintenanceFailure> {
    let digest = reviewed.digest();
    if let Some(held) = receipt(database, &reviewed.author_target_identity_digest, &digest)? {
        return Ok(ApplyOutcome::Replayed(Box::new(held)));
    }
    let allowed = maximum_removals();
    if reviewed.released_operation_rows() > allowed {
        return Err(MaintenanceFailure::TooManyRemovals {
            allowed,
            wanted: reviewed.released_operation_rows(),
        });
    }
    let current = preview(
        database,
        &reviewed.author_target_identity_digest,
        reviewed.before_unix_milliseconds,
        reviewed.limit,
    )?;
    if current.digest() != digest {
        return Err(MaintenanceFailure::ManifestChanged {
            current: current.digest(),
            reviewed: digest,
        });
    }
    remove_and_record(database, reviewed, &digest, now_unix_milliseconds)?;
    Ok(ApplyOutcome::Applied(Box::new(ApplicationReceipt {
        application_receipt_identifier: digest,
        author_target_identity_digest: reviewed.author_target_identity_digest.clone(),
        recorded_at_unix_milliseconds: now_unix_milliseconds,
        released_operation_rows: reviewed.released_operation_rows(),
        stage: ReceiptStage::DatabaseApplied,
    })))
}

/// Removes every listed operation and commits the receipt, together.
fn remove_and_record(
    database: &OperationDatabase,
    reviewed: &TerminalMaintenanceManifest,
    digest: &str,
    now_unix_milliseconds: u64,
) -> Result<(), RepositoryFailure> {
    let connection = database.connection();
    let transaction =
        rusqlite::Transaction::new_unchecked(connection, rusqlite::TransactionBehavior::Immediate)?;
    for removal in &reviewed.removals {
        transaction.execute(
            statement_text("remove one terminal operation and everything hanging off it"),
            rusqlite::params![reviewed.author_target_identity_digest, removal.operation_identifier],
        )?;
    }
    for removal in &reviewed.agent_removals {
        transaction.execute(
            statement_text("remove one ended agent submission"),
            rusqlite::params![
                reviewed.author_target_identity_digest,
                removal.agent_operation_identifier
            ],
        )?;
    }
    for subscription in &reviewed.retired_subscriptions {
        transaction.execute(
            statement_text("retire one subscription no retained agent submission needs"),
            rusqlite::params![
                reviewed.author_target_identity_digest,
                subscription,
                reviewed.author_target_identity_digest
            ],
        )?;
    }
    transaction.execute(
        statement_text("record one maintenance-application receipt"),
        rusqlite::params![
            digest,
            reviewed.author_target_identity_digest,
            i64::try_from(now_unix_milliseconds).unwrap_or(i64::MAX),
            digest,
        ],
    )?;
    transaction.commit()?;
    Ok(())
}

/// Returns one target's receipt for `digest`, when it has one.
///
/// # Errors
///
/// Returns [`RepositoryFailure`] when the database refuses.
pub fn receipt(
    database: &OperationDatabase,
    author_target_identity_digest: &str,
    digest: &str,
) -> Result<Option<ApplicationReceipt>, RepositoryFailure> {
    let mut prepared = database
        .connection()
        .prepare(statement_text("read one target's maintenance-application receipt"))?;
    let found = prepared
        .query_row(rusqlite::params![author_target_identity_digest, digest], |row| {
            Ok(ApplicationReceipt {
                application_receipt_identifier: digest.to_owned(),
                author_target_identity_digest: author_target_identity_digest.to_owned(),
                recorded_at_unix_milliseconds: u64::try_from(row.get::<_, i64>(0)?)
                    .unwrap_or_default(),
                released_operation_rows: 0,
                stage: ReceiptStage::DatabaseApplied,
            })
        })
        .map(Some)
        .or_else(|failure| match failure {
            rusqlite::Error::QueryReturnedNoRows => Ok(None),
            other => Err(RepositoryFailure::Statement(other)),
        })?;
    Ok(found)
}

/// Removes one artifact's content, but only if nothing references it.
///
/// The check happens here rather than in the manifest, because a reference may
/// have appeared since the preview was taken. Content that is still referenced
/// is left exactly where it is, and the caller is told it was kept.
///
/// # Errors
///
/// Returns [`RepositoryFailure`] when the database refuses.
pub fn release_if_unreferenced(
    database: &OperationDatabase,
    content_digest: &str,
) -> Result<bool, RepositoryFailure> {
    let connection = database.connection();
    let transaction =
        rusqlite::Transaction::new_unchecked(connection, rusqlite::TransactionBehavior::Immediate)?;
    let references: i64 = transaction.query_row(
        statement_text("count what still references one artifact's content"),
        rusqlite::params![content_digest, content_digest],
        |row| row.get(0),
    )?;
    if references > 0 {
        transaction.commit()?;
        return Ok(false);
    }
    transaction.execute(
        statement_text("remove one artifact's content, once nothing references it"),
        rusqlite::params![content_digest],
    )?;
    transaction.commit()?;
    Ok(true)
}
