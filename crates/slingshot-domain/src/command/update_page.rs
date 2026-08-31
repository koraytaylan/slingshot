//! Changing a page that already exists.
//!
//! `create_page` writes a title and initial properties to a new page's
//! `jcr:content` resource and then never touches it again. This command is the
//! other half: it writes to that same resource, on a page somebody already made,
//! and it computes the resource address from the page address rather than
//! accepting one - so a caller cannot aim content at the page node, where it
//! would be stored and never rendered.
//!
//! # Assigning and removing are one request and not two
//!
//! A caller may set properties, remove properties, and set a title in one
//! request. What it may not do is name one property in both documents: there is
//! no order between them that a caller could rely on, and picking one would make
//! the same request mean different things to two implementations. It is refused.
//!
//! A request that assigns nothing, removes nothing, and sets no title is refused
//! too. It would return success having done nothing, which is the answer least
//! likely to be noticed and most likely to be wrong.
//!
//! And a request that sets the title in both places - the `title` field and the
//! title property inside the property document - is refused for the reason
//! `create_page` refuses it: two writes to one property with no order between
//! them is a request that means two things.
//!
//! # What the failures say
//!
//! Every category but the last is emitted only with authoritative evidence that
//! this operation changed nothing. `mutation_outcome_unknown` says the opposite:
//! something may have changed, and it never authorizes a retry.

use serde::{Deserialize, Serialize};

use crate::command::create_page::{MutationProperties, PAGE_CONTENT_CHILD};
use crate::command::find_pages_containing_phrase::PageTitle;
use crate::command::repository_path::{PathFailure, RepositoryName, RepositoryPath};
use crate::command::resource_mutation::{
    MutationResultFailure, PropertyMutationFailure, RemovedPropertyNames, ResourceMutationResult,
    require_property_mutation, require_title_not_redefined,
};

/// One request to change an existing page.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct UpdatePageCommand {
    /// Page whose content resource is changed.
    pub page_path: RepositoryPath,
    /// Properties to assign to that resource.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub properties: Option<MutationProperties>,
    /// Properties to remove from that resource.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub removed_property_names: Option<RemovedPropertyNames>,
    /// Title to write to that resource.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<PageTitle>,
}

impl UpdatePageCommand {
    /// Returns the content resource this command writes to.
    ///
    /// Computed from the page address, so a result echoing it echoes something
    /// the request determined rather than something a caller asserted.
    ///
    /// # Errors
    ///
    /// Returns the path failure when the page address cannot take the content
    /// child, which is the same refusal the path grammar would make.
    pub fn content_path(&self) -> Result<RepositoryPath, PathFailure> {
        let child = RepositoryName::parse(PAGE_CONTENT_CHILD)?;
        self.page_path.creatable_child(&child)
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

/// Why a page was not changed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum UpdatePageFailure {
    /// The page is not there.
    PageNotFound,
    /// The page is there and unwritable.
    PageAccessDenied,
    /// The address is there and is not a page.
    PageInvalid,
    /// A property could not be applied.
    PropertyRejected,
    /// A property named for removal is one the repository keeps.
    PropertyNotRemovable,
    /// The save failed, provably without committing.
    RepositoryCommitFailed,
    /// Nobody can tell whether the save committed.
    MutationOutcomeUnknown,
}

/// One refused page update.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct UpdatePageRefusal {
    /// Why it was refused.
    pub failure: UpdatePageFailure,
    /// Page this request named.
    pub page_path: RepositoryPath,
}

impl UpdatePageRefusal {
    /// Returns whether this refusal proves the operation changed nothing.
    #[must_use]
    pub fn proves_no_effect(&self) -> bool {
        !matches!(self.failure, UpdatePageFailure::MutationOutcomeUnknown)
    }

    /// Requires this refusal to answer `command`.
    ///
    /// # Errors
    ///
    /// Returns [`MutationResultFailure::NotThisRequest`] when it names another
    /// request's page.
    pub fn require_answers(
        &self,
        command: &UpdatePageCommand,
    ) -> Result<(), MutationResultFailure> {
        if self.page_path == command.page_path {
            Ok(())
        } else {
            Err(MutationResultFailure::NotThisRequest)
        }
    }
}

/// What a completed page update changed.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct UpdatePageResult {
    /// Content resource this update wrote to.
    pub mutated: ResourceMutationResult,
}

impl UpdatePageResult {
    /// Requires this result to answer `command`.
    ///
    /// # Errors
    ///
    /// Returns [`MutationResultFailure::NotThisRequest`] when it names an
    /// address this request did not determine.
    pub fn require_answers(
        &self,
        command: &UpdatePageCommand,
    ) -> Result<(), MutationResultFailure> {
        let expected = command.content_path().map_err(|_| MutationResultFailure::NotThisRequest)?;
        self.mutated.require_answers(&expected)
    }
}
