//! Opening one operation database, and refusing to open a wrong one.
//!
//! Every setting this connection needs is verified rather than assumed. A
//! `PRAGMA` that is set and not read back is a wish: SQLite silently ignores
//! several of them under conditions a caller cannot see, and a database running
//! with rollback journalling when the accounting assumed a write-ahead log
//! would produce files nobody counted.
//!
//! The order matters as much as the settings. Statement-journal spilling is
//! disabled before the library initializes, because afterwards the
//! configuration call is refused and the process would carry on with spilling
//! enabled. The build's own compile options are read back for the same reason:
//! a build without in-memory temporary storage cannot honour the no-spill
//! invariant however the pragmas are set.
//!
//! # Migrations
//!
//! Ordered, transactional, and one-way. A database whose schema is newer than
//! this binary is refused without being touched: the newer binary knows things
//! about those rows this one does not, and migrating them backwards would be
//! guessing.

use std::ffi::CStr;
use std::sync::OnceLock;

use rusqlite::{Connection, OpenFlags};

use crate::sqlite_statement_inventory::FORBIDDEN_CONSTRUCTS;

/// Migrations, in the order they apply.
///
/// Embedded rather than read from disk, so the schema a binary applies is the
/// one it was built with and cannot be swapped underneath it.
pub const MIGRATIONS: &[(u32, &str)] = &[
    (1, include_str!("../migrations/0001-operations.sql")),
    (2, include_str!("../migrations/0002-agent-jobs.sql")),
    (3, include_str!("../migrations/0003-execution-fence.sql")),
    (4, include_str!("../migrations/0004-artifact-reservations.sql")),
];

/// The one temporary-storage mode the reviewed SQLite build may report.
pub const REQUIRED_COMPILE_OPTION: &str = "TEMP_STORE=3";

/// The vendored library and source identity reviewed with this binary.
pub const REVIEWED_SQLITE_LIBRARY: &str = "rusqlite 0.40.2 / libsqlite3-sys 0.38.2 (bundled)";
/// The SQLite version number in that reviewed vendored source.
pub const REVIEWED_SQLITE_VERSION: i32 = 3_053_002;
/// The SQLite source identifier in that reviewed vendored source.
pub const REVIEWED_SQLITE_SOURCE_ID: &str =
    "2026-06-03 19:12:13 d6e03d8c777cfa2d35e3b60d8ec3e0187f3e9f99d8e2ee9cac695fd6fcdf1a24";

/// Reason a database could not be opened.
#[derive(Debug, thiserror::Error)]
pub enum DatabaseFailure {
    /// SQLite refused something.
    #[error("the database refused: {0}")]
    Refused(String),
    /// The process SQLite library differs from the reviewed library identity.
    #[error("the SQLite runtime is not the reviewed {0}")]
    RuntimeIdentityRefused(&'static str),
    /// The process SQLite library reports a compile option this daemon cannot work under.
    #[error("the SQLite runtime does not report the required {0}")]
    CompileOptionRefused(String),
    /// SQLite was initialized before the product could apply its global configuration.
    #[error("SQLite was initialized before the no-spill configuration could be applied: {0}")]
    InitializationRefused(String),
    /// A setting did not read back as it was set.
    #[error("the setting {name} read back as {observed} rather than {expected}")]
    SettingMismatch {
        /// Setting that disagreed.
        name: &'static str,
        /// What it should have been.
        expected: String,
        /// What it was.
        observed: String,
    },
    /// The schema is newer than this binary understands.
    #[error("the schema is at version {observed}, which is newer than {supported}")]
    SchemaTooNew {
        /// Version the database is at.
        observed: u32,
        /// Newest version this binary applies.
        supported: u32,
    },
    /// A statement outside the inventory was offered.
    #[error("a statement outside the closed inventory cannot run")]
    StatementNotInventoried,
}

/// The settings every connection must read back.
///
/// Held as data so the verification and the documentation cannot drift: the
/// list a reader sees is the list the code checks.
#[derive(Debug, Clone, Copy)]
pub struct RequiredSettings {
    /// Bytes one page occupies.
    pub page_bytes: u64,
    /// Pages the database may reach.
    pub database_pages: u64,
    /// Milliseconds a busy connection waits.
    pub busy_timeout_milliseconds: u64,
}

impl RequiredSettings {
    /// Returns the pragmas whose values come from the runtime contract.
    #[must_use]
    pub fn valued_pragmas(self) -> Vec<(&'static str, String)> {
        vec![
            ("page_size", self.page_bytes.to_string()),
            ("max_page_count", self.database_pages.to_string()),
            ("busy_timeout", self.busy_timeout_milliseconds.to_string()),
        ]
    }

    /// Returns the pragmas whose values are the same everywhere.
    #[must_use]
    pub fn fixed_pragmas() -> Vec<(&'static str, &'static str)> {
        vec![
            ("temp_store", "2"),
            ("foreign_keys", "1"),
            ("journal_mode", "wal"),
            ("synchronous", "2"),
        ]
    }
}

/// One opened operation database.
#[derive(Debug)]
pub struct OperationDatabase {
    /// The connection this daemon owns.
    connection: Connection,
}

impl OperationDatabase {
    /// Opens the database at `path` and brings it to the current schema.
    ///
    /// # Errors
    ///
    /// Returns [`DatabaseFailure`] naming the first thing that was wrong, and
    /// changes nothing once anything is.
    pub fn open(
        path: &std::path::Path,
        settings: RequiredSettings,
    ) -> Result<Self, DatabaseFailure> {
        initialize_sqlite()?;
        let inspected = inspect_existing_schema(path)?;
        let connection = Connection::open(path).map_err(refused)?;
        if let Some(inspected) = inspected
            && file_snapshot(path)? != inspected
        {
            return Err(DatabaseFailure::Refused(
                "the database changed between inspection and reopen".to_owned(),
            ));
        }
        let database = Self { connection };
        database.require_compile_options()?;
        database.apply_and_verify(settings)?;
        database.migrate()?;
        database.reconcile_abandoned_artifact_reservations()?;
        Ok(database)
    }

    /// Opens a database held in memory, for a test that needs no file.
    ///
    /// # Errors
    ///
    /// Returns [`DatabaseFailure`] on the same grounds as [`Self::open`],
    /// except that an in-memory database keeps its own journalling mode.
    pub fn open_in_memory(settings: RequiredSettings) -> Result<Self, DatabaseFailure> {
        initialize_sqlite()?;
        let connection = Connection::open_in_memory().map_err(refused)?;
        let database = Self { connection };
        database.require_compile_options()?;
        database.apply_valued(settings)?;
        database.set_pragma("foreign_keys", "1")?;
        database.migrate()?;
        Ok(database)
    }

    /// Returns the connection this daemon owns.
    ///
    /// Borrowed rather than handed over: a connection that escaped this crate
    /// would be one nobody could hold to the inventory.
    #[must_use]
    pub fn connection(&self) -> &Connection {
        &self.connection
    }

    /// Returns the schema version this database is at.
    ///
    /// # Errors
    ///
    /// Returns [`DatabaseFailure::Refused`] when the value cannot be read.
    pub fn schema_version(&self) -> Result<u32, DatabaseFailure> {
        self.connection
            .query_row("PRAGMA user_version", [], |row| row.get::<_, i64>(0))
            .map(|version| u32::try_from(version).unwrap_or_default())
            .map_err(refused)
    }

    /// Requires the opened connection to report the exact reviewed compile option.
    ///
    /// # Errors
    ///
    /// This is defense in depth for a connection supplied by the product
    /// factory: global initialization checks the same option before opening it.
    pub fn require_compile_options(&self) -> Result<(), DatabaseFailure> {
        let mut statement = self.connection.prepare("PRAGMA compile_options").map_err(refused)?;
        let reported: Vec<String> = statement
            .query_map([], |row| row.get::<_, String>(0))
            .map_err(refused)?
            .collect::<Result<Vec<String>, _>>()
            .map_err(refused)?;
        if !reported.iter().any(|option| option == REQUIRED_COMPILE_OPTION) {
            return Err(DatabaseFailure::CompileOptionRefused(REQUIRED_COMPILE_OPTION.to_owned()));
        }
        Ok(())
    }

    /// Applies every setting and reads each one back.
    fn apply_and_verify(&self, settings: RequiredSettings) -> Result<(), DatabaseFailure> {
        self.apply_valued(settings)?;
        for (name, expected) in RequiredSettings::fixed_pragmas() {
            self.set_pragma(name, expected)?;
        }
        Ok(())
    }

    /// Applies the settings whose values come from the contract.
    fn apply_valued(&self, settings: RequiredSettings) -> Result<(), DatabaseFailure> {
        for (name, expected) in settings.valued_pragmas() {
            self.set_pragma(name, &expected)?;
        }
        Ok(())
    }

    /// Sets one pragma and requires it to read back as it was set.
    fn set_pragma(&self, name: &'static str, expected: &str) -> Result<(), DatabaseFailure> {
        self.connection.execute_batch(&format!("PRAGMA {name} = {expected}")).map_err(refused)?;
        let observed: String = self
            .connection
            .query_row(&format!("PRAGMA {name}"), [], |row| row.get::<_, rusqlite::types::Value>(0))
            .map(render_value)
            .map_err(refused)?;
        if observed.eq_ignore_ascii_case(expected) {
            Ok(())
        } else {
            Err(DatabaseFailure::SettingMismatch { name, expected: expected.to_owned(), observed })
        }
    }

    /// Applies every migration this database has not had.
    ///
    /// Each is one transaction, so an interrupted migration leaves the database
    /// at the version before it rather than partway through it.
    fn migrate(&self) -> Result<(), DatabaseFailure> {
        let supported = MIGRATIONS.iter().map(|(version, _)| *version).max().unwrap_or_default();
        let observed = self.schema_version()?;
        if observed > supported {
            return Err(DatabaseFailure::SchemaTooNew { observed, supported });
        }
        for (version, statements) in MIGRATIONS {
            if *version <= observed {
                continue;
            }
            self.connection
                .execute_batch(&format!(
                    "BEGIN IMMEDIATE; {statements} PRAGMA user_version = {version}; COMMIT;"
                ))
                .map_err(refused)?;
        }
        Ok(())
    }

    /// Releases reservations left by a process that did not finish an installation.
    fn reconcile_abandoned_artifact_reservations(&self) -> Result<(), DatabaseFailure> {
        let statement = crate::sqlite_statement_inventory::STATEMENTS
            .iter()
            .find(|held| held.purpose == "reconcile abandoned artifact reservations at startup")
            .map(|held| held.text)
            .unwrap_or_else(|| panic!("the inventory names startup reservation reconciliation"));
        self.connection.execute(statement, []).map_err(refused)?;
        Ok(())
    }
}

/// Establishes the only SQLite process configuration this product permits.
///
/// `sqlite3_config` deliberately runs before `sqlite3_initialize`: SQLite
/// refuses it after even one accidental connection.  `OnceLock` preserves the
/// first result, including a refusal, so a caller can never turn a partly
/// configured process into a usable one by retrying.
fn initialize_sqlite() -> Result<(), DatabaseFailure> {
    static INITIALIZATION: OnceLock<Result<(), String>> = OnceLock::new();
    INITIALIZATION
        .get_or_init(|| configure_sqlite().map_err(|failure| failure.to_string()))
        .as_ref()
        .map_err(|failure| DatabaseFailure::InitializationRefused(failure.clone()))
        .copied()
}

/// Checks the linked source and configures it before SQLite initializes.
#[allow(unsafe_code)]
fn configure_sqlite() -> Result<(), DatabaseFailure> {
    // These FFI calls cannot initialize SQLite.  Checking them first ensures a
    // different library is refused before a connection factory is reachable.
    let version = unsafe { rusqlite::ffi::sqlite3_libversion_number() };
    if version != REVIEWED_SQLITE_VERSION {
        return Err(DatabaseFailure::RuntimeIdentityRefused(REVIEWED_SQLITE_LIBRARY));
    }
    let source = unsafe { CStr::from_ptr(rusqlite::ffi::sqlite3_sourceid()) };
    if source.to_str().ok() != Some(REVIEWED_SQLITE_SOURCE_ID) {
        return Err(DatabaseFailure::RuntimeIdentityRefused(REVIEWED_SQLITE_LIBRARY));
    }
    let temp_store = c"TEMP_STORE=3";
    if unsafe { rusqlite::ffi::sqlite3_compileoption_used(temp_store.as_ptr()) } == 0 {
        return Err(DatabaseFailure::CompileOptionRefused(REQUIRED_COMPILE_OPTION.to_owned()));
    }

    let configured = unsafe {
        rusqlite::ffi::sqlite3_config(rusqlite::ffi::SQLITE_CONFIG_STMTJRNL_SPILL, -1_i32)
    };
    if configured != rusqlite::ffi::SQLITE_OK {
        return Err(DatabaseFailure::InitializationRefused(format!(
            "sqlite3_config(SQLITE_CONFIG_STMTJRNL_SPILL, -1) returned {configured}"
        )));
    }
    let initialized = unsafe { rusqlite::ffi::sqlite3_initialize() };
    if initialized != rusqlite::ffi::SQLITE_OK {
        return Err(DatabaseFailure::InitializationRefused(format!(
            "sqlite3_initialize returned {initialized}"
        )));
    }
    Ok(())
}

/// Refuses a future schema through a read-only connection before any mutable open.
fn inspect_existing_schema(
    path: &std::path::Path,
) -> Result<Option<FileSnapshot>, DatabaseFailure> {
    if !path.exists() {
        return Ok(None);
    }
    let snapshot = file_snapshot(path)?;
    let connection =
        Connection::open_with_flags(path, OpenFlags::SQLITE_OPEN_READ_ONLY).map_err(refused)?;
    let observed: i64 =
        connection.query_row("PRAGMA user_version", [], |row| row.get(0)).map_err(refused)?;
    let observed = u32::try_from(observed).unwrap_or_default();
    let supported = MIGRATIONS.iter().map(|(version, _)| *version).max().unwrap_or_default();
    if observed > supported {
        return Err(DatabaseFailure::SchemaTooNew { observed, supported });
    }
    Ok(Some(snapshot))
}

/// The stable identity of one inspected database pathname.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct FileSnapshot(u64, u64);

#[cfg(unix)]
fn file_snapshot(path: &std::path::Path) -> Result<FileSnapshot, DatabaseFailure> {
    use std::os::unix::fs::MetadataExt as _;
    let metadata = std::fs::symlink_metadata(path)
        .map_err(|failure| DatabaseFailure::Refused(failure.to_string()))?;
    if !metadata.is_file() || metadata.nlink() != 1 {
        return Err(DatabaseFailure::Refused(
            "the database is not one private regular file".to_owned(),
        ));
    }
    Ok(FileSnapshot(metadata.dev(), metadata.ino()))
}

#[cfg(not(unix))]
fn file_snapshot(path: &std::path::Path) -> Result<FileSnapshot, DatabaseFailure> {
    let metadata =
        std::fs::metadata(path).map_err(|failure| DatabaseFailure::Refused(failure.to_string()))?;
    if !metadata.is_file() {
        return Err(DatabaseFailure::Refused("the database is not one regular file".to_owned()));
    }
    Ok(FileSnapshot(metadata.len(), 0))
}

/// Returns whether `text` contains a construct this crate may never run.
///
/// Checked as text because the point is to catch it before it is prepared:
/// once a statement is running it has already done whatever it was going to.
#[must_use]
pub fn uses_forbidden_construct(text: &str) -> bool {
    let upper = text.to_uppercase();
    FORBIDDEN_CONSTRUCTS.iter().any(|construct| upper.contains(construct))
}

/// Returns one SQLite value as the text a pragma reads back as.
fn render_value(value: rusqlite::types::Value) -> String {
    match value {
        rusqlite::types::Value::Integer(number) => number.to_string(),
        rusqlite::types::Value::Real(number) => number.to_string(),
        rusqlite::types::Value::Text(text) => text,
        rusqlite::types::Value::Blob(_) | rusqlite::types::Value::Null => String::new(),
    }
}

/// Returns one SQLite refusal as this crate's failure.
fn refused(failure: rusqlite::Error) -> DatabaseFailure {
    DatabaseFailure::Refused(failure.to_string())
}
