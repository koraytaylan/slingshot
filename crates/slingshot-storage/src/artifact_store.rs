//! Artifacts addressed by their content and verified on the handle they came from.
//!
//! A large result cannot travel inside a protocol frame, and the obvious
//! alternative - handing a client a filesystem path - trades one problem for a
//! worse one: a path names whatever is at it when someone looks, which is not
//! necessarily what was verified. So nothing here ever hands out a path or a
//! remote label. An artifact is named by a deterministic identifier, its bytes
//! are addressed by their digest, and reading it happens through a handle that
//! was verified before the first byte was emitted and is verified again after
//! the last.
//!
//! The two-pass rule is the whole point of that handle. The first pass reads
//! the file through and checks the digest and the length. The handle is then
//! rewound rather than reopened, and the second pass hashes and counts what is
//! actually streamed to the caller. Success is reported only when both passes
//! agree with the recorded metadata and the handle is still the same file it
//! was. Anyone who replaces the path between the passes has replaced something
//! this reader is not reading; anyone who rewrites the file underneath it
//! changes the second-pass digest, and no successful end is emitted.
//!
//! Identity never comes from a file name or a remote label. It is a digest over
//! a version marker, the installation, the author-target partition, the
//! operation, and the command-declared slot, so the same work against the same
//! target names the same artifact and two targets that differ only by the
//! principal behind them name two.

use std::io::{Read as _, Seek as _, Write as _};
use std::path::{Path, PathBuf};

use rusqlite::OptionalExtension as _;
use sha2::{Digest as _, Sha256};
use slingshot_domain::command::command_identity::CommandContract;
use slingshot_domain::daemon_runtime_contract::DaemonRuntimeContract;
use slingshot_domain::installation::InstallationIdentifier;

/// Version marker every artifact identifier is derived under.
pub const ARTIFACT_IDENTIFIER_VERSION: &str = "slingshot.artifact-identifier/1";

/// Separator between the fields an artifact identifier is derived from.
///
/// A zero octet cannot occur in any of them, so no two different field lists
/// can produce one preimage.
pub const FIELD_SEPARATOR: u8 = 0;

/// The slot every command reserves for its own structured result.
pub const STRUCTURED_RESULT_SLOT: &str = "structured_result";

/// Media type a canonical structured result is stored under.
pub const CANONICAL_JSON_MEDIA_TYPE: &str = "application/json";

/// Characters a digest is spelled with.
pub const DIGEST_CHARACTERS: usize = 64;

/// The contract limit bounding an artifact slot.
const ARTIFACT_SLOT_LIMIT: &str = "maximum_artifact_slot_bytes";

/// The contract limit bounding an artifact's media type.
const MEDIA_TYPE_LIMIT: &str = "maximum_artifact_media_type_bytes";

/// Returns how many bytes an artifact slot may occupy.
///
/// Asked of the command contract by name rather than written down again here.
/// A second declaration is a second thing that can drift, and this one had:
/// the contract bounded a slot at one width and this crate enforced another.
#[must_use]
pub fn maximum_artifact_slot_bytes() -> usize {
    contract_limit(ARTIFACT_SLOT_LIMIT)
}

/// Returns how many bytes an artifact's media type may occupy.
#[must_use]
pub fn maximum_media_type_bytes() -> usize {
    contract_limit(MEDIA_TYPE_LIMIT)
}

/// Returns one command-contract limit as a byte count.
fn contract_limit(named: &str) -> usize {
    usize::try_from(CommandContract::embedded().limit(named))
        .expect("a byte bound fits this machine's word")
}

/// Bytes a descriptor may occupy.
pub const MAXIMUM_DESCRIPTOR_BYTES: usize = 512;

/// Bytes a byte length occupies when spelled in base ten.
pub const MAXIMUM_BYTE_LENGTH_CHARACTERS: usize = 20;

/// Bytes the whole of one artifact's access metadata may occupy.
///
/// The contract names bounds for what crosses the wire as bytes rather than for
/// what describes them, so this record's bound is declared here. It is the one
/// the machine envelope is built to hold - strictly below four kibibytes - and
/// the field bounds above are proved to sum below it rather than assumed to.
pub const MAXIMUM_ARTIFACT_ACCESS_BYTES: usize = 4096;

/// Directory below the store root that holds addressed content.
pub const CONTENT_DIRECTORY: &str = "content";

/// Suffix a partially installed artifact carries until it is complete.
///
/// A file wearing this is not a result and is never read as one. It is left
/// where it is after an interruption rather than deleted, so an operator can
/// see what happened, and it is ignorable because nothing addresses it.
pub const STAGING_SUFFIX: &str = ".partial";

/// Bytes one read of the installing stream moves.
const TRANSFER_BYTES: usize = 65_536;

/// Reason an artifact could not be installed, opened, or read.
#[derive(Debug, thiserror::Error)]
pub enum ArtifactFailure {
    /// The filesystem refused.
    #[error("the filesystem refused: {0}")]
    FilesystemRefused(String),
    /// A bounded field arrived longer than its bound.
    #[error("{field} holds {actual} bytes, and this store allows {allowed}")]
    TooLong {
        /// Which field.
        field: &'static str,
        /// How long it was.
        actual: usize,
        /// How long it may be.
        allowed: usize,
    },
    /// The content is longer than one artifact may be.
    #[error("an artifact holds at most {allowed} bytes, and this one holds {actual}")]
    ContentTooLong {
        /// How long it was.
        actual: u64,
        /// How long it may be.
        allowed: u64,
    },
    /// The bytes are not the bytes the metadata describes.
    #[error("the content digest is {actual}, and the metadata says {expected}")]
    DigestMismatch {
        /// What was read.
        actual: String,
        /// What was recorded.
        expected: String,
    },
    /// The length is not the length the metadata describes.
    #[error("the content is {actual} bytes, and the metadata says {expected}")]
    LengthMismatch {
        /// What was read.
        actual: u64,
        /// What was recorded.
        expected: u64,
    },
    /// The file changed identity while it was being read.
    #[error("the file being read is no longer the file that was verified")]
    HandleMoved,
    /// The file is not one this process may read.
    #[error("the artifact file is not a plain file this user alone can reach")]
    NotPrivate,
    /// A digest was not spelled the way a digest is.
    #[error("a digest is {DIGEST_CHARACTERS} lowercase hexadecimal characters")]
    DigestNotCanonical,
    /// There is no such artifact.
    #[error("no artifact holds the content {0}")]
    NoSuchContent(String),
    /// A concurrent publisher made the digest name visible first.
    #[error("content {0} was published concurrently")]
    ContentAlreadyPresent(String),
}

/// Returns a filesystem refusal as this module's failure.
fn refused(failure: std::io::Error) -> ArtifactFailure {
    ArtifactFailure::FilesystemRefused(failure.to_string())
}

/// Requires `text` to fit `allowed` bytes.
fn require_within(field: &'static str, allowed: usize, text: &str) -> Result<(), ArtifactFailure> {
    if text.len() > allowed {
        return Err(ArtifactFailure::TooLong { field, actual: text.len(), allowed });
    }
    Ok(())
}

/// Returns `octets` in lowercase hexadecimal.
fn render(octets: &[u8]) -> String {
    octets.iter().map(|octet| format!("{octet:02x}")).collect()
}

/// Returns whether `spelling` is a digest spelled canonically.
fn is_canonical_digest(spelling: &str) -> bool {
    spelling.len() == DIGEST_CHARACTERS
        && spelling.bytes().all(|octet| octet.is_ascii_digit() || (b'a'..=b'f').contains(&octet))
}

/// The deterministic name one artifact has.
///
/// Opaque on purpose. It is a digest, so it carries no file name, no remote
/// label, and nothing a caller could parse a path out of.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ArtifactIdentifier {
    /// The digest, in lowercase hexadecimal.
    spelling: String,
}

impl ArtifactIdentifier {
    /// Returns the identifier the five fields derive.
    #[must_use]
    pub fn derive(
        installation_identifier: &InstallationIdentifier,
        author_target_identity_digest: &str,
        operation_identifier: &str,
        artifact_slot: &str,
    ) -> Self {
        let mut hasher = Sha256::new();
        for field in [
            ARTIFACT_IDENTIFIER_VERSION,
            installation_identifier.as_text(),
            author_target_identity_digest,
            operation_identifier,
            artifact_slot,
        ] {
            hasher.update(field.as_bytes());
            hasher.update([FIELD_SEPARATOR]);
        }
        Self { spelling: render(&hasher.finalize()) }
    }

    /// Returns the identifier `spelling` names.
    ///
    /// # Errors
    ///
    /// Returns [`ArtifactFailure::DigestNotCanonical`].
    pub fn parse(spelling: &str) -> Result<Self, ArtifactFailure> {
        if !is_canonical_digest(spelling) {
            return Err(ArtifactFailure::DigestNotCanonical);
        }
        Ok(Self { spelling: spelling.to_owned() })
    }

    /// Returns this identifier's spelling.
    #[must_use]
    pub fn as_text(&self) -> &str {
        &self.spelling
    }
}

/// Everything a client is told about one artifact.
///
/// No path and no remote address, by construction: a client that could name the
/// file could open the file, and then the verification this store performs
/// would be verification of something else.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ArtifactMetadata {
    /// The deterministic name.
    pub artifact_identifier: ArtifactIdentifier,
    /// The command-declared slot it fills.
    pub artifact_slot: String,
    /// Exactly how many bytes it holds.
    pub byte_length: u64,
    /// The digest of those bytes.
    pub content_digest: String,
    /// A bounded human description, when there is one.
    pub descriptor: Option<String>,
    /// The bounded media type.
    pub media_type: String,
}

impl ArtifactMetadata {
    /// Returns how many bytes this record occupies as text.
    ///
    /// Used to prove the record fits the machine envelope, which is a property
    /// of the record rather than of any one field.
    #[must_use]
    pub fn access_bytes(&self) -> usize {
        self.artifact_identifier.as_text().len()
            + self.artifact_slot.len()
            + MAXIMUM_BYTE_LENGTH_CHARACTERS
            + self.content_digest.len()
            + self.descriptor.as_deref().unwrap_or_default().len()
            + self.media_type.len()
    }
}

/// Where one command's structured result ended up.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ResultPlacement {
    /// Small enough to travel in the response itself.
    Inline(String),
    /// Installed as a verified artifact under the structured-result slot.
    Externalized(Box<ArtifactMetadata>),
}

/// One request to install an artifact.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InstallationRequest {
    /// The command-declared slot it fills.
    pub artifact_slot: String,
    /// The partition it belongs to.
    pub author_target_identity_digest: String,
    /// A bounded human description, when there is one.
    pub descriptor: Option<String>,
    /// The installation installing it.
    pub installation_identifier: InstallationIdentifier,
    /// The bounded media type.
    pub media_type: String,
    /// The operation that produced it.
    pub operation_identifier: String,
}

impl InstallationRequest {
    /// Requires every bounded field to fit its bound.
    fn require_bounded(&self) -> Result<(), ArtifactFailure> {
        require_within("artifact slot", maximum_artifact_slot_bytes(), &self.artifact_slot)?;
        require_within("media type", maximum_media_type_bytes(), &self.media_type)?;
        match self.descriptor.as_deref() {
            Some(descriptor) => require_within("descriptor", MAXIMUM_DESCRIPTOR_BYTES, descriptor),
            None => Ok(()),
        }
    }
}

/// What one open file was, so a later look can tell whether it still is.
///
/// On a platform that reports them, the device and inode identify the file
/// itself rather than the name it currently answers to; length and modification
/// time catch a rewrite that kept the same inode.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct HandleSnapshot {
    /// The device the file is on, where the platform reports one.
    device: u64,
    /// The file's number on that device, where the platform reports one.
    number: u64,
    /// How long the file was.
    byte_length: u64,
    /// When it was last written, in nanoseconds, where the platform reports it.
    modified_nanoseconds: u128,
}

impl HandleSnapshot {
    /// Returns what `file` is right now.
    fn of(file: &std::fs::File) -> Result<Self, ArtifactFailure> {
        let metadata = file.metadata().map_err(refused)?;
        require_current_user_only(&metadata)?;
        let modified_nanoseconds = metadata
            .modified()
            .ok()
            .and_then(|at| at.duration_since(std::time::UNIX_EPOCH).ok())
            .map_or(0, |since| since.as_nanos());
        let (device, number) = file_identity(&metadata);
        Ok(Self { device, number, byte_length: metadata.len(), modified_nanoseconds })
    }
}

/// Returns the device and file number a platform reports, or zeroes.
#[cfg(unix)]
fn file_identity(metadata: &std::fs::Metadata) -> (u64, u64) {
    use std::os::unix::fs::MetadataExt as _;

    (metadata.dev(), metadata.ino())
}

/// Returns the device and file number a platform reports, or zeroes.
#[cfg(not(unix))]
fn file_identity(_metadata: &std::fs::Metadata) -> (u64, u64) {
    (0, 0)
}

/// Requires a file to be a plain file its owner alone can reach.
#[cfg(unix)]
fn require_current_user_only(metadata: &std::fs::Metadata) -> Result<(), ArtifactFailure> {
    use std::os::unix::fs::MetadataExt as _;
    use std::os::unix::fs::PermissionsExt as _;

    /// Permission bits anyone but the owner would need.
    const OTHERS: u32 = 0o077;

    let reachable_by_others = metadata.permissions().mode() & OTHERS != 0;
    if !metadata.is_file() || metadata.uid() != uzers::get_current_uid() || reachable_by_others {
        return Err(ArtifactFailure::NotPrivate);
    }
    Ok(())
}

/// Requires a file to be a plain file its owner alone can reach.
#[cfg(not(unix))]
fn require_current_user_only(metadata: &std::fs::Metadata) -> Result<(), ArtifactFailure> {
    if metadata.is_file() { Ok(()) } else { Err(ArtifactFailure::NotPrivate) }
}

/// Creates one file its owner alone can reach, refusing to replace one.
#[cfg(unix)]
fn create_private(path: &Path) -> Result<std::fs::File, ArtifactFailure> {
    use std::os::unix::fs::OpenOptionsExt as _;

    /// Permission bits a private file carries.
    const OWNER_ONLY: u32 = 0o600;

    std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(OWNER_ONLY)
        .open(path)
        .map_err(refused)
}

/// Creates one file its owner alone can reach, refusing to replace one.
#[cfg(not(unix))]
fn create_private(path: &Path) -> Result<std::fs::File, ArtifactFailure> {
    std::fs::OpenOptions::new().write(true).create_new(true).open(path).map_err(refused)
}

/// Opens one file for reading without following a link to it.
#[cfg(unix)]
fn open_without_following(path: &Path) -> Result<std::fs::File, ArtifactFailure> {
    use std::os::unix::fs::OpenOptionsExt as _;

    /// Refuse to open the target of a symbolic link.
    const NO_FOLLOW: i32 = libc_no_follow();

    std::fs::OpenOptions::new().read(true).custom_flags(NO_FOLLOW).open(path).map_err(refused)
}

/// Returns the open flag that refuses to follow a symbolic link.
///
/// Spelled here rather than taken from a C binding because it is one stable
/// number on every platform this daemon runs on, and one number is not worth a
/// dependency that reaches into unchecked interop.
#[cfg(unix)]
const fn libc_no_follow() -> i32 {
    /// The value `O_NOFOLLOW` has on Linux.
    const LINUX: i32 = 0o400_000;
    /// The value `O_NOFOLLOW` has on the Apple platforms.
    const DARWIN: i32 = 0x0010_0000;

    if cfg!(target_os = "linux") { LINUX } else { DARWIN }
}

/// Opens one file for reading without following a link to it.
#[cfg(not(unix))]
fn open_without_following(path: &Path) -> Result<std::fs::File, ArtifactFailure> {
    std::fs::File::open(path).map_err(refused)
}

/// Synchronizes one directory so a rename inside it is durable.
fn synchronize_directory(directory: &Path) -> Result<(), ArtifactFailure> {
    let handle = std::fs::File::open(directory).map_err(refused)?;
    // Some platforms explicitly report that directory synchronization is not
    // supported. Every other failure means durability is unknown and must be
    // reported rather than silently converted into success.
    match handle.sync_all() {
        Ok(()) => Ok(()),
        Err(failure) if failure.kind() == std::io::ErrorKind::InvalidInput => Ok(()),
        Err(failure) => Err(refused(failure)),
    }
}

/// Links verified staged bytes into a digest name without replacing an arrival.
#[cfg(target_os = "linux")]
fn publish_staged_no_replace(
    content: &Path,
    staging: &Path,
    content_digest: &str,
    expected: HandleSnapshot,
) -> Result<(), ArtifactFailure> {
    use rustix::fs::{AtFlags, CWD, Mode, OFlags, linkat, openat};
    use std::os::fd::AsRawFd as _;
    use std::os::unix::fs::MetadataExt as _;

    let staging_name = staging.file_name().ok_or_else(|| {
        ArtifactFailure::FilesystemRefused("the staging path has no file name".to_owned())
    })?;
    let held = std::fs::File::open(content).map_err(refused)?;
    let staged = openat(
        &held,
        staging_name,
        OFlags::RDONLY | OFlags::NOFOLLOW | OFlags::CLOEXEC,
        Mode::empty(),
    )
    .map_err(|failure| ArtifactFailure::FilesystemRefused(failure.to_string()))?;
    let staged = std::fs::File::from(staged);
    let snapshot = HandleSnapshot::of(&staged)?;
    if snapshot != expected {
        return Err(ArtifactFailure::FilesystemRefused("the private stage changed".to_owned()));
    }
    if staged.metadata().map_err(refused)?.nlink() != 1 {
        return Err(ArtifactFailure::NotPrivate);
    }
    staged.sync_all().map_err(refused)?;
    let retained = format!("/proc/self/fd/{}", staged.as_raw_fd());
    match linkat(CWD, retained, &held, content_digest, AtFlags::SYMLINK_FOLLOW) {
        Ok(()) => {}
        Err(failure) if failure == rustix::io::Errno::EXIST => {
            return Err(ArtifactFailure::ContentAlreadyPresent(content_digest.to_owned()));
        }
        Err(failure) => return Err(ArtifactFailure::FilesystemRefused(failure.to_string())),
    }
    let still_staged = std::fs::symlink_metadata(staging).is_ok_and(|current| {
        let (device, number) = file_identity(&current);
        current.is_file() && device == snapshot.device && number == snapshot.number
    });
    if still_staged {
        std::fs::remove_file(staging).map_err(refused)?;
    }
    Ok(())
}

/// Refuses platforms that cannot prove atomic no-replace publication.
#[cfg(not(target_os = "linux"))]
fn publish_staged_no_replace(
    _content: &Path,
    _staging: &Path,
    _content_digest: &str,
    _expected: HandleSnapshot,
) -> Result<(), ArtifactFailure> {
    Err(ArtifactFailure::FilesystemRefused(
        "this platform has no verified no-replace publication primitive".to_owned(),
    ))
}

/// The artifact store, rooted at one directory.
#[derive(Debug, Clone)]
pub struct ArtifactStore {
    /// Where addressed content lives.
    content: PathBuf,
}

/// A private stage owned by this invocation, never a digest-addressed artifact.
struct PrivateStage {
    path: PathBuf,
    identity: HandleSnapshot,
}

impl Drop for PrivateStage {
    fn drop(&mut self) {
        // Do not remove a different file installed at a replaced staging name.
        if std::fs::symlink_metadata(&self.path).is_ok_and(|metadata| {
            metadata.is_file()
                && file_identity(&metadata) == (self.identity.device, self.identity.number)
        }) {
            std::fs::remove_file(&self.path).ok();
        }
    }
}

/// Verified private content awaiting an explicit publication decision.
/// Dropping it removes only its own stage. The caller must keep its capacity
/// reservation alive through staging and the eventual database handoff.
#[must_use = "retain the stage until publication, or drop it to abandon the transfer"]
pub struct StagedArtifact<'store> {
    store: &'store ArtifactStore,
    stage: PrivateStage,
    metadata: ArtifactMetadata,
}

/// Incremental private staging for an asynchronous transport's bounded chunks.
/// Abandonment removes the private stage. Finishing verifies bytes but does not
/// establish transport framing; the caller must prove that before publication.
#[must_use = "keep the writer until the framed transfer finishes or abandon it"]
pub struct ArtifactStageWriter<'store> {
    store: &'store ArtifactStore,
    stage: PrivateStage,
    file: std::fs::File,
    request: InstallationRequest,
    hasher: Sha256,
    received: u64,
    allowed: u64,
    expected: Option<(u64, String)>,
    poisoned: bool,
}

impl core::fmt::Debug for ArtifactStageWriter<'_> {
    fn fmt(&self, formatter: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        formatter.write_str("ArtifactStageWriter([redacted])")
    }
}

impl<'store> ArtifactStageWriter<'store> {
    /// Appends one chunk without retaining it. An error permanently refuses
    /// this writer, even if a caller attempts to continue or finish it.
    pub fn write_chunk(&mut self, bytes: &[u8]) -> Result<(), ArtifactFailure> {
        if self.poisoned {
            return Err(ArtifactFailure::FilesystemRefused(
                "the artifact stage was refused".to_owned(),
            ));
        }
        self.poisoned = true;
        let length = self.received.saturating_add(bytes.len() as u64);
        if length > self.allowed {
            return Err(ArtifactFailure::ContentTooLong { actual: length, allowed: self.allowed });
        }
        self.file.write_all(bytes).map_err(refused)?;
        self.hasher.update(bytes);
        self.received = length;
        self.poisoned = false;
        Ok(())
    }

    /// Verifies complete length/digest and synchronizes the still-private file.
    pub fn finish(mut self) -> Result<StagedArtifact<'store>, ArtifactFailure> {
        if self.poisoned {
            return Err(ArtifactFailure::FilesystemRefused(
                "the artifact stage was refused".to_owned(),
            ));
        }
        let digest = render(&self.hasher.finalize());
        if let Some((length, expected)) = &self.expected {
            if self.received != *length {
                return Err(ArtifactFailure::LengthMismatch {
                    actual: self.received,
                    expected: *length,
                });
            }
            if digest != *expected {
                return Err(ArtifactFailure::DigestMismatch {
                    actual: digest,
                    expected: expected.clone(),
                });
            }
        }
        self.file.sync_all().map_err(refused)?;
        let current = HandleSnapshot::of(&self.file)?;
        if current.byte_length != self.received {
            return Err(ArtifactFailure::LengthMismatch {
                actual: current.byte_length,
                expected: self.received,
            });
        }
        self.stage.identity = current;
        Ok(StagedArtifact {
            store: self.store,
            stage: self.stage,
            metadata: ArtifactMetadata {
                artifact_identifier: ArtifactIdentifier::derive(
                    &self.request.installation_identifier,
                    &self.request.author_target_identity_digest,
                    &self.request.operation_identifier,
                    &self.request.artifact_slot,
                ),
                artifact_slot: self.request.artifact_slot,
                byte_length: self.received,
                content_digest: digest,
                descriptor: self.request.descriptor,
                media_type: self.request.media_type,
            },
        })
    }
}

impl core::fmt::Debug for StagedArtifact<'_> {
    fn fmt(&self, formatter: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        formatter.write_str("StagedArtifact([redacted])")
    }
}

impl StagedArtifact<'_> {
    /// Metadata measured from the private content; no readable association exists yet.
    pub fn metadata(&self) -> &ArtifactMetadata {
        &self.metadata
    }

    /// Opens the still-private bytes for semantic validation without exposing a
    /// filesystem path or publishing content. The consumer must finish the
    /// reader successfully before relying on the bytes it observed.
    pub fn open_verified(&self) -> Result<VerifiedArtifactReader, ArtifactFailure> {
        let file = open_without_following(&self.stage.path)?;
        if HandleSnapshot::of(&file)? != self.stage.identity {
            return Err(ArtifactFailure::FilesystemRefused("the private stage changed".to_owned()));
        }
        VerifiedArtifactReader::from_file(file, &self.metadata)
    }

    /// Publishes verified content without replacing an existing digest object.
    /// This does not commit a database association or release caller capacity.
    pub fn publish(self) -> Result<ArtifactMetadata, ArtifactFailure> {
        let file = open_without_following(&self.stage.path)?;
        if HandleSnapshot::of(&file)? != self.stage.identity {
            return Err(ArtifactFailure::FilesystemRefused("the private stage changed".to_owned()));
        }
        self.store.require_existing(
            &self.stage.path,
            &self.metadata.content_digest,
            self.metadata.byte_length,
        )?;
        self.store.publish(
            &self.stage.path,
            &self.metadata.content_digest,
            self.metadata.byte_length,
            self.stage.identity,
        )?;
        Ok(self.metadata.clone())
    }
}

impl ArtifactStore {
    /// Removes abandoned private stages under exclusive namespace ownership,
    /// before any producer is running. Addressed content and publication holds
    /// are untouched: missing staged bytes must be reverified during recovery.
    /// Invalid staging names or unsafe objects refuse rather than being deleted.
    pub fn recover_abandoned_stages(&self) -> Result<u64, ArtifactFailure> {
        let mut removed = 0_u64;
        for entry in std::fs::read_dir(&self.content).map_err(refused)? {
            let entry = entry.map_err(refused)?;
            let name = entry.file_name();
            let Some(name) = name.to_str() else { continue; };
            let Some(identifier) = name.strip_suffix(STAGING_SUFFIX) else { continue; };
            let parsed = uuid::Uuid::parse_str(identifier).map_err(|_| ArtifactFailure::NotPrivate)?;
            if parsed.to_string() != identifier { return Err(ArtifactFailure::NotPrivate); }
            let metadata = std::fs::symlink_metadata(entry.path()).map_err(refused)?;
            require_current_user_only(&metadata)?;
            #[cfg(unix)] {
                use std::os::unix::fs::MetadataExt as _;
                if metadata.nlink() != 1 { return Err(ArtifactFailure::NotPrivate); }
            }
            std::fs::remove_file(entry.path()).map_err(refused)?;
            removed = removed.saturating_add(1);
        }
        if removed != 0 {
            std::fs::File::open(&self.content).and_then(|directory| directory.sync_all()).map_err(refused)?;
        }
        Ok(removed)
    }

    /// Removes only one canonical, private content file during journaled
    /// maintenance. The caller must hold the database write transaction and
    /// prove there are no retained references. Missing content is an idempotent
    /// retry after deletion; the directory is synchronized before acknowledgement.
    pub(crate) fn remove_unreferenced_content(&self, digest: &str) -> Result<(), ArtifactFailure> {
        if !is_canonical_digest(digest) { return Err(ArtifactFailure::DigestNotCanonical); }
        let path = self.content.join(digest);
        match std::fs::symlink_metadata(&path) {
            Ok(metadata) => {
                require_current_user_only(&metadata)?;
                #[cfg(unix)] {
                    use std::os::unix::fs::MetadataExt as _;
                    if metadata.nlink() != 1 { return Err(ArtifactFailure::NotPrivate); }
                }
                std::fs::remove_file(&path).map_err(refused)?;
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {},
            Err(error) => return Err(refused(error)),
        }
        std::fs::File::open(&self.content).and_then(|directory| directory.sync_all()).map_err(refused)
    }

    /// Returns a store rooted at `root`, creating what it needs.
    ///
    /// # Errors
    ///
    /// Returns [`ArtifactFailure::FilesystemRefused`].
    pub fn open(root: &Path) -> Result<Self, ArtifactFailure> {
        let content = root.join(CONTENT_DIRECTORY);
        std::fs::create_dir_all(&content).map_err(refused)?;
        Ok(Self { content })
    }

    /// Installs everything `source` yields, and returns what it turned out to be.
    ///
    /// The bytes go to a temporary file in the same directory as their
    /// destination, are hashed and counted as they are written, and are
    /// synchronized before the rename that publishes them. So an interruption
    /// at any point leaves either the complete verified artifact or a file
    /// wearing the staging suffix, which nothing addresses and nothing reads.
    ///
    /// A stream failure removes only the UUID-named stage this invocation
    /// created. A process crash is distinguishable from that controlled failure
    /// and may leave a stage for startup reconciliation, but an ordinary
    /// refusal never turns into unbounded retained partial state.
    ///
    /// Content already present is not written twice. The destination is the
    /// digest, so identical bytes are the same artifact however many operations
    /// produce them, and the second producer's temporary file is removed rather
    /// than published over the first.
    ///
    /// # Errors
    ///
    /// Returns [`ArtifactFailure::TooLong`] for a bounded field,
    /// [`ArtifactFailure::ContentTooLong`] past the contract's individual
    /// artifact bound, or [`ArtifactFailure::FilesystemRefused`].
    pub fn install<Source: std::io::Read>(
        &self,
        request: &InstallationRequest,
        source: &mut Source,
    ) -> Result<ArtifactMetadata, ArtifactFailure> {
        self.stage_expected(request, source, None)?.publish()
    }

    /// Installs only the exact content described by validated result metadata.
    /// Length and digest are checked before a digest-named file is published.
    /// The caller reserves capacity and validates command/slot provenance first.
    ///
    /// # Errors
    ///
    /// Refuses noncanonical digests, excessive or mismatched lengths, digest
    /// disagreement and the same filesystem failures as [`Self::install`].
    pub fn install_verified<Source: std::io::Read>(
        &self,
        request: &InstallationRequest,
        source: &mut Source,
        expected_byte_length: u64,
        expected_digest: &str,
    ) -> Result<ArtifactMetadata, ArtifactFailure> {
        self.stage_verified(request, source, expected_byte_length, expected_digest)?.publish()
    }

    /// Verifies content in a private stage without publishing a digest object.
    /// The caller must reserve capacity before invoking this method.
    pub fn stage_verified<Source: std::io::Read>(
        &self,
        request: &InstallationRequest,
        source: &mut Source,
        expected_byte_length: u64,
        expected_digest: &str,
    ) -> Result<StagedArtifact<'_>, ArtifactFailure> {
        if !is_canonical_digest(expected_digest) {
            return Err(ArtifactFailure::DigestNotCanonical);
        }
        self.stage_expected(request, source, Some((expected_byte_length, expected_digest)))
    }

    /// Opens a private writer after the caller has reserved capacity.
    pub fn begin_verified(
        &self,
        request: &InstallationRequest,
        expected_byte_length: u64,
        expected_digest: &str,
    ) -> Result<ArtifactStageWriter<'_>, ArtifactFailure> {
        if !is_canonical_digest(expected_digest) {
            return Err(ArtifactFailure::DigestNotCanonical);
        }
        self.begin_staging(request, Some((expected_byte_length, expected_digest)))
    }

    fn begin_staging(
        &self,
        request: &InstallationRequest,
        expected: Option<(u64, &str)>,
    ) -> Result<ArtifactStageWriter<'_>, ArtifactFailure> {
        request.require_bounded()?;
        let maximum =
            DaemonRuntimeContract::embedded().formula("maximum_individual_artifact_bytes");
        let allowed = expected.map_or(maximum, |(length, _)| length);
        if allowed > maximum {
            return Err(ArtifactFailure::ContentTooLong { actual: allowed, allowed: maximum });
        }
        let path = self.content.join(format!("{}{STAGING_SUFFIX}", uuid::Uuid::new_v4()));
        let file = create_private(&path)?;
        let stage = PrivateStage { path, identity: HandleSnapshot::of(&file)? };
        Ok(ArtifactStageWriter {
            store: self,
            stage,
            file,
            request: request.clone(),
            hasher: Sha256::new(),
            received: 0,
            allowed,
            expected: expected.map(|(length, digest)| (length, digest.to_owned())),
            poisoned: false,
        })
    }

    fn stage_expected<Source: std::io::Read>(
        &self,
        request: &InstallationRequest,
        source: &mut Source,
        expected: Option<(u64, &str)>,
    ) -> Result<StagedArtifact<'_>, ArtifactFailure> {
        let mut writer = self.begin_staging(request, expected)?;
        let mut transfer = vec![0_u8; TRANSFER_BYTES];
        loop {
            let wanted =
                usize::try_from(writer.allowed.saturating_sub(writer.received).saturating_add(1))
                    .unwrap_or(TRANSFER_BYTES)
                    .min(TRANSFER_BYTES);
            let read = source.read(&mut transfer[..wanted]).map_err(refused)?;
            if read == 0 {
                break;
            }
            writer.write_chunk(&transfer[..read])?;
        }
        writer.finish()
    }

    /// Publishes one staged file as the content it turned out to hold.
    fn publish(
        &self,
        staging: &Path,
        content_digest: &str,
        byte_length: u64,
        expected: HandleSnapshot,
    ) -> Result<(), ArtifactFailure> {
        let destination = self.content.join(content_digest);
        if destination.symlink_metadata().is_ok() {
            self.require_existing(&destination, content_digest, byte_length)?;
            std::fs::remove_file(staging).map_err(refused)?;
            return Ok(());
        }
        match publish_staged_no_replace(&self.content, staging, content_digest, expected) {
            Ok(()) => {}
            Err(ArtifactFailure::ContentAlreadyPresent(_)) => {
                self.require_existing(&destination, content_digest, byte_length)?;
                std::fs::remove_file(staging).map_err(refused)?;
            }
            Err(failure) => return Err(failure),
        }
        synchronize_directory(&self.content)
    }

    /// Requires an existing digest-named file to be the verified bytes.
    fn require_existing(
        &self,
        destination: &Path,
        content_digest: &str,
        byte_length: u64,
    ) -> Result<(), ArtifactFailure> {
        let mut existing = open_without_following(destination)?;
        HandleSnapshot::of(&existing)?;
        let (digest, length) = measure(&mut existing)?;
        if length != byte_length {
            return Err(ArtifactFailure::LengthMismatch { actual: length, expected: byte_length });
        }
        if digest != content_digest {
            return Err(ArtifactFailure::DigestMismatch {
                actual: digest,
                expected: content_digest.to_owned(),
            });
        }
        Ok(())
    }

    /// Decides where one canonical structured result goes, and puts it there.
    ///
    /// A result that fits the inline budget travels in the response, because
    /// installing it would make a client fetch two things to learn one. A
    /// larger one is a valid result rather than a failure, so it becomes a
    /// verified artifact under the slot every command reserves for exactly
    /// this: not a refusal, and not a remote address for the client to go and
    /// resolve on its own.
    ///
    /// # Errors
    ///
    /// Returns [`ArtifactFailure::ContentTooLong`] past the contract's
    /// canonical structured-result bound, or whatever installing refuses.
    pub fn place_structured_result(
        &self,
        request: &InstallationRequest,
        canonical: &str,
    ) -> Result<ResultPlacement, ArtifactFailure> {
        let contract = DaemonRuntimeContract::embedded();
        let inline = contract.limit("maximum_inline_machine_result_bytes");
        let largest = contract.limit("maximum_canonical_structured_result_bytes");
        let byte_length = u64::try_from(canonical.len()).unwrap_or(u64::MAX);
        if byte_length <= inline {
            return Ok(ResultPlacement::Inline(canonical.to_owned()));
        }
        if byte_length > largest {
            return Err(ArtifactFailure::ContentTooLong { actual: byte_length, allowed: largest });
        }
        let externalized = InstallationRequest {
            artifact_slot: STRUCTURED_RESULT_SLOT.to_owned(),
            media_type: CANONICAL_JSON_MEDIA_TYPE.to_owned(),
            ..request.clone()
        };
        let metadata = self.install(&externalized, &mut canonical.as_bytes())?;
        Ok(ResultPlacement::Externalized(Box::new(metadata)))
    }

    /// Opens one artifact, verified, on the handle the reading happens through.
    ///
    /// The whole file is read through the handle and checked against the
    /// recorded digest and length before anything is returned. The handle is
    /// then rewound rather than reopened, so whatever the path names afterwards
    /// is irrelevant: the reader is holding the file that was verified, not the
    /// name it answered to.
    ///
    /// # Errors
    ///
    /// Returns [`ArtifactFailure::NoSuchContent`],
    /// [`ArtifactFailure::DigestMismatch`], [`ArtifactFailure::LengthMismatch`],
    /// [`ArtifactFailure::NotPrivate`], or
    /// [`ArtifactFailure::FilesystemRefused`].
    pub fn open_verified(
        &self,
        metadata: &ArtifactMetadata,
    ) -> Result<VerifiedArtifactReader, ArtifactFailure> {
        if !is_canonical_digest(&metadata.content_digest) {
            return Err(ArtifactFailure::DigestNotCanonical);
        }
        let path = self.content.join(&metadata.content_digest);
        if !path.exists() {
            return Err(ArtifactFailure::NoSuchContent(metadata.content_digest.clone()));
        }
        VerifiedArtifactReader::from_file(open_without_following(&path)?, metadata)
    }
}

/// Reads `file` through to its end, returning the digest and length found.
fn measure(file: &mut std::fs::File) -> Result<(String, u64), ArtifactFailure> {
    let mut hasher = Sha256::new();
    let mut byte_length = 0_u64;
    let mut transfer = vec![0_u8; TRANSFER_BYTES];
    loop {
        let read = file.read(&mut transfer).map_err(refused)?;
        if read == 0 {
            return Ok((render(&hasher.finalize()), byte_length));
        }
        byte_length = byte_length.saturating_add(read as u64);
        hasher.update(&transfer[..read]);
    }
}

/// Requires what was read to be what the metadata describes.
fn require_matches(
    metadata: &ArtifactMetadata,
    digest: &str,
    byte_length: u64,
) -> Result<(), ArtifactFailure> {
    if byte_length != metadata.byte_length {
        return Err(ArtifactFailure::LengthMismatch {
            actual: byte_length,
            expected: metadata.byte_length,
        });
    }
    if digest != metadata.content_digest {
        return Err(ArtifactFailure::DigestMismatch {
            actual: digest.to_owned(),
            expected: metadata.content_digest.clone(),
        });
    }
    Ok(())
}

/// One artifact being read, on the handle that was verified.
///
/// Hashing and counting continue as bytes are handed out, and [`Self::finish`]
/// is the only place success is reported. A reader that is dropped without
/// finishing has reported nothing, which is the right answer for a transfer
/// that stopped part way.
#[derive(Debug)]
pub struct VerifiedArtifactReader {
    /// How many bytes have been handed out so far.
    byte_length: u64,
    /// The digest the first pass agreed with.
    expected_digest: String,
    /// The length the first pass agreed with.
    expected_length: u64,
    /// The verified handle, rewound.
    file: std::fs::File,
    /// The second pass, in progress.
    hasher: Sha256,
    /// What the file was when it was opened.
    opened: HandleSnapshot,
}

impl std::io::Read for VerifiedArtifactReader {
    fn read(&mut self, buffer: &mut [u8]) -> std::io::Result<usize> {
        self.read_into(buffer).map_err(|_| std::io::Error::other("artifact read refused"))
    }
}

impl VerifiedArtifactReader {
    fn from_file(
        mut file: std::fs::File,
        metadata: &ArtifactMetadata,
    ) -> Result<Self, ArtifactFailure> {
        let opened = HandleSnapshot::of(&file)?;
        let (digest, byte_length) = measure(&mut file)?;
        require_matches(metadata, &digest, byte_length)?;
        if HandleSnapshot::of(&file)? != opened {
            return Err(ArtifactFailure::HandleMoved);
        }
        file.rewind().map_err(refused)?;
        Ok(Self {
            byte_length: 0,
            expected_digest: metadata.content_digest.clone(),
            expected_length: metadata.byte_length,
            file,
            hasher: Sha256::new(),
            opened,
        })
    }

    /// Returns how many bytes this reader has handed out.
    #[must_use]
    pub fn transferred_bytes(&self) -> u64 {
        self.byte_length
    }

    /// Discards `offset` bytes through the same handle, hashing them.
    ///
    /// A caller resuming a transfer still has to prove the part it already has,
    /// so the prefix is read and hashed rather than sought over. Seeking would
    /// leave the second-pass digest unable to say anything about the bytes that
    /// were skipped, and those are exactly the bytes a resumed transfer is
    /// trusting.
    ///
    /// # Errors
    ///
    /// Returns [`ArtifactFailure::LengthMismatch`] when the file ends before
    /// `offset`, or [`ArtifactFailure::FilesystemRefused`].
    pub fn discard_prefix(&mut self, offset: u64) -> Result<(), ArtifactFailure> {
        let mut transfer = vec![0_u8; TRANSFER_BYTES];
        while self.byte_length < offset {
            let wanted = usize::try_from(offset - self.byte_length).unwrap_or(TRANSFER_BYTES);
            let read =
                self.file.read(&mut transfer[..wanted.min(TRANSFER_BYTES)]).map_err(refused)?;
            if read == 0 {
                return Err(ArtifactFailure::LengthMismatch {
                    actual: self.byte_length,
                    expected: offset,
                });
            }
            self.byte_length = self.byte_length.saturating_add(read as u64);
            self.hasher.update(&transfer[..read]);
        }
        Ok(())
    }

    /// Reads the next bytes into `buffer`, hashing and counting them.
    ///
    /// # Errors
    ///
    /// Returns [`ArtifactFailure::FilesystemRefused`].
    pub fn read_into(&mut self, buffer: &mut [u8]) -> Result<usize, ArtifactFailure> {
        let read = self.file.read(buffer).map_err(refused)?;
        self.byte_length = self.byte_length.saturating_add(read as u64);
        self.hasher.update(&buffer[..read]);
        Ok(read)
    }

    /// Reports whether the transfer was of the artifact it claimed to be.
    ///
    /// Three things have to hold at once: what was actually streamed hashes to
    /// the recorded digest, it was the recorded length, and the handle is still
    /// the file it was when it was opened. A rewrite that kept the length still
    /// changes the digest; one that kept the digest still changes the file's
    /// modification time; and a replacement of the path changes neither,
    /// because this handle never looked at the path again.
    ///
    /// # Errors
    ///
    /// Returns [`ArtifactFailure::DigestMismatch`],
    /// [`ArtifactFailure::LengthMismatch`], or [`ArtifactFailure::HandleMoved`].
    pub fn finish(self) -> Result<(), ArtifactFailure> {
        let streamed = render(&self.hasher.finalize());
        if self.byte_length != self.expected_length {
            return Err(ArtifactFailure::LengthMismatch {
                actual: self.byte_length,
                expected: self.expected_length,
            });
        }
        if streamed != self.expected_digest {
            return Err(ArtifactFailure::DigestMismatch {
                actual: streamed,
                expected: self.expected_digest,
            });
        }
        if HandleSnapshot::of(&self.file)? != self.opened {
            return Err(ArtifactFailure::HandleMoved);
        }
        Ok(())
    }
}

/// The association between an operation slot and the artifact filling it.
///
/// Kept beside the store rather than inside it because the bytes and the fact
/// that an operation produced them are two different durable things: content is
/// addressed by its digest and shared by everything that produced identical
/// bytes, while an association is one operation's claim on one slot.
#[derive(Debug)]
pub struct ArtifactAssociations<'database> {
    /// The database the associations live in.
    database: &'database crate::database::OperationDatabase,
}

impl<'database> ArtifactAssociations<'database> {
    /// Returns the associations held in `database`.
    #[must_use]
    pub fn new(database: &'database crate::database::OperationDatabase) -> Self {
        Self { database }
    }

    /// Records one artifact against the operation slot it fills.
    ///
    /// The blob row and the association commit together, so an association can
    /// never name content the store has no record of. Identical content records
    /// one blob however many operations produce it, which is the same rule the
    /// filesystem side already follows by addressing content with its digest.
    ///
    /// # Errors
    ///
    /// Returns [`ArtifactFailure::FilesystemRefused`] carrying what the
    /// database said.
    pub fn associate(
        &self,
        author_target_identity_digest: &str,
        operation_identifier: &str,
        metadata: &ArtifactMetadata,
        now_unix_milliseconds: u64,
    ) -> Result<(), ArtifactFailure> {
        let connection = self.database.connection();
        let transaction = rusqlite::Transaction::new_unchecked(
            connection,
            rusqlite::TransactionBehavior::Immediate,
        )
        .map_err(database_refused)?;
        let byte_length = i64::try_from(metadata.byte_length).unwrap_or(i64::MAX);
        transaction
            .execute(
                statement("record one artifact's content, once per digest"),
                rusqlite::params![
                    byte_length,
                    metadata.content_digest,
                    i64::try_from(now_unix_milliseconds).unwrap_or(i64::MAX),
                ],
            )
            .map_err(database_refused)?;
        transaction
            .execute(
                statement("associate one artifact with the operation slot it fills"),
                rusqlite::params![
                    metadata.artifact_identifier.as_text(),
                    metadata.artifact_slot,
                    author_target_identity_digest,
                    byte_length,
                    metadata.content_digest,
                    metadata.media_type,
                    operation_identifier,
                ],
            )
            .map_err(database_refused)?;
        transaction.commit().map_err(database_refused)
    }

    /// Returns the artifact one operation slot holds, when it holds one.
    ///
    /// # Errors
    ///
    /// Returns [`ArtifactFailure::FilesystemRefused`] carrying what the
    /// database said, or [`ArtifactFailure::DigestNotCanonical`] for a stored
    /// identifier that is not one.
    pub fn read(
        &self,
        author_target_identity_digest: &str,
        operation_identifier: &str,
        artifact_slot: &str,
    ) -> Result<Option<ArtifactMetadata>, ArtifactFailure> {
        let mut prepared = self
            .database
            .connection()
            .prepare(statement("read the artifact one operation slot holds"))
            .map_err(database_refused)?;
        let row = prepared
            .query_row(
                rusqlite::params![
                    author_target_identity_digest,
                    operation_identifier,
                    artifact_slot
                ],
                |row| {
                    Ok((
                        row.get::<_, String>("artifact_identifier")?,
                        row.get::<_, i64>("byte_length")?,
                        row.get::<_, String>("content_digest")?,
                        row.get::<_, String>("media_type")?,
                    ))
                },
            )
            .optional()
            .map_err(database_refused)?;
        let Some((identifier, byte_length, content_digest, media_type)) = row else {
            return Ok(None);
        };
        Ok(Some(ArtifactMetadata {
            artifact_identifier: ArtifactIdentifier::parse(&identifier)?,
            artifact_slot: artifact_slot.to_owned(),
            byte_length: u64::try_from(byte_length).unwrap_or_default(),
            content_digest,
            descriptor: None,
            media_type,
        }))
    }
}

/// Returns a database refusal as this module's failure.
fn database_refused(failure: rusqlite::Error) -> ArtifactFailure {
    ArtifactFailure::FilesystemRefused(failure.to_string())
}

/// Returns the text of the inventoried statement with `purpose`.
fn statement(purpose: &str) -> &'static str {
    crate::sqlite_statement_inventory::STATEMENTS
        .iter()
        .find(|inventoried| inventoried.purpose == purpose)
        .map(|inventoried| inventoried.text)
        .unwrap_or_else(|| panic!("the inventory holds a statement for {purpose}"))
}
