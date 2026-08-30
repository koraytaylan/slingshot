//! Which files the database is allowed to be.
//!
//! An operation database is one file with two sidecars, and a rebuild adds one
//! more beside it. Anything else SQLite might open - a rollback journal, a
//! super journal, a temporary database, a transient index, a spilled statement
//! journal - is a file this daemon did not plan for, in a place it did not
//! choose, holding content it accounts for nowhere. So the set is closed and
//! named here, and every open is checked against it.
//!
//! # What this is, and what it is not
//!
//! This is the policy: the exact object names permitted under one state root,
//! the open flags that are refused outright, and a checker that answers for any
//! proposed open. Every path SQLite is given is built from it, and the
//! configuration that would make SQLite want anything else - memory temporary
//! storage, no statement-journal spill, journaling off during a rebuild - is
//! verified before the database is used.
//!
//! It is *not* a delegating virtual file system registered with SQLite itself.
//! Registering one means implementing `sqlite3_vfs` across the C boundary, and
//! this workspace forbids unchecked code outright. The difference is worth
//! being plain about: a registered virtual file system would refuse a prohibited
//! open from inside the library, whereas this refuses one from outside it and
//! relies on the verified configuration to keep the library from wanting one.
//! Both are checked; only the first would be enforced by SQLite.

use std::path::{Path, PathBuf};

/// Suffix the write-ahead log carries.
pub const WRITE_AHEAD_LOG_SUFFIX: &str = "-wal";

/// Suffix the shared-memory index carries.
pub const SHARED_MEMORY_SUFFIX: &str = "-shm";

/// Suffix a replacement database carries while it is being built.
pub const REPLACEMENT_SUFFIX: &str = ".replacement";

/// Suffixes SQLite must never be allowed to open here.
///
/// Each names a file that would hold database content somewhere this daemon
/// does not account for, and several of them would be created in an ambient
/// temporary directory rather than under the state root at all.
pub const REFUSED_SUFFIXES: &[&str] =
    &["-journal", "-shm-journal", "-super-journal", "-wal-journal", ".master-journal"];

/// Open intents this policy refuses whatever the path says.
pub const REFUSED_INTENTS: &[&str] = &[
    "temporary_database",
    "temporary_journal",
    "transient_database",
    "subjournal",
    "super_journal",
    "delete_on_close",
];

/// Environment variables that must not decide where SQLite writes.
///
/// Poisoned in conformance rather than merely unset: an empty value is a value,
/// and a test that only unset them would pass on a machine that never had them.
pub const AMBIENT_TEMPORARY_VARIABLES: &[&str] = &["SQLITE_TMPDIR", "TMPDIR", "TEMP", "TMP"];

/// Why one proposed open is refused.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum OpenRefusal {
    /// The path is not under the state root.
    #[error("a database object lives under the state root this daemon owns")]
    OutsideStateRoot,
    /// The name is not one of the permitted objects.
    #[error(
        "a database object is the main database, its log, its shared memory, or the one replacement"
    )]
    NotAPermittedObject,
    /// The intent is one this policy refuses outright.
    #[error("this daemon opens no {0}")]
    RefusedIntent(String),
}

/// The objects one database is allowed to be.
#[derive(Debug, Clone)]
pub struct ObjectWhitelist {
    /// Directory the database lives in.
    root: PathBuf,
    /// File name of the main database.
    main: String,
}

impl ObjectWhitelist {
    /// Returns the whitelist for `main` inside `root`.
    #[must_use]
    pub fn for_database(root: impl Into<PathBuf>, main: impl Into<String>) -> Self {
        Self { root: root.into(), main: main.into() }
    }

    /// Returns every object name this database may have, in order.
    #[must_use]
    pub fn permitted_names(&self) -> Vec<String> {
        vec![
            self.main.clone(),
            format!("{}{WRITE_AHEAD_LOG_SUFFIX}", self.main),
            format!("{}{SHARED_MEMORY_SUFFIX}", self.main),
            format!("{}{REPLACEMENT_SUFFIX}", self.main),
        ]
    }

    /// Returns where the main database lives.
    #[must_use]
    pub fn main_path(&self) -> PathBuf {
        self.root.join(&self.main)
    }

    /// Returns where a replacement is built.
    #[must_use]
    pub fn replacement_path(&self) -> PathBuf {
        self.root.join(format!("{}{REPLACEMENT_SUFFIX}", self.main))
    }

    /// Requires one proposed open to be permitted.
    ///
    /// # Errors
    ///
    /// Returns [`OpenRefusal`] naming the first rule the open breaks. The
    /// intent is checked before the path, because a temporary database is
    /// refused wherever somebody proposes to put it.
    pub fn require_permitted(&self, path: &Path, intent: &str) -> Result<(), OpenRefusal> {
        if REFUSED_INTENTS.contains(&intent) {
            return Err(OpenRefusal::RefusedIntent(intent.to_owned()));
        }
        let Some(parent) = path.parent() else {
            return Err(OpenRefusal::OutsideStateRoot);
        };
        if parent != self.root {
            return Err(OpenRefusal::OutsideStateRoot);
        }
        let Some(name) = path.file_name().and_then(|name| name.to_str()) else {
            return Err(OpenRefusal::NotAPermittedObject);
        };
        if self.permitted_names().iter().any(|permitted| permitted == name) {
            Ok(())
        } else {
            Err(OpenRefusal::NotAPermittedObject)
        }
    }
}

/// Settings a replacement database is built under.
///
/// Journaling off and locking exclusive, so a rebuild produces one file and no
/// sidecars at all. The whitelist would refuse a replacement log anyway; this
/// is what stops SQLite from wanting one.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ReplacementSettings;

impl ReplacementSettings {
    /// Returns the pragmas a replacement build sets, in order.
    #[must_use]
    pub fn pragmas() -> Vec<(&'static str, &'static str)> {
        vec![("journal_mode", "OFF"), ("locking_mode", "EXCLUSIVE"), ("temp_store", "MEMORY")]
    }
}
