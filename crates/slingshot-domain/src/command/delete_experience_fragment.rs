//! Removing an experience fragment and every variation it holds.
//!
//! What refers to an experience fragment usually refers to one of its
//! variations, not to the fragment itself, which is precisely why the reference
//! policy is stated: a caller deleting the fragment is deleting things it may
//! not have been looking at.

use serde::{Deserialize, Serialize};

use crate::command::repository_path::RepositoryPath;
use crate::command::resource_mutation::{
    DeletedResourceResult, MutationResultFailure, ReferencePolicy,
};

/// One request to remove an experience fragment.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DeleteExperienceFragmentCommand {
    /// Fragment to remove, with every variation it holds.
    pub fragment_path: RepositoryPath,
    /// What to do about whatever points at it.
    pub reference_policy: ReferencePolicy,
}

impl DeleteExperienceFragmentCommand {
    /// Reports whether this request refuses to remove a referenced fragment.
    #[must_use]
    pub fn refuses_when_referenced(&self) -> bool {
        self.reference_policy == ReferencePolicy::RefuseWhenReferenced
    }
}

/// Why an experience fragment was not removed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DeleteExperienceFragmentFailure {
    /// Nothing is at the address.
    FragmentNotFound,
    /// Something is there and this caller may not remove it.
    FragmentAccessDenied,
    /// Something is there and it is not an experience fragment.
    FragmentInvalid,
    /// Something still points at it, and the request said to refuse.
    FragmentIsReferenced,
    /// The subtree is larger than the contract permits removing at once.
    DeletionBudgetExceeded,
    /// The save failed, provably without committing.
    RepositoryCommitFailed,
    /// Nobody can tell whether the save committed.
    MutationOutcomeUnknown,
}

/// One refused experience fragment deletion.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DeleteExperienceFragmentRefusal {
    /// Why it was refused.
    pub failure: DeleteExperienceFragmentFailure,
    /// Fragment this request named.
    pub fragment_path: RepositoryPath,
}

impl DeleteExperienceFragmentRefusal {
    /// Returns whether this refusal proves the operation changed nothing.
    #[must_use]
    pub fn proves_no_effect(&self) -> bool {
        !matches!(self.failure, DeleteExperienceFragmentFailure::MutationOutcomeUnknown)
    }

    /// Requires this refusal to answer `command`.
    ///
    /// # Errors
    ///
    /// Returns [`MutationResultFailure::NotThisRequest`] when it names another
    /// request's fragment, and when it reports a reference the request said to
    /// ignore.
    pub fn require_answers(
        &self,
        command: &DeleteExperienceFragmentCommand,
    ) -> Result<(), MutationResultFailure> {
        let referenced =
            matches!(self.failure, DeleteExperienceFragmentFailure::FragmentIsReferenced);
        if self.fragment_path != command.fragment_path
            || (referenced && !command.refuses_when_referenced())
        {
            return Err(MutationResultFailure::NotThisRequest);
        }
        Ok(())
    }
}

/// What a completed experience fragment deletion removed.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct DeleteExperienceFragmentResult {
    /// Address that is no longer there, and how much went with it.
    pub deleted: DeletedResourceResult,
}

impl DeleteExperienceFragmentResult {
    /// Requires this result to answer `command`.
    ///
    /// # Errors
    ///
    /// Returns [`MutationResultFailure::NotThisRequest`] when it names another
    /// request's fragment.
    pub fn require_answers(
        &self,
        command: &DeleteExperienceFragmentCommand,
    ) -> Result<(), MutationResultFailure> {
        self.deleted.require_answers(&command.fragment_path)
    }
}
