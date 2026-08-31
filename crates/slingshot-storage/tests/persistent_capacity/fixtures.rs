//! The values and helpers both halves of this suite are built from.

use serde_json::Value;
use slingshot_domain::persistent_capacity::PersistentCapacityPolicy;
use slingshot_storage::database::{OperationDatabase, RequiredSettings};
use slingshot_storage::persistent_capacity::PersistentCapacityAccount;

/// Arithmetic vectors this suite reads.
pub const FORMULAS: &str = include_str!("../fixtures/persistent-capacity/formulas.jsonl");

/// Bytes one page occupies, from the runtime contract.
pub const PAGE_BYTES: u64 = 4096;

/// Pages the database may reach, from the runtime contract.
pub const DATABASE_PAGES: u64 = 262_144;

/// Milliseconds a busy connection waits, from the runtime contract.
pub const BUSY_TIMEOUT: u64 = 5000;

/// Two-character pairs in a sixty-four-character hexadecimal value.
pub const DIGEST_PAIRS: usize = 32;

/// One instant, for a test that does not care which.
pub const NOW: u64 = 1_700_000_000_000;

/// Returns one row's string member.
pub fn text<'row>(row: &'row Value, member: &str) -> &'row str {
    row[member].as_str().unwrap_or_else(|| panic!("{member} is a string in {row}"))
}

/// Returns every row of one fixture.
pub fn rows(fixture: &str) -> Vec<Value> {
    fixture
        .lines()
        .map(|line| serde_json::from_str(line).expect("every fixture line is one object"))
        .collect()
}

/// Returns the settings every connection is held to.
pub fn settings() -> RequiredSettings {
    RequiredSettings {
        page_bytes: PAGE_BYTES,
        database_pages: DATABASE_PAGES,
        busy_timeout_milliseconds: BUSY_TIMEOUT,
    }
}

/// Returns one migrated database held in memory.
pub fn database() -> OperationDatabase {
    OperationDatabase::open_in_memory(settings()).expect("a database")
}

/// Returns the accounting for `database`, under a policy of the caller's choosing.
pub fn account(
    database: &OperationDatabase,
    policy: PersistentCapacityPolicy,
) -> PersistentCapacityAccount<'_> {
    PersistentCapacityAccount::new(database, policy)
}

/// Returns the digest one principal's author target has.
pub fn partition(principal: &str) -> String {
    principal.repeat(DIGEST_PAIRS)
}

/// The partition the accounting fixtures use.
pub const FIRST_PRINCIPAL: &str = "1d";
