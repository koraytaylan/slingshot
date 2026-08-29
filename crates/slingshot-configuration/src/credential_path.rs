//! Root-contained references to credential and certificate sources.
//!
//! A profile names its service-credential document and its optional
//! certificate-authority document by a root-relative reference. Resolving one
//! must land inside the configuration root and nowhere else, whatever the
//! process working directory is and wherever the profile that named it lives.
//!
//! The reference grammar already refuses everything that could point outward -
//! an absolute path, a parent component, an empty component, a backslash, a
//! drive or uniform-naming-convention prefix - so this module resolves rather
//! than sanitizes. It also keeps the ordered components, because the resolved
//! path is a destination to check against, not a path to reopen: the filesystem
//! authority walks those components one at a time relative to the verified root
//! handle so no link along the way can be followed.

use std::path::{Path, PathBuf};

use slingshot_domain::configuration_snapshot::ConfigurationReference;
use slingshot_domain::profile_authentication_contract::ConfigurationFailureCode;

use crate::configuration_root::ConfigurationRoot;

/// Reason a credential or certificate reference could not be resolved.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("{code} at {structural_location}")]
pub struct CredentialPathFailure {
    /// Stable code from the contract registry.
    pub code: ConfigurationFailureCode,
    /// Manifest vocabulary naming where the failure was found.
    pub structural_location: &'static str,
}

/// One source resolved inside the configuration root.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CredentialPath {
    /// Reference the path was resolved from.
    reference: ConfigurationReference,
    /// Absolute path the reference names inside the root.
    path: PathBuf,
}

impl CredentialPath {
    /// Resolves one reference against `root`.
    ///
    /// # Errors
    ///
    /// Returns [`ConfigurationFailureCode::ConfigurationReferenceInvalid`] when
    /// a component is one the platform would read as something other than a
    /// plain name, or when the resolved path is not a descendant of the root.
    /// The second check is redundant against the grammar and is kept because a
    /// reference that escapes must fail even if the grammar ever loosens.
    pub fn resolve(
        root: &ConfigurationRoot,
        reference: &ConfigurationReference,
    ) -> Result<Self, CredentialPathFailure> {
        let refuse = || CredentialPathFailure {
            code: ConfigurationFailureCode::ConfigurationReferenceInvalid,
            structural_location: "reference",
        };
        let mut path = root.path().to_path_buf();
        for component in reference.components() {
            if !is_plain_component(component) {
                return Err(refuse());
            }
            path.push(component);
        }
        if !path.starts_with(root.path()) || path == root.path() {
            return Err(refuse());
        }
        if component_count(&path) != component_count(root.path()) + reference.components().count() {
            return Err(refuse());
        }
        Ok(Self { reference: reference.clone(), path })
    }

    /// Returns the reference this path was resolved from.
    #[must_use]
    pub fn reference(&self) -> &ConfigurationReference {
        &self.reference
    }

    /// Returns the absolute path the reference names.
    #[must_use]
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Returns the components to open in turn below the verified root handle.
    pub fn components(&self) -> impl Iterator<Item = &str> {
        self.reference.components()
    }
}

/// Reports whether one component names exactly itself on every platform.
///
/// The reference grammar already accepts only unreserved name bytes, so this
/// re-checks the two forms a platform would still read as navigation and the
/// separators a path builder would split on.
fn is_plain_component(component: &str) -> bool {
    !component.is_empty()
        && component != "."
        && component != ".."
        && !component.contains(['/', '\\', ':'])
}

/// Returns how many components one path is built from.
fn component_count(path: &Path) -> usize {
    path.components().count()
}
