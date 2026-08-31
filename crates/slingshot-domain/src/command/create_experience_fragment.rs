//! Making an experience fragment, together with the variation that renders it.
//!
//! An experience fragment is a page-shaped thing whose content lives in
//! variations. Creating the container on its own would leave a fragment nothing
//! can render and a caller with no address to write to, so the first variation is
//! created with it and the result carries both addresses - the fragment's and
//! the variation's - rather than leaving a caller to compose the second from the
//! first.

use serde::{Deserialize, Serialize};

use crate::command::content_fragment_element::ContentFragmentVariationName;
use crate::command::find_pages_containing_phrase::PageTitle;
use crate::command::query_paths::anchor_contains;
use crate::command::repository_path::{PathFailure, RepositoryName, RepositoryPath};
use crate::command::resource_mutation::MutationResultFailure;

/// Exact primary type this command creates.
pub const EXPERIENCE_FRAGMENT_PRIMARY_NODE_TYPE: &str = "cq:Page";

/// One request to create an experience fragment.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CreateExperienceFragmentCommand {
    /// Name of the fragment to create.
    pub name: RepositoryName,
    /// Node to create it under.
    pub parent_path: RepositoryPath,
    /// Template the first variation is built from.
    pub template_path: RepositoryPath,
    /// Title to record on it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<PageTitle>,
    /// Name of the variation created with it.
    pub variation_name: ContentFragmentVariationName,
}

impl CreateExperienceFragmentCommand {
    /// Returns where this command would create its fragment.
    ///
    /// # Errors
    ///
    /// Returns the path failure when the parent cannot take this child.
    pub fn target_path(&self) -> Result<RepositoryPath, PathFailure> {
        self.parent_path.creatable_child(&self.name)
    }

    /// Returns where this command would create its first variation.
    ///
    /// # Errors
    ///
    /// Returns the path failure when either address cannot take its child.
    pub fn variation_path(&self) -> Result<RepositoryPath, PathFailure> {
        let name = RepositoryName::parse(self.variation_name.as_text())?;
        self.target_path()?.creatable_child(&name)
    }
}

/// Why an experience fragment was not created.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CreateExperienceFragmentFailure {
    /// The parent is not there.
    ParentNotFound,
    /// The parent is there and unwritable.
    ParentAccessDenied,
    /// Something is already at the target.
    TargetAlreadyExists,
    /// The template is not there.
    TemplateNotFound,
    /// The template is there and is not one.
    TemplateInvalid,
    /// The save failed, provably without committing.
    RepositoryCommitFailed,
    /// Nobody can tell whether the save committed.
    MutationOutcomeUnknown,
}

/// One refused experience fragment creation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CreateExperienceFragmentRefusal {
    /// Why it was refused.
    pub failure: CreateExperienceFragmentFailure,
    /// Target this command computed.
    pub target_path: RepositoryPath,
}

impl CreateExperienceFragmentRefusal {
    /// Returns whether this refusal proves the operation changed nothing.
    #[must_use]
    pub fn proves_no_effect(&self) -> bool {
        !matches!(self.failure, CreateExperienceFragmentFailure::MutationOutcomeUnknown)
    }

    /// Requires this refusal to answer `command`.
    ///
    /// # Errors
    ///
    /// Returns [`MutationResultFailure::NotThisRequest`] when the named target
    /// is not the one the command computes.
    pub fn require_answers(
        &self,
        command: &CreateExperienceFragmentCommand,
    ) -> Result<(), MutationResultFailure> {
        let expected = command.target_path().map_err(|_| MutationResultFailure::NotThisRequest)?;
        if self.target_path == expected {
            Ok(())
        } else {
            Err(MutationResultFailure::NotThisRequest)
        }
    }
}

/// What a completed experience fragment creation produced.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CreateExperienceFragmentResult {
    /// Fragment that was created.
    pub repository_path: RepositoryPath,
    /// Variation created with it.
    pub variation_path: RepositoryPath,
}

impl CreateExperienceFragmentResult {
    /// Requires this result to answer `command`.
    ///
    /// # Errors
    ///
    /// Returns [`MutationResultFailure::NotThisRequest`] when either address is
    /// not the one this request computes, or when the variation is not inside
    /// the fragment it belongs to.
    pub fn require_answers(
        &self,
        command: &CreateExperienceFragmentCommand,
    ) -> Result<(), MutationResultFailure> {
        let fragment = command.target_path().map_err(|_| MutationResultFailure::NotThisRequest)?;
        let variation =
            command.variation_path().map_err(|_| MutationResultFailure::NotThisRequest)?;
        if self.repository_path != fragment
            || self.variation_path != variation
            || !anchor_contains(&self.repository_path, &self.variation_path)
        {
            return Err(MutationResultFailure::NotThisRequest);
        }
        Ok(())
    }
}
