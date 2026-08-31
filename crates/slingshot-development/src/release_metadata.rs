//! The legal declaration a release ships under.
//!
//! A license is a statement about what somebody else may do with this
//! software, and the only person who can make it is the person who owns the
//! copyright. So nothing here infers, defaults to, or repairs one: the
//! declaration is committed, the material it refers to is committed beside it
//! with the digest of its exact bytes, and every packaging command refuses
//! before compiling a binary when the two disagree.
//!
//! # One declaration, one file, one address
//!
//! Cargo will take either an expression or a file. Naming both lets two answers
//! exist for one question, so exactly one is declared and a second is refused.
//! The archive layout every supported platform declares carries exactly one
//! `LICENSE` member, so a dual license is one document holding both texts
//! rather than two files an archive has no room for. And the repository address
//! is copied from the validated automation authority rather than resolved
//! again, because two documents that each resolve an address are two documents
//! that can disagree about it.

use std::path::Path;

use serde::Deserialize;
use sha2::Digest as _;

/// Where the declaration lives.
pub const METADATA_PATH: &str = "support/release-metadata.toml";

/// The format this document declares.
pub const METADATA_FORMAT: &str = "slingshot.release-metadata/1";

/// The manifest the workspace declares its packages in.
pub const WORKSPACE_MANIFEST: &str = "Cargo.toml";

/// Values a declaration may not be filled in with.
pub const PLACEHOLDERS: &[&str] =
    &["TBD", "UNLICENSED", "NOASSERTION", "your-license-here", "CHANGEME"];

/// What the owner declared a release ships under.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "kebab-case", deny_unknown_fields)]
pub struct ReleaseMetadata {
    /// The format this document declares.
    pub format: String,
    /// The license and the material it refers to.
    pub license: LicenseDeclaration,
    /// Which repository this is, copied from the automation authority.
    pub repository: RepositoryReference,
    /// What every workspace member does about publishing.
    pub packages: PackagePolicy,
}

/// The license and the material it refers to.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "kebab-case", deny_unknown_fields)]
pub struct LicenseDeclaration {
    /// The SPDX expression Cargo declares.
    pub expression: String,
    /// The repository-relative material that expression refers to.
    pub material: String,
    /// How many bytes that material holds.
    pub material_bytes: u64,
    /// What those bytes digest to.
    pub material_sha256: String,
}

/// Which repository this is, copied from the automation authority.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "kebab-case", deny_unknown_fields)]
pub struct RepositoryReference {
    /// The canonical address.
    pub canonical_address: String,
    /// The immutable numeric identity of the account.
    pub owner_identifier: u64,
}

/// What every workspace member does about publishing.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "kebab-case", deny_unknown_fields)]
pub struct PackagePolicy {
    /// Whether any member is publishable.
    pub publish: bool,
}

/// Why release metadata is refused.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum MetadataRefusal {
    /// The document could not be read.
    #[error("the release metadata could not be read: {0}")]
    Unreadable(String),
    /// It declares another format.
    #[error("release metadata declares {METADATA_FORMAT}, and this declares {0}")]
    ForeignFormat(String),
    /// A value is a placeholder rather than a choice.
    #[error("{field} holds {held}, which is a placeholder rather than a declaration")]
    Placeholder {
        /// Which field.
        field: &'static str,
        /// What it holds.
        held: String,
    },
    /// A value is absent where one is required.
    #[error("{0} is empty, and a release ships under something rather than nothing")]
    Absent(&'static str),
    /// The material path would leave the repository.
    #[error("{0} is not a safe repository-relative path")]
    MaterialPathUnsafe(String),
    /// The material is not the bytes the declaration names.
    #[error("{field} says {expected} and the material holds {held}")]
    MaterialDrift {
        /// What the declaration says.
        expected: String,
        /// Which field.
        field: &'static str,
        /// What the material holds.
        held: String,
    },
    /// Cargo and the declaration disagree.
    #[error("the workspace manifest declares {held} and the release metadata declares {expected}")]
    CargoDrift {
        /// What the release metadata declares.
        expected: String,
        /// What Cargo declares.
        held: String,
    },
    /// Cargo declares a license two ways at once.
    #[error("the workspace manifest declares both a license expression and a license file")]
    CargoDeclaredTwice,
    /// A package would be published.
    #[error("{0} is publishable, and an accidental publish cannot be taken back")]
    PackagePublishable(String),
    /// The address is not the one the automation authority validated.
    #[error("the automation authority says {expected} and the release metadata says {held}")]
    AddressDrift {
        /// What the authority says.
        expected: String,
        /// What this says.
        held: String,
    },
}

/// Returns the metadata one document carries.
///
/// # Errors
///
/// Returns [`MetadataRefusal`] naming the first thing that stops it being a
/// declaration this build may ship under.
pub fn parse_metadata(text: &str) -> Result<ReleaseMetadata, MetadataRefusal> {
    let held: ReleaseMetadata =
        toml::from_str(text).map_err(|failure| MetadataRefusal::Unreadable(failure.to_string()))?;
    if held.format != METADATA_FORMAT {
        return Err(MetadataRefusal::ForeignFormat(held.format));
    }
    require_stated("license expression", &held.license.expression)?;
    require_stated("license material", &held.license.material)?;
    require_safe_material_path(&held.license.material)?;
    Ok(held)
}

/// Requires one field to say something that is not a placeholder.
fn require_stated(field: &'static str, held: &str) -> Result<(), MetadataRefusal> {
    if held.trim().is_empty() {
        return Err(MetadataRefusal::Absent(field));
    }
    let placeholder = PLACEHOLDERS
        .iter()
        .find(|candidate| held.eq_ignore_ascii_case(candidate) || held.contains(*candidate));
    match placeholder {
        None => Ok(()),
        Some(_) => Err(MetadataRefusal::Placeholder { field, held: held.to_owned() }),
    }
}

/// Requires the material to be a path inside the repository.
fn require_safe_material_path(named: &str) -> Result<(), MetadataRefusal> {
    let unsafe_shape = named.starts_with('/')
        || named.starts_with('~')
        || named.contains("..")
        || named.contains('\\')
        || named.contains(':');
    if unsafe_shape {
        return Err(MetadataRefusal::MaterialPathUnsafe(named.to_owned()));
    }
    Ok(())
}

/// Requires the committed material to be exactly the bytes declared.
///
/// # Errors
///
/// Returns [`MetadataRefusal::Unreadable`] when the material is absent and
/// [`MetadataRefusal::MaterialDrift`] when its size or digest differs.
pub fn require_material(
    metadata: &ReleaseMetadata,
    workspace_root: &Path,
) -> Result<(), MetadataRefusal> {
    let held = std::fs::read(workspace_root.join(&metadata.license.material))
        .map_err(|failure| MetadataRefusal::Unreadable(failure.to_string()))?;
    let bytes = held.len() as u64;
    if bytes != metadata.license.material_bytes {
        return Err(MetadataRefusal::MaterialDrift {
            expected: metadata.license.material_bytes.to_string(),
            field: "material-bytes",
            held: bytes.to_string(),
        });
    }
    let digest = hex::encode(sha2::Sha256::digest(&held));
    if digest != metadata.license.material_sha256 {
        return Err(MetadataRefusal::MaterialDrift {
            expected: metadata.license.material_sha256.clone(),
            field: "material-sha256",
            held: digest,
        });
    }
    Ok(())
}

/// Requires the workspace manifest to declare exactly what the metadata does.
///
/// # Errors
///
/// Returns [`MetadataRefusal::CargoDeclaredTwice`] when both a license
/// expression and a license file are declared, and
/// [`MetadataRefusal::CargoDrift`] when the expression differs.
pub fn require_workspace_agreement(
    metadata: &ReleaseMetadata,
    manifest: &str,
) -> Result<(), MetadataRefusal> {
    let held: toml::Value = toml::from_str(manifest)
        .map_err(|failure| MetadataRefusal::Unreadable(failure.to_string()))?;
    let package = &held["workspace"]["package"];
    if package.get("license-file").is_some() && package.get("license").is_some() {
        return Err(MetadataRefusal::CargoDeclaredTwice);
    }
    let declared = package.get("license").and_then(toml::Value::as_str).unwrap_or_default();
    if declared != metadata.license.expression {
        return Err(MetadataRefusal::CargoDrift {
            expected: metadata.license.expression.clone(),
            held: declared.to_owned(),
        });
    }
    let publishable = package.get("publish").and_then(toml::Value::as_bool).unwrap_or(true);
    if publishable != metadata.packages.publish {
        return Err(MetadataRefusal::PackagePublishable(WORKSPACE_MANIFEST.to_owned()));
    }
    Ok(())
}

/// Requires every member manifest to inherit rather than declare its own.
///
/// # Errors
///
/// Returns [`MetadataRefusal::CargoDrift`] for a member that declares a license
/// of its own and [`MetadataRefusal::PackagePublishable`] for one that could be
/// published.
pub fn require_member_inherits(name: &str, manifest: &str) -> Result<(), MetadataRefusal> {
    let held: toml::Value = toml::from_str(manifest)
        .map_err(|failure| MetadataRefusal::Unreadable(failure.to_string()))?;
    let package = &held["package"];
    let inherits = |field: &str| {
        package
            .get(field)
            .and_then(toml::Value::as_table)
            .and_then(|table| table.get("workspace"))
            .and_then(toml::Value::as_bool)
            .unwrap_or(false)
    };
    if !inherits("license") {
        return Err(MetadataRefusal::CargoDrift {
            expected: "license.workspace = true".to_owned(),
            held: name.to_owned(),
        });
    }
    if !inherits("publish") {
        return Err(MetadataRefusal::PackagePublishable(name.to_owned()));
    }
    Ok(())
}

/// Requires the address to be the one the automation authority validated.
///
/// # Errors
///
/// Returns [`MetadataRefusal::AddressDrift`] when the two documents name
/// different repositories.
pub fn require_authoritative_address(
    metadata: &ReleaseMetadata,
    canonical_address: &str,
    owner_identifier: u64,
) -> Result<(), MetadataRefusal> {
    if metadata.repository.canonical_address != canonical_address {
        return Err(MetadataRefusal::AddressDrift {
            expected: canonical_address.to_owned(),
            held: metadata.repository.canonical_address.clone(),
        });
    }
    if metadata.repository.owner_identifier != owner_identifier {
        return Err(MetadataRefusal::AddressDrift {
            expected: owner_identifier.to_string(),
            held: metadata.repository.owner_identifier.to_string(),
        });
    }
    Ok(())
}
