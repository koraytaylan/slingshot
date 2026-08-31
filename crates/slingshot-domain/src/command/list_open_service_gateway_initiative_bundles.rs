//! What is installed, and which of it is actually running.
//!
//! After a deployment the first question is which bundles are not active, and
//! there was no way to ask it. This command answers it as a page of rows,
//! filtered by a symbolic-name prefix and by a set of states.
//!
//! The order is by symbolic name and then by version, because a deployment can
//! hold two versions of one bundle and a listing ordered by name alone would
//! have no defined order between them - which is exactly the case where a
//! resumed page silently skips one.

use serde::de::Error as _;
use serde::{Deserialize, Serialize};

use crate::command::operational_listing::ListingResultFailure;
use crate::command::platform_service_identity::{
    BundleState, BundleSymbolicName, BundleVersion, RequestedBundleStates,
};
use crate::command::result_window::{ContinuationToken, ResultWindow};

/// One request to list bundles.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ListOpenServiceGatewayInitiativeBundlesCommand {
    /// Page the caller is asking for, when the caller said.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub result_window: Option<ResultWindow>,
    /// States a reported bundle may be in, when the caller said.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub states: Option<RequestedBundleStates>,
    /// Prefix every reported symbolic name begins with, when the caller said.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub symbolic_name_prefix: Option<BundleSymbolicName>,
}

impl ListOpenServiceGatewayInitiativeBundlesCommand {
    /// Returns the page this request asks for, stated or resolved.
    #[must_use]
    pub fn resolved_window(&self) -> ResultWindow {
        self.result_window.clone().unwrap_or_default()
    }

    /// Returns whether a bundle so named and so stated is one this request asked
    /// about.
    #[must_use]
    pub fn admits(&self, symbolic_name: &BundleSymbolicName, state: BundleState) -> bool {
        let named = self
            .symbolic_name_prefix
            .as_ref()
            .is_none_or(|prefix| symbolic_name.as_text().starts_with(prefix.as_text()));
        let stated = self.states.as_ref().is_none_or(|states| states.contains(state));
        named && stated
    }
}

/// One bundle the author reports.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BundleMatch {
    /// The author's own numeric identifier for it.
    pub bundle_identifier: u64,
    /// What state it is in.
    pub state: BundleState,
    /// The name it is addressed by.
    pub symbolic_name: BundleSymbolicName,
    /// The version it reports.
    pub version: BundleVersion,
}

impl BundleMatch {
    /// Returns the key this row is ordered by.
    ///
    /// Name and version together, because one deployment can hold two versions
    /// of one bundle and the pair is what makes a row unique.
    #[must_use]
    pub fn ordering_key(&self) -> String {
        format!("{}\u{0}{}", self.symbolic_name.as_text(), self.version.as_text())
    }
}

/// One page of bundles.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ListOpenServiceGatewayInitiativeBundlesResult {
    /// Matches, strictly ascending by symbolic name and then version.
    pub matches: Vec<BundleMatch>,
    /// Where the next page resumes, when there is one.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub next_continuation_token: Option<ContinuationToken>,
}

impl ListOpenServiceGatewayInitiativeBundlesResult {
    /// Returns the page these matches describe.
    ///
    /// # Errors
    ///
    /// Returns [`ListingResultFailure::NotStrictlyAscending`] when a name and
    /// version pair repeats or sorts before its predecessor.
    pub fn new(
        matches: Vec<BundleMatch>,
        next_continuation_token: Option<ContinuationToken>,
    ) -> Result<Self, ListingResultFailure> {
        let keys: Vec<String> = matches.iter().map(BundleMatch::ordering_key).collect();
        crate::command::operational_listing::require_strictly_ascending_text(
            keys.iter().map(String::as_str),
        )?;
        Ok(Self { matches, next_continuation_token })
    }

    /// Requires this page to answer `command`.
    ///
    /// # Errors
    ///
    /// Returns [`ListingResultFailure::NotThisRequest`] when a match is outside
    /// the prefix or the states the command asked about.
    pub fn require_answers(
        &self,
        command: &ListOpenServiceGatewayInitiativeBundlesCommand,
    ) -> Result<(), ListingResultFailure> {
        let admitted =
            self.matches.iter().all(|found| command.admits(&found.symbolic_name, found.state));
        if admitted { Ok(()) } else { Err(ListingResultFailure::NotThisRequest) }
    }
}

/// One page exactly as it is written on the wire.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct ResultDocument {
    /// Matches this page carries.
    matches: Vec<BundleMatch>,
    /// Where the next page resumes.
    #[serde(default)]
    next_continuation_token: Option<ContinuationToken>,
}

impl<'de> Deserialize<'de> for ListOpenServiceGatewayInitiativeBundlesResult {
    fn deserialize<Source: serde::Deserializer<'de>>(
        deserializer: Source,
    ) -> Result<Self, Source::Error> {
        let document = ResultDocument::deserialize(deserializer)?;
        Self::new(document.matches, document.next_continuation_token).map_err(Source::Error::custom)
    }
}
