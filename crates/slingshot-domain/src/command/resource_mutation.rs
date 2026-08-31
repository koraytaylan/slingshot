//! What a write answers with, and the one payload a write carries inward.
//!
//! Sixteen of the writes this registry publishes have the same whole answer:
//! the address they changed. Written sixteen times that becomes sixteen chances
//! to name the member differently, to decide differently whether it is the page
//! or its content resource, and to disagree about whether a result may echo an
//! address the request never determined. It is written once here instead.
//!
//! A write with more to report - how much a deletion removed, where a move went
//! and what it adjusted - declares its own result and still carries the address,
//! so a caller reads the same member in the same place either way.
//!
//! # Why a reference policy has no default
//!
//! Refusing to delete something another page points at is right for an operator
//! cleaning up and wrong for one decommissioning a site; ignoring references is
//! the reverse. Both defaults are wrong for somebody, and a caller who has to
//! state the policy has had to think about it once.
//!
//! # Why bytes come inline and only inline
//!
//! Creating an asset is the one command that carries content inward. It carries
//! it as Base64 in the request, bounded twice - before decoding by the encoded
//! length, after decoding by the decoded length - and refuses anything larger.
//! The alternative is an inbound staging protocol, and inventing one inside a
//! command contract would put an unproven file transfer where a vocabulary
//! belongs.

use base64::Engine as _;
use base64::engine::general_purpose::STANDARD;
use serde::de::Error as _;
use serde::{Deserialize, Serialize};

use crate::command::command_identity::CommandContract;
use crate::command::create_page::MutationProperties;
use crate::command::operational_listing::{ListingResultFailure, require_ascending_distinct};
use crate::command::repository_path::{PropertyName, RepositoryPath};

/// Characters standard Base64 spells its groups with.
const BASE64_ALPHABET: &str = "ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

/// The character standard Base64 pads a partial group with.
const BASE64_PADDING: char = '=';

/// Encoded characters one whole Base64 group spells.
const BASE64_GROUP: usize = 4;

/// Padding characters a canonical encoding may end with.
const MAXIMUM_PADDING: usize = 2;

/// Why a mutation value is not one this contract can carry.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum MutationResultFailure {
    /// A reported count is larger than the contract allows.
    #[error("a reported count is within the bound its contract declares")]
    CountTooLarge,
    /// A result echoes an address its request did not determine.
    #[error("a mutation result echoes the address its request determined")]
    NotThisRequest,
    /// A move reports a destination inside its own source.
    #[error("a move reports a destination outside the subtree it moves")]
    DestinationInsideSource,
}

/// Why an inline payload is not one this contract can carry.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum InlinePayloadFailure {
    /// The encoded form is longer than the contract allows.
    #[error("an inline payload's encoded form is within the bound its contract declares")]
    EncodedTooLarge,
    /// The encoded form is not standard Base64 with canonical padding.
    #[error("an inline payload is standard Base64 with canonical padding")]
    EncodingMalformed,
    /// The decoded form is longer than the contract allows.
    #[error("an inline payload's decoded form is within the bound its contract declares")]
    DecodedTooLarge,
    /// The media type is empty or longer than the contract allows.
    #[error("an inline payload names a media type within the bound its contract declares")]
    MediaTypeRejected,
}

/// What a write did, when the whole answer is the address it changed.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ResourceMutationResult {
    /// Address the mutation changed.
    pub repository_path: RepositoryPath,
}

impl ResourceMutationResult {
    /// Requires this result to answer a request that determined `expected`.
    ///
    /// # Errors
    ///
    /// Returns [`MutationResultFailure::NotThisRequest`] when the address is
    /// another request's.
    pub fn require_answers(&self, expected: &RepositoryPath) -> Result<(), MutationResultFailure> {
        if self.repository_path == *expected {
            Ok(())
        } else {
            Err(MutationResultFailure::NotThisRequest)
        }
    }
}

/// What a deletion removed.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct DeletedResourceResult {
    /// How many nodes the deletion removed, the address itself included.
    pub removed_node_count: u64,
    /// Address that is no longer there.
    pub repository_path: RepositoryPath,
}

impl DeletedResourceResult {
    /// Returns the deletion `repository_path` and `removed_node_count` describe.
    ///
    /// # Errors
    ///
    /// Returns [`MutationResultFailure::CountTooLarge`] when the count exceeds
    /// the contract's deleted-node bound.
    pub fn new(
        repository_path: RepositoryPath,
        removed_node_count: u64,
    ) -> Result<Self, MutationResultFailure> {
        require_within(removed_node_count, "maximum_deleted_nodes")?;
        Ok(Self { removed_node_count, repository_path })
    }

    /// Requires this result to answer a request that determined `expected`.
    ///
    /// # Errors
    ///
    /// Returns [`MutationResultFailure::NotThisRequest`] when the address is
    /// another request's.
    pub fn require_answers(&self, expected: &RepositoryPath) -> Result<(), MutationResultFailure> {
        if self.repository_path == *expected {
            Ok(())
        } else {
            Err(MutationResultFailure::NotThisRequest)
        }
    }
}

/// What a move moved, and what it adjusted on the way.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct MovedResourceResult {
    /// How many references the move rewrote.
    pub adjusted_reference_count: u64,
    /// Address the subtree arrived at.
    pub destination_path: RepositoryPath,
    /// Address the subtree left.
    pub source_path: RepositoryPath,
}

impl MovedResourceResult {
    /// Returns the move these three values describe.
    ///
    /// # Errors
    ///
    /// Returns [`MutationResultFailure::CountTooLarge`] when the count exceeds
    /// the contract's adjusted-reference bound, and
    /// [`MutationResultFailure::DestinationInsideSource`] when the destination
    /// is the source or lies within it.
    pub fn new(
        source_path: RepositoryPath,
        destination_path: RepositoryPath,
        adjusted_reference_count: u64,
    ) -> Result<Self, MutationResultFailure> {
        require_within(adjusted_reference_count, "maximum_adjusted_references")?;
        require_destination_outside_source(&source_path, &destination_path)?;
        Ok(Self { adjusted_reference_count, destination_path, source_path })
    }

    /// Requires this result to answer the move `source` to `destination`.
    ///
    /// # Errors
    ///
    /// Returns [`MutationResultFailure::NotThisRequest`] when either address is
    /// another request's.
    pub fn require_answers(
        &self,
        source: &RepositoryPath,
        destination: &RepositoryPath,
    ) -> Result<(), MutationResultFailure> {
        if self.source_path == *source && self.destination_path == *destination {
            Ok(())
        } else {
            Err(MutationResultFailure::NotThisRequest)
        }
    }
}

/// One deletion exactly as it is written on the wire.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct DeletedDocument {
    /// Address that is no longer there.
    repository_path: RepositoryPath,
    /// How many nodes the deletion removed.
    removed_node_count: u64,
}

impl<'de> Deserialize<'de> for DeletedResourceResult {
    fn deserialize<Source: serde::Deserializer<'de>>(
        deserializer: Source,
    ) -> Result<Self, Source::Error> {
        let document = DeletedDocument::deserialize(deserializer)?;
        Self::new(document.repository_path, document.removed_node_count)
            .map_err(Source::Error::custom)
    }
}

/// One move exactly as it is written on the wire.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct MovedDocument {
    /// How many references the move rewrote.
    adjusted_reference_count: u64,
    /// Address the subtree arrived at.
    destination_path: RepositoryPath,
    /// Address the subtree left.
    source_path: RepositoryPath,
}

impl<'de> Deserialize<'de> for MovedResourceResult {
    fn deserialize<Source: serde::Deserializer<'de>>(
        deserializer: Source,
    ) -> Result<Self, Source::Error> {
        let document = MovedDocument::deserialize(deserializer)?;
        Self::new(
            document.source_path,
            document.destination_path,
            document.adjusted_reference_count,
        )
        .map_err(Source::Error::custom)
    }
}

/// What a destructive command does about what points at its target.
///
/// Closed and without a default, because both answers are right somewhere.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReferencePolicy {
    /// Remove it anyway, leaving whatever pointed at it pointing nowhere.
    IgnoreReferences,
    /// Refuse while anything still points at it.
    RefuseWhenReferenced,
}

/// Content one creation carries inward, encoded in the request itself.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct InlineBinaryPayload {
    /// The media type the caller says these bytes are.
    media_type: String,
    /// The content, standard Base64 with canonical padding.
    encoded_content: String,
}

impl InlineBinaryPayload {
    /// Returns the payload `media_type` and `encoded_content` describe.
    ///
    /// The encoded length is checked before anything is decoded, so a request
    /// too large to accept never allocates the thing it is too large for.
    ///
    /// # Errors
    ///
    /// Returns [`InlinePayloadFailure::MediaTypeRejected`] when the media type
    /// is empty or over its bound, [`InlinePayloadFailure::EncodedTooLarge`]
    /// when the encoded form is over its bound,
    /// [`InlinePayloadFailure::EncodingMalformed`] when the encoded form is not
    /// standard Base64 with canonical padding, and
    /// [`InlinePayloadFailure::DecodedTooLarge`] when the decoded form is over
    /// its bound.
    pub fn new(media_type: &str, encoded_content: &str) -> Result<Self, InlinePayloadFailure> {
        let contract = CommandContract::embedded();
        let media_bound = contract.limit("maximum_inline_binary_media_type_bytes");
        if media_type.is_empty()
            || u64::try_from(media_type.len()).unwrap_or(u64::MAX) > media_bound
        {
            return Err(InlinePayloadFailure::MediaTypeRejected);
        }
        let encoded_bound = contract.limit("maximum_inline_binary_encoded_bytes");
        if u64::try_from(encoded_content.len()).unwrap_or(u64::MAX) > encoded_bound {
            return Err(InlinePayloadFailure::EncodedTooLarge);
        }
        accept_encoding(encoded_content)?;
        let decoded = STANDARD
            .decode(encoded_content)
            .map_err(|_| InlinePayloadFailure::EncodingMalformed)?;
        let decoded_bound = contract.limit("maximum_inline_binary_decoded_bytes");
        if u64::try_from(decoded.len()).unwrap_or(u64::MAX) > decoded_bound {
            return Err(InlinePayloadFailure::DecodedTooLarge);
        }
        Ok(Self { media_type: media_type.to_owned(), encoded_content: encoded_content.to_owned() })
    }

    /// Returns the media type the caller stated.
    #[must_use]
    pub fn media_type(&self) -> &str {
        &self.media_type
    }

    /// Returns the encoded content exactly as it was accepted.
    #[must_use]
    pub fn encoded_content(&self) -> &str {
        &self.encoded_content
    }

    /// Returns the content these bytes decode to.
    ///
    /// # Panics
    ///
    /// Panics when the accepted encoding no longer decodes, which cannot happen
    /// because construction decoded it and the value is immutable.
    #[must_use]
    pub fn decoded_content(&self) -> Vec<u8> {
        STANDARD.decode(&self.encoded_content).expect("an accepted payload decodes")
    }

    /// Returns how many bytes the content decodes to.
    #[must_use]
    pub fn decoded_byte_length(&self) -> u64 {
        u64::try_from(self.decoded_content().len()).unwrap_or(u64::MAX)
    }
}

/// One payload exactly as it is written on the wire.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct PayloadDocument {
    /// The media type the caller says these bytes are.
    media_type: String,
    /// The content, standard Base64 with canonical padding.
    encoded_content: String,
}

impl<'de> Deserialize<'de> for InlineBinaryPayload {
    fn deserialize<Source: serde::Deserializer<'de>>(
        deserializer: Source,
    ) -> Result<Self, Source::Error> {
        let document = PayloadDocument::deserialize(deserializer)?;
        Self::new(&document.media_type, &document.encoded_content).map_err(Source::Error::custom)
    }
}

/// Requires one reported count to be within the bound `limit` names.
fn require_within(count: u64, limit: &str) -> Result<(), MutationResultFailure> {
    if count > CommandContract::embedded().limit(limit) {
        return Err(MutationResultFailure::CountTooLarge);
    }
    Ok(())
}

/// Requires a destination to lie outside the subtree it is moved from.
///
/// Checked on the request as well as on the result, because a move into its own
/// subtree is the one mistake whose consequence is a tree nobody can put back:
/// the caller learns here rather than from what is left afterwards.
///
/// # Errors
///
/// Returns [`MutationResultFailure::DestinationInsideSource`] when the
/// destination is the source or lies within it.
pub fn require_destination_outside_source(
    source: &RepositoryPath,
    destination: &RepositoryPath,
) -> Result<(), MutationResultFailure> {
    if crate::command::query_paths::anchor_contains(source, destination) {
        return Err(MutationResultFailure::DestinationInsideSource);
    }
    Ok(())
}

/// Requires one encoded payload to be spelled the way this contract accepts.
///
/// The decoder would refuse most of these on its own. Doing it here as well
/// makes the refusal this contract's own answer rather than a dependency's, and
/// makes an interior line break refused whatever a future decoder tolerates.
fn accept_encoding(encoded: &str) -> Result<(), InlinePayloadFailure> {
    if !encoded.len().is_multiple_of(BASE64_GROUP) {
        return Err(InlinePayloadFailure::EncodingMalformed);
    }
    let body = encoded.trim_end_matches(BASE64_PADDING);
    let padding = encoded.len() - body.len();
    if padding > MAXIMUM_PADDING {
        return Err(InlinePayloadFailure::EncodingMalformed);
    }
    if body.chars().any(|character| !BASE64_ALPHABET.contains(character)) {
        return Err(InlinePayloadFailure::EncodingMalformed);
    }
    Ok(())
}

/// Why a property mutation is not one this contract can carry.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum PropertyMutationFailure {
    /// A removal list names more properties than the contract allows.
    #[error("a removal list is within the bound its contract declares")]
    TooManyRemovals,
    /// A removal list is empty, repeats a name, or is out of order.
    #[error("a removal list is nonempty, distinct, and ascending")]
    RemovalsNotAscendingDistinct,
    /// One property is both assigned and removed by the same request.
    #[error("a property is assigned or removed, and not both by one request")]
    BothAssignedAndRemoved,
    /// The request would change nothing.
    #[error("a mutation changes something")]
    ChangesNothing,
}

/// Properties one mutation removes.
///
/// Ascending and distinct for the reason every set in this family is: two
/// documents that mean the same thing must not be able to serialize
/// differently. Nonempty because an empty removal list is a caller asking for
/// nothing while appearing to ask for something, and the request that meant
/// nothing is the one that omits the member.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(transparent)]
pub struct RemovedPropertyNames {
    /// The names, ascending and distinct.
    names: Vec<PropertyName>,
}

impl RemovedPropertyNames {
    /// Returns the removal list `names` describes.
    ///
    /// # Errors
    ///
    /// Returns [`PropertyMutationFailure::TooManyRemovals`] above the contract's
    /// removal bound and
    /// [`PropertyMutationFailure::RemovalsNotAscendingDistinct`] when the list
    /// is empty, repeats a name, or is out of order.
    pub fn new(names: Vec<PropertyName>) -> Result<Self, PropertyMutationFailure> {
        let bound = CommandContract::embedded().limit("maximum_removed_property_names");
        match require_ascending_distinct(&names, bound) {
            Ok(()) => Ok(Self { names }),
            Err(ListingResultFailure::TooManyRequested) => {
                Err(PropertyMutationFailure::TooManyRemovals)
            }
            Err(_) => Err(PropertyMutationFailure::RemovalsNotAscendingDistinct),
        }
    }

    /// Returns the names, ascending.
    #[must_use]
    pub fn names(&self) -> &[PropertyName] {
        &self.names
    }

    /// Reports whether this list removes `name`.
    #[must_use]
    pub fn removes(&self, name: &str) -> bool {
        self.names.iter().any(|removed| removed.as_text() == name)
    }
}

impl<'de> Deserialize<'de> for RemovedPropertyNames {
    fn deserialize<Source: serde::Deserializer<'de>>(
        deserializer: Source,
    ) -> Result<Self, Source::Error> {
        Self::new(Vec::<PropertyName>::deserialize(deserializer)?).map_err(Source::Error::custom)
    }
}

/// Requires one update request to change something, and to say so once.
///
/// `changes_something_else` is whatever the command carries besides these two
/// documents - a title, a state - so that a request carrying only that is not
/// mistaken for a request carrying nothing.
///
/// # Errors
///
/// Returns [`PropertyMutationFailure::BothAssignedAndRemoved`] when one property
/// is named in both documents, and [`PropertyMutationFailure::ChangesNothing`]
/// when the request would change nothing at all.
pub fn require_property_mutation(
    properties: Option<&MutationProperties>,
    removals: Option<&RemovedPropertyNames>,
    changes_something_else: bool,
) -> Result<(), PropertyMutationFailure> {
    if let (Some(assigned), Some(removed)) = (properties, removals)
        && assigned.values().keys().any(|name| removed.removes(name))
    {
        return Err(PropertyMutationFailure::BothAssignedAndRemoved);
    }
    let assigns = properties.is_some_and(|assigned| !assigned.values().is_empty());
    if assigns || removals.is_some() || changes_something_else {
        Ok(())
    } else {
        Err(PropertyMutationFailure::ChangesNothing)
    }
}
