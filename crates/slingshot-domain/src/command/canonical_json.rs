//! The byte contract, kept separate from the schema contract.
//!
//! A JSON Schema validator answers questions about a decoded tree. It cannot
//! tell you whether the bytes it was handed had a byte-order mark, whether an
//! object's members arrived in ascending order, whether an integer was written
//! minimally, or whether a set-like array was sorted and free of repeats.
//! Those are byte questions, and every one of them matters here: a digest is
//! over bytes, and two spellings of one document would be two digests of one
//! meaning.
//!
//! So this module is a second, independent validator. It runs first, on raw
//! bytes, before any parser sees them. Nothing here consults a schema and no
//! schema result is ever presented as proof of what this checks.
//!
//! # Array order
//!
//! Some arrays are sets and some are sequences, and only the contract knows
//! which. An inventory maps each array's JSON Pointer to a comparator: a
//! pointer that is absent preserves its order, because sequences are the common
//! case and silently sorting one would change what it means.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

/// Format this contract declares.
pub const CANONICAL_JSON_FORMAT: &str = "slingshot.command-canonical-json/1";

/// Comparator a pointer carries when it is a sequence.
pub const PRESERVE_COMPARATOR: &str = "preserve";

/// Comparator for a set of strings.
pub const UTF8_ASCENDING_UNIQUE_COMPARATOR: &str = "utf8_ascending_unique";

/// Comparator for a set of absolute repository paths.
pub const REPOSITORY_PATH_COMPARATOR: &str = "repository_path_utf8_ascending_unique";

/// Comparator for a set of relative repository paths.
pub const RELATIVE_REPOSITORY_PATH_COMPARATOR: &str =
    "relative_repository_path_utf8_ascending_unique";

/// Comparator for a set of objects ordered by the replication agent each names.
pub const AGENT_IDENTIFIER_COMPARATOR: &str = "agent_identifier_utf8_ascending_unique";

/// Comparator for a set of objects ordered by the authorizable each names.
pub const AUTHORIZABLE_IDENTIFIER_COMPARATOR: &str =
    "authorizable_identifier_utf8_ascending_unique";

/// Comparator for a set of objects ordered by the replication queue entry each names.
pub const ENTRY_IDENTIFIER_COMPARATOR: &str = "entry_identifier_utf8_ascending_unique";

/// Comparator for a set of objects ordered by the mapping entry address each names.
pub const ENTRY_PATH_COMPARATOR: &str = "entry_path_utf8_ascending_unique";

/// Comparator for a set of objects ordered by the workflow instance each names.
pub const INSTANCE_IDENTIFIER_COMPARATOR: &str = "instance_identifier_utf8_ascending_unique";

/// Comparator for a set of objects ordered by the Sling job each names.
pub const JOB_IDENTIFIER_COMPARATOR: &str = "job_identifier_utf8_ascending_unique";

/// Comparator for a set of objects ordered by the workflow model each names.
pub const MODEL_IDENTIFIER_COMPARATOR: &str = "model_identifier_utf8_ascending_unique";

/// Comparator for a set of objects ordered by the name each carries.
pub const NAME_COMPARATOR: &str = "name_utf8_ascending_unique";

/// Comparator for a set of objects ordered by the configuration each names.
pub const PERSISTENT_IDENTIFIER_COMPARATOR: &str = "persistent_identifier_utf8_ascending_unique";

/// Comparator for a set of objects ordered by the queue each names.
pub const QUEUE_NAME_COMPARATOR: &str = "queue_name_utf8_ascending_unique";

/// Comparator for a set of objects ordered by symbolic name and then by version, because one deployment can hold two versions of one bundle.
pub const SYMBOLIC_NAME_COMPARATOR: &str = "symbolic_name_then_version_utf8_ascending_unique";

/// Comparator for a set of objects ordered by the work item each names.
pub const WORK_ITEM_IDENTIFIER_COMPARATOR: &str = "work_item_identifier_utf8_ascending_unique";

/// Every comparator this contract defines.
pub const DECLARED_COMPARATORS: &[&str] = &[
    PRESERVE_COMPARATOR,
    UTF8_ASCENDING_UNIQUE_COMPARATOR,
    REPOSITORY_PATH_COMPARATOR,
    RELATIVE_REPOSITORY_PATH_COMPARATOR,
    AGENT_IDENTIFIER_COMPARATOR,
    AUTHORIZABLE_IDENTIFIER_COMPARATOR,
    ENTRY_IDENTIFIER_COMPARATOR,
    ENTRY_PATH_COMPARATOR,
    INSTANCE_IDENTIFIER_COMPARATOR,
    JOB_IDENTIFIER_COMPARATOR,
    MODEL_IDENTIFIER_COMPARATOR,
    NAME_COMPARATOR,
    PERSISTENT_IDENTIFIER_COMPARATOR,
    QUEUE_NAME_COMPARATOR,
    SYMBOLIC_NAME_COMPARATOR,
    WORK_ITEM_IDENTIFIER_COMPARATOR,
];

/// Reason a document is not canonical.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum CanonicalFailure {
    /// The bytes are not valid UTF-8.
    #[error("a canonical document is valid UTF-8")]
    NotUnicode,
    /// The bytes begin with a byte-order mark.
    #[error("a canonical document carries no byte-order mark")]
    ByteOrderMark,
    /// The bytes carry whitespace outside a string.
    #[error("a canonical document carries no insignificant whitespace")]
    InsignificantWhitespace,
    /// The bytes are not one complete value.
    #[error("a canonical document is exactly one value")]
    NotOneValue,
    /// An object's members are not in ascending order.
    #[error("the members of a canonical object ascend by their UTF-8 bytes")]
    MembersNotAscending,
    /// An object names one member twice.
    #[error("a canonical object names each member once")]
    MemberRepeated,
    /// A string uses an escape this contract does not write.
    #[error("a canonical string escapes only the quote, the reverse solidus, and controls")]
    EscapeNotCanonical,
    /// An integer is not written minimally.
    #[error("a canonical integer carries no plus, no leading zero, and no negative zero")]
    IntegerNotMinimal,
    /// A number is not an integer.
    #[error("a canonical document carries no nonintegral number")]
    NumberNotIntegral,
    /// An array the inventory calls a set is out of order or repeats a value.
    #[error("the array at {pointer} ascends by its comparator and repeats no value")]
    ArrayNotOrdered {
        /// Pointer of the array that is wrong.
        pointer: String,
    },
    /// An array row does not carry the member its comparator orders by.
    #[error("every row of the array at {pointer} carries the member it is ordered by")]
    ArrayItemNotKeyed {
        /// Pointer of the array holding it.
        pointer: String,
    },
    /// The inventory names a comparator this contract does not define.
    #[error("the array at {pointer} names a comparator this contract does not define")]
    ComparatorUnknown {
        /// Pointer that names it.
        pointer: String,
    },
}

/// The array-order inventory for one command role.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct ArrayOrderInventory {
    /// Comparator for each array, by JSON Pointer.
    pointers: BTreeMap<String, String>,
}

impl ArrayOrderInventory {
    /// Returns the inventory `pointers` describe.
    ///
    /// # Errors
    ///
    /// Returns [`CanonicalFailure::ComparatorUnknown`] for a comparator this
    /// contract does not define, because an unrecognized comparator would
    /// otherwise be silently treated as "preserve" and a set would go unchecked.
    pub fn new(pointers: BTreeMap<String, String>) -> Result<Self, CanonicalFailure> {
        for (pointer, comparator) in &pointers {
            if !DECLARED_COMPARATORS.contains(&comparator.as_str()) {
                return Err(CanonicalFailure::ComparatorUnknown { pointer: pointer.clone() });
            }
        }
        Ok(Self { pointers })
    }

    /// Returns the comparator for the array at `pointer`.
    ///
    /// An absent pointer preserves its order. Sequences are the common case,
    /// and sorting one because nobody wrote it down would change its meaning.
    #[must_use]
    pub fn comparator(&self, pointer: &str) -> &str {
        self.pointers.get(pointer).map_or(PRESERVE_COMPARATOR, String::as_str)
    }

    /// Returns every pointer this inventory names.
    #[must_use]
    pub fn pointers(&self) -> &BTreeMap<String, String> {
        &self.pointers
    }
}

/// Writes one value as canonical bytes.
///
/// Members sort by their UTF-8 bytes, arrays keep their order, and there is no
/// insignificant whitespace anywhere. The result is what a digest is taken
/// over.
///
/// # Errors
///
/// Returns [`CanonicalFailure::NumberNotIntegral`] when the value carries a
/// number this contract cannot write.
pub fn write_canonical(value: &serde_json::Value) -> Result<String, CanonicalFailure> {
    let mut written = String::new();
    write_value(value, &mut written)?;
    Ok(written)
}

/// Writes one value into `written`.
fn write_value(value: &serde_json::Value, written: &mut String) -> Result<(), CanonicalFailure> {
    match value {
        serde_json::Value::Null => written.push_str("null"),
        serde_json::Value::Bool(true) => written.push_str("true"),
        serde_json::Value::Bool(false) => written.push_str("false"),
        serde_json::Value::Number(number) => write_number(number, written)?,
        serde_json::Value::String(text) => write_string(text, written),
        serde_json::Value::Array(items) => write_array(items, written)?,
        serde_json::Value::Object(members) => write_object(members, written)?,
    }
    Ok(())
}

/// Writes one number, which must be an integer.
fn write_number(number: &serde_json::Number, written: &mut String) -> Result<(), CanonicalFailure> {
    if let Some(value) = number.as_i64() {
        written.push_str(&value.to_string());
        return Ok(());
    }
    if let Some(value) = number.as_u64() {
        written.push_str(&value.to_string());
        return Ok(());
    }
    Err(CanonicalFailure::NumberNotIntegral)
}

/// Writes one string, escaping only what must be escaped.
///
/// Unicode is emitted directly. Escaping it would be a second spelling of the
/// same text, and this contract has exactly one spelling for everything.
fn write_string(text: &str, written: &mut String) {
    written.push('"');
    for character in text.chars() {
        match character {
            '"' => written.push_str("\\\""),
            '\\' => written.push_str("\\\\"),
            control if control.is_control() => {
                written.push_str(&format!("\\u{:04x}", control as u32));
            }
            other => written.push(other),
        }
    }
    written.push('"');
}

/// Writes one array, keeping its order.
fn write_array(items: &[serde_json::Value], written: &mut String) -> Result<(), CanonicalFailure> {
    written.push('[');
    for (position, item) in items.iter().enumerate() {
        if position > 0 {
            written.push(',');
        }
        write_value(item, written)?;
    }
    written.push(']');
    Ok(())
}

/// Writes one object, ascending by member bytes.
fn write_object(
    members: &serde_json::Map<String, serde_json::Value>,
    written: &mut String,
) -> Result<(), CanonicalFailure> {
    let mut names: Vec<&String> = members.keys().collect();
    names.sort_by(|left, right| left.as_bytes().cmp(right.as_bytes()));
    written.push('{');
    for (position, name) in names.iter().enumerate() {
        if position > 0 {
            written.push(',');
        }
        write_string(name, written);
        written.push(':');
        write_value(&members[*name], written)?;
    }
    written.push('}');
    Ok(())
}

/// Requires `bytes` to be one canonical document.
///
/// Raw bytes, checked before any parser sees them. The order of the checks is
/// the order the faults occur in: encoding, then framing, then structure.
///
/// # Errors
///
/// Returns whichever [`CanonicalFailure`] the bytes provoke first.
pub fn require_canonical_bytes(bytes: &[u8]) -> Result<serde_json::Value, CanonicalFailure> {
    /// The three bytes a UTF-8 byte-order mark occupies.
    const BYTE_ORDER_MARK: &[u8] = &[0xef, 0xbb, 0xbf];

    if bytes.starts_with(BYTE_ORDER_MARK) {
        return Err(CanonicalFailure::ByteOrderMark);
    }
    let text = std::str::from_utf8(bytes).map_err(|_| CanonicalFailure::NotUnicode)?;
    if text != text.trim() {
        return Err(CanonicalFailure::InsignificantWhitespace);
    }
    let value: serde_json::Value =
        serde_json::from_str(text).map_err(|_| CanonicalFailure::NotOneValue)?;
    require_member_order(text)?;
    if write_canonical(&value)? != text {
        return Err(CanonicalFailure::EscapeNotCanonical);
    }
    Ok(value)
}

/// Requires every object in `text` to name its members once, ascending.
///
/// Read off the raw text rather than the parsed tree, because a parser has
/// already sorted the members and dropped the repeats by the time it hands one
/// over - which is exactly the evidence that is needed.
fn require_member_order(text: &str) -> Result<(), CanonicalFailure> {
    let mut previous: Vec<Option<String>> = Vec::new();
    for (position, character) in text.char_indices() {
        match character {
            '"' => {
                let name = read_string(text, position);
                let after = position + name.written;
                if text.get(after..).is_some_and(|rest| rest.starts_with(':'))
                    && let Some(seen) = previous.last_mut()
                {
                    compare_member(seen, &name.value)?;
                }
            }
            '{' => previous.push(None),
            '}' => {
                previous.pop();
            }
            _ => (),
        }
    }
    Ok(())
}

/// One string read out of raw text.
struct ReadString {
    /// What it spells.
    value: String,
    /// How many characters it occupied, including both quotes.
    written: usize,
}

/// Reads one string beginning at `position`.
fn read_string(text: &str, position: usize) -> ReadString {
    let mut value = String::new();
    let mut written = 1;
    let mut escaped = false;
    for character in text[position + 1..].chars() {
        written += character.len_utf8();
        if escaped {
            escaped = false;
            value.push(character);
            continue;
        }
        match character {
            '\\' => escaped = true,
            '"' => break,
            other => value.push(other),
        }
    }
    ReadString { value, written }
}

/// Compares one member name against the last one seen in its object.
fn compare_member(seen: &mut Option<String>, name: &str) -> Result<(), CanonicalFailure> {
    if let Some(earlier) = seen {
        match earlier.as_bytes().cmp(name.as_bytes()) {
            std::cmp::Ordering::Less => (),
            std::cmp::Ordering::Equal => return Err(CanonicalFailure::MemberRepeated),
            std::cmp::Ordering::Greater => return Err(CanonicalFailure::MembersNotAscending),
        }
    }
    *seen = Some(name.to_owned());
    Ok(())
}

/// Requires every array in `value` to satisfy its comparator.
///
/// # Errors
///
/// Returns [`CanonicalFailure::ArrayNotOrdered`] at the first array that is a
/// set and is not sorted, or repeats a value.
pub fn require_array_order(
    value: &serde_json::Value,
    inventory: &ArrayOrderInventory,
) -> Result<(), CanonicalFailure> {
    require_array_order_at(value, inventory, "")
}

/// Requires the arrays at and below `pointer` to satisfy their comparators.
fn require_array_order_at(
    value: &serde_json::Value,
    inventory: &ArrayOrderInventory,
    pointer: &str,
) -> Result<(), CanonicalFailure> {
    match value {
        serde_json::Value::Array(items) => {
            let comparator = inventory.comparator(pointer);
            if comparator != PRESERVE_COMPARATOR {
                require_ascending_unique(items, pointer, comparator)?;
            }
            for (position, item) in items.iter().enumerate() {
                require_array_order_at(item, inventory, &format!("{pointer}/{position}"))?;
            }
        }
        serde_json::Value::Object(members) => {
            for (name, member) in members {
                require_array_order_at(member, inventory, &format!("{pointer}/{name}"))?;
            }
        }
        _ => (),
    }
    Ok(())
}

/// Requires one array to ascend strictly by its canonical bytes.
fn require_ascending_unique(
    items: &[serde_json::Value],
    pointer: &str,
    comparator: &str,
) -> Result<(), CanonicalFailure> {
    let mut previous: Option<String> = None;
    for item in items {
        let keyed = comparison_key(item, comparator)
            .ok_or_else(|| CanonicalFailure::ArrayItemNotKeyed { pointer: pointer.to_owned() })?;
        let written = write_canonical(&keyed)?;
        if let Some(earlier) = previous
            && earlier.as_bytes() >= written.as_bytes()
        {
            return Err(CanonicalFailure::ArrayNotOrdered { pointer: pointer.to_owned() });
        }
        previous = Some(written);
    }
    Ok(())
}

/// The member each object comparator orders by, in declaration order.
///
/// A comparator that named no member would order by the whole object, which is
/// the same as ordering by whichever member happens to sort first in canonical
/// bytes - a queue listing would order by its active job count rather than by
/// its queue name, and a correctly ordered page would be refused. The pairs here
/// are the same ones the committed contract states as each comparator's key,
/// and `command_schemas` compares the two: a comparator added to the contract
/// and to `DECLARED_COMPARATORS` but forgotten here would silently order by the
/// whole row again.
pub const COMPARATOR_MEMBERS: &[(&str, &[&str])] = &[
    (REPOSITORY_PATH_COMPARATOR, &["repository_path"]),
    (AGENT_IDENTIFIER_COMPARATOR, &["agent_identifier"]),
    (AUTHORIZABLE_IDENTIFIER_COMPARATOR, &["authorizable_identifier"]),
    (ENTRY_IDENTIFIER_COMPARATOR, &["entry_identifier"]),
    (ENTRY_PATH_COMPARATOR, &["entry_path"]),
    (INSTANCE_IDENTIFIER_COMPARATOR, &["instance_identifier"]),
    (JOB_IDENTIFIER_COMPARATOR, &["job_identifier"]),
    (MODEL_IDENTIFIER_COMPARATOR, &["model_identifier"]),
    (NAME_COMPARATOR, &["name"]),
    (PERSISTENT_IDENTIFIER_COMPARATOR, &["persistent_identifier"]),
    (QUEUE_NAME_COMPARATOR, &["queue_name"]),
    (SYMBOLIC_NAME_COMPARATOR, &["symbolic_name", "version"]),
    (WORK_ITEM_IDENTIFIER_COMPARATOR, &["work_item_identifier"]),
];

/// Returns the part of one item its comparator orders by.
///
/// A set of strings orders by the string. A set of objects orders by the members
/// its comparator names and by nothing else, because the rest of the object is
/// derived from those members and would make the order depend on data the caller
/// never chose. One comparator names two members, because one deployment can
/// hold two versions of one bundle and the pair is what makes a row unique.
///
/// Returns nothing when a row does not carry what it is ordered by, which is a
/// malformed row rather than a row that sorts somewhere.
fn comparison_key(item: &serde_json::Value, comparator: &str) -> Option<serde_json::Value> {
    let Some((_, members)) = COMPARATOR_MEMBERS.iter().find(|(named, _)| *named == comparator)
    else {
        return Some(item.clone());
    };
    let keyed: Vec<serde_json::Value> =
        members.iter().filter_map(|member| item.get(*member).cloned()).collect();
    // A row missing the member its comparator names cannot be ordered by what
    // the contract says orders it. Falling back to the whole row would give that
    // row a key sorting after every well-formed one, so the same malformed
    // document would be accepted or refused depending on where the row sat.
    (keyed.len() == members.len()).then_some(serde_json::Value::Array(keyed))
}

/// Returns the digest of one canonical document, in lowercase hexadecimal.
#[must_use]
pub fn canonical_digest(canonical: &str) -> String {
    use sha2::Digest as _;

    let mut hasher = sha2::Sha256::new();
    hasher.update(canonical.as_bytes());
    hasher.finalize().iter().map(|byte| format!("{byte:02x}")).collect::<Vec<String>>().join("")
}
