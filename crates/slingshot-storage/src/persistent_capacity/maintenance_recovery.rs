//! Reconstruction of operation-free publication ownership, without releasing it.

use super::*;
use crate::artifact_store::maintenance_content::MaintenanceDocument;
use rusqlite::OptionalExtension as _;
use slingshot_domain::daemon_runtime_contract::{
    DIGEST_OCTETS, MaintenanceResultIdentifier, MaintenanceResultKind,
};

/// A fully identity-bound publication record. File presence and association
/// completion are separate facts, never inferred from this retained owner.
#[derive(Debug, Clone)]
pub struct PendingMaintenancePublication {
    document: MaintenanceDocument,
    hold: MaintenancePublication,
    recorded_at_unix_milliseconds: u64,
}

/// Startup reconciles producer state, never creates an association or approval.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MaintenancePublicationRecovery {
    /// An already retained result and its bytes verified; its hold was consumed.
    Completed,
    /// No result association committed; only the interrupted producer was retired.
    Abandoned,
}

impl PendingMaintenancePublication {
    /// The validated target/kind/source/content derivation retained at staging.
    pub fn document(&self) -> &MaintenanceDocument {
        &self.document
    }
    /// Exact durable producer token; dropping it does not release protection.
    pub fn hold(&self) -> &MaintenancePublication {
        &self.hold
    }
    /// Original publication time, not a refreshed recovery timestamp.
    pub fn recorded_at_unix_milliseconds(&self) -> u64 {
        self.recorded_at_unix_milliseconds
    }
}

impl PersistentCapacityAccount<'_> {
    /// Reconciles an authenticated interrupted producer under exclusive namespace
    /// ownership, after private stage cleanup and the complete owner audit.
    /// Published associations are completed, not recreated. Without an
    /// association the producer never acknowledged a result: remove only its
    /// hold and, if no other reference remains, its private content and charge.
    ///
    /// # Errors
    /// Invalid ownership or I/O/database failure retains retryable producer
    /// evidence. Referenced content and approved maintenance history never shrink.
    pub fn reconcile_maintenance_publication(
        &self,
        store: &crate::artifact_store::ArtifactStore,
        target: &str,
        hold: &MaintenancePublication,
    ) -> Result<MaintenancePublicationRecovery, AccountingFailure> {
        let transaction = rusqlite::Transaction::new_unchecked(
            self.database.connection(),
            rusqlite::TransactionBehavior::Immediate,
        )
        .map_err(refused)?;
        let pending = self.reconstruct_maintenance_publication(target)?.ok_or_else(invalid)?;
        if pending.hold().identifier() != hold.identifier() {
            return Err(invalid());
        }
        let document = pending.document();
        if crate::maintenance_results::read(self.database, target, &document.identifier)
            .map_err(|_| invalid())?
            .is_some()
        {
            drop(transaction);
            self.complete_maintenance_publication(store, target, hold)?;
            return Ok(MaintenancePublicationRecovery::Completed);
        }
        let changed = transaction
            .execute(
                statement("consume one completed artifact publication"),
                rusqlite::params![
                    hold.identifier(),
                    document.identifier.as_text(),
                    document.content_digest
                ],
            )
            .map_err(refused)?;
        if changed != 1 {
            return Err(invalid());
        }
        let references: i64 = transaction
            .query_row(
                statement("count what still references one artifact's content"),
                rusqlite::params![
                    document.content_digest,
                    document.content_digest,
                    document.content_digest
                ],
                |row| row.get(0),
            )
            .map_err(refused)?;
        if references < 0 {
            return Err(invalid());
        }
        if references == 0 {
            store.remove_unreferenced_content(&document.content_digest).map_err(|_| invalid())?;
            transaction
                .execute(
                    statement("remove one artifact's content, once nothing references it"),
                    [&document.content_digest],
                )
                .map_err(refused)?;
        }
        transaction.commit().map_err(refused)?;
        Ok(MaintenancePublicationRecovery::Abandoned)
    }

    /// Releases only this producer after validating the retained association and
    /// its published bytes. The caller holds namespace ownership throughout.
    /// Association commit deliberately precedes this transaction: a crash in
    /// between leaves a recoverable hold, never unprotected content.
    ///
    /// # Errors
    /// Missing, stale, foreign, corrupt, or unpublished evidence leaves the hold
    /// and byte accounting unchanged. This never creates maintenance approval.
    pub fn complete_maintenance_publication(
        &self,
        store: &crate::artifact_store::ArtifactStore,
        target: &str,
        hold: &MaintenancePublication,
    ) -> Result<(), AccountingFailure> {
        let transaction = rusqlite::Transaction::new_unchecked(
            self.database.connection(),
            rusqlite::TransactionBehavior::Immediate,
        )
        .map_err(refused)?;
        let pending = self.reconstruct_maintenance_publication(target)?.ok_or_else(invalid)?;
        if pending.hold().identifier() != hold.identifier() {
            return Err(invalid());
        }
        let document = pending.document();
        let metadata =
            crate::maintenance_results::read(self.database, target, &document.identifier)
                .map_err(|_| invalid())?
                .ok_or_else(invalid)?;
        if metadata.identifier != document.identifier
            || metadata.kind != document.kind
            || metadata.reviewed_source_digest != document.reviewed_source_digest
            || metadata.content_digest != document.content_digest
            || metadata.byte_length != document.byte_length
        {
            return Err(invalid());
        }
        let mut reader = store.open_maintenance_result(&metadata).map_err(|_| invalid())?;
        std::io::copy(
            &mut std::io::Read::take(&mut reader, document.byte_length.saturating_add(1)),
            &mut std::io::sink(),
        )
        .map_err(|_| invalid())?;
        reader.finish().map_err(|_| invalid())?;
        let changed = transaction
            .execute(
                statement("consume one completed artifact publication"),
                rusqlite::params![
                    hold.identifier(),
                    document.identifier.as_text(),
                    document.content_digest
                ],
            )
            .map_err(refused)?;
        if changed != 1 {
            return Err(invalid());
        }
        transaction.commit().map_err(refused)
    }

    /// Classifies the complete inventory while the caller holds namespace
    /// ownership. Only the selected target's validated maintenance producer is
    /// removed from the operation inventory; unknown/foreign producers remain
    /// visible for the operation-owner audit to reject.
    pub fn reconstruct_target_publications(
        &self,
        target: &str,
    ) -> Result<
        (Vec<PendingArtifactPublication>, Option<PendingMaintenancePublication>),
        AccountingFailure,
    > {
        let mut operations = self.reconstruct_publications()?;
        let maintenance = self.reconstruct_maintenance_publication(target)?;
        if let Some(pending) = &maintenance {
            let position = operations
                .iter()
                .position(|record| record.publication_identifier == pending.hold.identifier())
                .ok_or_else(invalid)?;
            let record = &operations[position];
            let document = pending.document();
            if record.artifact_identifier.as_text() != document.identifier.as_text()
                || record.content_digest != document.content_digest
                || record.byte_length != document.byte_length
                || record.recorded_at_unix_milliseconds != pending.recorded_at_unix_milliseconds()
            {
                return Err(invalid());
            }
            operations.remove(position);
        }
        Ok((operations, maintenance))
    }

    /// Reconstructs the target's sole pending maintenance publication without
    /// looking up an operation or changing any reservation, blob, or association.
    ///
    /// # Errors
    /// Refuses noncanonical identifiers, broken derivation, invalid kind/length,
    /// missing blob accounting, or unreadable ownership records.
    pub fn reconstruct_maintenance_publication(
        &self,
        target: &str,
    ) -> Result<Option<PendingMaintenancePublication>, AccountingFailure> {
        let target_bytes = digest(target)?;
        let mut statement = self
            .database
            .connection()
            .prepare(statement("read one target's pending maintenance publication"))
            .map_err(refused)?;
        let row = statement
            .query_row([target], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, i64>(3)?,
                    row.get::<_, i64>(4)?,
                    row.get::<_, String>(5)?,
                    row.get::<_, String>(6)?,
                ))
            })
            .optional()
            .map_err(refused)?;
        let Some((publication, identifier, content, length, timestamp, kind, source)) = row else {
            return Ok(None);
        };
        let uuid = uuid::Uuid::parse_str(&publication).map_err(|_| invalid())?;
        if uuid.to_string() != publication {
            return Err(invalid());
        }
        let kind = match kind.as_str() {
            "preview" => MaintenanceResultKind::Preview,
            "application" => MaintenanceResultKind::Application,
            _ => return Err(invalid()),
        };
        let expected = MaintenanceResultIdentifier::derive(
            &target_bytes,
            kind,
            &digest(&source)?,
            &digest(&content)?,
        );
        let parsed = MaintenanceResultIdentifier::parse(&identifier).map_err(|_| invalid())?;
        let length = u64::try_from(length).map_err(|_| invalid())?;
        let timestamp = u64::try_from(timestamp).map_err(|_| invalid())?;
        let contract = slingshot_domain::daemon_runtime_contract::DaemonRuntimeContract::embedded();
        let limit = match kind {
            MaintenanceResultKind::Preview => {
                contract.limit("maximum_terminal_maintenance_manifest_bytes")
            }
            MaintenanceResultKind::Application => {
                contract.limit("maximum_canonical_structured_result_bytes")
            }
        };
        if parsed != expected
            || length == 0
            || length > limit
            || length > self.policy.individual_artifact_bytes
        {
            return Err(invalid());
        }
        Ok(Some(PendingMaintenancePublication {
            document: MaintenanceDocument {
                target: target.to_owned(),
                byte_length: length,
                content_digest: content,
                identifier: parsed,
                kind,
                reviewed_source_digest: source,
            },
            hold: MaintenancePublication {
                publication: ArtifactPublication { identifier: publication },
            },
            recorded_at_unix_milliseconds: timestamp,
        }))
    }
}

fn invalid() -> AccountingFailure {
    AccountingFailure::DatabaseRefused("the retained maintenance publication is invalid".to_owned())
}

fn digest(text: &str) -> Result<[u8; DIGEST_OCTETS], AccountingFailure> {
    if text.len() != DIGEST_OCTETS * 2
        || !text.bytes().all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    {
        return Err(invalid());
    }
    let mut bytes = [0; DIGEST_OCTETS];
    hex::decode_to_slice(text, &mut bytes).map_err(|_| invalid())?;
    Ok(bytes)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn restart_reconciles_each_publication_boundary_and_retries_cleanup_failure() {
        for mode in ["staged", "published", "associated", "shared", "blocked", "corrupt-result"] {
            let root = tempfile::tempdir().unwrap();
            let path = root.path().join("recovery.sqlite");
            let limits =
                slingshot_domain::daemon_runtime_contract::DaemonRuntimeContract::embedded();
            let open = || {
                OperationDatabase::open(
                    &path,
                    crate::database::RequiredSettings {
                        page_bytes: limits.limit("sqlite_page_bytes"),
                        database_pages: limits.limit("maximum_sqlite_database_pages"),
                        busy_timeout_milliseconds: limits
                            .limit("database_busy_timeout_milliseconds"),
                    },
                )
                .unwrap()
            };
            let database = open();
            let store = crate::artifact_store::ArtifactStore::open(root.path()).unwrap();
            let target = "a".repeat(DIGEST_OCTETS * 2);
            let source = "b".repeat(DIGEST_OCTETS * 2);
            let account =
                PersistentCapacityAccount::new(&database, PersistentCapacityPolicy::embedded());
            let reservation = account.reserve_artifact(None, 2).unwrap();
            let stage = store
                .stage_maintenance_document(&target, MaintenanceResultKind::Preview, &source, b"{}")
                .unwrap();
            let hold = account.retain_maintenance_publication(&stage, reservation, 1).unwrap();
            let document = stage.document().clone();
            let content_path = root.path().join("content").join(&document.content_digest);
            if mode == "staged" || mode == "blocked" {
                drop(stage);
            } else {
                stage.publish().unwrap();
            }
            if mode == "associated" || mode == "corrupt-result" {
                crate::maintenance_results::record_current_preview(
                    &database,
                    &account,
                    &target,
                    &source,
                    &document.content_digest,
                    2,
                )
                .unwrap();
            }
            if mode == "shared" {
                let other = "c".repeat(DIGEST_OCTETS * 2);
                crate::maintenance_results::record_current_preview(
                    &database,
                    &account,
                    &other,
                    &source,
                    &document.content_digest,
                    2,
                )
                .unwrap();
            }
            if mode == "blocked" {
                std::fs::create_dir(&content_path).unwrap();
            }
            if mode == "corrupt-result" {
                std::fs::write(&content_path, b"[]").unwrap();
            }
            drop(hold);
            drop(account);
            drop(database);
            let database = open();
            let account =
                PersistentCapacityAccount::new(&database, PersistentCapacityPolicy::embedded());
            store.recover_abandoned_stages().unwrap();
            let (operations, pending) = account.reconstruct_target_publications(&target).unwrap();
            assert!(operations.is_empty());
            let pending = pending.unwrap();
            if mode == "blocked" || mode == "corrupt-result" {
                assert!(
                    account
                        .reconcile_maintenance_publication(&store, &target, pending.hold())
                        .is_err()
                );
                assert_eq!(account.pending_publications().unwrap(), 1);
                assert_eq!(account.usage().unwrap().committed_artifact_bytes, 2);
                if mode == "blocked" {
                    std::fs::remove_dir(&content_path).unwrap();
                } else {
                    std::fs::write(&content_path, b"{}").unwrap();
                }
            }
            let result =
                account.reconcile_maintenance_publication(&store, &target, pending.hold()).unwrap();
            let associated = mode == "associated" || mode == "corrupt-result";
            assert_eq!(
                result,
                if associated {
                    MaintenancePublicationRecovery::Completed
                } else {
                    MaintenancePublicationRecovery::Abandoned
                }
            );
            assert_eq!(account.pending_publications().unwrap(), 0);
            assert!(account.reconstruct_maintenance_publication(&target).unwrap().is_none());
            let retained = associated || mode == "shared";
            assert_eq!(
                account.usage().unwrap().committed_artifact_bytes,
                if retained { 2 } else { 0 }
            );
            assert_eq!(content_path.exists(), retained);
            assert_eq!(
                crate::maintenance_results::read(&database, &target, &document.identifier)
                    .unwrap()
                    .is_some(),
                associated
            );
        }
    }

    #[test]
    fn receipt_owned_preview_and_application_complete_independently_over_shared_content() {
        let limits = slingshot_domain::daemon_runtime_contract::DaemonRuntimeContract::embedded();
        let database = OperationDatabase::open_in_memory(crate::database::RequiredSettings {
            page_bytes: limits.limit("sqlite_page_bytes"),
            database_pages: limits.limit("maximum_sqlite_database_pages"),
            busy_timeout_milliseconds: limits.limit("database_busy_timeout_milliseconds"),
        })
        .unwrap();
        let root = tempfile::tempdir().unwrap();
        let store = crate::artifact_store::ArtifactStore::open(root.path()).unwrap();
        let target = "a".repeat(DIGEST_OCTETS * 2);
        let account =
            PersistentCapacityAccount::new(&database, PersistentCapacityPolicy::embedded());
        let manifest = crate::maintenance::preview(&database, &target, 2, 1).unwrap();
        let source = manifest.digest();
        let reservation = account.reserve_artifact(None, 2).unwrap();
        let stage = store
            .stage_maintenance_document(&target, MaintenanceResultKind::Preview, &source, b"{}")
            .unwrap();
        let preview_hold = account.retain_maintenance_publication(&stage, reservation, 1).unwrap();
        let preview = stage.publish().unwrap();
        crate::maintenance_results::record_current_preview(
            &database,
            &account,
            &target,
            &source,
            &preview.content_digest,
            preview.byte_length,
        )
        .unwrap();
        crate::maintenance::apply(&database, &manifest, 3).unwrap();
        // Receipt ownership is a valid successor to current-preview ownership;
        // completion must not force the retained preview back to current.
        account.complete_maintenance_publication(&store, &target, &preview_hold).unwrap();
        let stage = store
            .stage_maintenance_document(&target, MaintenanceResultKind::Application, &source, b"{}")
            .unwrap();
        let application_hold = account.retain_maintenance_publication(&stage, None, 4).unwrap();
        let application = stage.publish().unwrap();
        assert_ne!(preview.identifier, application.identifier);
        crate::maintenance_results::record_application_result(
            &database,
            &account,
            &target,
            &source,
            &application.content_digest,
            application.byte_length,
        )
        .unwrap();
        let path = root.path().join("content").join(&application.content_digest);
        // Same-length mutation must fail digest verification, without consuming
        // either the association or producer. Restore the fixture for retry.
        std::fs::write(&path, b"[]").unwrap();
        assert!(
            account.complete_maintenance_publication(&store, &target, &application_hold).is_err()
        );
        assert_eq!(account.pending_publications().unwrap(), 1);
        std::fs::write(&path, b"{}").unwrap();
        account.complete_maintenance_publication(&store, &target, &application_hold).unwrap();
        assert_eq!(account.pending_publications().unwrap(), 0);
        assert_eq!(account.usage().unwrap().committed_artifact_bytes, 2);
        for document in [preview, application] {
            let metadata =
                crate::maintenance_results::read(&database, &target, &document.identifier)
                    .unwrap()
                    .unwrap();
            assert_eq!(
                metadata.owner,
                crate::maintenance_results::RetentionOwner::ApplicationReceipt(source.clone())
            );
        }
    }

    #[test]
    fn classification_refuses_corrupt_maintenance_ownership_without_releasing_bytes() {
        let limits = slingshot_domain::daemon_runtime_contract::DaemonRuntimeContract::embedded();
        for fault in ["none", "source", "identifier", "timestamp", "empty", "oversize"] {
            let database = OperationDatabase::open_in_memory(crate::database::RequiredSettings {
                page_bytes: limits.limit("sqlite_page_bytes"),
                database_pages: limits.limit("maximum_sqlite_database_pages"),
                busy_timeout_milliseconds: limits.limit("database_busy_timeout_milliseconds"),
            })
            .unwrap();
            let target = "a".repeat(DIGEST_OCTETS * 2);
            let content = "b".repeat(DIGEST_OCTETS * 2);
            let source = "c".repeat(DIGEST_OCTETS * 2);
            let expected = MaintenanceResultIdentifier::derive(
                &digest(&target).unwrap(),
                MaintenanceResultKind::Application,
                &digest(&source).unwrap(),
                &digest(&content).unwrap(),
            );
            let length = match fault {
                "empty" => 0,
                "oversize" => limits.limit("maximum_canonical_structured_result_bytes") + 1,
                _ => 2,
            };
            let producer = uuid::Uuid::new_v4().to_string();
            database
                .connection()
                .execute(
                    statement("record one artifact's content, once per digest"),
                    rusqlite::params![i64::try_from(length).unwrap(), content, 1],
                )
                .unwrap();
            database
                .connection()
                .execute(
                    statement("retain one artifact publication across restart"),
                    rusqlite::params![
                        producer,
                        if fault == "identifier" { &target } else { expected.as_text() },
                        content,
                        if fault == "timestamp" { -1 } else { 1 }
                    ],
                )
                .unwrap();
            database
                .connection()
                .execute(
                    statement("bind a maintenance publication to its operation-free owner"),
                    rusqlite::params![
                        producer,
                        target,
                        "application",
                        if fault == "source" { &content } else { &source }
                    ],
                )
                .unwrap();
            let account =
                PersistentCapacityAccount::new(&database, PersistentCapacityPolicy::embedded());
            let result = account.reconstruct_target_publications(&target);
            if fault == "none" {
                let (operations, maintenance) = result.unwrap();
                assert!(operations.is_empty());
                assert_eq!(
                    maintenance.unwrap().document().kind,
                    MaintenanceResultKind::Application
                );
            } else {
                assert!(result.is_err(), "{fault}");
            }
            assert_eq!(account.pending_publications().unwrap(), 1);
            assert_eq!(account.usage().unwrap().committed_artifact_bytes, length);
        }
    }
}
