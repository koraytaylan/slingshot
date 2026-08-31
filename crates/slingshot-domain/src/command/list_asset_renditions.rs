//! What an asset actually offers a consumer.
//!
//! A page refers to an asset and a browser fetches a rendition, and there was no
//! way to ask which renditions exist. Loading the asset's subtree would answer
//! it accidentally, at the cost of every property on the way; this answers it on
//! purpose, as a page of rows ordered by rendition name.
//!
//! The order is over the rendition name rather than the address, because that is
//! the name a consumer asks for and the one a caller resumes from. Every address
//! reported is required to be under the asset the request named: a rendition of
//! something else is not this asset's rendition however plausible it looks.

use serde::de::Error as _;
use serde::{Deserialize, Serialize};

use crate::command::command_identity::CommandContract;
use crate::command::find_assets_by_metadata::AssetByteLength;
use crate::command::operational_listing::{ListingResultFailure, require_strictly_ascending_text};
use crate::command::query_paths::anchor_contains;
use crate::command::repository_path::{PathFailure, RepositoryPath, accept_within, address_value};
use crate::command::resource_mutation::MediaType;
use crate::command::result_window::{ContinuationToken, ResultWindow};

address_value!(
    /// The name one rendition of an asset is asked for by.
    RenditionName,
    "rendition name"
);

impl RenditionName {
    /// Validates one rendition name.
    ///
    /// # Errors
    ///
    /// Returns [`PathFailure`] when the name is empty, longer than the contract
    /// allows, not already in normalization form C, carries a separator or a
    /// control, or has a leading or trailing ASCII space.
    pub fn parse(name: &str) -> Result<Self, PathFailure> {
        let bound = CommandContract::embedded().limit("maximum_rendition_name_bytes");
        accept_within(name, bound, Self::role(), "bytes")?;
        let refuse = |field| PathFailure::at(Self::role(), field);
        if name.starts_with(' ') || name.ends_with(' ') {
            return Err(refuse("space"));
        }
        if name.chars().any(|character| character == '/' || character.is_control()) {
            return Err(refuse("character"));
        }
        Ok(Self::from_accepted(name))
    }
}

/// One request to list an asset's renditions.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ListAssetRenditionsCommand {
    /// Asset whose renditions are listed.
    pub asset_path: RepositoryPath,
    /// Page the caller is asking for, when the caller said.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub result_window: Option<ResultWindow>,
}

impl ListAssetRenditionsCommand {
    /// Returns the page this request asks for, stated or resolved.
    #[must_use]
    pub fn resolved_window(&self) -> ResultWindow {
        self.result_window.clone().unwrap_or_default()
    }
}

/// One rendition of one asset.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RenditionMatch {
    /// How many bytes this rendition holds.
    pub byte_length: AssetByteLength,
    /// What kind of thing those bytes are.
    pub media_type: MediaType,
    /// The name this rendition is asked for by.
    pub name: RenditionName,
    /// Where this rendition is.
    pub repository_path: RepositoryPath,
}

/// One page of an asset's renditions.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ListAssetRenditionsResult {
    /// Matches, strictly ascending by rendition name bytes.
    pub matches: Vec<RenditionMatch>,
    /// Where the next page resumes, when there is one.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub next_continuation_token: Option<ContinuationToken>,
}

impl ListAssetRenditionsResult {
    /// Returns the page `matches` and `next_continuation_token` describe.
    ///
    /// # Errors
    ///
    /// Returns [`ListingResultFailure::NotStrictlyAscending`] when a name
    /// repeats or sorts before its predecessor.
    pub fn new(
        matches: Vec<RenditionMatch>,
        next_continuation_token: Option<ContinuationToken>,
    ) -> Result<Self, ListingResultFailure> {
        require_strictly_ascending_text(matches.iter().map(|found| found.name.as_text()))?;
        Ok(Self { matches, next_continuation_token })
    }

    /// Requires this page to answer `command`.
    ///
    /// # Errors
    ///
    /// Returns [`ListingResultFailure::NotThisRequest`] when a rendition is
    /// addressed outside the asset the command named.
    pub fn require_answers(
        &self,
        command: &ListAssetRenditionsCommand,
    ) -> Result<(), ListingResultFailure> {
        let within = self
            .matches
            .iter()
            .all(|found| anchor_contains(&command.asset_path, &found.repository_path));
        if within { Ok(()) } else { Err(ListingResultFailure::NotThisRequest) }
    }
}

/// One page exactly as it is written on the wire.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct ResultDocument {
    /// Matches this page carries.
    matches: Vec<RenditionMatch>,
    /// Where the next page resumes.
    #[serde(default)]
    next_continuation_token: Option<ContinuationToken>,
}

impl<'de> Deserialize<'de> for ListAssetRenditionsResult {
    fn deserialize<Source: serde::Deserializer<'de>>(
        deserializer: Source,
    ) -> Result<Self, Source::Error> {
        let document = ResultDocument::deserialize(deserializer)?;
        Self::new(document.matches, document.next_continuation_token).map_err(Source::Error::custom)
    }
}
