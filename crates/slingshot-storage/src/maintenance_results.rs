//! Operation-free maintenance metadata, validated in one repository observation.

use crate::database::OperationDatabase;

mod write;
mod cleanup;
pub use cleanup::cleanup_superseded_preview;
use rusqlite::OptionalExtension as _;
use slingshot_domain::daemon_runtime_contract::{
    DIGEST_OCTETS, DaemonRuntimeContract, MaintenanceResultIdentifier, MaintenanceResultKind,
};
pub use write::{
    ApplicationWrite, PreviewWrite, WriteFailure, record_application_result, record_current_preview,
};
pub(crate) use write::{current_preview, retain_applied_preview};

/// Returns the bounded operation-free result IDs retained by one receipt.
pub fn result_identifiers_for_receipt(
    database: &OperationDatabase,
    target: &str,
    receipt: &str,
) -> Result<Vec<String>, ReadFailure> {
    let statement = crate::sqlite_statement_inventory::STATEMENTS
        .iter()
        .find(|statement| {
            statement.purpose == "read maintenance result identifiers retained by one receipt"
        })
        .expect("receipt result lookup is inventoried");
    let mut query = database.connection().prepare(statement.text).map_err(ReadFailure::Database)?;
    let rows = query
        .query_map(rusqlite::params![target, receipt], |row| row.get::<_, String>(0))
        .map_err(ReadFailure::Database)?;
    rows.collect::<Result<Vec<_>, _>>().map_err(ReadFailure::Database)
}

/// The durable owner keeping a maintenance result reachable.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RetentionOwner {
    /// The target's single current unapplied preview.
    CurrentPreview,
    /// A target-qualified retained maintenance application receipt.
    ApplicationReceipt(String),
}

/// Validated access metadata with no operation identifier or filesystem path.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MaintenanceResultMetadata {
    /// Revision of the retained association.
    pub association_revision: u64,
    /// Exact canonical-document length.
    pub byte_length: u64,
    /// Canonical document digest.
    pub content_digest: String,
    /// Preview or application document.
    pub kind: MaintenanceResultKind,
    /// Deterministically derived operation-free identifier.
    pub identifier: MaintenanceResultIdentifier,
    /// Retaining owner, validated in the same SQLite observation.
    pub owner: RetentionOwner,
    /// Reviewed manifest digest.
    pub reviewed_source_digest: String,
}

/// A metadata read failed; no failure rewrites an association or receipt.
#[derive(Debug, thiserror::Error)]
pub enum ReadFailure {
    /// Address or retained metadata violates its identity/ownership contract.
    #[error("maintenance result metadata failed validation")]
    Invalid,
    /// SQLite could not complete the observation.
    #[error("maintenance result metadata could not be read")]
    Database(#[source] rusqlite::Error),
}

struct Row {
    revision: i64,
    length: i64,
    digest: String,
    kind: String,
    media_type: String,
    owner: Option<String>,
    source: String,
    current: i64,
    blob_length: Option<i64>,
    receipt: Option<String>,
}

/// Reads one target-qualified result, joining its blob and receipt owner in the
/// same snapshot. Missing is distinct from corrupt or dangling retained state.
///
/// # Errors
/// Returns [`ReadFailure`] without exposing paths or changing retained history.
pub fn read(
    database: &OperationDatabase,
    target: &str,
    identifier: &MaintenanceResultIdentifier,
) -> Result<Option<MaintenanceResultMetadata>, ReadFailure> {
    let target = digest(target)?;
    let statement = crate::sqlite_statement_inventory::STATEMENTS
        .iter()
        .find(|statement| {
            statement.purpose == "read one maintenance result by target and identifier alone"
        })
        .expect("the maintenance result read is inventoried");
    let mut query = database.connection().prepare(statement.text).map_err(ReadFailure::Database)?;
    let row = query
        .query_row(rusqlite::params![hex::encode(target), identifier.as_text()], |row| {
            Ok(Row {
                revision: row.get(0)?,
                length: row.get(1)?,
                digest: row.get(2)?,
                kind: row.get(3)?,
                media_type: row.get(4)?,
                owner: row.get(5)?,
                source: row.get(6)?,
                current: row.get(7)?,
                blob_length: row.get(8)?,
                receipt: row.get(9)?,
            })
        })
        .optional()
        .map_err(ReadFailure::Database)?;
    row.map(|row| validate(target, identifier, row)).transpose()
}

fn digest(text: &str) -> Result<[u8; DIGEST_OCTETS], ReadFailure> {
    if text.len() != DIGEST_OCTETS * 2
        || !text.bytes().all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    {
        return Err(ReadFailure::Invalid);
    }
    let mut bytes = [0; DIGEST_OCTETS];
    hex::decode_to_slice(text, &mut bytes).map_err(|_| ReadFailure::Invalid)?;
    Ok(bytes)
}

fn validate(
    target: [u8; DIGEST_OCTETS],
    identifier: &MaintenanceResultIdentifier,
    row: Row,
) -> Result<MaintenanceResultMetadata, ReadFailure> {
    let kind = match row.kind.as_str() {
        "preview" => MaintenanceResultKind::Preview,
        "application" => MaintenanceResultKind::Application,
        _ => return Err(ReadFailure::Invalid),
    };
    if row.revision < 1
        || row.length < 1
        || row.blob_length != Some(row.length)
        || row.length as u64
            > DaemonRuntimeContract::embedded().formula("maximum_individual_artifact_bytes")
        || row.media_type != "application/json"
        || MaintenanceResultIdentifier::derive(
            &target,
            kind,
            &digest(&row.source)?,
            &digest(&row.digest)?,
        ) != *identifier
    {
        return Err(ReadFailure::Invalid);
    }
    let owner = match (row.current, row.owner, row.receipt) {
        (1, None, None) if kind == MaintenanceResultKind::Preview => RetentionOwner::CurrentPreview,
        (0, Some(owner), Some(receipt)) if owner == receipt && owner == row.source => {
            RetentionOwner::ApplicationReceipt(owner)
        }
        _ => return Err(ReadFailure::Invalid),
    };
    Ok(MaintenanceResultMetadata {
        association_revision: row.revision as u64,
        byte_length: row.length as u64,
        content_digest: row.digest,
        kind,
        identifier: identifier.clone(),
        owner,
        reviewed_source_digest: row.source,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn row() -> Row {
        Row {
            revision: 1,
            length: 2,
            digest: "b".repeat(DIGEST_OCTETS * 2),
            kind: "preview".to_owned(),
            media_type: "application/json".to_owned(),
            owner: None,
            source: "c".repeat(DIGEST_OCTETS * 2),
            current: 1,
            blob_length: Some(2),
            receipt: None,
        }
    }

    fn identifier(kind: MaintenanceResultKind) -> MaintenanceResultIdentifier {
        MaintenanceResultIdentifier::derive(
            &[0xaa; DIGEST_OCTETS],
            kind,
            &[0xcc; DIGEST_OCTETS],
            &[0xbb; DIGEST_OCTETS],
        )
    }

    #[test]
    fn current_and_receipt_owned_results_validate_their_complete_identity() {
        let target = [0xaa; DIGEST_OCTETS];
        let metadata =
            validate(target, &identifier(MaintenanceResultKind::Preview), row()).unwrap();
        assert_eq!(metadata.owner, RetentionOwner::CurrentPreview);
        for kind in [MaintenanceResultKind::Preview, MaintenanceResultKind::Application] {
            let mut row = row();
            row.kind =
                if kind == MaintenanceResultKind::Preview { "preview" } else { "application" }
                    .to_owned();
            row.current = 0;
            row.owner = Some(row.source.clone());
            row.receipt = row.owner.clone();
            let metadata = validate(target, &identifier(kind), row).unwrap();
            assert!(matches!(metadata.owner, RetentionOwner::ApplicationReceipt(_)));
            assert_eq!(metadata.kind, kind);
        }
    }

    #[test]
    fn corrupt_binding_lengths_and_owners_fail_closed() {
        let target = [0xaa; DIGEST_OCTETS];
        let id = identifier(MaintenanceResultKind::Preview);
        assert!(validate([0xab; DIGEST_OCTETS], &id, row()).is_err());
        let mutations: &[fn(&mut Row)] = &[
            |row| row.revision = 0,
            |row| row.length = -1,
            |row| row.length = 0,
            |row| row.blob_length = None,
            |row| row.blob_length = Some(3),
            |row| row.digest = row.digest.to_uppercase(),
            |row| row.source = "not-a-digest".to_owned(),
            |row| row.kind = "unknown".to_owned(),
            |row| row.media_type = "text/plain".to_owned(),
            |row| row.current = 0,
            |row| row.current = 2,
            |row| row.owner = Some(row.source.clone()),
            |row| {
                row.current = 0;
                row.owner = Some(row.source.clone());
            },
            |row| {
                row.current = 0;
                row.owner = Some("d".repeat(DIGEST_OCTETS * 2));
                row.receipt = row.owner.clone();
            },
            |row| {
                row.length = i64::MAX;
                row.blob_length = Some(row.length);
            },
        ];
        for mutation in mutations {
            let mut row = row();
            mutation(&mut row);
            assert!(matches!(validate(target, &id, row), Err(ReadFailure::Invalid)));
        }
        let mut application = row();
        application.kind = "application".to_owned();
        assert!(
            validate(target, &identifier(MaintenanceResultKind::Application), application).is_err()
        );
    }

    #[test]
    fn actual_repository_distinguishes_absent_result_from_invalid_address() {
        let contract = DaemonRuntimeContract::embedded();
        let database = OperationDatabase::open_in_memory(crate::database::RequiredSettings {
            page_bytes: contract.limit("sqlite_page_bytes"),
            database_pages: contract.limit("maximum_sqlite_database_pages"),
            busy_timeout_milliseconds: contract.limit("database_busy_timeout_milliseconds"),
        })
        .unwrap();
        let id = identifier(MaintenanceResultKind::Preview);
        assert!(read(&database, &"a".repeat(DIGEST_OCTETS * 2), &id).unwrap().is_none());
        for invalid in ["", "foreign-readable-principal", &"A".repeat(DIGEST_OCTETS * 2)] {
            assert!(matches!(read(&database, invalid, &id), Err(ReadFailure::Invalid)));
        }
    }
}
