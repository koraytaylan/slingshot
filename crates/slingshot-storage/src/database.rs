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
use std::fs::File;
use std::sync::OnceLock;

use rusqlite::{Connection, OpenFlags, OptionalExtension};
use slingshot_domain::daemon_runtime_contract::DaemonRuntimeContract;
use slingshot_domain::installation::InstallationIdentifier;

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
    (5, include_str!("../migrations/0005-recovery-receipt-operation-key.sql")),
    (6, include_str!("../migrations/0006-successful-result-atomicity.sql")),
    (7, include_str!("../migrations/0007-subscription-event-generation.sql")),
    (8, include_str!("../migrations/0008-maintenance-receipt-count.sql")),
    (9, include_str!("../migrations/0009-maintenance-cleanup-work.sql")),
    (10, include_str!("../migrations/0010-artifact-publication.sql")),
    (11, include_str!("../migrations/0011-artifact-acquisition-anchor.sql")),
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
    /// The pinned database directory contains an unexplained object or exceeds its byte budget.
    #[error("the SQLite physical inventory is refused: {0}")]
    PhysicalInventoryRefused(String),
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

/// The selected installation and execution partition checked before startup recovery.
#[derive(Debug, Clone, Copy)]
pub struct StartupDatabaseBinding<'binding> {
    /// Identity from the locked installation ledger.
    pub installation: &'binding InstallationIdentifier,
    /// Target derived from the immutable selected environment.
    pub target: &'binding str,
    /// Revision derived from the immutable selected environment.
    pub revision: &'binding str,
    /// Runtime contract embedded in this daemon build.
    pub runtime_contract: &'binding str,
}

/// One opened operation database.
#[derive(Debug)]
pub struct OperationDatabase {
    /// The connection this daemon owns.
    connection: Connection,
    /// The verified directory whose descriptor keeps SQLite's object paths pinned.
    _state_root: Option<File>,
    /// The named SQLite objects and physical byte limit for this file-backed database.
    physical_inventory: Option<PhysicalInventory>,
    /// The main-file identity captured when this connection was opened.
    opened_file: Option<FileSnapshot>,
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
        Self::open_with_startup_recovery(path, settings, true, None)
    }

    /// Opens another connection for an already-started, exclusively owned
    /// daemon. Preserves live artifact reservations and refuses missing or
    /// outdated schemas rather than running startup recovery or migrations.
    /// The caller must retain the daemon's ownership lock for the namespace.
    ///
    /// # Errors
    ///
    /// Returns [`DatabaseFailure`] for an uninitialized database or any of the
    /// same path, physical-budget, settings and authorizer failures as `open`.
    pub fn open_live(
        path: &std::path::Path,
        settings: RequiredSettings,
    ) -> Result<Self, DatabaseFailure> {
        Self::open_with_startup_recovery(path, settings, false, None)
    }

    /// Reopens a ledger-registered database only after a read-only identity and
    /// unfinished-partition audit. Refusal precedes migration and artifact recovery.
    /// The caller must retain installation and namespace ownership throughout.
    pub fn reopen_bound(
        path: &std::path::Path,
        settings: RequiredSettings,
        binding: StartupDatabaseBinding<'_>,
    ) -> Result<Self, DatabaseFailure> {
        Self::open_with_startup_recovery(path, settings, true, Some(binding))
    }

    fn open_with_startup_recovery(
        path: &std::path::Path,
        settings: RequiredSettings,
        startup: bool,
        binding: Option<StartupDatabaseBinding<'_>>,
    ) -> Result<Self, DatabaseFailure> {
        initialize_sqlite()?;
        let (state_root, pinned_path) = PinnedDatabasePath::open(path)?;
        let physical_inventory = PhysicalInventory::new(pinned_path.clone())?;
        physical_inventory.require_within_budget()?;
        let inspected = inspect_existing_schema(&pinned_path, !startup)?;
        if let Some(binding) = binding {
            if inspected.is_none() {
                return Err(DatabaseFailure::Refused(
                    "the registered database is missing".to_owned(),
                ));
            }
            audit_existing_binding(&pinned_path, binding)?;
        }
        if !startup && inspected.is_none() {
            return Err(DatabaseFailure::Refused(
                "a live connection requires an initialized database".to_owned(),
            ));
        }
        let connection = Connection::open(&pinned_path).map_err(refused)?;
        if let Some(inspected) = inspected
            && file_snapshot(&pinned_path)? != inspected
        {
            return Err(DatabaseFailure::Refused(
                "the database changed between inspection and reopen".to_owned(),
            ));
        }
        let database = Self {
            connection,
            _state_root: Some(state_root),
            physical_inventory: Some(physical_inventory),
            opened_file: Some(file_snapshot(&pinned_path)?),
        };
        database.require_compile_options()?;
        database.apply_and_verify(settings)?;
        if startup {
            database.migrate()?;
            database.reconcile_abandoned_artifact_reservations()?;
        } else if database.schema_version()?
            != MIGRATIONS.iter().map(|(version, _)| *version).max().unwrap_or_default()
        {
            return Err(DatabaseFailure::Refused(
                "a live connection requires the current database schema".to_owned(),
            ));
        }
        database.reconcile_physical_inventory()?;
        database.install_authorizer()?;
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
        let database =
            Self { connection, _state_root: None, physical_inventory: None, opened_file: None };
        database.require_compile_options()?;
        database.apply_valued(settings)?;
        database.set_pragma("foreign_keys", "1")?;
        database.migrate()?;
        database.install_authorizer()?;
        Ok(database)
    }

    /// Whether both connections still name the same opened database object.
    /// No path or filesystem identity is exposed to callers. Distinct in-memory
    /// databases and platforms without stable file identity fail closed.
    #[must_use]
    pub fn shares_database_with(&self, other: &Self) -> bool {
        match (&self.physical_inventory, &other.physical_inventory) {
            (None, None) => core::ptr::eq(self, other),
            (Some(left), Some(right)) => {
                #[cfg(unix)]
                {
                    self.opened_file == other.opened_file
                        && file_snapshot(&left.main).ok() == self.opened_file
                        && file_snapshot(&right.main).ok() == other.opened_file
                }
                #[cfg(not(unix))]
                {
                    let _ = (left, right);
                    false
                }
            }
            _ => false,
        }
    }

    /// Returns the connection this daemon owns.
    ///
    /// Borrowed rather than handed over: a connection that escaped this crate
    /// would be one nobody could hold to the inventory.
    #[must_use]
    pub(crate) fn connection(&self) -> &Connection {
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

    /// Reads the database's installation identity without creating or repairing it.
    /// An absent identity is distinct from malformed or unreadable durable state.
    pub fn installation_identifier(
        &self,
    ) -> Result<Option<InstallationIdentifier>, DatabaseFailure> {
        let statement = crate::sqlite_statement_inventory::statement_text(
            "read this installation's identifier",
        );
        let value: Option<String> = self
            .connection
            .query_row(statement, [], |row| row.get(0))
            .optional()
            .map_err(refused)?;
        value
            .map(|value| {
                InstallationIdentifier::parse(&value).map_err(|_| {
                    DatabaseFailure::Refused(
                        "the database installation identifier is not canonical".to_owned(),
                    )
                })
            })
            .transpose()
    }

    /// Records the staged installation exactly once. Even an identical second
    /// insertion refuses; callers must read and verify existing identity, never
    /// use this method to adopt or repair an existing database.
    pub fn record_installation_identifier(
        &self,
        identifier: &InstallationIdentifier,
        recorded_at_unix_milliseconds: i64,
    ) -> Result<(), DatabaseFailure> {
        let statement = crate::sqlite_statement_inventory::statement_text(
            "record this installation's identifier once",
        );
        self.connection
            .execute(
                statement,
                rusqlite::params![identifier.as_text(), recorded_at_unix_milliseconds,],
            )
            .map_err(refused)?;
        Ok(())
    }

    /// Lists target/revision/runtime-contract partitions holding nonterminal work.
    ///
    /// # Errors
    ///
    /// Returns [`DatabaseFailure::Refused`] when the reviewed audit statement
    /// cannot be prepared or read.
    pub fn unfinished_partitions(&self) -> Result<Vec<(String, String, String)>, DatabaseFailure> {
        let statement = crate::sqlite_statement_inventory::statement_text(
            "list every partition holding work that has not ended",
        );
        let mut prepared = self.connection.prepare(statement).map_err(refused)?;
        prepared
            .query_map([], |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)))
            .map_err(refused)?
            .collect::<Result<Vec<(String, String, String)>, _>>()
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
        if !reports_only_required_temp_store(&reported) {
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
        let frames = DaemonRuntimeContract::embedded()
            .limit("maximum_sqlite_write_ahead_log_frames")
            .to_string();
        self.set_pragma("wal_autocheckpoint", &frames)?;
        let wal_bytes = DaemonRuntimeContract::embedded()
            .formula("maximum_sqlite_write_ahead_log_bytes")
            .to_string();
        self.set_pragma("journal_size_limit", &wal_bytes)?;
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

    /// Refuses a restart whose named SQLite objects no longer match the physical contract.
    fn reconcile_physical_inventory(&self) -> Result<(), DatabaseFailure> {
        match &self.physical_inventory {
            Some(inventory) => inventory.require_within_budget(),
            None => Ok(()),
        }
    }

    /// Installs the final runtime guard after the reviewed setup and migrations.
    ///
    /// The authorizer runs while SQLite prepares a statement, before it can
    /// create an attachment, a temporary object, or load a native extension.
    fn install_authorizer(&self) -> Result<(), DatabaseFailure> {
        use rusqlite::hooks::{AuthAction, AuthContext, Authorization};

        let physical_inventory = self.physical_inventory.clone();
        self.connection
            .authorizer(Some(move |context: AuthContext<'_>| match context.action {
                AuthAction::Insert { .. } | AuthAction::Update { .. } | AuthAction::Delete { .. }
                    if physical_inventory
                        .as_ref()
                        .is_some_and(|inventory| !inventory.has_write_headroom()) =>
                {
                    Authorization::Deny
                }
                AuthAction::Attach { .. }
                | AuthAction::Detach { .. }
                | AuthAction::CreateIndex { .. }
                | AuthAction::CreateTable { .. }
                | AuthAction::CreateTrigger { .. }
                | AuthAction::CreateView { .. }
                | AuthAction::CreateTempIndex { .. }
                | AuthAction::CreateTempTable { .. }
                | AuthAction::CreateTempTrigger { .. }
                | AuthAction::CreateTempView { .. }
                | AuthAction::DropTempIndex { .. }
                | AuthAction::DropTempTable { .. }
                | AuthAction::DropTempTrigger { .. }
                | AuthAction::DropTempView { .. }
                | AuthAction::CreateVtable { .. }
                | AuthAction::DropVtable { .. }
                | AuthAction::DropIndex { .. }
                | AuthAction::DropTable { .. }
                | AuthAction::DropTrigger { .. }
                | AuthAction::DropView { .. }
                | AuthAction::AlterTable { .. }
                | AuthAction::Reindex { .. }
                | AuthAction::Analyze { .. }
                // Parameterized ATTACH has no filename while SQLite prepares
                // it, which rusqlite represents as an unknown action. Unknown
                // authorizer codes are never safe to accept by default.
                | AuthAction::Unknown { .. } => Authorization::Deny,
                AuthAction::Pragma { pragma_value: Some(_), .. } => Authorization::Deny,
                AuthAction::Function { function_name: "load_extension" } => Authorization::Deny,
                _ => Authorization::Allow,
            }))
            .map_err(refused)
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
    if !reports_only_required_temp_store_ffi() {
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

/// The fixed SQLite object names that share one physical byte budget.
#[derive(Debug, Clone)]
struct PhysicalInventory {
    /// The SQLite main database path resolved through the retained directory descriptor.
    main: std::path::PathBuf,
    /// The largest combined main, WAL, and shared-memory footprint the contract permits.
    maximum_bytes: u64,
}

impl PhysicalInventory {
    /// Builds one inventory from a pinned main-database path.
    fn new(main: std::path::PathBuf) -> Result<Self, DatabaseFailure> {
        let maximum_bytes =
            DaemonRuntimeContract::embedded().formula("maximum_sqlite_physical_bytes");
        if maximum_bytes == 0 {
            return Err(DatabaseFailure::PhysicalInventoryRefused(
                "the runtime contract names no SQLite physical byte budget".to_owned(),
            ));
        }
        Ok(Self { main, maximum_bytes })
    }

    /// Requires all SQLite-named objects to be private regular files within the byte budget.
    fn require_within_budget(&self) -> Result<(), DatabaseFailure> {
        let total = self.measured_bytes()?;
        if total > self.maximum_bytes {
            return Err(DatabaseFailure::PhysicalInventoryRefused(format!(
                "{total} bytes exceeds the {} byte SQLite budget",
                self.maximum_bytes
            )));
        }
        Ok(())
    }

    /// Returns whether another largest permitted write can begin safely.
    fn has_write_headroom(&self) -> bool {
        let contract = DaemonRuntimeContract::embedded();
        let required = contract.formula("maximum_sqlite_write_transaction_bytes").checked_add(
            contract.formula("maximum_sqlite_write_transaction_write_ahead_log_bytes"),
        );
        self.measured_bytes()
            .ok()
            .zip(required)
            .and_then(|(held, required)| held.checked_add(required))
            .is_some_and(|needed| needed <= self.maximum_bytes)
    }

    /// Sums the closed set of SQLite object bytes, refusing undeclared names and links.
    fn measured_bytes(&self) -> Result<u64, DatabaseFailure> {
        use std::os::unix::fs::MetadataExt as _;

        let parent = self.main.parent().ok_or_else(|| {
            DatabaseFailure::PhysicalInventoryRefused(
                "the pinned database has no parent".to_owned(),
            )
        })?;
        let main_name = self.main.file_name().and_then(|name| name.to_str()).ok_or_else(|| {
            DatabaseFailure::PhysicalInventoryRefused(
                "the pinned database has no UTF-8 name".to_owned(),
            )
        })?;
        let permitted = [
            main_name.to_owned(),
            format!("{main_name}-wal"),
            format!("{main_name}-shm"),
            format!("{main_name}.replacement"),
        ];
        let mut total = 0_u64;
        for entry in std::fs::read_dir(parent)
            .map_err(|failure| DatabaseFailure::PhysicalInventoryRefused(failure.to_string()))?
        {
            let entry = entry.map_err(|failure| {
                DatabaseFailure::PhysicalInventoryRefused(failure.to_string())
            })?;
            let name = entry.file_name();
            let name = name.to_str().ok_or_else(|| {
                DatabaseFailure::PhysicalInventoryRefused(
                    "the database directory has a non-UTF-8 SQLite object name".to_owned(),
                )
            })?;
            if !name.starts_with(main_name) {
                continue;
            }
            if !permitted.iter().any(|permitted| permitted == name) {
                return Err(DatabaseFailure::PhysicalInventoryRefused(format!(
                    "{name} is not a permitted SQLite object"
                )));
            }
            let metadata = std::fs::symlink_metadata(entry.path()).map_err(|failure| {
                DatabaseFailure::PhysicalInventoryRefused(failure.to_string())
            })?;
            if !metadata.is_file() || metadata.nlink() != 1 {
                return Err(DatabaseFailure::PhysicalInventoryRefused(format!(
                    "{name} is not one private regular SQLite object"
                )));
            }
            total = total.checked_add(metadata.len()).ok_or_else(|| {
                DatabaseFailure::PhysicalInventoryRefused(
                    "SQLite object lengths overflow".to_owned(),
                )
            })?;
        }
        Ok(total)
    }
}

/// A database pathname resolved through an open, verified state-root directory.
///
/// Unix resolves the descriptor namespace through that descriptor, not by
/// looking up the original directory pathname again. Keeping the file alive in
/// [`OperationDatabase`] therefore pins the main database and SQLite's `-wal`
/// and `-shm` sidecars to the verified directory across a pathname swap.
#[cfg(unix)]
struct PinnedDatabasePath;

#[cfg(unix)]
impl PinnedDatabasePath {
    /// Opens the containing directory without following it and returns its pinned child path.
    fn open(path: &std::path::Path) -> Result<(File, std::path::PathBuf), DatabaseFailure> {
        use rustix::fs::{Mode, OFlags, open};
        use std::os::fd::AsRawFd as _;
        use std::os::unix::fs::{MetadataExt as _, PermissionsExt as _};

        let root = path.parent().ok_or_else(|| {
            DatabaseFailure::Refused("the database path has no state-root directory".to_owned())
        })?;
        let name = path.file_name().and_then(|name| name.to_str()).ok_or_else(|| {
            DatabaseFailure::Refused("the database name is not valid UTF-8".to_owned())
        })?;
        if name.is_empty() || name.contains(['/', '\\', '?', '#', '%']) {
            return Err(DatabaseFailure::Refused(
                "the database name is not one literal state-root child".to_owned(),
            ));
        }
        let descriptor = open(
            root,
            OFlags::RDONLY | OFlags::DIRECTORY | OFlags::NOFOLLOW | OFlags::CLOEXEC,
            Mode::empty(),
        )
        .map_err(|failure| DatabaseFailure::Refused(failure.to_string()))?;
        let root = File::from(descriptor);
        let metadata =
            root.metadata().map_err(|failure| DatabaseFailure::Refused(failure.to_string()))?;
        if !metadata.is_dir() || metadata.uid() != uzers::get_current_uid() {
            return Err(DatabaseFailure::Refused(
                "the state-root directory is not a directory owned by this user".to_owned(),
            ));
        }
        root.set_permissions(std::fs::Permissions::from_mode(0o700))
            .map_err(|failure| DatabaseFailure::Refused(failure.to_string()))?;
        if root.metadata().map_err(|failure| DatabaseFailure::Refused(failure.to_string()))?.mode()
            & 0o077
            != 0
        {
            return Err(DatabaseFailure::Refused(
                "the state-root directory could not be made private".to_owned(),
            ));
        }
        let descriptor = root.as_raw_fd();
        Ok((root, std::path::PathBuf::from(format!("{DESCRIPTOR_DIRECTORY}/{descriptor}/{name}"))))
    }
}

/// The operating system's stable directory-descriptor namespace.
#[cfg(target_os = "linux")]
const DESCRIPTOR_DIRECTORY: &str = "/proc/self/fd";

/// The operating system's stable directory-descriptor namespace.
#[cfg(target_os = "macos")]
const DESCRIPTOR_DIRECTORY: &str = "/dev/fd";

#[cfg(not(unix))]
struct PinnedDatabasePath;

#[cfg(not(unix))]
impl PinnedDatabasePath {
    /// Refuses rather than silently falling back to an unpinned default path.
    fn open(_path: &std::path::Path) -> Result<(File, std::path::PathBuf), DatabaseFailure> {
        Err(DatabaseFailure::Refused(
            "this build has no pinned-directory SQLite open layer".to_owned(),
        ))
    }
}

/// Returns whether compile-option output names exactly the required temp mode.
fn reports_only_required_temp_store(reported: &[String]) -> bool {
    reported.iter().any(|option| option == REQUIRED_COMPILE_OPTION)
        && reported
            .iter()
            .filter(|option| option.starts_with("TEMP_STORE="))
            .all(|option| option == REQUIRED_COMPILE_OPTION)
}

/// Reads the temporary-store build identity without opening a connection.
#[allow(unsafe_code)]
fn reports_only_required_temp_store_ffi() -> bool {
    const TEMP_STORE_OPTIONS: [&std::ffi::CStr; 4] =
        [c"TEMP_STORE=0", c"TEMP_STORE=1", c"TEMP_STORE=2", c"TEMP_STORE=3"];
    TEMP_STORE_OPTIONS.iter().all(|option| {
        let reported = unsafe { rusqlite::ffi::sqlite3_compileoption_used(option.as_ptr()) } != 0;
        (*option == c"TEMP_STORE=3") == reported
    })
}

/// Refuses incompatible schemas through a read-only connection before any
/// mutable open. Live connections require exact currency; startup may migrate.
fn inspect_existing_schema(
    path: &std::path::Path,
    require_current: bool,
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
    if require_current && observed != supported {
        return Err(DatabaseFailure::Refused(
            "a live connection requires the current database schema".to_owned(),
        ));
    }
    Ok(Some(snapshot))
}

/// Uses only reviewed reads before the mutable startup connection is opened.
fn audit_existing_binding(
    path: &std::path::Path,
    binding: StartupDatabaseBinding<'_>,
) -> Result<(), DatabaseFailure> {
    let connection =
        Connection::open_with_flags(path, OpenFlags::SQLITE_OPEN_READ_ONLY).map_err(refused)?;
    let identity: Option<String> = connection
        .query_row(
            crate::sqlite_statement_inventory::statement_text(
                "read this installation's identifier",
            ),
            [],
            |row| row.get(0),
        )
        .optional()
        .map_err(refused)?;
    if identity.as_deref() != Some(binding.installation.as_text()) {
        return Err(DatabaseFailure::Refused(
            "the database does not belong to the selected installation".to_owned(),
        ));
    }
    let foreign_installations: i64 = connection.query_row(
        crate::sqlite_statement_inventory::statement_text("count unfinished operations from another installation"),
        [binding.installation.as_text()], |row| row.get(0),
    ).map_err(refused)?;
    if foreign_installations != 0 {
        return Err(DatabaseFailure::Refused("unfinished work belongs to another installation".to_owned()));
    }
    let version: i64 = connection.query_row("PRAGMA user_version", [], |row| row.get(0)).map_err(refused)?;
    // Version one predates the outbox; its normal migration creates the empty
    // table. Never query a table that this supported historical schema lacks.
    if version >= 2 {
        let foreign_children: i64 = connection.query_row(
            crate::sqlite_statement_inventory::statement_text("count unfinished author submissions without the selected local owner"),
            rusqlite::params![binding.target, binding.revision], |row| row.get(0),
        ).map_err(refused)?;
        if foreign_children != 0 {
            return Err(DatabaseFailure::Refused("unfinished author work has no selected local owner".to_owned()));
        }
    }
    let mut statement = connection
        .prepare(crate::sqlite_statement_inventory::statement_text(
            "list every partition holding work that has not ended",
        ))
        .map_err(refused)?;
    let mut rows = statement.query([]).map_err(refused)?;
    while let Some(row) = rows.next().map_err(refused)? {
        let target: String = row.get(0).map_err(refused)?;
        let revision: String = row.get(1).map_err(refused)?;
        let contract: String = row.get(2).map_err(refused)?;
        if target != binding.target
            || revision != binding.revision
            || contract != binding.runtime_contract
        {
            return Err(DatabaseFailure::Refused(
                "unfinished work belongs to another target, revision or runtime contract"
                    .to_owned(),
            ));
        }
    }
    Ok(())
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

#[cfg(test)]
mod tests {
    use super::{OperationDatabase, RequiredSettings, StartupDatabaseBinding};
    use slingshot_domain::daemon_runtime_contract::DaemonRuntimeContract;
    use slingshot_domain::installation::InstallationIdentifier;

    fn settings() -> RequiredSettings {
        RequiredSettings {
            page_bytes: 4096,
            database_pages: 262_144,
            busy_timeout_milliseconds: 5000,
        }
    }

    #[test]
    fn bound_startup_refuses_each_foreign_partition_before_changes() {
        for dimension in 0..4 {
            let root = tempfile::tempdir().unwrap();
            let path = root.path().join("operations.sqlite3");
            let identity = InstallationIdentifier::parse(&"a".repeat(64)).unwrap();
            let database = OperationDatabase::open(&path, settings()).unwrap();
            database.record_installation_identifier(&identity, 123).unwrap();
            let mut partition = ["target", "revision", "contract"];
            if dimension < 3 { partition[dimension] = "foreign"; }
            let retained_installation = if dimension == 3 { "b".repeat(64) } else { identity.as_text().to_owned() };
            database.connection.execute(
                "INSERT INTO operation (author_target_identity, author_target_identity_digest, \
                 canonical_command, command_fingerprint, command_wire_name, daemon_runtime_contract_digest, \
                 enqueue_sequence, installation_identifier, lifecycle_state, operation_identifier, \
                 operation_revision, recorded_at_unix_milliseconds, selected_environment_revision) \
                 VALUES ('identity', ?1, '{}', 'fingerprint', 'command', ?3, 1, ?4, 'queued', 'operation', 1, 123, ?2)",
                rusqlite::params![partition[0], partition[1], partition[2], retained_installation],
            ).unwrap();
            drop(database);
            let before = std::fs::read(&path).unwrap();
            let binding = StartupDatabaseBinding {
                installation: &identity,
                target: "target",
                revision: "revision",
                runtime_contract: "contract",
            };
            assert!(OperationDatabase::reopen_bound(&path, settings(), binding).is_err());
            assert_eq!(std::fs::read(&path).unwrap(), before);
            let database = OperationDatabase::open_live(&path, settings()).unwrap();
            assert_eq!(
                database.unfinished_partitions().unwrap(),
                vec![(partition[0].to_owned(), partition[1].to_owned(), partition[2].to_owned(),)]
            );
            database
                .connection
                .execute("UPDATE operation SET lifecycle_state = 'succeeded'", [])
                .unwrap();
            drop(database);
            assert!(OperationDatabase::reopen_bound(&path, settings(), binding).is_ok());
        }
    }

    #[test]
    fn authorizer_refuses_file_escaping_and_temporary_sql_before_effect() {
        let root = tempfile::tempdir().expect("a temporary directory");
        let database = OperationDatabase::open(&root.path().join("operations.sqlite3"), settings())
            .expect("a migrated database");
        let attachment = root.path().join("attachment.sqlite3");
        assert!(
            database
                .connection()
                .execute("ATTACH DATABASE ? AS outside", [attachment.to_string_lossy()])
                .is_err(),
            "the authorizer refuses an attachment while SQLite prepares it"
        );
        assert!(!attachment.exists(), "the refused attachment creates no file");
        assert!(
            database
                .connection()
                .execute_batch("CREATE TEMP TABLE forbidden (value INTEGER)")
                .is_err(),
            "the authorizer refuses temporary database objects"
        );
        assert!(
            database.connection().execute_batch("CREATE TABLE forbidden (value INTEGER)").is_err(),
            "the authorizer refuses permanent schema changes after migration"
        );
        assert!(
            database.connection().execute_batch("PRAGMA temp_store_directory = '/tmp'").is_err(),
            "the authorizer refuses an ambient temporary-directory override"
        );
        assert!(
            database.connection().execute_batch("PRAGMA user_version = 99").is_err(),
            "the authorizer refuses write pragmas after migration"
        );
    }

    #[test]
    fn settings_are_read_back_on_the_product_connection() {
        let root = tempfile::tempdir().expect("a temporary directory");
        let settings = settings();
        let database = OperationDatabase::open(&root.path().join("operations.sqlite3"), settings)
            .expect("a migrated database");
        let read_integer = |pragma: &str| {
            database
                .connection()
                .query_row(&format!("PRAGMA {pragma}"), [], |row| row.get::<_, i64>(0))
                .expect("the pragma reads")
        };
        let read_text = |pragma: &str| {
            database
                .connection()
                .query_row(&format!("PRAGMA {pragma}"), [], |row| row.get::<_, String>(0))
                .expect("the pragma reads")
        };
        assert_eq!(
            read_integer("page_size"),
            i64::try_from(settings.page_bytes).expect("a page count")
        );
        assert_eq!(
            read_integer("max_page_count"),
            i64::try_from(settings.database_pages).expect("a page count")
        );
        assert_eq!(
            read_integer("busy_timeout"),
            i64::try_from(settings.busy_timeout_milliseconds).expect("a timeout")
        );
        assert_eq!(read_text("journal_mode"), "wal");
        assert_eq!(read_integer("synchronous"), 2);
        assert_eq!(read_integer("foreign_keys"), 1);
        assert_eq!(read_integer("temp_store"), 2);
        assert_eq!(
            read_integer("wal_autocheckpoint"),
            i64::try_from(
                DaemonRuntimeContract::embedded().limit("maximum_sqlite_write_ahead_log_frames")
            )
            .expect("a frame limit")
        );
        assert_eq!(
            read_integer("journal_size_limit"),
            i64::try_from(
                DaemonRuntimeContract::embedded().formula("maximum_sqlite_write_ahead_log_bytes")
            )
            .expect("a WAL byte limit")
        );
        assert!(database.require_compile_options().is_ok());
    }

    #[test]
    fn restart_refuses_an_uninventoried_sqlite_sidecar() {
        let root = tempfile::tempdir().expect("a temporary directory");
        let path = root.path().join("operations.sqlite3");
        drop(OperationDatabase::open(&path, settings()).expect("a migrated database"));
        std::fs::write(root.path().join("operations.sqlite3-journal"), b"unexpected")
            .expect("the adversarial sidecar exists");
        let outcome = OperationDatabase::open(&path, settings());
        assert!(
            matches!(outcome, Err(super::DatabaseFailure::PhysicalInventoryRefused(_))),
            "an undeclared SQLite sidecar is refused before service: {outcome:?}"
        );
    }

    #[test]
    fn restart_refuses_a_permitted_object_over_the_physical_budget() {
        let root = tempfile::tempdir().expect("a temporary directory");
        let path = root.path().join("operations.sqlite3");
        drop(OperationDatabase::open(&path, settings()).expect("a migrated database"));
        let replacement = std::fs::File::create(root.path().join("operations.sqlite3.replacement"))
            .expect("the adversarial replacement exists");
        replacement
            .set_len(DaemonRuntimeContract::embedded().formula("maximum_sqlite_physical_bytes") + 1)
            .expect("a sparse over-budget fixture");
        let outcome = OperationDatabase::open(&path, settings());
        assert!(
            matches!(outcome, Err(super::DatabaseFailure::PhysicalInventoryRefused(_))),
            "an over-budget SQLite object is refused before service: {outcome:?}"
        );
    }
}
