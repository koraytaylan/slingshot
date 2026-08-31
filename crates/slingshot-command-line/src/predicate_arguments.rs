//! Turning command-line predicate arguments into typed questions.
//!
//! One option, given as many times as the caller has questions, each carrying a
//! canonical JSON object. The alternative - a bespoke flag grammar - would be a
//! second encoding of the same vocabulary, and the two would drift on exactly
//! the values where drifting matters: which operator takes a value, which take
//! several, and which types those values may be.
//!
//! So every spelling here comes from the domain rather than from this file. The
//! operator tags are the domain's declared list, the type tags are its own
//! constants, and a caller who writes one this build does not publish is told
//! which one they wrote rather than which one this surface happens to know.
//!
//! # What each operator may carry is checked here and not left to the agent
//!
//! Presence takes no value; equality takes one; membership takes a non-empty
//! homogeneous list; and the four comparisons take one ordered scalar. A
//! predicate that broke any of those would be a well-formed question the agent
//! could only answer with a refusal, and refusing it here costs a caller a
//! message instead of a round trip.

use serde_json::Value;
use slingshot_domain::command::property_value::{
    BOOLEAN_TYPE, DATE_TIME_TYPE, DECIMAL_TYPE, DateTimeString, DecimalString, INTEGER_TYPE,
    PropertyScalarValue, PropertyValue, REPOSITORY_PATH_TYPE, STRING_TYPE,
};
use slingshot_domain::command::repository_path::{RelativePropertyPath, RepositoryPropertyPath};
use slingshot_domain::command::search_predicate::{
    DECLARED_OPERATORS, EQUALS_OPERATOR, EXISTS_OPERATOR, GREATER_THAN_OPERATOR,
    GREATER_THAN_OR_EQUAL_OPERATOR, LESS_THAN_OPERATOR, LESS_THAN_OR_EQUAL_OPERATOR,
    LIST_CONTAINS_ALL_OPERATOR, LIST_CONTAINS_ANY_OPERATOR, MembershipValues, NOT_EQUALS_OPERATOR,
    OrderedScalarPropertyValue, PropertyPredicate, PropertyPredicates, SCALAR_IN_OPERATOR,
};

/// The option one predicate is given with.
pub const PREDICATE_OPTION: &str = "--property-predicate";

/// The member naming which property a predicate asks about.
pub const PROPERTY_PATH_MEMBER: &str = "property_path";

/// The member naming which question it asks.
pub const OPERATOR_MEMBER: &str = "operator";

/// The member naming what type a value is.
pub const TYPE_MEMBER: &str = "type";

/// The member carrying one value.
pub const VALUE_MEMBER: &str = "value";

/// The member carrying several.
pub const VALUES_MEMBER: &str = "values";

/// Every member a predicate object may carry.
pub const EVERY_MEMBER: &[&str] =
    &[PROPERTY_PATH_MEMBER, OPERATOR_MEMBER, TYPE_MEMBER, VALUE_MEMBER, VALUES_MEMBER];

/// Why one predicate argument is not a question.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum PredicateArgumentRefusal {
    /// The argument is not one JSON object.
    #[error("a predicate is one canonical JSON object, and this is not")]
    NotAnObject,
    /// It carries a member this build does not know.
    #[error("{named} is not a member a predicate carries")]
    SurplusMember {
        /// Which member.
        named: String,
    },
    /// It omits a member this operator needs.
    #[error("{named} is required by this predicate and is not there")]
    MemberMissing {
        /// Which member.
        named: String,
    },
    /// The operator is not one this build publishes.
    #[error("{named} is not an operator this build publishes")]
    UnknownOperator {
        /// What was written.
        named: String,
    },
    /// The type is not one this build publishes.
    #[error("{named} is not a type this build publishes")]
    UnknownType {
        /// What was written.
        named: String,
    },
    /// The operator was given a value it does not take.
    #[error("this operator takes no value, and one was given")]
    ValueNotTaken,
    /// The value is not one the domain accepts for that type.
    #[error("that is not a canonical {named}")]
    ValueUnusable {
        /// Which type it was supposed to be.
        named: String,
    },
    /// A membership list is empty, repeats itself, or is out of order.
    #[error("a membership list is non-empty, ordered, and holds each value once")]
    MembershipUnusable,
    /// A comparison was given a type that does not sort.
    #[error("{named} does not sort, so it cannot be compared")]
    NotOrdered {
        /// Which type was given.
        named: String,
    },
    /// More predicates were given than one request may carry.
    #[error("one request carries fewer predicates than this")]
    TooManyPredicates,
}

/// Returns the predicates a repeated option's values describe.
///
/// # Errors
///
/// Returns [`PredicateArgumentRefusal`] naming the first thing that is wrong,
/// before any of them reaches a daemon.
pub fn parse_all(arguments: &[String]) -> Result<PropertyPredicates, PredicateArgumentRefusal> {
    let predicates = arguments
        .iter()
        .map(|argument| parse_one(argument))
        .collect::<Result<Vec<PropertyPredicate>, PredicateArgumentRefusal>>()?;
    PropertyPredicates::new(predicates).map_err(|_| PredicateArgumentRefusal::TooManyPredicates)
}

/// Returns the predicate one canonical object describes.
///
/// # Errors
///
/// Returns [`PredicateArgumentRefusal`] naming the first thing that is wrong.
pub fn parse_one(argument: &str) -> Result<PropertyPredicate, PredicateArgumentRefusal> {
    let document: Value =
        serde_json::from_str(argument).map_err(|_| PredicateArgumentRefusal::NotAnObject)?;
    let object = document.as_object().ok_or(PredicateArgumentRefusal::NotAnObject)?;
    for named in object.keys() {
        if !EVERY_MEMBER.contains(&named.as_str()) {
            return Err(PredicateArgumentRefusal::SurplusMember { named: named.clone() });
        }
    }
    let property_path = required_text(object, PROPERTY_PATH_MEMBER)?;
    let property_path = RelativePropertyPath::parse(property_path).map_err(|_| {
        PredicateArgumentRefusal::ValueUnusable { named: PROPERTY_PATH_MEMBER.to_owned() }
    })?;
    let operator = required_text(object, OPERATOR_MEMBER)?.to_owned();
    if !DECLARED_OPERATORS.contains(&operator.as_str()) {
        return Err(PredicateArgumentRefusal::UnknownOperator { named: operator });
    }
    build(object, &operator, property_path)
}

/// Returns the predicate one checked operator and path produce.
fn build(
    object: &serde_json::Map<String, Value>,
    operator: &str,
    property_path: RelativePropertyPath,
) -> Result<PropertyPredicate, PredicateArgumentRefusal> {
    match operator {
        EXISTS_OPERATOR => {
            if object.contains_key(VALUE_MEMBER) || object.contains_key(VALUES_MEMBER) {
                return Err(PredicateArgumentRefusal::ValueNotTaken);
            }
            Ok(PropertyPredicate::Exists { property_path })
        }
        EQUALS_OPERATOR => Ok(PropertyPredicate::Equals {
            property_path,
            value: PropertyValue::Single(single(object)?),
        }),
        NOT_EQUALS_OPERATOR => Ok(PropertyPredicate::NotEquals {
            property_path,
            value: PropertyValue::Single(single(object)?),
        }),
        SCALAR_IN_OPERATOR => {
            Ok(PropertyPredicate::ScalarIn { property_path, values: membership(object)? })
        }
        LIST_CONTAINS_ANY_OPERATOR => {
            Ok(PropertyPredicate::ListContainsAny { property_path, values: membership(object)? })
        }
        LIST_CONTAINS_ALL_OPERATOR => {
            Ok(PropertyPredicate::ListContainsAll { property_path, values: membership(object)? })
        }
        _ => comparison(object, operator, property_path),
    }
}

/// Returns the comparison one of the four ordered operators produces.
fn comparison(
    object: &serde_json::Map<String, Value>,
    operator: &str,
    property_path: RelativePropertyPath,
) -> Result<PropertyPredicate, PredicateArgumentRefusal> {
    let value = ordered(object)?;
    match operator {
        LESS_THAN_OPERATOR => Ok(PropertyPredicate::LessThan { property_path, value }),
        LESS_THAN_OR_EQUAL_OPERATOR => {
            Ok(PropertyPredicate::LessThanOrEqual { property_path, value })
        }
        GREATER_THAN_OPERATOR => Ok(PropertyPredicate::GreaterThan { property_path, value }),
        GREATER_THAN_OR_EQUAL_OPERATOR => {
            Ok(PropertyPredicate::GreaterThanOrEqual { property_path, value })
        }
        named => Err(PredicateArgumentRefusal::UnknownOperator { named: named.to_owned() }),
    }
}

/// Returns the one value an equality predicate carries.
fn single(
    object: &serde_json::Map<String, Value>,
) -> Result<PropertyScalarValue, PredicateArgumentRefusal> {
    if object.contains_key(VALUES_MEMBER) {
        return Err(PredicateArgumentRefusal::SurplusMember { named: VALUES_MEMBER.to_owned() });
    }
    let stated = object.get(VALUE_MEMBER).ok_or_else(|| {
        PredicateArgumentRefusal::MemberMissing { named: VALUE_MEMBER.to_owned() }
    })?;
    scalar(required_text(object, TYPE_MEMBER)?, stated)
}

/// Returns the ordered value a comparison predicate carries.
fn ordered(
    object: &serde_json::Map<String, Value>,
) -> Result<OrderedScalarPropertyValue, PredicateArgumentRefusal> {
    let value = single(object)?;
    let named = value.type_name().to_owned();
    OrderedScalarPropertyValue::new(value)
        .map_err(|_| PredicateArgumentRefusal::NotOrdered { named })
}

/// Returns the values a membership predicate carries.
fn membership(
    object: &serde_json::Map<String, Value>,
) -> Result<MembershipValues, PredicateArgumentRefusal> {
    if object.contains_key(VALUE_MEMBER) {
        return Err(PredicateArgumentRefusal::SurplusMember { named: VALUE_MEMBER.to_owned() });
    }
    let stated = object.get(VALUES_MEMBER).and_then(Value::as_array).ok_or_else(|| {
        PredicateArgumentRefusal::MemberMissing { named: VALUES_MEMBER.to_owned() }
    })?;
    let named = required_text(object, TYPE_MEMBER)?;
    let values = stated
        .iter()
        .map(|value| scalar(named, value))
        .collect::<Result<Vec<PropertyScalarValue>, PredicateArgumentRefusal>>()?;
    MembershipValues::new(values).map_err(|_| PredicateArgumentRefusal::MembershipUnusable)
}

/// Returns one scalar of the named type.
fn scalar(named: &str, stated: &Value) -> Result<PropertyScalarValue, PredicateArgumentRefusal> {
    let unusable = || PredicateArgumentRefusal::ValueUnusable { named: named.to_owned() };
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
        other => Err(PredicateArgumentRefusal::UnknownType { named: other.to_owned() }),
    }
}

/// Returns one required text member.
fn required_text<'object>(
    object: &'object serde_json::Map<String, Value>,
    named: &str,
) -> Result<&'object str, PredicateArgumentRefusal> {
    object
        .get(named)
        .and_then(Value::as_str)
        .ok_or_else(|| PredicateArgumentRefusal::MemberMissing { named: named.to_owned() })
}
