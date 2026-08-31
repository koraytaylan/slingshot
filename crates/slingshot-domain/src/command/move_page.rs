//! Moving a page, and deciding what happens to what pointed at it.
//!
//! Two things make this more than a rename. Everything that referred to the page
//! at its old address still refers to that address, and a caller has to say
//! whether those references travel with the page; and a destination inside the
//! source is a request that cannot be carried out at all, which is refused
//! before anything moves rather than discovered halfway through.
//!
//! The containment refusal covers three shapes that are one shape: a destination
//! equal to the source, a descendant of it, and its immediate child. All three
//! are the same mistake and all three return the same closed category.

use serde::{Deserialize, Serialize};

use crate::command::repository_path::RepositoryPath;
use crate::command::resource_mutation::{
    MovedResourceResult, MutationResultFailure, require_destination_outside_source,
};

/// One request to move a page.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MovePageCommand {
    /// Whether references to the old address follow the page.
    pub adjust_references: bool,
    /// Address the page is moved to.
    pub destination_path: RepositoryPath,
    /// Address the page is moved from.
    pub source_path: RepositoryPath,
}

impl MovePageCommand {
    /// Requires the destination to be somewhere this move can reach.
    ///
    /// # Errors
    ///
    /// Returns [`MutationResultFailure::DestinationInsideSource`] when the
    /// destination is the source or lies within it.
    pub fn require_usable(&self) -> Result<(), MutationResultFailure> {
        require_destination_outside_source(&self.source_path, &self.destination_path)
    }
}

/// Why a page was not moved.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MovePageFailure {
    /// Nothing is at the source.
    SourceNotFound,
    /// Something is at the source and this caller may not move it.
    SourceAccessDenied,
    /// The destination's parent is not there.
    DestinationParentNotFound,
    /// Something is already at the destination.
    DestinationAlreadyExists,
    /// The destination is the source or lies within it.
    DestinationInsideSource,
    /// More references point at the source than the contract permits rewriting.
    ReferenceAdjustmentBudgetExceeded,
    /// The save failed, provably without committing.
    RepositoryCommitFailed,
    /// Nobody can tell whether the save committed.
    MutationOutcomeUnknown,
}

/// One refused page move.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MovePageRefusal {
    /// Address this request would have moved to.
    pub destination_path: RepositoryPath,
    /// Why it was refused.
    pub failure: MovePageFailure,
    /// Address this request would have moved from.
    pub source_path: RepositoryPath,
}

impl MovePageRefusal {
    /// Returns whether this refusal proves the operation changed nothing.
    #[must_use]
    pub fn proves_no_effect(&self) -> bool {
        !matches!(self.failure, MovePageFailure::MutationOutcomeUnknown)
    }

    /// Requires this refusal to answer `command`.
    ///
    /// # Errors
    ///
    /// Returns [`MutationResultFailure::NotThisRequest`] when either address is
    /// another request's, and when it reports a reference budget the request
    /// never asked to spend.
    pub fn require_answers(&self, command: &MovePageCommand) -> Result<(), MutationResultFailure> {
        let adjusting = matches!(self.failure, MovePageFailure::ReferenceAdjustmentBudgetExceeded);
        if self.source_path != command.source_path
            || self.destination_path != command.destination_path
            || (adjusting && !command.adjust_references)
        {
            return Err(MutationResultFailure::NotThisRequest);
        }
        Ok(())
    }
}

/// What a completed page move did.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct MovePageResult {
    /// Where the page went, and what the move rewrote on the way.
    pub moved: MovedResourceResult,
}

impl MovePageResult {
    /// Requires this result to answer `command`.
    ///
    /// # Errors
    ///
    /// Returns [`MutationResultFailure::NotThisRequest`] when either address is
    /// another request's, or when it reports adjusted references a request that
    /// asked for none could not have produced.
    pub fn require_answers(&self, command: &MovePageCommand) -> Result<(), MutationResultFailure> {
        self.moved.require_answers(&command.source_path, &command.destination_path)?;
        if !command.adjust_references && self.moved.adjusted_reference_count > 0 {
            return Err(MutationResultFailure::NotThisRequest);
        }
        Ok(())
    }
}
