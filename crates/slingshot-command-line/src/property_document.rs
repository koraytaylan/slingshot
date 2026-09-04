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
use std::io::Read;

use serde_json::Value;
use slingshot_domain::command::command_identity::CommandContract;
use slingshot_domain::command::create_page::maximum_mutation_properties;
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
    /// The document exceeds the command argument byte bound.
    #[error("a property document is at most {maximum} bytes")]
    TooLarge {
        /// The canonical command-argument byte bound.
        maximum: usize,
    },
    /// It is not one JSON object.
    #[error("a property document is one canonical JSON object, and this is not")]
    NotAnObject,
    /// It nests deeper than the vocabulary has meaning for.
    #[error("a property document nests at most {MAXIMUM_DEPTH} deep, and this nests further")]
    TooDeep,
    /// An object gives one semantic member name more than once.
    #[error("{named} is given twice")]
    DuplicateMember {
        /// The decoded member name that has no unambiguous value.
        named: String,
    },
    /// The document names more properties than one mutation admits.
    #[error("a mutation carries at most {maximum} properties")]
    TooManyProperties {
        /// The canonical maximum property count.
        maximum: u64,
    },
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
    require_unique_members(text)?;
    let document: Value =
        serde_json::from_str(text).map_err(|_| PropertyDocumentRefusal::NotAnObject)?;
    require_bounded_depth(&document, MAXIMUM_DEPTH)?;
    let object = document.as_object().ok_or(PropertyDocumentRefusal::NotAnObject)?;
    object.iter().map(|(named, entry)| Ok((named.clone(), property(entry)?))).collect()
}

/// Rejects duplicate decoded member names before a map can choose a winner.
fn require_unique_members(text: &str) -> Result<(), PropertyDocumentRefusal> {
    let mut objects: Vec<BTreeMap<String, ()>> = Vec::new();
    let mut depth = 0_usize;
    let mut outer_properties = 0_u64;
    let mut raw = None::<String>;
    let mut escaped = false;
    let mut held = None::<String>;
    for character in text.chars() {
        if let Some(reading) = raw.as_mut() {
            if escaped {
                reading.push(character);
                escaped = false;
            } else {
                match character {
                    '\\' => {
                        reading.push(character);
                        escaped = true;
                    }
                    '"' => held = raw.take(),
                    other => reading.push(other),
                }
            }
            continue;
        }
        match character {
            '"' => raw = Some(String::new()),
            '{' => {
                depth = depth.saturating_add(1);
                if depth > MAXIMUM_DEPTH {
                    return Err(PropertyDocumentRefusal::TooDeep);
                }
                objects.push(BTreeMap::new());
            }
            '[' => {
                depth = depth.saturating_add(1);
                if depth > MAXIMUM_DEPTH {
                    return Err(PropertyDocumentRefusal::TooDeep);
                }
            }
            '}' => {
                objects.pop();
                depth = depth.saturating_sub(1);
                held = None;
            }
            ']' => depth = depth.saturating_sub(1),
            ':' => {
                let Some(raw_name) = held.take() else { continue };
                let name = serde_json::from_str::<String>(&format!("\"{raw_name}\""))
                    .map_err(|_| PropertyDocumentRefusal::NotAnObject)?;
                let Some(members) = objects.last_mut() else { continue };
                if members.insert(name.clone(), ()).is_some() {
                    return Err(PropertyDocumentRefusal::DuplicateMember { named: name });
                }
                if objects.len() == 1 {
                    outer_properties = outer_properties.saturating_add(1);
                    let maximum = maximum_mutation_properties();
                    if outer_properties > maximum {
                        return Err(PropertyDocumentRefusal::TooManyProperties { maximum });
                    }
                }
            }
            ',' => held = None,
            _ => {}
        }
    }
    Ok(())
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
    let maximum = maximum_document_bytes();
    let metadata = std::fs::metadata(path).map_err(|_| PropertyDocumentRefusal::Unreadable)?;
    if metadata.len() > u64::try_from(maximum).unwrap_or(u64::MAX) {
        return Err(PropertyDocumentRefusal::TooLarge { maximum });
    }
    let mut file = std::fs::File::open(path).map_err(|_| PropertyDocumentRefusal::Unreadable)?;
    let mut bytes = Vec::with_capacity(maximum.saturating_add(1));
    file.by_ref()
        .take(u64::try_from(maximum.saturating_add(1)).unwrap_or(u64::MAX))
        .read_to_end(&mut bytes)
        .map_err(|_| PropertyDocumentRefusal::Unreadable)?;
    if bytes.len() > maximum {
        return Err(PropertyDocumentRefusal::TooLarge { maximum });
    }
    let text = core::str::from_utf8(&bytes).map_err(|_| PropertyDocumentRefusal::NotAnObject)?;
    parse(text)
}

/// Returns the canonical maximum size of one command argument document.
fn maximum_document_bytes() -> usize {
    usize::try_from(CommandContract::embedded().limit("maximum_command_argument_bytes"))
        .unwrap_or(usize::MAX)
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
