//! Editing one variation of a content fragment.
//!
//! Editing a fragment is editing a variation of it. A command that took no
//! variation would write the master every time, which is right until the first
//! time somebody has a variation and wrong silently from then on. The variation
//! is therefore an argument, and an absent one means the master - the same rule
//! the read command follows, stated in the same place.
//!
//! A request that carries neither a title nor an element is refused, because a
//! success that changed nothing is the answer least likely to be noticed.

use serde::{Deserialize, Serialize};

use crate::command::content_fragment_element::{
    ContentFragmentElementValues, ContentFragmentFailure, ContentFragmentVariationName,
};
use crate::command::find_pages_containing_phrase::PageTitle;
use crate::command::repository_path::RepositoryPath;
use crate::command::resource_mutation::{MutationResultFailure, ResourceMutationResult};

/// One request to change a content fragment.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct UpdateContentFragmentCommand {
    /// Elements to write into the variation.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub elements: Option<ContentFragmentElementValues>,
    /// Fragment to change.
    pub fragment_path: RepositoryPath,
    /// Title to record on it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<PageTitle>,
    /// Variation to write to, or the master variation when absent.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub variation_name: Option<ContentFragmentVariationName>,
}

impl UpdateContentFragmentCommand {
    /// Requires this request to change something.
    ///
    /// # Errors
    ///
    /// Returns [`ContentFragmentFailure::NotThisRequest`] when the request
    /// carries neither a title nor an element, which is a request that would
    /// report success having done nothing.
    pub fn require_usable(&self) -> Result<(), ContentFragmentFailure> {
        let assigns = self.elements.as_ref().is_some_and(|elements| !elements.is_empty());
        if assigns || self.title.is_some() {
            Ok(())
        } else {
            Err(ContentFragmentFailure::NotThisRequest)
        }
    }
}

/// Why a content fragment was not changed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum UpdateContentFragmentFailure {
    /// Nothing is at the address.
    FragmentNotFound,
    /// Something is there and this caller may not change it.
    FragmentAccessDenied,
    /// Something is there and it is not a content fragment.
    FragmentInvalid,
    /// The fragment has no variation of that name.
    VariationNotFound,
    /// An element named is not one the model declares.
    ElementUnknown,
    /// An element value is not one the model accepts.
    ElementValueRejected,
    /// The save failed, provably without committing.
    RepositoryCommitFailed,
    /// Nobody can tell whether the save committed.
    MutationOutcomeUnknown,
}

/// One refused content fragment update.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct UpdateContentFragmentRefusal {
    /// Why it was refused.
    pub failure: UpdateContentFragmentFailure,
    /// Fragment this request named.
    pub fragment_path: RepositoryPath,
}

impl UpdateContentFragmentRefusal {
    /// Returns whether this refusal proves the operation changed nothing.
    #[must_use]
    pub fn proves_no_effect(&self) -> bool {
        !matches!(self.failure, UpdateContentFragmentFailure::MutationOutcomeUnknown)
    }

    /// Requires this refusal to answer `command`.
    ///
    /// # Errors
    ///
    /// Returns [`MutationResultFailure::NotThisRequest`] when it names another
    /// request's fragment, and when it reports a missing variation a request
    /// that named none could not have looked for.
    pub fn require_answers(
        &self,
        command: &UpdateContentFragmentCommand,
    ) -> Result<(), MutationResultFailure> {
        let sought = matches!(self.failure, UpdateContentFragmentFailure::VariationNotFound);
        if self.fragment_path != command.fragment_path
            || (sought && command.variation_name.is_none())
        {
            return Err(MutationResultFailure::NotThisRequest);
        }
        Ok(())
    }
}

/// What a completed content fragment update changed.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct UpdateContentFragmentResult {
    /// Fragment this update wrote to.
    pub mutated: ResourceMutationResult,
}

impl UpdateContentFragmentResult {
    /// Requires this result to answer `command`.
    ///
    /// # Errors
    ///
    /// Returns [`MutationResultFailure::NotThisRequest`] when it names another
    /// request's fragment.
    pub fn require_answers(
        &self,
        command: &UpdateContentFragmentCommand,
    ) -> Result<(), MutationResultFailure> {
        self.mutated.require_answers(&command.fragment_path)
    }
}
