//! Editing one variation of an experience fragment.
//!
//! An experience fragment's content is in its variations, so this command
//! addresses a variation directly. Taking a fragment and a variation name and
//! composing them here would be a second way of computing an address the caller
//! can already hold, and two ways of computing one address eventually disagree.
//!
//! The properties go to the variation's content resource, whose address is
//! computed rather than accepted, for the reason a page update computes its own:
//! content written to the page node is stored and never rendered.

use serde::{Deserialize, Serialize};

use crate::command::create_page::{MutationProperties, PAGE_CONTENT_CHILD};
use crate::command::find_pages_containing_phrase::PageTitle;
use crate::command::repository_path::{PathFailure, RepositoryName, RepositoryPath};
use crate::command::resource_mutation::{
    MutationResultFailure, PropertyMutationFailure, RemovedPropertyNames, ResourceMutationResult,
    require_property_mutation, require_title_not_redefined,
};

/// One request to change an experience fragment variation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct UpdateExperienceFragmentCommand {
    /// Properties to assign to the variation's content resource.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub properties: Option<MutationProperties>,
    /// Properties to remove from it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub removed_property_names: Option<RemovedPropertyNames>,
    /// Title to record on it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<PageTitle>,
    /// Variation to change.
    pub variation_path: RepositoryPath,
}

impl UpdateExperienceFragmentCommand {
    /// Returns the content resource this command writes to.
    ///
    /// # Errors
    ///
    /// Returns the path failure when the variation address cannot take the
    /// content child.
    pub fn content_path(&self) -> Result<RepositoryPath, PathFailure> {
        let child = RepositoryName::parse(PAGE_CONTENT_CHILD)?;
        self.variation_path.creatable_child(&child)
    }

    /// Requires this request to change exactly one thing per property.
    ///
    /// # Errors
    ///
    /// Returns [`PropertyMutationFailure::TitleRedefined`] when the property
    /// document carries the title property this command sets from its own field,
    /// [`PropertyMutationFailure::BothAssignedAndRemoved`] when one property is
    /// named in both documents, and [`PropertyMutationFailure::ChangesNothing`]
    /// when the request would change nothing at all.
    pub fn require_usable(&self) -> Result<(), PropertyMutationFailure> {
        require_title_not_redefined(self.properties.as_ref(), self.title.is_some())?;
        require_property_mutation(
            self.properties.as_ref(),
            self.removed_property_names.as_ref(),
            self.title.is_some(),
        )
    }
}

/// Why an experience fragment variation was not changed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum UpdateExperienceFragmentFailure {
    /// Nothing is at the address.
    VariationNotFound,
    /// Something is there and this caller may not change it.
    VariationAccessDenied,
    /// Something is there and it is not a variation.
    VariationInvalid,
    /// A property could not be applied.
    PropertyRejected,
    /// A property named for removal is one the repository keeps.
    PropertyNotRemovable,
    /// The save failed, provably without committing.
    RepositoryCommitFailed,
    /// Nobody can tell whether the save committed.
    MutationOutcomeUnknown,
}

/// One refused experience fragment update.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct UpdateExperienceFragmentRefusal {
    /// Why it was refused.
    pub failure: UpdateExperienceFragmentFailure,
    /// Variation this request named.
    pub variation_path: RepositoryPath,
}

impl UpdateExperienceFragmentRefusal {
    /// Returns whether this refusal proves the operation changed nothing.
    #[must_use]
    pub fn proves_no_effect(&self) -> bool {
        !matches!(self.failure, UpdateExperienceFragmentFailure::MutationOutcomeUnknown)
    }

    /// Requires this refusal to answer `command`.
    ///
    /// # Errors
    ///
    /// Returns [`MutationResultFailure::NotThisRequest`] when it names another
    /// request's variation.
    pub fn require_answers(
        &self,
        command: &UpdateExperienceFragmentCommand,
    ) -> Result<(), MutationResultFailure> {
        if self.variation_path == command.variation_path {
            Ok(())
        } else {
            Err(MutationResultFailure::NotThisRequest)
        }
    }
}

/// What a completed experience fragment update changed.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct UpdateExperienceFragmentResult {
    /// Content resource this update wrote to.
    pub mutated: ResourceMutationResult,
}

impl UpdateExperienceFragmentResult {
    /// Requires this result to answer `command`.
    ///
    /// # Errors
    ///
    /// Returns [`MutationResultFailure::NotThisRequest`] when it names an
    /// address this request did not compute.
    pub fn require_answers(
        &self,
        command: &UpdateExperienceFragmentCommand,
    ) -> Result<(), MutationResultFailure> {
        let expected = command.content_path().map_err(|_| MutationResultFailure::NotThisRequest)?;
        self.mutated.require_answers(&expected)
    }
}
