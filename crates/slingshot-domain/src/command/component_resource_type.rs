//! Sling resource-type spellings, which are not repository names.
//!
//! A resource type looks like a path and is not one. It has no namespace, no
//! same-name-sibling syntax, and its own byte and segment bounds, so giving it
//! the repository-name grammar would quietly accept spellings a Sling
//! resolution would never resolve - and would let a caller pass a resource type
//! where a path belongs, or the reverse.

use serde::{Deserialize, Serialize};
use unicode_normalization::UnicodeNormalization;
use unicode_normalization::is_nfc_quick;

use crate::command::command_identity::CommandContract;
use crate::command::repository_path::PathFailure;

/// Separator between two resource-type segments.
const SEGMENT_SEPARATOR: char = '/';

/// Characters a resource-type segment may never contain.
const REFUSED_CHARACTERS: &[char] = &[':', '[', ']', '*', '|'];

/// One Sling resource type, absolute or relative.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct ComponentResourceType {
    /// The exact accepted spelling.
    value: String,
}

impl ComponentResourceType {
    /// Returns the role this value is built as.
    #[must_use]
    pub fn role() -> &'static str {
        "component resource type"
    }

    /// Validates one resource-type spelling.
    ///
    /// # Errors
    ///
    /// Returns [`PathFailure`] when the spelling is empty, too long, not
    /// already in normalization form C, ends with a separator, names more
    /// segments than the contract allows, or carries a segment that is empty, a
    /// dot form, space-padded, or holds a refused character.
    pub fn parse(resource_type: &str) -> Result<Self, PathFailure> {
        let contract = CommandContract::embedded();
        let refuse = |field| PathFailure::at(Self::role(), field);
        let bound = contract.limit("maximum_component_resource_type_bytes");
        if resource_type.is_empty()
            || u64::try_from(resource_type.len()).unwrap_or(u64::MAX) > bound
        {
            return Err(refuse("bytes"));
        }
        if !is_normalized(resource_type) {
            return Err(refuse("normalization"));
        }
        let body = resource_type.strip_prefix(SEGMENT_SEPARATOR).unwrap_or(resource_type);
        let segments: Vec<&str> = body.split(SEGMENT_SEPARATOR).collect();
        let maximum = contract.limit("maximum_component_resource_type_segments");
        if u64::try_from(segments.len()).unwrap_or(u64::MAX) > maximum {
            return Err(refuse("segments"));
        }
        for segment in segments {
            accept_segment(segment)?;
        }
        Ok(Self { value: resource_type.to_owned() })
    }

    /// Returns the spelling exactly as it was accepted.
    #[must_use]
    pub fn as_text(&self) -> &str {
        &self.value
    }

    /// Reports whether this spelling is rooted at the resolver root.
    #[must_use]
    pub fn is_absolute(&self) -> bool {
        self.value.starts_with(SEGMENT_SEPARATOR)
    }
}

impl TryFrom<String> for ComponentResourceType {
    type Error = PathFailure;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        Self::parse(&value)
    }
}

impl From<ComponentResourceType> for String {
    fn from(value: ComponentResourceType) -> Self {
        value.value
    }
}

impl ::core::fmt::Display for ComponentResourceType {
    fn fmt(&self, formatter: &mut ::core::fmt::Formatter<'_>) -> ::core::fmt::Result {
        formatter.write_str(&self.value)
    }
}

/// Requires one segment to be a name a resolver could act on.
fn accept_segment(segment: &str) -> Result<(), PathFailure> {
    let refuse = || PathFailure::at(ComponentResourceType::role(), "segment");
    if segment.is_empty() || segment == "." || segment == ".." {
        return Err(refuse());
    }
    if segment.starts_with(' ') || segment.ends_with(' ') {
        return Err(refuse());
    }
    let usable = segment.chars().all(|character| {
        !REFUSED_CHARACTERS.contains(&character) && character > '\u{1f}' && character != '\u{7f}'
    });
    if usable {
        return Ok(());
    }
    Err(refuse())
}

/// Reports whether one value is already in normalization form C.
fn is_normalized(value: &str) -> bool {
    match is_nfc_quick(value.chars()) {
        unicode_normalization::IsNormalized::Yes => true,
        unicode_normalization::IsNormalized::No => false,
        unicode_normalization::IsNormalized::Maybe => value.nfc().eq(value.chars()),
    }
}
