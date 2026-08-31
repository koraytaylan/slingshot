//! The repository's own value model, kept lossless and kept separate.
//!
//! A JCR property is typed. A number that arrives as a Long is a Long, a
//! decimal keeps the scale it was written with, and a path is a path rather
//! than a string that happens to contain slashes. Collapsing any of that into
//! "whatever JSON has" would lose the type on the way out and guess it wrong on
//! the way back, so every scalar carries its own discriminator and every
//! spelling is exact.
//!
//! Three of those exactness rules are worth stating plainly, because each one
//! is a bug that would otherwise be invisible:
//!
//! - An Integer is written as a JSON *string*. JCR's Long is the full signed
//!   64-bit range, and a parser that reads JSON numbers as double-precision
//!   floating point silently rounds the far end of it. A string survives every
//!   parser intact.
//! - A Decimal preserves its scale. `1.50` and `1.5` are equal numbers and
//!   different JCR Decimals, so both spellings round-trip unchanged while
//!   comparison treats them as equal.
//! - A DateTime is one canonical spelling per instant. Offsets, leap seconds,
//!   and a `.000` suffix are all refused rather than normalized, so two
//!   spellings of one instant cannot exist to disagree about.
//!
//! # What is not here
//!
//! There is no null and no deletion. Both would make "leave this property
//! alone" and "remove this property" the same request, and a mutation that
//! omits a property leaves it unchanged. There is also no redaction marker and
//! no Open Service Gateway Initiative carrier: configuration observation is a
//! separate model with a separate grammar, and letting its values through here
//! would let a caller write configuration evidence into repository content.

use serde::de::Error as _;
use serde::{Deserialize, Serialize};

use crate::command::command_identity::CommandContract;
use crate::command::repository_path::RepositoryPropertyPath;

/// Wire spelling of the textual type.
pub const STRING_TYPE: &str = "string";

/// Wire spelling of the Boolean type.
pub const BOOLEAN_TYPE: &str = "boolean";

/// Wire spelling of the signed 64-bit type.
pub const INTEGER_TYPE: &str = "integer";

/// Wire spelling of the arbitrary-precision decimal type.
pub const DECIMAL_TYPE: &str = "decimal";

/// Wire spelling of the instant type.
pub const DATE_TIME_TYPE: &str = "date_time";

/// Wire spelling of the path type.
pub const REPOSITORY_PATH_TYPE: &str = "repository_path";

/// Wire spelling of a property holding one value.
pub const SINGLE_CARDINALITY: &str = "single";

/// Wire spelling of a property holding an ordered collection.
pub const MULTIPLE_CARDINALITY: &str = "multiple";

/// Separator between the integer and fractional parts of a decimal.
const DECIMAL_POINT: char = '.';

/// Sign a negative number carries.
const MINUS_SIGN: char = '-';

/// Where each field of an instant stands, and how many digits writes it.
///
/// The layout is data rather than arithmetic scattered through a parser,
/// because the whole point of this subset is that there is one layout.
const INSTANT_FIELDS: [(usize, usize); 6] = [(0, 4), (5, 2), (8, 2), (11, 2), (14, 2), (17, 2)];

/// Fields one instant carries, the six written ones plus its milliseconds.
const INSTANT_FIELD_COUNT: usize = INSTANT_FIELDS.len() + 1;

/// Position of the year among those fields.
const YEAR_FIELD: usize = 0;

/// Position of the month.
const MONTH_FIELD: usize = 1;

/// Position of the day.
const DAY_FIELD: usize = 2;

/// Position of the hour.
const HOUR_FIELD: usize = 3;

/// Position of the minute.
const MINUTE_FIELD: usize = 4;

/// Position of the second.
const SECOND_FIELD: usize = 5;

/// Position of the millisecond.
const MILLISECOND_FIELD: usize = 6;

/// Smallest year this subset represents.
const SMALLEST_YEAR: u32 = 1;

/// Largest year this subset represents.
const LARGEST_YEAR: u32 = 9999;

/// Largest month number.
const LARGEST_MONTH: u32 = 12;

/// Largest hour number.
const LARGEST_HOUR: u32 = 23;

/// Largest minute number.
///
/// Also the largest second number: this subset has no leap second, because a
/// leap second is a spelling no instant of this precision needs and a source of
/// two answers to one comparison.
const LARGEST_MINUTE_OR_SECOND: u32 = 59;

/// Days each month holds outside a leap year.
const DAYS_IN_MONTH: [u32; 12] = [31, 28, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31];

/// Day February gains in a leap year.
const LEAP_DAY: u32 = 29;

/// Years between one leap year and the next.
const LEAP_YEAR_PERIOD: u32 = 4;

/// Years between one skipped leap year and the next.
const SKIPPED_LEAP_YEAR_PERIOD: u32 = 100;

/// Years between one restored leap year and the next.
const RESTORED_LEAP_YEAR_PERIOD: u32 = 400;

/// Returns the largest textual property this contract accepts.
#[must_use]
pub fn maximum_property_string_bytes() -> u64 {
    CommandContract::embedded().limit("maximum_property_string_bytes")
}

/// Returns the largest decimal spelling this contract accepts.
#[must_use]
pub fn maximum_decimal_bytes() -> u64 {
    CommandContract::embedded().limit("maximum_decimal_bytes")
}

/// Returns the most integer digits a decimal may carry.
#[must_use]
pub fn maximum_decimal_integer_digits() -> u64 {
    CommandContract::embedded().limit("maximum_decimal_integer_digits")
}

/// Returns the most fraction digits a decimal may carry.
#[must_use]
pub fn maximum_decimal_fraction_digits() -> u64 {
    CommandContract::embedded().limit("maximum_decimal_fraction_digits")
}

/// Returns how many digits a nonzero millisecond fraction is written with.
#[must_use]
pub fn maximum_date_time_fraction_digits() -> u64 {
    CommandContract::embedded().limit("maximum_date_time_fraction_digits")
}

/// Returns the most values one multiple-valued property may carry.
#[must_use]
pub fn maximum_property_value_items() -> u64 {
    CommandContract::embedded().limit("maximum_property_value_items")
}

/// Reason a property value could not be built.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum PropertyValueFailure {
    /// The type discriminator is not one of the six.
    #[error("a property scalar type is one of the six this contract defines")]
    UnknownType,
    /// The cardinality is neither of the two.
    #[error("a property cardinality is either {SINGLE_CARDINALITY} or {MULTIPLE_CARDINALITY}")]
    UnknownCardinality,
    /// The value does not match the type it declared.
    #[error("a property value does not match the type it declares")]
    TypeMismatch,
    /// A textual value is longer than the contract allows.
    #[error("a textual property value is at most {maximum} bytes", maximum = maximum_property_string_bytes())]
    StringTooLong,
    /// An integer is spelled some way other than its one minimal spelling.
    #[error(
        "an integer property value is spelled minimally, without a plus, a leading zero, or a negative zero"
    )]
    IntegerNotMinimal,
    /// An integer is outside the signed 64-bit range JCR Long occupies.
    #[error("an integer property value lies in the signed 64-bit range")]
    IntegerOutOfRange,
    /// A decimal is spelled some way this subset does not accept.
    #[error(
        "a decimal property value is a plain decimal without a plus, an exponent, a leading integer zero, or a negative zero"
    )]
    DecimalNotCanonical,
    /// A decimal is longer than the contract allows.
    #[error("a decimal property value stays inside its digit and byte bounds")]
    DecimalTooLong,
    /// An instant is spelled some way this subset does not accept.
    #[error(
        "an instant is spelled YYYY-MM-DDTHH:MM:SSZ, with exactly three fraction digits when its milliseconds are nonzero"
    )]
    DateTimeNotCanonical,
    /// An instant names a day that does not exist.
    #[error("an instant names a day the calendar has")]
    DateTimeNotACalendarDay,
    /// A multiple-valued property carries nothing.
    #[error("a multiple-valued property carries at least one value")]
    ListEmpty,
    /// A multiple-valued property mixes types.
    #[error("every value of a multiple-valued property has the same type")]
    ListNotHomogeneous,
    /// A multiple-valued property carries more values than the contract allows.
    #[error("a multiple-valued property carries at most {maximum} values", maximum = maximum_property_value_items())]
    ListTooLong,
}

/// One JCR scalar, with the type it actually has.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PropertyScalarValue {
    /// A JCR String.
    Text(String),
    /// A JCR Boolean.
    Boolean(bool),
    /// A JCR Long, the full signed 64-bit range.
    Integer(i64),
    /// A JCR Decimal, keeping the scale it was written with.
    Decimal(DecimalString),
    /// A JCR Date, in this subset's canonical UTC millisecond spelling.
    DateTime(DateTimeString),
    /// A JCR Path.
    Path(RepositoryPropertyPath),
}

impl PropertyScalarValue {
    /// Returns the wire spelling of this value's type.
    #[must_use]
    pub fn type_name(&self) -> &'static str {
        match self {
            Self::Text(_) => STRING_TYPE,
            Self::Boolean(_) => BOOLEAN_TYPE,
            Self::Integer(_) => INTEGER_TYPE,
            Self::Decimal(_) => DECIMAL_TYPE,
            Self::DateTime(_) => DATE_TIME_TYPE,
            Self::Path(_) => REPOSITORY_PATH_TYPE,
        }
    }

    /// Returns the textual value `text` carries.
    ///
    /// # Errors
    ///
    /// Returns [`PropertyValueFailure::StringTooLong`] above the named bound.
    pub fn text(text: impl Into<String>) -> Result<Self, PropertyValueFailure> {
        let text = text.into();
        if u64::try_from(text.len()).unwrap_or(u64::MAX) > maximum_property_string_bytes() {
            return Err(PropertyValueFailure::StringTooLong);
        }
        Ok(Self::Text(text))
    }

    /// Returns the integer `spelling` names.
    ///
    /// # Errors
    ///
    /// Returns [`PropertyValueFailure::IntegerNotMinimal`] for a plus sign, a
    /// leading zero, or a negative zero, and
    /// [`PropertyValueFailure::IntegerOutOfRange`] outside signed 64 bits.
    pub fn integer(spelling: &str) -> Result<Self, PropertyValueFailure> {
        accept_minimal_integer(spelling)?;
        let value = spelling.parse::<i64>().map_err(|_| PropertyValueFailure::IntegerOutOfRange)?;
        Ok(Self::Integer(value))
    }

    /// Returns whether two values compare as equal numbers, instants, or
    /// spellings.
    ///
    /// Unlike types never compare equal, and equality never crosses a type:
    /// `1` the Integer and `1.0` the Decimal are different properties, however
    /// close their arithmetic.
    #[must_use]
    pub fn equals(&self, other: &Self) -> bool {
        match (self, other) {
            (Self::Decimal(left), Self::Decimal(right)) => left.compare(right).is_eq(),
            (left, right) => left == right,
        }
    }

    /// Returns how two values order, when their type is ordered and shared.
    ///
    /// Paths and unlike types have no order: a path is an address rather than a
    /// quantity, and ordering across types would answer a question the caller
    /// did not ask.
    #[must_use]
    pub fn compare(&self, other: &Self) -> Option<std::cmp::Ordering> {
        match (self, other) {
            (Self::Text(left), Self::Text(right)) => Some(left.cmp(right)),
            (Self::Boolean(left), Self::Boolean(right)) => Some(left.cmp(right)),
            (Self::Integer(left), Self::Integer(right)) => Some(left.cmp(right)),
            (Self::Decimal(left), Self::Decimal(right)) => Some(left.compare(right)),
            (Self::DateTime(left), Self::DateTime(right)) => Some(left.compare(right)),
            _ => None,
        }
    }

    /// Returns whether this value can take part in an ordered comparison.
    #[must_use]
    pub fn is_ordered(&self) -> bool {
        !matches!(self, Self::Path(_))
    }
}

/// Requires `spelling` to be the one minimal spelling of its integer.
fn accept_minimal_integer(spelling: &str) -> Result<(), PropertyValueFailure> {
    let digits = spelling.strip_prefix(MINUS_SIGN).unwrap_or(spelling);
    let minimal = !digits.is_empty()
        && digits.bytes().all(|byte| byte.is_ascii_digit())
        && (digits == "0" || !digits.starts_with('0'))
        && !(digits == "0" && spelling.starts_with(MINUS_SIGN));
    if minimal { Ok(()) } else { Err(PropertyValueFailure::IntegerNotMinimal) }
}

/// A JCR Decimal, kept exactly as it was written.
///
/// Scale is part of the value: `1.50` and `1.5` are the same number and
/// different Decimals, so the spelling round-trips unchanged while
/// [`Self::compare`] treats them as equal. Comparison is decimal arithmetic on
/// the digits, never binary floating point, which would round the far end of a
/// thousand-digit value into agreement with its neighbors.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct DecimalString {
    /// The spelling, exactly as it arrived.
    value: String,
}

impl DecimalString {
    /// Returns the decimal `spelling` names.
    ///
    /// # Errors
    ///
    /// Returns [`PropertyValueFailure::DecimalNotCanonical`] for a plus sign,
    /// an exponent, a leading integer zero, a negative zero, or an omitted
    /// integer or fraction digit, and [`PropertyValueFailure::DecimalTooLong`]
    /// above any of the three named bounds.
    pub fn new(spelling: impl Into<String>) -> Result<Self, PropertyValueFailure> {
        let value = spelling.into();
        if u64::try_from(value.len()).unwrap_or(u64::MAX) > maximum_decimal_bytes() {
            return Err(PropertyValueFailure::DecimalTooLong);
        }
        let unsigned = value.strip_prefix(MINUS_SIGN).unwrap_or(&value);
        let (integer, fraction) = split_decimal(unsigned)?;
        accept_decimal_parts(integer, fraction)?;
        if value.starts_with(MINUS_SIGN) && is_decimal_zero(integer, fraction) {
            return Err(PropertyValueFailure::DecimalNotCanonical);
        }
        Ok(Self { value })
    }

    /// Returns the spelling, exactly as it arrived.
    #[must_use]
    pub fn as_text(&self) -> &str {
        &self.value
    }

    /// Returns how this decimal orders against `other`, numerically.
    #[must_use]
    pub fn compare(&self, other: &Self) -> std::cmp::Ordering {
        let left = self.parts();
        let right = other.parts();
        match (left.0, right.0) {
            (true, false) => std::cmp::Ordering::Less,
            (false, true) => std::cmp::Ordering::Greater,
            (negative, _) => {
                let magnitude = compare_magnitude((left.1, left.2), (right.1, right.2));
                if negative { magnitude.reverse() } else { magnitude }
            }
        }
    }

    /// Returns this decimal as sign, integer digits, and fraction digits.
    fn parts(&self) -> (bool, &str, &str) {
        let negative = self.value.starts_with(MINUS_SIGN);
        let unsigned = self.value.strip_prefix(MINUS_SIGN).unwrap_or(&self.value);
        match unsigned.split_once(DECIMAL_POINT) {
            Some((integer, fraction)) => (negative, integer, fraction),
            None => (negative, unsigned, ""),
        }
    }
}

/// Splits one unsigned decimal into its integer and fractional digits.
fn split_decimal(unsigned: &str) -> Result<(&str, &str), PropertyValueFailure> {
    match unsigned.split_once(DECIMAL_POINT) {
        Some((_, fraction)) if fraction.is_empty() || fraction.contains(DECIMAL_POINT) => {
            Err(PropertyValueFailure::DecimalNotCanonical)
        }
        Some((integer, fraction)) => Ok((integer, fraction)),
        None => Ok((unsigned, "")),
    }
}

/// Requires both parts of a decimal to be spelled the one way this subset
/// accepts.
///
/// A fraction is optional, but a decimal point is not: `1.` names a scale it
/// does not write down, so it is refused rather than read as `1`.
fn accept_decimal_parts(integer: &str, fraction: &str) -> Result<(), PropertyValueFailure> {
    let integer_minimal = !integer.is_empty()
        && integer.bytes().all(|byte| byte.is_ascii_digit())
        && (integer == "0" || !integer.starts_with('0'));
    let fraction_shaped = fraction.bytes().all(|byte| byte.is_ascii_digit());
    if !integer_minimal || !fraction_shaped {
        return Err(PropertyValueFailure::DecimalNotCanonical);
    }
    let digits = u64::try_from(integer.len()).unwrap_or(u64::MAX);
    let fraction_digits = u64::try_from(fraction.len()).unwrap_or(u64::MAX);
    if digits > maximum_decimal_integer_digits()
        || fraction_digits > maximum_decimal_fraction_digits()
    {
        return Err(PropertyValueFailure::DecimalTooLong);
    }
    Ok(())
}

/// Returns whether both parts together spell zero.
fn is_decimal_zero(integer: &str, fraction: &str) -> bool {
    integer == "0" && fraction.bytes().all(|byte| byte == b'0')
}

/// Returns how two unsigned decimal magnitudes order.
///
/// Longer integer parts are larger, because both are minimal and therefore
/// carry no leading zero. Fractions are compared position by position, so
/// `1.50` and `1.5` reach the same answer without either being rewritten.
fn compare_magnitude(left: (&str, &str), right: (&str, &str)) -> std::cmp::Ordering {
    left.0
        .len()
        .cmp(&right.0.len())
        .then_with(|| left.0.cmp(right.0))
        .then_with(|| compare_fractions(left.1, right.1))
}

/// Returns how two fractional digit strings order.
fn compare_fractions(left: &str, right: &str) -> std::cmp::Ordering {
    let width = left.len().max(right.len());
    let padded = |digits: &str| {
        let mut padded = digits.to_owned();
        padded.push_str(&"0".repeat(width - digits.len()));
        padded
    };
    padded(left).cmp(&padded(right))
}

/// One instant, in the single spelling this subset accepts.
///
/// The supported range is the interoperable one, not the whole Java Calendar:
/// year 0001 through 9999, whole seconds, and milliseconds written with exactly
/// three digits when they are nonzero and omitted entirely when they are zero.
/// Offsets are refused rather than converted, so no instant has two spellings
/// that could disagree about their order.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct DateTimeString {
    /// The spelling, exactly as it arrived.
    value: String,
    /// The fields it names, in comparison order.
    fields: [u32; INSTANT_FIELD_COUNT],
}

impl serde::Serialize for DateTimeString {
    fn serialize<Target: serde::Serializer>(
        &self,
        serializer: Target,
    ) -> Result<Target::Ok, Target::Error> {
        serializer.serialize_str(&self.value)
    }
}

impl<'de> serde::Deserialize<'de> for DateTimeString {
    fn deserialize<Source: serde::Deserializer<'de>>(
        deserializer: Source,
    ) -> Result<Self, Source::Error> {
        use serde::de::Error as _;

        Self::new(String::deserialize(deserializer)?).map_err(Source::Error::custom)
    }
}

impl DateTimeString {
    /// Returns the instant `spelling` names.
    ///
    /// # Errors
    ///
    /// Returns [`PropertyValueFailure::DateTimeNotCanonical`] for any spelling
    /// outside this subset and
    /// [`PropertyValueFailure::DateTimeNotACalendarDay`] when the fields are
    /// well shaped but name a day that does not exist.
    pub fn new(spelling: impl Into<String>) -> Result<Self, PropertyValueFailure> {
        let value = spelling.into();
        let fields = parse_instant(&value)?;
        require_calendar_day(fields[YEAR_FIELD], fields[MONTH_FIELD], fields[DAY_FIELD])?;
        Ok(Self { value, fields })
    }

    /// Returns the spelling, exactly as it arrived.
    #[must_use]
    pub fn as_text(&self) -> &str {
        &self.value
    }

    /// Returns how this instant orders against `other`.
    ///
    /// The fields are already in most-significant-first order and the spelling
    /// is unique per instant, so comparing them compares instants.
    #[must_use]
    pub fn compare(&self, other: &Self) -> std::cmp::Ordering {
        self.fields.cmp(&other.fields)
    }
}

/// Reads the fields out of one canonical instant.
fn parse_instant(spelling: &str) -> Result<[u32; INSTANT_FIELD_COUNT], PropertyValueFailure> {
    /// Bytes a whole-second instant occupies.
    const WHOLE_SECOND_BYTES: usize = 20;
    /// Positions the fixed punctuation occupies, and what stands there.
    const PUNCTUATION: [(usize, u8); 5] =
        [(4, b'-'), (7, b'-'), (10, b'T'), (13, b':'), (16, b':')];

    let malformed = || PropertyValueFailure::DateTimeNotCanonical;
    let bytes = spelling.as_bytes();
    let fraction_digits =
        usize::try_from(maximum_date_time_fraction_digits()).map_err(|_| malformed())?;
    let millisecond = read_millisecond(spelling, WHOLE_SECOND_BYTES, fraction_digits)?;
    if bytes.len() < WHOLE_SECOND_BYTES {
        return Err(malformed());
    }
    if PUNCTUATION.iter().any(|(position, character)| bytes[*position] != *character) {
        return Err(malformed());
    }
    let mut fields = [0; INSTANT_FIELD_COUNT];
    for (field, (offset, width)) in INSTANT_FIELDS.iter().enumerate() {
        fields[field] = read_digits(spelling, *offset, *width)?;
    }
    fields[MILLISECOND_FIELD] = millisecond;
    accept_instant_ranges(&fields)?;
    Ok(fields)
}

/// Requires every field except the day to lie in its range.
///
/// The day is checked separately, because whether it exists depends on the
/// month and the year rather than on its own digits.
fn accept_instant_ranges(fields: &[u32; INSTANT_FIELD_COUNT]) -> Result<(), PropertyValueFailure> {
    /// Smallest month and day number.
    const SMALLEST_MONTH_OR_DAY: u32 = 1;

    let inside = (SMALLEST_YEAR..=LARGEST_YEAR).contains(&fields[YEAR_FIELD])
        && (SMALLEST_MONTH_OR_DAY..=LARGEST_MONTH).contains(&fields[MONTH_FIELD])
        && fields[HOUR_FIELD] <= LARGEST_HOUR
        && fields[MINUTE_FIELD] <= LARGEST_MINUTE_OR_SECOND
        && fields[SECOND_FIELD] <= LARGEST_MINUTE_OR_SECOND;
    if inside { Ok(()) } else { Err(PropertyValueFailure::DateTimeNotCanonical) }
}

/// Reads the millisecond suffix, which is present exactly when it is nonzero.
///
/// A `.000` suffix is refused rather than accepted as zero, because it would be
/// a second spelling of an instant that already has one.
fn read_millisecond(
    spelling: &str,
    whole_second_bytes: usize,
    fraction_digits: usize,
) -> Result<u32, PropertyValueFailure> {
    /// Bytes the trailing zone designator occupies.
    const ZONE_BYTES: usize = 1;

    let malformed = || PropertyValueFailure::DateTimeNotCanonical;
    if !spelling.ends_with('Z') {
        return Err(malformed());
    }
    if spelling.len() == whole_second_bytes {
        return Ok(0);
    }
    if spelling.len() != whole_second_bytes + ZONE_BYTES + fraction_digits {
        return Err(malformed());
    }
    if spelling.as_bytes().get(whole_second_bytes - ZONE_BYTES) != Some(&b'.') {
        return Err(malformed());
    }
    let millisecond = read_digits(spelling, whole_second_bytes, fraction_digits)?;
    if millisecond == 0 { Err(malformed()) } else { Ok(millisecond) }
}

/// Reads `width` ASCII digits starting at `offset`.
fn read_digits(spelling: &str, offset: usize, width: usize) -> Result<u32, PropertyValueFailure> {
    let malformed = || PropertyValueFailure::DateTimeNotCanonical;
    let field = spelling.get(offset..offset + width).ok_or_else(malformed)?;
    if !field.bytes().all(|byte| byte.is_ascii_digit()) {
        return Err(malformed());
    }
    field.parse::<u32>().map_err(|_| malformed())
}

/// Requires the named day to exist in the named month of the named year.
fn require_calendar_day(year: u32, month: u32, day: u32) -> Result<(), PropertyValueFailure> {
    /// Smallest day number any month has.
    const SMALLEST_DAY: u32 = 1;
    /// Position February occupies among the months.
    const FEBRUARY: usize = 1;

    let index = usize::try_from(month - SMALLEST_DAY).unwrap_or_default();
    let ordinary = DAYS_IN_MONTH.get(index).copied().unwrap_or_default();
    let leap = year.is_multiple_of(LEAP_YEAR_PERIOD)
        && (!year.is_multiple_of(SKIPPED_LEAP_YEAR_PERIOD)
            || year.is_multiple_of(RESTORED_LEAP_YEAR_PERIOD));
    let longest = if leap && index == FEBRUARY { LEAP_DAY } else { ordinary };
    if day >= SMALLEST_DAY && day <= longest {
        Ok(())
    } else {
        Err(PropertyValueFailure::DateTimeNotACalendarDay)
    }
}

/// What one JCR property holds.
///
/// Single and multiple are different properties, not one property with a
/// convenience spelling: a repository that holds a one-element list is not
/// holding a scalar, and flattening the two would make a mutation change the
/// property's cardinality by accident.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PropertyValue {
    /// One scalar.
    Single(PropertyScalarValue),
    /// An ordered collection of scalars, all of one type.
    Multiple(Vec<PropertyScalarValue>),
}

impl PropertyValue {
    /// Returns the multiple-valued property `values` spell.
    ///
    /// # Errors
    ///
    /// Returns [`PropertyValueFailure::ListEmpty`] for no values,
    /// [`PropertyValueFailure::ListNotHomogeneous`] when the types differ, and
    /// [`PropertyValueFailure::ListTooLong`] above the named bound.
    pub fn multiple(values: Vec<PropertyScalarValue>) -> Result<Self, PropertyValueFailure> {
        let Some(first) = values.first() else {
            return Err(PropertyValueFailure::ListEmpty);
        };
        if u64::try_from(values.len()).unwrap_or(u64::MAX) > maximum_property_value_items() {
            return Err(PropertyValueFailure::ListTooLong);
        }
        let discriminator = first.type_name();
        if values.iter().any(|value| value.type_name() != discriminator) {
            return Err(PropertyValueFailure::ListNotHomogeneous);
        }
        Ok(Self::Multiple(values))
    }

    /// Returns the wire spelling of this property's cardinality.
    #[must_use]
    pub fn cardinality(&self) -> &'static str {
        match self {
            Self::Single(_) => SINGLE_CARDINALITY,
            Self::Multiple(_) => MULTIPLE_CARDINALITY,
        }
    }

    /// Returns the wire spelling of the type every value here has.
    #[must_use]
    pub fn type_name(&self) -> &'static str {
        match self {
            Self::Single(value) => value.type_name(),
            Self::Multiple(values) => {
                values.first().map_or(STRING_TYPE, PropertyScalarValue::type_name)
            }
        }
    }

    /// Returns whether two properties hold the same values.
    ///
    /// Cardinality is part of the answer, and a list compares element for
    /// element in repository order, because a JCR multi-value is ordered.
    #[must_use]
    pub fn equals(&self, other: &Self) -> bool {
        match (self, other) {
            (Self::Single(left), Self::Single(right)) => left.equals(right),
            (Self::Multiple(left), Self::Multiple(right)) => {
                left.len() == right.len()
                    && left.iter().zip(right).all(|(left, right)| left.equals(right))
            }
            _ => false,
        }
    }
}

/// One scalar exactly as it is written on the wire.
#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct ScalarDocument {
    /// Which of the six types this is.
    #[serde(rename = "type")]
    type_name: String,
    /// The value, in the spelling that type uses.
    value: serde_json::Value,
}

impl TryFrom<ScalarDocument> for PropertyScalarValue {
    type Error = PropertyValueFailure;

    fn try_from(document: ScalarDocument) -> Result<Self, Self::Error> {
        let mismatch = || PropertyValueFailure::TypeMismatch;
        match document.type_name.as_str() {
            STRING_TYPE => Self::text(document.value.as_str().ok_or_else(mismatch)?),
            BOOLEAN_TYPE => Ok(Self::Boolean(document.value.as_bool().ok_or_else(mismatch)?)),
            INTEGER_TYPE => Self::integer(document.value.as_str().ok_or_else(mismatch)?),
            DECIMAL_TYPE => {
                DecimalString::new(document.value.as_str().ok_or_else(mismatch)?).map(Self::Decimal)
            }
            DATE_TIME_TYPE => DateTimeString::new(document.value.as_str().ok_or_else(mismatch)?)
                .map(Self::DateTime),
            REPOSITORY_PATH_TYPE => {
                let spelling = document.value.as_str().ok_or_else(mismatch)?;
                RepositoryPropertyPath::parse(spelling).map(Self::Path).map_err(|_| mismatch())
            }
            _ => Err(PropertyValueFailure::UnknownType),
        }
    }
}

impl From<&PropertyScalarValue> for ScalarDocument {
    fn from(value: &PropertyScalarValue) -> Self {
        let written = match value {
            PropertyScalarValue::Text(text) => serde_json::Value::from(text.as_str()),
            PropertyScalarValue::Boolean(value) => serde_json::Value::from(*value),
            PropertyScalarValue::Integer(value) => serde_json::Value::from(value.to_string()),
            PropertyScalarValue::Decimal(value) => serde_json::Value::from(value.as_text()),
            PropertyScalarValue::DateTime(value) => serde_json::Value::from(value.as_text()),
            PropertyScalarValue::Path(value) => serde_json::Value::from(value.as_text()),
        };
        Self { type_name: value.type_name().to_owned(), value: written }
    }
}

impl Serialize for PropertyScalarValue {
    fn serialize<Target: serde::Serializer>(
        &self,
        serializer: Target,
    ) -> Result<Target::Ok, Target::Error> {
        ScalarDocument::from(self).serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for PropertyScalarValue {
    fn deserialize<Source: serde::Deserializer<'de>>(
        deserializer: Source,
    ) -> Result<Self, Source::Error> {
        let document = ScalarDocument::deserialize(deserializer)?;
        Self::try_from(document).map_err(Source::Error::custom)
    }
}

/// One property exactly as it is written on the wire.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct PropertyDocument {
    /// Whether this property holds one value or an ordered collection.
    cardinality: String,
    /// The single value, on a single-valued property alone.
    #[serde(default)]
    value: Option<PropertyScalarValue>,
    /// The ordered values, on a multiple-valued property alone.
    #[serde(default)]
    values: Option<Vec<PropertyScalarValue>>,
}

impl TryFrom<PropertyDocument> for PropertyValue {
    type Error = PropertyValueFailure;

    fn try_from(document: PropertyDocument) -> Result<Self, Self::Error> {
        match (document.cardinality.as_str(), document.value, document.values) {
            (SINGLE_CARDINALITY, Some(value), None) => Ok(Self::Single(value)),
            (MULTIPLE_CARDINALITY, None, Some(values)) => Self::multiple(values),
            (SINGLE_CARDINALITY | MULTIPLE_CARDINALITY, _, _) => {
                Err(PropertyValueFailure::TypeMismatch)
            }
            _ => Err(PropertyValueFailure::UnknownCardinality),
        }
    }
}

impl Serialize for PropertyValue {
    fn serialize<Target: serde::Serializer>(
        &self,
        serializer: Target,
    ) -> Result<Target::Ok, Target::Error> {
        use serde::ser::SerializeStruct;

        /// Members a property writes: its cardinality and its value or values.
        const MEMBERS: usize = 2;

        let mut property = serializer.serialize_struct("PropertyValue", MEMBERS)?;
        property.serialize_field("cardinality", self.cardinality())?;
        match self {
            Self::Single(value) => property.serialize_field("value", value)?,
            Self::Multiple(values) => property.serialize_field("values", values)?,
        }
        property.end()
    }
}

impl<'de> Deserialize<'de> for PropertyValue {
    fn deserialize<Source: serde::Deserializer<'de>>(
        deserializer: Source,
    ) -> Result<Self, Source::Error> {
        let document = PropertyDocument::deserialize(deserializer)?;
        Self::try_from(document).map_err(Source::Error::custom)
    }
}
