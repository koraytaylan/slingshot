//! Finding pages that use particular components.
//!
//! A component resource is the page content resource, or any descendant of it,
//! carrying a single `sling:resourceType` string. The stored string is compared
//! complete and exactly: there is no resource-super-type expansion, so asking
//! for a base type does not find pages using something derived from it. That is
//! the conservative answer - super-type resolution depends on the deployed
//! application, and a discovery command that guessed at it would return
//! different results against two environments holding the same content.
//!
//! `Any` and `All` differ in a way worth stating. `All` requires every requested
//! type to appear somewhere in the page's subtree, not all on one resource - a
//! page built from a header, a body, and a footer uses all three, and no single
//! resource does.

use serde::de::Error as _;
use serde::{Deserialize, Serialize};

use crate::command::command_identity::CommandContract;
use crate::command::component_resource_type::ComponentResourceType;
use crate::command::find_pages_containing_phrase::PageMatch;
use crate::command::query_paths::{
    DiscoveryResultFailure, anchor_contains, require_strictly_ascending,
};
use crate::command::repository_path::RepositoryPath;
use crate::command::result_window::{ContinuationToken, ResultWindow};

/// Values one comparison of neighbours looks at.
const ADJACENT_PAIR: usize = 2;

/// Property a component resource records its type in.
pub const COMPONENT_RESOURCE_TYPE_PROPERTY: &str = "sling:resourceType";

/// Returns the most component types one request may name.
#[must_use]
pub fn maximum_requested_component_resource_types() -> u64 {
    CommandContract::embedded().limit("maximum_requested_component_resource_types")
}

/// Reason a component search value could not be built.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum ComponentSearchFailure {
    /// A request named no component type.
    #[error("a component search names at least one resource type")]
    TypesEmpty,
    /// A request named one type twice.
    #[error("a component search names each resource type once")]
    TypesNotUnique,
    /// A request named types out of canonical order.
    #[error("requested resource types arrive in ascending byte order, so one set has one spelling")]
    TypesNotSorted,
    /// A request named more types than the contract allows.
    #[error("a component search names at most {maximum} resource types", maximum = maximum_requested_component_resource_types())]
    TypesTooMany,
}

/// How many of the requested types a page must use.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ComponentMatchMode {
    /// One of them, anywhere in the page.
    Any,
    /// Every one of them, anywhere in the page.
    All,
}

/// The component types one request names.
///
/// Canonical on the wire: strictly ascending, no repeats. A merely permuted
/// spelling is refused rather than sorted, because the set participates in the
/// digest a continuation token is bound to, and two spellings of one set would
/// be two queries that could not resume each other.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(transparent)]
pub struct RequestedComponentResourceTypes {
    /// The types, ascending.
    types: Vec<ComponentResourceType>,
}

impl RequestedComponentResourceTypes {
    /// Returns the set `types` spell, sorting them once.
    ///
    /// # Errors
    ///
    /// Returns [`ComponentSearchFailure::TypesEmpty`],
    /// [`ComponentSearchFailure::TypesTooMany`], or
    /// [`ComponentSearchFailure::TypesNotUnique`].
    pub fn new(mut types: Vec<ComponentResourceType>) -> Result<Self, ComponentSearchFailure> {
        types.sort_by(|left, right| left.as_text().as_bytes().cmp(right.as_text().as_bytes()));
        Self::accept(types)
    }

    /// Returns the set `types` spell, requiring them already canonical.
    ///
    /// The order is checked without strictness so a repeated type is reported
    /// as the repeat it is rather than as a spelling out of order, which would
    /// send a caller looking for the wrong mistake.
    ///
    /// # Errors
    ///
    /// Returns [`ComponentSearchFailure::TypesNotSorted`] in addition to
    /// whatever [`Self::new`] refuses.
    pub fn canonical(types: Vec<ComponentResourceType>) -> Result<Self, ComponentSearchFailure> {
        let ascending = types
            .windows(ADJACENT_PAIR)
            .all(|pair| pair[0].as_text().as_bytes() <= pair[1].as_text().as_bytes());
        if !ascending {
            return Err(ComponentSearchFailure::TypesNotSorted);
        }
        Self::accept(types)
    }

    /// Accepts one already-ordered collection.
    fn accept(types: Vec<ComponentResourceType>) -> Result<Self, ComponentSearchFailure> {
        if types.is_empty() {
            return Err(ComponentSearchFailure::TypesEmpty);
        }
        if u64::try_from(types.len()).unwrap_or(u64::MAX)
            > maximum_requested_component_resource_types()
        {
            return Err(ComponentSearchFailure::TypesTooMany);
        }
        if types.windows(ADJACENT_PAIR).any(|pair| pair[0] == pair[1]) {
            return Err(ComponentSearchFailure::TypesNotUnique);
        }
        Ok(Self { types })
    }

    /// Returns the types, ascending.
    #[must_use]
    pub fn types(&self) -> &[ComponentResourceType] {
        &self.types
    }
}

impl<'de> Deserialize<'de> for RequestedComponentResourceTypes {
    fn deserialize<Source: serde::Deserializer<'de>>(
        deserializer: Source,
    ) -> Result<Self, Source::Error> {
        let types = Vec::<ComponentResourceType>::deserialize(deserializer)?;
        Self::canonical(types).map_err(Source::Error::custom)
    }
}

/// One request to find pages using components.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FindPagesUsingComponentsCommand {
    /// How many of the requested types a page must use.
    pub match_mode: ComponentMatchMode,
    /// Types to look for.
    pub resource_types: RequestedComponentResourceTypes,
    /// Page the caller is asking for, when the caller said.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub result_window: Option<ResultWindow>,
    /// Node to search at and below.
    pub root_path: RepositoryPath,
}

impl FindPagesUsingComponentsCommand {
    /// Returns the page this request asks for, stated or resolved.
    #[must_use]
    pub fn resolved_window(&self) -> ResultWindow {
        self.result_window.clone().unwrap_or_default()
    }

    /// Returns whether a page whose subtree uses `used` matches.
    ///
    /// `used` is every complete resource type found anywhere at or below the
    /// page content resource, so `All` is satisfied across resources rather
    /// than on any one of them.
    #[must_use]
    pub fn matches_used(&self, used: &[ComponentResourceType]) -> bool {
        let present = |wanted: &ComponentResourceType| used.contains(wanted);
        match self.match_mode {
            ComponentMatchMode::Any => self.resource_types.types().iter().any(present),
            ComponentMatchMode::All => self.resource_types.types().iter().all(present),
        }
    }
}

/// One page of pages using the components.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct FindPagesUsingComponentsResult {
    /// Matches, strictly ascending by page path bytes.
    pub matches: Vec<PageMatch>,
    /// Where the next page resumes, when there is one.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub next_continuation_token: Option<ContinuationToken>,
}

impl FindPagesUsingComponentsResult {
    /// Returns the page `matches` and `next_continuation_token` describe.
    ///
    /// # Errors
    ///
    /// Returns [`DiscoveryResultFailure::NotStrictlyAscending`] when a path
    /// repeats or sorts before its predecessor.
    pub fn new(
        matches: Vec<PageMatch>,
        next_continuation_token: Option<ContinuationToken>,
    ) -> Result<Self, DiscoveryResultFailure> {
        require_strictly_ascending(matches.iter().map(|found| &found.repository_path))?;
        Ok(Self { matches, next_continuation_token })
    }

    /// Requires this page to answer `command`.
    ///
    /// # Errors
    ///
    /// Returns [`DiscoveryResultFailure::NotThisRequest`] when a match lies
    /// outside the anchor the command asked about.
    pub fn require_answers(
        &self,
        command: &FindPagesUsingComponentsCommand,
    ) -> Result<(), DiscoveryResultFailure> {
        let within = self
            .matches
            .iter()
            .all(|found| anchor_contains(&command.root_path, &found.repository_path));
        if within { Ok(()) } else { Err(DiscoveryResultFailure::NotThisRequest) }
    }
}

/// One page exactly as it is written on the wire.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct ResultDocument {
    /// Matches this page carries.
    matches: Vec<PageMatch>,
    /// Where the next page resumes.
    #[serde(default)]
    next_continuation_token: Option<ContinuationToken>,
}

impl<'de> Deserialize<'de> for FindPagesUsingComponentsResult {
    fn deserialize<Source: serde::Deserializer<'de>>(
        deserializer: Source,
    ) -> Result<Self, Source::Error> {
        let document = ResultDocument::deserialize(deserializer)?;
        Self::new(document.matches, document.next_continuation_token).map_err(Source::Error::custom)
    }
}
