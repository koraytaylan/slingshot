//! What an operator receives, and what is refused before they receive it.
//!
//! A release is the archive somebody downloads, not the workspace tests that
//! produced it. So everything here is about the archive as an untrusted object:
//! what it may contain, how large each part of it may be, and which shapes are
//! refused before a single entry is written to a filesystem.
//!
//! # Refused before extraction, not during
//!
//! An entry name that escapes its directory, a link that points anywhere it
//! likes, a device node, a duplicate that silently replaces what came before -
//! each of those does its damage at the moment it is written. Checking them
//! while extracting means checking some of them after the damage. So the whole
//! membership is surveyed first, from the archive alone, and extraction happens
//! only once nothing is left to refuse.
//!
//! Case-fold collisions are refused too, because two names that differ only in
//! case are one file on two of the three supported platforms, and which of them
//! survives depends on the order they happened to be written in.
//!
//! # Bytes that do not depend on the machine
//!
//! Two builds of one revision produce one archive or the contract is not being
//! kept. Archive metadata is normalized rather than inherited: an ambient
//! locale, timezone, user, group, umask, source path, or filesystem enumeration
//! order is a property of a machine, and a release that carried any of them
//! would be a release nobody else could reproduce.

use std::collections::{BTreeMap, BTreeSet};

use serde::Deserialize;

/// What the checksum manifest inside every archive is called.
pub const CHECKSUM_MANIFEST: &str = "SHA256SUMS";

/// The separator a checksum manifest writes between a digest and a name.
pub const CHECKSUM_SEPARATOR: &str = "  ";

/// How many characters a digest is written in.
pub const DIGEST_CHARACTERS: usize = 64;

/// The largest an archive may be, compressed.
///
/// A release archive holds one executable, one licence, and one small manifest.
/// A bound generous enough for a debug build with symbols is still far below
/// anything that would exhaust a machine reading it.
pub const MAXIMUM_ARCHIVE_BYTES: u64 = 268_435_456;

/// The largest an archive may be once decoded.
///
/// Separate from the compressed bound on purpose: the ratio between them is
/// exactly what an archive designed to exhaust a reader exploits.
pub const MAXIMUM_DECODED_BYTES: u64 = 1_073_741_824;

/// The largest any single entry may be once decoded.
pub const MAXIMUM_ENTRY_BYTES: u64 = 1_073_741_824;

/// How many entries an archive may hold.
pub const MAXIMUM_ENTRIES: usize = 16;

/// How many bytes one entry name may hold.
pub const MAXIMUM_ENTRY_NAME_BYTES: usize = 255;

/// What kind of thing one archive entry is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EntryKind {
    /// An ordinary file.
    OrdinaryFile,
    /// A directory.
    Directory,
    /// A symbolic or hard link.
    Link,
    /// A device, socket, or named pipe.
    Special,
}

/// One entry an archive claims to hold.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ArchiveEntry {
    /// How many bytes it holds once decoded.
    pub decoded_bytes: u64,
    /// What kind of thing it is.
    pub kind: EntryKind,
    /// The name it carries.
    pub name: String,
}

/// Why an archive is refused before anything is extracted from it.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ArchiveRefusal {
    /// The archive or a document beside it could not be read.
    #[error("this archive could not be read: {0}")]
    Unreadable(String),
    /// An entry name would escape the directory it is extracted into.
    #[error("{0} would be written outside the directory it is extracted into")]
    NameEscapes(String),
    /// An entry name is not one this contract admits.
    #[error("{named} is not a name a release archive entry may carry: {reason}")]
    NameUnacceptable {
        /// What it is called.
        named: String,
        /// Why it is not admitted.
        reason: &'static str,
    },
    /// Two entries are one file on a platform that folds case.
    #[error("{first} and {second} are one file where names fold case")]
    NamesCollide {
        /// The first name.
        first: String,
        /// The second.
        second: String,
    },
    /// An entry is not an ordinary file.
    #[error("{named} is a {kind}, and a release archive holds ordinary files")]
    EntryNotOrdinary {
        /// What kind it is.
        kind: &'static str,
        /// What it is called.
        named: String,
    },
    /// The archive holds something the platform row does not declare.
    #[error("{0} is in this archive and the supported platform row declares no such member")]
    MemberUndeclared(String),
    /// The archive is missing something the platform row declares.
    #[error("{0} is declared by the supported platform row and is not in this archive")]
    MemberMissing(String),
    /// The archive is beyond one of its bounds.
    #[error("{what} holds {held} and a release archive allows {allowed}")]
    BeyondBounds {
        /// What is allowed.
        allowed: u64,
        /// What it holds.
        held: u64,
        /// Which bound.
        what: &'static str,
    },
    /// The checksum manifest is not the shape one is.
    #[error("the checksum manifest is not one this contract reads: {0}")]
    ManifestUnacceptable(String),
    /// The checksum manifest and the archive disagree.
    #[error("the checksum manifest names {expected} for {named} and the archive holds {held}")]
    ChecksumDrift {
        /// What the manifest names.
        expected: String,
        /// What the archive holds.
        held: String,
        /// Which member.
        named: String,
    },
    /// The evidence manifest does not bind what it has to bind.
    #[error("this evidence does not bind {0}")]
    EvidenceUnbound(&'static str),
    /// The evidence manifest binds something other than this release.
    #[error("this evidence binds {field} {held}, and this release is {expected}")]
    EvidenceDrift {
        /// What this release is.
        expected: String,
        /// Which field.
        field: &'static str,
        /// What the evidence binds.
        held: String,
    },
}

/// Requires one entry name to be one a release archive may carry.
///
/// # Errors
///
/// Returns [`ArchiveRefusal::NameEscapes`] for a name that would be written
/// outside its directory and [`ArchiveRefusal::NameUnacceptable`] for one this
/// contract does not admit at all.
pub fn require_name_admissible(named: &str) -> Result<(), ArchiveRefusal> {
    let unacceptable =
        |reason| ArchiveRefusal::NameUnacceptable { named: named.to_owned(), reason };
    if named.is_empty() {
        return Err(unacceptable("it is empty"));
    }
    if named.len() > MAXIMUM_ENTRY_NAME_BYTES {
        return Err(unacceptable("it is longer than a name may be"));
    }
    if named.starts_with('/') || named.contains(':') {
        return Err(ArchiveRefusal::NameEscapes(named.to_owned()));
    }
    if named.split('/').any(|segment| segment == ".." || segment == ".") {
        return Err(ArchiveRefusal::NameEscapes(named.to_owned()));
    }
    if named.contains('\\') {
        return Err(unacceptable("a backslash is a separator on one supported platform"));
    }
    if named.chars().any(|held| held.is_control()) {
        return Err(unacceptable("it carries a control character"));
    }
    if named.contains('/') {
        return Err(unacceptable("a release archive is flat"));
    }
    Ok(())
}

/// Requires no two entry names to be one file where names fold case.
fn require_no_case_collision(entries: &[ArchiveEntry]) -> Result<(), ArchiveRefusal> {
    let mut folded: BTreeMap<String, String> = BTreeMap::new();
    for entry in entries {
        let key = entry.name.to_lowercase();
        if let Some(first) = folded.insert(key, entry.name.clone()) {
            return Err(ArchiveRefusal::NamesCollide { first, second: entry.name.clone() });
        }
    }
    Ok(())
}

/// Returns what one entry kind is called in a diagnostic.
fn kind_named(kind: EntryKind) -> &'static str {
    match kind {
        EntryKind::OrdinaryFile => "ordinary file",
        EntryKind::Directory => "directory",
        EntryKind::Link => "link",
        EntryKind::Special => "device, socket, or named pipe",
    }
}

/// Requires one surveyed archive to be one this contract admits.
///
/// Every rule is applied to the survey rather than to a filesystem, so nothing
/// has been written anywhere by the time an archive is refused.
///
/// # Errors
///
/// Returns [`ArchiveRefusal`] naming the first rule the archive breaks.
pub fn require_admissible(
    entries: &[ArchiveEntry],
    declared_members: &[String],
    compressed_bytes: u64,
) -> Result<(), ArchiveRefusal> {
    if compressed_bytes > MAXIMUM_ARCHIVE_BYTES {
        return Err(ArchiveRefusal::BeyondBounds {
            allowed: MAXIMUM_ARCHIVE_BYTES,
            held: compressed_bytes,
            what: "this archive",
        });
    }
    if entries.len() > MAXIMUM_ENTRIES {
        return Err(ArchiveRefusal::BeyondBounds {
            allowed: MAXIMUM_ENTRIES as u64,
            held: entries.len() as u64,
            what: "this archive's membership",
        });
    }
    let mut decoded = 0_u64;
    for entry in entries {
        require_name_admissible(&entry.name)?;
        if entry.kind != EntryKind::OrdinaryFile {
            return Err(ArchiveRefusal::EntryNotOrdinary {
                kind: kind_named(entry.kind),
                named: entry.name.clone(),
            });
        }
        if entry.decoded_bytes > MAXIMUM_ENTRY_BYTES {
            return Err(ArchiveRefusal::BeyondBounds {
                allowed: MAXIMUM_ENTRY_BYTES,
                held: entry.decoded_bytes,
                what: "one entry",
            });
        }
        decoded = decoded.saturating_add(entry.decoded_bytes);
    }
    if decoded > MAXIMUM_DECODED_BYTES {
        return Err(ArchiveRefusal::BeyondBounds {
            allowed: MAXIMUM_DECODED_BYTES,
            held: decoded,
            what: "this archive decoded",
        });
    }
    require_no_case_collision(entries)?;
    require_exact_membership(entries, declared_members)
}

/// Requires the archive to hold exactly what the platform row declares.
fn require_exact_membership(
    entries: &[ArchiveEntry],
    declared_members: &[String],
) -> Result<(), ArchiveRefusal> {
    let held: BTreeSet<&str> = entries.iter().map(|entry| entry.name.as_str()).collect();
    if held.len() != entries.len() {
        let repeated = entries
            .iter()
            .find(|entry| entries.iter().filter(|other| other.name == entry.name).count() > 1)
            .map(|entry| entry.name.clone())
            .unwrap_or_default();
        return Err(ArchiveRefusal::NamesCollide { first: repeated.clone(), second: repeated });
    }
    for declared in declared_members {
        if !held.contains(declared.as_str()) {
            return Err(ArchiveRefusal::MemberMissing(declared.clone()));
        }
    }
    for name in held {
        if !declared_members.iter().any(|declared| declared == name) {
            return Err(ArchiveRefusal::MemberUndeclared(name.to_owned()));
        }
    }
    Ok(())
}

/// Returns the checksum manifest one set of members produces.
///
/// Ascending by name, one line each, with the digest first. The order is fixed
/// here rather than taken from a filesystem, because the order a directory
/// enumerates in is a property of that filesystem.
#[must_use]
pub fn render_checksum_manifest(members: &BTreeMap<String, String>) -> String {
    let mut rendered = String::new();
    for (name, digest) in members {
        rendered.push_str(digest);
        rendered.push_str(CHECKSUM_SEPARATOR);
        rendered.push_str(name);
        rendered.push('\n');
    }
    rendered
}

/// Returns the members one checksum manifest names.
///
/// # Errors
///
/// Returns [`ArchiveRefusal::ManifestUnacceptable`] for a manifest whose lines
/// are not exactly one digest, one separator, and one admissible name, or whose
/// names do not ascend.
pub fn parse_checksum_manifest(text: &str) -> Result<BTreeMap<String, String>, ArchiveRefusal> {
    let mut held = BTreeMap::new();
    let mut previous: Option<String> = None;
    for line in text.lines() {
        let (digest, name) = line
            .split_once(CHECKSUM_SEPARATOR)
            .ok_or_else(|| ArchiveRefusal::ManifestUnacceptable(line.to_owned()))?;
        if digest.len() != DIGEST_CHARACTERS
            || !digest.chars().all(|held| held.is_ascii_hexdigit() && !held.is_uppercase())
        {
            return Err(ArchiveRefusal::ManifestUnacceptable(digest.to_owned()));
        }
        require_name_admissible(name)?;
        if let Some(previous) = &previous
            && previous.as_str() >= name
        {
            return Err(ArchiveRefusal::ManifestUnacceptable(format!(
                "{previous} is not before {name}"
            )));
        }
        previous = Some(name.to_owned());
        held.insert(name.to_owned(), digest.to_owned());
    }
    if held.is_empty() {
        return Err(ArchiveRefusal::ManifestUnacceptable("it names nothing".to_owned()));
    }
    Ok(held)
}

/// Requires every member's digest to be the one the manifest names.
///
/// # Errors
///
/// Returns [`ArchiveRefusal::ChecksumDrift`] for the first member whose bytes
/// digest to something else, and [`ArchiveRefusal::MemberMissing`] for one the
/// manifest names and the archive does not hold.
pub fn require_checksums(
    manifest: &BTreeMap<String, String>,
    observed: &BTreeMap<String, String>,
) -> Result<(), ArchiveRefusal> {
    for (named, expected) in manifest {
        let held =
            observed.get(named).ok_or_else(|| ArchiveRefusal::MemberMissing(named.clone()))?;
        if held != expected {
            return Err(ArchiveRefusal::ChecksumDrift {
                expected: expected.clone(),
                held: held.clone(),
                named: named.clone(),
            });
        }
    }
    Ok(())
}

/// What one native row's evidence binds.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, serde::Serialize)]
#[serde(rename_all = "kebab-case", deny_unknown_fields)]
pub struct EvidenceManifest {
    /// The archive this evidence is about, and its digest.
    pub archive: String,
    /// What the archive's bytes digest to.
    pub archive_sha256: String,
    /// What the release input cache it built from digests to.
    pub cache_sha256: String,
    /// The format this manifest declares.
    pub format: String,
    /// The workflow run that produced it.
    pub provider_run: String,
    /// The RustSec owner-review record this release is bound to.
    pub rustsec_review_record_sha256: String,
    /// The exact source commit it was built from.
    pub source_commit: String,
    /// The exact source tree that commit names.
    pub source_tree: String,
    /// The Rust toolchain it was built with.
    pub toolchain: String,
    /// The abstract target row it is.
    pub triple: String,
}

/// The format an evidence manifest declares.
pub const EVIDENCE_FORMAT: &str = "slingshot.release-evidence/1";

/// How many bindings a release's evidence must carry.
const REQUIRED_BINDINGS: usize = 7;

/// How many of them describe the release being verified rather than the build.
const COMPARED_BINDINGS: usize = 3;

/// Returns the evidence one document carries.
///
/// # Errors
///
/// Returns [`ArchiveRefusal::Unreadable`] for a document this cannot read and
/// [`ArchiveRefusal::EvidenceUnbound`] for one that leaves a required binding
/// empty.
pub fn parse_evidence(text: &str) -> Result<EvidenceManifest, ArchiveRefusal> {
    let held: EvidenceManifest =
        toml::from_str(text).map_err(|failure| ArchiveRefusal::Unreadable(failure.to_string()))?;
    if held.format != EVIDENCE_FORMAT {
        return Err(ArchiveRefusal::EvidenceDrift {
            expected: EVIDENCE_FORMAT.to_owned(),
            field: "format",
            held: held.format,
        });
    }
    let bound: [(&'static str, &str); REQUIRED_BINDINGS] = [
        ("an archive", &held.archive),
        ("an archive digest", &held.archive_sha256),
        ("a cache digest", &held.cache_sha256),
        ("a provider run", &held.provider_run),
        ("a RustSec review record", &held.rustsec_review_record_sha256),
        ("a source commit", &held.source_commit),
        ("a source tree", &held.source_tree),
    ];
    for (what, value) in bound {
        if value.trim().is_empty() {
            return Err(ArchiveRefusal::EvidenceUnbound(what));
        }
    }
    Ok(held)
}

/// Requires evidence to be about this row, this source, and this cache.
///
/// # Errors
///
/// Returns [`ArchiveRefusal::EvidenceDrift`] naming the first binding that
/// describes something other than the release being verified.
pub fn require_evidence_binds(
    evidence: &EvidenceManifest,
    triple: &str,
    source_commit: &str,
    cache_sha256: &str,
) -> Result<(), ArchiveRefusal> {
    let compared: [(&'static str, &str, &str); COMPARED_BINDINGS] = [
        ("triple", triple, &evidence.triple),
        ("source commit", source_commit, &evidence.source_commit),
        ("cache digest", cache_sha256, &evidence.cache_sha256),
    ];
    for (field, expected, held) in compared {
        if expected != held {
            return Err(ArchiveRefusal::EvidenceDrift {
                expected: expected.to_owned(),
                field,
                held: held.to_owned(),
            });
        }
    }
    Ok(())
}

/// The archive profile a `tar.gz` row declares.
pub const TAR_PROFILE: &str = "tar.gz";

/// The archive profile a `zip` row declares.
pub const ZIP_PROFILE: &str = "zip";

/// The instant every archive entry records, so no clock decides bytes.
const FIXED_MODIFIED_SECONDS: u64 = 0;

/// The earliest year a zip entry can record, which is what every one records.
///
/// A zip timestamp cannot express the epoch a tape archive uses, so the two
/// profiles fix different instants. What matters is that each is fixed.
const EARLIEST_ZIP_YEAR: u16 = 1980;

/// The first month, day, hour, minute, and second a zip entry records.
const FIRST_UNIT: u8 = 1;

/// The zero a zip entry's hour, minute, and second record.
const ZERO_UNIT: u8 = 0;

/// The identity every archive entry records, so no account decides bytes.
const FIXED_OWNER: u64 = 0;

/// The permissions an executable entry records.
const EXECUTABLE_MODE: u32 = 0o755;

/// The permissions an ordinary entry records.
const ORDINARY_MODE: u32 = 0o644;

/// Returns what one archive actually holds, without extracting any of it.
///
/// The entries are read from the archive's own membership, and the decoded size
/// is accumulated as it goes, so an archive that would decode to more than a
/// release archive may is refused while it is still being read rather than
/// after it has been written somewhere.
///
/// # Errors
///
/// Returns [`ArchiveRefusal::Unreadable`] for an archive this cannot read and
/// [`ArchiveRefusal::BeyondBounds`] for one whose decoded size passes its bound
/// during the survey.
pub fn survey_archive(
    path: &std::path::Path,
    profile: &str,
) -> Result<Vec<ArchiveEntry>, ArchiveRefusal> {
    match profile {
        TAR_PROFILE => survey_tar(path),
        ZIP_PROFILE => survey_zip(path),
        other => Err(ArchiveRefusal::Unreadable(format!("{other} is not an archive profile"))),
    }
}

/// Returns what one compressed tape archive holds.
fn survey_tar(path: &std::path::Path) -> Result<Vec<ArchiveEntry>, ArchiveRefusal> {
    let unreadable = |failure: std::io::Error| ArchiveRefusal::Unreadable(failure.to_string());
    let file = std::fs::File::open(path).map_err(unreadable)?;
    let decoder = flate2::read::GzDecoder::new(file);
    let mut archive = tar::Archive::new(decoder);
    let mut held = Vec::new();
    let mut decoded = 0_u64;
    for entry in archive.entries().map_err(unreadable)? {
        let entry = entry.map_err(unreadable)?;
        let header = entry.header();
        let kind = match header.entry_type() {
            tar::EntryType::Regular => EntryKind::OrdinaryFile,
            tar::EntryType::Directory => EntryKind::Directory,
            tar::EntryType::Symlink | tar::EntryType::Link => EntryKind::Link,
            _ => EntryKind::Special,
        };
        let size = header.size().map_err(unreadable)?;
        decoded = decoded.saturating_add(size);
        if decoded > MAXIMUM_DECODED_BYTES {
            return Err(ArchiveRefusal::BeyondBounds {
                allowed: MAXIMUM_DECODED_BYTES,
                held: decoded,
                what: "this archive decoded",
            });
        }
        let name = entry.path().map_err(unreadable)?.to_string_lossy().to_string();
        held.push(ArchiveEntry { decoded_bytes: size, kind, name });
    }
    Ok(held)
}

/// Returns what one zip archive holds.
fn survey_zip(path: &std::path::Path) -> Result<Vec<ArchiveEntry>, ArchiveRefusal> {
    let file = std::fs::File::open(path)
        .map_err(|failure| ArchiveRefusal::Unreadable(failure.to_string()))?;
    let mut archive = zip::ZipArchive::new(file)
        .map_err(|failure| ArchiveRefusal::Unreadable(failure.to_string()))?;
    let mut held = Vec::new();
    let mut decoded = 0_u64;
    for index in 0..archive.len() {
        let entry = archive
            .by_index(index)
            .map_err(|failure| ArchiveRefusal::Unreadable(failure.to_string()))?;
        let size = entry.size();
        decoded = decoded.saturating_add(size);
        if decoded > MAXIMUM_DECODED_BYTES {
            return Err(ArchiveRefusal::BeyondBounds {
                allowed: MAXIMUM_DECODED_BYTES,
                held: decoded,
                what: "this archive decoded",
            });
        }
        let kind = if entry.is_dir() { EntryKind::Directory } else { EntryKind::OrdinaryFile };
        held.push(ArchiveEntry { decoded_bytes: size, kind, name: entry.name().to_owned() });
    }
    Ok(held)
}

/// Writes one release archive whose bytes depend on nothing but its members.
///
/// Every piece of metadata an archive can carry that a machine would otherwise
/// supply is fixed here: the modification time, the owner, the group, the
/// names of both, and the permissions. What is left is the members and their
/// order, and the order is the one the platform row declares.
///
/// # Errors
///
/// Returns [`ArchiveRefusal::Unreadable`] when a member cannot be read or the
/// archive cannot be written.
pub fn write_archive(
    path: &std::path::Path,
    profile: &str,
    members: &BTreeMap<String, Vec<u8>>,
    executable: &str,
) -> Result<(), ArchiveRefusal> {
    match profile {
        TAR_PROFILE => write_tar(path, members, executable),
        ZIP_PROFILE => write_zip(path, members, executable),
        other => Err(ArchiveRefusal::Unreadable(format!("{other} is not an archive profile"))),
    }
}

/// Returns the permissions one member records.
fn mode_for(name: &str, executable: &str) -> u32 {
    if name == executable { EXECUTABLE_MODE } else { ORDINARY_MODE }
}

/// Writes one compressed tape archive.
fn write_tar(
    path: &std::path::Path,
    members: &BTreeMap<String, Vec<u8>>,
    executable: &str,
) -> Result<(), ArchiveRefusal> {
    let unreadable = |failure: std::io::Error| ArchiveRefusal::Unreadable(failure.to_string());
    let file = std::fs::File::create(path).map_err(unreadable)?;
    let encoder = flate2::write::GzEncoder::new(file, flate2::Compression::default());
    let mut builder = tar::Builder::new(encoder);
    for (name, bytes) in members {
        let mut header = tar::Header::new_gnu();
        header.set_size(bytes.len() as u64);
        header.set_mode(mode_for(name, executable));
        header.set_mtime(FIXED_MODIFIED_SECONDS);
        header.set_uid(FIXED_OWNER);
        header.set_gid(FIXED_OWNER);
        header.set_entry_type(tar::EntryType::Regular);
        header.set_cksum();
        builder.append_data(&mut header, name, bytes.as_slice()).map_err(unreadable)?;
    }
    builder.into_inner().map_err(unreadable)?.finish().map_err(unreadable)?;
    Ok(())
}

/// Writes one zip archive.
fn write_zip(
    path: &std::path::Path,
    members: &BTreeMap<String, Vec<u8>>,
    executable: &str,
) -> Result<(), ArchiveRefusal> {
    use std::io::Write as _;

    let unreadable = |failure: std::io::Error| ArchiveRefusal::Unreadable(failure.to_string());
    let file = std::fs::File::create(path).map_err(unreadable)?;
    let mut writer = zip::ZipWriter::new(file);
    for (name, bytes) in members {
        let options = zip::write::SimpleFileOptions::default()
            .compression_method(zip::CompressionMethod::Deflated)
            .unix_permissions(mode_for(name, executable))
            .last_modified_time(
                zip::DateTime::from_date_and_time(
                    EARLIEST_ZIP_YEAR,
                    FIRST_UNIT,
                    FIRST_UNIT,
                    ZERO_UNIT,
                    ZERO_UNIT,
                    ZERO_UNIT,
                )
                .map_err(|failure| ArchiveRefusal::Unreadable(failure.to_string()))?,
            );
        writer
            .start_file(name, options)
            .map_err(|failure| ArchiveRefusal::Unreadable(failure.to_string()))?;
        writer.write_all(bytes).map_err(unreadable)?;
    }
    writer.finish().map_err(|failure| ArchiveRefusal::Unreadable(failure.to_string()))?;
    Ok(())
}
