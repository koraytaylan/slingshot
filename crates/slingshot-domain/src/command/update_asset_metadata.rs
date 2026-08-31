//! Writing the metadata that asset searches read.
//!
//! `find_assets_by_metadata` searches properties on an asset's metadata
//! resource, and until now nothing could put them there. This command writes to
//! that resource, whose address it computes from the asset address rather than
//! accepting: metadata lives at a fixed place under an asset, and letting a
//! caller name it would let a caller write asset metadata onto something that is
//! not an asset's metadata.

use serde::{Deserialize, Serialize};

use crate::command::create_page::MutationProperties;
use crate::command::repository_path::{PathFailure, RepositoryName, RepositoryPath};
use crate::command::resource_mutation::{
    MutationResultFailure, PropertyMutationFailure, RemovedPropertyNames, ResourceMutationResult,
    require_property_mutation,
};

/// Child of an asset that its content lives under.
pub const ASSET_CONTENT_CHILD: &str = "jcr:content";

/// Child of that content resource that its metadata lives under.
pub const ASSET_METADATA_CHILD: &str = "metadata";

/// One request to change an asset's metadata.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct UpdateAssetMetadataCommand {
    /// Asset whose metadata resource is changed.
    pub asset_path: RepositoryPath,
    /// Properties to assign to it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub properties: Option<MutationProperties>,
    /// Properties to remove from it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub removed_property_names: Option<RemovedPropertyNames>,
}

impl UpdateAssetMetadataCommand {
    /// Returns the metadata resource this command writes to.
    ///
    /// # Errors
    ///
    /// Returns the path failure when the asset address cannot take the two
    /// children this address is composed from.
    pub fn metadata_path(&self) -> Result<RepositoryPath, PathFailure> {
        let content = RepositoryName::parse(ASSET_CONTENT_CHILD)?;
        let metadata = RepositoryName::parse(ASSET_METADATA_CHILD)?;
        self.asset_path.creatable_child(&content)?.creatable_child(&metadata)
    }

    /// Requires this request to change exactly one thing per property.
    ///
    /// # Errors
    ///
    /// Returns [`PropertyMutationFailure::BothAssignedAndRemoved`] when one
    /// property is named in both documents, and
    /// [`PropertyMutationFailure::ChangesNothing`] when the request would change
    /// nothing at all.
    pub fn require_usable(&self) -> Result<(), PropertyMutationFailure> {
        require_property_mutation(
            self.properties.as_ref(),
            self.removed_property_names.as_ref(),
            false,
        )
    }
}

/// Why an asset's metadata was not changed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum UpdateAssetMetadataFailure {
    /// Nothing is at the address.
    AssetNotFound,
    /// Something is there and this caller may not change it.
    AssetAccessDenied,
    /// Something is there and it is not an asset.
    AssetInvalid,
    /// A property could not be applied.
    PropertyRejected,
    /// A property named for removal is one the repository keeps.
    PropertyNotRemovable,
    /// The save failed, provably without committing.
    RepositoryCommitFailed,
    /// Nobody can tell whether the save committed.
    MutationOutcomeUnknown,
}

/// One refused asset metadata update.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct UpdateAssetMetadataRefusal {
    /// Asset this request named.
    pub asset_path: RepositoryPath,
    /// Why it was refused.
    pub failure: UpdateAssetMetadataFailure,
}

impl UpdateAssetMetadataRefusal {
    /// Returns whether this refusal proves the operation changed nothing.
    #[must_use]
    pub fn proves_no_effect(&self) -> bool {
        !matches!(self.failure, UpdateAssetMetadataFailure::MutationOutcomeUnknown)
    }

    /// Requires this refusal to answer `command`.
    ///
    /// # Errors
    ///
    /// Returns [`MutationResultFailure::NotThisRequest`] when it names another
    /// request's asset.
    pub fn require_answers(
        &self,
        command: &UpdateAssetMetadataCommand,
    ) -> Result<(), MutationResultFailure> {
        if self.asset_path == command.asset_path {
            Ok(())
        } else {
            Err(MutationResultFailure::NotThisRequest)
        }
    }
}

/// What a completed metadata update changed.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct UpdateAssetMetadataResult {
    /// Metadata resource this update wrote to.
    pub mutated: ResourceMutationResult,
}

impl UpdateAssetMetadataResult {
    /// Requires this result to answer `command`.
    ///
    /// # Errors
    ///
    /// Returns [`MutationResultFailure::NotThisRequest`] when it names an
    /// address this request did not compute.
    pub fn require_answers(
        &self,
        command: &UpdateAssetMetadataCommand,
    ) -> Result<(), MutationResultFailure> {
        let expected =
            command.metadata_path().map_err(|_| MutationResultFailure::NotThisRequest)?;
        self.mutated.require_answers(&expected)
    }
}
