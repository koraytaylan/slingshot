//! RustSec advisory database pin.
//!
//! The advisory input is one exact snapshot: a canonical origin, one full
//! commit identifier, and the canonical digest of the tree that commit names.
//! Nothing here reads a clock, and the schema refuses a timestamp, an age, a
//! freshness flag, and a review assertion, because a Git author chooses those
//! values and none of them authenticates anything.
//!
//! The verifier reads only the checkout an environment variable names. It never
//! discovers an ambient cache, never fetches, never repairs, and never advances
//! anything. Every result it produces is labelled an exact snapshot and says
//! nothing at all about whether that snapshot is current.

use std::path::{Path, PathBuf};
use std::process::Command;

use serde::{Deserialize, Serialize};

/// Repository path of the committed pin.
pub const PIN_PATH: &str = "compatibility/rustsec-advisory-database.toml";

/// Environment variable that names the one checkout the verifier reads.
pub const CHECKOUT_VARIABLE: &str = "SLINGSHOT_RUSTSEC_ADVISORY_DATABASE_DIRECTORY";

/// Format identifier the pin must declare.
pub const PIN_FORMAT: &str = "slingshot.rustsec-advisory-database/1";

/// Label every result carries, in place of any freshness claim.
pub const EXACT_SNAPSHOT_LABEL: &str = "exact_snapshot_only";

/// Length of a full Git object identifier, in hexadecimal characters.
const FULL_IDENTIFIER_LENGTH: usize = 40;

/// One exact advisory-database snapshot.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case", deny_unknown_fields)]
pub struct AdvisoryDatabasePin {
    /// Format identifier of the pin.
    pub format: String,
    /// Canonical location the snapshot was taken from.
    pub origin: String,
    /// Full commit identifier of the snapshot.
    pub commit: String,
    /// Canonical digest of the tree that commit names.
    pub tree: String,
}

/// Reason a checkout is not the pinned snapshot.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum PinFailure {
    /// The pin bytes are not a valid pin document.
    #[error("the advisory pin could not be read: {0}")]
    Unreadable(String),
    /// The pin declares a format this build does not implement.
    #[error("the advisory pin declares the format {0}")]
    UnsupportedFormat(String),
    /// A pinned identifier is not a full lowercase hexadecimal identifier.
    #[error("the pinned {field} {value:?} is not a full object identifier")]
    NotAFullIdentifier {
        /// Field that holds the identifier.
        field: &'static str,
        /// Value the field holds.
        value: String,
    },
    /// No checkout was named.
    #[error("no checkout was named; set {CHECKOUT_VARIABLE} to the one to read")]
    NoCheckoutNamed,
    /// The named checkout could not be read as a repository.
    #[error("{path} is not a readable repository: {reason}")]
    NotARepository {
        /// Path of the named checkout.
        path: PathBuf,
        /// Reason the checkout could not be read.
        reason: String,
    },
    /// The checkout does not match the pin.
    #[error("the checkout {field} is {found:?}, not the pinned {expected:?}")]
    Mismatch {
        /// Field that differs.
        field: &'static str,
        /// Value the checkout holds.
        found: String,
        /// Value the pin declares.
        expected: String,
    },
    /// The checkout is not in the state a snapshot must be read from.
    #[error("the checkout is {0}")]
    UnusableState(String),
}

/// One verified snapshot, which says nothing about whether it is current.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub struct VerifiedSnapshot {
    /// Label that keeps the result from being read as a freshness claim.
    pub label: String,
    /// Canonical location the snapshot was taken from.
    pub origin: String,
    /// Full commit identifier of the snapshot.
    pub commit: String,
    /// Canonical digest of the tree that commit names.
    pub tree: String,
}

/// Reports whether a value is a full lowercase hexadecimal object identifier.
#[must_use]
pub fn is_full_identifier(value: &str) -> bool {
    value.len() == FULL_IDENTIFIER_LENGTH
        && value.bytes().all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

/// Reads one pin document.
///
/// # Errors
///
/// Returns [`PinFailure::Unreadable`] when the bytes are not a pin document,
/// including when they carry a timestamp, an age, a freshness flag, a review
/// assertion, or any other field outside the closed schema;
/// [`PinFailure::UnsupportedFormat`] for another format; and
/// [`PinFailure::NotAFullIdentifier`] for a branch, a tag, or a shortened
/// identifier.
pub fn parse_pin(text: &str) -> Result<AdvisoryDatabasePin, PinFailure> {
    let pin: AdvisoryDatabasePin =
        toml::from_str(text).map_err(|failure| PinFailure::Unreadable(failure.to_string()))?;
    if pin.format != PIN_FORMAT {
        return Err(PinFailure::UnsupportedFormat(pin.format));
    }
    for (field, value) in [("commit", &pin.commit), ("tree", &pin.tree)] {
        if !is_full_identifier(value) {
            return Err(PinFailure::NotAFullIdentifier { field, value: value.clone() });
        }
    }
    Ok(pin)
}

/// Runs one read-only Git query against a checkout.
fn query(checkout: &Path, arguments: &[&str]) -> Result<String, PinFailure> {
    let produced = Command::new("git").arg("-C").arg(checkout).args(arguments).output().map_err(
        |failure| PinFailure::NotARepository {
            path: checkout.to_path_buf(),
            reason: failure.to_string(),
        },
    )?;
    if !produced.status.success() {
        return Err(PinFailure::NotARepository {
            path: checkout.to_path_buf(),
            reason: String::from_utf8_lossy(&produced.stderr).trim().to_owned(),
        });
    }
    Ok(String::from_utf8_lossy(&produced.stdout).trim().to_owned())
}

/// Normalizes a remote location so two spellings of one origin compare equal.
#[must_use]
pub fn normalize_origin(origin: &str) -> String {
    origin.trim().trim_end_matches('/').trim_end_matches(".git").to_lowercase()
}

/// Reports the state a checkout must not be in.
fn evaluate_state(checkout: &Path) -> Result<(), PinFailure> {
    if query(checkout, &["rev-parse", "--is-shallow-repository"])? != "false" {
        return Err(PinFailure::UnusableState("shallow".to_owned()));
    }
    let changed = query(checkout, &["status", "--porcelain", "--untracked-files=all"])?;
    if !changed.is_empty() {
        return Err(PinFailure::UnusableState(format!("not clean: {changed}")));
    }
    let attached = Command::new("git")
        .arg("-C")
        .arg(checkout)
        .args(["symbolic-ref", "--quiet", "HEAD"])
        .output()
        .map_err(|failure| PinFailure::NotARepository {
            path: checkout.to_path_buf(),
            reason: failure.to_string(),
        })?;
    if attached.status.success() {
        return Err(PinFailure::UnusableState("not detached at the pinned commit".to_owned()));
    }
    Ok(())
}

/// Reads the exact snapshot one checkout holds.
///
/// # Errors
///
/// Returns [`PinFailure`] when the checkout cannot be read, is shallow, is not
/// clean, or is not detached at a commit.
pub fn read_snapshot(checkout: &Path) -> Result<AdvisoryDatabasePin, PinFailure> {
    evaluate_state(checkout)?;
    let commit = query(checkout, &["rev-parse", "--verify", "HEAD"])?;
    query(checkout, &["cat-file", "-e", &format!("{commit}^{{commit}}")])?;
    let tree = query(checkout, &["rev-parse", &format!("{commit}^{{tree}}")])?;
    let origin = query(checkout, &["remote", "get-url", "origin"])?;
    Ok(AdvisoryDatabasePin {
        format: PIN_FORMAT.to_owned(),
        origin: normalize_origin(&origin),
        commit,
        tree,
    })
}

/// Returns the one checkout the environment names.
///
/// # Errors
///
/// Returns [`PinFailure::NoCheckoutNamed`] when the variable is unset or empty.
/// The verifier never falls back to an ambient cache and never accepts a
/// positional argument in its place.
pub fn named_checkout() -> Result<PathBuf, PinFailure> {
    match std::env::var_os(CHECKOUT_VARIABLE) {
        Some(value) if !value.is_empty() => Ok(PathBuf::from(value)),
        _ => Err(PinFailure::NoCheckoutNamed),
    }
}

/// Verifies that one checkout is exactly the pinned snapshot.
///
/// # Errors
///
/// Returns [`PinFailure::Mismatch`] when the origin, the commit, or the tree
/// differs, and the other variants when the checkout cannot be read or is not
/// in a state a snapshot can be taken from.
pub fn verify(pin: &AdvisoryDatabasePin, checkout: &Path) -> Result<VerifiedSnapshot, PinFailure> {
    let found = read_snapshot(checkout)?;
    let expected = normalize_origin(&pin.origin);
    let compared = [
        ("origin", found.origin.clone(), expected),
        ("commit", found.commit.clone(), pin.commit.clone()),
        ("tree", found.tree.clone(), pin.tree.clone()),
    ];
    for (field, found, expected) in compared {
        if found != expected {
            return Err(PinFailure::Mismatch { field, found, expected });
        }
    }
    Ok(VerifiedSnapshot {
        label: EXACT_SNAPSHOT_LABEL.to_owned(),
        origin: found.origin,
        commit: found.commit,
        tree: found.tree,
    })
}

/// Renders the pin bytes one verified candidate proposes.
///
/// The proposal is bytes for a person to review. It carries no time, no age,
/// and no claim that the snapshot is current, because nothing here can know
/// that.
#[must_use]
pub fn propose_pin(snapshot: &AdvisoryDatabasePin) -> String {
    format!(
        "# Exact advisory-database snapshot. This proposal makes no claim that\n\
         # the snapshot is current: it records only which snapshot it is.\n\
         \n\
         format = \"{}\"\n\
         origin = \"{}\"\n\
         commit = \"{}\"\n\
         tree = \"{}\"\n",
        snapshot.format, snapshot.origin, snapshot.commit, snapshot.tree
    )
}
