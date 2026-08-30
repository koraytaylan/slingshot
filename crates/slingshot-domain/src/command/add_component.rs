//! Adding one component to a page, in a place where order means something.
//!
//! The component path is computed as `page/jcr:content[/descendant]/name`, and
//! the descendant address is relative to the content resource. A descendant
//! that itself begins with `jcr:content` is refused, because it would produce a
//! path with the segment twice - a mistake that reads as reasonable and creates
//! content nobody will find.
//!
//! # Ordering, and why the parent has to say
//!
//! The component is appended last. That only means anything if the parent node
//! type reports orderable children: on a non-orderable type the repository is
//! free to return children in any order it likes, and "last" would be a claim
//! the repository never made. So a non-orderable parent is refused outright
//! rather than appended to hopefully, and nothing infers order by observing
//! what a query happened to return.
//!
//! The resource type comes from its own field and a property map cannot
//! override it, for the same reason the page title cannot be redefined: two
//! parts of one request would disagree with nothing to say which wins.

use serde::{Deserialize, Serialize};

use crate::command::component_resource_type::ComponentResourceType;
use crate::command::create_page::{
    MutationFailure, MutationProperties, PAGE_CONTENT_CHILD, PAGE_PRIMARY_NODE_TYPE,
};
use crate::command::repository_path::{
    ComponentName, PathFailure, RepositoryName, RepositoryPath, RepositoryPathSegment,
    RepositoryRelativePath,
};

/// Property a component records its type in, which no property map may set.
pub const COMPONENT_RESOURCE_TYPE_PROPERTY: &str = "sling:resourceType";

/// Where under a page's content resource a component is created.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum PageContentParent {
    /// The content resource itself.
    ContentRoot(ContentRootMarker),
    /// A descendant of it, addressed relatively.
    Descendant(RepositoryRelativePath),
}

/// The spelling that names the content resource itself.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ContentRootMarker {
    /// The content resource itself.
    ContentRoot,
}

impl PageContentParent {
    /// Returns the descendant address this names, when it names one.
    #[must_use]
    pub fn descendant(&self) -> Option<&RepositoryRelativePath> {
        match self {
            Self::ContentRoot(_) => None,
            Self::Descendant(path) => Some(path),
        }
    }

    /// Returns whether this address would repeat the content resource segment.
    ///
    /// A descendant beginning with `jcr:content`, with or without a sibling
    /// suffix, would produce `jcr:content/jcr:content/...`, which addresses
    /// something that is almost certainly not what was meant.
    #[must_use]
    pub fn repeats_content_segment(&self) -> bool {
        self.descendant()
            .and_then(|path| descendant_segments(path).into_iter().next())
            .is_some_and(|segment| segment.starts_with(PAGE_CONTENT_CHILD))
    }
}

/// Returns the segments one relative address is spelled with.
///
/// The address was validated when it was built, so splitting is reading it
/// back rather than parsing it again.
fn descendant_segments(path: &RepositoryRelativePath) -> Vec<&str> {
    path.as_text().split('/').collect()
}

/// One request to add a component.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AddComponentCommand {
    /// Name of the component to create.
    pub component_name: ComponentName,
    /// Where under the page's content resource to create it.
    pub content_parent: PageContentParent,
    /// Page to add it to.
    pub page_path: RepositoryPath,
    /// Properties to apply to the new component.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub properties: Option<MutationProperties>,
    /// Type the new component records.
    pub resource_type: ComponentResourceType,
}

impl AddComponentCommand {
    /// Returns where this command would create its component.
    ///
    /// # Errors
    ///
    /// Returns the path failure when any segment of the computed address is not
    /// one the path grammar accepts.
    pub fn target_path(&self) -> Result<RepositoryPath, PathFailure> {
        let content = RepositoryPathSegment::parse(PAGE_CONTENT_CHILD)?;
        let mut path = self.page_path.address_child(&content)?;
        if let Some(descendant) = self.content_parent.descendant() {
            for spelling in descendant_segments(descendant) {
                path = path.address_child(&RepositoryPathSegment::parse(spelling)?)?;
            }
        }
        let name = RepositoryName::parse(self.component_name.as_text())?;
        path.creatable_child(&name)
    }

    /// Requires the property map to leave the resource type alone.
    ///
    /// # Errors
    ///
    /// Returns [`MutationFailure::PropertyReserved`] when the map carries the
    /// property this command sets from its own field.
    pub fn require_resource_type_not_overridden(&self) -> Result<(), MutationFailure> {
        let overridden = self.properties.as_ref().is_some_and(|properties| {
            properties.values().contains_key(COMPONENT_RESOURCE_TYPE_PROPERTY)
        });
        if overridden { Err(MutationFailure::PropertyReserved) } else { Ok(()) }
    }

    /// Returns the primary type a page must have for this command to apply.
    #[must_use]
    pub fn required_page_primary_node_type() -> &'static str {
        PAGE_PRIMARY_NODE_TYPE
    }
}

/// Why a component was not added.
///
/// Every category but the last is emitted only with authoritative evidence that
/// this operation changed neither content nor ordering.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "failure", rename_all = "snake_case", deny_unknown_fields)]
pub enum AddComponentRefusal {
    /// The page is not there.
    PageNotFound {
        /// Target this command computed.
        target_path: RepositoryPath,
    },
    /// The node is there and is not a page.
    PageInvalid {
        /// Target this command computed.
        target_path: RepositoryPath,
    },
    /// The parent resource is not there.
    ParentNotFound {
        /// Target this command computed.
        target_path: RepositoryPath,
    },
    /// The parent is there and unwritable.
    ParentAccessDenied {
        /// Target this command computed.
        target_path: RepositoryPath,
    },
    /// The parent's type does not report orderable children.
    ///
    /// Refused rather than appended to, because "last" would be a claim the
    /// repository never made.
    ParentNotOrderable {
        /// Target this command computed.
        target_path: RepositoryPath,
    },
    /// Something is already at the target.
    TargetAlreadyExists {
        /// Target this command computed.
        target_path: RepositoryPath,
    },
    /// A property could not be applied.
    PropertyRejected {
        /// Target this command computed.
        target_path: RepositoryPath,
    },
    /// The save failed, provably without committing.
    RepositoryCommitFailed {
        /// Target this command computed.
        target_path: RepositoryPath,
    },
    /// Nobody can tell whether the save committed.
    MutationOutcomeUnknown {
        /// Target this command computed.
        target_path: RepositoryPath,
    },
}

impl AddComponentRefusal {
    /// Returns the target this refusal names.
    #[must_use]
    pub fn target_path(&self) -> &RepositoryPath {
        match self {
            Self::PageNotFound { target_path }
            | Self::PageInvalid { target_path }
            | Self::ParentNotFound { target_path }
            | Self::ParentAccessDenied { target_path }
            | Self::ParentNotOrderable { target_path }
            | Self::TargetAlreadyExists { target_path }
            | Self::PropertyRejected { target_path }
            | Self::RepositoryCommitFailed { target_path }
            | Self::MutationOutcomeUnknown { target_path } => target_path,
        }
    }

    /// Returns whether this refusal proves the operation changed nothing.
    #[must_use]
    pub fn proves_no_effect(&self) -> bool {
        !matches!(self, Self::MutationOutcomeUnknown { .. })
    }

    /// Requires this refusal to answer `command`.
    ///
    /// # Errors
    ///
    /// Returns [`MutationFailure::NotThisRequest`] when the named target is not
    /// the one the command computes.
    pub fn require_answers(&self, command: &AddComponentCommand) -> Result<(), MutationFailure> {
        let expected = command.target_path().map_err(|_| MutationFailure::NotThisRequest)?;
        if *self.target_path() == expected { Ok(()) } else { Err(MutationFailure::NotThisRequest) }
    }
}

/// What a completed addition produced.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AddComponentResult {
    /// Component that was created.
    pub target_path: RepositoryPath,
}

impl AddComponentResult {
    /// Requires this result to answer `command`.
    ///
    /// # Errors
    ///
    /// Returns [`MutationFailure::NotThisRequest`] when the named target is not
    /// the one the command computes.
    pub fn require_answers(&self, command: &AddComponentCommand) -> Result<(), MutationFailure> {
        let expected = command.target_path().map_err(|_| MutationFailure::NotThisRequest)?;
        if self.target_path == expected { Ok(()) } else { Err(MutationFailure::NotThisRequest) }
    }
}
