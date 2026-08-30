//! What a command says about a file it produced, without saying where it is.
//!
//! An artifact descriptor is metadata about bytes that live somewhere else. It
//! deliberately carries no location: no remote address, no local path, no
//! inline bytes, and nothing credential-bearing. A descriptor that named a
//! place to fetch from would make every consumer a fetcher, and would let a
//! result direct a client at an address the command never validated.
//!
//! Three fields look similar and are not interchangeable, so they are three
//! types rather than three strings:
//!
//! - The identifier is stable identity. It is what two systems agree they are
//!   talking about.
//! - The slot is the command-declared purpose. It is fixed by the command's
//!   schema, not chosen per result.
//! - The suggested file name is presentation only. It is what a person might
//!   like the download called, and nothing reads it as a path or as identity -
//!   which is exactly why it refuses separators, traversal spellings, and
//!   controls, rather than sanitizing them into something plausible.
//!
//! A command declares a slot once, with its requirement and its maximum byte
//! length. The maximum is schema data an agent admits a retention against; the
//! descriptor supplies the exact length, at or below it.

use serde::de::Error as _;
use serde::{Deserialize, Serialize};

use crate::command::command_identity::CommandContract;

/// Slot the load command fills with a canonical document.
pub const LOADED_CONTENT_SLOT: &str = "loaded_content_json";

/// Media type that slot always carries.
pub const LOADED_CONTENT_MEDIA_TYPE: &str = "application/json";

/// File name that slot always suggests.
pub const LOADED_CONTENT_FILE_NAME: &str = "loaded-content.json";

/// Slot the package command fills with an archive.
pub const CONTENT_PACKAGE_SLOT: &str = "content_package";

/// Media type that slot always carries.
pub const CONTENT_PACKAGE_MEDIA_TYPE: &str = "application/zip";

/// Suffix the package command's suggested file name always ends with.
pub const CONTENT_PACKAGE_FILE_NAME_SUFFIX: &str = ".zip";

/// Wire spelling of a slot a result may fill or leave empty.
pub const OPTIONAL_ALTERNATIVE_REQUIREMENT: &str = "optional_alternative";

/// Wire spelling of a slot a result must fill.
pub const REQUIRED_REQUIREMENT: &str = "required";

/// Characters a hexadecimal digest is written with.
const DIGEST_CHARACTERS: usize = 64;

/// Spellings a suggested file name may never be, whole.
///
/// Each names a directory rather than a file, so a consumer that did treat the
/// name as a path would be handed one.
const REFUSED_FILE_NAMES: &[&str] = &[".", ".."];

/// Characters a suggested file name may never contain.
const REFUSED_FILE_NAME_CHARACTERS: &[char] = &['/', '\\', ':'];

/// Returns the largest artifact identifier this contract accepts.
#[must_use]
pub fn maximum_artifact_identifier_bytes() -> u64 {
    CommandContract::embedded().limit("maximum_artifact_identifier_bytes")
}

/// Returns the largest slot name this contract accepts.
#[must_use]
pub fn maximum_artifact_slot_bytes() -> u64 {
    CommandContract::embedded().limit("maximum_artifact_slot_bytes")
}

/// Returns the largest media type this contract accepts.
#[must_use]
pub fn maximum_artifact_media_type_bytes() -> u64 {
    CommandContract::embedded().limit("maximum_artifact_media_type_bytes")
}

/// Returns the largest suggested file name this contract accepts.
#[must_use]
pub fn maximum_artifact_suggested_file_name_bytes() -> u64 {
    CommandContract::embedded().limit("maximum_artifact_suggested_file_name_bytes")
}

/// Returns the largest loaded-content artifact this contract accepts.
#[must_use]
pub fn maximum_loaded_content_artifact_bytes() -> u64 {
    CommandContract::embedded().limit("maximum_loaded_content_artifact_bytes")
}

/// Returns the largest content package this contract accepts.
#[must_use]
pub fn maximum_package_output_bytes() -> u64 {
    CommandContract::embedded().limit("maximum_package_output_bytes")
}

/// Reason an artifact value could not be built.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum ArtifactFailure {
    /// An identifier is empty or longer than the contract allows.
    #[error("an artifact identifier is nonempty and at most {maximum} bytes", maximum = maximum_artifact_identifier_bytes())]
    IdentifierOutOfBounds,
    /// An identifier carries a character no identifier carries.
    #[error("an artifact identifier is printable ASCII")]
    IdentifierNotPrintable,
    /// A slot name is empty or longer than the contract allows.
    #[error("an artifact slot is nonempty and at most {maximum} bytes", maximum = maximum_artifact_slot_bytes())]
    SlotOutOfBounds,
    /// A slot name is spelled some way no slot is spelled.
    #[error("an artifact slot is lowercase ASCII letters, digits, and underscores")]
    SlotNotCanonical,
    /// A media type is empty or longer than the contract allows.
    #[error("a media type is nonempty and at most {maximum} bytes", maximum = maximum_artifact_media_type_bytes())]
    MediaTypeOutOfBounds,
    /// A media type is spelled some way no media type is spelled.
    #[error("a media type is a type and a subtype separated by a solidus")]
    MediaTypeNotCanonical,
    /// A suggested file name is empty or longer than the contract allows.
    #[error("a suggested file name is nonempty and at most {maximum} bytes", maximum = maximum_artifact_suggested_file_name_bytes())]
    FileNameOutOfBounds,
    /// A suggested file name could be read as a path.
    #[error(
        "a suggested file name is presentation metadata, so it carries no separator, no traversal spelling, and no control character"
    )]
    FileNameNotPresentational,
    /// A digest is not sixty-four lowercase hexadecimal characters.
    #[error("an artifact digest is exactly sixty-four lowercase hexadecimal characters")]
    DigestNotCanonical,
    /// A requirement is neither of the two.
    #[error(
        "an artifact requirement is either {OPTIONAL_ALTERNATIVE_REQUIREMENT} or {REQUIRED_REQUIREMENT}"
    )]
    UnknownRequirement,
    /// A descriptor is longer than the slot that declared it allows.
    #[error("an artifact is at most as long as its slot declares")]
    LongerThanSlotAllows,
    /// A descriptor names a slot the command did not declare.
    #[error("an artifact fills a slot its command declares")]
    SlotNotDeclared,
}

/// Builds one bounded, validated wrapper over a spelling.
macro_rules! artifact_text {
    ($(#[$attribute:meta])* $name:ident, $bound:ident, $out_of_bounds:ident, $accept:ident) => {
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
            /// Returns [`ArtifactFailure`] when the spelling is empty, longer
            /// than its named bound, or outside the alphabet its role allows.
            pub fn new(spelling: impl Into<String>) -> Result<Self, ArtifactFailure> {
                let value = spelling.into();
                let length = u64::try_from(value.len()).unwrap_or(u64::MAX);
                if value.is_empty() || length > $bound() {
                    return Err(ArtifactFailure::$out_of_bounds);
                }
                $accept(&value)?;
                Ok(Self { value })
            }

            /// Returns the spelling, exactly as it arrived.
            #[must_use]
            pub fn as_text(&self) -> &str {
                &self.value
            }
        }

        impl TryFrom<String> for $name {
            type Error = ArtifactFailure;

            fn try_from(value: String) -> Result<Self, Self::Error> {
                Self::new(value)
            }
        }

        impl From<$name> for String {
            fn from(value: $name) -> Self {
                value.value
            }
        }

        impl ::core::fmt::Display for $name {
            fn fmt(&self, formatter: &mut ::core::fmt::Formatter<'_>) -> ::core::fmt::Result {
                formatter.write_str(&self.value)
            }
        }
    };
}

artifact_text!(
    /// Stable identity of one artifact.
    ///
    /// This is what two systems agree they are talking about. It is not the
    /// slot, which says what the artifact is for, and not the file name, which
    /// says what to call it.
    ArtifactIdentifier,
    maximum_artifact_identifier_bytes,
    IdentifierOutOfBounds,
    accept_printable
);

artifact_text!(
    /// Command-declared purpose of one artifact.
    ///
    /// Fixed by the command's schema rather than chosen per result, so two
    /// results of one command fill the same slot and a consumer knows what it
    /// received without inspecting it.
    ArtifactSlot,
    maximum_artifact_slot_bytes,
    SlotOutOfBounds,
    accept_slot_spelling
);

artifact_text!(
    /// What kind of bytes the artifact holds.
    ArtifactMediaType,
    maximum_artifact_media_type_bytes,
    MediaTypeOutOfBounds,
    accept_media_type
);

artifact_text!(
    /// What a person might like the download called.
    ///
    /// Presentation only. Nothing reads it as a path or as identity, which is
    /// why it refuses separators and traversal spellings outright instead of
    /// sanitizing them into something that looks safe.
    SuggestedFileName,
    maximum_artifact_suggested_file_name_bytes,
    FileNameOutOfBounds,
    accept_presentational
);

/// Requires every character to be printable ASCII.
fn accept_printable(value: &str) -> Result<(), ArtifactFailure> {
    if value.chars().all(|character| character.is_ascii_graphic()) {
        Ok(())
    } else {
        Err(ArtifactFailure::IdentifierNotPrintable)
    }
}

/// Requires the one spelling every slot name uses.
fn accept_slot_spelling(value: &str) -> Result<(), ArtifactFailure> {
    let canonical = value.chars().all(|character| {
        character.is_ascii_lowercase() || character.is_ascii_digit() || character == '_'
    });
    if canonical { Ok(()) } else { Err(ArtifactFailure::SlotNotCanonical) }
}

/// Requires a type and a subtype, separated once.
fn accept_media_type(value: &str) -> Result<(), ArtifactFailure> {
    /// Separator between a media type and its subtype.
    const SUBTYPE_SEPARATOR: char = '/';

    let malformed = || ArtifactFailure::MediaTypeNotCanonical;
    let (kind, subtype) = value.split_once(SUBTYPE_SEPARATOR).ok_or_else(malformed)?;
    let shaped = |part: &str| {
        !part.is_empty()
            && part.chars().all(|character| {
                character.is_ascii_lowercase()
                    || character.is_ascii_digit()
                    || "+-.".contains(character)
            })
    };
    if shaped(kind) && shaped(subtype) { Ok(()) } else { Err(malformed()) }
}

/// Requires a name nothing could mistake for a path.
fn accept_presentational(value: &str) -> Result<(), ArtifactFailure> {
    let presentational = !REFUSED_FILE_NAMES.contains(&value)
        && !value.chars().any(|character| {
            character.is_control() || REFUSED_FILE_NAME_CHARACTERS.contains(&character)
        });
    if presentational { Ok(()) } else { Err(ArtifactFailure::FileNameNotPresentational) }
}

/// The content digest of one artifact.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct ArtifactDigest {
    /// The digest, in lowercase hexadecimal.
    value: String,
}

impl ArtifactDigest {
    /// Returns the digest `spelling` names.
    ///
    /// # Errors
    ///
    /// Returns [`ArtifactFailure::DigestNotCanonical`] for anything but exactly
    /// sixty-four lowercase hexadecimal characters. Uppercase is refused rather
    /// than folded, so one set of bytes has one digest spelling.
    pub fn new(spelling: impl Into<String>) -> Result<Self, ArtifactFailure> {
        let value = spelling.into();
        let canonical = value.len() == DIGEST_CHARACTERS
            && value
                .chars()
                .all(|character| character.is_ascii_hexdigit() && !character.is_ascii_uppercase());
        if canonical { Ok(Self { value }) } else { Err(ArtifactFailure::DigestNotCanonical) }
    }

    /// Returns the digest, in lowercase hexadecimal.
    #[must_use]
    pub fn as_text(&self) -> &str {
        &self.value
    }
}

impl TryFrom<String> for ArtifactDigest {
    type Error = ArtifactFailure;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        Self::new(value)
    }
}

impl From<ArtifactDigest> for String {
    fn from(digest: ArtifactDigest) -> Self {
        digest.value
    }
}

/// Whether a command's result must fill a slot.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ArtifactRequirement {
    /// The result fills this slot or supplies its declared alternative.
    OptionalAlternative,
    /// The result fills this slot.
    Required,
}

impl ArtifactRequirement {
    /// Returns the wire spelling of this requirement.
    #[must_use]
    pub fn as_text(self) -> &'static str {
        match self {
            Self::OptionalAlternative => OPTIONAL_ALTERNATIVE_REQUIREMENT,
            Self::Required => REQUIRED_REQUIREMENT,
        }
    }
}

/// One slot a command declares, once.
///
/// The maximum byte length lives here rather than on the descriptor because it
/// is schema data: an agent admits a retention against it before any bytes
/// exist. The descriptor then supplies the exact length, at or below it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ArtifactSlotDeclaration {
    /// The slot this command fills.
    pub slot: ArtifactSlot,
    /// Media type every artifact in it carries.
    pub media_type: ArtifactMediaType,
    /// Whether the result must fill it.
    pub requirement: ArtifactRequirement,
    /// Largest artifact this slot admits.
    pub maximum_byte_length: u64,
}

impl ArtifactSlotDeclaration {
    /// Returns the slot the load command declares.
    ///
    /// # Panics
    ///
    /// Panics when the declared spellings are themselves invalid, which is a
    /// defect in this module rather than in any caller's input.
    #[must_use]
    pub fn loaded_content() -> Self {
        Self {
            slot: ArtifactSlot::new(LOADED_CONTENT_SLOT).expect("the declared slot is valid"),
            media_type: ArtifactMediaType::new(LOADED_CONTENT_MEDIA_TYPE)
                .expect("the declared media type is valid"),
            requirement: ArtifactRequirement::OptionalAlternative,
            maximum_byte_length: maximum_loaded_content_artifact_bytes(),
        }
    }

    /// Returns the slot the package command declares.
    ///
    /// # Panics
    ///
    /// Panics when the declared spellings are themselves invalid.
    #[must_use]
    pub fn content_package() -> Self {
        Self {
            slot: ArtifactSlot::new(CONTENT_PACKAGE_SLOT).expect("the declared slot is valid"),
            media_type: ArtifactMediaType::new(CONTENT_PACKAGE_MEDIA_TYPE)
                .expect("the declared media type is valid"),
            requirement: ArtifactRequirement::Required,
            maximum_byte_length: maximum_package_output_bytes(),
        }
    }

    /// Returns every slot this plan's commands declare.
    ///
    /// Two, and no others. A command that declares no slot forbids one, which
    /// is why this is a closed list rather than a registry anything can add to.
    #[must_use]
    pub fn declared() -> Vec<Self> {
        vec![Self::loaded_content(), Self::content_package()]
    }

    /// Admits one descriptor into this slot.
    ///
    /// # Errors
    ///
    /// Returns [`ArtifactFailure::SlotNotDeclared`] when the descriptor names
    /// another slot and [`ArtifactFailure::LongerThanSlotAllows`] when it is
    /// longer than this slot admits.
    pub fn admit(&self, descriptor: &ArtifactDescriptor) -> Result<(), ArtifactFailure> {
        if descriptor.slot != self.slot {
            return Err(ArtifactFailure::SlotNotDeclared);
        }
        if descriptor.byte_length > self.maximum_byte_length {
            return Err(ArtifactFailure::LongerThanSlotAllows);
        }
        Ok(())
    }
}

/// What a result says about one artifact it produced.
///
/// Every field is separately validated and none substitutes for another. There
/// is nothing here that says where the bytes are.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ArtifactDescriptor {
    /// Stable identity.
    pub identifier: ArtifactIdentifier,
    /// Command-declared purpose.
    pub slot: ArtifactSlot,
    /// What kind of bytes these are.
    pub media_type: ArtifactMediaType,
    /// Exactly how many bytes there are.
    pub byte_length: u64,
    /// Their content digest.
    pub digest: ArtifactDigest,
    /// What a person might like the download called.
    pub suggested_file_name: SuggestedFileName,
}

/// One descriptor exactly as it is written on the wire.
///
/// Closed, so a field naming a location, a path, or inline bytes is refused by
/// the shape itself rather than by a rule someone has to remember to write.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct DescriptorDocument {
    /// Stable identity.
    identifier: ArtifactIdentifier,
    /// Command-declared purpose.
    slot: ArtifactSlot,
    /// What kind of bytes these are.
    media_type: ArtifactMediaType,
    /// Exactly how many bytes there are.
    byte_length: u64,
    /// Their content digest.
    digest: ArtifactDigest,
    /// What a person might like the download called.
    suggested_file_name: SuggestedFileName,
}

impl From<DescriptorDocument> for ArtifactDescriptor {
    fn from(document: DescriptorDocument) -> Self {
        Self {
            identifier: document.identifier,
            slot: document.slot,
            media_type: document.media_type,
            byte_length: document.byte_length,
            digest: document.digest,
            suggested_file_name: document.suggested_file_name,
        }
    }
}

impl<'de> Deserialize<'de> for ArtifactDescriptor {
    fn deserialize<Source: serde::Deserializer<'de>>(
        deserializer: Source,
    ) -> Result<Self, Source::Error> {
        let document = DescriptorDocument::deserialize(deserializer)
            .map_err(|failure| Source::Error::custom(failure.to_string()))?;
        Ok(Self::from(document))
    }
}
