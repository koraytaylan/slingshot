//! Reading one effective configuration, without touching it and without
//! leaking it.
//!
//! Two hazards shape everything here. The first is that inspection must not
//! become creation: `getConfiguration` and `getFactoryConfiguration` bind a
//! configuration into existence as a side effect of asking about one, so this
//! lookup calls neither. It builds one filter, lists, and checks what came back.
//!
//! The second is that a configuration holds secrets, so the decision to read a
//! value is made *before* the value is touched, from the key alone: password
//! evidence, no usable evidence, or a name that reads like a secret all redact,
//! and a redacted property is never fetched - not fetched and hidden, not
//! fetched. That ordering is why a malformed or oversized value under a
//! sensitive name is reported as redacted rather than as malformed: nothing
//! here ever learned what it was. The redaction carries no value, type,
//! carrier, length, hash, error text, or hint, because each is a channel.
//!
//! It does not claim to detect secret bytes that a compromised trusted agent
//! labels as an ordinary type under an innocuous name; nothing about a key can
//! prove that. Nor does it execute anything: Configuration Admin and the Meta
//! Type Service are the agent's to call, and this module owns the values, the
//! order, and the shapes.

use std::collections::BTreeMap;

use serde::de::Error as _;
use serde::{Deserialize, Serialize};
use unicode_normalization::UnicodeNormalization;

use crate::command::command_identity::CommandContract;

/// Wire name of this command.
pub const INSPECT_COMMAND_WIRE_NAME: &str = "inspect_open_service_gateway_initiative_configuration";

/// Property the lookup filter matches on.
pub const SERVICE_PERSISTENT_IDENTIFIER_PROPERTY: &str = "service.pid";

/// Characters a filter escapes, each with one preceding reverse solidus.
pub const FILTER_ESCAPED_CHARACTERS: &[char] = &['\\', '*', '(', ')'];

/// Spellings the sensitive-name policy refuses to read.
///
/// Matched against a name reduced to ASCII alphanumerics, so `API_KEY`,
/// `api-key`, and `apiKey` all reduce to `apikey` and all redact.
pub const SENSITIVE_NAME_LITERALS: &[&str] =
    &["password", "passwd", "secret", "token", "credential", "privatekey", "apikey", "accesskey"];

/// Every Java class this contract converts, in the order it names them.
pub const DECLARED_SCALAR_TYPES: &[&str] =
    &["string", "boolean", "character", "byte", "short", "integer", "long", "float", "double"];

/// Every carrier a configuration value can arrive in.
pub const DECLARED_CARDINALITIES: &[&str] =
    &["scalar", "primitive_array", "scalar_array", "collection"];

/// Hexadecimal digits one binary32 bit string is written with.
const BINARY32_DIGITS: usize = 8;

/// Hexadecimal digits one binary64 bit string is written with.
const BINARY64_DIGITS: usize = 16;

/// Radix a bit string is written in.
const HEXADECIMAL_RADIX: u32 = 16;

/// Returns the largest persistent identifier this contract accepts.
#[must_use]
pub fn maximum_configuration_persistent_identifier_bytes() -> u64 {
    CommandContract::embedded().limit("maximum_configuration_persistent_identifier_bytes")
}

/// Returns the largest lookup filter this contract builds.
#[must_use]
pub fn maximum_configuration_lookup_filter_bytes() -> u64 {
    CommandContract::embedded().limit("maximum_configuration_lookup_filter_bytes")
}

/// Returns the most configurations a lookup may match.
#[must_use]
pub fn maximum_configuration_lookup_matches() -> u64 {
    CommandContract::embedded().limit("maximum_configuration_lookup_matches")
}
/// Returns the largest property key this contract accepts.
#[must_use]
pub fn maximum_configuration_property_key_bytes() -> u64 {
    CommandContract::embedded().limit("maximum_configuration_property_key_bytes")
}

/// Returns the largest textual configuration value this contract accepts.
#[must_use]
pub fn maximum_configuration_scalar_string_bytes() -> u64 {
    CommandContract::embedded().limit("maximum_configuration_scalar_string_bytes")
}

/// Returns the most items one sequence may carry.
#[must_use]
pub fn maximum_configuration_sequence_items() -> u64 {
    CommandContract::embedded().limit("maximum_configuration_sequence_items")
}

/// Returns the largest canonical sequence this contract accepts.
#[must_use]
pub fn maximum_configuration_sequence_canonical_bytes() -> u64 {
    CommandContract::embedded().limit("maximum_configuration_sequence_canonical_bytes")
}

/// Returns the most properties one inspection reports.
#[must_use]
pub fn maximum_inspected_configuration_properties() -> u64 {
    CommandContract::embedded().limit("maximum_inspected_configuration_properties")
}

/// Returns the largest canonical result this contract produces.
#[must_use]
pub fn maximum_inspected_configuration_result_bytes() -> u64 {
    CommandContract::embedded().limit("maximum_inspected_configuration_result_bytes")
}

/// Reason a configuration value could not be built.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum ConfigurationFailure {
    /// An identifier is empty, controlled, or over bound.
    #[error("a persistent identifier is nonempty, control-free, and at most {maximum} bytes", maximum = maximum_configuration_persistent_identifier_bytes())]
    IdentifierOutOfBounds,
    /// An escaped filter is longer than the contract allows.
    #[error("a lookup filter is at most {maximum} bytes once escaped", maximum = maximum_configuration_lookup_filter_bytes())]
    FilterTooLong,
    /// A property key is empty, controlled, or over bound.
    #[error("a property key is nonempty, control-free, and at most {maximum} bytes", maximum = maximum_configuration_property_key_bytes())]
    KeyOutOfBounds,
    /// Two property keys are the same key.
    #[error("two property keys have the same folded identity")]
    DuplicateKey,
    /// The scalar type is not one of the nine.
    #[error("a configuration scalar is one of the nine Java classes this contract converts")]
    UnknownScalarType,
    /// The carrier is not one of the four.
    #[error("a configuration value arrives in one of the four carriers this contract knows")]
    UnknownCardinality,
    /// A value does not match the type it declared.
    #[error("a configuration value does not match the type it declares")]
    TypeMismatch,
    /// A character value is not exactly one non-surrogate scalar.
    #[error("a character is exactly one Unicode scalar value")]
    NotOneScalar,
    /// An integer is outside the exact width it declared.
    #[error("an integer lies in the exact range of the Java class it declares")]
    IntegerOutOfRange,
    /// A floating bit string is not the exact width for its class.
    #[error("a float is eight and a double is sixteen lowercase hexadecimal digits")]
    NotBitString,
    /// A string is longer than the contract allows.
    #[error("a configuration string is at most {maximum} bytes", maximum = maximum_configuration_scalar_string_bytes())]
    StringTooLong,
    /// A sequence carries more items than the contract allows.
    #[error("a sequence carries at most {maximum} items", maximum = maximum_configuration_sequence_items())]
    SequenceTooManyItems,
    /// A sequence is longer than the contract allows.
    #[error("a canonical sequence is at most {maximum} bytes", maximum = maximum_configuration_sequence_canonical_bytes())]
    SequenceTooLong,
    /// A sequence mixes types.
    #[error("every item of one sequence has that sequence's declared type")]
    MixedTypes,
    /// A primitive array declared the one type it cannot carry.
    #[error("a primitive array carries a primitive, which a string is not")]
    CarrierTypeMismatch,
    /// A redacted observation carried something beside its verdict.
    #[error("a redacted observation carries no value, type, carrier, length, or hint")]
    RedactionNotContentFree,
    /// Evidence and visibility disagree.
    #[error("only exact non-password evidence makes a value visible")]
    EvidenceDoesNotPermitVisibility,
    /// A result reports more properties than the contract allows.
    #[error("an inspection reports at most {maximum} properties", maximum = maximum_inspected_configuration_properties())]
    TooManyProperties,
    /// An absent configuration reported properties anyway.
    #[error("an absent configuration reports no properties")]
    AbsentWithProperties,
    /// A result does not answer the command it claims to answer.
    #[error("an inspection result echoes the persistent identifier its command asked for")]
    NotThisRequest,
}

/// One configuration's persistent identifier.
///
/// Filter metacharacters are permitted on purpose. A persistent identifier
/// containing an asterisk is a legal identifier, and refusing it would narrow
/// what can be inspected; the lookup escapes instead.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct OpenServiceGatewayInitiativePersistentIdentifier {
    /// The identifier, exactly as it arrived.
    value: String,
}

impl OpenServiceGatewayInitiativePersistentIdentifier {
    /// Returns the identifier `spelling` names.
    ///
    /// # Errors
    ///
    /// Returns [`ConfigurationFailure::IdentifierOutOfBounds`] when empty, over
    /// bound, or carrying a control character.
    pub fn new(spelling: impl Into<String>) -> Result<Self, ConfigurationFailure> {
        let value = spelling.into();
        let bounded = !value.is_empty()
            && u64::try_from(value.len()).unwrap_or(u64::MAX)
                <= maximum_configuration_persistent_identifier_bytes()
            && !value.chars().any(char::is_control);
        if bounded { Ok(Self { value }) } else { Err(ConfigurationFailure::IdentifierOutOfBounds) }
    }

    /// Returns the identifier, exactly as it arrived.
    #[must_use]
    pub fn as_text(&self) -> &str {
        &self.value
    }

    /// Returns the one lookup filter this identifier produces.
    ///
    /// Exactly four characters are escaped and every other scalar is copied, so
    /// an identifier cannot introduce a second filter term, a wildcard, or an
    /// unbalanced parenthesis.
    ///
    /// # Errors
    ///
    /// Returns [`ConfigurationFailure::FilterTooLong`] when escaping pushes the
    /// complete filter past its bound.
    pub fn lookup_filter(&self) -> Result<String, ConfigurationFailure> {
        let mut escaped = String::with_capacity(self.value.len());
        for character in self.value.chars() {
            if FILTER_ESCAPED_CHARACTERS.contains(&character) {
                escaped.push('\\');
            }
            escaped.push(character);
        }
        let filter = format!("({SERVICE_PERSISTENT_IDENTIFIER_PROPERTY}={escaped})");
        if u64::try_from(filter.len()).unwrap_or(u64::MAX)
            > maximum_configuration_lookup_filter_bytes()
        {
            return Err(ConfigurationFailure::FilterTooLong);
        }
        Ok(filter)
    }
}

impl TryFrom<String> for OpenServiceGatewayInitiativePersistentIdentifier {
    type Error = ConfigurationFailure;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        Self::new(value)
    }
}

impl From<OpenServiceGatewayInitiativePersistentIdentifier> for String {
    fn from(identifier: OpenServiceGatewayInitiativePersistentIdentifier) -> Self {
        identifier.value
    }
}

/// One key of one configuration's property dictionary.
///
/// Independent of the JCR property name: a configuration key is a Java string
/// with its own alphabet, and borrowing the repository's rules would refuse
/// keys the framework itself supplies, `service.pid` among them.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct OpenServiceGatewayInitiativeConfigurationPropertyKey {
    /// The key, in its original case.
    value: String,
}

impl OpenServiceGatewayInitiativeConfigurationPropertyKey {
    /// Returns the key `spelling` names.
    ///
    /// # Errors
    ///
    /// Returns [`ConfigurationFailure::KeyOutOfBounds`] when empty, over bound,
    /// or carrying a control character.
    pub fn new(spelling: impl Into<String>) -> Result<Self, ConfigurationFailure> {
        let value = spelling.into();
        let bounded = !value.is_empty()
            && u64::try_from(value.len()).unwrap_or(u64::MAX)
                <= maximum_configuration_property_key_bytes()
            && !value.chars().any(char::is_control);
        if bounded { Ok(Self { value }) } else { Err(ConfigurationFailure::KeyOutOfBounds) }
    }

    /// Returns the key in its original case.
    #[must_use]
    pub fn as_text(&self) -> &str {
        &self.value
    }

    /// Returns the spelling two keys are the same key when they share.
    ///
    /// # Divergence from the specified identity
    ///
    /// The contract calls for Unicode default case folding followed by
    /// normalization form C. What is implemented is Unicode *lowercase mapping*
    /// followed by form C, because no full-case-folding table is available to
    /// this build. They agree on every ASCII, Latin, Greek, and Cyrillic
    /// spelling and differ where a full fold expands, U+00DF and the ligatures
    /// among them: there this identity separates two keys full folding joins,
    /// so it accepts a pair the specification refuses and never the reverse.
    ///
    /// The redaction policy does not use this: it folds ASCII only, by
    /// specification, so nothing about which values are read depends on the
    /// divergence.
    #[must_use]
    pub fn folded_identity(&self) -> String {
        self.value.to_lowercase().nfc().collect()
    }

    /// Returns whether this key reads like a secret.
    ///
    /// ASCII lowercasing and separator removal only, with no Unicode folding,
    /// because the policy has to be predictable to the person naming the
    /// property. An empty reduction is sensitive: a name with nothing ordinary
    /// left in it is not a name this contract will read a value under.
    #[must_use]
    pub fn reads_as_sensitive(&self) -> bool {
        let reduced: String = self
            .value
            .chars()
            .filter(|character| character.is_ascii_alphanumeric())
            .map(|character| character.to_ascii_lowercase())
            .collect();
        reduced.is_empty()
            || SENSITIVE_NAME_LITERALS.iter().any(|literal| reduced.contains(literal))
    }
}

impl TryFrom<String> for OpenServiceGatewayInitiativeConfigurationPropertyKey {
    type Error = ConfigurationFailure;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        Self::new(value)
    }
}

impl From<OpenServiceGatewayInitiativeConfigurationPropertyKey> for String {
    fn from(key: OpenServiceGatewayInitiativeConfigurationPropertyKey) -> Self {
        key.value
    }
}

/// One Java value, with the exact class it had.
///
/// Nothing widens. A `Byte` holding 1 and an `Integer` holding 1 are different
/// observations, because the configuration declared different things and a
/// consumer writing the value back needs the class it started with.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(untagged)]
pub enum OpenServiceGatewayInitiativeConfigurationScalar {
    /// A `java.lang.String`.
    Text(String),
    /// A `java.lang.Boolean`.
    Boolean(bool),
    /// A `java.lang.Character`, as one scalar value.
    Character(char),
    /// A `java.lang.Byte`, `Short`, `Integer`, or `Long`, in base ten.
    Integer(String),
    /// A `java.lang.Float` or `Double`, as its bits.
    Floating(String),
}

impl OpenServiceGatewayInitiativeConfigurationScalar {
    /// Returns the scalar of `type_name` that `value` writes.
    ///
    /// # Errors
    ///
    /// Returns [`ConfigurationFailure::UnknownScalarType`] for a class this
    /// contract does not convert, and the exact shape failure for a value that
    /// does not fit the class it declared.
    pub fn read(type_name: &str, value: &serde_json::Value) -> Result<Self, ConfigurationFailure> {
        let mismatch = || ConfigurationFailure::TypeMismatch;
        let spelling = || value.as_str().ok_or_else(mismatch);
        match type_name {
            "string" => {
                let text = spelling()?;
                if u64::try_from(text.len()).unwrap_or(u64::MAX)
                    > maximum_configuration_scalar_string_bytes()
                {
                    return Err(ConfigurationFailure::StringTooLong);
                }
                Ok(Self::Text(text.to_owned()))
            }
            "boolean" => value.as_bool().map(Self::Boolean).ok_or_else(mismatch),
            "character" => read_character(spelling()?),
            "byte" | "short" | "integer" | "long" => read_integer(type_name, spelling()?),
            "float" => read_floating(spelling()?, BINARY32_DIGITS),
            "double" => read_floating(spelling()?, BINARY64_DIGITS),
            _ => Err(ConfigurationFailure::UnknownScalarType),
        }
    }

    /// Returns whether this scalar can stand for `type_name`.
    #[must_use]
    pub fn suits(&self, type_name: &str) -> bool {
        matches!(
            (self, type_name),
            (Self::Text(_), "string")
                | (Self::Boolean(_), "boolean")
                | (Self::Character(_), "character")
                | (Self::Integer(_), "byte" | "short" | "integer" | "long")
                | (Self::Floating(_), "float" | "double")
        )
    }
}

/// Reads one character, which is exactly one scalar value.
fn read_character(
    spelling: &str,
) -> Result<OpenServiceGatewayInitiativeConfigurationScalar, ConfigurationFailure> {
    let mut characters = spelling.chars();
    let (Some(character), None) = (characters.next(), characters.next()) else {
        return Err(ConfigurationFailure::NotOneScalar);
    };
    Ok(OpenServiceGatewayInitiativeConfigurationScalar::Character(character))
}

/// Returns whether `spelling` is the one minimal spelling of its integer.
fn is_minimal_integer(spelling: &str) -> bool {
    let digits = spelling.strip_prefix('-').unwrap_or(spelling);
    !digits.is_empty()
        && digits.bytes().all(|byte| byte.is_ascii_digit())
        && (digits == "0" || !digits.starts_with('0'))
        && !(digits == "0" && spelling.starts_with('-'))
}

/// Returns whether `value` fits the exact range of the Java class `type_name`.
fn fits_integer_class(type_name: &str, value: i64) -> bool {
    match type_name {
        "byte" => i8::try_from(value).is_ok(),
        "short" => i16::try_from(value).is_ok(),
        "integer" => i32::try_from(value).is_ok(),
        _ => true,
    }
}

/// Reads one integer in the exact range of the class it declared.
fn read_integer(
    type_name: &str,
    spelling: &str,
) -> Result<OpenServiceGatewayInitiativeConfigurationScalar, ConfigurationFailure> {
    if !is_minimal_integer(spelling) {
        return Err(ConfigurationFailure::TypeMismatch);
    }
    let value = spelling.parse::<i64>().map_err(|_| ConfigurationFailure::IntegerOutOfRange)?;
    if !fits_integer_class(type_name, value) {
        return Err(ConfigurationFailure::IntegerOutOfRange);
    }
    Ok(OpenServiceGatewayInitiativeConfigurationScalar::Integer(spelling.to_owned()))
}

/// Reads one floating value as the exact bits it was retrieved with.
fn read_floating(
    spelling: &str,
    digits: usize,
) -> Result<OpenServiceGatewayInitiativeConfigurationScalar, ConfigurationFailure> {
    let canonical = spelling.len() == digits
        && spelling
            .chars()
            .all(|character| character.is_ascii_hexdigit() && !character.is_ascii_uppercase());
    if !canonical {
        return Err(ConfigurationFailure::NotBitString);
    }
    u64::from_str_radix(spelling, HEXADECIMAL_RADIX)
        .map_err(|_| ConfigurationFailure::NotBitString)?;
    Ok(OpenServiceGatewayInitiativeConfigurationScalar::Floating(spelling.to_owned()))
}

/// How one configuration value was carried.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ConfigurationCardinality {
    /// One value.
    Scalar,
    /// A Java primitive array.
    PrimitiveArray,
    /// A Java wrapper or string array.
    ScalarArray,
    /// A Configuration Admin collection.
    Collection,
}

impl ConfigurationCardinality {
    /// Returns the wire spelling of this carrier.
    #[must_use]
    pub fn as_text(self) -> &'static str {
        match self {
            Self::Scalar => "scalar",
            Self::PrimitiveArray => "primitive_array",
            Self::ScalarArray => "scalar_array",
            Self::Collection => "collection",
        }
    }

    /// Returns whether this carrier holds a sequence rather than one value.
    #[must_use]
    pub fn is_sequence(self) -> bool {
        !matches!(self, Self::Scalar)
    }
}

/// One configuration value, with its class and its carrier.
///
/// The carrier is kept because it is not decoration: writing a value back needs
/// to know whether the framework handed over a primitive array, a wrapper
/// array, or a collection, and those are three different things to construct.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OpenServiceGatewayInitiativeConfigurationValue {
    /// Which of the nine Java classes the items have.
    type_name: String,
    /// How they were carried.
    cardinality: ConfigurationCardinality,
    /// The items, in retrieved order.
    items: Vec<OpenServiceGatewayInitiativeConfigurationScalar>,
}

impl OpenServiceGatewayInitiativeConfigurationValue {
    /// Returns the value `items` carry under `type_name` and `cardinality`.
    ///
    /// A sequence may be empty: the type is stated rather than deduced from an
    /// item that would have to exist to be inspected. An empty collection is
    /// the one exception the agent must resolve before it gets here, because a
    /// collection carries no runtime component class to read the type from.
    ///
    /// # Errors
    ///
    /// Returns [`ConfigurationFailure::UnknownScalarType`] for a class this
    /// contract does not convert, [`ConfigurationFailure::CarrierTypeMismatch`]
    /// for a primitive array of strings, and the count, byte, shape, and
    /// homogeneity failures for everything else.
    pub fn new(
        type_name: &str,
        cardinality: ConfigurationCardinality,
        items: Vec<OpenServiceGatewayInitiativeConfigurationScalar>,
    ) -> Result<Self, ConfigurationFailure> {
        if !DECLARED_SCALAR_TYPES.contains(&type_name) {
            return Err(ConfigurationFailure::UnknownScalarType);
        }
        if cardinality == ConfigurationCardinality::PrimitiveArray && type_name == "string" {
            return Err(ConfigurationFailure::CarrierTypeMismatch);
        }
        if !cardinality.is_sequence() && items.len() != 1 {
            return Err(ConfigurationFailure::TypeMismatch);
        }
        if u64::try_from(items.len()).unwrap_or(u64::MAX) > maximum_configuration_sequence_items() {
            return Err(ConfigurationFailure::SequenceTooManyItems);
        }
        if !items.iter().all(|item| item.suits(type_name)) {
            return Err(ConfigurationFailure::MixedTypes);
        }
        let value = Self { type_name: type_name.to_owned(), cardinality, items };
        value.require_bounded_sequence()?;
        Ok(value)
    }

    /// Requires the canonical sequence to fit its named bound.
    fn require_bounded_sequence(&self) -> Result<(), ConfigurationFailure> {
        if !self.cardinality.is_sequence() {
            return Ok(());
        }
        let written = serde_json::to_string(&self.items)
            .map_err(|_| ConfigurationFailure::SequenceTooLong)?;
        if u64::try_from(written.len()).unwrap_or(u64::MAX)
            > maximum_configuration_sequence_canonical_bytes()
        {
            return Err(ConfigurationFailure::SequenceTooLong);
        }
        Ok(())
    }

    /// Returns which of the nine Java classes the items have.
    #[must_use]
    pub fn type_name(&self) -> &str {
        &self.type_name
    }

    /// Returns how the items were carried.
    #[must_use]
    pub fn cardinality(&self) -> ConfigurationCardinality {
        self.cardinality
    }

    /// Returns the items, in retrieved order.
    #[must_use]
    pub fn items(&self) -> &[OpenServiceGatewayInitiativeConfigurationScalar] {
        &self.items
    }
}

/// One value exactly as it is written on the wire.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct ValueDocument {
    /// How the items were carried.
    cardinality: String,
    /// Which of the nine Java classes the items have.
    #[serde(rename = "type")]
    type_name: String,
    /// The single value, on a scalar alone.
    #[serde(default)]
    value: Option<serde_json::Value>,
    /// The items, on a sequence alone.
    #[serde(default)]
    values: Option<Vec<serde_json::Value>>,
}

impl TryFrom<ValueDocument> for OpenServiceGatewayInitiativeConfigurationValue {
    type Error = ConfigurationFailure;

    fn try_from(document: ValueDocument) -> Result<Self, Self::Error> {
        let cardinality = match document.cardinality.as_str() {
            "scalar" => ConfigurationCardinality::Scalar,
            "primitive_array" => ConfigurationCardinality::PrimitiveArray,
            "scalar_array" => ConfigurationCardinality::ScalarArray,
            "collection" => ConfigurationCardinality::Collection,
            _ => return Err(ConfigurationFailure::UnknownCardinality),
        };
        let written = match (cardinality.is_sequence(), document.value, document.values) {
            (false, Some(value), None) => vec![value],
            (true, None, Some(values)) => values,
            _ => return Err(ConfigurationFailure::TypeMismatch),
        };
        let items = written
            .iter()
            .map(|value| {
                OpenServiceGatewayInitiativeConfigurationScalar::read(&document.type_name, value)
            })
            .collect::<Result<Vec<_>, _>>()?;
        Self::new(&document.type_name, cardinality, items)
    }
}

impl Serialize for OpenServiceGatewayInitiativeConfigurationValue {
    fn serialize<Target: serde::Serializer>(
        &self,
        serializer: Target,
    ) -> Result<Target::Ok, Target::Error> {
        use serde::ser::SerializeStruct;

        /// Members one value writes.
        const MEMBERS: usize = 3;

        let mut value = serializer
            .serialize_struct("OpenServiceGatewayInitiativeConfigurationValue", MEMBERS)?;
        value.serialize_field("type", &self.type_name)?;
        value.serialize_field("cardinality", self.cardinality.as_text())?;
        if self.cardinality.is_sequence() {
            value.serialize_field("values", &self.items)?;
        } else {
            value.serialize_field("value", &self.items[0])?;
        }
        value.end()
    }
}

impl<'de> Deserialize<'de> for OpenServiceGatewayInitiativeConfigurationValue {
    fn deserialize<Source: serde::Deserializer<'de>>(
        deserializer: Source,
    ) -> Result<Self, Source::Error> {
        let document = ValueDocument::deserialize(deserializer)?;
        Self::try_from(document).map_err(Source::Error::custom)
    }
}

/// What the Meta Type Service was able to say about one property.
///
/// `Unavailable` is the answer to every absent, ambiguous, duplicated, failed,
/// or unsupported observation, and it is deliberately content free: a caller
/// learns that no evidence was obtained and nothing about why, because the why
/// would describe the deployment.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MetatypeEvidence {
    /// The attribute is declared as a password.
    Password,
    /// The attribute is declared as one of the nine ordinary types.
    NonPassword,
    /// No usable evidence was obtained.
    Unavailable,
}

impl MetatypeEvidence {
    /// Returns whether this evidence alone would allow a value to be read.
    ///
    /// Only exact non-password evidence does. Absence of evidence is not
    /// evidence of harmlessness, so `Unavailable` redacts.
    #[must_use]
    pub fn permits_reading(self) -> bool {
        matches!(self, Self::NonPassword)
    }
}

/// What was observed about one property.
///
/// The redacted shape carries nothing but its own name. It is closed by the
/// reader below rather than by a derive, because an internally tagged enum
/// ignores members it does not know - which would silently accept a redaction
/// with a value attached to it, the one thing this contract exists to prevent.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "visibility", rename_all = "snake_case")]
pub enum PropertyObservation {
    /// The value was not read.
    Redacted,
    /// The value was read, once.
    Visible {
        /// What the property holds.
        value: OpenServiceGatewayInitiativeConfigurationValue,
    },
}

/// One property of one configuration, with the evidence that decided it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ObservedConfigurationProperty {
    /// What the Meta Type Service said.
    metatype_evidence: MetatypeEvidence,
    /// What was observed.
    observation: PropertyObservation,
}

impl ObservedConfigurationProperty {
    /// Returns the observation for a property that was not read.
    #[must_use]
    pub fn redacted(metatype_evidence: MetatypeEvidence) -> Self {
        Self { metatype_evidence, observation: PropertyObservation::Redacted }
    }

    /// Returns the observation for a property that was read.
    ///
    /// # Errors
    ///
    /// Returns [`ConfigurationFailure::EvidenceDoesNotPermitVisibility`] when
    /// the evidence is anything but exact non-password evidence.
    pub fn visible(
        metatype_evidence: MetatypeEvidence,
        value: OpenServiceGatewayInitiativeConfigurationValue,
    ) -> Result<Self, ConfigurationFailure> {
        if !metatype_evidence.permits_reading() {
            return Err(ConfigurationFailure::EvidenceDoesNotPermitVisibility);
        }
        Ok(Self { metatype_evidence, observation: PropertyObservation::Visible { value } })
    }

    /// Returns what the Meta Type Service said.
    #[must_use]
    pub fn metatype_evidence(&self) -> MetatypeEvidence {
        self.metatype_evidence
    }

    /// Returns what was observed.
    #[must_use]
    pub fn observation(&self) -> &PropertyObservation {
        &self.observation
    }

    /// Returns the observation `key` and `evidence` call for, before the value
    /// exists.
    ///
    /// This is the whole policy in one place: a password attribute, missing
    /// evidence, or a name that reads like a secret all answer `None`, meaning
    /// the value is never fetched. Only an ordinary attribute under an ordinary
    /// name answers `Some`, and the caller then reads exactly once.
    #[must_use]
    pub fn decide_before_reading(
        key: &OpenServiceGatewayInitiativeConfigurationPropertyKey,
        evidence: MetatypeEvidence,
    ) -> Option<MetatypeEvidence> {
        if evidence.permits_reading() && !key.reads_as_sensitive() { Some(evidence) } else { None }
    }
}

/// One observation exactly as it is written on the wire.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct ObservationDocument {
    metatype_evidence: MetatypeEvidence,
    observation: VisibilityDocument,
}

/// Whether a value was read, exactly as it is written on the wire.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "snake_case")]
enum Visibility {
    Redacted,
    Visible,
}

/// One visibility exactly as it is written on the wire.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct VisibilityDocument {
    visibility: Visibility,
    /// What the property held, on a visible observation alone.
    #[serde(default)]
    value: Option<OpenServiceGatewayInitiativeConfigurationValue>,
}

impl<'de> Deserialize<'de> for ObservedConfigurationProperty {
    fn deserialize<Source: serde::Deserializer<'de>>(
        deserializer: Source,
    ) -> Result<Self, Source::Error> {
        let document = ObservationDocument::deserialize(deserializer)?;
        match (document.observation.visibility, document.observation.value) {
            (Visibility::Redacted, None) => Ok(Self::redacted(document.metatype_evidence)),
            (Visibility::Visible, Some(value)) => {
                Self::visible(document.metatype_evidence, value).map_err(Source::Error::custom)
            }
            _ => Err(Source::Error::custom(ConfigurationFailure::RedactionNotContentFree)),
        }
    }
}

/// One request to inspect a configuration.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct InspectOpenServiceGatewayInitiativeConfigurationCommand {
    /// Configuration to inspect.
    pub persistent_identifier: OpenServiceGatewayInitiativePersistentIdentifier,
}

/// What an inspection found.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct InspectOpenServiceGatewayInitiativeConfigurationResult {
    /// Configuration that was asked about.
    pub persistent_identifier: OpenServiceGatewayInitiativePersistentIdentifier,
    /// Whether that exact configuration exists.
    pub present: bool,
    /// What it holds, by ascending key bytes.
    pub properties: BTreeMap<String, ObservedConfigurationProperty>,
}

impl InspectOpenServiceGatewayInitiativeConfigurationResult {
    /// Returns the result `properties` describe.
    ///
    /// # Errors
    ///
    /// Returns [`ConfigurationFailure::AbsentWithProperties`] when an absent
    /// configuration reports properties anyway, and
    /// [`ConfigurationFailure::TooManyProperties`] above the named bound.
    pub fn new(
        persistent_identifier: OpenServiceGatewayInitiativePersistentIdentifier,
        present: bool,
        properties: BTreeMap<String, ObservedConfigurationProperty>,
    ) -> Result<Self, ConfigurationFailure> {
        if !present && !properties.is_empty() {
            return Err(ConfigurationFailure::AbsentWithProperties);
        }
        if u64::try_from(properties.len()).unwrap_or(u64::MAX)
            > maximum_inspected_configuration_properties()
        {
            return Err(ConfigurationFailure::TooManyProperties);
        }
        Ok(Self { persistent_identifier, present, properties })
    }

    /// Requires this result to answer `command`.
    ///
    /// # Errors
    ///
    /// Returns [`ConfigurationFailure::NotThisRequest`] when the echoed
    /// identifier is another request's.
    pub fn require_answers(
        &self,
        command: &InspectOpenServiceGatewayInitiativeConfigurationCommand,
    ) -> Result<(), ConfigurationFailure> {
        if self.persistent_identifier == command.persistent_identifier {
            Ok(())
        } else {
            Err(ConfigurationFailure::NotThisRequest)
        }
    }
}

/// One result exactly as it is written on the wire.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct ResultDocument {
    persistent_identifier: OpenServiceGatewayInitiativePersistentIdentifier,
    present: bool,
    #[serde(default)]
    properties: BTreeMap<String, ObservedConfigurationProperty>,
}

impl<'de> Deserialize<'de> for InspectOpenServiceGatewayInitiativeConfigurationResult {
    fn deserialize<Source: serde::Deserializer<'de>>(
        deserializer: Source,
    ) -> Result<Self, Source::Error> {
        let document = ResultDocument::deserialize(deserializer)?;
        Self::new(document.persistent_identifier, document.present, document.properties)
            .map_err(Source::Error::custom)
    }
}

/// Which budget a lookup ran out of.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LookupBudget {
    /// Time elapsed since the lookup began.
    LookupDuration,
    /// Configurations the filter matched.
    MatchingConfigurations,
}

/// Why a value could not be represented.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum UnsupportedValueReason {
    /// The Java class is not one of the nine.
    NonPrimaryType,
    /// The value is not valid Unicode.
    NonUnicodeValue,
}

/// Why a key or value was malformed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MalformedValueReason {
    /// A key is not a key this contract accepts.
    InvalidPropertyKey,
    /// Two keys are the same key.
    DuplicatePropertyKey,
    /// A value or item was null.
    NullValue,
    /// A sequence mixed exact classes.
    MixedTypes,
    /// A sequence contained another sequence.
    NestedContainer,
    /// The carrier and the class disagree.
    CarrierTypeMismatch,
    /// An empty collection has no type and no evidence to give it one.
    EmptyCollectionTypeUnavailable,
}

/// Which value budget was exhausted.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ValueBudget {
    /// Bytes one string carried.
    ScalarStringBytes,
    /// Items one sequence carried.
    SequenceItems,
    /// Bytes one canonical sequence reached.
    SequenceCanonicalBytes,
}

/// Which result budget was exhausted.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ResultBudget {
    /// Properties the configuration held.
    PropertyCount,
    /// Bytes the canonical result reached.
    SerializedResultBytes,
}

/// Why an inspection produced no result.
///
/// Every one of these is closed and none carries a key, a value, a filter, a
/// persistent identifier, a configuration object, or a partial map. A failure
/// that named the key it failed on would be a channel for exactly the thing the
/// redaction policy exists to withhold.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "failure", rename_all = "snake_case", deny_unknown_fields)]
pub enum ConfigurationRefusal {
    /// A Configuration Admin call failed or returned nothing usable.
    ConfigurationLookupFailed,
    /// A returned configuration's identifier was not the one asked for.
    ConfigurationLookupMismatch,
    /// Two configurations carry the exact identifier asked for.
    ConfigurationLookupAmbiguous,
    /// The lookup ran out of one of its budgets.
    ConfigurationLookupBudgetExceeded {
        /// Budget that ran out.
        budget: LookupBudget,
    },
    /// A value is outside what this contract represents.
    ConfigurationValueUnsupported {
        /// Why it is outside.
        reason: UnsupportedValueReason,
    },
    /// A key or value is not the shape this contract requires.
    ConfigurationValueMalformed {
        /// Which shape was wrong.
        reason: MalformedValueReason,
    },
    /// One value ran out of one of its budgets.
    ConfigurationValueBudgetExceeded {
        /// Budget that ran out.
        budget: ValueBudget,
    },
    /// The complete result ran out of one of its budgets.
    ConfigurationResultBudgetExceeded {
        /// Budget that ran out.
        budget: ResultBudget,
    },
}
