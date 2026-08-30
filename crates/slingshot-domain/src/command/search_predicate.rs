//! What a search asks about a node, said structurally.
//!
//! No command here accepts query text. A caller cannot send JCR-SQL2, XPath, or
//! anything else the repository would interpret, because a string that reaches
//! a query engine is a string that can address content the command was never
//! pointed at. A predicate is a closed value instead: an operator, a property
//! to look at, and - for the operators that need one - a typed value to compare
//! against.
//!
//! Types are never inferred. A comparison carries a [`PropertyScalarValue`]
//! with its own discriminator, so `"1"` the string and `1` the Long are two
//! different questions rather than one question whose meaning depends on how
//! the caller happened to write it. Unlike types never compare at all, which
//! makes a mistyped predicate a refusal rather than a silently empty result.
//!
//! Resolution is exact. A [`RelativePropertyPath`] names child resources and
//! then one property, and it is resolved from the candidate node with no
//! descendant search, no name aliasing, and no fallback. A predicate that finds
//! nothing has found nothing.

use serde::de::Error as _;
use serde::{Deserialize, Serialize};

use crate::command::command_identity::CommandContract;
use crate::command::property_value::{PropertyScalarValue, PropertyValue};
use crate::command::repository_path::RelativePropertyPath;

/// Operator asking only whether a property is there.
pub const EXISTS_OPERATOR: &str = "exists";

/// Operator asking whether a property holds exactly one value.
pub const EQUALS_OPERATOR: &str = "equals";

/// Operator asking whether it holds anything else.
pub const NOT_EQUALS_OPERATOR: &str = "not_equals";

/// Operator asking whether a scalar property is one of several values.
pub const SCALAR_IN_OPERATOR: &str = "scalar_in";

/// Operator asking whether a list property holds any of several values.
pub const LIST_CONTAINS_ANY_OPERATOR: &str = "list_contains_any";

/// Operator asking whether it holds all of them.
pub const LIST_CONTAINS_ALL_OPERATOR: &str = "list_contains_all";

/// Operator asking whether a scalar property sorts before a value.
pub const LESS_THAN_OPERATOR: &str = "less_than";

/// Operator asking whether it sorts before or equal to one.
pub const LESS_THAN_OR_EQUAL_OPERATOR: &str = "less_than_or_equal";

/// Operator asking whether it sorts after a value.
pub const GREATER_THAN_OPERATOR: &str = "greater_than";

/// Operator asking whether it sorts after or equal to one.
pub const GREATER_THAN_OR_EQUAL_OPERATOR: &str = "greater_than_or_equal";

/// Every operator this language has, in the order they are documented.
pub const DECLARED_OPERATORS: &[&str] = &[
    EXISTS_OPERATOR,
    EQUALS_OPERATOR,
    NOT_EQUALS_OPERATOR,
    SCALAR_IN_OPERATOR,
    LIST_CONTAINS_ANY_OPERATOR,
    LIST_CONTAINS_ALL_OPERATOR,
    LESS_THAN_OPERATOR,
    LESS_THAN_OR_EQUAL_OPERATOR,
    GREATER_THAN_OPERATOR,
    GREATER_THAN_OR_EQUAL_OPERATOR,
];

/// Returns the most values one membership predicate may name.
#[must_use]
pub fn maximum_property_predicate_values() -> u64 {
    CommandContract::embedded().limit("maximum_property_predicate_values")
}

/// Returns the most predicates one search may compose.
#[must_use]
pub fn maximum_property_predicates() -> u64 {
    CommandContract::embedded().limit("maximum_property_predicates")
}

/// Reason a predicate could not be built.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum PredicateFailure {
    /// The operator is not one of the ten.
    #[error("a predicate operator is one of the ten this language defines")]
    UnknownOperator,
    /// The predicate carries a field its operator does not take, or omits one
    /// it requires.
    #[error("a predicate carries exactly the fields its operator takes")]
    FieldsDoNotMatchOperator,
    /// A membership predicate names no values.
    #[error("a membership predicate names at least one value")]
    ValuesEmpty,
    /// A membership predicate names one value twice.
    #[error("a membership predicate names each value once")]
    ValuesNotUnique,
    /// A membership predicate mixes types.
    #[error("every value of a membership predicate has the same type")]
    ValuesNotHomogeneous,
    /// A membership predicate names more values than the contract allows.
    #[error("a membership predicate names at most {maximum} values", maximum = maximum_property_predicate_values())]
    ValuesTooMany,
    /// An ordered comparison was given a value that has no order.
    #[error("an ordered comparison takes a value that has an order, which a path does not")]
    ValueNotOrdered,
    /// A search composes more predicates than the contract allows.
    #[error("a search composes at most {maximum} predicates", maximum = maximum_property_predicates())]
    TooManyPredicates,
}

/// One scalar that has an order.
///
/// A path is excluded by construction rather than by a check at comparison
/// time, so a predicate asking whether one address is less than another cannot
/// be built at all - which is the honest answer, because addresses are not
/// ordered quantities.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(transparent)]
pub struct OrderedScalarPropertyValue {
    /// The scalar, known to have an order.
    value: PropertyScalarValue,
}

impl OrderedScalarPropertyValue {
    /// Returns the ordered scalar `value` carries.
    ///
    /// # Errors
    ///
    /// Returns [`PredicateFailure::ValueNotOrdered`] for a path.
    pub fn new(value: PropertyScalarValue) -> Result<Self, PredicateFailure> {
        if value.is_ordered() { Ok(Self { value }) } else { Err(PredicateFailure::ValueNotOrdered) }
    }

    /// Returns the scalar itself.
    #[must_use]
    pub fn scalar(&self) -> &PropertyScalarValue {
        &self.value
    }
}

impl<'de> Deserialize<'de> for OrderedScalarPropertyValue {
    fn deserialize<Source: serde::Deserializer<'de>>(
        deserializer: Source,
    ) -> Result<Self, Source::Error> {
        let value = PropertyScalarValue::deserialize(deserializer)?;
        Self::new(value).map_err(Source::Error::custom)
    }
}

/// A non-empty, unique, same-type collection of candidate values.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(transparent)]
pub struct MembershipValues {
    /// The candidates, in the order they were given.
    values: Vec<PropertyScalarValue>,
}

impl MembershipValues {
    /// Returns the membership collection `values` spell.
    ///
    /// # Errors
    ///
    /// Returns [`PredicateFailure::ValuesEmpty`],
    /// [`PredicateFailure::ValuesTooMany`],
    /// [`PredicateFailure::ValuesNotHomogeneous`], or
    /// [`PredicateFailure::ValuesNotUnique`].
    pub fn new(values: Vec<PropertyScalarValue>) -> Result<Self, PredicateFailure> {
        let Some(first) = values.first() else {
            return Err(PredicateFailure::ValuesEmpty);
        };
        if u64::try_from(values.len()).unwrap_or(u64::MAX) > maximum_property_predicate_values() {
            return Err(PredicateFailure::ValuesTooMany);
        }
        let discriminator = first.type_name();
        if values.iter().any(|value| value.type_name() != discriminator) {
            return Err(PredicateFailure::ValuesNotHomogeneous);
        }
        if has_repeat(&values) {
            return Err(PredicateFailure::ValuesNotUnique);
        }
        Ok(Self { values })
    }

    /// Returns the candidates, in the order they were given.
    #[must_use]
    pub fn values(&self) -> &[PropertyScalarValue] {
        &self.values
    }

    /// Returns whether any candidate equals `value`.
    #[must_use]
    pub fn contains(&self, value: &PropertyScalarValue) -> bool {
        self.values.iter().any(|candidate| candidate.equals(value))
    }
}

/// Returns whether any two values are equal.
///
/// Equality here is the model's, not the spelling's, so `1.50` and `1.5` count
/// as one candidate named twice rather than two.
fn has_repeat(values: &[PropertyScalarValue]) -> bool {
    values
        .iter()
        .enumerate()
        .any(|(position, value)| values.iter().skip(position + 1).any(|later| later.equals(value)))
}

impl<'de> Deserialize<'de> for MembershipValues {
    fn deserialize<Source: serde::Deserializer<'de>>(
        deserializer: Source,
    ) -> Result<Self, Source::Error> {
        let values = Vec::<PropertyScalarValue>::deserialize(deserializer)?;
        Self::new(values).map_err(Source::Error::custom)
    }
}

/// What the repository was found to hold at one resolved property path.
///
/// Absence and an explicitly empty multi-value are different observations. A
/// repository can hold a property that has no values, and `Exists` is true of
/// it, so collapsing the two would answer that question wrongly.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ObservedProperty {
    /// Present, holding an explicitly empty repository multi-value.
    EmptyMultiple,
    /// Present, holding values.
    Held(PropertyValue),
}

/// One question about one property of one candidate node.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PropertyPredicate {
    /// Whether the property is present at all.
    Exists {
        /// Property to resolve from the candidate node.
        property_path: RelativePropertyPath,
    },
    /// Whether it holds exactly this value.
    Equals {
        /// Property to resolve.
        property_path: RelativePropertyPath,
        /// Value it must hold.
        value: PropertyValue,
    },
    /// Whether it holds anything but this value.
    NotEquals {
        /// Property to resolve.
        property_path: RelativePropertyPath,
        /// Value it must not hold.
        value: PropertyValue,
    },
    /// Whether a scalar property holds one of these values.
    ScalarIn {
        /// Property to resolve.
        property_path: RelativePropertyPath,
        /// Candidates it may hold.
        values: MembershipValues,
    },
    /// Whether a list property holds any of these values.
    ListContainsAny {
        /// Property to resolve.
        property_path: RelativePropertyPath,
        /// Members it may hold.
        values: MembershipValues,
    },
    /// Whether a list property holds all of them.
    ListContainsAll {
        /// Property to resolve.
        property_path: RelativePropertyPath,
        /// Members it must hold.
        values: MembershipValues,
    },
    /// Whether a scalar property sorts before this value.
    LessThan {
        /// Property to resolve.
        property_path: RelativePropertyPath,
        /// Value to sort against.
        value: OrderedScalarPropertyValue,
    },
    /// Whether it sorts before or equal to it.
    LessThanOrEqual {
        /// Property to resolve.
        property_path: RelativePropertyPath,
        /// Value to sort against.
        value: OrderedScalarPropertyValue,
    },
    /// Whether it sorts after it.
    GreaterThan {
        /// Property to resolve.
        property_path: RelativePropertyPath,
        /// Value to sort against.
        value: OrderedScalarPropertyValue,
    },
    /// Whether it sorts after or equal to it.
    GreaterThanOrEqual {
        /// Property to resolve.
        property_path: RelativePropertyPath,
        /// Value to sort against.
        value: OrderedScalarPropertyValue,
    },
}

impl PropertyPredicate {
    /// Returns the wire spelling of this predicate's operator.
    #[must_use]
    pub fn operator(&self) -> &'static str {
        match self {
            Self::Exists { .. } => EXISTS_OPERATOR,
            Self::Equals { .. } => EQUALS_OPERATOR,
            Self::NotEquals { .. } => NOT_EQUALS_OPERATOR,
            Self::ScalarIn { .. } => SCALAR_IN_OPERATOR,
            Self::ListContainsAny { .. } => LIST_CONTAINS_ANY_OPERATOR,
            Self::ListContainsAll { .. } => LIST_CONTAINS_ALL_OPERATOR,
            Self::LessThan { .. } => LESS_THAN_OPERATOR,
            Self::LessThanOrEqual { .. } => LESS_THAN_OR_EQUAL_OPERATOR,
            Self::GreaterThan { .. } => GREATER_THAN_OPERATOR,
            Self::GreaterThanOrEqual { .. } => GREATER_THAN_OR_EQUAL_OPERATOR,
        }
    }

    /// Returns the property this predicate resolves.
    #[must_use]
    pub fn property_path(&self) -> &RelativePropertyPath {
        match self {
            Self::Exists { property_path }
            | Self::Equals { property_path, .. }
            | Self::NotEquals { property_path, .. }
            | Self::ScalarIn { property_path, .. }
            | Self::ListContainsAny { property_path, .. }
            | Self::ListContainsAll { property_path, .. }
            | Self::LessThan { property_path, .. }
            | Self::LessThanOrEqual { property_path, .. }
            | Self::GreaterThan { property_path, .. }
            | Self::GreaterThanOrEqual { property_path, .. } => property_path,
        }
    }

    /// Returns whether `observed` answers this predicate affirmatively.
    ///
    /// An absent property answers no to everything except a `NotEquals`, which
    /// asks whether the property holds some particular value and is answered by
    /// a property that holds nothing at all.
    #[must_use]
    pub fn matches(&self, observed: Option<&ObservedProperty>) -> bool {
        match self {
            Self::Exists { .. } => observed.is_some(),
            Self::Equals { value, .. } => held(observed).is_some_and(|held| held.equals(value)),
            Self::NotEquals { value, .. } => !held(observed).is_some_and(|held| held.equals(value)),
            Self::ScalarIn { values, .. } => {
                matches!(held(observed), Some(PropertyValue::Single(scalar)) if values.contains(scalar))
            }
            Self::ListContainsAny { values, .. } => list_of(observed)
                .is_some_and(|members| members.iter().any(|member| values.contains(member))),
            Self::ListContainsAll { values, .. } => list_of(observed).is_some_and(|members| {
                values
                    .values()
                    .iter()
                    .all(|wanted| members.iter().any(|member| member.equals(wanted)))
            }),
            Self::LessThan { value, .. } => ordered(observed, value).is_some_and(Ordering::is_lt),
            Self::LessThanOrEqual { value, .. } => {
                ordered(observed, value).is_some_and(Ordering::is_le)
            }
            Self::GreaterThan { value, .. } => {
                ordered(observed, value).is_some_and(Ordering::is_gt)
            }
            Self::GreaterThanOrEqual { value, .. } => {
                ordered(observed, value).is_some_and(Ordering::is_ge)
            }
        }
    }
}

use std::cmp::Ordering;

/// Returns the value an observation holds, when it holds one.
fn held(observed: Option<&ObservedProperty>) -> Option<&PropertyValue> {
    match observed {
        Some(ObservedProperty::Held(value)) => Some(value),
        _ => None,
    }
}

/// Returns the members an observation holds, when it is list-valued.
///
/// An explicitly empty repository multi-value is list-valued and holds nothing,
/// which is why it answers `ListContainsAny` no and `Exists` yes.
fn list_of(observed: Option<&ObservedProperty>) -> Option<&[PropertyScalarValue]> {
    match observed {
        Some(ObservedProperty::EmptyMultiple) => Some(&[]),
        Some(ObservedProperty::Held(PropertyValue::Multiple(values))) => Some(values),
        _ => None,
    }
}

/// Returns how a scalar observation orders against `value`.
fn ordered(
    observed: Option<&ObservedProperty>,
    value: &OrderedScalarPropertyValue,
) -> Option<Ordering> {
    match held(observed) {
        Some(PropertyValue::Single(scalar)) => scalar.compare(value.scalar()),
        _ => None,
    }
}

/// A bounded collection of predicates one search composes.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(transparent)]
pub struct PropertyPredicates {
    /// The predicates, in the order they were given.
    predicates: Vec<PropertyPredicate>,
}

impl PropertyPredicates {
    /// Returns the collection `predicates` spell.
    ///
    /// An empty collection is legal: a search that names no property predicate
    /// is a search constrained by its other arguments alone.
    ///
    /// # Errors
    ///
    /// Returns [`PredicateFailure::TooManyPredicates`] above the named bound.
    pub fn new(predicates: Vec<PropertyPredicate>) -> Result<Self, PredicateFailure> {
        if u64::try_from(predicates.len()).unwrap_or(u64::MAX) > maximum_property_predicates() {
            return Err(PredicateFailure::TooManyPredicates);
        }
        Ok(Self { predicates })
    }

    /// Returns the predicates, in the order they were given.
    #[must_use]
    pub fn predicates(&self) -> &[PropertyPredicate] {
        &self.predicates
    }

    /// Returns whether every predicate is answered affirmatively.
    #[must_use]
    pub fn all_match(
        &self,
        resolve: impl Fn(&RelativePropertyPath) -> Option<ObservedProperty>,
    ) -> bool {
        self.predicates
            .iter()
            .all(|predicate| predicate.matches(resolve(predicate.property_path()).as_ref()))
    }
}

impl<'de> Deserialize<'de> for PropertyPredicates {
    fn deserialize<Source: serde::Deserializer<'de>>(
        deserializer: Source,
    ) -> Result<Self, Source::Error> {
        let predicates = Vec::<PropertyPredicate>::deserialize(deserializer)?;
        Self::new(predicates).map_err(Source::Error::custom)
    }
}

/// One predicate exactly as it is written on the wire.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct PredicateDocument {
    /// Which of the ten questions this is.
    operator: String,
    /// Property to resolve from the candidate node.
    property_path: RelativePropertyPath,
    /// The single value, on the operators that take one.
    #[serde(default)]
    value: Option<serde_json::Value>,
    /// The candidates, on the operators that take several.
    #[serde(default)]
    values: Option<MembershipValues>,
}

impl TryFrom<PredicateDocument> for PropertyPredicate {
    type Error = PredicateFailure;

    fn try_from(document: PredicateDocument) -> Result<Self, Self::Error> {
        if !DECLARED_OPERATORS.contains(&document.operator.as_str()) {
            return Err(PredicateFailure::UnknownOperator);
        }
        let path = document.property_path;
        match (document.operator.as_str(), document.value, document.values) {
            (EXISTS_OPERATOR, None, None) => Ok(Self::Exists { property_path: path }),
            (EQUALS_OPERATOR | NOT_EQUALS_OPERATOR, Some(value), None) => {
                let value = parse_property(value)?;
                Ok(if document.operator == EQUALS_OPERATOR {
                    Self::Equals { property_path: path, value }
                } else {
                    Self::NotEquals { property_path: path, value }
                })
            }
            (
                SCALAR_IN_OPERATOR | LIST_CONTAINS_ANY_OPERATOR | LIST_CONTAINS_ALL_OPERATOR,
                None,
                Some(values),
            ) => Ok(membership(&document.operator, path, values)),
            (
                LESS_THAN_OPERATOR
                | LESS_THAN_OR_EQUAL_OPERATOR
                | GREATER_THAN_OPERATOR
                | GREATER_THAN_OR_EQUAL_OPERATOR,
                Some(value),
                None,
            ) => Ok(comparison(&document.operator, path, parse_ordered(value)?)),
            _ => Err(PredicateFailure::FieldsDoNotMatchOperator),
        }
    }
}

/// Reads one property value out of a predicate field.
fn parse_property(value: serde_json::Value) -> Result<PropertyValue, PredicateFailure> {
    serde_json::from_value(value).map_err(|_| PredicateFailure::FieldsDoNotMatchOperator)
}

/// Reads one ordered scalar out of a predicate field.
fn parse_ordered(value: serde_json::Value) -> Result<OrderedScalarPropertyValue, PredicateFailure> {
    let scalar: PropertyScalarValue =
        serde_json::from_value(value).map_err(|_| PredicateFailure::FieldsDoNotMatchOperator)?;
    OrderedScalarPropertyValue::new(scalar)
}

/// Returns the membership predicate `operator` names.
fn membership(
    operator: &str,
    property_path: RelativePropertyPath,
    values: MembershipValues,
) -> PropertyPredicate {
    match operator {
        SCALAR_IN_OPERATOR => PropertyPredicate::ScalarIn { property_path, values },
        LIST_CONTAINS_ANY_OPERATOR => PropertyPredicate::ListContainsAny { property_path, values },
        _ => PropertyPredicate::ListContainsAll { property_path, values },
    }
}

/// Returns the ordered comparison `operator` names.
fn comparison(
    operator: &str,
    property_path: RelativePropertyPath,
    value: OrderedScalarPropertyValue,
) -> PropertyPredicate {
    match operator {
        LESS_THAN_OPERATOR => PropertyPredicate::LessThan { property_path, value },
        LESS_THAN_OR_EQUAL_OPERATOR => PropertyPredicate::LessThanOrEqual { property_path, value },
        GREATER_THAN_OPERATOR => PropertyPredicate::GreaterThan { property_path, value },
        _ => PropertyPredicate::GreaterThanOrEqual { property_path, value },
    }
}

impl Serialize for PropertyPredicate {
    fn serialize<Target: serde::Serializer>(
        &self,
        serializer: Target,
    ) -> Result<Target::Ok, Target::Error> {
        use serde::ser::SerializeStruct;

        /// Members a predicate that takes no value writes.
        const BARE_MEMBERS: usize = 2;
        /// Members a predicate that takes one writes.
        const VALUED_MEMBERS: usize = 3;

        let members = match self {
            Self::Exists { .. } => BARE_MEMBERS,
            _ => VALUED_MEMBERS,
        };
        let mut predicate = serializer.serialize_struct("PropertyPredicate", members)?;
        predicate.serialize_field("operator", self.operator())?;
        predicate.serialize_field("property_path", self.property_path())?;
        match self {
            Self::Exists { .. } => (),
            Self::Equals { value, .. } | Self::NotEquals { value, .. } => {
                predicate.serialize_field("value", value)?;
            }
            Self::ScalarIn { values, .. }
            | Self::ListContainsAny { values, .. }
            | Self::ListContainsAll { values, .. } => {
                predicate.serialize_field("values", values)?;
            }
            Self::LessThan { value, .. }
            | Self::LessThanOrEqual { value, .. }
            | Self::GreaterThan { value, .. }
            | Self::GreaterThanOrEqual { value, .. } => {
                predicate.serialize_field("value", value)?;
            }
        }
        predicate.end()
    }
}

impl<'de> Deserialize<'de> for PropertyPredicate {
    fn deserialize<Source: serde::Deserializer<'de>>(
        deserializer: Source,
    ) -> Result<Self, Source::Error> {
        let document = PredicateDocument::deserialize(deserializer)?;
        Self::try_from(document).map_err(Source::Error::custom)
    }
}
