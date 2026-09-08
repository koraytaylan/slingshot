//! Filesystem authority for one configuration source.
//!
//! One authority reads every configuration source, so the same rules apply to
//! the commit inventory, a profile, the selection, a credential, and a
//! certificate. The rules exist because the alternative - checking a path and
//! then opening it - is a race: whatever was checked can be replaced before it
//! is read. A descendant is therefore opened relative to an already verified
//! directory handle, without following a link, and every later decision is made
//! against that same open object.
//!
//! A final file must have exactly one name. A second hard link, even one owned
//! by the same account, is a second entry outside the verified root through
//! which the accepted object can still be rewritten.
//!
//! A read is accepted only when evidence taken before, between, and after two
//! complete reads is identical and both reads produced the same bytes. That
//! proves the object did not change while it was read. It does not prove the
//! object is the newest: a handle opened just before an atomic rename keeps
//! naming the old object and truthfully returns all of it, so deciding which
//! generation a set of sources belongs to is the coordinator's job.
//!
//! An actively malicious writer running as the same account is inside the trust
//! boundary these rules draw.

use slingshot_domain::profile_authentication_contract::ConfigurationFailureCode;
use slingshot_domain::secret_value::SensitiveConfigurationDocument;

/// Reason a configuration source could not be read.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("{code} at {structural_location}")]
pub struct CredentialFilesystemFailure {
    /// Stable code from the contract registry.
    pub code: ConfigurationFailureCode,
    /// Manifest vocabulary naming where the failure was found.
    pub structural_location: &'static str,
}

impl CredentialFilesystemFailure {
    /// Returns one failure at a named structural location.
    #[must_use]
    pub fn at(code: ConfigurationFailureCode, structural_location: &'static str) -> Self {
        Self { code, structural_location }
    }

    /// Returns the failure a source that failed the safety policy produces.
    #[must_use]
    pub fn unsafe_file(structural_location: &'static str) -> Self {
        Self::at(ConfigurationFailureCode::ConfigurationFileUnsafe, structural_location)
    }
}

/// One entry directly below a verified directory.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct DirectoryEntry {
    /// Complete name, exactly as the directory holds it.
    pub name: String,
    /// Whether the entry is an ordinary file.
    pub ordinary_file: bool,
}

/// One source read stably through a single verified handle.
#[derive(Debug)]
pub struct StableSource {
    /// The bytes, redacted and zeroized on disposal.
    pub document: SensitiveConfigurationDocument,
    /// Length the two reads agreed on.
    pub length: u64,
}

/// Identity and mutation evidence taken from one open object.
///
/// The fields are the platform-independent shape of what each row reports: the
/// device and inode or the volume and volume-scoped identifier, the kind, the
/// link count, the length, and the two times. Comparing the whole tuple is what
/// makes "unchanged" mean unchanged rather than "the same length".
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SourceEvidence {
    /// Volume the object lives on.
    pub volume: u64,
    /// Identity of the object on that volume.
    pub object: u128,
    /// Whether the object is an ordinary file.
    pub ordinary_file: bool,
    /// Names the object has.
    pub links: u64,
    /// Length of the object.
    pub length: u64,
    /// When the object's content last changed.
    pub content_changed: i128,
    /// When the object's metadata last changed.
    pub metadata_changed: i128,
}

/// The authority every configuration source is read through.
///
/// The trait exists so the coordinator above it never learns which platform it
/// is on, and so every rule can be proved by a fake producing states this
/// machine cannot be asked for.
pub trait ConfigurationFilesystemAuthority {
    /// Verifies the configuration root and every component leading to it.
    ///
    /// # Errors
    ///
    /// Returns [`ConfigurationFailureCode::ConfigurationRootUnsafe`] when a
    /// component is a link, or is owned or reachable by another account.
    fn verify_root(&self) -> Result<(), CredentialFilesystemFailure>;

    /// Returns the entries directly below one verified directory.
    ///
    /// # Errors
    ///
    /// Returns [`ConfigurationFailureCode::ConfigurationFileUnsafe`] when the
    /// directory fails the policy, and
    /// [`ConfigurationFailureCode::ConfigurationDirectoryLimitExceeded`] when it
    /// holds more entries than the contract allows.
    fn list_directory(
        &self,
        components: &[&str],
        maximum_entries: u64,
    ) -> Result<Vec<DirectoryEntry>, CredentialFilesystemFailure>;

    /// Reports whether one optional descendant is present, following no link
    /// and reopening no path, so it answers about the object a read reaches.
    ///
    /// # Errors
    ///
    /// Returns [`ConfigurationFailureCode::ConfigurationFileUnsafe`] when the
    /// descendant exists but is not an object this authority may open.
    fn observe_presence(&self, components: &[&str]) -> Result<bool, CredentialFilesystemFailure>;

    /// Reads one descendant stably, within `maximum_bytes`.
    ///
    /// # Errors
    ///
    /// Returns [`ConfigurationFailureCode::ConfigurationFileUnsafe`] for a
    /// policy violation, `ConfigurationDocumentTooLarge` for an object beyond
    /// the bound, and `ConfigurationFileChangedDuringRead` for one that changed
    /// during both attempts.
    fn read_source(
        &self,
        components: &[&str],
        maximum_bytes: u64,
    ) -> Result<StableSource, CredentialFilesystemFailure>;
}

/// Reads one object twice, accepting only identical bytes between identical
/// evidence.
///
/// The protocol is one thing and where the evidence comes from is another, so
/// each row supplies its own observer and both follow the same sequence.
///
/// # Errors
///
/// Returns [`ConfigurationFailureCode::ConfigurationFileChangedDuringRead`]
/// when either read was short, the reads disagreed, or the evidence moved.
pub fn read_twice<FileType>(
    file: &mut FileType,
    before: SourceEvidence,
    location: &'static str,
    observe: impl Fn(&FileType) -> Result<SourceEvidence, CredentialFilesystemFailure>,
) -> Result<Vec<u8>, CredentialFilesystemFailure>
where
    FileType: std::io::Read + std::io::Seek,
{
    use std::io::SeekFrom;

    let changed = || {
        CredentialFilesystemFailure::at(
            ConfigurationFailureCode::ConfigurationFileChangedDuringRead,
            location,
        )
    };
    let first = read_exactly(file, before.length).ok_or_else(changed)?;
    let middle = observe(file)?;
    file.seek(SeekFrom::Start(0)).map_err(|_| changed())?;
    let second = read_exactly(file, middle.length).ok_or_else(changed)?;
    let after = observe(file)?;
    if before != middle || middle != after || first != second {
        return Err(changed());
    }
    Ok(first)
}

/// Reads exactly `length` bytes and requires the object to end there.
///
/// A short read and a longer one are both refused: either means the object
/// moved under the read.
fn read_exactly<FileType: std::io::Read>(file: &mut FileType, length: u64) -> Option<Vec<u8>> {
    let mut bytes = vec![0; usize::try_from(length).unwrap_or(usize::MAX)];
    file.read_exact(&mut bytes).ok()?;
    let mut beyond = [0; 1];
    match file.read(&mut beyond) {
        Ok(0) => Some(bytes),
        _ => None,
    }
}

/// The authority the two Unix rows use.
///
/// Every decision is made against an already-open object: the root is reached
/// one component at a time from the account's home, each component is opened
/// without following a link, and a descendant is opened relative to the
/// directory just verified.
#[cfg(unix)]
#[derive(Debug)]
pub struct UnixConfigurationFilesystem {
    /// Root this authority reads below.
    root: crate::configuration_root::ConfigurationRoot,
    /// Effective user every object must be owned by.
    owner: u32,
    /// Attempts one source receives before it is refused as unstable.
    attempts: u32,
}

#[cfg(unix)]
mod unix_policy {
    //! Rules the two Unix rows apply to one already-open object.

    use std::fs::File;

    use rustix::fs::{FileType, Mode, OFlags, openat};
    use slingshot_domain::profile_authentication_contract::{
        ConfigurationFailureCode, ProfileAuthenticationContract,
    };
    use slingshot_domain::secret_value::SensitiveConfigurationDocument;
    #[cfg(any(target_os = "linux", target_os = "macos"))]
    use xattr::FileExt;

    use super::{
        ConfigurationFilesystemAuthority, CredentialFilesystemFailure, DirectoryEntry,
        SourceEvidence, StableSource, UnixConfigurationFilesystem, read_twice,
    };
    use crate::configuration_root::{AccountIdentity, ConfigurationRoot};

    #[cfg(target_os = "linux")]
    const ACCESS_LIST_ATTRIBUTE: &str = "system.posix_acl_access";

    #[cfg(target_os = "linux")]
    const DEFAULT_LIST_ATTRIBUTE: &str = "system.posix_acl_default";

    #[cfg(target_os = "macos")]
    const EXTENDED_LIST_ATTRIBUTE: &str = "com.apple.system.Security";

    #[cfg(target_os = "linux")]
    const ACCESS_LIST_VERSION: u32 = 2;

    #[cfg(target_os = "linux")]
    const VERSION_LENGTH: usize = 4;

    #[cfg(target_os = "linux")]
    const ENTRY_LENGTH: usize = 8;

    #[cfg(target_os = "linux")]
    const NAMED_USER_TAG: u16 = 0x02;

    #[cfg(target_os = "linux")]
    const OWNING_GROUP_TAG: u16 = 0x04;

    #[cfg(target_os = "linux")]
    const NAMED_GROUP_TAG: u16 = 0x08;

    #[cfg(target_os = "linux")]
    const MASK_TAG: u16 = 0x10;

    #[cfg(target_os = "linux")]
    const OTHER_TAG: u16 = 0x20;

    #[cfg(target_os = "linux")]
    const EVERY_PERMISSION: u16 = 0x07;

    #[cfg(target_os = "linux")]
    const WRITE_PERMISSION: u16 = 0x02;

    const NON_OWNER_ACCESS: u64 = 0o077;

    const NON_OWNER_WRITE: u64 = 0o022;

    const ACCEPTED_LINKS: u64 = 1;

    const NANOSECONDS_PER_SECOND: i128 = 1_000_000_000;

    const ROOT_LOCATION: &str = "configuration_root";

    const SOURCE_LOCATION: &str = "configuration_source";

    #[cfg(target_os = "linux")]
    #[derive(Debug, Clone, Copy)]
    struct AccessEntry {
        tag: u16,
        permissions: u16,
    }

    impl UnixConfigurationFilesystem {
        /// Returns the authority for `root`.
        ///
        /// # Errors
        ///
        /// Returns [`ConfigurationFailureCode::UnsupportedPlatform`] when the
        /// root was sampled for an identity this row cannot compare against.
        pub fn new(root: ConfigurationRoot) -> Result<Self, CredentialFilesystemFailure> {
            let AccountIdentity::UnixUser(owner) = *root.identity() else {
                return Err(CredentialFilesystemFailure::at(
                    ConfigurationFailureCode::UnsupportedPlatform,
                    ROOT_LOCATION,
                ));
            };
            let attempts = u32::try_from(
                ProfileAuthenticationContract::embedded()
                    .limits
                    .maximum_configuration_stable_read_attempts,
            )
            .unwrap_or(u32::MAX);
            Ok(Self { root, owner, attempts })
        }

        /// Opens the verified root directory.
        fn open_root(&self) -> Result<File, CredentialFilesystemFailure> {
            let mut directory = File::open(self.root.traversal_origin()).map_err(|_| {
                failure(ConfigurationFailureCode::ConfigurationRootUnsafe, ROOT_LOCATION)
            })?;
            for component in ConfigurationRoot::root_components() {
                directory =
                    open_child_directory(&directory, component, ROOT_LOCATION).map_err(|_| {
                        failure(ConfigurationFailureCode::ConfigurationRootUnsafe, ROOT_LOCATION)
                    })?;
                self.verify_directory(&directory, ROOT_LOCATION).map_err(|_| {
                    failure(ConfigurationFailureCode::ConfigurationRootUnsafe, ROOT_LOCATION)
                })?;
            }
            Ok(directory)
        }

        /// Opens the directory `components` names, verifying every step.
        fn open_directory(&self, components: &[&str]) -> Result<File, CredentialFilesystemFailure> {
            let mut directory = self.open_root()?;
            for component in components {
                directory = open_child_directory(&directory, component, SOURCE_LOCATION)?;
                self.verify_directory(&directory, SOURCE_LOCATION)?;
            }
            Ok(directory)
        }

        /// Reports that one open directory lets no other account change it.
        fn verify_directory(
            &self,
            directory: &File,
            location: &'static str,
        ) -> Result<(), CredentialFilesystemFailure> {
            let identity = rustix::fs::fstat(directory)
                .map_err(|_| CredentialFilesystemFailure::unsafe_file(location))?;
            let mode = widen(identity.st_mode);
            if widen(identity.st_uid) != u64::from(self.owner) || mode & NON_OWNER_WRITE != 0 {
                return Err(CredentialFilesystemFailure::unsafe_file(location));
            }
            refuse_widened_access(directory, true, location)
        }

        /// Reports that one open final file is a source this authority may read.
        fn verify_source(
            &self,
            file: &File,
        ) -> Result<SourceEvidence, CredentialFilesystemFailure> {
            let observed = evidence(file)?;
            let identity = rustix::fs::fstat(file)
                .map_err(|_| CredentialFilesystemFailure::unsafe_file(SOURCE_LOCATION))?;
            let mode = widen(identity.st_mode);
            let owned = widen(identity.st_uid) == u64::from(self.owner);
            if !owned || !observed.ordinary_file || observed.links != ACCEPTED_LINKS {
                return Err(CredentialFilesystemFailure::unsafe_file(SOURCE_LOCATION));
            }
            if mode & NON_OWNER_ACCESS != 0 {
                return Err(CredentialFilesystemFailure::unsafe_file(SOURCE_LOCATION));
            }
            refuse_widened_access(file, false, SOURCE_LOCATION)?;
            Ok(observed)
        }

        /// Opens one final source relative to its verified parent.
        fn open_source(&self, components: &[&str]) -> Result<File, CredentialFilesystemFailure> {
            let (parents, name) = split_components(components)?;
            let directory = self.open_directory(parents)?;
            open_child(&directory, name, OFlags::RDONLY, SOURCE_LOCATION)
        }
    }

    impl ConfigurationFilesystemAuthority for UnixConfigurationFilesystem {
        fn verify_root(&self) -> Result<(), CredentialFilesystemFailure> {
            self.open_root().map(|_| ())
        }

        fn list_directory(
            &self,
            components: &[&str],
            maximum_entries: u64,
        ) -> Result<Vec<DirectoryEntry>, CredentialFilesystemFailure> {
            let directory = self.open_directory(components)?;
            let reader = rustix::fs::Dir::read_from(&directory)
                .map_err(|_| CredentialFilesystemFailure::unsafe_file(SOURCE_LOCATION))?;
            let mut entries = Vec::new();
            for entry in reader {
                let entry =
                    entry.map_err(|_| CredentialFilesystemFailure::unsafe_file(SOURCE_LOCATION))?;
                let name = entry.file_name().to_string_lossy().into_owned();
                if name == "." || name == ".." {
                    continue;
                }
                if u64::try_from(entries.len()).unwrap_or(u64::MAX) >= maximum_entries {
                    return Err(failure(
                        ConfigurationFailureCode::ConfigurationDirectoryLimitExceeded,
                        SOURCE_LOCATION,
                    ));
                }
                entries.push(DirectoryEntry {
                    name,
                    ordinary_file: entry.file_type() == FileType::RegularFile,
                });
            }
            entries.sort();
            Ok(entries)
        }

        fn observe_presence(
            &self,
            components: &[&str],
        ) -> Result<bool, CredentialFilesystemFailure> {
            let (parents, name) = split_components(components)?;
            let directory = self.open_directory(parents)?;
            match openat(
                &directory,
                name,
                OFlags::RDONLY | OFlags::NOFOLLOW | OFlags::CLOEXEC,
                Mode::empty(),
            ) {
                Ok(opened) => {
                    let file = File::from(opened);
                    self.verify_source(&file).map(|_| true)
                }
                Err(rustix::io::Errno::NOENT) => Ok(false),
                Err(_) => Err(CredentialFilesystemFailure::unsafe_file(SOURCE_LOCATION)),
            }
        }

        fn read_source(
            &self,
            components: &[&str],
            maximum_bytes: u64,
        ) -> Result<StableSource, CredentialFilesystemFailure> {
            let mut unstable = None;
            for _ in 0..self.attempts {
                let mut file = self.open_source(components)?;
                let observed = self.verify_source(&file)?;
                if observed.length > maximum_bytes {
                    return Err(failure(
                        ConfigurationFailureCode::ConfigurationDocumentTooLarge,
                        SOURCE_LOCATION,
                    ));
                }
                match read_twice(&mut file, observed, SOURCE_LOCATION, evidence) {
                    Ok(bytes) => {
                        let length = observed.length;
                        return Ok(StableSource {
                            document: SensitiveConfigurationDocument::from_bytes(bytes),
                            length,
                        });
                    }
                    Err(reason) => unstable = Some(reason),
                }
            }
            Err(unstable.unwrap_or_else(|| {
                failure(
                    ConfigurationFailureCode::ConfigurationFileChangedDuringRead,
                    SOURCE_LOCATION,
                )
            }))
        }
    }

    fn failure(
        code: ConfigurationFailureCode,
        location: &'static str,
    ) -> CredentialFilesystemFailure {
        CredentialFilesystemFailure::at(code, location)
    }

    fn split_components<'components>(
        components: &'components [&'components str],
    ) -> Result<(&'components [&'components str], &'components str), CredentialFilesystemFailure>
    {
        components
            .split_last()
            .map(|(name, parents)| (parents, *name))
            .ok_or_else(|| CredentialFilesystemFailure::unsafe_file(SOURCE_LOCATION))
    }

    fn open_child(
        parent: &File,
        component: &str,
        flags: OFlags,
        location: &'static str,
    ) -> Result<File, CredentialFilesystemFailure> {
        openat(parent, component, flags | OFlags::NOFOLLOW | OFlags::CLOEXEC, Mode::empty())
            .map(File::from)
            .map_err(|_| CredentialFilesystemFailure::unsafe_file(location))
    }

    fn open_child_directory(
        parent: &File,
        component: &str,
        location: &'static str,
    ) -> Result<File, CredentialFilesystemFailure> {
        open_child(parent, component, OFlags::RDONLY | OFlags::DIRECTORY, location)
    }

    fn evidence(object: &File) -> Result<SourceEvidence, CredentialFilesystemFailure> {
        let identity = rustix::fs::fstat(object)
            .map_err(|_| CredentialFilesystemFailure::unsafe_file(SOURCE_LOCATION))?;
        Ok(SourceEvidence {
            volume: widen(identity.st_dev),
            object: u128::from(widen(identity.st_ino)),
            ordinary_file: FileType::from_raw_mode(identity.st_mode) == FileType::RegularFile,
            links: widen(identity.st_nlink),
            length: widen(identity.st_size),
            content_changed: moment(identity.st_mtime, identity.st_mtime_nsec),
            metadata_changed: moment(identity.st_ctime, identity.st_ctime_nsec),
        })
    }

    fn widen<Value: TryInto<u64>>(value: Value) -> u64 {
        value.try_into().unwrap_or(u64::MAX)
    }

    fn moment<Seconds: TryInto<i128>, Nanoseconds: TryInto<i128>>(
        seconds: Seconds,
        nanoseconds: Nanoseconds,
    ) -> i128 {
        let seconds: i128 = seconds.try_into().unwrap_or(i128::MAX);
        let nanoseconds: i128 = nanoseconds.try_into().unwrap_or(i128::MAX);
        seconds * NANOSECONDS_PER_SECOND + nanoseconds
    }

    /// The two Unix rows keep that state in different places, so each asks its
    /// own platform's question. Asking the other's is not merely useless: the
    /// kernel refuses a name outside the namespaces it knows, and that refusal
    /// is indistinguishable from a widened object.
    #[cfg(target_os = "linux")]
    fn refuse_widened_access(
        object: &File,
        directory: bool,
        location: &'static str,
    ) -> Result<(), CredentialFilesystemFailure> {
        let refused = if directory { WRITE_PERMISSION } else { EVERY_PERMISSION };
        refuse_widened_list(object, ACCESS_LIST_ATTRIBUTE, refused, location)?;
        if !directory {
            return Ok(());
        }
        refuse_widened_list(object, DEFAULT_LIST_ATTRIBUTE, EVERY_PERMISSION, location)
    }

    /// Refuses an object whose access-control state widens it beyond its owner.
    ///
    /// This row's semantic version accepts no extended entry at all, so a
    /// nontrivial allow or deny fails rather than being interpreted.
    #[cfg(target_os = "macos")]
    fn refuse_widened_access(
        object: &File,
        _directory: bool,
        location: &'static str,
    ) -> Result<(), CredentialFilesystemFailure> {
        match object.get_xattr(EXTENDED_LIST_ATTRIBUTE) {
            Ok(None) => Ok(()),
            _ => Err(CredentialFilesystemFailure::unsafe_file(location)),
        }
    }

    #[cfg(not(any(target_os = "linux", target_os = "macos")))]
    fn refuse_widened_access(
        _object: &File,
        _directory: bool,
        location: &'static str,
    ) -> Result<(), CredentialFilesystemFailure> {
        Err(CredentialFilesystemFailure::at(
            ConfigurationFailureCode::UnsupportedPlatform,
            location,
        ))
    }

    /// Refuses an object whose access-control list widens it beyond its owner.
    ///
    /// `refused` names the permission bits a non-owner entry may not hold once
    /// the mask has been applied: every bit for a file, the write bit for a
    /// directory whose entries must stay under this account's control.
    #[cfg(target_os = "linux")]
    fn refuse_widened_list(
        object: &File,
        attribute: &str,
        refused: u16,
        location: &'static str,
    ) -> Result<(), CredentialFilesystemFailure> {
        let Ok(stored) = object.get_xattr(attribute) else {
            return Err(CredentialFilesystemFailure::unsafe_file(location));
        };
        let Some(stored) = stored else {
            return Ok(());
        };
        let entries = decode_list(&stored)
            .ok_or_else(|| CredentialFilesystemFailure::unsafe_file(location))?;
        let mask = entries
            .iter()
            .find(|entry| entry.tag == MASK_TAG)
            .map_or(EVERY_PERMISSION, |entry| entry.permissions);
        for entry in &entries {
            let effective = match entry.tag {
                NAMED_USER_TAG | NAMED_GROUP_TAG | OWNING_GROUP_TAG => entry.permissions & mask,
                OTHER_TAG => entry.permissions,
                _ => continue,
            };
            if effective & refused != 0 {
                return Err(CredentialFilesystemFailure::unsafe_file(location));
            }
        }
        Ok(())
    }

    #[cfg(target_os = "linux")]
    fn decode_list(stored: &[u8]) -> Option<Vec<AccessEntry>> {
        let version = stored.get(..VERSION_LENGTH)?;
        let version = u32::from_le_bytes(version.try_into().ok()?);
        if version != ACCESS_LIST_VERSION {
            return None;
        }
        let (entries, remainder) = stored.get(VERSION_LENGTH..)?.as_chunks::<ENTRY_LENGTH>();
        if !remainder.is_empty() {
            return None;
        }
        Some(
            entries
                .iter()
                .map(|entry| AccessEntry {
                    tag: u16::from_le_bytes([entry[0], entry[1]]),
                    permissions: u16::from_le_bytes([entry[2], entry[3]]),
                })
                .collect(),
        )
    }
}

#[cfg(windows)]
#[derive(Debug)]
/// Windows configuration filesystem authority.
pub struct WindowsConfigurationFilesystem {
    root: crate::configuration_root::ConfigurationRoot,
    owner: String,
    attempts: u32,
}

#[cfg(windows)]
mod windows_policy {
    //! Rules the Windows row applies to one already-open object.

    use std::io::{self, Read, Seek, SeekFrom};
    use std::os::windows::io::{AsRawHandle, RawHandle};
    use std::path::{Path, PathBuf};

    use slingshot_domain::profile_authentication_contract::{
        ConfigurationFailureCode, ProfileAuthenticationContract,
    };
    use slingshot_domain::secret_value::SensitiveConfigurationDocument;
    use windows_permissions::constants::{
        AccessRights, AceType, SeObjectType, SecurityInformation,
    };
    use windows_permissions::wrappers::{ConvertSidToStringSid, GetSecurityInfo};

    use super::{
        ConfigurationFilesystemAuthority, CredentialFilesystemFailure, DirectoryEntry,
        SourceEvidence, StableSource, WindowsConfigurationFilesystem, read_twice,
    };
    use crate::configuration_root::{AccountIdentity, ConfigurationRoot};

    const REPARSE_POINT_ATTRIBUTE: u32 = 0x0000_0400;

    const DIRECTORY_ATTRIBUTE: u32 = 0x0000_0010;

    const LOCAL_SYSTEM: &str = "S-1-5-18";

    const BUILTIN_ADMINISTRATORS: &str = "S-1-5-32-544";

    const ACCEPTED_LINKS: u64 = 1;

    const ROOT_LOCATION: &str = "configuration_root";

    const SOURCE_LOCATION: &str = "configuration_source";

    struct WindowsFile {
        handle: winsafe::guard::CloseHandleGuard<winsafe::HFILE>,
    }

    impl WindowsFile {
        fn open(path: &Path, location: &'static str) -> Result<Self, CredentialFilesystemFailure> {
            let path =
                path.to_str().ok_or_else(|| CredentialFilesystemFailure::unsafe_file(location))?;
            let (handle, _) = winsafe::HFILE::CreateFile(
                path,
                winsafe::co::GENERIC::READ,
                Some(
                    winsafe::co::FILE_SHARE::READ
                        | winsafe::co::FILE_SHARE::WRITE
                        | winsafe::co::FILE_SHARE::DELETE,
                ),
                None,
                winsafe::co::DISPOSITION::OPEN_EXISTING,
                winsafe::co::FILE_ATTRIBUTE::NORMAL,
                Some(
                    winsafe::co::FILE_FLAG::BACKUP_SEMANTICS
                        | winsafe::co::FILE_FLAG::OPEN_REPARSE_POINT,
                ),
                None,
                None,
            )
            .map_err(|_| CredentialFilesystemFailure::unsafe_file(location))?;
            Ok(Self { handle })
        }
    }

    impl AsRawHandle for WindowsFile {
        fn as_raw_handle(&self) -> RawHandle {
            self.handle.ptr()
        }
    }

    impl Read for WindowsFile {
        fn read(&mut self, bytes: &mut [u8]) -> io::Result<usize> {
            self.handle
                .ReadFile(bytes)
                .map(|read| usize::try_from(read).unwrap_or(usize::MAX))
                .map_err(windows_io_error)
        }
    }

    impl Seek for WindowsFile {
        fn seek(&mut self, position: SeekFrom) -> io::Result<u64> {
            let (distance, origin) = match position {
                SeekFrom::Start(offset) => (
                    i64::try_from(offset).map_err(|_| {
                        io::Error::new(io::ErrorKind::InvalidInput, "offset too large")
                    })?,
                    winsafe::co::FILE_STARTING_POINT::BEGIN,
                ),
                SeekFrom::Current(offset) => (offset, winsafe::co::FILE_STARTING_POINT::CURRENT),
                SeekFrom::End(offset) => (offset, winsafe::co::FILE_STARTING_POINT::END),
            };
            let offset =
                self.handle.SetFilePointerEx(distance, origin).map_err(windows_io_error)?;
            u64::try_from(offset)
                .map_err(|_| io::Error::new(io::ErrorKind::InvalidInput, "negative offset"))
        }
    }

    fn windows_io_error<Error: Into<u32>>(error: Error) -> io::Error {
        io::Error::from_raw_os_error(i32::try_from(error.into()).unwrap_or(i32::MAX))
    }

    impl WindowsConfigurationFilesystem {
        /// Returns the authority for `root`.
        ///
        /// # Errors
        ///
        /// Returns [`ConfigurationFailureCode::UnsupportedPlatform`] when the
        /// root was sampled for an identity this row cannot compare against.
        pub fn new(root: ConfigurationRoot) -> Result<Self, CredentialFilesystemFailure> {
            let AccountIdentity::WindowsUser(owner) = root.identity().clone() else {
                return Err(CredentialFilesystemFailure::at(
                    ConfigurationFailureCode::UnsupportedPlatform,
                    ROOT_LOCATION,
                ));
            };
            let attempts = u32::try_from(
                ProfileAuthenticationContract::embedded()
                    .limits
                    .maximum_configuration_stable_read_attempts,
            )
            .unwrap_or(u32::MAX);
            Ok(Self { root, owner, attempts })
        }

        fn walk(&self, components: &[&str]) -> Result<PathBuf, CredentialFilesystemFailure> {
            let mut path = self.root.traversal_origin().to_path_buf();
            let root_components: Vec<&str> =
                ConfigurationRoot::root_components().iter().map(String::as_str).collect();
            for component in root_components {
                path.push(component);
                let opened = open_object(&path, ROOT_LOCATION)?;
                self.verify_directory(&opened, ROOT_LOCATION).map_err(|_| {
                    CredentialFilesystemFailure::at(
                        ConfigurationFailureCode::ConfigurationRootUnsafe,
                        ROOT_LOCATION,
                    )
                })?;
            }
            for component in components {
                path.push(component);
                let opened = open_object(&path, SOURCE_LOCATION)?;
                self.verify_directory(&opened, SOURCE_LOCATION)?;
            }
            Ok(path)
        }

        fn verify_directory(
            &self,
            directory: &WindowsFile,
            location: &'static str,
        ) -> Result<(), CredentialFilesystemFailure> {
            let observed = evidence(directory, location)?;
            if observed.ordinary_file {
                return Err(CredentialFilesystemFailure::unsafe_file(location));
            }
            self.verify_security(directory, location)
        }

        fn verify_source(
            &self,
            file: &WindowsFile,
        ) -> Result<SourceEvidence, CredentialFilesystemFailure> {
            let observed = evidence(file, SOURCE_LOCATION)?;
            if !observed.ordinary_file || observed.links != ACCEPTED_LINKS {
                return Err(CredentialFilesystemFailure::unsafe_file(SOURCE_LOCATION));
            }
            self.verify_security(file, SOURCE_LOCATION)?;
            Ok(observed)
        }

        fn verify_security(
            &self,
            object: &WindowsFile,
            location: &'static str,
        ) -> Result<(), CredentialFilesystemFailure> {
            let refuse = || CredentialFilesystemFailure::unsafe_file(location);
            let descriptor = GetSecurityInfo(
                object,
                SeObjectType::SE_FILE_OBJECT,
                SecurityInformation::Owner | SecurityInformation::Dacl,
            )
            .map_err(|_| refuse())?;
            let owner = descriptor.owner().ok_or_else(refuse)?;
            let rendered = ConvertSidToStringSid(owner).map_err(|_| refuse())?;
            if rendered.to_string_lossy() != self.owner {
                return Err(refuse());
            }
            let list = descriptor.dacl().ok_or_else(refuse)?;
            let harmless = AccessRights::ReadControl | AccessRights::Synchronize;
            for index in 0..list.len() {
                let entry = list.get_ace(index).ok_or_else(refuse)?;
                let allowed = match entry.ace_type() {
                    AceType::ACCESS_ALLOWED_ACE_TYPE => true,
                    AceType::ACCESS_DENIED_ACE_TYPE => false,
                    _ => return Err(refuse()),
                };
                let principal = entry.sid().ok_or_else(refuse)?;
                let rendered = ConvertSidToStringSid(principal).map_err(|_| refuse())?;
                let named = rendered.to_string_lossy();
                let trusted =
                    named == self.owner || named == LOCAL_SYSTEM || named == BUILTIN_ADMINISTRATORS;
                if trusted || !allowed {
                    continue;
                }
                if entry.mask() & !harmless != AccessRights::empty() {
                    return Err(refuse());
                }
            }
            Ok(())
        }
    }

    impl ConfigurationFilesystemAuthority for WindowsConfigurationFilesystem {
        fn verify_root(&self) -> Result<(), CredentialFilesystemFailure> {
            self.walk(&[]).map(|_| ())
        }

        fn list_directory(
            &self,
            components: &[&str],
            maximum_entries: u64,
        ) -> Result<Vec<DirectoryEntry>, CredentialFilesystemFailure> {
            let path = self.walk(components)?;
            let reader = std::fs::read_dir(&path)
                .map_err(|_| CredentialFilesystemFailure::unsafe_file(SOURCE_LOCATION))?;
            let mut entries = Vec::new();
            for entry in reader {
                let entry =
                    entry.map_err(|_| CredentialFilesystemFailure::unsafe_file(SOURCE_LOCATION))?;
                if u64::try_from(entries.len()).unwrap_or(u64::MAX) >= maximum_entries {
                    return Err(CredentialFilesystemFailure::at(
                        ConfigurationFailureCode::ConfigurationDirectoryLimitExceeded,
                        SOURCE_LOCATION,
                    ));
                }
                let kind = entry
                    .file_type()
                    .map_err(|_| CredentialFilesystemFailure::unsafe_file(SOURCE_LOCATION))?;
                entries.push(DirectoryEntry {
                    name: entry.file_name().to_string_lossy().into_owned(),
                    ordinary_file: kind.is_file(),
                });
            }
            entries.sort();
            Ok(entries)
        }

        fn observe_presence(
            &self,
            components: &[&str],
        ) -> Result<bool, CredentialFilesystemFailure> {
            let (parents, name) =
                components
                    .split_last()
                    .map(|(name, parents)| (parents, *name))
                    .ok_or_else(|| CredentialFilesystemFailure::unsafe_file(SOURCE_LOCATION))?;
            let mut path = self.walk(parents)?;
            path.push(name);
            if !path.exists() {
                return Ok(false);
            }
            let file = open_object(&path, SOURCE_LOCATION)?;
            self.verify_source(&file).map(|_| true)
        }

        fn read_source(
            &self,
            components: &[&str],
            maximum_bytes: u64,
        ) -> Result<StableSource, CredentialFilesystemFailure> {
            let (parents, name) =
                components
                    .split_last()
                    .map(|(name, parents)| (parents, *name))
                    .ok_or_else(|| CredentialFilesystemFailure::unsafe_file(SOURCE_LOCATION))?;
            let mut unstable = None;
            for _ in 0..self.attempts {
                let mut path = self.walk(parents)?;
                path.push(name);
                let mut file = open_object(&path, SOURCE_LOCATION)?;
                let observed = self.verify_source(&file)?;
                if observed.length > maximum_bytes {
                    return Err(CredentialFilesystemFailure::at(
                        ConfigurationFailureCode::ConfigurationDocumentTooLarge,
                        SOURCE_LOCATION,
                    ));
                }
                match read_twice(&mut file, observed, SOURCE_LOCATION, |object| {
                    evidence(object, SOURCE_LOCATION)
                }) {
                    Ok(bytes) => {
                        return Ok(StableSource {
                            document: SensitiveConfigurationDocument::from_bytes(bytes),
                            length: observed.length,
                        });
                    }
                    Err(reason) => unstable = Some(reason),
                }
            }
            Err(unstable.unwrap_or_else(|| {
                CredentialFilesystemFailure::at(
                    ConfigurationFailureCode::ConfigurationFileChangedDuringRead,
                    SOURCE_LOCATION,
                )
            }))
        }
    }

    fn open_object(
        path: &Path,
        location: &'static str,
    ) -> Result<WindowsFile, CredentialFilesystemFailure> {
        WindowsFile::open(path, location)
    }

    fn evidence(
        object: &WindowsFile,
        location: &'static str,
    ) -> Result<SourceEvidence, CredentialFilesystemFailure> {
        let refuse = || CredentialFilesystemFailure::unsafe_file(location);
        let metadata = object.handle.GetFileInformationByHandle().map_err(|_| refuse())?;
        let attributes = metadata.dwFileAttributes.raw();
        if attributes & REPARSE_POINT_ATTRIBUTE != 0 {
            return Err(refuse());
        }
        let (creation, _, last_write) = object.handle.GetFileTime().map_err(|_| refuse())?;
        Ok(SourceEvidence {
            volume: u64::from(metadata.dwVolumeSerialNumber),
            object: u128::from(metadata.nFileIndex()),
            ordinary_file: attributes & DIRECTORY_ATTRIBUTE == 0,
            links: u64::from(metadata.nNumberOfLinks),
            length: metadata.nFileSize(),
            content_changed: i128::from(u64::from(last_write)),
            // Stable Rust has no public Windows change-time accessor. The
            // creation time is immutable identity metadata and therefore gives
            // the stable read protocol a safe, handle-bound metadata value.
            metadata_changed: i128::from(u64::from(creation)),
        })
    }
}
