//! Changing a component that is already on a page.
//!
//! `add_component` puts one component under a page's content resource and
//! nothing afterwards can change what it holds. This command does, addressed at
//! the component resource itself rather than at a page and a relative position:
//! a component is a resource with an address, and asking a caller to describe it
//! as a page plus a path would be a second way of saying something the
//! repository already says one way.
//!
//! The two property documents follow the same shared rule every update in this
//! registry does: one property is assigned or removed, never both by one
//! request, and a request that would change nothing is refused rather than
//! answered with a success that did nothing.

use serde::{Deserialize, Serialize};

use crate::command::create_page::MutationProperties;
use crate::command::repository_path::RepositoryPath;
use crate::command::resource_mutation::{
    MutationResultFailure, PropertyMutationFailure, RemovedPropertyNames, ResourceMutationResult,
    require_property_mutation,
};

/// One request to change a component.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct UpdateComponentCommand {
    /// Component resource to change.
    pub component_path: RepositoryPath,
    /// Properties to assign to it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub properties: Option<MutationProperties>,
    /// Properties to remove from it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub removed_property_names: Option<RemovedPropertyNames>,
}

impl UpdateComponentCommand {
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

/// Why a component was not changed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum UpdateComponentFailure {
    /// Nothing is at the address.
    ComponentNotFound,
    /// Something is there and this caller may not change it.
    ComponentAccessDenied,
    /// Something is there and it is not a component.
    ComponentInvalid,
    /// A property could not be applied.
    PropertyRejected,
    /// A property named for removal is one the repository keeps.
    PropertyNotRemovable,
    /// The save failed, provably without committing.
    RepositoryCommitFailed,
    /// Nobody can tell whether the save committed.
    MutationOutcomeUnknown,
}

/// One refused component update.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct UpdateComponentRefusal {
    /// Component this request named.
    pub component_path: RepositoryPath,
    /// Why it was refused.
    pub failure: UpdateComponentFailure,
}

impl UpdateComponentRefusal {
    /// Returns whether this refusal proves the operation changed nothing.
    #[must_use]
    pub fn proves_no_effect(&self) -> bool {
        !matches!(self.failure, UpdateComponentFailure::MutationOutcomeUnknown)
    }

    /// Requires this refusal to answer `command`.
    ///
    /// # Errors
    ///
    /// Returns [`MutationResultFailure::NotThisRequest`] when it names another
    /// request's component.
    pub fn require_answers(
        &self,
        command: &UpdateComponentCommand,
    ) -> Result<(), MutationResultFailure> {
        if self.component_path == command.component_path {
            Ok(())
        } else {
            Err(MutationResultFailure::NotThisRequest)
        }
    }
}

/// What a completed component update changed.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct UpdateComponentResult {
    /// Component this update wrote to.
    pub mutated: ResourceMutationResult,
}

impl UpdateComponentResult {
    /// Requires this result to answer `command`.
    ///
    /// # Errors
    ///
    /// Returns [`MutationResultFailure::NotThisRequest`] when it names another
    /// request's component.
    pub fn require_answers(
        &self,
        command: &UpdateComponentCommand,
    ) -> Result<(), MutationResultFailure> {
        self.mutated.require_answers(&command.component_path)
    }
}
