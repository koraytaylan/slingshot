//! Descriptor-bound SQLite physical file accounting.

use super::*;

/// One database's named SQLite objects and the bound their combined bytes stay under.
#[derive(Debug)]
pub(super) struct PhysicalInventory {
    /// The SQLite main database path resolved through the retained directory descriptor.
    pub(super) main: std::path::PathBuf,
    /// The verified state-root directory, kept open so its entries are read through
    /// the descriptor rather than through the pinned pathname again. On macOS the
    /// descriptor namespace is not directory-scannable by pathname, so the inventory
    /// reads through the descriptor itself.
    #[cfg(unix)]
    pub(super) state_root: Option<std::os::fd::OwnedFd>,
    /// The largest combined main, WAL, and shared-memory footprint the contract permits.
    pub(super) maximum_bytes: u64,
}

impl PhysicalInventory {
    /// Builds one inventory from a pinned main-database path.
    pub(super) fn new(main: std::path::PathBuf) -> Result<Self, DatabaseFailure> {
        let maximum_bytes =
            DaemonRuntimeContract::embedded().formula("maximum_sqlite_physical_bytes");
        if maximum_bytes == 0 {
            return Err(DatabaseFailure::PhysicalInventoryRefused(
                "the runtime contract names no SQLite physical byte budget".to_owned(),
            ));
        }
        Ok(Self {
            main,
            #[cfg(unix)]
            state_root: None,
            maximum_bytes,
        })
    }

    /// Requires all SQLite-named objects to be private regular files within the byte budget.
    pub(super) fn require_within_budget(&self) -> Result<(), DatabaseFailure> {
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
    pub(super) fn has_write_headroom(&self) -> bool {
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

    #[cfg(unix)]
    fn entries(&self) -> Result<Vec<(String, rustix::fs::Stat)>, DatabaseFailure> {
        Ok(match &self.state_root {
            Some(state_root) => {
                // Read the verified directory through its own descriptor. The pinned
                // pathname's parent is a descriptor namespace that a path-based
                // read_dir cannot scan everywhere this build runs.
                let reader = rustix::fs::Dir::read_from(state_root).map_err(|failure| {
                    DatabaseFailure::PhysicalInventoryRefused(failure.to_string())
                })?;
                let mut named = Vec::new();
                for entry in reader {
                    let entry = entry.map_err(|failure| {
                        DatabaseFailure::PhysicalInventoryRefused(failure.to_string())
                    })?;
                    let name = match entry.file_name().to_str() {
                        Ok(name) => name,
                        Err(_) => {
                            return Err(DatabaseFailure::PhysicalInventoryRefused(
                                "the database directory has a non-UTF-8 SQLite object name"
                                    .to_owned(),
                            ));
                        }
                    };
                    let metadata =
                        rustix::fs::statat(state_root, name, rustix::fs::AtFlags::SYMLINK_NOFOLLOW)
                            .map_err(|failure| {
                                DatabaseFailure::PhysicalInventoryRefused(failure.to_string())
                            })?;
                    named.push((name.to_owned(), metadata));
                }
                named
            }
            None => Vec::new(),
        })
    }

    /// Sums the closed set of SQLite object bytes, refusing undeclared names and links.
    fn measured_bytes(&self) -> Result<u64, DatabaseFailure> {
        #[cfg(not(unix))]
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
        #[cfg(unix)]
        let entries = self.entries()?;
        #[cfg(not(unix))]
        let entries: Vec<(String, std::fs::Metadata)> = {
            let mut named = Vec::new();
            for entry in std::fs::read_dir(parent)
                .map_err(|failure| DatabaseFailure::PhysicalInventoryRefused(failure.to_string()))?
            {
                let entry = entry.map_err(|failure| {
                    DatabaseFailure::PhysicalInventoryRefused(failure.to_string())
                })?;
                let metadata = entry.metadata().map_err(|failure| {
                    DatabaseFailure::PhysicalInventoryRefused(failure.to_string())
                })?;
                let name = entry.file_name();
                let name = name.to_str().ok_or_else(|| {
                    DatabaseFailure::PhysicalInventoryRefused(
                        "the database directory has a non-UTF-8 SQLite object name".to_owned(),
                    )
                })?;
                named.push((name.to_owned(), metadata));
            }
            named
        };
        for (name, metadata) in entries {
            if !name.starts_with(main_name) {
                continue;
            }
            if !permitted.contains(&name.to_owned()) {
                return Err(DatabaseFailure::PhysicalInventoryRefused(format!(
                    "{name} is not a permitted SQLite object"
                )));
            }
            #[cfg(unix)]
            {
                if metadata.st_nlink != 1
                    || rustix::fs::FileType::from_raw_mode(metadata.st_mode)
                        != rustix::fs::FileType::RegularFile
                {
                    return Err(DatabaseFailure::PhysicalInventoryRefused(format!(
                        "{name} is not one private regular SQLite object"
                    )));
                }
                total = total
                    .checked_add(u64::try_from(metadata.st_size).unwrap_or(u64::MAX))
                    .ok_or_else(|| {
                        DatabaseFailure::PhysicalInventoryRefused(
                            "SQLite object lengths overflow".to_owned(),
                        )
                    })?;
            }
            #[cfg(not(unix))]
            {
                if !is_private_regular_file(&metadata) {
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
        }
        Ok(total)
    }
}

/// Windows has no stable standard-library hard-link count accessor. It still
/// refuses non-regular objects here; the Windows handle-bound policy performs
/// the stronger identity checks available through its safe API.
#[cfg(not(unix))]
fn is_private_regular_file(metadata: &std::fs::Metadata) -> bool {
    metadata.is_file()
}
