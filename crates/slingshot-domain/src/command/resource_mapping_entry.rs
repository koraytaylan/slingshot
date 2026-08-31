//! What one mapping entry is, in the vocabulary all three mapping commands use.
//!
//! A listing and a trace describe the same entries, and describing them twice
//! would make the two impossible to compare - which is the only reason anybody
//! asks for both.
//!
//! # A pattern is opaque text
//!
//! Mapping entries carry regular expressions, and this contract does not parse
//! one. It bounds the pattern, refuses controls, and otherwise treats it as the
//! author's business, because a second regular-expression grammar here would
//! accept and refuse different spellings than the one that actually runs.
//!
//! # A status code belongs to a redirect and to nothing else
//!
//! An entry that redirects has a status; an entry that maps internally does not.
//! Carrying the member on both would make a caller ask which one to believe, so
//! it is required on one kind and refused on the others.

use serde::de::Error as _;
use serde::{Deserialize, Serialize};

use crate::command::command_identity::CommandContract;
use crate::command::repository_path::{PathFailure, RepositoryPath, accept_within, address_value};

/// Kinds a mapping entry can be.
pub const RESOURCE_MAPPING_KIND_COUNT: usize = 4;

/// Why a mapping entry is not one this contract can carry.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum ResourceMappingFailure {
    /// An entry names more replacements than the contract allows.
    #[error("a mapping entry is within the replacement bound its contract declares")]
    TooManyReplacements,
    /// A redirecting entry names no status code, or another kind names one.
    #[error("a status code belongs to a redirecting entry and to no other kind")]
    StatusCodeMisplaced,
    /// A result echoes a request other than the one it answers.
    #[error("a mapping result echoes the request it answers")]
    NotThisRequest,
    /// A trace is present when none was asked for, or absent when one was.
    #[error("a trace is present exactly when the request asked for one")]
    TraceMisplaced,
    /// A trace names more entries than the contract allows.
    #[error("a trace is within the entry bound its contract declares")]
    TraceTooLong,
}

address_value!(
    /// The expression one mapping entry matches with.
    ResourceMappingPattern,
    "resource mapping pattern"
);

address_value!(
    /// The address one request arrived at, or one mapping would emit.
    RequestAddress,
    "request address"
);

impl ResourceMappingPattern {
    /// Validates one mapping pattern.
    ///
    /// # Errors
    ///
    /// Returns [`PathFailure`] when the pattern is empty, longer than the
    /// contract allows, not already in normalization form C, or carries a
    /// control.
    pub fn parse(pattern: &str) -> Result<Self, PathFailure> {
        let bound = CommandContract::embedded().limit("maximum_resource_mapping_pattern_bytes");
        accept_within(pattern, bound, Self::role(), "bytes")?;
        if pattern.chars().any(char::is_control) {
            return Err(PathFailure::at(Self::role(), "character"));
        }
        Ok(Self::from_accepted(pattern))
    }
}

impl RequestAddress {
    /// Validates one request address.
    ///
    /// # Errors
    ///
    /// Returns [`PathFailure`] when the address is empty, longer than the
    /// contract allows, not already in normalization form C, carries a control
    /// or whitespace, or names neither a scheme nor an absolute path.
    pub fn parse(address: &str) -> Result<Self, PathFailure> {
        let bound = CommandContract::embedded().limit("maximum_request_address_bytes");
        accept_within(address, bound, Self::role(), "bytes")?;
        let refuse = |field| PathFailure::at(Self::role(), field);
        if address.chars().any(|character| character.is_control() || character.is_whitespace()) {
            return Err(refuse("character"));
        }
        let absolute = address.starts_with('/');
        let schemed = address.split_once("://").is_some_and(|(scheme, rest)| {
            !scheme.is_empty()
                && scheme.chars().all(|character| character.is_ascii_alphanumeric())
                && !rest.is_empty()
        });
        if absolute || schemed { Ok(Self::from_accepted(address)) } else { Err(refuse("form")) }
    }
}

/// What one mapping entry does with what it matches.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ResourceMappingKind {
    /// Another name the same resource answers to.
    Alias,
    /// A resolution that continues inside the author.
    InternalRedirect,
    /// An outward address the author emits for a resource.
    Map,
    /// A response that sends the client somewhere else.
    Redirect,
}

impl ResourceMappingKind {
    /// Returns every kind, in the order they are written.
    #[must_use]
    pub fn every() -> [Self; RESOURCE_MAPPING_KIND_COUNT] {
        [Self::Alias, Self::InternalRedirect, Self::Map, Self::Redirect]
    }

    /// Reports whether an entry of this kind answers with a status code.
    #[must_use]
    pub fn redirects(self) -> bool {
        matches!(self, Self::Redirect)
    }
}

/// One entry of the effective resource mapping.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ResourceMappingEntry {
    /// Where the entry itself lives.
    pub entry_path: RepositoryPath,
    /// What the entry does.
    pub kind: ResourceMappingKind,
    /// What it matches.
    pub pattern: ResourceMappingPattern,
    /// What it maps to, in order.
    pub replacements: Vec<String>,
    /// The status a redirecting entry answers with.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub status_code: Option<u16>,
}

impl ResourceMappingEntry {
    /// Returns the entry these facts describe.
    ///
    /// # Errors
    ///
    /// Returns [`ResourceMappingFailure::TooManyReplacements`] above the
    /// contract's replacement bound, and
    /// [`ResourceMappingFailure::StatusCodeMisplaced`] when a redirecting entry
    /// names no status or another kind names one.
    pub fn new(
        entry_path: RepositoryPath,
        kind: ResourceMappingKind,
        pattern: ResourceMappingPattern,
        replacements: Vec<String>,
        status_code: Option<u16>,
    ) -> Result<Self, ResourceMappingFailure> {
        let bound = CommandContract::embedded().limit("maximum_resource_mapping_replacements");
        if u64::try_from(replacements.len()).unwrap_or(u64::MAX) > bound {
            return Err(ResourceMappingFailure::TooManyReplacements);
        }
        if kind.redirects() != status_code.is_some() {
            return Err(ResourceMappingFailure::StatusCodeMisplaced);
        }
        Ok(Self { entry_path, kind, pattern, replacements, status_code })
    }
}

/// One entry exactly as it is written on the wire.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct EntryDocument {
    /// Where the entry itself lives.
    entry_path: RepositoryPath,
    /// What the entry does.
    kind: ResourceMappingKind,
    /// What it matches.
    pattern: ResourceMappingPattern,
    /// What it maps to, in order.
    replacements: Vec<String>,
    /// The status a redirecting entry answers with.
    #[serde(default)]
    status_code: Option<u16>,
}

impl<'de> Deserialize<'de> for ResourceMappingEntry {
    fn deserialize<Source: serde::Deserializer<'de>>(
        deserializer: Source,
    ) -> Result<Self, Source::Error> {
        let document = EntryDocument::deserialize(deserializer)?;
        Self::new(
            document.entry_path,
            document.kind,
            document.pattern,
            document.replacements,
            document.status_code,
        )
        .map_err(Source::Error::custom)
    }
}
