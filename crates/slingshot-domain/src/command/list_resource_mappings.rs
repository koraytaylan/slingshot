//! The entries that decide where a request lands.
//!
//! Nobody can reason about a resolution problem without seeing the entries that
//! decide it, and reading them out of the repository by hand means knowing where
//! a particular deployment keeps them - which is the thing the person with the
//! problem does not know.
//!
//! The command takes no filter. A pattern-shaped argument would invite a caller
//! to believe a pattern had been matched when it had only been listed, and the
//! difference between those two is the whole question a mapping problem is
//! about. `resolve_resource_path` answers the matching question.

use serde::de::Error as _;
use serde::{Deserialize, Serialize};

use crate::command::operational_listing::{ListingResultFailure, require_strictly_ascending_text};
use crate::command::resource_mapping_entry::ResourceMappingEntry;
use crate::command::result_window::{ContinuationToken, ResultWindow};

/// One request to list the effective resource mapping.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ListResourceMappingsCommand {
    /// Page the caller is asking for, when the caller said.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub result_window: Option<ResultWindow>,
}

impl ListResourceMappingsCommand {
    /// Returns the page this request asks for, stated or resolved.
    #[must_use]
    pub fn resolved_window(&self) -> ResultWindow {
        self.result_window.clone().unwrap_or_default()
    }
}

/// One page of mapping entries.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ListResourceMappingsResult {
    /// Entries, strictly ascending by entry address bytes.
    pub entries: Vec<ResourceMappingEntry>,
    /// Where the next page resumes, when there is one.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub next_continuation_token: Option<ContinuationToken>,
}

impl ListResourceMappingsResult {
    /// Returns the page these entries describe.
    ///
    /// # Errors
    ///
    /// Returns [`ListingResultFailure::NotStrictlyAscending`] when an entry
    /// address repeats or sorts before its predecessor.
    pub fn new(
        entries: Vec<ResourceMappingEntry>,
        next_continuation_token: Option<ContinuationToken>,
    ) -> Result<Self, ListingResultFailure> {
        require_strictly_ascending_text(entries.iter().map(|entry| entry.entry_path.as_text()))?;
        Ok(Self { entries, next_continuation_token })
    }
}

/// One page exactly as it is written on the wire.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct ResultDocument {
    /// Entries this page carries.
    entries: Vec<ResourceMappingEntry>,
    /// Where the next page resumes.
    #[serde(default)]
    next_continuation_token: Option<ContinuationToken>,
}

impl<'de> Deserialize<'de> for ListResourceMappingsResult {
    fn deserialize<Source: serde::Deserializer<'de>>(
        deserializer: Source,
    ) -> Result<Self, Source::Error> {
        let document = ResultDocument::deserialize(deserializer)?;
        Self::new(document.entries, document.next_continuation_token).map_err(Source::Error::custom)
    }
}
