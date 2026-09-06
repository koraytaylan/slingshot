//! Restartable cleanup intent for the single outstanding preview supersession.

use super::*;

fn sql(purpose: &str) -> &'static str {
    crate::sqlite_statement_inventory::STATEMENTS
        .iter()
        .find(|statement| statement.purpose == purpose)
        .expect("preview cleanup is inventoried")
        .text
}

pub(super) fn pending(
    database: &OperationDatabase,
    target: &str,
) -> Result<Vec<(String, String)>, ReadFailure> {
    let mut statement = database
        .connection()
        .prepare(sql("read pending superseded preview cleanup"))
        .map_err(ReadFailure::Database)?;
    let rows = statement
        .query_map([target], |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?)))
        .map_err(ReadFailure::Database)?
        .collect::<Result<Vec<_>, _>>()
        .map_err(ReadFailure::Database)?;
    if rows.len() > 1 {
        return Err(ReadFailure::Invalid);
    }
    for (owner, content) in &rows {
        // Prefix separates this operation-free cleanup owner from canonical
        // digest application-receipt identifiers in the shared intent table.
        MaintenanceResultIdentifier::parse(
            owner.strip_prefix("preview:").ok_or(ReadFailure::Invalid)?,
        )
        .map_err(|_| ReadFailure::Invalid)?;
        digest(content)?;
    }
    Ok(rows)
}

/// Completes an already-approved supersession while namespace ownership is held.
/// File removal precedes accounting release; a crash between them is retryable.
/// Durable sharing retains bytes; publication-only holds keep the intent pending.
///
/// # Errors
/// Refuses malformed journal state or filesystem/database failures, retaining
/// retryable intent. This never selects a new preview or removes an association.
pub fn cleanup_superseded_preview(
    database: &OperationDatabase,
    store: &crate::artifact_store::ArtifactStore,
    target: &str,
) -> Result<u64, crate::maintenance::MaintenanceFailure> {
    let transaction = rusqlite::Transaction::new_unchecked(
        database.connection(),
        rusqlite::TransactionBehavior::Immediate,
    )?;
    let rows = pending(database, target)?;
    let mut completed = 0;
    for (owner, content) in rows {
        let references: i64 = transaction.query_row(
            sql("count what still references one artifact's content"),
            rusqlite::params![content, content, content],
            |row| row.get(0),
        )?;
        if references == 0 {
            store.remove_unreferenced_content(&content)?;
            transaction.execute(
                sql("remove one artifact's content, once nothing references it"),
                [&content],
            )?;
        } else {
            let durable: i64 = transaction.query_row(
                sql("count durable associations retaining shared artifact content"),
                rusqlite::params![content, content],
                |row| row.get(0),
            )?;
            if durable == 0 {
                continue;
            }
        }
        transaction.execute(
            sql("remove one completed maintenance artifact cleanup item"),
            rusqlite::params![target, owner, content],
        )?;
        completed += 1;
    }
    transaction.commit()?;
    Ok(completed)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::persistent_capacity::PersistentCapacityAccount;
    use slingshot_domain::persistent_capacity::PersistentCapacityPolicy;
    use std::io::Write as _;

    #[test]
    fn superseded_cleanup_retries_file_failure_and_publication_holds_without_losing_intent() {
        for mode in ["file", "already-unlinked", "blocked", "publication"] {
            let root = tempfile::tempdir().unwrap();
            let limits = DaemonRuntimeContract::embedded();
            let database = OperationDatabase::open_in_memory(crate::database::RequiredSettings {
                page_bytes: limits.limit("sqlite_page_bytes"),
                database_pages: limits.limit("maximum_sqlite_database_pages"),
                busy_timeout_milliseconds: limits.limit("database_busy_timeout_milliseconds"),
            })
            .unwrap();
            let store = crate::artifact_store::ArtifactStore::open(root.path()).unwrap();
            let target = "a".repeat(DIGEST_OCTETS * 2);
            let old = "b".repeat(DIGEST_OCTETS * 2);
            let new = "c".repeat(DIGEST_OCTETS * 2);
            let source = "d".repeat(DIGEST_OCTETS * 2);
            for content in [&old, &new] {
                database
                    .connection()
                    .execute(
                        sql("record one artifact's content, once per digest"),
                        rusqlite::params![2, content, 1],
                    )
                    .unwrap();
            }
            let account =
                PersistentCapacityAccount::new(&database, PersistentCapacityPolicy::embedded());
            record_current_preview(&database, &account, &target, &source, &old, 2).unwrap();
            let current = record_current_preview(&database, &account, &target, &source, &new, 2)
                .unwrap()
                .current;
            let path = root.path().join("content").join(&old);
            if mode == "blocked" {
                std::fs::create_dir(&path).unwrap();
            } else if mode != "already-unlinked" {
                let mut options = std::fs::OpenOptions::new();
                options.create_new(true).write(true);
                #[cfg(unix)]
                {
                    use std::os::unix::fs::OpenOptionsExt as _;
                    options.mode(0o600);
                }
                options.open(&path).unwrap().write_all(b"{}").unwrap();
            }
            if mode == "publication" {
                database
                    .connection()
                    .execute(
                        sql("retain one artifact publication across restart"),
                        rusqlite::params!["publication", "artifact", old, 1],
                    )
                    .unwrap();
                assert_eq!(cleanup_superseded_preview(&database, &store, &target).unwrap(), 0);
                assert_eq!(pending(&database, &target).unwrap().len(), 1);
                assert!(path.exists());
                database
                    .connection()
                    .execute(
                        sql("consume one completed artifact publication"),
                        rusqlite::params!["publication", "artifact", old],
                    )
                    .unwrap();
            }
            if mode == "blocked" {
                assert!(cleanup_superseded_preview(&database, &store, &target).is_err());
                assert_eq!(pending(&database, &target).unwrap().len(), 1);
                std::fs::remove_dir(&path).unwrap();
            }
            assert_eq!(cleanup_superseded_preview(&database, &store, "another-target").unwrap(), 0);
            assert_eq!(cleanup_superseded_preview(&database, &store, &target).unwrap(), 1);
            assert!(pending(&database, &target).unwrap().is_empty());
            assert!(!path.exists());
            let length: Option<i64> = database
                .connection()
                .query_row(sql("read one artifact blob's recorded length"), [&old], |row| {
                    row.get(0)
                })
                .optional()
                .unwrap();
            assert_eq!(length, None);
            assert_eq!(read(&database, &target, &current.identifier).unwrap(), Some(current));
        }
    }
}
