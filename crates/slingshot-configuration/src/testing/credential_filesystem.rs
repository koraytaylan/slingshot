//! Deterministic configuration roots for filesystem-policy tests.
//!
//! Most of what the filesystem authority refuses cannot be arranged on the
//! machine running the tests. A file owned by another account, a widened
//! access-control list, a second hard link outside the root, a source that
//! changes between two reads, a writer publishing a new commit inventory
//! between two of them - each needs either privileges the test does not have or
//! a race it cannot schedule.
//!
//! So the authority is an injected trait and this module implements it from a
//! script. Every rule the real rows enforce is stated here as a state a caller
//! asks for, which makes each rule provable on any host and makes the rows'
//! behavior comparable to one another rather than to whatever the test machine
//! happens to allow.

use std::cell::RefCell;
use std::collections::{BTreeMap, BTreeSet};

use slingshot_domain::profile_authentication_contract::{
    ConfigurationFailureCode, ProfileAuthenticationContract,
};
use slingshot_domain::secret_value::SensitiveConfigurationDocument;

use crate::credential_filesystem::{
    ConfigurationFilesystemAuthority, CredentialFilesystemFailure, DirectoryEntry, StableSource,
};

/// Separator every scripted path uses.
const SEPARATOR: char = '/';

/// Structural location every scripted decision is reported at.
const SOURCE_LOCATION: &str = "configuration_source";

/// Why one scripted entry is or is not safe to read.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EntrySafety {
    /// The entry satisfies every rule.
    Safe,
    /// The entry is a symbolic link or a reparse point.
    Link,
    /// The entry is owned by another account.
    ForeignOwner,
    /// The entry grants another principal access to its bytes.
    WidenedAccess,
    /// The entry has a second name, which may be outside the verified root.
    SecondLink,
    /// The entry is not an ordinary file.
    NotOrdinary,
}

/// How one scripted entry behaves while it is being read.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Instability {
    /// The entry does not change.
    Stable,
    /// The entry changes during the first attempt and settles afterwards.
    SettlesAfterOneAttempt,
    /// The entry changes during every attempt.
    NeverSettles,
}

/// One entry of a scripted configuration root.
#[derive(Debug, Clone)]
pub struct ScriptedEntry {
    /// Bytes the entry holds.
    pub bytes: Vec<u8>,
    /// Why the entry is or is not safe to read.
    pub safety: EntrySafety,
    /// How the entry behaves while it is being read.
    pub instability: Instability,
}

impl ScriptedEntry {
    /// Returns a safe, stable entry holding `bytes`.
    #[must_use]
    pub fn safe(bytes: &[u8]) -> Self {
        Self { bytes: bytes.to_vec(), safety: EntrySafety::Safe, instability: Instability::Stable }
    }

    /// Returns the same entry with another safety state.
    #[must_use]
    pub fn with_safety(mut self, safety: EntrySafety) -> Self {
        self.safety = safety;
        self
    }

    /// Returns the same entry with another instability.
    #[must_use]
    pub fn with_instability(mut self, instability: Instability) -> Self {
        self.instability = instability;
        self
    }
}

/// A configuration root that behaves exactly as a test scripts it.
#[derive(Debug, Default)]
pub struct ScriptedFilesystem {
    /// Directories the root holds, by root-relative path.
    directories: BTreeSet<String>,
    /// Entries the root holds, by root-relative path.
    entries: BTreeMap<String, ScriptedEntry>,
    /// Whether the root itself is safe to traverse.
    root_safe: bool,
    /// Reads taken so far, so a script can act on the third one.
    reads: RefCell<u64>,
    /// Attempts already spent on each entry, so instability can settle.
    attempts: RefCell<BTreeMap<String, u32>>,
    /// Bytes every entry holds once the writer's next generation is published.
    published: Option<(u64, BTreeMap<String, Vec<u8>>)>,
}

impl ScriptedFilesystem {
    /// Returns an empty scripted root whose own traversal is safe.
    #[must_use]
    pub fn new() -> Self {
        Self { root_safe: true, ..Self::default() }
    }

    /// Returns the same root with a directory at `path`.
    #[must_use]
    pub fn with_directory(mut self, path: &str) -> Self {
        self.directories.insert(path.to_owned());
        self
    }

    /// Returns the same root with a safe, stable source at `path`.
    #[must_use]
    pub fn with_source(self, path: &str, bytes: &[u8]) -> Self {
        self.with_entry(path, ScriptedEntry::safe(bytes))
    }

    /// Returns the same root with `entry` at `path`.
    #[must_use]
    pub fn with_entry(mut self, path: &str, entry: ScriptedEntry) -> Self {
        if let Some((parent, _)) = path.rsplit_once(SEPARATOR) {
            self.directories.insert(parent.to_owned());
        }
        self.entries.insert(path.to_owned(), entry);
        self
    }

    /// Returns the same root with an unsafe traversal to the root itself.
    #[must_use]
    pub fn with_unsafe_root(mut self) -> Self {
        self.root_safe = false;
        self
    }

    /// Returns the same root, switching to `replacement` after `reads` reads.
    ///
    /// This is how a writer publishing a new generation part way through a
    /// coordinator's attempt is scripted: the sources a later read sees are the
    /// new ones while the ones already read are the old.
    #[must_use]
    pub fn publishing_after(mut self, reads: u64, replacement: BTreeMap<String, Vec<u8>>) -> Self {
        self.published = Some((reads, replacement));
        self
    }

    /// Returns how many reads this root has answered.
    #[must_use]
    pub fn reads(&self) -> u64 {
        *self.reads.borrow()
    }

    /// Returns the bytes `path` holds at this point in the script.
    fn bytes_now(&self, path: &str, entry: &ScriptedEntry) -> Vec<u8> {
        let Some((after, replacement)) = &self.published else {
            return entry.bytes.clone();
        };
        if *self.reads.borrow() <= *after {
            return entry.bytes.clone();
        }
        replacement.get(path).cloned().unwrap_or_else(|| entry.bytes.clone())
    }

    /// Reports whether every directory above `path` is present and safe.
    fn verify_ancestors(&self, path: &str) -> Result<(), CredentialFilesystemFailure> {
        if !self.root_safe {
            return Err(CredentialFilesystemFailure::at(
                ConfigurationFailureCode::ConfigurationRootUnsafe,
                "configuration_root",
            ));
        }
        let Some((parent, _)) = path.rsplit_once(SEPARATOR) else {
            return Ok(());
        };
        if self.directories.contains(parent) {
            return Ok(());
        }
        Err(CredentialFilesystemFailure::unsafe_file(SOURCE_LOCATION))
    }

    /// Reports the failure one entry's safety state produces.
    fn verify_safety(entry: &ScriptedEntry) -> Result<(), CredentialFilesystemFailure> {
        match entry.safety {
            EntrySafety::Safe => Ok(()),
            _ => Err(CredentialFilesystemFailure::unsafe_file(SOURCE_LOCATION)),
        }
    }

    /// Reports whether this attempt at `path` observes the entry changing.
    fn observes_change(&self, path: &str, entry: &ScriptedEntry) -> bool {
        match entry.instability {
            Instability::Stable => false,
            Instability::NeverSettles => true,
            Instability::SettlesAfterOneAttempt => {
                let mut attempts = self.attempts.borrow_mut();
                let spent = attempts.entry(path.to_owned()).or_insert(0);
                *spent += 1;
                *spent == 1
            }
        }
    }
}

impl ConfigurationFilesystemAuthority for ScriptedFilesystem {
    fn verify_root(&self) -> Result<(), CredentialFilesystemFailure> {
        if self.root_safe {
            return Ok(());
        }
        Err(CredentialFilesystemFailure::at(
            ConfigurationFailureCode::ConfigurationRootUnsafe,
            "configuration_root",
        ))
    }

    fn list_directory(
        &self,
        components: &[&str],
        maximum_entries: u64,
    ) -> Result<Vec<DirectoryEntry>, CredentialFilesystemFailure> {
        let directory = components.join("/");
        self.verify_root()?;
        if !self.directories.contains(&directory) {
            return Err(CredentialFilesystemFailure::unsafe_file(SOURCE_LOCATION));
        }
        let prefix = format!("{directory}{SEPARATOR}");
        let mut entries = Vec::new();
        for (path, entry) in &self.entries {
            let Some(name) = path.strip_prefix(&prefix) else {
                continue;
            };
            if name.contains(SEPARATOR) {
                continue;
            }
            if u64::try_from(entries.len()).unwrap_or(u64::MAX) >= maximum_entries {
                return Err(CredentialFilesystemFailure::at(
                    ConfigurationFailureCode::ConfigurationDirectoryLimitExceeded,
                    SOURCE_LOCATION,
                ));
            }
            entries.push(DirectoryEntry {
                name: name.to_owned(),
                ordinary_file: entry.safety != EntrySafety::NotOrdinary,
            });
        }
        entries.sort();
        Ok(entries)
    }

    fn observe_presence(&self, components: &[&str]) -> Result<bool, CredentialFilesystemFailure> {
        let path = components.join("/");
        self.verify_ancestors(&path)?;
        let Some(entry) = self.entries.get(&path) else {
            return Ok(false);
        };
        Self::verify_safety(entry).map(|()| true)
    }

    fn read_source(
        &self,
        components: &[&str],
        maximum_bytes: u64,
    ) -> Result<StableSource, CredentialFilesystemFailure> {
        let path = components.join("/");
        self.verify_ancestors(&path)?;
        let entry = self
            .entries
            .get(&path)
            .ok_or_else(|| CredentialFilesystemFailure::unsafe_file(SOURCE_LOCATION))?;
        Self::verify_safety(entry)?;
        let attempts = ProfileAuthenticationContract::embedded()
            .limits
            .maximum_configuration_stable_read_attempts;
        for _ in 0..attempts {
            *self.reads.borrow_mut() += 1;
            let bytes = self.bytes_now(&path, entry);
            let length = u64::try_from(bytes.len()).unwrap_or(u64::MAX);
            if length > maximum_bytes {
                return Err(CredentialFilesystemFailure::at(
                    ConfigurationFailureCode::ConfigurationDocumentTooLarge,
                    SOURCE_LOCATION,
                ));
            }
            if self.observes_change(&path, entry) {
                continue;
            }
            return Ok(StableSource {
                document: SensitiveConfigurationDocument::from_bytes(bytes),
                length,
            });
        }
        Err(CredentialFilesystemFailure::at(
            ConfigurationFailureCode::ConfigurationFileChangedDuringRead,
            SOURCE_LOCATION,
        ))
    }
}
