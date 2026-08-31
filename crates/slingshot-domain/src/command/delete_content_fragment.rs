//! Removing a content fragment.
//!
//! Every page that renders the fragment refers to it, and none of those pages is
//! visible from the fragment's address. The reference policy is therefore stated
//! rather than assumed, exactly as it is for an asset, and an absent fragment is
//! a failure rather than a success with nothing to do.

use serde::{Deserialize, Serialize};

use crate::command::repository_path::RepositoryPath;
use crate::command::resource_mutation::{
    DeletedResourceResult, MutationResultFailure, ReferencePolicy,
};

/// One request to remove a content fragment.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DeleteContentFragmentCommand {
    /// Fragment to remove, with every variation it holds.
    pub fragment_path: RepositoryPath,
    /// What to do about whatever points at it.
    pub reference_policy: ReferencePolicy,
}

impl DeleteContentFragmentCommand {
    /// Reports whether this request refuses to remove a referenced fragment.
    #[must_use]
    pub fn refuses_when_referenced(&self) -> bool {
        self.reference_policy == ReferencePolicy::RefuseWhenReferenced
    }
}

/// Why a content fragment was not removed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DeleteContentFragmentFailure {
    /// Nothing is at the address.
    FragmentNotFound,
    /// Something is there and this caller may not remove it.
    FragmentAccessDenied,
    /// Something is there and it is not a content fragment.
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

/// One refused content fragment deletion.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DeleteContentFragmentRefusal {
    /// Why it was refused.
    pub failure: DeleteContentFragmentFailure,
    /// Fragment this request named.
    pub fragment_path: RepositoryPath,
}

impl DeleteContentFragmentRefusal {
    /// Returns whether this refusal proves the operation changed nothing.
    #[must_use]
    pub fn proves_no_effect(&self) -> bool {
        !matches!(self.failure, DeleteContentFragmentFailure::MutationOutcomeUnknown)
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
        command: &DeleteContentFragmentCommand,
    ) -> Result<(), MutationResultFailure> {
        let referenced = matches!(self.failure, DeleteContentFragmentFailure::FragmentIsReferenced);
        if self.fragment_path != command.fragment_path
            || (referenced && !command.refuses_when_referenced())
        {
            return Err(MutationResultFailure::NotThisRequest);
        }
        Ok(())
    }
}

/// What a completed content fragment deletion removed.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct DeleteContentFragmentResult {
    /// Address that is no longer there, and how much went with it.
    pub deleted: DeletedResourceResult,
}

impl DeleteContentFragmentResult {
    /// Requires this result to answer `command`.
    ///
    /// # Errors
    ///
    /// Returns [`MutationResultFailure::NotThisRequest`] when it names another
    /// request's fragment.
    pub fn require_answers(
        &self,
        command: &DeleteContentFragmentCommand,
    ) -> Result<(), MutationResultFailure> {
        self.deleted.require_answers(&command.fragment_path)
    }
}
