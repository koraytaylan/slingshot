//! The one bounded pagination contract every discovery command shares.
//!
//! A caller asks for a page, not for a repository. Without one contract each
//! command would grow its own notion of "how much", and the first one that
//! forgot a bound would let a caller ask for everything. So the window is a
//! closed value with named bounds, and every command takes the same one.
//!
//! It has exactly two shapes. [`ResultWindow::Initial`] starts an enumeration
//! and says where to start and how much to take. [`ResultWindow::Continuation`]
//! resumes one and says nothing else at all: the offset and the limit it
//! resumes under were fixed when the enumeration began and travel inside the
//! token, so a caller cannot widen a page it is already halfway through by
//! restating a limit. A continuation that carries an offset or a limit is
//! refused even when the offset is zero and the limit equals the default,
//! because a field that is always ignored is a field that will eventually be
//! believed.
//!
//! # What this module does not do
//!
//! A [`ContinuationToken`] is opaque here. This crate never mints one, never
//! reads one, and holds no key. It checks that the bytes could be a token -
//! nonempty, control-free, within bound - and nothing more.
//!
//! What a token must contain, how it is protected, and which failure wins when
//! two things are wrong at once are still this plan's to state, because they are
//! the contract an agent implements rather than an implementation detail an
//! agent chooses. They are stated here as the named constants an external vector
//! is checked against, and Plan 0005 owns the key material, its rotation, its
//! durability, and the code that computes a tag. This module is deliberately
//! provider-, topology-, and storage-neutral: it blesses no file, no database,
//! no content property, and no environment variable as the place a key lives.

use serde::de::Error as _;
use serde::{Deserialize, Serialize};

use crate::command::command_identity::CommandContract;

/// Wire spelling of the mode that begins an enumeration.
pub const INITIAL_MODE: &str = "initial";

/// Wire spelling of the mode that resumes one.
pub const CONTINUATION_MODE: &str = "continuation";

/// Offset an omitted window begins at.
pub const DEFAULT_RESULT_OFFSET: u64 = 0;

/// Format identifier the continuation-token protection contract declares.
pub const CONTINUATION_TOKEN_FORMAT: &str = "slingshot.continuation-token/1";

/// Canonicalization the digested argument object is taken under.
pub const COMMAND_ARGUMENTS_CANONICALIZATION: &str = "slingshot.command-arguments-canonical/1";

/// Canonicalization the argument rules are inherited from.
pub const COMMAND_CANONICAL_JSON: &str = "slingshot.command-canonical-json/1";

/// Message authentication algorithm the protected header declares.
pub const CONTINUATION_TOKEN_ALGORITHM: &str = "hmac_sha256";

/// Argument member the protected digest omits.
///
/// The window is the one argument that legitimately differs between two pages
/// of one enumeration, so digesting it would make every token wrong for its own
/// successor.
pub const WINDOW_ARGUMENT_MEMBER: &str = "result_window";

/// Members the protected header carries, in canonical key order.
pub const CONTINUATION_HEADER_MEMBERS: &[&str] = &["algorithm", "format", "key_identifier"];

/// Members the payload carries, in canonical key order.
pub const CONTINUATION_PAYLOAD_MEMBERS: &[&str] = &[
    "arguments_digest",
    "author_target_identity_digest",
    "command_semantic_contract_version",
    "command_wire_name",
    "expires_at_unix_milliseconds",
    "format",
    "initial_result_limit",
    "issued_at_unix_milliseconds",
    "key_identifier",
    "resume_sort_key",
];

/// Sole member every Plan 0003 resume sort key carries.
pub const RESUME_SORT_KEY_MEMBER: &str = "repository_path";

/// Characters separating the three compact token segments.
pub const CONTINUATION_TOKEN_SEGMENT_SEPARATOR: char = '.';

/// Segments a compact token carries.
pub const CONTINUATION_TOKEN_SEGMENTS: usize = 3;

/// Byte separating two parts of the tag input.
///
/// The role tag and the two segments are joined by a byte that cannot occur in
/// either, so no pair of different inputs can produce one tag input.
pub const CONTINUATION_TAG_INPUT_SEPARATOR: u8 = 0;

/// Length in bytes of the tag a token carries.
pub const CONTINUATION_TAG_BYTES: usize = 32;

/// Length in bytes of the key that tag is computed under.
pub const CONTINUATION_KEY_BYTES: usize = 32;

/// Characters a hexadecimal digest is written with.
pub const DIGEST_HEXADECIMAL_CHARACTERS: usize = 64;

/// Sole member every closed continuation failure carries.
pub const CONTINUATION_FAILURE_MEMBER: &str = "failure";

/// Failure literals continuation-token validation may report, in precedence
/// order.
///
/// The order is the contract, not a convenience: a token is authenticated
/// before anything it claims is believed, so a forged payload cannot choose
/// which failure it provokes.
pub const CONTINUATION_FAILURE_PRECEDENCE: &[&str] = &[
    "continuation_token_malformed",
    "continuation_token_integrity_invalid",
    "continuation_token_wrong_target",
    "continuation_token_wrong_query",
    "continuation_token_expired",
];

/// Returns the limit an omitted window resolves to.
#[must_use]
pub fn default_result_limit() -> u64 {
    CommandContract::embedded().limit("default_result_limit")
}

/// Returns the largest limit a caller may ask for.
#[must_use]
pub fn maximum_result_limit() -> u64 {
    CommandContract::embedded().limit("maximum_result_limit")
}

/// Returns the largest offset a caller may ask for.
#[must_use]
pub fn maximum_result_offset() -> u64 {
    CommandContract::embedded().limit("maximum_result_offset")
}

/// Returns the largest compact token this contract accepts.
#[must_use]
pub fn maximum_continuation_token_bytes() -> u64 {
    CommandContract::embedded().limit("maximum_continuation_token_bytes")
}

/// Returns the largest key identifier a protected header may name.
#[must_use]
pub fn maximum_continuation_key_identifier_bytes() -> u64 {
    CommandContract::embedded().limit("maximum_continuation_key_identifier_bytes")
}

/// Returns the largest canonical resume sort key a token may carry.
#[must_use]
pub fn maximum_continuation_resume_key_canonical_bytes() -> u64 {
    CommandContract::embedded().limit("maximum_continuation_resume_key_canonical_bytes")
}

/// Returns how long a token stays valid after it is issued.
#[must_use]
pub fn continuation_token_lifetime_milliseconds() -> u64 {
    CommandContract::embedded().limit("continuation_token_lifetime_milliseconds")
}

/// Returns how far ahead of validation a token may claim to have been issued.
#[must_use]
pub fn maximum_continuation_token_clock_skew_milliseconds() -> u64 {
    CommandContract::embedded().limit("maximum_continuation_token_clock_skew_milliseconds")
}

/// Reason a result window could not be built.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum WindowFailure {
    /// The mode is neither of the two this contract defines.
    #[error("a result window mode is either {INITIAL_MODE} or {CONTINUATION_MODE}")]
    UnknownMode,
    /// An initial window asked for no results at all.
    #[error("an initial result limit is at least one")]
    LimitZero,
    /// An initial window asked for more results than the contract allows.
    #[error("an initial result limit is at most {maximum}", maximum = maximum_result_limit())]
    LimitAboveMaximum,
    /// An initial window asked to skip further than the contract allows.
    #[error("a result offset is at most {maximum}", maximum = maximum_result_offset())]
    OffsetAboveMaximum,
    /// An initial window omitted a member its canonical form always writes.
    #[error("an initial result window carries both an offset and a limit")]
    InitialIncomplete,
    /// A continuation carried a window field the token already fixes.
    #[error("a continuation result window carries a continuation token alone")]
    ContinuationNotAlone,
    /// A continuation omitted the only member it carries.
    #[error("a continuation result window carries a continuation token")]
    ContinuationIncomplete,
    /// The token bytes are empty.
    #[error("a continuation token is not empty")]
    TokenEmpty,
    /// The token bytes contain a character no compact token contains.
    #[error("a continuation token contains no control character")]
    TokenControlCharacter,
    /// The token is longer than the contract allows.
    #[error(
        "a continuation token is at most {maximum} bytes",
        maximum = maximum_continuation_token_bytes()
    )]
    TokenTooLong,
}

/// How many fully evaluated matches an enumeration skips before emitting one.
///
/// It counts matches, not repository rows. Counting rows would make the same
/// offset mean different things against two repositories that answer the same
/// question, and would let a caller page past content it can see by asking
/// about content it cannot.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize)]
#[serde(into = "u64")]
pub struct ResultOffset {
    /// Matches to skip.
    value: u64,
}

impl ResultOffset {
    /// Returns the offset an omitted window begins at.
    #[must_use]
    pub fn beginning() -> Self {
        Self { value: DEFAULT_RESULT_OFFSET }
    }

    /// Returns the offset `value` names.
    ///
    /// # Errors
    ///
    /// Returns [`WindowFailure::OffsetAboveMaximum`] when `value` is above the
    /// contract's inclusive maximum.
    pub fn new(value: u64) -> Result<Self, WindowFailure> {
        if value > maximum_result_offset() {
            return Err(WindowFailure::OffsetAboveMaximum);
        }
        Ok(Self { value })
    }

    /// Returns how many matches this offset skips.
    #[must_use]
    pub fn count(self) -> u64 {
        self.value
    }
}

impl From<ResultOffset> for u64 {
    fn from(offset: ResultOffset) -> Self {
        offset.value
    }
}

/// How many matches one page may carry.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize)]
#[serde(into = "u64")]
pub struct ResultLimit {
    /// Matches the page may carry.
    value: u64,
}

impl ResultLimit {
    /// Returns the limit an omitted window resolves to.
    #[must_use]
    pub fn default_limit() -> Self {
        Self { value: default_result_limit() }
    }

    /// Returns the limit `value` names.
    ///
    /// # Errors
    ///
    /// Returns [`WindowFailure::LimitZero`] when `value` is zero and
    /// [`WindowFailure::LimitAboveMaximum`] when it is above the contract's
    /// inclusive maximum.
    pub fn new(value: u64) -> Result<Self, WindowFailure> {
        if value == 0 {
            return Err(WindowFailure::LimitZero);
        }
        if value > maximum_result_limit() {
            return Err(WindowFailure::LimitAboveMaximum);
        }
        Ok(Self { value })
    }

    /// Returns how many matches this limit allows.
    #[must_use]
    pub fn count(self) -> u64 {
        self.value
    }
}

impl From<ResultLimit> for u64 {
    fn from(limit: ResultLimit) -> Self {
        limit.value
    }
}

/// Bytes that resume one enumeration.
///
/// Opaque on purpose. This crate holds no key, so it can tell a shape from a
/// non-shape and nothing else; treating the contents as meaningful here would
/// be trusting bytes a caller supplied.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct ContinuationToken {
    /// The compact serialization, exactly as it arrived.
    value: String,
}

impl ContinuationToken {
    /// Returns the token `value` spells.
    ///
    /// # Errors
    ///
    /// Returns [`WindowFailure::TokenEmpty`] for empty bytes,
    /// [`WindowFailure::TokenControlCharacter`] when a control character is
    /// present, and [`WindowFailure::TokenTooLong`] above the contract's
    /// inclusive byte maximum.
    pub fn new(value: impl Into<String>) -> Result<Self, WindowFailure> {
        let value = value.into();
        if value.is_empty() {
            return Err(WindowFailure::TokenEmpty);
        }
        if value.chars().any(char::is_control) {
            return Err(WindowFailure::TokenControlCharacter);
        }
        if u64::try_from(value.len()).unwrap_or(u64::MAX) > maximum_continuation_token_bytes() {
            return Err(WindowFailure::TokenTooLong);
        }
        Ok(Self { value })
    }

    /// Returns the token exactly as it arrived.
    #[must_use]
    pub fn as_text(&self) -> &str {
        &self.value
    }
}

impl TryFrom<String> for ContinuationToken {
    type Error = WindowFailure;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        Self::new(value)
    }
}

impl From<ContinuationToken> for String {
    fn from(token: ContinuationToken) -> Self {
        token.value
    }
}

impl ::core::fmt::Display for ContinuationToken {
    fn fmt(&self, formatter: &mut ::core::fmt::Formatter<'_>) -> ::core::fmt::Result {
        formatter.write_str(&self.value)
    }
}

/// Which page of an enumeration a caller is asking for.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ResultWindow {
    /// Begin an enumeration.
    Initial {
        /// Fully evaluated matches to skip before emitting one.
        offset: ResultOffset,
        /// Matches this page may carry.
        limit: ResultLimit,
    },
    /// Resume one.
    ///
    /// The offset and limit are not repeated because the token already carries
    /// the ones the enumeration began under.
    Continuation {
        /// Bytes naming where to resume.
        continuation_token: ContinuationToken,
    },
}

impl ResultWindow {
    /// Returns the window an omitted argument resolves to.
    #[must_use]
    pub fn omitted() -> Self {
        Self::Initial { offset: ResultOffset::beginning(), limit: ResultLimit::default_limit() }
    }

    /// Returns the initial window `offset` and `limit` name.
    ///
    /// # Errors
    ///
    /// Returns whatever [`ResultOffset::new`] or [`ResultLimit::new`] refuses.
    pub fn initial(offset: u64, limit: u64) -> Result<Self, WindowFailure> {
        Ok(Self::Initial { offset: ResultOffset::new(offset)?, limit: ResultLimit::new(limit)? })
    }

    /// Returns the continuation window `token` names.
    ///
    /// # Errors
    ///
    /// Returns whatever [`ContinuationToken::new`] refuses.
    pub fn continuation(token: impl Into<String>) -> Result<Self, WindowFailure> {
        Ok(Self::Continuation { continuation_token: ContinuationToken::new(token)? })
    }

    /// Returns the wire spelling of this window's mode.
    #[must_use]
    pub fn mode(&self) -> &'static str {
        match self {
            Self::Initial { .. } => INITIAL_MODE,
            Self::Continuation { .. } => CONTINUATION_MODE,
        }
    }

    /// Returns the offset this window begins at, when it begins an enumeration.
    ///
    /// A continuation has none: it resumes after a sort key rather than by
    /// counting, and reapplying an offset would skip the matches the previous
    /// page already stopped before.
    #[must_use]
    pub fn offset(&self) -> Option<ResultOffset> {
        match self {
            Self::Initial { offset, .. } => Some(*offset),
            Self::Continuation { .. } => None,
        }
    }

    /// Returns the limit this window states, when it states one.
    #[must_use]
    pub fn limit(&self) -> Option<ResultLimit> {
        match self {
            Self::Initial { limit, .. } => Some(*limit),
            Self::Continuation { .. } => None,
        }
    }

    /// Returns the token this window resumes from, when it resumes one.
    #[must_use]
    pub fn continuation_token(&self) -> Option<&ContinuationToken> {
        match self {
            Self::Initial { .. } => None,
            Self::Continuation { continuation_token } => Some(continuation_token),
        }
    }
}

impl Default for ResultWindow {
    fn default() -> Self {
        Self::omitted()
    }
}

/// One window exactly as it is written on the wire.
///
/// Every member is optional here and required by the mode that owns it, so a
/// window carrying the wrong member for its mode is refused with a reason
/// rather than silently reinterpreted as the other mode.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct WindowDocument {
    /// Which of the two shapes this is.
    mode: String,
    /// Matches to skip, on an initial window alone.
    #[serde(default)]
    offset: Option<u64>,
    /// Matches to carry, on an initial window alone.
    #[serde(default)]
    limit: Option<u64>,
    /// Bytes to resume from, on a continuation alone.
    #[serde(default)]
    continuation_token: Option<String>,
}

impl TryFrom<WindowDocument> for ResultWindow {
    type Error = WindowFailure;

    fn try_from(document: WindowDocument) -> Result<Self, Self::Error> {
        match document.mode.as_str() {
            INITIAL_MODE => accept_initial(document),
            CONTINUATION_MODE => accept_continuation(document),
            _ => Err(WindowFailure::UnknownMode),
        }
    }
}

/// Accepts a document that declared the initial mode.
fn accept_initial(document: WindowDocument) -> Result<ResultWindow, WindowFailure> {
    if document.continuation_token.is_some() {
        return Err(WindowFailure::ContinuationNotAlone);
    }
    let (Some(offset), Some(limit)) = (document.offset, document.limit) else {
        return Err(WindowFailure::InitialIncomplete);
    };
    ResultWindow::initial(offset, limit)
}

/// Accepts a document that declared the continuation mode.
///
/// An offset or a limit is refused before the token is looked at, because the
/// caller has asked for something this contract cannot honor and answering with
/// a token complaint would hide that.
fn accept_continuation(document: WindowDocument) -> Result<ResultWindow, WindowFailure> {
    if document.offset.is_some() || document.limit.is_some() {
        return Err(WindowFailure::ContinuationNotAlone);
    }
    let Some(token) = document.continuation_token else {
        return Err(WindowFailure::ContinuationIncomplete);
    };
    ResultWindow::continuation(token)
}

impl Serialize for ResultWindow {
    fn serialize<Target: serde::Serializer>(
        &self,
        serializer: Target,
    ) -> Result<Target::Ok, Target::Error> {
        use serde::ser::SerializeStruct;

        /// Members an initial window writes.
        const INITIAL_MEMBERS: usize = 3;
        /// Members a continuation writes.
        const CONTINUATION_MEMBERS: usize = 2;

        match self {
            Self::Initial { offset, limit } => {
                let mut window = serializer.serialize_struct("ResultWindow", INITIAL_MEMBERS)?;
                window.serialize_field("mode", INITIAL_MODE)?;
                window.serialize_field("offset", offset)?;
                window.serialize_field("limit", limit)?;
                window.end()
            }
            Self::Continuation { continuation_token } => {
                let mut window =
                    serializer.serialize_struct("ResultWindow", CONTINUATION_MEMBERS)?;
                window.serialize_field("mode", CONTINUATION_MODE)?;
                window.serialize_field("continuation_token", continuation_token)?;
                window.end()
            }
        }
    }
}

impl<'de> Deserialize<'de> for ResultWindow {
    fn deserialize<Source: serde::Deserializer<'de>>(
        deserializer: Source,
    ) -> Result<Self, Source::Error> {
        let document = WindowDocument::deserialize(deserializer)?;
        Self::try_from(document).map_err(Source::Error::custom)
    }
}
