//! Finding pages that contain an exact phrase.
//!
//! The phrase is matched as one contiguous sequence of Unicode scalar values,
//! with no normalization, no case folding, no stemming, and no tokenization.
//! That makes the command predictable rather than clever: a caller who searches
//! for `Straße` gets pages containing `Straße`, and one who wanted `Strasse`
//! searches for that instead. A search engine guessing between them would give
//! two different answers to the same question depending on how the content was
//! authored.
//!
//! For the same reason the phrase is never trimmed. Leading and trailing
//! whitespace is refused as a noncanonical spelling rather than silently
//! removed, because removing it would mean two spellings of one phrase; internal
//! whitespace is preserved exactly, because it is part of what was asked for.

use serde::de::Error as _;
use serde::{Deserialize, Serialize};

use crate::command::command_identity::CommandContract;
use crate::command::query_paths::{
    DiscoveryResultFailure, anchor_contains, require_strictly_ascending,
};
use crate::command::repository_path::RepositoryPath;
use crate::command::result_window::{ContinuationToken, ResultWindow};

/// Exact primary type every Adobe Experience Manager page has.
pub const PAGE_PRIMARY_NODE_TYPE: &str = "cq:Page";

/// Direct child holding a page's content.
pub const PAGE_CONTENT_CHILD: &str = "jcr:content";

/// Property a page title is read from, on that content resource.
pub const PAGE_TITLE_PROPERTY: &str = "jcr:title";

/// Returns the largest phrase this contract searches for.
#[must_use]
pub fn maximum_search_phrase_bytes() -> u64 {
    CommandContract::embedded().limit("maximum_search_phrase_bytes")
}

/// Returns the largest page title this contract reports.
#[must_use]
pub fn maximum_page_title_bytes() -> u64 {
    CommandContract::embedded().limit("maximum_page_title_bytes")
}

/// Reason a page search value could not be built.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum PageSearchFailure {
    /// A phrase is empty or longer than the contract allows.
    #[error("a search phrase is nonempty and at most {maximum} bytes", maximum = maximum_search_phrase_bytes())]
    PhraseOutOfBounds,
    /// A phrase begins or ends with whitespace.
    #[error(
        "a search phrase is spelled without leading or trailing whitespace, because trimming it would make two spellings of one phrase"
    )]
    PhraseNotCanonical,
    /// A phrase carries a character no phrase carries.
    #[error("a search phrase carries no control character")]
    PhraseControlCharacter,
    /// A title is longer than the contract allows.
    #[error("a page title is at most {maximum} bytes", maximum = maximum_page_title_bytes())]
    TitleTooLong,
}

/// Exactly what to look for.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct SearchPhrase {
    /// The phrase, exactly as it arrived.
    value: String,
}

impl SearchPhrase {
    /// Returns the phrase `spelling` names.
    ///
    /// # Errors
    ///
    /// Returns [`PageSearchFailure::PhraseOutOfBounds`] when empty or over
    /// bound, [`PageSearchFailure::PhraseNotCanonical`] when it begins or ends
    /// with whitespace, and [`PageSearchFailure::PhraseControlCharacter`] when
    /// it carries a control character.
    pub fn new(spelling: impl Into<String>) -> Result<Self, PageSearchFailure> {
        let value = spelling.into();
        if value.is_empty()
            || u64::try_from(value.len()).unwrap_or(u64::MAX) > maximum_search_phrase_bytes()
        {
            return Err(PageSearchFailure::PhraseOutOfBounds);
        }
        if value.chars().any(char::is_control) {
            return Err(PageSearchFailure::PhraseControlCharacter);
        }
        let edges = [value.chars().next(), value.chars().next_back()];
        if edges.iter().flatten().any(|character| character.is_whitespace()) {
            return Err(PageSearchFailure::PhraseNotCanonical);
        }
        Ok(Self { value })
    }

    /// Returns the phrase, exactly as it arrived.
    #[must_use]
    pub fn as_text(&self) -> &str {
        &self.value
    }

    /// Returns whether `property` contains this phrase.
    ///
    /// One contiguous run of scalar values, compared byte for byte. Nothing is
    /// normalized on either side, so a composed spelling does not find its
    /// decomposed twin and the caller learns what is actually stored.
    #[must_use]
    pub fn occurs_in(&self, property: &str) -> bool {
        property.contains(&self.value)
    }
}

impl TryFrom<String> for SearchPhrase {
    type Error = PageSearchFailure;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        Self::new(value)
    }
}

impl From<SearchPhrase> for String {
    fn from(phrase: SearchPhrase) -> Self {
        phrase.value
    }
}

/// A page's title, when it has one this contract can report.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct PageTitle {
    /// The title, exactly as stored.
    value: String,
}

impl PageTitle {
    /// Returns the title `spelling` carries.
    ///
    /// # Errors
    ///
    /// Returns [`PageSearchFailure::TitleTooLong`] above the named bound.
    pub fn new(spelling: impl Into<String>) -> Result<Self, PageSearchFailure> {
        let value = spelling.into();
        if u64::try_from(value.len()).unwrap_or(u64::MAX) > maximum_page_title_bytes() {
            return Err(PageSearchFailure::TitleTooLong);
        }
        Ok(Self { value })
    }

    /// Returns the title, exactly as stored.
    #[must_use]
    pub fn as_text(&self) -> &str {
        &self.value
    }
}

impl TryFrom<String> for PageTitle {
    type Error = PageSearchFailure;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        Self::new(value)
    }
}

impl From<PageTitle> for String {
    fn from(title: PageTitle) -> Self {
        title.value
    }
}

/// One page that matched.
///
/// The path names the `cq:Page` node itself rather than its content resource,
/// because that is the address a person navigates to. The title is optional
/// because a page need not have one, and it comes only from a single-valued
/// `jcr:title` string directly on the content resource - not from a heading in
/// the content, which would be a different question.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PageMatch {
    /// Page that matched.
    pub repository_path: RepositoryPath,
    /// Its title, when it has one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<PageTitle>,
}

/// One request to find pages containing a phrase.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FindPagesContainingPhraseCommand {
    /// Exactly what to look for.
    pub phrase: SearchPhrase,
    /// Page the caller is asking for, when the caller said.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub result_window: Option<ResultWindow>,
    /// Node to search at and below.
    pub root_path: RepositoryPath,
}

impl FindPagesContainingPhraseCommand {
    /// Returns the page this request asks for, stated or resolved.
    #[must_use]
    pub fn resolved_window(&self) -> ResultWindow {
        self.result_window.clone().unwrap_or_default()
    }
}

/// One page of pages that contain the phrase.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct FindPagesContainingPhraseResult {
    /// Matches, strictly ascending by page path bytes.
    pub matches: Vec<PageMatch>,
    /// Where the next page resumes, when there is one.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub next_continuation_token: Option<ContinuationToken>,
}

impl FindPagesContainingPhraseResult {
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
        command: &FindPagesContainingPhraseCommand,
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

impl<'de> Deserialize<'de> for FindPagesContainingPhraseResult {
    fn deserialize<Source: serde::Deserializer<'de>>(
        deserializer: Source,
    ) -> Result<Self, Source::Error> {
        let document = ResultDocument::deserialize(deserializer)?;
        Self::new(document.matches, document.next_continuation_token).map_err(Source::Error::custom)
    }
}
