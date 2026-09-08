//! Counting what a namespace holds, and refusing before it holds too much.
//!
//! Every count here is read from the rows that are authoritative for it rather
//! than from a counter kept alongside them. A counter can drift; a `COUNT` over
//! the table cannot, and it makes restart reconstruction free - there is
//! nothing to reconstruct, because there was never a second copy of the truth.
//!
//! Reservations are the exception, and deliberately so. An artifact being
//! installed has no blob row yet, so its bytes are held in a reservation row
//! against the same bound until the association commits. That is the right lifetime: a
//! reservation belongs to an installation in progress, and an installation
//! interrupted by a restart is not in progress any more. What it leaves behind
//! is the staged file the artifact store deliberately does not delete, which
//! nothing addresses and which no count includes.
//!
//! Accounts on already-open connections share the reservation rows. An
//! immediate transaction holds the capacity check and insert together, so two
//! connections cannot each consume the same remaining capacity. Startup still
//! owns abandonment reconciliation; opening a new startup database while live
//! reservations exist must not be confused with acquiring another account.
//!
//! Refusal always happens before mutation. Nothing here deletes a terminal row,
//! a receipt, or committed content to make room: reaching a bound is a fact to
//! report, with what is held and what would release some, not a licence to
//! destroy the record of work that happened.

use slingshot_domain::persistent_capacity::{
    CapacityFacts, CapacityRefusal, PersistentCapacityPolicy,
};

use crate::database::OperationDatabase;

/// Returns the text of the inventoried statement with `purpose`.
fn statement(purpose: &str) -> &'static str {
    crate::sqlite_statement_inventory::STATEMENTS
        .iter()
        .find(|inventoried| inventoried.purpose == purpose)
        .map(|inventoried| inventoried.text)
        .unwrap_or_else(|| panic!("the inventory holds a statement for {purpose}"))
}

/// Reason a capacity question could not be answered or acted on.
#[derive(Debug, thiserror::Error)]
pub enum AccountingFailure {
    /// A purported duplicate disagrees with the already committed content.
    #[error("the artifact length differs from the committed content")]
    ContentLengthConflict,
    /// The namespace is at or past a bound.
    #[error(transparent)]
    Refused(#[from] CapacityRefusal),
    /// The database refused.
    #[error("the database refused: {0}")]
    DatabaseRefused(String),
    /// A stored count is not one.
    #[error("a stored count is {0}, which is not a count")]
    NotACount(i64),
}

/// Returns a database refusal as this module's failure.
fn refused(failure: rusqlite::Error) -> AccountingFailure {
    AccountingFailure::DatabaseRefused(failure.to_string())
}

#[cfg(test)]
mod publication_reconstruction_tests {
    use super::*;

    #[test]
    fn reconstruction_crosses_page_boundary_and_refuses_corrupt_metadata() {
        let database = OperationDatabase::open_in_memory(crate::database::RequiredSettings {
            page_bytes: 4096,
            database_pages: 262144,
            busy_timeout_milliseconds: 5000,
        })
        .unwrap();
        let digest = "a".repeat(64);
        database
            .connection()
            .execute(
                statement("record one artifact's content, once per digest"),
                rusqlite::params![1, digest, 1],
            )
            .unwrap();
        for _ in 0..257 {
            database
                .connection()
                .execute(
                    statement("retain one artifact publication across restart"),
                    rusqlite::params![uuid::Uuid::new_v4().to_string(), digest, digest, 1],
                )
                .unwrap();
        }
        let account =
            PersistentCapacityAccount::new(&database, PersistentCapacityPolicy::embedded());
        let records = account.reconstruct_publications().unwrap();
        assert_eq!(records.len(), 257);
        assert!(
            records
                .windows(2)
                .all(|pair| pair[0].publication_identifier < pair[1].publication_identifier)
        );
        database
            .connection()
            .execute(
                statement("retain one artifact publication across restart"),
                rusqlite::params!["invalid-producer", digest, digest, 1],
            )
            .unwrap();
        assert!(account.reconstruct_publications().is_err());
        assert_eq!(account.pending_publications().unwrap(), 258);
        assert_eq!(account.usage().unwrap().committed_artifact_bytes, 1);
    }
}

/// One artifact's bytes, held against the bound until its association commits.
/// Dropping the guard releases only this reservation on its owning connection.
///
/// A consumed reservation cannot release a reused ticket a second time:
/// ```compile_fail
/// use slingshot_storage::persistent_capacity::ArtifactReservation;
/// fn release_twice(reservation: ArtifactReservation<'_>) {
///     drop(reservation);
///     drop(reservation);
/// }
/// ```
#[must_use = "keep the reservation alive until publication commits or the transfer is abandoned"]
pub struct ArtifactReservation<'database> {
    /// How many bytes are held.
    pub byte_length: u64,
    /// Which reservation this is.
    ticket: Option<u64>,
    /// The connection owning this reservation; never a caller-selected account.
    database: &'database OperationDatabase,
}

impl core::fmt::Debug for ArtifactReservation<'_> {
    fn fmt(&self, formatter: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        formatter.write_str("ArtifactReservation([redacted])")
    }
}

impl Drop for ArtifactReservation<'_> {
    fn drop(&mut self) {
        let Some(ticket) = self.ticket.and_then(|ticket| i64::try_from(ticket).ok()) else {
            return;
        };
        // A failed release conservatively retains capacity until startup
        // reconciliation; it never releases another connection's ticket.
        self.database
            .connection()
            .execute(
                statement("release one durable artifact reservation"),
                rusqlite::params![ticket],
            )
            .ok();
    }
}

/// A durable, producer-specific hold protecting content during publication.
/// Dropping this value deliberately does not release the persisted hold.
/// Completion/reconciliation must consume the exact record, never all records
/// sharing its digest. It is not proof of a published file or local success.
#[derive(Clone)]
pub struct ArtifactPublication {
    identifier: String,
}

/// Durable protection for one operation-free maintenance document.
/// Dropping it does not release the persisted hold.
#[derive(Debug, Clone)]
pub struct MaintenancePublication {
    publication: ArtifactPublication,
}

impl MaintenancePublication {
    /// Identifies the exact retained producer; never a content-wide release key.
    pub fn identifier(&self) -> &str {
        self.publication.identifier()
    }
}

struct PublicationMetadata<'a> {
    artifact_identifier: &'a str,
    content_digest: &'a str,
    byte_length: u64,
    maintenance: Option<&'a crate::artifact_store::maintenance_content::MaintenanceDocument>,
}

mod maintenance_recovery;
pub use maintenance_recovery::{MaintenancePublicationRecovery, PendingMaintenancePublication};

#[cfg(test)]
mod maintenance_publication_tests {
    use super::*;
    use slingshot_domain::daemon_runtime_contract::{
        DIGEST_OCTETS, DaemonRuntimeContract, MaintenanceResultKind,
    };

    #[test]
    fn maintenance_hold_transfers_reserved_bytes_and_preserves_owner_across_reopen() {
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("publication.sqlite");
        let limits = DaemonRuntimeContract::embedded();
        let open = || {
            OperationDatabase::open(
                &path,
                crate::database::RequiredSettings {
                    page_bytes: limits.limit("sqlite_page_bytes"),
                    database_pages: limits.limit("maximum_sqlite_database_pages"),
                    busy_timeout_milliseconds: limits.limit("database_busy_timeout_milliseconds"),
                },
            )
            .unwrap()
        };
        let database = open();
        let account =
            PersistentCapacityAccount::new(&database, PersistentCapacityPolicy::embedded());
        let store = crate::artifact_store::ArtifactStore::open(root.path()).unwrap();
        let target = "a".repeat(DIGEST_OCTETS * 2);
        let source = "b".repeat(DIGEST_OCTETS * 2);
        let reservation = account.reserve_artifact(None, 2).unwrap();
        let stage = store
            .stage_maintenance_document(&target, MaintenanceResultKind::Preview, &source, b"{}")
            .unwrap();
        assert!(account.retain_maintenance_publication(&stage, None, 1).is_err());
        assert_eq!(account.pending_publications().unwrap(), 0);
        let hold = account.retain_maintenance_publication(&stage, reservation, 1).unwrap();
        assert_eq!(account.pending_publications().unwrap(), 1);
        assert_eq!(account.usage().unwrap().reserved_artifact_bytes, 0);
        assert!(account.retain_maintenance_publication(&stage, None, 2).is_err());
        assert_eq!(account.pending_publications().unwrap(), 1);
        let document = stage.publish().unwrap();
        let identifier = hold.identifier().to_owned();
        drop(hold);
        drop(account);
        drop(database);
        let database = open();
        let account =
            PersistentCapacityAccount::new(&database, PersistentCapacityPolicy::embedded());
        assert_eq!(account.pending_publications().unwrap(), 1);
        let (operations, recovered) = account.reconstruct_target_publications(&target).unwrap();
        assert!(operations.is_empty());
        let recovered = recovered.unwrap();
        assert_eq!(recovered.document(), &document);
        assert_eq!(recovered.hold().identifier(), identifier);
        assert_eq!(recovered.recorded_at_unix_milliseconds(), 1);
        drop(recovered);
        assert_eq!(account.pending_publications().unwrap(), 1);
        let foreign = "c".repeat(DIGEST_OCTETS * 2);
        let (unresolved, maintenance) = account.reconstruct_target_publications(&foreign).unwrap();
        assert!(maintenance.is_none());
        assert_eq!(unresolved.len(), 1);
        assert_eq!(unresolved[0].publication_identifier, identifier);
        assert!(account.reconstruct_target_publications(&target.to_uppercase()).is_err());
        assert_eq!(account.pending_publications().unwrap(), 1);
        let row: (String, String, String, i64, i64, String, String) = database
            .connection()
            .query_row(
                statement("read one target's pending maintenance publication"),
                [&target],
                |row| {
                    Ok((
                        row.get(0)?,
                        row.get(1)?,
                        row.get(2)?,
                        row.get(3)?,
                        row.get(4)?,
                        row.get(5)?,
                        row.get(6)?,
                    ))
                },
            )
            .unwrap();
        assert_eq!(
            row,
            (
                identifier,
                document.identifier.as_text().to_owned(),
                document.content_digest.clone(),
                2,
                1,
                "preview".to_owned(),
                source.clone()
            )
        );
        let recovered = account.reconstruct_maintenance_publication(&target).unwrap().unwrap();
        assert!(
            account.complete_maintenance_publication(&store, &target, recovered.hold()).is_err()
        );
        assert_eq!(account.pending_publications().unwrap(), 1);
        crate::maintenance_results::record_current_preview(
            &database,
            &account,
            &target,
            &source,
            &document.content_digest,
            document.byte_length,
        )
        .unwrap();
        let absent_root = tempfile::tempdir().unwrap();
        let absent_store = crate::artifact_store::ArtifactStore::open(absent_root.path()).unwrap();
        assert!(
            account
                .complete_maintenance_publication(&absent_store, &target, recovered.hold())
                .is_err()
        );
        assert!(
            account.complete_maintenance_publication(&store, &foreign, recovered.hold()).is_err()
        );
        assert_eq!(account.pending_publications().unwrap(), 1);
        account.complete_maintenance_publication(&store, &target, recovered.hold()).unwrap();
        assert_eq!(account.pending_publications().unwrap(), 0);
        assert!(account.reconstruct_maintenance_publication(&target).unwrap().is_none());
        assert_eq!(account.usage().unwrap().committed_artifact_bytes, 2);
        let stage = store
            .stage_maintenance_document(&target, MaintenanceResultKind::Preview, &source, b"{}")
            .unwrap();
        let next = account.retain_maintenance_publication(&stage, None, 2).unwrap();
        stage.publish().unwrap();
        assert!(
            account.complete_maintenance_publication(&store, &target, recovered.hold()).is_err()
        );
        assert_eq!(account.pending_publications().unwrap(), 1);
        let unrelated = uuid::Uuid::new_v4().to_string();
        database
            .connection()
            .execute(
                statement("retain one artifact publication across restart"),
                rusqlite::params![unrelated, foreign, document.content_digest, 2],
            )
            .unwrap();
        account.complete_maintenance_publication(&store, &target, &next).unwrap();
        assert_eq!(account.pending_publications().unwrap(), 1);
        assert_eq!(
            account.reconstruct_publications().unwrap()[0].publication_identifier,
            unrelated
        );
        assert_eq!(account.usage().unwrap().committed_artifact_bytes, 2);
        assert!(
            crate::maintenance_results::read(&database, &target, &document.identifier)
                .unwrap()
                .is_some()
        );
    }
}

impl ArtifactPublication {
    /// Opaque identifier for the exact persisted publication attempt.
    pub fn identifier(&self) -> &str {
        &self.identifier
    }
}

impl core::fmt::Debug for ArtifactPublication {
    fn fmt(&self, formatter: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        formatter.write_str("ArtifactPublication([redacted])")
    }
}

/// Retained publication evidence reconstructed at startup. This is not proof
/// of file presence and grants no authority to settle an operation or release bytes.
#[derive(Clone, PartialEq, Eq)]
pub struct PendingArtifactPublication {
    /// Exact producer identity; shared content must not merge producer records.
    pub publication_identifier: String,
    /// Stable operation/slot-derived artifact identity.
    pub artifact_identifier: crate::artifact_store::ArtifactIdentifier,
    /// Content protected by this producer.
    pub content_digest: String,
    /// Charged length of that content.
    pub byte_length: u64,
    /// Original publication timestamp, never renewed on restart.
    pub recorded_at_unix_milliseconds: u64,
}

impl core::fmt::Debug for PendingArtifactPublication {
    fn fmt(&self, formatter: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        formatter.write_str("PendingArtifactPublication([redacted])")
    }
}

/// What one namespace is currently holding.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NamespaceUsage {
    /// Bytes committed content occupies.
    pub committed_artifact_bytes: u64,
    /// Operation rows held.
    pub operation_rows: u64,
    /// Bytes reservations are holding.
    pub reserved_artifact_bytes: u64,
}

/// The accounting for one runtime namespace.
#[derive(Debug)]
pub struct PersistentCapacityAccount<'database> {
    /// The database the authoritative rows live in.
    database: &'database OperationDatabase,
    /// The bounds this namespace is held to.
    policy: PersistentCapacityPolicy,
}

impl<'database> PersistentCapacityAccount<'database> {
    /// Returns the accounting for the namespace `database` holds.
    #[must_use]
    pub fn new(database: &'database OperationDatabase, policy: PersistentCapacityPolicy) -> Self {
        Self { database, policy }
    }

    /// Returns the bounds this namespace is held to.
    #[must_use]
    pub fn policy(&self) -> PersistentCapacityPolicy {
        self.policy
    }

    /// Requires accounting and operation state to share the same live database.
    #[must_use]
    pub fn belongs_to(&self, database: &OperationDatabase) -> bool {
        self.database.shares_database_with(database)
    }

    /// Returns one count from the rows that are authoritative for it.
    fn count(
        &self,
        purpose: &str,
        parameters: &[&dyn rusqlite::ToSql],
    ) -> Result<u64, AccountingFailure> {
        let counted: i64 = self
            .database
            .connection()
            .query_row(statement(purpose), parameters, |row| row.get(0))
            .map_err(refused)?;
        u64::try_from(counted).map_err(|_| AccountingFailure::NotACount(counted))
    }

    /// Returns what this namespace is currently holding.
    ///
    /// # Errors
    ///
    /// Returns [`AccountingFailure::DatabaseRefused`].
    pub fn usage(&self) -> Result<NamespaceUsage, AccountingFailure> {
        Ok(NamespaceUsage {
            committed_artifact_bytes: self
                .count("measure the bytes this namespace's committed content occupies", &[])?,
            operation_rows: self.count("count this namespace's retained operation rows", &[])?,
            reserved_artifact_bytes: self.reserved_bytes()?,
        })
    }

    /// Returns how many bytes reservations are holding.
    fn reserved_bytes(&self) -> Result<u64, AccountingFailure> {
        self.count("measure this namespace's durable artifact reservations", &[])
    }
}

impl<'database> PersistentCapacityAccount<'database> {
    /// Requires room for one more operation row.
    ///
    /// Asked before admission writes anything, so a namespace at its bound
    /// refuses with a row count that is still true rather than after creating
    /// the row that made it false.
    ///
    /// # Errors
    ///
    /// Returns [`CapacityRefusal::OperationRows`] wrapped in
    /// [`AccountingFailure::Refused`].
    pub fn require_room_for_operation(&self) -> Result<CapacityFacts, AccountingFailure> {
        let facts = CapacityFacts {
            held: self.count("count this namespace's retained operation rows", &[])?,
            limit: self.policy.retained_operation_rows,
            wanted: 1,
        };
        if facts.fits() { Ok(facts) } else { Err(CapacityRefusal::OperationRows { facts }.into()) }
    }

    /// Requires room for one more resume receipt on `operation_identifier`.
    ///
    /// # Errors
    ///
    /// Returns [`CapacityRefusal::ResumeReceipts`].
    pub fn require_room_for_resume_receipt(
        &self,
        author_target_identity_digest: &str,
        operation_identifier: &str,
    ) -> Result<CapacityFacts, AccountingFailure> {
        let facts = CapacityFacts {
            held: self.count(
                "count one operation's recovery-resume receipts",
                &[&author_target_identity_digest, &operation_identifier],
            )?,
            limit: self.policy.recovery_resume_receipts_per_operation,
            wanted: 1,
        };
        if facts.fits() { Ok(facts) } else { Err(CapacityRefusal::ResumeReceipts { facts }.into()) }
    }

    /// Requires room for one more maintenance-application receipt on a target.
    ///
    /// # Errors
    ///
    /// Returns [`CapacityRefusal::MaintenanceReceipts`].
    pub fn require_room_for_maintenance_receipt(
        &self,
        author_target_identity_digest: &str,
    ) -> Result<CapacityFacts, AccountingFailure> {
        let facts = CapacityFacts {
            held: self.count(
                "count one target's maintenance-application receipts",
                &[&author_target_identity_digest],
            )?,
            limit: self.policy.maintenance_application_receipts_per_target,
            wanted: 1,
        };
        if facts.fits() {
            Ok(facts)
        } else {
            Err(CapacityRefusal::MaintenanceReceipts { facts }.into())
        }
    }

    /// Requires room for `wanted` more maintenance-result associations.
    ///
    /// # Errors
    ///
    /// Returns [`CapacityRefusal::MaintenanceAssociations`].
    pub fn require_room_for_maintenance_associations(
        &self,
        author_target_identity_digest: &str,
        wanted: u64,
    ) -> Result<CapacityFacts, AccountingFailure> {
        let facts = CapacityFacts {
            held: self.count(
                "count one target's maintenance-result associations",
                &[&author_target_identity_digest],
            )?,
            limit: self.policy.maintenance_result_associations_per_target,
            wanted,
        };
        if facts.fits() {
            Ok(facts)
        } else {
            Err(CapacityRefusal::MaintenanceAssociations { facts }.into())
        }
    }

    /// Holds `byte_length` bytes against the artifact bound.
    ///
    /// Content already committed under `content_digest` reserves nothing: the
    /// bytes are already being counted, and installing them again would produce
    /// the same one file. So a duplicate consumes no second allocation, which
    /// is the same rule the store follows by addressing content with its
    /// digest.
    ///
    /// # Errors
    ///
    /// Returns [`CapacityRefusal::ArtifactTooLarge`] for one artifact past the
    /// individual bound, or [`CapacityRefusal::ArtifactBytes`] when committed
    /// plus reserved would cross the aggregate.
    pub fn reserve_artifact(
        &self,
        content_digest: Option<&str>,
        byte_length: u64,
    ) -> Result<Option<ArtifactReservation<'database>>, AccountingFailure> {
        self.policy.require_artifact_representable(byte_length)?;
        let transaction = rusqlite::Transaction::new_unchecked(
            self.database.connection(),
            rusqlite::TransactionBehavior::Immediate,
        )
        .map_err(refused)?;
        if let Some(digest) = content_digest
            && let Some(committed_length) = self.committed_length(digest)?
        {
            if committed_length != byte_length {
                return Err(AccountingFailure::ContentLengthConflict);
            }
            return Ok(None);
        }
        let committed =
            self.count("measure the bytes this namespace's committed content occupies", &[])?;
        let reserved = self.count("measure this namespace's durable artifact reservations", &[])?;
        let facts = CapacityFacts {
            held: committed.saturating_add(reserved),
            limit: self.policy.committed_plus_reserved_artifact_bytes,
            wanted: byte_length,
        };
        if !facts.fits() {
            return Err(CapacityRefusal::ArtifactBytes { facts }.into());
        }
        let reserved_length = i64::try_from(byte_length).map_err(|_| {
            AccountingFailure::DatabaseRefused(
                "the reservation length exceeds SQLite's integer range".to_owned(),
            )
        })?;
        self.database
            .connection()
            .execute(
                statement("hold one artifact reservation durably"),
                rusqlite::params![reserved_length],
            )
            .map_err(refused)?;
        let ticket =
            u64::try_from(self.database.connection().last_insert_rowid()).map_err(|_| {
                AccountingFailure::DatabaseRefused("the reservation ticket is negative".to_owned())
            })?;
        transaction.commit().map_err(refused)?;
        Ok(Some(ArtifactReservation { byte_length, ticket: Some(ticket), database: self.database }))
    }

    /// Converts reserved bytes into a durable publication hold before a verified
    /// private stage is published. Blob accounting and reservation release are
    /// atomic; startup cleanup cannot release these bytes. Existing content
    /// still receives a separate producer hold without a second byte charge.
    pub fn retain_staged_publication(
        &self,
        stage: &crate::artifact_store::StagedArtifact<'_>,
        reservation: Option<ArtifactReservation<'_>>,
        now_unix_milliseconds: u64,
    ) -> Result<ArtifactPublication, AccountingFailure> {
        let metadata = stage.metadata();
        self.retain_publication(
            PublicationMetadata {
                artifact_identifier: metadata.artifact_identifier.as_text(),
                content_digest: &metadata.content_digest,
                byte_length: metadata.byte_length,
                maintenance: None,
            },
            reservation,
            now_unix_milliseconds,
        )
    }

    /// Transfers a reservation to a durable operation-free publication hold.
    /// The owner record and shared blob protection commit together, before the
    /// caller may physically publish or acknowledge a result association.
    ///
    /// # Errors
    /// Refuses capacity, an existing pending maintenance producer for the target,
    /// mismatched/foreign reservations, or conflicting recorded content length.
    pub fn retain_maintenance_publication(
        &self,
        stage: &crate::artifact_store::maintenance_content::StagedMaintenanceDocument<'_>,
        reservation: Option<ArtifactReservation<'_>>,
        now_unix_milliseconds: u64,
    ) -> Result<MaintenancePublication, AccountingFailure> {
        let document = stage.document();
        let publication = self.retain_publication(
            PublicationMetadata {
                artifact_identifier: document.identifier.as_text(),
                content_digest: &document.content_digest,
                byte_length: document.byte_length,
                maintenance: Some(document),
            },
            reservation,
            now_unix_milliseconds,
        )?;
        Ok(MaintenancePublication { publication })
    }

    fn retain_publication(
        &self,
        metadata: PublicationMetadata<'_>,
        mut reservation: Option<ArtifactReservation<'_>>,
        now_unix_milliseconds: u64,
    ) -> Result<ArtifactPublication, AccountingFailure> {
        use rusqlite::OptionalExtension as _;
        let invalid =
            || AccountingFailure::DatabaseRefused("the publication hold is not valid".to_owned());
        let length = i64::try_from(metadata.byte_length).map_err(|_| invalid())?;
        let now = i64::try_from(now_unix_milliseconds).map_err(|_| invalid())?;
        let transaction = rusqlite::Transaction::new_unchecked(
            self.database.connection(),
            rusqlite::TransactionBehavior::Immediate,
        )
        .map_err(refused)?;
        let existing = self.committed_length(metadata.content_digest)?;
        // Each retained command can produce one remote artifact and one local
        // structured-result artifact. Pending producers consume bounded rows
        // even when their shared content consumes no additional byte charge.
        let maximum_publications =
            self.policy.retained_operation_rows.checked_mul(2).ok_or_else(invalid)?;
        if self.count("count this namespace's pending artifact publications", &[])?
            >= maximum_publications
        {
            return Err(invalid());
        }
        if existing.is_some_and(|held| held != metadata.byte_length) {
            return Err(AccountingFailure::ContentLengthConflict);
        }
        if let Some(hold) = &reservation {
            if !core::ptr::eq(hold.database, self.database) {
                return Err(invalid());
            }
            let ticket =
                hold.ticket.and_then(|value| i64::try_from(value).ok()).ok_or_else(invalid)?;
            let held: Option<i64> = transaction
                .query_row(statement("read one durable artifact reservation"), [ticket], |row| {
                    row.get(0)
                })
                .optional()
                .map_err(refused)?;
            if held != Some(length) {
                return Err(invalid());
            }
        } else if existing.is_none() {
            return Err(invalid());
        }
        transaction
            .execute(
                statement("record one artifact's content, once per digest"),
                rusqlite::params![length, metadata.content_digest, now],
            )
            .map_err(refused)?;
        let identifier = uuid::Uuid::new_v4().to_string();
        transaction
            .execute(
                statement("retain one artifact publication across restart"),
                rusqlite::params![
                    identifier,
                    metadata.artifact_identifier,
                    metadata.content_digest,
                    now
                ],
            )
            .map_err(refused)?;
        if let Some(document) = metadata.maintenance {
            let kind = match document.kind {
                slingshot_domain::daemon_runtime_contract::MaintenanceResultKind::Preview => {
                    "preview"
                }
                slingshot_domain::daemon_runtime_contract::MaintenanceResultKind::Application => {
                    "application"
                }
            };
            transaction
                .execute(
                    statement("bind a maintenance publication to its operation-free owner"),
                    rusqlite::params![
                        identifier,
                        document.target,
                        kind,
                        document.reviewed_source_digest
                    ],
                )
                .map_err(refused)?;
        }
        if let Some(hold) = &reservation {
            let ticket =
                hold.ticket.and_then(|value| i64::try_from(value).ok()).ok_or_else(invalid)?;
            if transaction
                .execute(statement("release one durable artifact reservation"), [ticket])
                .map_err(refused)?
                != 1
            {
                return Err(invalid());
            }
        }
        transaction.commit().map_err(refused)?;
        if let Some(hold) = &mut reservation {
            hold.ticket = None;
        }
        Ok(ArtifactPublication { identifier })
    }

    /// Number of durable publication holds awaiting completion or reconciliation.
    pub fn pending_publications(&self) -> Result<u64, AccountingFailure> {
        self.count("count this namespace's pending artifact publications", &[])
    }

    /// Reads the complete bounded publication inventory without releasing holds
    /// or collapsing ambiguous producers. A left join makes missing accounting
    /// visible as a refusal rather than silently omitting its publication.
    pub fn reconstruct_publications(
        &self,
    ) -> Result<Vec<PendingArtifactPublication>, AccountingFailure> {
        let invalid = || {
            AccountingFailure::DatabaseRefused(
                "the retained publication inventory is invalid".to_owned(),
            )
        };
        let maximum = self.policy.retained_operation_rows.checked_mul(2).ok_or_else(invalid)?;
        let transaction = self.database.connection().unchecked_transaction().map_err(refused)?;
        let mut statement = transaction
            .prepare(statement("reconstruct bounded pending artifact publications"))
            .map_err(refused)?;
        let mut retained = Vec::new();
        let mut cursor: Option<String> = None;
        loop {
            let mut page = 0;
            let mut rows = statement.query(rusqlite::params![cursor, cursor]).map_err(refused)?;
            while let Some(row) = rows.next().map_err(refused)? {
                page += 1;
                if retained.len() as u64 >= maximum {
                    return Err(invalid());
                }
                let publication_identifier: String = row.get(0).map_err(refused)?;
                let artifact: String = row.get(1).map_err(refused)?;
                let content_digest: String = row.get(2).map_err(refused)?;
                let length: i64 = row.get(3).map_err(refused)?;
                let timestamp: i64 = row.get(4).map_err(refused)?;
                let identifier =
                    uuid::Uuid::parse_str(&publication_identifier).map_err(|_| invalid())?;
                if identifier.to_string() != publication_identifier {
                    return Err(invalid());
                }
                cursor = Some(publication_identifier.clone());
                let artifact_identifier =
                    crate::artifact_store::ArtifactIdentifier::parse(&artifact)
                        .map_err(|_| invalid())?;
                crate::artifact_store::ArtifactIdentifier::parse(&content_digest)
                    .map_err(|_| invalid())?;
                let byte_length = u64::try_from(length).map_err(|_| invalid())?;
                if byte_length > self.policy.individual_artifact_bytes {
                    return Err(invalid());
                }
                retained.push(PendingArtifactPublication {
                    publication_identifier,
                    artifact_identifier,
                    content_digest,
                    byte_length,
                    recorded_at_unix_milliseconds: u64::try_from(timestamp)
                        .map_err(|_| invalid())?,
                });
            }
            if page < 256 {
                break;
            }
        }
        drop(statement);
        transaction.commit().map_err(refused)?;
        Ok(retained)
    }

    /// Recovers a sole pending producer for exactly the expected artifact and
    /// verified content. Multiple producers or changed content refuse without
    /// choosing or deleting a hold. This does not prove file presence or grant
    /// execution authority; the caller still validates the result and its owner.
    pub fn recover_publication(
        &self,
        metadata: &crate::artifact_store::ArtifactMetadata,
    ) -> Result<Option<ArtifactPublication>, AccountingFailure> {
        let invalid = || {
            AccountingFailure::DatabaseRefused(
                "the pending publication is ambiguous or changed".to_owned(),
            )
        };
        let mut statement = self
            .database
            .connection()
            .prepare(statement("find an artifact's pending publication without hiding ambiguity"))
            .map_err(refused)?;
        let mut rows =
            statement.query([metadata.artifact_identifier.as_text()]).map_err(refused)?;
        let Some(row) = rows.next().map_err(refused)? else {
            return Ok(None);
        };
        let identifier: String = row.get(0).map_err(refused)?;
        let digest: String = row.get(1).map_err(refused)?;
        let length: i64 = row.get(2).map_err(refused)?;
        if digest != metadata.content_digest
            || u64::try_from(length).ok() != Some(metadata.byte_length)
            || rows.next().map_err(refused)?.is_some()
        {
            return Err(invalid());
        }
        Ok(Some(ArtifactPublication { identifier }))
    }

    /// Releases one reservation without committing it.
    ///
    /// An installation that was abandoned held bytes it never used, and holding
    /// them afterwards would refuse work for space nothing occupies.
    pub fn release(&self, reservation: ArtifactReservation<'_>) {
        drop(reservation);
    }

    /// Converts one reservation into committed usage.
    ///
    /// The caller has already committed the blob row, so the bytes are now
    /// counted by the authoritative table. Dropping the reservation at the same
    /// moment is what makes the total neither double-count them nor lose them.
    pub fn commit(&self, reservation: ArtifactReservation<'_>) {
        self.release(reservation);
    }

    /// Returns the length of committed content, when it is committed.
    fn committed_length(&self, content_digest: &str) -> Result<Option<u64>, AccountingFailure> {
        let mut prepared = self
            .database
            .connection()
            .prepare(statement("read one artifact blob's recorded length"))
            .map_err(refused)?;
        let found = prepared
            .query_row(rusqlite::params![content_digest], |row| row.get::<_, i64>(0))
            .map(Some)
            .or_else(|failure| match failure {
                rusqlite::Error::QueryReturnedNoRows => Ok(None),
                other => Err(refused(other)),
            })?;
        found
            .map(|length| u64::try_from(length).map_err(|_| AccountingFailure::NotACount(length)))
            .transpose()
    }
}
