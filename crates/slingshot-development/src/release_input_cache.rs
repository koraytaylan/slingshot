//! What a release is allowed to build from.
//!
//! A release that resolved anything at build time would be a release nobody can
//! reproduce: the same commit built twice would draw different bytes on the two
//! days. So a release builds from a cache prepared beforehand, and the cache is
//! verified against what this repository declares before a single crate is
//! compiled from it.
//!
//! # Verifying is not the same as trusting
//!
//! What verification establishes is that the cache is the one that was
//! prepared, unchanged, and bounded. Whether the bytes inside it were
//! trustworthy when they were fetched is a separate question this does not
//! answer and does not pretend to.
//!
//! Unchanged is established by walking the cache and digesting what is actually
//! there, not by reading the count the manifest reports about itself. A manifest
//! that described its own cache would establish nothing: whoever changed the
//! cache would change the manifest in the same motion.
//!
//! # A cache is a Cargo home, so it is bounded as one
//!
//! The seven dimensions a supplied Cargo home is bounded in are declared once,
//! in Plan 0008's compatibility manifest, and enforced by that plan's verifier.
//! This consumes both rather than restating either. A second document naming
//! the same limits is a second document that can disagree with the first, and
//! the disagreement would be discovered by a release rather than by a gate.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use sha2::Digest;

use crate::finite_state_machine_compatibility::{SeedLimits, SeedRefusal, verify_seed};

/// Where the declaration lives.
pub const DECLARATION_PATH: &str = "support/release-input-cache.toml";

/// Where the lockfile a release builds from lives.
pub const LOCKFILE_PATH: &str = "Cargo.lock";

/// Where the manifest schema lives.
pub const SCHEMA_PATH: &str = "schemas/release/locked-source-cache.schema.json";

/// What a cache manifest is called inside a cache.
pub const CACHE_MANIFEST: &str = "cache.json";

/// The format a cache manifest declares.
pub const MANIFEST_FORMAT: &str = "slingshot.release-input-cache-manifest/1";

/// How a release resolves its dependencies, and the only way it may.
pub const RESOLUTION: &str = "frozen-offline";

/// Byte separating an entry's path from its digest inside the cache digest.
const DIGEST_SEPARATOR: u8 = 0;

/// What this repository declares a release may build from.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CacheDeclaration {
    /// The format this declaration carries.
    pub format: String,
    /// What a cache must provide.
    pub requires: CacheRequirements,
    /// How a release resolves.
    pub resolution: String,
}

/// What a cache must provide.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CacheRequirements {
    /// Whether every entry carries a checksum.
    pub checksums: bool,
    /// Whether resolution is locked.
    pub locked_resolution: bool,
    /// Whether every dependency comes from a registry.
    pub registry_only: bool,
}

/// One prepared cache, as its manifest describes it.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct CacheManifest {
    /// What the cache's entries digest to.
    pub cache_sha256: String,
    /// How many entries it holds, not counting the manifest.
    pub entries: u64,
    /// The format this manifest declares.
    pub format: String,
    /// What the lockfile it was prepared from digests to.
    pub lock_sha256: String,
    /// How a release using it resolves.
    pub resolution: String,
}

/// Why a cache is refused.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum CacheRefusal {
    /// The manifest could not be read.
    #[error("the cache manifest could not be read: {0}")]
    Unreadable(String),
    /// The manifest declares another format.
    #[error("a cache manifest declares {MANIFEST_FORMAT}, and this declares {0}")]
    ForeignFormat(String),
    /// The cache would let a release resolve something at build time.
    #[error("a release resolves {RESOLUTION}, and this cache says {0}")]
    ResolutionUnacceptable(String),
    /// The cache was prepared from another lockfile.
    #[error("this cache was prepared from another lockfile than the one being built")]
    AnotherLockfile,
    /// The cache holds more, or holds other things, than a Cargo home may.
    #[error("this cache is not a Cargo home a build may be given: {0}")]
    OutsideItsBounds(SeedRefusal),
    /// The cache on disk is not the cache the manifest describes.
    #[error("this cache is not the one that was prepared: {0}")]
    Changed(String),
}

/// What one cache's entries are, and what they digest to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CacheSurvey {
    /// What the entries digest to, together.
    pub digest: String,
    /// How many entries there are.
    pub entries: u64,
}

/// Returns the declaration one text carries.
///
/// # Errors
///
/// Returns [`CacheRefusal::Unreadable`] for a declaration this build cannot
/// read and [`CacheRefusal::ResolutionUnacceptable`] for one that would let a
/// release resolve at build time.
pub fn parse_declaration(text: &str) -> Result<CacheDeclaration, CacheRefusal> {
    let held: CacheDeclaration =
        toml::from_str(text).map_err(|failure| CacheRefusal::Unreadable(failure.to_string()))?;
    if held.resolution != RESOLUTION {
        return Err(CacheRefusal::ResolutionUnacceptable(held.resolution));
    }
    Ok(held)
}

/// Returns what the lockfile under `workspace_root` digests to.
///
/// # Errors
///
/// Returns [`CacheRefusal::Unreadable`] when there is no lockfile to prepare
/// against, because a release with nothing pinned pins nothing.
pub fn lockfile_digest(workspace_root: &Path) -> Result<String, CacheRefusal> {
    let held = std::fs::read(workspace_root.join(LOCKFILE_PATH))
        .map_err(|failure| CacheRefusal::Unreadable(failure.to_string()))?;
    Ok(hex::encode(sha2::Sha256::digest(&held)))
}

/// Returns where a cache keeps its manifest.
#[must_use]
pub fn manifest_path(cache: &Path) -> PathBuf {
    cache.join(CACHE_MANIFEST)
}

/// Returns what is actually in `cache`, leaving out the manifest itself.
///
/// The manifest is left out because it is written from this: counting it would
/// make preparing a cache change the thing it was measuring.
///
/// # Errors
///
/// Returns [`CacheRefusal::Unreadable`] for a cache this build cannot walk.
pub fn survey(cache: &Path) -> Result<CacheSurvey, CacheRefusal> {
    let mut entries = Vec::new();
    collect_entries(cache, cache, &mut entries)?;
    entries.sort();
    let mut digest = sha2::Sha256::new();
    for relative in &entries {
        let held = std::fs::read(cache.join(relative))
            .map_err(|failure| CacheRefusal::Unreadable(failure.to_string()))?;
        digest.update(relative.as_bytes());
        digest.update([DIGEST_SEPARATOR]);
        digest.update(hex::encode(sha2::Sha256::digest(&held)).as_bytes());
    }
    Ok(CacheSurvey { digest: hex::encode(digest.finalize()), entries: entries.len() as u64 })
}

/// Collects every entry path under `directory`, relative to `cache`.
fn collect_entries(
    cache: &Path,
    directory: &Path,
    collected: &mut Vec<String>,
) -> Result<(), CacheRefusal> {
    let listing = std::fs::read_dir(directory)
        .map_err(|failure| CacheRefusal::Unreadable(failure.to_string()))?;
    for entry in listing {
        let entry = entry.map_err(|failure| CacheRefusal::Unreadable(failure.to_string()))?;
        let path = entry.path();
        let kind =
            entry.file_type().map_err(|failure| CacheRefusal::Unreadable(failure.to_string()))?;
        if kind.is_dir() {
            collect_entries(cache, &path, collected)?;
            continue;
        }
        let relative = path.strip_prefix(cache).unwrap_or(&path);
        let Some(named) = relative.to_str() else {
            return Err(CacheRefusal::Unreadable(format!(
                "{} is not a path a manifest can name",
                relative.display()
            )));
        };
        if named == CACHE_MANIFEST {
            continue;
        }
        collected.push(named.to_owned());
    }
    Ok(())
}

/// Writes the manifest describing the cache that was just fetched.
///
/// The finished cache is then put through the same verification a release puts
/// it through, and a cache that does not pass is left without a manifest. A
/// cache nobody may build from is better refused where it is made than where it
/// is used, and a manifest is exactly the thing that would make it look usable.
///
/// # Errors
///
/// Returns [`CacheRefusal`] naming the first thing that stops the cache.
pub fn prepare(
    cache: &Path,
    declaration: &CacheDeclaration,
    limits: &SeedLimits,
    lock_sha256: &str,
) -> Result<CacheManifest, CacheRefusal> {
    let surveyed = survey(cache)?;
    let manifest = CacheManifest {
        cache_sha256: surveyed.digest,
        entries: surveyed.entries,
        format: MANIFEST_FORMAT.to_owned(),
        lock_sha256: lock_sha256.to_owned(),
        resolution: declaration.resolution.clone(),
    };
    let rendered = serde_json::to_string_pretty(&manifest)
        .map_err(|failure| CacheRefusal::Unreadable(failure.to_string()))?;
    std::fs::write(manifest_path(cache), format!("{rendered}\n"))
        .map_err(|failure| CacheRefusal::Unreadable(failure.to_string()))?;
    match verified(cache, declaration, limits, lock_sha256) {
        Ok(held) => Ok(held),
        Err(refusal) => {
            std::fs::remove_file(manifest_path(cache)).ok();
            Err(refusal)
        }
    }
}

/// Requires one cache to be the one that was prepared for this lockfile.
///
/// The bounds are Plan 0008's, checked by Plan 0008's verifier against the
/// limits its manifest declares, so a cache is exactly as bounded as any other
/// Cargo home this repository accepts and is bounded by one authority.
///
/// # Errors
///
/// Returns [`CacheRefusal`] naming the first thing that stops the cache.
pub fn verified(
    cache: &Path,
    declaration: &CacheDeclaration,
    limits: &SeedLimits,
    lock_sha256: &str,
) -> Result<CacheManifest, CacheRefusal> {
    let text = std::fs::read_to_string(manifest_path(cache))
        .map_err(|failure| CacheRefusal::Unreadable(failure.to_string()))?;
    let manifest: CacheManifest = serde_json::from_str(&text)
        .map_err(|failure| CacheRefusal::Unreadable(failure.to_string()))?;
    if manifest.format != MANIFEST_FORMAT {
        return Err(CacheRefusal::ForeignFormat(manifest.format));
    }
    if manifest.resolution != declaration.resolution {
        return Err(CacheRefusal::ResolutionUnacceptable(manifest.resolution));
    }
    if manifest.lock_sha256 != lock_sha256 {
        return Err(CacheRefusal::AnotherLockfile);
    }
    verify_seed(cache, limits).map_err(CacheRefusal::OutsideItsBounds)?;
    let surveyed = survey(cache)?;
    if surveyed.entries == 0 {
        return Err(CacheRefusal::Changed("it holds nothing, so it caches nothing".to_owned()));
    }
    if surveyed.entries != manifest.entries {
        return Err(CacheRefusal::Changed(format!(
            "its manifest counts {} entries and it holds {}",
            manifest.entries, surveyed.entries
        )));
    }
    if surveyed.digest != manifest.cache_sha256 {
        return Err(CacheRefusal::Changed(
            "its entries digest to something other than what its manifest records".to_owned(),
        ));
    }
    Ok(manifest)
}
