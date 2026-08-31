//! Reading a property document a caller supplies for a mutation.
//!
//! One canonical JSON object of typed property values, read from a file so a
//! caller is not asked to quote a document through a shell. Nothing here
//! templates, interpolates, or expands: what the file holds is what the request
//! carries, because a surface that rewrote a value would be writing content the
//! caller never approved.
//!
//! The bounds are checked before anything is built. A document larger or deeper
//! than a mutation admits is refused as a document rather than discovered
//! halfway through constructing a request, and a repeated key is refused rather
//! than resolved - two values under one name have no correct winner.

use std::collections::BTreeMap;

use serde_json::Value;
use slingshot_domain::command::property_value::{
    BOOLEAN_TYPE, DATE_TIME_TYPE, DECIMAL_TYPE, DateTimeString, DecimalString, INTEGER_TYPE,
    PropertyScalarValue, PropertyValue, REPOSITORY_PATH_TYPE, STRING_TYPE,
};
use slingshot_domain::command::repository_path::RepositoryPropertyPath;

/// The member naming what type a value is.
pub const TYPE_MEMBER: &str = "type";

/// The member carrying one value.
pub const VALUE_MEMBER: &str = "value";

/// The member carrying several.
pub const VALUES_MEMBER: &str = "values";

/// Every member one property entry may carry.
pub const EVERY_MEMBER: &[&str] = &[TYPE_MEMBER, VALUE_MEMBER, VALUES_MEMBER];

/// How deep a property document may nest.
///
/// One object of entries, each an object of members, one of which may be an
/// array of scalars. Anything deeper is a structure this vocabulary has no
/// meaning for, and accepting it would mean deciding what it meant.
pub const MAXIMUM_DEPTH: usize = 4;

/// Why one property document is not a set of properties.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum PropertyDocumentRefusal {
    /// The file could not be read.
    #[error("the property document could not be read")]
    Unreadable,
    /// It is not one JSON object.
    #[error("a property document is one canonical JSON object, and this is not")]
    NotAnObject,
    /// It nests deeper than the vocabulary has meaning for.
    #[error("a property document nests at most {MAXIMUM_DEPTH} deep, and this nests further")]
    TooDeep,
    /// It carries a member this build does not know.
    #[error("{named} is not a member a property entry carries")]
    SurplusMember {
        /// Which member.
        named: String,
    },
    /// One entry omits a member it needs.
    #[error("{named} is required by this property and is not there")]
    MemberMissing {
        /// Which member.
        named: String,
    },
    /// A type is not one this build publishes.
    #[error("{named} is not a type this build publishes")]
    UnknownType {
        /// What was written.
        named: String,
    },
    /// A value is not one the domain accepts for that type.
    #[error("that is not a canonical {named}")]
    ValueUnusable {
        /// Which type it was supposed to be.
        named: String,
    },
    /// A multiple value carries nothing.
    #[error("a multiple property holds at least one value")]
    EmptyMultiple,
}

/// Returns the properties one document's bytes describe.
///
/// # Errors
///
/// Returns [`PropertyDocumentRefusal`] naming the first thing that is wrong.
pub fn parse(text: &str) -> Result<BTreeMap<String, PropertyValue>, PropertyDocumentRefusal> {
    let document: Value =
        serde_json::from_str(text).map_err(|_| PropertyDocumentRefusal::NotAnObject)?;
    require_bounded_depth(&document, MAXIMUM_DEPTH)?;
    let object = document.as_object().ok_or(PropertyDocumentRefusal::NotAnObject)?;
    object.iter().map(|(named, entry)| Ok((named.clone(), property(entry)?))).collect()
}

/// Returns the properties one file holds.
///
/// # Errors
///
/// Returns [`PropertyDocumentRefusal::Unreadable`] when the file cannot be
/// read, and whatever [`parse`] returns otherwise.
pub fn read(
    path: &std::path::Path,
) -> Result<BTreeMap<String, PropertyValue>, PropertyDocumentRefusal> {
    let text = std::fs::read_to_string(path).map_err(|_| PropertyDocumentRefusal::Unreadable)?;
    parse(&text)
}

/// Requires one document to nest no deeper than the vocabulary reaches.
fn require_bounded_depth(value: &Value, remaining: usize) -> Result<(), PropertyDocumentRefusal> {
    let Some(remaining) = remaining.checked_sub(1) else {
        return Err(PropertyDocumentRefusal::TooDeep);
    };
    match value {
        Value::Object(members) => {
            members.values().try_for_each(|member| require_bounded_depth(member, remaining))
        }
        Value::Array(members) => {
            members.iter().try_for_each(|member| require_bounded_depth(member, remaining))
        }
        _ => Ok(()),
    }
}

/// Returns the value one entry describes.
fn property(entry: &Value) -> Result<PropertyValue, PropertyDocumentRefusal> {
    let object = entry.as_object().ok_or(PropertyDocumentRefusal::NotAnObject)?;
    for named in object.keys() {
        if !EVERY_MEMBER.contains(&named.as_str()) {
            return Err(PropertyDocumentRefusal::SurplusMember { named: named.clone() });
        }
    }
    let named = object
        .get(TYPE_MEMBER)
        .and_then(Value::as_str)
        .ok_or_else(|| PropertyDocumentRefusal::MemberMissing { named: TYPE_MEMBER.to_owned() })?;
    if let Some(stated) = object.get(VALUES_MEMBER) {
        if object.contains_key(VALUE_MEMBER) {
            return Err(PropertyDocumentRefusal::SurplusMember { named: VALUE_MEMBER.to_owned() });
        }
        let members = stated.as_array().ok_or(PropertyDocumentRefusal::NotAnObject)?;
        if members.is_empty() {
            return Err(PropertyDocumentRefusal::EmptyMultiple);
        }
        let values = members
            .iter()
            .map(|member| scalar(named, member))
            .collect::<Result<Vec<PropertyScalarValue>, PropertyDocumentRefusal>>()?;
        return PropertyValue::multiple(values)
            .map_err(|_| PropertyDocumentRefusal::ValueUnusable { named: named.to_owned() });
    }
    let stated = object
        .get(VALUE_MEMBER)
        .ok_or_else(|| PropertyDocumentRefusal::MemberMissing { named: VALUE_MEMBER.to_owned() })?;
    Ok(PropertyValue::Single(scalar(named, stated)?))
}

/// Returns one scalar of the named type.
fn scalar(named: &str, stated: &Value) -> Result<PropertyScalarValue, PropertyDocumentRefusal> {
    let unusable = || PropertyDocumentRefusal::ValueUnusable { named: named.to_owned() };
    let text = || stated.as_str().ok_or_else(unusable);
    match named {
        STRING_TYPE => PropertyScalarValue::text(text()?).map_err(|_| unusable()),
        BOOLEAN_TYPE => Ok(PropertyScalarValue::Boolean(stated.as_bool().ok_or_else(unusable)?)),
        INTEGER_TYPE => PropertyScalarValue::integer(text()?).map_err(|_| unusable()),
        DECIMAL_TYPE => {
            DecimalString::new(text()?).map(PropertyScalarValue::Decimal).map_err(|_| unusable())
        }
        DATE_TIME_TYPE => {
            DateTimeString::new(text()?).map(PropertyScalarValue::DateTime).map_err(|_| unusable())
        }
        REPOSITORY_PATH_TYPE => RepositoryPropertyPath::parse(text()?)
            .map(PropertyScalarValue::Path)
            .map_err(|_| unusable()),
        other => Err(PropertyDocumentRefusal::UnknownType { named: other.to_owned() }),
    }
}
