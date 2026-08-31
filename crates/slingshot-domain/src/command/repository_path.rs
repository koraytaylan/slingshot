//! Repository names, segments, paths, and property paths.
//!
//! Every command addresses repository content, so one address has to have one
//! spelling before any transport sees it. Two spellings of the same node would
//! mean two identities for one thing; a spelling that looks like a traversal
//! would mean a caller could address content the command was never pointed at.
//!
//! So the roles are separate types rather than one string with rules applied at
//! each use. A property name is not a path segment, a page name is not a
//! property name, and a query path is not a mutation target. Each is built by a
//! constructor that validates it, and no operation here concatenates text that
//! has not been through one.
//!
//! Input must already be in normalization form C. Normalizing it here would
//! mean accepting two spellings and silently making them one, which is the same
//! ambiguity written in a friendlier way; a caller that has not normalized its
//! input is told so instead.

use serde::{Deserialize, Serialize};
use unicode_normalization::UnicodeNormalization;
use unicode_normalization::is_nfc_quick;

use crate::command::command_identity::CommandContract;

/// Separator between two path segments.
const PATH_SEPARATOR: char = '/';

/// Separator between a namespace prefix and a local name.
const NAMESPACE_SEPARATOR: char = ':';

/// Opening of a same-name-sibling suffix.
const SIBLING_OPENING: char = '[';

/// Closing of a same-name-sibling suffix.
const SIBLING_CLOSING: char = ']';

/// Smallest same-name-sibling index that is written down.
///
/// The first sibling has no suffix, so a suffix that named it would be a second
/// spelling of one address.
const SMALLEST_WRITTEN_SIBLING_INDEX: u32 = 2;

/// Characters a local name may never contain.
const REFUSED_LOCAL_CHARACTERS: &[char] =
    &[PATH_SEPARATOR, NAMESPACE_SEPARATOR, SIBLING_OPENING, SIBLING_CLOSING, '*', '|'];

/// Characters a namespace prefix may contain after its first.
const PREFIX_PUNCTUATION: &str = "_-.";

/// Reason a repository address could not be built.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[error("{field} is not a valid {role}")]
pub struct PathFailure {
    /// Role the value was being built as.
    pub role: &'static str,
    /// Part of the value that was refused.
    pub field: &'static str,
}

impl PathFailure {
    /// Returns one failure in `role` at `field`.
    #[must_use]
    pub fn at(role: &'static str, field: &'static str) -> Self {
        Self { role, field }
    }
}

/// Declares one validated address wrapper over its exact accepted spelling.
///
/// Every role shares the same shape - construct by validating, keep the exact
/// bytes, render them back - and differs only in what it accepts, so the shape
/// is written once and each role supplies its own rule. The operational
/// vocabulary the later families address - an authorizable, a bundle, a
/// workflow instance, a job topic - is the same shape again, so the macro is
/// visible to the whole family and names its own failure type absolutely,
/// rather than being copied into each module that needs one.
macro_rules! address_value {
    ($(#[$attribute:meta])* $name:ident, $role:literal) => {
        $(#[$attribute])*
        #[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
        #[serde(try_from = "String", into = "String")]
        pub struct $name {
            /// The exact accepted spelling.
            value: String,
        }

        impl $name {
            /// Returns the value exactly as it was accepted.
            #[must_use]
            pub fn as_text(&self) -> &str {
                &self.value
            }

            /// Returns the role this value was built as.
            #[must_use]
            pub fn role() -> &'static str {
                $role
            }

            /// Returns the wrapper around a spelling this role has accepted.
            ///
            /// Reachable only inside this crate and only from the `parse` that
            /// validated the bytes, because a wrapper is a claim that the
            /// validation ran.
            pub(crate) fn from_accepted(value: &str) -> Self {
                Self { value: value.to_owned() }
            }
        }

        impl TryFrom<String> for $name {
            type Error = $crate::command::repository_path::PathFailure;

            fn try_from(value: String) -> Result<Self, Self::Error> {
                Self::parse(&value)
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

pub(crate) use address_value;

address_value!(
    /// One repository item name, optionally namespace-qualified.
    RepositoryName,
    "repository name"
);

address_value!(
    /// One path segment: a repository name and an optional sibling index.
    RepositoryPathSegment,
    "repository path segment"
);

address_value!(
    /// One absolute repository path.
    RepositoryPath,
    "repository path"
);

address_value!(
    /// One relative repository path, which never begins with a separator.
    RepositoryRelativePath,
    "repository relative path"
);

address_value!(
    /// The name of one property directly on a node.
    PropertyName,
    "property name"
);

address_value!(
    /// The name of one page a command may create.
    PageName,
    "page name"
);

address_value!(
    /// The name of one component a command may add.
    ComponentName,
    "component name"
);

address_value!(
    /// The name of one primary node type, which is always qualified.
    PrimaryNodeTypeName,
    "primary node type name"
);

address_value!(
    /// A query-only path to one property below a candidate node.
    RelativePropertyPath,
    "relative property path"
);

impl RepositoryName {
    /// Validates one repository name.
    ///
    /// # Errors
    ///
    /// Returns [`PathFailure`] when the name is empty, too long, not already in
    /// normalization form C, spells more than one namespace separator, uses a
    /// refused character, is a dot or dot-dot, or has a leading or trailing
    /// ASCII space.
    pub fn parse(name: &str) -> Result<Self, PathFailure> {
        let bound = CommandContract::embedded().limit("maximum_repository_name_bytes");
        accept_within(name, bound, Self::role(), "bytes")?;
        let (prefix, local) = match name.split_once(NAMESPACE_SEPARATOR) {
            Some((prefix, local)) => (Some(prefix), local),
            None => (None, name),
        };
        if let Some(prefix) = prefix {
            accept_prefix(prefix)?;
        }
        accept_local(local)?;
        Ok(Self::from_accepted(name))
    }

    /// Reports whether this name carries a namespace prefix.
    #[must_use]
    pub fn is_qualified(&self) -> bool {
        self.value.contains(NAMESPACE_SEPARATOR)
    }
}

impl RepositoryPathSegment {
    /// Validates one path segment.
    ///
    /// # Errors
    ///
    /// Returns [`PathFailure`] when the name part is invalid, or when a
    /// same-name-sibling suffix is present and is not a canonical index of two
    /// or more within the contract's maximum.
    pub fn parse(segment: &str) -> Result<Self, PathFailure> {
        let refuse = || PathFailure::at(Self::role(), "sibling index");
        let Some(opening) = segment.find(SIBLING_OPENING) else {
            RepositoryName::parse(segment)?;
            return Ok(Self::from_accepted(segment));
        };
        let (name, suffix) = segment.split_at(opening);
        RepositoryName::parse(name)?;
        let index = suffix
            .strip_prefix(SIBLING_OPENING)
            .and_then(|rest| rest.strip_suffix(SIBLING_CLOSING))
            .ok_or_else(refuse)?;
        accept_sibling_index(index)?;
        Ok(Self::from_accepted(segment))
    }

    /// Returns the name part of this segment.
    ///
    /// # Panics
    ///
    /// Panics when the segment was not built by [`RepositoryPathSegment::parse`],
    /// which no caller can arrange.
    #[must_use]
    pub fn name(&self) -> RepositoryName {
        let name = self.value.split(SIBLING_OPENING).next().unwrap_or(&self.value);
        RepositoryName::parse(name).expect("an accepted segment carries an accepted name")
    }
}

impl RepositoryPath {
    /// The one path that has no parent and no name.
    pub const ROOT: &'static str = "/";

    /// Validates one absolute repository path.
    ///
    /// # Errors
    ///
    /// Returns [`PathFailure`] when the path is empty, too long, does not begin
    /// with one separator, ends with one, names more segments than the contract
    /// allows, or carries a segment that is not valid on its own.
    pub fn parse(path: &str) -> Result<Self, PathFailure> {
        let contract = CommandContract::embedded();
        accept_within(
            path,
            contract.limit("maximum_repository_path_bytes"),
            Self::role(),
            "bytes",
        )?;
        if path == Self::ROOT {
            return Ok(Self::from_accepted(path));
        }
        let Some(body) = path.strip_prefix(PATH_SEPARATOR) else {
            return Err(PathFailure::at(Self::role(), "leading separator"));
        };
        accept_segments(body, Self::role())?;
        Ok(Self::from_accepted(path))
    }

    /// Reports whether this path is the root.
    #[must_use]
    pub fn is_root(&self) -> bool {
        self.value == Self::ROOT
    }

    /// Returns the segments of this path, in order.
    #[must_use]
    pub fn segments(&self) -> Vec<RepositoryPathSegment> {
        if self.is_root() {
            return Vec::new();
        }
        self.value
            .trim_start_matches(PATH_SEPARATOR)
            .split(PATH_SEPARATOR)
            .map(|segment| RepositoryPathSegment { value: segment.to_owned() })
            .collect()
    }

    /// Returns the path this one is directly below, if any.
    ///
    /// The root has no parent, which is a fact about the repository rather than
    /// a failure, so it answers with nothing.
    #[must_use]
    pub fn parent(&self) -> Option<Self> {
        if self.is_root() {
            return None;
        }
        let (head, _) = self.value.rsplit_once(PATH_SEPARATOR)?;
        if head.is_empty() {
            return Some(Self::from_accepted(Self::ROOT));
        }
        Some(Self::from_accepted(head))
    }

    /// Returns the path of the child `segment` addresses.
    ///
    /// # Errors
    ///
    /// Returns [`PathFailure`] when the result would exceed the contract's byte
    /// or segment bounds.
    pub fn address_child(&self, segment: &RepositoryPathSegment) -> Result<Self, PathFailure> {
        let joined = if self.is_root() {
            format!("{PATH_SEPARATOR}{segment}")
        } else {
            format!("{}{PATH_SEPARATOR}{segment}", self.value)
        };
        Self::parse(&joined)
    }

    /// Returns the path of the child `name` would create.
    ///
    /// A creatable child is a plain name: it can carry no sibling index,
    /// because a caller creating a node does not get to choose which sibling it
    /// becomes.
    ///
    /// # Errors
    ///
    /// Returns [`PathFailure`] when the result would exceed the contract's byte
    /// or segment bounds.
    pub fn creatable_child(&self, name: &RepositoryName) -> Result<Self, PathFailure> {
        let segment = RepositoryPathSegment::parse(name.as_text())?;
        self.address_child(&segment)
    }
}

impl RepositoryRelativePath {
    /// Validates one relative repository path.
    ///
    /// # Errors
    ///
    /// Returns [`PathFailure`] when the path is empty, too long, begins or ends
    /// with a separator, names more segments than the contract allows, or
    /// carries a segment that is not valid on its own.
    pub fn parse(path: &str) -> Result<Self, PathFailure> {
        let bound = CommandContract::embedded().limit("maximum_repository_relative_path_bytes");
        accept_within(path, bound, Self::role(), "bytes")?;
        if path.starts_with(PATH_SEPARATOR) {
            return Err(PathFailure::at(Self::role(), "leading separator"));
        }
        accept_segments(path, Self::role())?;
        Ok(Self::from_accepted(path))
    }
}

impl PropertyName {
    /// Validates one property name.
    ///
    /// # Errors
    ///
    /// Returns [`PathFailure`] when the name is not a valid repository name, or
    /// carries a same-name-sibling suffix, which a property never has.
    pub fn parse(name: &str) -> Result<Self, PathFailure> {
        let bound = CommandContract::embedded().limit("maximum_property_name_bytes");
        accept_within(name, bound, Self::role(), "bytes")?;
        RepositoryName::parse(name).map_err(|_| PathFailure::at(Self::role(), "name"))?;
        if name.contains(SIBLING_OPENING) {
            return Err(PathFailure::at(Self::role(), "sibling index"));
        }
        Ok(Self::from_accepted(name))
    }
}

impl PageName {
    /// Validates one creatable page name.
    ///
    /// # Errors
    ///
    /// Returns [`PathFailure`] when the name is qualified, carries a sibling
    /// suffix, exceeds its own bound, or is not a valid repository name.
    pub fn parse(name: &str) -> Result<Self, PathFailure> {
        let bound = CommandContract::embedded().limit("maximum_page_name_bytes");
        accept_creatable(name, bound, Self::role())?;
        Ok(Self::from_accepted(name))
    }
}

impl ComponentName {
    /// Validates one creatable component name.
    ///
    /// # Errors
    ///
    /// Returns [`PathFailure`] when the name is qualified, carries a sibling
    /// suffix, exceeds its own bound, or is not a valid repository name.
    pub fn parse(name: &str) -> Result<Self, PathFailure> {
        let bound = CommandContract::embedded().limit("maximum_component_name_bytes");
        accept_creatable(name, bound, Self::role())?;
        Ok(Self::from_accepted(name))
    }
}

impl PrimaryNodeTypeName {
    /// Validates one primary node type name.
    ///
    /// # Errors
    ///
    /// Returns [`PathFailure`] when the name is not a qualified repository
    /// name, which every node type is.
    pub fn parse(name: &str) -> Result<Self, PathFailure> {
        let bound = CommandContract::embedded().limit("maximum_primary_node_type_name_bytes");
        accept_within(name, bound, Self::role(), "bytes")?;
        let parsed =
            RepositoryName::parse(name).map_err(|_| PathFailure::at(Self::role(), "name"))?;
        if !parsed.is_qualified() {
            return Err(PathFailure::at(Self::role(), "namespace"));
        }
        Ok(Self::from_accepted(name))
    }
}

impl RelativePropertyPath {
    /// Validates one query-only property path.
    ///
    /// # Errors
    ///
    /// Returns [`PathFailure`] when the path is empty, too long, begins or ends
    /// with a separator, carries an invalid child address, or does not end with
    /// exactly one property name.
    pub fn parse(path: &str) -> Result<Self, PathFailure> {
        let bound = CommandContract::embedded().limit("maximum_relative_property_path_bytes");
        accept_within(path, bound, Self::role(), "bytes")?;
        if path.starts_with(PATH_SEPARATOR) {
            return Err(PathFailure::at(Self::role(), "leading separator"));
        }
        let segments: Vec<&str> = path.split(PATH_SEPARATOR).collect();
        let (name, addresses) =
            segments.split_last().ok_or(PathFailure::at(Self::role(), "name"))?;
        for address in addresses {
            RepositoryPathSegment::parse(address)
                .map_err(|_| PathFailure::at(Self::role(), "child address"))?;
        }
        PropertyName::parse(name).map_err(|_| PathFailure::at(Self::role(), "name"))?;
        Ok(Self::from_accepted(path))
    }
}

/// One property address, absolute or relative, as a repository value.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub enum RepositoryPropertyPath {
    /// An address rooted at the repository root.
    Absolute(RepositoryPath),
    /// An address relative to some node.
    Relative(RepositoryRelativePath),
}

impl RepositoryPropertyPath {
    /// Validates one property address in either form.
    ///
    /// # Errors
    ///
    /// Returns [`PathFailure`] when neither form accepts the spelling.
    pub fn parse(path: &str) -> Result<Self, PathFailure> {
        if path.starts_with(PATH_SEPARATOR) {
            return RepositoryPath::parse(path).map(Self::Absolute);
        }
        RepositoryRelativePath::parse(path).map(Self::Relative)
    }

    /// Returns the address exactly as it was accepted.
    #[must_use]
    pub fn as_text(&self) -> &str {
        match self {
            Self::Absolute(path) => path.as_text(),
            Self::Relative(path) => path.as_text(),
        }
    }
}

impl TryFrom<String> for RepositoryPropertyPath {
    type Error = PathFailure;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        Self::parse(&value)
    }
}

impl From<RepositoryPropertyPath> for String {
    fn from(value: RepositoryPropertyPath) -> Self {
        value.as_text().to_owned()
    }
}

impl ::core::fmt::Display for RepositoryPropertyPath {
    fn fmt(&self, formatter: &mut ::core::fmt::Formatter<'_>) -> ::core::fmt::Result {
        formatter.write_str(self.as_text())
    }
}

/// Requires one opaque identifier body to be addressable.
///
/// An opaque identifier is whatever an author minted, so the rule is narrow: it
/// refuses only the two things that make a value unusable as an address rather
/// than merely unusual - a control, which no transport agrees how to carry, and
/// an edge space, which every renderer disagrees about keeping.
pub(crate) fn accept_opaque_body(value: &str, role: &'static str) -> Result<(), PathFailure> {
    let refuse = |field| PathFailure::at(role, field);
    if value.starts_with(' ') || value.ends_with(' ') {
        return Err(refuse("space"));
    }
    if value.chars().any(char::is_control) {
        return Err(refuse("character"));
    }
    Ok(())
}

/// Requires one value to be nonempty, within `bound`, and already normalized.
pub(crate) fn accept_within(
    value: &str,
    bound: u64,
    role: &'static str,
    field: &'static str,
) -> Result<(), PathFailure> {
    if value.is_empty() || u64::try_from(value.len()).unwrap_or(u64::MAX) > bound {
        return Err(PathFailure::at(role, field));
    }
    if !is_normalized(value) {
        return Err(PathFailure::at(role, "normalization"));
    }
    Ok(())
}

/// Reports whether one value is already in normalization form C.
fn is_normalized(value: &str) -> bool {
    match is_nfc_quick(value.chars()) {
        unicode_normalization::IsNormalized::Yes => true,
        unicode_normalization::IsNormalized::No => false,
        unicode_normalization::IsNormalized::Maybe => value.nfc().eq(value.chars()),
    }
}

/// Requires one namespace prefix to be the ASCII spelling a prefix has.
fn accept_prefix(prefix: &str) -> Result<(), PathFailure> {
    let refuse = || PathFailure::at(RepositoryName::role(), "namespace prefix");
    let mut characters = prefix.chars();
    let opens = characters.next().is_some_and(|first| first.is_ascii_alphabetic() || first == '_');
    let usable = opens
        && characters.all(|character| {
            character.is_ascii_alphanumeric() || PREFIX_PUNCTUATION.contains(character)
        });
    if usable {
        return Ok(());
    }
    Err(refuse())
}

/// Requires one local name to carry no character that would make it ambiguous.
fn accept_local(local: &str) -> Result<(), PathFailure> {
    let refuse = || PathFailure::at(RepositoryName::role(), "local name");
    if local.is_empty() || local == "." || local == ".." {
        return Err(refuse());
    }
    if local.starts_with(' ') || local.ends_with(' ') {
        return Err(refuse());
    }
    let usable = local.chars().all(|character| {
        !REFUSED_LOCAL_CHARACTERS.contains(&character) && !is_refused_control(character)
    });
    if usable {
        return Ok(());
    }
    Err(refuse())
}

/// Reports whether one character is a control this grammar refuses.
fn is_refused_control(character: char) -> bool {
    character <= '\u{1f}' || character == '\u{7f}'
}

/// Requires one same-name-sibling index to be canonical and in range.
fn accept_sibling_index(index: &str) -> Result<(), PathFailure> {
    let refuse = || PathFailure::at(RepositoryPathSegment::role(), "sibling index");
    if index.is_empty()
        || index.starts_with('0')
        || !index.bytes().all(|byte| byte.is_ascii_digit())
    {
        return Err(refuse());
    }
    let maximum = CommandContract::embedded().limit("maximum_same_name_sibling_index");
    let value: u64 = index.parse().map_err(|_| refuse())?;
    if value < u64::from(SMALLEST_WRITTEN_SIBLING_INDEX) || value > maximum {
        return Err(refuse());
    }
    Ok(())
}

/// Requires one separator-joined body to be valid segments within the bound.
fn accept_segments(body: &str, role: &'static str) -> Result<(), PathFailure> {
    let segments: Vec<&str> = body.split(PATH_SEPARATOR).collect();
    let maximum = CommandContract::embedded().limit("maximum_repository_path_segments");
    if u64::try_from(segments.len()).unwrap_or(u64::MAX) > maximum {
        return Err(PathFailure::at(role, "segments"));
    }
    for segment in segments {
        RepositoryPathSegment::parse(segment).map_err(|_| PathFailure::at(role, "segment"))?;
    }
    Ok(())
}

/// Requires one creatable child name to be unqualified and suffix-free.
fn accept_creatable(name: &str, bound: u64, role: &'static str) -> Result<(), PathFailure> {
    accept_within(name, bound, role, "bytes")?;
    let parsed = RepositoryName::parse(name).map_err(|_| PathFailure::at(role, "name"))?;
    if parsed.is_qualified() {
        return Err(PathFailure::at(role, "namespace"));
    }
    if name.contains(SIBLING_OPENING) {
        return Err(PathFailure::at(role, "sibling index"));
    }
    Ok(())
}
