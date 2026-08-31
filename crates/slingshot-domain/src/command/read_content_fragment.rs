//! Reading a fragment as what it is rather than as where it is stored.
//!
//! `load_content_as_json` can return a fragment's subtree, and a caller then has
//! to know that elements live under one child, variations under another, and the
//! model under a property - which is storage, not meaning. This command answers
//! in the fragment's own vocabulary: a model, a title, a variation, and the
//! elements that variation holds.
//!
//! The result names the variation it read even when the request named none, so a
//! caller learns which variation the master is instead of assuming.

use serde::{Deserialize, Serialize};

use crate::command::content_fragment_element::{
    ContentFragmentElementValues, ContentFragmentFailure, ContentFragmentVariationName,
};
use crate::command::find_pages_containing_phrase::PageTitle;
use crate::command::repository_path::RepositoryPath;

/// One request to read a content fragment.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ReadContentFragmentCommand {
    /// Fragment to read.
    pub fragment_path: RepositoryPath,
    /// Variation to read, or the master variation when absent.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub variation_name: Option<ContentFragmentVariationName>,
}

/// Why a content fragment was not read.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReadContentFragmentFailure {
    /// Nothing is at the address.
    FragmentNotFound,
    /// Something is there and this caller may not read it.
    FragmentAccessDenied,
    /// Something is there and it is not a content fragment.
    FragmentInvalid,
    /// The fragment has no variation of that name.
    VariationNotFound,
    /// The fragment holds more than this contract will return at once.
    ResultBudgetExceeded,
}

/// One refused content fragment read.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ReadContentFragmentRefusal {
    /// Why it was refused.
    pub failure: ReadContentFragmentFailure,
    /// Fragment this request named.
    pub fragment_path: RepositoryPath,
}

impl ReadContentFragmentRefusal {
    /// Requires this refusal to answer `command`.
    ///
    /// # Errors
    ///
    /// Returns [`ContentFragmentFailure::NotThisRequest`] when it names another
    /// request's fragment, and when it reports a missing variation for a request
    /// that named none - a request about the master is a request about a
    /// variation that is always there.
    pub fn require_answers(
        &self,
        command: &ReadContentFragmentCommand,
    ) -> Result<(), ContentFragmentFailure> {
        let sought = matches!(self.failure, ReadContentFragmentFailure::VariationNotFound);
        if self.fragment_path != command.fragment_path
            || (sought && command.variation_name.is_none())
        {
            return Err(ContentFragmentFailure::NotThisRequest);
        }
        Ok(())
    }
}

/// What one content fragment holds.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ReadContentFragmentResult {
    /// Elements the variation holds, by name.
    pub elements: ContentFragmentElementValues,
    /// Model the fragment answers to.
    pub model_path: RepositoryPath,
    /// Fragment that was read.
    pub repository_path: RepositoryPath,
    /// Title the fragment records.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<PageTitle>,
    /// Variation that was read, which the request may not have named.
    pub variation_name: ContentFragmentVariationName,
}

impl ReadContentFragmentResult {
    /// Requires this result to answer `command`.
    ///
    /// # Errors
    ///
    /// Returns [`ContentFragmentFailure::NotThisRequest`] when it names another
    /// request's fragment, or another variation than the one requested. A
    /// request that named no variation accepts whichever one the author read,
    /// because the master's name is the author's answer.
    pub fn require_answers(
        &self,
        command: &ReadContentFragmentCommand,
    ) -> Result<(), ContentFragmentFailure> {
        let asked_elsewhere =
            command.variation_name.as_ref().is_some_and(|asked| *asked != self.variation_name);
        if self.repository_path != command.fragment_path || asked_elsewhere {
            return Err(ContentFragmentFailure::NotThisRequest);
        }
        Ok(())
    }
}
