//! Making somewhere to put an asset.
//!
//! Assets could be searched and never written, and the first thing writing one
//! needs is a place. This is the smallest write in the family: one node, one
//! primary type, one optional title, and a target address computed from the
//! parent and the name rather than accepted from the caller - so a failure names
//! the address this request would have made rather than one somebody asserted.

use serde::{Deserialize, Serialize};

use crate::command::find_pages_containing_phrase::PageTitle;
use crate::command::repository_path::{PathFailure, RepositoryName, RepositoryPath};
use crate::command::resource_mutation::{MutationResultFailure, ResourceMutationResult};

/// Exact primary type this command creates.
pub const ASSET_FOLDER_PRIMARY_NODE_TYPE: &str = "sling:OrderedFolder";

/// One request to create an asset folder.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CreateAssetFolderCommand {
    /// Name of the folder to create.
    pub name: RepositoryName,
    /// Node to create it under.
    pub parent_path: RepositoryPath,
    /// Title to record on it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<PageTitle>,
}

impl CreateAssetFolderCommand {
    /// Returns where this command would create its folder.
    ///
    /// # Errors
    ///
    /// Returns the path failure when the parent cannot take this child, which
    /// is the same refusal the path grammar would make.
    pub fn target_path(&self) -> Result<RepositoryPath, PathFailure> {
        self.parent_path.creatable_child(&self.name)
    }
}

/// Why an asset folder was not created.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CreateAssetFolderFailure {
    /// The parent is not there.
    ParentNotFound,
    /// The parent is there and unwritable.
    ParentAccessDenied,
    /// Something is already at the target.
    TargetAlreadyExists,
    /// The title could not be applied.
    PropertyRejected,
    /// The save failed, provably without committing.
    RepositoryCommitFailed,
    /// Nobody can tell whether the save committed.
    MutationOutcomeUnknown,
}

/// One refused asset folder creation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CreateAssetFolderRefusal {
    /// Why it was refused.
    pub failure: CreateAssetFolderFailure,
    /// Target this command computed.
    pub target_path: RepositoryPath,
}

impl CreateAssetFolderRefusal {
    /// Returns whether this refusal proves the operation changed nothing.
    #[must_use]
    pub fn proves_no_effect(&self) -> bool {
        !matches!(self.failure, CreateAssetFolderFailure::MutationOutcomeUnknown)
    }

    /// Requires this refusal to answer `command`.
    ///
    /// # Errors
    ///
    /// Returns [`MutationResultFailure::NotThisRequest`] when the named target
    /// is not the one the command computes.
    pub fn require_answers(
        &self,
        command: &CreateAssetFolderCommand,
    ) -> Result<(), MutationResultFailure> {
        let expected = command.target_path().map_err(|_| MutationResultFailure::NotThisRequest)?;
        if self.target_path == expected {
            Ok(())
        } else {
            Err(MutationResultFailure::NotThisRequest)
        }
    }
}

/// What a completed asset folder creation produced.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct CreateAssetFolderResult {
    /// Folder that was created.
    pub mutated: ResourceMutationResult,
}

impl CreateAssetFolderResult {
    /// Requires this result to answer `command`.
    ///
    /// # Errors
    ///
    /// Returns [`MutationResultFailure::NotThisRequest`] when it names an
    /// address this request did not compute.
    pub fn require_answers(
        &self,
        command: &CreateAssetFolderCommand,
    ) -> Result<(), MutationResultFailure> {
        let expected = command.target_path().map_err(|_| MutationResultFailure::NotThisRequest)?;
        self.mutated.require_answers(&expected)
    }
}
