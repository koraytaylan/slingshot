//! Finding pages built from one template.
//!
//! A page records its template as a single property on its content resource,
//! and this command asks whether that property equals the template the caller
//! named. The comparison is on the validated repository path, so a stored value
//! that happens to spell the same characters differently - a trailing slash, a
//! doubled separator, a same-name-sibling suffix - is a different template and
//! does not match.
//!
//! The property may be stored as a JCR Path or as a String, because both occur
//! in real content. Neither is preferred and neither is coerced: the stored text
//! has to validate as a repository path on its own before it is compared. A
//! missing property, a multi-valued one, or one of any other type does not
//! match, rather than matching a page that merely has no template recorded.

use serde::de::Error as _;
use serde::{Deserialize, Serialize};

use crate::command::find_pages_containing_phrase::PageMatch;
use crate::command::query_paths::{
    DiscoveryResultFailure, anchor_contains, require_strictly_ascending,
};
use crate::command::repository_path::RepositoryPath;
use crate::command::result_window::{ContinuationToken, ResultWindow};

/// Property a page's template is recorded in, on its content resource.
pub const PAGE_TEMPLATE_PROPERTY: &str = "cq:template";

/// Types that property may be stored as.
pub const TEMPLATE_PROPERTY_TYPES: &[&str] = &["path", "string"];

/// One request to find pages built from a template.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FindPagesByTemplateCommand {
    /// Page the caller is asking for, when the caller said.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub result_window: Option<ResultWindow>,
    /// Node to search at and below.
    pub root_path: RepositoryPath,
    /// Template a page must record.
    pub template_path: RepositoryPath,
}

impl FindPagesByTemplateCommand {
    /// Returns the page this request asks for, stated or resolved.
    #[must_use]
    pub fn resolved_window(&self) -> ResultWindow {
        self.result_window.clone().unwrap_or_default()
    }

    /// Returns whether a page recording `stored` under `property_type` matches.
    ///
    /// The stored text is validated as a repository path before it is compared,
    /// so equality is between two addresses rather than between two strings that
    /// look alike.
    #[must_use]
    pub fn matches_recorded(&self, property_type: &str, stored: Option<&str>) -> bool {
        if !TEMPLATE_PROPERTY_TYPES.contains(&property_type) {
            return false;
        }
        stored.is_some_and(|stored| {
            RepositoryPath::parse(stored).is_ok_and(|recorded| recorded == self.template_path)
        })
    }
}

/// One page of pages built from the template.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct FindPagesByTemplateResult {
    /// Matches, strictly ascending by page path bytes.
    pub matches: Vec<PageMatch>,
    /// Where the next page resumes, when there is one.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub next_continuation_token: Option<ContinuationToken>,
}

impl FindPagesByTemplateResult {
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
        command: &FindPagesByTemplateCommand,
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

impl<'de> Deserialize<'de> for FindPagesByTemplateResult {
    fn deserialize<Source: serde::Deserializer<'de>>(
        deserializer: Source,
    ) -> Result<Self, Source::Error> {
        let document = ResultDocument::deserialize(deserializer)?;
        Self::new(document.matches, document.next_continuation_token).map_err(Source::Error::custom)
    }
}
