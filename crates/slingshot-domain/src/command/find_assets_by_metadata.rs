//! Finding assets by what is recorded about them.
//!
//! An asset's metadata lives in several places and this command reads each one
//! from exactly where the architecture says it is. Media format comes from
//! `dc:format` if that is a usable single string, and otherwise from the
//! original rendition's mime type - in that order, with no third fallback. Byte
//! size is the length of the original rendition's binary and nothing else: not
//! the sum of the renditions, not the storage the asset occupies. Those two
//! answers are the ones a person can check by hand, which is why they are
//! pinned rather than approximated.
//!
//! Absence is not a match. A requested format does not match an asset whose
//! format could not be read, a requested size range does not match one whose
//! original rendition has no length, and requested tags do not match an asset
//! with no tags. The alternative - treating unknown as permissible - would
//! quietly widen every search that used a criterion.

use serde::de::Error as _;
use serde::{Deserialize, Serialize};

use crate::command::command_identity::CommandContract;
use crate::command::query_paths::{
    DiscoveryResultFailure, anchor_contains, require_strictly_ascending,
};
use crate::command::repository_path::RepositoryPath;
use crate::command::result_window::{ContinuationToken, ResultWindow};
use crate::command::search_predicate::{PropertyPredicate, PropertyPredicates};

/// Values one comparison of neighbours looks at.
const ADJACENT_PAIR: usize = 2;

/// Exact primary type every Adobe Experience Manager asset has.
pub const ASSET_PRIMARY_NODE_TYPE: &str = "dam:Asset";

/// Property the media format is read from first.
pub const ASSET_FORMAT_PROPERTY: &str = "jcr:content/metadata/dc:format";

/// Property it falls back to, and nowhere else.
pub const ASSET_RENDITION_MIME_TYPE_PROPERTY: &str =
    "jcr:content/renditions/original/jcr:content/jcr:mimeType";

/// Binary the byte size is the length of.
pub const ASSET_ORIGINAL_BINARY_PROPERTY: &str =
    "jcr:content/renditions/original/jcr:content/jcr:data";

/// Property the tags are read from.
pub const ASSET_TAGS_PROPERTY: &str = "jcr:content/metadata/cq:tags";

/// Returns the largest original-rendition length this contract represents.
#[must_use]
pub fn maximum_asset_byte_length() -> u64 {
    CommandContract::embedded().limit("maximum_asset_byte_length")
}

/// Returns the largest media format this contract accepts.
#[must_use]
pub fn maximum_media_format_bytes() -> u64 {
    CommandContract::embedded().limit("maximum_media_format_bytes")
}

/// Returns the largest tag this contract accepts.
#[must_use]
pub fn maximum_asset_tag_bytes() -> u64 {
    CommandContract::embedded().limit("maximum_asset_tag_bytes")
}

/// Returns the most media formats one request may name.
#[must_use]
pub fn maximum_requested_media_formats() -> u64 {
    CommandContract::embedded().limit("maximum_requested_media_formats")
}

/// Returns the most tags one request may name.
#[must_use]
pub fn maximum_requested_asset_tags() -> u64 {
    CommandContract::embedded().limit("maximum_requested_asset_tags")
}

/// Reason an asset search value could not be built.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum AssetSearchFailure {
    /// A length is outside what a JCR binary can report.
    #[error("an asset byte length is zero through {maximum}", maximum = maximum_asset_byte_length())]
    ByteLengthOutOfRange,
    /// A range asks for nothing.
    #[error("an asset byte range has a minimum at or below its maximum")]
    RangeInverted,
    /// A media format is empty, controlled, or over bound.
    #[error("a media format is nonempty, control-free, and at most {maximum} bytes", maximum = maximum_media_format_bytes())]
    MediaFormatOutOfBounds,
    /// A tag is empty, controlled, or over bound.
    #[error("a tag is nonempty, control-free, and at most {maximum} bytes", maximum = maximum_asset_tag_bytes())]
    TagOutOfBounds,
    /// A request named one value twice.
    #[error("a requested set names each value once")]
    SetNotUnique,
    /// A request named values out of canonical order.
    #[error("a requested set arrives in ascending byte order, so one set has one spelling")]
    SetNotSorted,
    /// A request named more values than the contract allows.
    #[error("a requested set stays inside its named bound")]
    SetTooLarge,
}

/// How many of the requested tags an asset must carry.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TagMatchMode {
    /// One of them.
    Any,
    /// Every one of them.
    All,
}

/// How many bytes an asset's original rendition holds.
///
/// A JSON integer token, not a string and not a floating value. The domain is
/// zero through the largest length a JCR binary reports, which is the signed
/// 64-bit maximum, and the same type is used for both ends of a requested range
/// and for the length a match reports - so a request cannot express a size the
/// repository could never answer with.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize)]
#[serde(into = "u64")]
pub struct AssetByteLength {
    /// The length.
    value: u64,
}

impl AssetByteLength {
    /// Returns the length `value` names.
    ///
    /// # Errors
    ///
    /// Returns [`AssetSearchFailure::ByteLengthOutOfRange`] above the largest
    /// length a JCR binary reports.
    pub fn new(value: u64) -> Result<Self, AssetSearchFailure> {
        if value > maximum_asset_byte_length() {
            return Err(AssetSearchFailure::ByteLengthOutOfRange);
        }
        Ok(Self { value })
    }

    /// Returns the length itself.
    #[must_use]
    pub fn count(self) -> u64 {
        self.value
    }
}

impl From<AssetByteLength> for u64 {
    fn from(length: AssetByteLength) -> Self {
        length.value
    }
}

impl<'de> Deserialize<'de> for AssetByteLength {
    fn deserialize<Source: serde::Deserializer<'de>>(
        deserializer: Source,
    ) -> Result<Self, Source::Error> {
        let value = u64::deserialize(deserializer)?;
        Self::new(value).map_err(Source::Error::custom)
    }
}

/// Builds one bounded, nonempty, control-free wrapper over a spelling.
macro_rules! asset_text {
    ($(#[$attribute:meta])* $name:ident, $bound:ident, $failure:ident) => {
        $(#[$attribute])*
        #[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
        #[serde(try_from = "String", into = "String")]
        pub struct $name {
            /// The spelling, exactly as it arrived.
            value: String,
        }

        impl $name {
            /// Returns the value `spelling` names.
            ///
            /// # Errors
            ///
            /// Returns [`AssetSearchFailure`] when the spelling is empty, over
            /// its named bound, or carries a control character.
            pub fn new(spelling: impl Into<String>) -> Result<Self, AssetSearchFailure> {
                let value = spelling.into();
                let bounded = !value.is_empty()
                    && u64::try_from(value.len()).unwrap_or(u64::MAX) <= $bound()
                    && !value.chars().any(char::is_control);
                if bounded { Ok(Self { value }) } else { Err(AssetSearchFailure::$failure) }
            }

            /// Returns the spelling, exactly as it arrived.
            #[must_use]
            pub fn as_text(&self) -> &str {
                &self.value
            }
        }

        impl TryFrom<String> for $name {
            type Error = AssetSearchFailure;

            fn try_from(value: String) -> Result<Self, Self::Error> {
                Self::new(value)
            }
        }

        impl From<$name> for String {
            fn from(value: $name) -> Self {
                value.value
            }
        }
    };
}

asset_text!(
    /// What kind of bytes an asset holds.
    ///
    /// Compared exactly as recorded. This is repository metadata rather than a
    /// parsed media type, so `image/jpeg` and `image/JPEG` are two values and
    /// the caller asks for the one that is stored.
    MediaFormat,
    maximum_media_format_bytes,
    MediaFormatOutOfBounds
);

asset_text!(
    /// One tag an asset carries.
    AssetTag,
    maximum_asset_tag_bytes,
    TagOutOfBounds
);

/// Builds one canonical ascending set of bounded values.
macro_rules! requested_set {
    ($(#[$attribute:meta])* $name:ident, $item:ident, $bound:ident) => {
        $(#[$attribute])*
        #[derive(Debug, Clone, PartialEq, Eq, Serialize)]
        #[serde(transparent)]
        pub struct $name {
            /// The values, ascending.
            values: Vec<$item>,
        }

        impl $name {
            /// Returns the set `values` spell, sorting them once.
            ///
            /// # Errors
            ///
            /// Returns [`AssetSearchFailure::SetTooLarge`] or
            /// [`AssetSearchFailure::SetNotUnique`].
            pub fn new(mut values: Vec<$item>) -> Result<Self, AssetSearchFailure> {
                values.sort_by(|left, right| {
                    left.as_text().as_bytes().cmp(right.as_text().as_bytes())
                });
                Self::accept(values)
            }

            /// Returns the set `values` spell, requiring them already canonical.
            ///
            /// # Errors
            ///
            /// Returns [`AssetSearchFailure::SetNotSorted`] in addition to
            /// whatever [`Self::new`] refuses.
            pub fn canonical(values: Vec<$item>) -> Result<Self, AssetSearchFailure> {
                let ascending = values
                    .windows(ADJACENT_PAIR)
                    .all(|pair| pair[0].as_text().as_bytes() <= pair[1].as_text().as_bytes());
                if !ascending {
                    return Err(AssetSearchFailure::SetNotSorted);
                }
                Self::accept(values)
            }

            /// Accepts one already-ordered collection.
            fn accept(values: Vec<$item>) -> Result<Self, AssetSearchFailure> {
                if u64::try_from(values.len()).unwrap_or(u64::MAX) > $bound() {
                    return Err(AssetSearchFailure::SetTooLarge);
                }
                if values.windows(ADJACENT_PAIR).any(|pair| pair[0] == pair[1]) {
                    return Err(AssetSearchFailure::SetNotUnique);
                }
                Ok(Self { values })
            }

            /// Returns the values, ascending.
            #[must_use]
            pub fn values(&self) -> &[$item] {
                &self.values
            }

            /// Returns whether this set names nothing.
            #[must_use]
            pub fn is_empty(&self) -> bool {
                self.values.is_empty()
            }
        }

        impl<'de> Deserialize<'de> for $name {
            fn deserialize<Source: serde::Deserializer<'de>>(
                deserializer: Source,
            ) -> Result<Self, Source::Error> {
                let values = Vec::<$item>::deserialize(deserializer)?;
                Self::canonical(values).map_err(Source::Error::custom)
            }
        }
    };
}

requested_set!(
    /// The media formats one request names.
    ///
    /// Canonical on the wire, because the set participates in the digest a
    /// continuation token is bound to and two spellings of one set would be two
    /// queries that could not resume each other.
    RequestedMediaFormats,
    MediaFormat,
    maximum_requested_media_formats
);

requested_set!(
    /// The tags one request names.
    RequestedAssetTags,
    AssetTag,
    maximum_requested_asset_tags
);

/// One request to find assets by their metadata.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FindAssetsByMetadataCommand {
    /// Largest original rendition an asset may have, inclusive.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub maximum_byte_length: Option<AssetByteLength>,
    /// Formats an asset may be in, when the caller named any.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub media_formats: Option<RequestedMediaFormats>,
    /// Smallest original rendition an asset may have, inclusive.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub minimum_byte_length: Option<AssetByteLength>,
    /// Questions every match must answer, resolved from the asset node.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub property_predicates: Option<PropertyPredicates>,
    /// Page the caller is asking for, when the caller said.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub result_window: Option<ResultWindow>,
    /// Node to search at and below.
    pub root_path: RepositoryPath,
    /// How many of the requested tags an asset must carry.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tag_match_mode: Option<TagMatchMode>,
    /// Tags an asset must carry, when the caller named any.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tags: Option<RequestedAssetTags>,
}

impl FindAssetsByMetadataCommand {
    /// Requires the stated byte range to be one an asset could fall inside.
    ///
    /// Equality is legal at both ends, including zero and the maximum, because
    /// asking for assets of exactly one size is a real question.
    ///
    /// # Errors
    ///
    /// Returns [`AssetSearchFailure::RangeInverted`] when the minimum is above
    /// the maximum.
    pub fn require_usable_range(&self) -> Result<(), AssetSearchFailure> {
        match (self.minimum_byte_length, self.maximum_byte_length) {
            (Some(minimum), Some(maximum)) if minimum.count() > maximum.count() => {
                Err(AssetSearchFailure::RangeInverted)
            }
            _ => Ok(()),
        }
    }

    /// Returns the page this request asks for, stated or resolved.
    #[must_use]
    pub fn resolved_window(&self) -> ResultWindow {
        self.result_window.clone().unwrap_or_default()
    }

    /// Returns the questions every match must answer.
    #[must_use]
    pub fn predicates(&self) -> &[PropertyPredicate] {
        self.property_predicates.as_ref().map_or(&[], PropertyPredicates::predicates)
    }

    /// Returns whether `found` satisfies every metadata criterion this names.
    ///
    /// Every criterion family combines with logical and, and an absent value
    /// never satisfies a stated criterion: an asset whose format could not be
    /// read is not in any requested format, and one with no tags carries none
    /// of the requested ones.
    #[must_use]
    pub fn matches_metadata(&self, found: &AssetMatch) -> bool {
        self.matches_format(found) && self.matches_size(found) && self.matches_tags(found)
    }

    /// Returns whether the found format satisfies the requested formats.
    fn matches_format(&self, found: &AssetMatch) -> bool {
        let Some(requested) = self.media_formats.as_ref() else {
            return true;
        };
        if requested.is_empty() {
            return true;
        }
        found.media_format.as_ref().is_some_and(|format| requested.values().contains(format))
    }

    /// Returns whether the found length falls inside the requested range.
    fn matches_size(&self, found: &AssetMatch) -> bool {
        if self.minimum_byte_length.is_none() && self.maximum_byte_length.is_none() {
            return true;
        }
        let Some(length) = found.byte_length else {
            return false;
        };
        let above =
            self.minimum_byte_length.is_none_or(|minimum| length.count() >= minimum.count());
        let below =
            self.maximum_byte_length.is_none_or(|maximum| length.count() <= maximum.count());
        above && below
    }

    /// Returns whether the found tags satisfy the requested tags.
    fn matches_tags(&self, found: &AssetMatch) -> bool {
        let Some(requested) = self.tags.as_ref() else {
            return true;
        };
        if requested.is_empty() {
            return true;
        }
        let carried = |wanted: &AssetTag| found.tags.contains(wanted);
        match self.tag_match_mode.unwrap_or(TagMatchMode::All) {
            TagMatchMode::Any => requested.values().iter().any(carried),
            TagMatchMode::All => requested.values().iter().all(carried),
        }
    }
}

/// One asset that matched.
///
/// Format and size are absent when they could not be read, rather than being
/// filled in with a plausible value. Tags are deduplicated and ascending, so
/// one asset has one tag list.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AssetMatch {
    /// Length of the original rendition, when it has one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub byte_length: Option<AssetByteLength>,
    /// What kind of bytes it holds, when that could be read.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub media_format: Option<MediaFormat>,
    /// Asset that matched.
    pub repository_path: RepositoryPath,
    /// Tags it carries, ascending.
    #[serde(default)]
    pub tags: Vec<AssetTag>,
}

/// One page of assets that answered the question.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct FindAssetsByMetadataResult {
    /// Matches, strictly ascending by asset path bytes.
    pub matches: Vec<AssetMatch>,
    /// Where the next page resumes, when there is one.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub next_continuation_token: Option<ContinuationToken>,
}

impl FindAssetsByMetadataResult {
    /// Returns the page `matches` and `next_continuation_token` describe.
    ///
    /// # Errors
    ///
    /// Returns [`DiscoveryResultFailure::NotStrictlyAscending`] when a path
    /// repeats, when a path sorts before its predecessor, or when one match's
    /// tags are not themselves ascending and unique.
    pub fn new(
        matches: Vec<AssetMatch>,
        next_continuation_token: Option<ContinuationToken>,
    ) -> Result<Self, DiscoveryResultFailure> {
        require_strictly_ascending(matches.iter().map(|found| &found.repository_path))?;
        for found in &matches {
            let ascending = found
                .tags
                .windows(ADJACENT_PAIR)
                .all(|pair| pair[0].as_text().as_bytes() < pair[1].as_text().as_bytes());
            if !ascending {
                return Err(DiscoveryResultFailure::NotStrictlyAscending);
            }
        }
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
        command: &FindAssetsByMetadataCommand,
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
    matches: Vec<AssetMatch>,
    /// Where the next page resumes.
    #[serde(default)]
    next_continuation_token: Option<ContinuationToken>,
}

impl<'de> Deserialize<'de> for FindAssetsByMetadataResult {
    fn deserialize<Source: serde::Deserializer<'de>>(
        deserializer: Source,
    ) -> Result<Self, Source::Error> {
        let document = ResultDocument::deserialize(deserializer)?;
        Self::new(document.matches, document.next_continuation_token).map_err(Source::Error::custom)
    }
}
