//! Atomic association writes for already verified, durably accounted content.

use super::*;
use crate::persistent_capacity::{AccountingFailure, PersistentCapacityAccount};

/// Receipt of a current-preview association write.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PreviewWrite {
    /// The durable association now retaining the reviewed preview.
    pub current: MaintenanceResultMetadata,
    /// A byte-identical preview was already retained; no association changed.
    pub replayed: bool,
    /// Prior association retired atomically. Its blob remains conservatively
    /// accounted until the owner performs reference-checked file cleanup.
    pub superseded: Option<MaintenanceResultMetadata>,
}

/// The immutable application-result association retained by a receipt.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ApplicationWrite {
    /// Validated durable association.
    pub result: MaintenanceResultMetadata,
    /// The same result was already retained; nothing was replaced.
    pub replayed: bool,
}

/// Why a preview association was not changed.
#[derive(Debug, thiserror::Error)]
pub enum WriteFailure {
    /// The previous supersession still has a durable cleanup obligation.
    #[error("superseded preview cleanup must finish before another replacement")]
    CleanupPending,
    /// The receipt repository could not complete its observation.
    #[error(transparent)]
    Repository(#[from] crate::operation_repository::RepositoryFailure),
    /// Content or retained state failed validation.
    #[error(transparent)]
    Read(#[from] ReadFailure),
    /// Capacity refused the association write.
    #[error(transparent)]
    Capacity(#[from] AccountingFailure),
}

/// Retains one application document for an existing target-qualified receipt.
/// Content must already be verified and durably accounted; the caller retains
/// the exact publication hold through commit, as for `record_current_preview`.
/// A receipt never silently switches to a different result document.
///
/// # Errors
/// Refuses invalid ownership/content, a conflicting result, or capacity pressure
/// without changing the receipt, preview, or any previously retained result.
pub fn record_application_result(
    database: &OperationDatabase,
    accounting: &PersistentCapacityAccount<'_>,
    target: &str,
    receipt_identifier: &str,
    content_digest: &str,
    byte_length: u64,
) -> Result<ApplicationWrite, WriteFailure> {
    let invalid = || WriteFailure::Read(ReadFailure::Invalid);
    if !accounting.belongs_to(database) {
        return Err(invalid());
    }
    let identifier = MaintenanceResultIdentifier::derive(
        &digest(target)?,
        MaintenanceResultKind::Application,
        &digest(receipt_identifier)?,
        &digest(content_digest)?,
    );
    let length = i64::try_from(byte_length).map_err(|_| invalid())?;
    let transaction = rusqlite::Transaction::new_unchecked(
        database.connection(),
        rusqlite::TransactionBehavior::Immediate,
    )
    .map_err(ReadFailure::Database)?;
    if crate::maintenance::receipt(database, target, receipt_identifier)?.is_none() {
        return Err(invalid());
    }
    let blob_length: Option<i64> = transaction
        .query_row(sql("read one artifact blob's recorded length"), [content_digest], |row| {
            row.get(0)
        })
        .optional()
        .map_err(ReadFailure::Database)?;
    let metadata = validate(
        digest(target)?,
        &identifier,
        Row {
            revision: 1,
            length,
            digest: content_digest.to_owned(),
            kind: "application".to_owned(),
            media_type: "application/json".to_owned(),
            owner: Some(receipt_identifier.to_owned()),
            source: receipt_identifier.to_owned(),
            current: 0,
            blob_length,
            receipt: Some(receipt_identifier.to_owned()),
        },
    )?;
    let existing = {
        let mut query = transaction
            .prepare(sql("read application result identifiers owned by one receipt"))
            .map_err(ReadFailure::Database)?;
        let rows = query
            .query_map(rusqlite::params![target, receipt_identifier], |row| row.get::<_, String>(0))
            .map_err(ReadFailure::Database)?;
        rows.collect::<Result<Vec<_>, _>>().map_err(ReadFailure::Database)?
    };
    if !existing.is_empty() {
        if existing.len() != 1 || existing[0] != identifier.as_text() {
            return Err(invalid());
        }
        let result = read(database, target, &identifier)?.ok_or_else(invalid)?;
        if result != metadata {
            return Err(invalid());
        }
        return Ok(ApplicationWrite { result, replayed: true });
    }
    accounting.require_room_for_maintenance_associations(target, 1)?;
    transaction
        .execute(
            sql("record a receipt-owned maintenance application result"),
            rusqlite::params![
                target,
                length,
                content_digest,
                identifier.as_text(),
                receipt_identifier,
                receipt_identifier
            ],
        )
        .map_err(ReadFailure::Database)?;
    transaction.commit().map_err(ReadFailure::Database)?;
    Ok(ApplicationWrite { result: metadata, replayed: false })
}

fn sql(purpose: &str) -> &'static str {
    crate::sqlite_statement_inventory::STATEMENTS
        .iter()
        .find(|statement| statement.purpose == purpose)
        .expect("maintenance writes are inventoried")
        .text
}

/// Reads the current association under the caller's maintenance transaction.
pub(crate) fn current_preview(
    database: &OperationDatabase,
    target: &str,
) -> Result<Option<MaintenanceResultMetadata>, ReadFailure> {
    let identifier: Option<String> = database
        .connection()
        .query_row(sql("read the current maintenance preview identifier"), [target], |row| {
            row.get(0)
        })
        .optional()
        .map_err(ReadFailure::Database)?;
    identifier
        .map(|identifier| {
            let identifier = MaintenanceResultIdentifier::parse(&identifier)
                .map_err(|_| ReadFailure::Invalid)?;
            read(database, target, &identifier)?.ok_or(ReadFailure::Invalid)
        })
        .transpose()
}

/// The caller holds the apply transaction, with its new receipt already inserted.
/// Thus receipt ownership and removal commit together or roll back together.
pub(crate) fn retain_applied_preview(
    database: &OperationDatabase,
    target: &str,
    preview: &MaintenanceResultMetadata,
) -> Result<(), ReadFailure> {
    let revision = i64::try_from(preview.association_revision).map_err(|_| ReadFailure::Invalid)?;
    if revision == i64::MAX || preview.owner != RetentionOwner::CurrentPreview {
        return Err(ReadFailure::Invalid);
    }
    let changed = database
        .connection()
        .execute(
            sql("retain an applied preview under its application receipt"),
            rusqlite::params![
                preview.reviewed_source_digest,
                target,
                preview.identifier.as_text(),
                preview.reviewed_source_digest,
                revision
            ],
        )
        .map_err(ReadFailure::Database)?;
    if changed != 1 {
        return Err(ReadFailure::Invalid);
    }
    Ok(())
}

/// Retains a preview over an already accounted blob. The caller must keep
/// namespace ownership and the verified content's durable publication hold
/// through this transaction, releasing that exact hold only after commit.
/// This method neither stages files nor asserts that file publication happened.
///
/// # Errors
/// Refuses missing/mismatched content, foreign accounting, invalid retained
/// state, or capacity pressure without replacing the prior preview.
pub fn record_current_preview(
    database: &OperationDatabase,
    accounting: &PersistentCapacityAccount<'_>,
    target: &str,
    reviewed_source_digest: &str,
    content_digest: &str,
    byte_length: u64,
) -> Result<PreviewWrite, WriteFailure> {
    let invalid = || WriteFailure::Read(ReadFailure::Invalid);
    if !accounting.belongs_to(database) {
        return Err(invalid());
    }
    let identifier = MaintenanceResultIdentifier::derive(
        &digest(target)?,
        MaintenanceResultKind::Preview,
        &digest(reviewed_source_digest)?,
        &digest(content_digest)?,
    );
    let length = i64::try_from(byte_length).map_err(|_| invalid())?;
    let transaction = rusqlite::Transaction::new_unchecked(
        database.connection(),
        rusqlite::TransactionBehavior::Immediate,
    )
    .map_err(ReadFailure::Database)?;
    let blob_length: Option<i64> = transaction
        .query_row(sql("read one artifact blob's recorded length"), [content_digest], |row| {
            row.get(0)
        })
        .optional()
        .map_err(ReadFailure::Database)?;
    let metadata = validate(
        digest(target)?,
        &identifier,
        Row {
            revision: 1,
            length,
            digest: content_digest.to_owned(),
            kind: "preview".to_owned(),
            media_type: "application/json".to_owned(),
            owner: None,
            source: reviewed_source_digest.to_owned(),
            current: 1,
            blob_length,
            receipt: None,
        },
    )?;
    let existing = read(database, target, &identifier)?;
    if let Some(current) = existing {
        if current.byte_length != metadata.byte_length
            || current.owner != RetentionOwner::CurrentPreview
        {
            return Err(invalid());
        }
        return Ok(PreviewWrite { current, replayed: true, superseded: None });
    }
    if !super::cleanup::pending(database, target)?.is_empty() {
        return Err(WriteFailure::CleanupPending);
    }
    let prior: Option<String> = transaction
        .query_row(sql("read the current maintenance preview identifier"), [target], |row| {
            row.get(0)
        })
        .optional()
        .map_err(ReadFailure::Database)?;
    let superseded = prior
        .map(|prior| {
            let prior =
                MaintenanceResultIdentifier::parse(&prior).map_err(|_| ReadFailure::Invalid)?;
            read(database, target, &prior)?.ok_or(ReadFailure::Invalid)
        })
        .transpose()?;
    transaction
        .execute(sql("retire the current maintenance preview association"), [target])
        .map_err(ReadFailure::Database)?;
    // Count under the write transaction after supersession. Any later refusal
    // rolls that deletion back together with the attempted replacement.
    accounting.require_room_for_maintenance_associations(target, 1)?;
    transaction
        .execute(
            sql("record the current maintenance preview association"),
            rusqlite::params![
                target,
                length,
                content_digest,
                identifier.as_text(),
                reviewed_source_digest
            ],
        )
        .map_err(ReadFailure::Database)?;
    if let Some(prior) = &superseded
        && prior.content_digest != metadata.content_digest
    {
        transaction
            .execute(
                sql("record one maintenance artifact cleanup item"),
                rusqlite::params![
                    format!("preview:{}", prior.identifier.as_text()),
                    target,
                    prior.content_digest
                ],
            )
            .map_err(ReadFailure::Database)?;
    }
    transaction.commit().map_err(ReadFailure::Database)?;
    Ok(PreviewWrite { current: metadata, replayed: false, superseded })
}

#[cfg(test)]
mod tests {
    use super::*;
    use slingshot_domain::persistent_capacity::PersistentCapacityPolicy;

    #[test]
    fn apply_atomically_retains_its_preview_and_replay_preserves_later_current_preview() {
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("receipt-owner.sqlite");
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
        let target = "a".repeat(DIGEST_OCTETS * 2);
        let content = "b".repeat(DIGEST_OCTETS * 2);
        database
            .connection()
            .execute(
                sql("record one artifact's content, once per digest"),
                rusqlite::params![2, content, 1],
            )
            .unwrap();
        let account =
            PersistentCapacityAccount::new(&database, PersistentCapacityPolicy::embedded());
        let first = crate::maintenance::preview(&database, &target, 2, 1).unwrap();
        let second = crate::maintenance::preview(&database, &target, 3, 1).unwrap();
        record_current_preview(&database, &account, &target, &first.digest(), &content, 2).unwrap();
        let current =
            record_current_preview(&database, &account, &target, &second.digest(), &content, 2)
                .unwrap()
                .current;
        assert!(matches!(
            crate::maintenance::apply(&database, &first, 4),
            Err(crate::maintenance::MaintenanceFailure::ManifestChanged { .. })
        ));
        assert!(
            crate::maintenance::receipt(&database, &target, &first.digest()).unwrap().is_none()
        );
        assert_eq!(read(&database, &target, &current.identifier).unwrap(), Some(current.clone()));
        assert!(matches!(
            crate::maintenance::apply(&database, &second, 4).unwrap(),
            crate::maintenance::ApplyOutcome::Applied(_)
        ));
        let owned = read(&database, &target, &current.identifier).unwrap().unwrap();
        assert_eq!(owned.association_revision, current.association_revision + 1);
        assert_eq!(owned.owner, RetentionOwner::ApplicationReceipt(second.digest()));
        assert!(current_preview(&database, &target).unwrap().is_none());
        let full = PersistentCapacityAccount::new(
            &database,
            PersistentCapacityPolicy {
                maintenance_result_associations_per_target: 0,
                ..PersistentCapacityPolicy::embedded()
            },
        );
        assert!(matches!(
            record_application_result(&database, &full, &target, &second.digest(), &content, 2),
            Err(WriteFailure::Capacity(_))
        ));
        let application =
            record_application_result(&database, &account, &target, &second.digest(), &content, 2)
                .unwrap();
        assert!(!application.replayed);
        assert_eq!(application.result.kind, MaintenanceResultKind::Application);
        assert_ne!(application.result.identifier, owned.identifier);
        assert!(
            record_application_result(&database, &full, &target, &second.digest(), &content, 2)
                .unwrap()
                .replayed
        );
        let other_content = "c".repeat(DIGEST_OCTETS * 2);
        database
            .connection()
            .execute(
                sql("record one artifact's content, once per digest"),
                rusqlite::params![2, other_content, 1],
            )
            .unwrap();
        assert!(
            record_application_result(
                &database,
                &account,
                &target,
                &second.digest(),
                &other_content,
                2
            )
            .is_err()
        );
        assert!(
            record_application_result(
                &database,
                &account,
                &"d".repeat(DIGEST_OCTETS * 2),
                &second.digest(),
                &content,
                2
            )
            .is_err()
        );
        assert!(
            record_application_result(&database, &account, &target, &first.digest(), &content, 2)
                .is_err()
        );
        assert_eq!(
            read(&database, &target, &application.result.identifier).unwrap(),
            Some(application.result.clone())
        );
        drop(full);
        drop(account);
        drop(database);
        let database = open();
        assert_eq!(read(&database, &target, &current.identifier).unwrap(), Some(owned.clone()));
        let account =
            PersistentCapacityAccount::new(&database, PersistentCapacityPolicy::embedded());
        assert!(
            record_application_result(&database, &account, &target, &second.digest(), &content, 2)
                .unwrap()
                .replayed
        );
        assert_eq!(
            read(&database, &target, &application.result.identifier).unwrap(),
            Some(application.result)
        );
        let later = crate::maintenance::preview(&database, &target, 5, 1).unwrap();
        let new =
            record_current_preview(&database, &account, &target, &later.digest(), &content, 2)
                .unwrap()
                .current;
        assert!(matches!(
            crate::maintenance::apply(&database, &second, 6).unwrap(),
            crate::maintenance::ApplyOutcome::Replayed(_)
        ));
        assert_eq!(current_preview(&database, &target).unwrap(), Some(new));
        assert_eq!(read(&database, &target, &owned.identifier).unwrap(), Some(owned));
    }

    #[test]
    fn preview_writes_replay_replace_and_roll_back_at_capacity_after_reopen() {
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("preview.sqlite");
        let contract = DaemonRuntimeContract::embedded();
        let open = || {
            OperationDatabase::open(
                &path,
                crate::database::RequiredSettings {
                    page_bytes: contract.limit("sqlite_page_bytes"),
                    database_pages: contract.limit("maximum_sqlite_database_pages"),
                    busy_timeout_milliseconds: contract.limit("database_busy_timeout_milliseconds"),
                },
            )
            .unwrap()
        };
        let database = open();
        let target = "a".repeat(DIGEST_OCTETS * 2);
        let source = "b".repeat(DIGEST_OCTETS * 2);
        let first = "c".repeat(DIGEST_OCTETS * 2);
        let second = "d".repeat(DIGEST_OCTETS * 2);
        // Seed only the already-accounted blob prerequisite; these tests make
        // no claim about file staging/publication, which this API does not do.
        for content in [&first, &second] {
            database
                .connection()
                .execute(
                    sql("record one artifact's content, once per digest"),
                    rusqlite::params![2, content, 1],
                )
                .unwrap();
        }
        let policy = PersistentCapacityPolicy {
            maintenance_result_associations_per_target: 1,
            ..PersistentCapacityPolicy::embedded()
        };
        let account = PersistentCapacityAccount::new(&database, policy);
        let written =
            record_current_preview(&database, &account, &target, &source, &first, 2).unwrap();
        assert!(!written.replayed);
        assert!(written.superseded.is_none());
        assert_eq!(
            read(&database, &target, &written.current.identifier).unwrap(),
            Some(written.current.clone())
        );
        drop(account);
        drop(database);
        let database = open();
        let full = PersistentCapacityAccount::new(
            &database,
            PersistentCapacityPolicy { maintenance_result_associations_per_target: 0, ..policy },
        );
        let replay = record_current_preview(&database, &full, &target, &source, &first, 2).unwrap();
        assert!(replay.replayed);
        assert!(matches!(
            record_current_preview(&database, &full, &target, &source, &second, 2),
            Err(WriteFailure::Capacity(_))
        ));
        assert_eq!(
            read(&database, &target, &written.current.identifier).unwrap(),
            Some(written.current.clone())
        );
        let account = PersistentCapacityAccount::new(&database, policy);
        assert!(record_current_preview(&database, &account, &target, &source, &second, 3).is_err());
        let replaced =
            record_current_preview(&database, &account, &target, &source, &second, 2).unwrap();
        assert_eq!(replaced.superseded, Some(written.current.clone()));
        assert!(read(&database, &target, &written.current.identifier).unwrap().is_none());
        assert_eq!(
            read(&database, &target, &replaced.current.identifier).unwrap(),
            Some(replaced.current.clone())
        );
        let foreign = "e".repeat(DIGEST_OCTETS * 2);
        assert!(read(&database, &foreign, &replaced.current.identifier).unwrap().is_none());
        assert!(record_current_preview(&database, &account, &foreign, &source, &first, 2).is_ok());
        let missing = "f".repeat(DIGEST_OCTETS * 2);
        assert!(
            record_current_preview(&database, &account, &target, &source, &missing, 2).is_err()
        );
        assert_eq!(
            read(&database, &target, &replaced.current.identifier).unwrap(),
            Some(replaced.current)
        );
        assert!(matches!(
            record_current_preview(&database, &account, &target, &source, &first, 2),
            Err(WriteFailure::CleanupPending)
        ));
        drop(account);
        drop(full);
        drop(database);
        let database = open();
        let store = crate::artifact_store::ArtifactStore::open(root.path()).unwrap();
        // Another target durably retains the old content: completing this
        // supersession must keep that blob accounted rather than deleting it.
        assert_eq!(
            super::super::cleanup_superseded_preview(&database, &store, &target).unwrap(),
            1
        );
        assert_eq!(
            super::super::cleanup_superseded_preview(&database, &store, &target).unwrap(),
            0
        );
        let account = PersistentCapacityAccount::new(&database, policy);
        assert!(record_current_preview(&database, &account, &target, &source, &first, 2).is_ok());
    }
}
