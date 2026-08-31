//! What a content fragment holds, and the variation it holds it in.
//!
//! Three of the four fragment commands carry the same thing: a set of element
//! names, each holding either one text value or an ordered list of them. Writing
//! that three times would give the create and the update three chances to
//! disagree about what an element is, so it is written once.
//!
//! # A single value is not a list of one
//!
//! The two forms are closed alternatives and neither is rewritten as the other.
//! A model declares an element as single-valued or multi-valued, and a request
//! that sent one where the model wanted the other is a request the author should
//! refuse rather than one this contract should quietly reshape.
//!
//! # An absent variation is the master variation
//!
//! Every fragment has a master variation and may have others. A command that
//! names no variation means the master, and that is said here, once, rather than
//! in each of the three commands that take the name.

use serde::de::Error as _;
use serde::{Deserialize, Serialize};

use crate::command::command_identity::CommandContract;
use crate::command::repository_path::{PathFailure, accept_within, address_value};

/// Why an element document is not one this contract can carry.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum ContentFragmentFailure {
    /// A document names more elements than the contract allows.
    #[error("an element document is within the element bound its contract declares")]
    TooManyElements,
    /// A list holds more values than the contract allows.
    #[error("an element list is within the value bound its contract declares")]
    TooManyValues,
    /// A list holds no values at all.
    #[error("an element list holds at least one value")]
    EmptyList,
    /// A value is longer than the contract allows.
    #[error("an element value is within the byte bound its contract declares")]
    ValueTooLong,
    /// A result does not answer the command it claims to answer.
    #[error("a fragment result names the fragment and variation its command asked about")]
    NotThisRequest,
    /// The request would change nothing.
    #[error("a fragment update changes something")]
    ChangesNothing,
}

address_value!(
    /// The name one element of a content fragment is addressed by.
    ContentFragmentElementName,
    "content fragment element name"
);

address_value!(
    /// The name one variation of a content fragment is addressed by.
    ContentFragmentVariationName,
    "content fragment variation name"
);

impl ContentFragmentElementName {
    /// Validates one element name.
    ///
    /// # Errors
    ///
    /// Returns [`PathFailure`] when the name is empty, longer than the contract
    /// allows, not already in normalization form C, carries a separator or a
    /// control, or has a leading or trailing ASCII space.
    pub fn parse(name: &str) -> Result<Self, PathFailure> {
        let bound =
            CommandContract::embedded().limit("maximum_content_fragment_element_name_bytes");
        accept_within(name, bound, Self::role(), "bytes")?;
        accept_addressable(name, Self::role())?;
        Ok(Self::from_accepted(name))
    }
}

impl ContentFragmentVariationName {
    /// Validates one variation name.
    ///
    /// # Errors
    ///
    /// Returns [`PathFailure`] when the name is empty, longer than the contract
    /// allows, not already in normalization form C, carries a separator or a
    /// control, or has a leading or trailing ASCII space.
    pub fn parse(name: &str) -> Result<Self, PathFailure> {
        let bound =
            CommandContract::embedded().limit("maximum_content_fragment_variation_name_bytes");
        accept_within(name, bound, Self::role(), "bytes")?;
        accept_addressable(name, Self::role())?;
        Ok(Self::from_accepted(name))
    }
}

/// What one element of a content fragment holds.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(untagged)]
pub enum ContentFragmentElementValue {
    /// One value.
    Single(String),
    /// An ordered list of values, which is never a list of one by accident.
    List(Vec<String>),
}

impl ContentFragmentElementValue {
    /// Returns the single value `value` describes.
    ///
    /// # Errors
    ///
    /// Returns [`ContentFragmentFailure::ValueTooLong`] above the contract's
    /// value bound.
    pub fn single(value: &str) -> Result<Self, ContentFragmentFailure> {
        require_value_within(value)?;
        Ok(Self::Single(value.to_owned()))
    }

    /// Returns the list `values` describes.
    ///
    /// # Errors
    ///
    /// Returns [`ContentFragmentFailure::EmptyList`] for an empty list,
    /// [`ContentFragmentFailure::TooManyValues`] above the contract's item
    /// bound, and [`ContentFragmentFailure::ValueTooLong`] above its value
    /// bound.
    pub fn list(values: Vec<String>) -> Result<Self, ContentFragmentFailure> {
        let bound = CommandContract::embedded().limit("maximum_content_fragment_element_values");
        if values.is_empty() {
            return Err(ContentFragmentFailure::EmptyList);
        }
        if u64::try_from(values.len()).unwrap_or(u64::MAX) > bound {
            return Err(ContentFragmentFailure::TooManyValues);
        }
        for value in &values {
            require_value_within(value)?;
        }
        Ok(Self::List(values))
    }
}

/// One element value exactly as it is written on the wire.
#[derive(Debug, Deserialize)]
#[serde(untagged)]
enum ValueDocument {
    /// One value.
    Single(String),
    /// An ordered list of values.
    List(Vec<String>),
}

impl<'de> Deserialize<'de> for ContentFragmentElementValue {
    fn deserialize<Source: serde::Deserializer<'de>>(
        deserializer: Source,
    ) -> Result<Self, Source::Error> {
        match ValueDocument::deserialize(deserializer)? {
            ValueDocument::Single(value) => Self::single(&value),
            ValueDocument::List(values) => Self::list(values),
        }
        .map_err(Source::Error::custom)
    }
}

/// The elements one request assigns, by name.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(transparent)]
pub struct ContentFragmentElementValues {
    /// The elements, by name.
    values: std::collections::BTreeMap<ContentFragmentElementName, ContentFragmentElementValue>,
}

impl ContentFragmentElementValues {
    /// Returns the elements `values` describes.
    ///
    /// # Errors
    ///
    /// Returns [`ContentFragmentFailure::TooManyElements`] above the contract's
    /// element bound.
    pub fn new(
        values: std::collections::BTreeMap<ContentFragmentElementName, ContentFragmentElementValue>,
    ) -> Result<Self, ContentFragmentFailure> {
        let bound = CommandContract::embedded().limit("maximum_content_fragment_elements");
        if u64::try_from(values.len()).unwrap_or(u64::MAX) > bound {
            return Err(ContentFragmentFailure::TooManyElements);
        }
        Ok(Self { values })
    }

    /// Returns the elements, by name.
    #[must_use]
    pub fn values(
        &self,
    ) -> &std::collections::BTreeMap<ContentFragmentElementName, ContentFragmentElementValue> {
        &self.values
    }

    /// Reports whether this document assigns nothing.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.values.is_empty()
    }
}

impl<'de> Deserialize<'de> for ContentFragmentElementValues {
    fn deserialize<Source: serde::Deserializer<'de>>(
        deserializer: Source,
    ) -> Result<Self, Source::Error> {
        let values = std::collections::BTreeMap::<
            ContentFragmentElementName,
            ContentFragmentElementValue,
        >::deserialize(deserializer)?;
        Self::new(values).map_err(Source::Error::custom)
    }
}

/// Requires one element value to be within the contract's byte bound.
fn require_value_within(value: &str) -> Result<(), ContentFragmentFailure> {
    let bound = CommandContract::embedded().limit("maximum_property_string_bytes");
    if u64::try_from(value.len()).unwrap_or(u64::MAX) > bound {
        return Err(ContentFragmentFailure::ValueTooLong);
    }
    Ok(())
}

/// Requires one fragment name to be addressable.
fn accept_addressable(name: &str, role: &'static str) -> Result<(), PathFailure> {
    let refuse = |field| PathFailure::at(role, field);
    if name.starts_with(' ') || name.ends_with(' ') {
        return Err(refuse("space"));
    }
    if name.chars().any(|character| character == '/' || character.is_control()) {
        return Err(refuse("character"));
    }
    Ok(())
}
