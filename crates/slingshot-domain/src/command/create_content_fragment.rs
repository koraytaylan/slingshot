//! Making a content fragment from a model.
//!
//! A fragment is the unit a headless consumer actually reads, and nothing in the
//! registry could make one. This command does, under a model whose elements this
//! contract deliberately does not know.
//!
//! That last point is the design decision worth stating. A model lives in the
//! repository and declares which elements a fragment has and what kind each one
//! is. This contract cannot read it, so it does not pretend to validate against
//! it: an element the model does not declare is refused by the author, under its
//! own closed category, so a caller can tell "that element does not exist" from
//! "that value was too long" without guessing.

use serde::{Deserialize, Serialize};

use crate::command::content_fragment_element::ContentFragmentElementValues;
use crate::command::find_pages_containing_phrase::PageTitle;
use crate::command::repository_path::{PathFailure, RepositoryName, RepositoryPath};
use crate::command::resource_mutation::{MutationResultFailure, ResourceMutationResult};

/// Exact primary type this command creates.
pub const CONTENT_FRAGMENT_PRIMARY_NODE_TYPE: &str = "dam:Asset";

/// One request to create a content fragment.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CreateContentFragmentCommand {
    /// Elements to write into the new fragment's master variation.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub elements: Option<ContentFragmentElementValues>,
    /// Model the fragment answers to.
    pub model_path: RepositoryPath,
    /// Name of the fragment to create.
    pub name: RepositoryName,
    /// Node to create it under.
    pub parent_path: RepositoryPath,
    /// Title to record on it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<PageTitle>,
}

impl CreateContentFragmentCommand {
    /// Returns where this command would create its fragment.
    ///
    /// # Errors
    ///
    /// Returns the path failure when the parent cannot take this child.
    pub fn target_path(&self) -> Result<RepositoryPath, PathFailure> {
        self.parent_path.creatable_child(&self.name)
    }
}

/// Why a content fragment was not created.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CreateContentFragmentFailure {
    /// The parent is not there.
    ParentNotFound,
    /// The parent is there and unwritable.
    ParentAccessDenied,
    /// Something is already at the target.
    TargetAlreadyExists,
    /// The model is not there.
    ModelNotFound,
    /// The model is there and is not one.
    ModelInvalid,
    /// An element named is not one the model declares.
    ElementUnknown,
    /// An element value is not one the model accepts.
    ElementValueRejected,
    /// The save failed, provably without committing.
    RepositoryCommitFailed,
    /// Nobody can tell whether the save committed.
    MutationOutcomeUnknown,
}

/// One refused content fragment creation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CreateContentFragmentRefusal {
    /// Why it was refused.
    pub failure: CreateContentFragmentFailure,
    /// Target this command computed.
    pub target_path: RepositoryPath,
}

impl CreateContentFragmentRefusal {
    /// Returns whether this refusal proves the operation changed nothing.
    #[must_use]
    pub fn proves_no_effect(&self) -> bool {
        !matches!(self.failure, CreateContentFragmentFailure::MutationOutcomeUnknown)
    }

    /// Requires this refusal to answer `command`.
    ///
    /// # Errors
    ///
    /// Returns [`MutationResultFailure::NotThisRequest`] when the named target
    /// is not the one the command computes.
    pub fn require_answers(
        &self,
        command: &CreateContentFragmentCommand,
    ) -> Result<(), MutationResultFailure> {
        let expected = command.target_path().map_err(|_| MutationResultFailure::NotThisRequest)?;
        if self.target_path == expected {
            Ok(())
        } else {
            Err(MutationResultFailure::NotThisRequest)
        }
    }
}

/// What a completed content fragment creation produced.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct CreateContentFragmentResult {
    /// Fragment that was created.
    pub mutated: ResourceMutationResult,
}

impl CreateContentFragmentResult {
    /// Requires this result to answer `command`.
    ///
    /// # Errors
    ///
    /// Returns [`MutationResultFailure::NotThisRequest`] when it names an
    /// address this request did not compute.
    pub fn require_answers(
        &self,
        command: &CreateContentFragmentCommand,
    ) -> Result<(), MutationResultFailure> {
        let expected = command.target_path().map_err(|_| MutationResultFailure::NotThisRequest)?;
        self.mutated.require_answers(&expected)
    }
}
