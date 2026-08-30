//! Finding the assets one page refers to.
//!
//! A reference here is a value that survives three separate checks: it is
//! stored in a Path or String property at or below the page's content
//! resource, it validates as an absolute repository path on its own, and the
//! node it addresses is an asset. A value that fails any of those is not a
//! reference - not a broken one, not a suspected one - and the page simply does
//! not refer to it.
//!
//! Refusing to report unresolved values is deliberate. A path-shaped string in
//! a text field is not a reference, and reporting it would make every page look
//! as though it referred to whatever its authors happened to type.
//!
//! Each match records where the reference was found, as relative property paths
//! from the content resource. A page can refer to one asset from several places
//! and that is worth knowing, so the paths are a set: unique, ascending, and
//! bounded.

use serde::de::Error as _;
use serde::{Deserialize, Serialize};

use crate::command::command_identity::CommandContract;
use crate::command::query_paths::{DiscoveryResultFailure, require_strictly_ascending};
use crate::command::repository_path::{RelativePropertyPath, RepositoryPath};
use crate::command::result_window::{ContinuationToken, ResultWindow};

/// Values one comparison of neighbours looks at.
const ADJACENT_PAIR: usize = 2;

/// Types a reference may be stored as.
pub const REFERENCE_PROPERTY_TYPES: &[&str] = &["path", "string"];

/// Returns the most places one page may refer to one asset from.
#[must_use]
pub fn maximum_asset_reference_paths() -> u64 {
    CommandContract::embedded().limit("maximum_asset_reference_paths")
}

/// Reason a reference search value could not be built.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum ReferenceSearchFailure {
    /// A match records no place the reference was found.
    #[error("a referenced asset records at least one place it was referred to from")]
    ReferencePathsEmpty,
    /// A match records the same place twice, or out of order.
    #[error("reference paths are strictly ascending by bytes, so one place is recorded once")]
    ReferencePathsNotStrictlyAscending,
    /// A match records more places than the contract allows.
    #[error("a referenced asset records at most {maximum} places", maximum = maximum_asset_reference_paths())]
    ReferencePathsTooMany,
}

/// Why a reference search produced no page.
///
/// `PageInvalid` is separate from `PageNotFound` on purpose: a node that is
/// there and readable but is not a page is a different mistake from a node that
/// is not there, and telling a caller which one happened is the difference
/// between fixing a path and fixing an assumption.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "failure", rename_all = "snake_case", deny_unknown_fields)]
pub enum PageAnchorRefusal {
    /// The page is not there.
    PageNotFound {
        /// Page that is not there.
        page_path: RepositoryPath,
    },
    /// The page is there and unreadable.
    PageAccessDenied {
        /// Page that could not be read.
        page_path: RepositoryPath,
    },
    /// The node is there and readable and is not a page.
    PageInvalid {
        /// Node that is not a page.
        page_path: RepositoryPath,
    },
}

impl PageAnchorRefusal {
    /// Returns the anchor this refusal names.
    #[must_use]
    pub fn page_path(&self) -> &RepositoryPath {
        match self {
            Self::PageNotFound { page_path }
            | Self::PageAccessDenied { page_path }
            | Self::PageInvalid { page_path } => page_path,
        }
    }

    /// Requires this refusal to answer `command`.
    ///
    /// # Errors
    ///
    /// Returns [`DiscoveryResultFailure::NotThisRequest`] when the echoed page
    /// is another request's.
    pub fn require_answers(
        &self,
        command: &FindAssetsReferencedByPageCommand,
    ) -> Result<(), DiscoveryResultFailure> {
        if *self.page_path() == command.page_path {
            Ok(())
        } else {
            Err(DiscoveryResultFailure::NotThisRequest)
        }
    }
}

/// One request to find the assets a page refers to.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FindAssetsReferencedByPageCommand {
    /// Page whose references to read.
    pub page_path: RepositoryPath,
    /// Page of results the caller is asking for, when the caller said.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub result_window: Option<ResultWindow>,
}

impl FindAssetsReferencedByPageCommand {
    /// Returns the page of results this request asks for, stated or resolved.
    #[must_use]
    pub fn resolved_window(&self) -> ResultWindow {
        self.result_window.clone().unwrap_or_default()
    }

    /// Returns whether a value stored under `property_type` is a reference.
    ///
    /// Two of the three checks happen here: the property type has to be one a
    /// reference is stored in, and the value has to validate as an absolute
    /// repository path. Whether the node it addresses is an asset is the
    /// agent's to answer, and `resolves_to_asset` says what it found.
    #[must_use]
    pub fn is_reference(property_type: &str, stored: &str, resolves_to_asset: bool) -> bool {
        REFERENCE_PROPERTY_TYPES.contains(&property_type)
            && RepositoryPath::parse(stored).is_ok_and(|path| !path.is_root())
            && resolves_to_asset
    }
}

/// One asset the page refers to.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ReferencedAssetMatch {
    /// Places the reference was found, ascending.
    pub reference_paths: Vec<RelativePropertyPath>,
    /// Asset that is referred to.
    pub repository_path: RepositoryPath,
}

impl ReferencedAssetMatch {
    /// Returns the match `repository_path` and `reference_paths` describe.
    ///
    /// # Errors
    ///
    /// Returns [`ReferenceSearchFailure::ReferencePathsEmpty`] when no place is
    /// recorded, [`ReferenceSearchFailure::ReferencePathsTooMany`] above the
    /// named bound, and
    /// [`ReferenceSearchFailure::ReferencePathsNotStrictlyAscending`] when a
    /// place repeats or sorts before its predecessor.
    pub fn new(
        repository_path: RepositoryPath,
        reference_paths: Vec<RelativePropertyPath>,
    ) -> Result<Self, ReferenceSearchFailure> {
        if reference_paths.is_empty() {
            return Err(ReferenceSearchFailure::ReferencePathsEmpty);
        }
        if u64::try_from(reference_paths.len()).unwrap_or(u64::MAX)
            > maximum_asset_reference_paths()
        {
            return Err(ReferenceSearchFailure::ReferencePathsTooMany);
        }
        let ascending = reference_paths
            .windows(ADJACENT_PAIR)
            .all(|pair| pair[0].as_text().as_bytes() < pair[1].as_text().as_bytes());
        if !ascending {
            return Err(ReferenceSearchFailure::ReferencePathsNotStrictlyAscending);
        }
        Ok(Self { reference_paths, repository_path })
    }
}

/// One match exactly as it is written on the wire.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct MatchDocument {
    /// Places the reference was found.
    reference_paths: Vec<RelativePropertyPath>,
    /// Asset that is referred to.
    repository_path: RepositoryPath,
}

impl<'de> Deserialize<'de> for ReferencedAssetMatch {
    fn deserialize<Source: serde::Deserializer<'de>>(
        deserializer: Source,
    ) -> Result<Self, Source::Error> {
        let document = MatchDocument::deserialize(deserializer)?;
        Self::new(document.repository_path, document.reference_paths).map_err(Source::Error::custom)
    }
}

/// One page of assets the page refers to.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct FindAssetsReferencedByPageResult {
    /// Matches, strictly ascending by asset path bytes.
    pub matches: Vec<ReferencedAssetMatch>,
    /// Where the next page resumes, when there is one.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub next_continuation_token: Option<ContinuationToken>,
}

impl FindAssetsReferencedByPageResult {
    /// Returns the page `matches` and `next_continuation_token` describe.
    ///
    /// # Errors
    ///
    /// Returns [`DiscoveryResultFailure::NotStrictlyAscending`] when an asset
    /// path repeats or sorts before its predecessor.
    pub fn new(
        matches: Vec<ReferencedAssetMatch>,
        next_continuation_token: Option<ContinuationToken>,
    ) -> Result<Self, DiscoveryResultFailure> {
        require_strictly_ascending(matches.iter().map(|found| &found.repository_path))?;
        Ok(Self { matches, next_continuation_token })
    }

    /// Requires this page to answer `command`.
    ///
    /// # Errors
    ///
    /// Returns [`DiscoveryResultFailure::NotThisRequest`] when a match is the
    /// page itself, which a page cannot refer to.
    pub fn require_answers(
        &self,
        command: &FindAssetsReferencedByPageCommand,
    ) -> Result<(), DiscoveryResultFailure> {
        let refers_to_itself =
            self.matches.iter().any(|found| found.repository_path == command.page_path);
        if refers_to_itself {
            return Err(DiscoveryResultFailure::NotThisRequest);
        }
        Ok(())
    }
}

/// One page exactly as it is written on the wire.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct ResultDocument {
    /// Matches this page carries.
    matches: Vec<ReferencedAssetMatch>,
    /// Where the next page resumes.
    #[serde(default)]
    next_continuation_token: Option<ContinuationToken>,
}

impl<'de> Deserialize<'de> for FindAssetsReferencedByPageResult {
    fn deserialize<Source: serde::Deserializer<'de>>(
        deserializer: Source,
    ) -> Result<Self, Source::Error> {
        let document = ResultDocument::deserialize(deserializer)?;
        Self::new(document.matches, document.next_continuation_token).map_err(Source::Error::custom)
    }
}
