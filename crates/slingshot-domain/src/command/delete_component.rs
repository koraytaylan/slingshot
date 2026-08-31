//! Removing a component from a page.
//!
//! A component added by mistake currently has no way out, which makes
//! `add_component` a one-way door. This is the other side of it.
//!
//! It states no reference policy, and that is a decision rather than an
//! omission. A page and an asset are referred to from elsewhere in the
//! repository and a component is not: what points at a component is the page it
//! is part of, which is being edited by this very request. Carrying a policy
//! that could never apply would suggest there was a case where it did.

use serde::{Deserialize, Serialize};

use crate::command::repository_path::RepositoryPath;
use crate::command::resource_mutation::{DeletedResourceResult, MutationResultFailure};

/// One request to remove a component.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DeleteComponentCommand {
    /// Component resource to remove, with everything below it.
    pub component_path: RepositoryPath,
}

/// Why a component was not removed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DeleteComponentFailure {
    /// Nothing is at the address.
    ComponentNotFound,
    /// Something is there and this caller may not remove it.
    ComponentAccessDenied,
    /// Something is there and it is not a component.
    ComponentInvalid,
    /// The save failed, provably without committing.
    RepositoryCommitFailed,
    /// Nobody can tell whether the save committed.
    MutationOutcomeUnknown,
}

/// One refused component deletion.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DeleteComponentRefusal {
    /// Component this request named.
    pub component_path: RepositoryPath,
    /// Why it was refused.
    pub failure: DeleteComponentFailure,
}

impl DeleteComponentRefusal {
    /// Returns whether this refusal proves the operation changed nothing.
    #[must_use]
    pub fn proves_no_effect(&self) -> bool {
        !matches!(self.failure, DeleteComponentFailure::MutationOutcomeUnknown)
    }

    /// Requires this refusal to answer `command`.
    ///
    /// # Errors
    ///
    /// Returns [`MutationResultFailure::NotThisRequest`] when it names another
    /// request's component.
    pub fn require_answers(
        &self,
        command: &DeleteComponentCommand,
    ) -> Result<(), MutationResultFailure> {
        if self.component_path == command.component_path {
            Ok(())
        } else {
            Err(MutationResultFailure::NotThisRequest)
        }
    }
}

/// What a completed component deletion removed.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct DeleteComponentResult {
    /// Address that is no longer there, and how much went with it.
    pub deleted: DeletedResourceResult,
}

impl DeleteComponentResult {
    /// Requires this result to answer `command`.
    ///
    /// # Errors
    ///
    /// Returns [`MutationResultFailure::NotThisRequest`] when it names another
    /// request's component.
    pub fn require_answers(
        &self,
        command: &DeleteComponentCommand,
    ) -> Result<(), MutationResultFailure> {
        self.deleted.require_answers(&command.component_path)
    }
}
