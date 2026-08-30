//! Loading one repository subtree, said exactly.
//!
//! The repository is typed and JSON is not, so every value here carries its
//! JCR type beside it rather than being inferred from the shape of a token. A
//! `long` and a `double` that both happen to be whole are different properties;
//! a `path` and a `string` that spell the same characters are different
//! properties. Nothing in this module guesses.
//!
//! Three encodings are deliberately not the obvious ones:
//!
//! - Binary carries a length and no bytes. Reading a hundred megabytes to
//!   report that it is a hundred megabytes is the cost this contract avoids.
//! - Double is sixteen hexadecimal digits of IEEE 754 binary64, not a JSON
//!   number. JSON cannot write an infinity, a negative zero, or a particular
//!   NaN payload, and a repository holds all three. The bits survive.
//! - Long is a minimal decimal string, because the JCR range is the full signed
//!   64-bit one and a parser reading JSON numbers as binary64 rounds its ends.
//!
//! A result is Inline or Artifact, never both and never neither, and the
//! canonical bytes of the document alone decide which. The echoed path, the
//! discriminator, the descriptor, and every transport envelope are excluded on
//! purpose: if they counted, one subtree would come back inline from one
//! request and as a file from another that differed only in path length.

use std::collections::BTreeMap;

use serde::de::Error as _;
use serde::{Deserialize, Serialize};

use crate::command::artifact::{ArtifactDescriptor, ArtifactSlotDeclaration};
use crate::command::command_identity::CommandContract;
use crate::command::property_value::{DateTimeString, DecimalString};
use crate::command::repository_path::{RepositoryName, RepositoryPath, RepositoryPropertyPath};

/// Wire name of this command.
pub const LOAD_COMMAND_WIRE_NAME: &str = "load_content_as_json";

/// Wire spelling of a result that carries its document.
pub const INLINE_DISPOSITION: &str = "inline";

/// Wire spelling of a result that carries a descriptor instead.
pub const ARTIFACT_DISPOSITION: &str = "artifact";

/// Wire spelling of a property holding one value.
pub const SINGLE_CARDINALITY: &str = "single";

/// Wire spelling of a property holding a sequence.
pub const MULTIPLE_CARDINALITY: &str = "multiple";

/// Every JCR type this contract maps, in the order the architecture lists them.
pub const DECLARED_PROPERTY_TYPES: &[&str] = &[
    "string",
    "binary",
    "long",
    "double",
    "date",
    "boolean",
    "name",
    "path",
    "reference",
    "weak_reference",
    "uri",
    "decimal",
];

/// Hexadecimal digits one binary64 bit string is written with.
const BINARY64_DIGITS: usize = 16;

/// Radix a bit string is written in.
const HEXADECIMAL_RADIX: u32 = 16;

/// Returns the depth an omitted request resolves to.
#[must_use]
pub fn default_load_depth() -> u64 {
    CommandContract::embedded().limit("default_load_depth")
}

/// Returns the deepest subtree this contract loads.
#[must_use]
pub fn maximum_load_depth() -> u64 {
    CommandContract::embedded().limit("maximum_load_depth")
}

/// Returns the largest canonical document this contract produces.
#[must_use]
pub fn maximum_load_document_bytes() -> u64 {
    CommandContract::embedded().limit("maximum_load_document_bytes")
}

/// Returns the largest document an agent may return inline.
#[must_use]
pub fn maximum_agent_inline_loaded_document_bytes() -> u64 {
    CommandContract::embedded().limit("maximum_agent_inline_loaded_document_bytes")
}

/// Returns the largest reference identifier this contract accepts.
#[must_use]
pub fn maximum_repository_reference_bytes() -> u64 {
    CommandContract::embedded().limit("maximum_repository_reference_bytes")
}

/// Returns the largest identifier a `uri` property may carry.
#[must_use]
pub fn maximum_repository_uniform_resource_identifier_bytes() -> u64 {
    CommandContract::embedded().limit("maximum_repository_uniform_resource_identifier_bytes")
}

/// Returns the largest textual property this contract accepts.
#[must_use]
pub fn maximum_property_string_bytes() -> u64 {
    CommandContract::embedded().limit("maximum_property_string_bytes")
}

/// Reason a load value could not be built.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum LoadFailure {
    /// The depth is deeper than this contract loads.
    #[error("a load depth is at most {maximum}", maximum = maximum_load_depth())]
    DepthAboveMaximum,
    /// The property type is not one of the twelve.
    #[error("a property type is one of the twelve this contract maps")]
    UnknownPropertyType,
    /// The cardinality is neither of the two.
    #[error("a property cardinality is either {SINGLE_CARDINALITY} or {MULTIPLE_CARDINALITY}")]
    UnknownCardinality,
    /// A value does not match the type it declared.
    #[error("a property value does not match the type it declares")]
    TypeMismatch,
    /// A single-valued property carries anything but one value.
    #[error("a single-valued property carries exactly one value")]
    NotExactlyOneValue,
    /// A sequence mixes types.
    #[error("every value of one property has that property's declared type")]
    NotHomogeneous,
    /// A binary length is negative or larger than the repository can report.
    #[error("a binary length is a minimal decimal from zero through the signed 64-bit maximum")]
    BinaryLengthOutOfRange,
    /// A binary64 bit string is not sixteen lowercase hexadecimal digits.
    #[error("a double is exactly sixteen lowercase hexadecimal digits of binary64 bits")]
    DoubleNotBitString,
    /// A reference identifier is empty, controlled, or over bound.
    #[error("a reference identifier is nonempty, control-free, and at most {maximum} bytes", maximum = maximum_repository_reference_bytes())]
    ReferenceOutOfBounds,
    /// An identifier is not the bounded RFC 3986 form this contract accepts.
    #[error("a uri is an absolute RFC 3986 identifier in ASCII with valid percent triplets")]
    UniformResourceIdentifierNotCanonical,
    /// A textual value is longer than the contract allows.
    #[error("a textual property value is at most {maximum} bytes", maximum = maximum_property_string_bytes())]
    StringTooLong,
    /// The disposition is neither of the two.
    #[error("a load result is either {INLINE_DISPOSITION} or {ARTIFACT_DISPOSITION}")]
    UnknownDisposition,
    /// The disposition disagrees with the document it carries.
    #[error("a document at or below the inline maximum is inline, and a larger one is an artifact")]
    DispositionDoesNotMatchDocument,
    /// A document is larger than this contract produces.
    #[error("a loaded document is at most {maximum} bytes", maximum = maximum_load_document_bytes())]
    DocumentTooLong,
    /// An artifact result does not fill the slot this command declares.
    #[error(
        "a loaded artifact fills the slot this command declares, with its exact media type, suggested file name, and length"
    )]
    ArtifactDoesNotMatchSlot,
    /// A result does not answer the command it claims to answer.
    #[error("a load result echoes the path its command asked for")]
    NotThisRequest,
}

/// How far below the requested resource a load reaches.
///
/// Zero is the requested resource alone, and the maximum is inclusive. A child
/// has depth one more than its parent, so the number counts edges rather than
/// levels.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize)]
#[serde(into = "u64")]
pub struct LoadDepth {
    /// Edges below the requested resource.
    value: u64,
}

impl LoadDepth {
    /// Returns the depth an omitted request resolves to.
    #[must_use]
    pub fn default_depth() -> Self {
        Self { value: default_load_depth() }
    }

    /// Returns the depth `value` names.
    ///
    /// # Errors
    ///
    /// Returns [`LoadFailure::DepthAboveMaximum`] above the inclusive maximum.
    pub fn new(value: u64) -> Result<Self, LoadFailure> {
        if value > maximum_load_depth() {
            return Err(LoadFailure::DepthAboveMaximum);
        }
        Ok(Self { value })
    }

    /// Returns how many edges below the requested resource this reaches.
    #[must_use]
    pub fn edges(self) -> u64 {
        self.value
    }
}

impl From<LoadDepth> for u64 {
    fn from(depth: LoadDepth) -> Self {
        depth.value
    }
}

/// One request to load a subtree.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LoadContentAsJavaScriptObjectNotationCommand {
    /// How far below the requested resource to reach, when the caller said.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub depth: Option<LoadDepth>,
    /// Resource to load.
    pub path: RepositoryPath,
}

impl LoadContentAsJavaScriptObjectNotationCommand {
    /// Returns the depth this request reaches, stated or resolved.
    #[must_use]
    pub fn resolved_depth(&self) -> LoadDepth {
        self.depth.unwrap_or_else(LoadDepth::default_depth)
    }
}

impl<'de> Deserialize<'de> for LoadDepth {
    fn deserialize<Source: serde::Deserializer<'de>>(
        deserializer: Source,
    ) -> Result<Self, Source::Error> {
        let value = u64::deserialize(deserializer)?;
        Self::new(value).map_err(Source::Error::custom)
    }
}

/// How many bytes one binary property holds.
///
/// The length is what the repository reported and the bytes are never read. A
/// negative report is unrepresentable rather than a number to clamp.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct BinaryMetadata {
    /// Length, as its minimal decimal spelling.
    #[serde(serialize_with = "write_binary_length")]
    pub byte_length: i64,
}

/// Writes one binary length as its minimal decimal string.
fn write_binary_length<Target: serde::Serializer>(
    length: &i64,
    serializer: Target,
) -> Result<Target::Ok, Target::Error> {
    serializer.serialize_str(&length.to_string())
}

impl BinaryMetadata {
    /// Returns the metadata `spelling` names.
    ///
    /// # Errors
    ///
    /// Returns [`LoadFailure::BinaryLengthOutOfRange`] for a negative length, a
    /// nonminimal spelling, or a value outside the signed 64-bit range JCR
    /// reports.
    pub fn new(spelling: &str) -> Result<Self, LoadFailure> {
        let out_of_range = || LoadFailure::BinaryLengthOutOfRange;
        let minimal = !spelling.is_empty()
            && spelling.bytes().all(|byte| byte.is_ascii_digit())
            && (spelling == "0" || !spelling.starts_with('0'));
        if !minimal {
            return Err(out_of_range());
        }
        let byte_length = spelling.parse::<i64>().map_err(|_| out_of_range())?;
        Ok(Self { byte_length })
    }
}

/// The bits of one binary64 value.
///
/// Held as bits rather than as a number so a signed zero, an infinity, and each
/// distinct NaN payload survive unchanged; two NaN payloads stay two values.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(into = "String")]
pub struct DoubleBits {
    /// The retrieved bits, most significant byte first.
    bits: u64,
}

impl DoubleBits {
    /// Returns the bits `spelling` writes.
    ///
    /// # Errors
    ///
    /// Returns [`LoadFailure::DoubleNotBitString`] for anything but exactly
    /// sixteen lowercase hexadecimal digits.
    pub fn new(spelling: &str) -> Result<Self, LoadFailure> {
        let canonical = spelling.len() == BINARY64_DIGITS
            && spelling
                .chars()
                .all(|character| character.is_ascii_hexdigit() && !character.is_ascii_uppercase());
        if !canonical {
            return Err(LoadFailure::DoubleNotBitString);
        }
        let bits = u64::from_str_radix(spelling, HEXADECIMAL_RADIX)
            .map_err(|_| LoadFailure::DoubleNotBitString)?;
        Ok(Self { bits })
    }

    /// Returns the bits themselves.
    #[must_use]
    pub fn bits(self) -> u64 {
        self.bits
    }
}

impl From<DoubleBits> for String {
    fn from(value: DoubleBits) -> Self {
        format!("{:0width$x}", value.bits, width = BINARY64_DIGITS)
    }
}

/// One bounded identifier a reference property carries.
///
/// Kept exactly as retrieved: a reference identifies a node, and normalizing it
/// would be inventing a different reference.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(into = "String")]
pub struct ReferenceIdentifier {
    /// The identifier, exactly as retrieved.
    value: String,
}

impl ReferenceIdentifier {
    /// Returns the identifier `spelling` carries.
    ///
    /// # Errors
    ///
    /// Returns [`LoadFailure::ReferenceOutOfBounds`] when empty, over bound, or
    /// carrying a control character.
    pub fn new(spelling: impl Into<String>) -> Result<Self, LoadFailure> {
        let value = spelling.into();
        let bounded = !value.is_empty()
            && u64::try_from(value.len()).unwrap_or(u64::MAX)
                <= maximum_repository_reference_bytes()
            && !value.chars().any(char::is_control);
        if bounded { Ok(Self { value }) } else { Err(LoadFailure::ReferenceOutOfBounds) }
    }

    /// Returns the identifier, exactly as retrieved.
    #[must_use]
    pub fn as_text(&self) -> &str {
        &self.value
    }
}

impl From<ReferenceIdentifier> for String {
    fn from(value: ReferenceIdentifier) -> Self {
        value.value
    }
}

/// One absolute identifier a `uri` property carries.
///
/// Four things are required and nothing else is: a scheme, ASCII throughout,
/// characters the grammar allows, and well-formed percent triplets. Spelling
/// survives exactly, so a non-ASCII character has to arrive percent encoded and
/// two escapes that differ only in case stay two spellings.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(into = "String")]
pub struct RepositoryUniformResourceIdentifier {
    /// The identifier, exactly as retrieved.
    value: String,
}

impl RepositoryUniformResourceIdentifier {
    /// Returns the identifier `spelling` carries.
    ///
    /// # Errors
    ///
    /// Returns [`LoadFailure::UniformResourceIdentifierNotCanonical`] for a
    /// relative reference, a non-ASCII character, a character outside the
    /// grammar, or a malformed percent escape, and
    /// [`LoadFailure::ReferenceOutOfBounds`] above the named bound.
    pub fn new(spelling: impl Into<String>) -> Result<Self, LoadFailure> {
        /// Characters the grammar allows beside letters and digits.
        const ALLOWED_PUNCTUATION: &str = "-._~:/?#[]@!$&'()*+,;=%";

        let value = spelling.into();
        let malformed = || LoadFailure::UniformResourceIdentifierNotCanonical;
        if u64::try_from(value.len()).unwrap_or(u64::MAX)
            > maximum_repository_uniform_resource_identifier_bytes()
        {
            return Err(LoadFailure::ReferenceOutOfBounds);
        }
        if !value.is_ascii() {
            return Err(malformed());
        }
        accept_scheme(&value)?;
        if !value.chars().all(|character| {
            character.is_ascii_alphanumeric() || ALLOWED_PUNCTUATION.contains(character)
        }) {
            return Err(malformed());
        }
        accept_percent_triplets(&value)?;
        Ok(Self { value })
    }

    /// Returns the identifier, exactly as retrieved.
    #[must_use]
    pub fn as_text(&self) -> &str {
        &self.value
    }
}

/// Requires an identifier to begin with a scheme.
fn accept_scheme(value: &str) -> Result<(), LoadFailure> {
    /// Characters a scheme may carry after its first.
    const SCHEME_PUNCTUATION: &str = "+-.";

    let malformed = || LoadFailure::UniformResourceIdentifierNotCanonical;
    let (scheme, _) = value.split_once(':').ok_or_else(malformed)?;
    let mut characters = scheme.chars();
    let leads = characters.next().is_some_and(|character| character.is_ascii_alphabetic());
    let follows = characters.all(|character| {
        character.is_ascii_alphanumeric() || SCHEME_PUNCTUATION.contains(character)
    });
    if leads && follows { Ok(()) } else { Err(malformed()) }
}

/// Requires every percent sign to introduce two hexadecimal digits.
fn accept_percent_triplets(value: &str) -> Result<(), LoadFailure> {
    /// Digits one percent escape carries.
    const ESCAPE_DIGITS: usize = 2;

    let bytes = value.as_bytes();
    for (position, byte) in bytes.iter().enumerate() {
        if *byte != b'%' {
            continue;
        }
        let escape = bytes.get(position + 1..=position + ESCAPE_DIGITS);
        let well_formed = escape.is_some_and(|digits| digits.iter().all(u8::is_ascii_hexdigit));
        if !well_formed {
            return Err(LoadFailure::UniformResourceIdentifierNotCanonical);
        }
    }
    Ok(())
}

impl From<RepositoryUniformResourceIdentifier> for String {
    fn from(value: RepositoryUniformResourceIdentifier) -> Self {
        value.value
    }
}

/// One repository value, with the JCR type it actually has.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(untagged)]
pub enum LoadedScalar {
    /// A JCR String.
    Text(String),
    /// A JCR Binary, as metadata alone.
    Binary(BinaryMetadata),
    /// A JCR Long, as its minimal decimal spelling.
    Long(String),
    /// A JCR Double, as its bits.
    Double(DoubleBits),
    /// A JCR Date, in the interoperable subset.
    Date(#[serde(serialize_with = "write_date")] DateTimeString),
    /// A JCR Boolean.
    Boolean(bool),
    /// A JCR Name.
    Name(RepositoryName),
    /// A JCR Path.
    Path(RepositoryPropertyPath),
    /// A JCR Reference or WeakReference.
    Reference(ReferenceIdentifier),
    /// A JCR URI.
    UniformResourceIdentifier(RepositoryUniformResourceIdentifier),
    /// A JCR Decimal, keeping its scale.
    Decimal(#[serde(serialize_with = "write_decimal")] DecimalString),
}

/// Writes one instant as the spelling it arrived with.
fn write_date<Target: serde::Serializer>(
    value: &DateTimeString,
    serializer: Target,
) -> Result<Target::Ok, Target::Error> {
    serializer.serialize_str(value.as_text())
}

/// Writes one decimal as the spelling it arrived with, scale included.
fn write_decimal<Target: serde::Serializer>(
    value: &DecimalString,
    serializer: Target,
) -> Result<Target::Ok, Target::Error> {
    serializer.serialize_str(value.as_text())
}

impl LoadedScalar {
    /// Returns whether this value can stand for `property_type`.
    #[must_use]
    pub fn suits(&self, property_type: &str) -> bool {
        matches!(
            (self, property_type),
            (Self::Text(_), "string")
                | (Self::Binary(_), "binary")
                | (Self::Long(_), "long")
                | (Self::Double(_), "double")
                | (Self::Date(_), "date")
                | (Self::Boolean(_), "boolean")
                | (Self::Name(_), "name")
                | (Self::Path(_), "path")
                | (Self::Reference(_), "reference" | "weak_reference")
                | (Self::UniformResourceIdentifier(_), "uri")
                | (Self::Decimal(_), "decimal")
        )
    }
}

/// One property of one resource, with its type and its cardinality.
///
/// A multi-valued property may be empty, because the type is stated rather than
/// inferred from a value that would have to exist to be inspected.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RepositoryJavaScriptObjectNotationPropertyValue {
    /// Which of the twelve JCR types this is.
    property_type: String,
    /// Whether the repository holds one value or a sequence.
    multiple: bool,
    /// The values, in repository order.
    values: Vec<LoadedScalar>,
}

impl RepositoryJavaScriptObjectNotationPropertyValue {
    /// Returns one property of either cardinality.
    ///
    /// # Errors
    ///
    /// Returns [`LoadFailure::UnknownPropertyType`] for an undeclared type,
    /// [`LoadFailure::NotExactlyOneValue`] when a single-valued property does
    /// not carry exactly one, and [`LoadFailure::NotHomogeneous`] when a value
    /// does not suit the declared type.
    pub fn new(
        property_type: &str,
        multiple: bool,
        values: Vec<LoadedScalar>,
    ) -> Result<Self, LoadFailure> {
        if !DECLARED_PROPERTY_TYPES.contains(&property_type) {
            return Err(LoadFailure::UnknownPropertyType);
        }
        if !multiple && values.len() != 1 {
            return Err(LoadFailure::NotExactlyOneValue);
        }
        if !values.iter().all(|value| value.suits(property_type)) {
            return Err(LoadFailure::NotHomogeneous);
        }
        Ok(Self { property_type: property_type.to_owned(), multiple, values })
    }

    /// Returns which of the twelve JCR types this is.
    #[must_use]
    pub fn property_type(&self) -> &str {
        &self.property_type
    }

    /// Returns the wire spelling of this property's cardinality.
    #[must_use]
    pub fn cardinality(&self) -> &'static str {
        if self.multiple { MULTIPLE_CARDINALITY } else { SINGLE_CARDINALITY }
    }

    /// Returns the values, in repository order.
    #[must_use]
    pub fn values(&self) -> &[LoadedScalar] {
        &self.values
    }
}

/// One property exactly as it is written on the wire.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct PropertyDocument {
    /// Whether the repository holds one value or a sequence.
    cardinality: String,
    /// Which of the twelve JCR types this is.
    property_type: String,
    /// The single value, on a single-valued property alone.
    #[serde(default)]
    value: Option<serde_json::Value>,
    /// The values, on a multiple-valued property alone.
    #[serde(default)]
    values: Option<Vec<serde_json::Value>>,
}

impl TryFrom<PropertyDocument> for RepositoryJavaScriptObjectNotationPropertyValue {
    type Error = LoadFailure;

    fn try_from(document: PropertyDocument) -> Result<Self, Self::Error> {
        let written = match (document.cardinality.as_str(), document.value, document.values) {
            (SINGLE_CARDINALITY, Some(value), None) => vec![value],
            (MULTIPLE_CARDINALITY, None, Some(values)) => values,
            (SINGLE_CARDINALITY | MULTIPLE_CARDINALITY, _, _) => {
                return Err(LoadFailure::TypeMismatch);
            }
            _ => return Err(LoadFailure::UnknownCardinality),
        };
        let multiple = document.cardinality == MULTIPLE_CARDINALITY;
        let values = written
            .into_iter()
            .map(|value| read_scalar(&document.property_type, value))
            .collect::<Result<Vec<_>, _>>()?;
        Self::new(&document.property_type, multiple, values)
    }
}

/// Reads one value of the declared type, without inferring the type from it.
fn read_scalar(property_type: &str, value: serde_json::Value) -> Result<LoadedScalar, LoadFailure> {
    let mismatch = || LoadFailure::TypeMismatch;
    match property_type {
        "binary" => read_binary(&value),
        "boolean" => value.as_bool().map(LoadedScalar::Boolean).ok_or_else(mismatch),
        "string" => {
            let text = value.as_str().ok_or_else(mismatch)?;
            if u64::try_from(text.len()).unwrap_or(u64::MAX) > maximum_property_string_bytes() {
                return Err(LoadFailure::StringTooLong);
            }
            Ok(LoadedScalar::Text(text.to_owned()))
        }
        _ => read_written_scalar(property_type, value.as_str().ok_or_else(mismatch)?),
    }
}

/// Reads one value whose JCR type travels as a string.
///
/// Every one of these is a string on the wire because JSON has no way to write
/// it faithfully: a Long would be rounded, a Double could not be nonfinite, and
/// the rest are grammars rather than numbers.
fn read_written_scalar(property_type: &str, spelling: &str) -> Result<LoadedScalar, LoadFailure> {
    let mismatch = || LoadFailure::TypeMismatch;
    match property_type {
        "long" => read_long(spelling),
        "double" => DoubleBits::new(spelling).map(LoadedScalar::Double),
        "date" => DateTimeString::new(spelling).map(LoadedScalar::Date).map_err(|_| mismatch()),
        "name" => RepositoryName::parse(spelling).map(LoadedScalar::Name).map_err(|_| mismatch()),
        "path" => {
            RepositoryPropertyPath::parse(spelling).map(LoadedScalar::Path).map_err(|_| mismatch())
        }
        "reference" | "weak_reference" => {
            ReferenceIdentifier::new(spelling).map(LoadedScalar::Reference)
        }
        "uri" => RepositoryUniformResourceIdentifier::new(spelling)
            .map(LoadedScalar::UniformResourceIdentifier),
        "decimal" => {
            DecimalString::new(spelling).map(LoadedScalar::Decimal).map_err(|_| mismatch())
        }
        _ => Err(LoadFailure::UnknownPropertyType),
    }
}

/// Reads one binary property's metadata.
fn read_binary(value: &serde_json::Value) -> Result<LoadedScalar, LoadFailure> {
    let mismatch = || LoadFailure::TypeMismatch;
    let object = value.as_object().ok_or_else(mismatch)?;
    if object.len() != 1 {
        return Err(mismatch());
    }
    let spelling =
        object.get("byte_length").and_then(serde_json::Value::as_str).ok_or_else(mismatch)?;
    BinaryMetadata::new(spelling).map(LoadedScalar::Binary)
}

/// Reads one long property's minimal decimal spelling.
fn read_long(spelling: &str) -> Result<LoadedScalar, LoadFailure> {
    let digits = spelling.strip_prefix('-').unwrap_or(spelling);
    let minimal = !digits.is_empty()
        && digits.bytes().all(|byte| byte.is_ascii_digit())
        && (digits == "0" || !digits.starts_with('0'))
        && !(digits == "0" && spelling.starts_with('-'));
    if !minimal {
        return Err(LoadFailure::TypeMismatch);
    }
    spelling.parse::<i64>().map_err(|_| LoadFailure::TypeMismatch)?;
    Ok(LoadedScalar::Long(spelling.to_owned()))
}

impl Serialize for RepositoryJavaScriptObjectNotationPropertyValue {
    fn serialize<Target: serde::Serializer>(
        &self,
        serializer: Target,
    ) -> Result<Target::Ok, Target::Error> {
        use serde::ser::SerializeStruct;

        /// Members one property writes.
        const MEMBERS: usize = 3;

        let mut property = serializer
            .serialize_struct("RepositoryJavaScriptObjectNotationPropertyValue", MEMBERS)?;
        property.serialize_field("cardinality", self.cardinality())?;
        property.serialize_field("property_type", &self.property_type)?;
        if self.multiple {
            property.serialize_field("values", &self.values)?;
        } else {
            property.serialize_field("value", &self.values[0])?;
        }
        property.end()
    }
}

impl<'de> Deserialize<'de> for RepositoryJavaScriptObjectNotationPropertyValue {
    fn deserialize<Source: serde::Deserializer<'de>>(
        deserializer: Source,
    ) -> Result<Self, Source::Error> {
        let document = PropertyDocument::deserialize(deserializer)?;
        Self::try_from(document).map_err(Source::Error::custom)
    }
}

/// One resource of the loaded document.
///
/// Every resource carries its own path, so no name is inferred from the syntax
/// of the object that contains it - which is what lets a same-name sibling be
/// represented at all.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RepositoryJavaScriptObjectNotationResource {
    /// Children through the requested depth, in repository order.
    pub children: Vec<RepositoryJavaScriptObjectNotationResource>,
    /// Whether readable children exist beyond that depth.
    pub children_truncated: bool,
    /// This resource's own absolute path.
    pub path: RepositoryPath,
    /// This resource's direct properties, by ascending name bytes.
    pub properties: BTreeMap<String, RepositoryJavaScriptObjectNotationPropertyValue>,
}

impl RepositoryJavaScriptObjectNotationResource {
    /// Returns the canonical bytes this document is charged as.
    ///
    /// Only the document itself is charged. The result discriminator, the
    /// echoed path, any descriptor, and every transport envelope are outside
    /// it, so the same subtree reaches the same disposition however it was
    /// asked for.
    ///
    /// # Errors
    ///
    /// Returns [`LoadFailure::DocumentTooLong`] above the document maximum.
    pub fn canonical_bytes(&self) -> Result<String, LoadFailure> {
        let written = serde_json::to_string(self).map_err(|_| LoadFailure::DocumentTooLong)?;
        if u64::try_from(written.len()).unwrap_or(u64::MAX) > maximum_load_document_bytes() {
            return Err(LoadFailure::DocumentTooLong);
        }
        Ok(written)
    }

    /// Returns which disposition a document of this size must use.
    ///
    /// # Errors
    ///
    /// Returns [`LoadFailure::DocumentTooLong`] above the document maximum.
    pub fn required_disposition(&self) -> Result<&'static str, LoadFailure> {
        let charged = u64::try_from(self.canonical_bytes()?.len()).unwrap_or(u64::MAX);
        Ok(if charged <= maximum_agent_inline_loaded_document_bytes() {
            INLINE_DISPOSITION
        } else {
            ARTIFACT_DISPOSITION
        })
    }
}

/// What a load produced.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LoadContentAsJavaScriptObjectNotationResult {
    /// The document itself, because it is small enough to carry.
    Inline {
        /// Document that was loaded.
        document: RepositoryJavaScriptObjectNotationResource,
        /// Path the command asked for.
        path: RepositoryPath,
    },
    /// A descriptor for the document, because it is not.
    Artifact {
        /// Descriptor of the document's bytes.
        artifact: ArtifactDescriptor,
        /// Path the command asked for.
        path: RepositoryPath,
    },
}

impl LoadContentAsJavaScriptObjectNotationResult {
    /// Returns the path this result echoes.
    #[must_use]
    pub fn path(&self) -> &RepositoryPath {
        match self {
            Self::Inline { path, .. } | Self::Artifact { path, .. } => path,
        }
    }

    /// Returns the wire spelling of this result's disposition.
    #[must_use]
    pub fn disposition(&self) -> &'static str {
        match self {
            Self::Inline { .. } => INLINE_DISPOSITION,
            Self::Artifact { .. } => ARTIFACT_DISPOSITION,
        }
    }

    /// Requires this result to be the one its own contents call for.
    ///
    /// # Errors
    ///
    /// Returns [`LoadFailure::DispositionDoesNotMatchDocument`] when an inline
    /// document is too large to carry, and
    /// [`LoadFailure::ArtifactDoesNotMatchSlot`] when an artifact does not fill
    /// the slot this command declares.
    pub fn require_consistent(&self) -> Result<(), LoadFailure> {
        match self {
            Self::Inline { document, .. } => {
                if document.required_disposition()? == INLINE_DISPOSITION {
                    Ok(())
                } else {
                    Err(LoadFailure::DispositionDoesNotMatchDocument)
                }
            }
            Self::Artifact { artifact, .. } => require_declared_slot(artifact),
        }
    }

    /// Requires this result to answer `command`.
    ///
    /// # Errors
    ///
    /// Returns [`LoadFailure::NotThisRequest`] when the echoed path is another
    /// request's, and whatever [`Self::require_consistent`] refuses.
    pub fn require_answers(
        &self,
        command: &LoadContentAsJavaScriptObjectNotationCommand,
    ) -> Result<(), LoadFailure> {
        if *self.path() != command.path {
            return Err(LoadFailure::NotThisRequest);
        }
        self.require_consistent()
    }
}

/// Requires one descriptor to fill the slot this command declares.
fn require_declared_slot(artifact: &ArtifactDescriptor) -> Result<(), LoadFailure> {
    let declaration = ArtifactSlotDeclaration::loaded_content();
    let mismatch = || LoadFailure::ArtifactDoesNotMatchSlot;
    declaration.admit(artifact).map_err(|_| mismatch())?;
    let expected_name = crate::command::artifact::LOADED_CONTENT_FILE_NAME;
    if artifact.media_type != declaration.media_type
        || artifact.suggested_file_name.as_text() != expected_name
    {
        return Err(mismatch());
    }
    if artifact.byte_length <= maximum_agent_inline_loaded_document_bytes() {
        return Err(LoadFailure::DispositionDoesNotMatchDocument);
    }
    Ok(())
}

/// One result exactly as it is written on the wire.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct ResultDocument {
    /// Descriptor, on an artifact result alone.
    #[serde(default)]
    artifact: Option<ArtifactDescriptor>,
    /// Which of the two shapes this is.
    disposition: String,
    /// Document, on an inline result alone.
    #[serde(default)]
    document: Option<RepositoryJavaScriptObjectNotationResource>,
    /// Path the command asked for.
    path: RepositoryPath,
}

impl TryFrom<ResultDocument> for LoadContentAsJavaScriptObjectNotationResult {
    type Error = LoadFailure;

    fn try_from(document: ResultDocument) -> Result<Self, Self::Error> {
        let result = match (document.disposition.as_str(), document.document, document.artifact) {
            (INLINE_DISPOSITION, Some(loaded), None) => {
                Self::Inline { document: loaded, path: document.path }
            }
            (ARTIFACT_DISPOSITION, None, Some(artifact)) => {
                Self::Artifact { artifact, path: document.path }
            }
            (INLINE_DISPOSITION | ARTIFACT_DISPOSITION, _, _) => {
                return Err(LoadFailure::DispositionDoesNotMatchDocument);
            }
            _ => return Err(LoadFailure::UnknownDisposition),
        };
        result.require_consistent()?;
        Ok(result)
    }
}

impl Serialize for LoadContentAsJavaScriptObjectNotationResult {
    fn serialize<Target: serde::Serializer>(
        &self,
        serializer: Target,
    ) -> Result<Target::Ok, Target::Error> {
        use serde::ser::SerializeStruct;

        /// Members one result writes.
        const MEMBERS: usize = 3;

        let mut result =
            serializer.serialize_struct("LoadContentAsJavaScriptObjectNotationResult", MEMBERS)?;
        match self {
            Self::Artifact { artifact, path } => {
                result.serialize_field("artifact", artifact)?;
                result.serialize_field("disposition", ARTIFACT_DISPOSITION)?;
                result.serialize_field("path", path)?;
            }
            Self::Inline { document, path } => {
                result.serialize_field("disposition", INLINE_DISPOSITION)?;
                result.serialize_field("document", document)?;
                result.serialize_field("path", path)?;
            }
        }
        result.end()
    }
}

impl<'de> Deserialize<'de> for LoadContentAsJavaScriptObjectNotationResult {
    fn deserialize<Source: serde::Deserializer<'de>>(
        deserializer: Source,
    ) -> Result<Self, Source::Error> {
        let document = ResultDocument::deserialize(deserializer)?;
        Self::try_from(document).map_err(Source::Error::custom)
    }
}

/// Which part of a repository held a value this contract cannot represent.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum UnsupportedValueRole {
    /// The name of a resource.
    ResourceName,
    /// The name of a property.
    PropertyName,
    /// The value of a property.
    PropertyValue,
}

/// Which budget a traversal ran out of.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LoadBudget {
    /// Resources visited.
    ResourceNodes,
    /// Property values read.
    PropertyValues,
    /// Bytes those values carried.
    PropertyBytes,
    /// Bytes the canonical document reached.
    SerializedDocumentBytes,
    /// Time elapsed since the traversal began.
    TraversalDuration,
}

/// Why a load produced no document.
///
/// Each shape is closed and carries nothing beyond what it names. A budget
/// failure in particular carries no partial document and no artifact: a partial
/// subtree that looked complete would be worse than no answer.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "failure", rename_all = "snake_case", deny_unknown_fields)]
pub enum LoadRefusal {
    /// The resource is not there.
    NotFound {
        /// Resource that is not there.
        path: RepositoryPath,
    },
    /// The resource is there and unreadable.
    AccessDenied {
        /// Resource that could not be read.
        path: RepositoryPath,
    },
    /// A name or value cannot be represented in this contract.
    ///
    /// The path names the nearest representable containing resource when the
    /// offending resource name is itself unrepresentable.
    UnsupportedRepositoryValue {
        /// Nearest representable resource.
        path: RepositoryPath,
        /// Where the offending value stood.
        value_role: UnsupportedValueRole,
    },
    /// The traversal ran out of one of its budgets.
    LoadBudgetExceeded {
        /// Budget that ran out.
        budget: LoadBudget,
    },
}
