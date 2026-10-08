//! The component definitions under one anchor.
//!
//! `add_component` takes a resource type, and a resource type is the path of a
//! `cq:Component` with `/apps/` or `/libs/` removed. This command lists those
//! definitions so a caller can hand `add_component` a type the repository
//! actually has.
//!
//! A match is the definition node itself. The dialog and the rendering script
//! underneath it are parts of that definition, and listing them would hand
//! `add_component` a type it cannot resolve.

use serde::de::Error as _;
use serde::{Deserialize, Serialize};

use crate::command::component_resource_type::ComponentResourceType;
use crate::command::incremental_discovery::{
    DiscoveryProgress, IncrementalDiscoveryFailure, require_page,
};
use crate::command::list_components::ComponentMatch;
use crate::command::query_paths::{DiscoveryResultFailure, anchor_contains};
use crate::command::repository_path::RepositoryPath;
use crate::command::result_window::{ContinuationToken, ResultWindow};

/// Prefix whose removal yields a resource type resolved from `/apps`.
const APPS_PREFIX: &str = "/apps/";

/// Prefix whose removal yields a resource type resolved from `/libs`.
const LIBS_PREFIX: &str = "/libs/";

/// Returns the resource type one definition path resolves as.
///
/// A definition under `/apps` or `/libs` resolves as the remainder of its path.
/// Anywhere else, the absolute path is the type, which is how Sling names a
/// type that is not on the search path.
#[must_use]
pub fn definition_resource_type(path: &RepositoryPath) -> Option<ComponentResourceType> {
    let text = path.as_text();
    let spelling =
        text.strip_prefix(APPS_PREFIX).or_else(|| text.strip_prefix(LIBS_PREFIX)).unwrap_or(text);
    if spelling.is_empty() { None } else { ComponentResourceType::parse(spelling).ok() }
}

/// One request to list the component definitions under an anchor.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ListComponentDefinitionsCommand {
    /// Page the caller is asking for, when the caller said.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub result_window: Option<ResultWindow>,
    /// Node to search under.
    pub root_path: RepositoryPath,
}

impl ListComponentDefinitionsCommand {
    /// Returns the page this request asks for, stated or resolved.
    #[must_use]
    pub fn resolved_window(&self) -> ResultWindow {
        self.result_window.clone().unwrap_or_default()
    }

    /// Returns whether `candidate` is a definition under this anchor.
    #[must_use]
    pub fn admits(&self, candidate: &RepositoryPath) -> bool {
        anchor_contains(&self.root_path, candidate)
            && *candidate != self.root_path
            && definition_resource_type(candidate).is_some()
    }

    /// Returns whether `found` is a definition this request asked for.
    #[must_use]
    pub fn admits_match(&self, found: &ComponentMatch) -> bool {
        self.admits(&found.repository_path)
            && definition_resource_type(&found.repository_path).as_ref()
                == Some(&found.resource_type)
    }
}

/// One page of component definitions under the anchor.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ListComponentDefinitionsResult {
    /// Matches in live repository provider order, without repeated paths.
    pub matches: Vec<ComponentMatch>,
    /// Where the next page resumes, when there is one.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub next_continuation_token: Option<ContinuationToken>,
    /// Explicit completeness and the work performed during this page.
    #[serde(flatten)]
    pub progress: DiscoveryProgress,
}

impl ListComponentDefinitionsResult {
    /// Returns the page `matches` and `next_continuation_token` describe.
    ///
    /// # Errors
    ///
    /// Returns an incremental discovery failure when completeness is inconsistent,
    /// counts exceed their bounds, or one page repeats a path.
    pub fn new(
        matches: Vec<ComponentMatch>,
        next_continuation_token: Option<ContinuationToken>,
        progress: DiscoveryProgress,
    ) -> Result<Self, IncrementalDiscoveryFailure> {
        require_page(
            matches.iter().map(|found| &found.repository_path),
            next_continuation_token.as_ref(),
            progress,
        )?;
        Ok(Self { matches, next_continuation_token, progress })
    }

    /// Requires this page to answer `command`.
    ///
    /// # Errors
    ///
    /// Returns [`DiscoveryResultFailure::NotThisRequest`] when a match is not a
    /// component definition under the anchor, or names a resource type other
    /// than the one its path resolves as.
    pub fn require_answers(
        &self,
        command: &ListComponentDefinitionsCommand,
    ) -> Result<(), DiscoveryResultFailure> {
        if self.matches.iter().all(|found| command.admits_match(found)) {
            Ok(())
        } else {
            Err(DiscoveryResultFailure::NotThisRequest)
        }
    }
}

/// One page exactly as it is written on the wire.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct ResultDocument {
    /// Matches this page carries.
    matches: Vec<ComponentMatch>,
    /// Where the next page resumes.
    #[serde(default)]
    next_continuation_token: Option<ContinuationToken>,
    /// Whether the retained traversal ended.
    complete: bool,
    /// Nodes examined while producing this page.
    examined_nodes: u64,
}

impl<'de> Deserialize<'de> for ListComponentDefinitionsResult {
    fn deserialize<Source: serde::Deserializer<'de>>(
        deserializer: Source,
    ) -> Result<Self, Source::Error> {
        let document = ResultDocument::deserialize(deserializer)?;
        Self::new(
            document.matches,
            document.next_continuation_token,
            DiscoveryProgress {
                complete: document.complete,
                examined_nodes: document.examined_nodes,
            },
        )
        .map_err(Source::Error::custom)
    }
}
