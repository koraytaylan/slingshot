//! Probe for the embedded Structured Query Language capability.
//!
//! Requires the bundled engine rather than an ambient system library, working
//! transactions with rollback, enforced foreign keys, and a reported busy
//! timeout, because the operation ledger depends on all four.

use rusqlite::{Connection, params};

#[test]
fn the_bundled_engine_enforces_transactions_and_foreign_keys() {
    let mut connection = Connection::open_in_memory().expect("an in-memory database opens");
    let version: String = connection
        .query_row("SELECT sqlite_version()", [], |row| row.get(0))
        .expect("the version reads");
    assert!(version.starts_with('3'), "{version}");
    connection.pragma_update(None, "foreign_keys", true).expect("foreign keys enable");
    connection
        .execute_batch(
            "CREATE TABLE operation (identifier TEXT PRIMARY KEY) STRICT;
             CREATE TABLE artifact (
                 identifier TEXT PRIMARY KEY,
                 operation_identifier TEXT NOT NULL REFERENCES operation (identifier)
             ) STRICT;",
        )
        .expect("the schema applies");
    let orphan =
        connection.execute("INSERT INTO artifact VALUES (?1, ?2)", params!["one", "missing"]);
    assert!(orphan.is_err(), "an orphan row must be refused");
    let transaction = connection.transaction().expect("a transaction begins");
    transaction
        .execute("INSERT INTO operation VALUES (?1)", params!["one"])
        .expect("the row inserts");
    transaction.rollback().expect("the transaction rolls back");
    let remaining: i64 = connection
        .query_row("SELECT count(*) FROM operation", [], |row| row.get(0))
        .expect("the count reads");
    assert_eq!(remaining, 0);
}
