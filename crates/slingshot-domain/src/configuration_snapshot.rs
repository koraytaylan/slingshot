//! Commit inventory of the configuration root.
//!
//! `configuration-snapshot.toml` is what makes a set of separately written
//! files one generation. It lists every profile, the optional selection, and
//! every credential or certificate those profiles reach, each with the exact
//! digest of its bytes, and a writer publishes it only after every listed
//! source is in place. A reader that accepts an inventory has accepted a
//! complete commit or nothing.
//!
//! This module owns the typed inventory and the two value objects it is built
//! from: a root-contained portable reference, and a source digest. Both bounds
//! and both grammars come from the contract manifest, and the digest is
//! deliberately unrenderable: a source can hold a low-entropy secret, so its
//! digest is secret-adjacent and belongs only inside the private inventory it
//! was published in.

use serde::Deserialize;

use crate::profile_authentication_contract::{
    ConfigurationFailureCode, ProfileAuthenticationContract, narrow_limit,
};

/// Separator every root-relative reference uses, on every platform.
const REFERENCE_SEPARATOR: char = '/';

/// Bytes a rendered source digest occupies.
const RENDERED_DIGEST_BYTES: usize = 64;

/// Bytes a raw source digest occupies.
const RAW_DIGEST_BYTES: usize = 32;

/// Radix a rendered source digest is written in.
const DIGEST_RADIX: u32 = 16;

/// Rendered characters one raw digest byte occupies.
const RENDERED_BYTES_PER_DIGEST_BYTE: usize = 2;

/// Reason a commit inventory or one of its values was refused.
///
/// The failure carries the contract's stable code and a structural location
/// drawn from the manifest's member vocabulary. It never carries the source
/// reference, the digest, the offending bytes, or a parser excerpt, because a
/// configuration source can hold a secret anywhere in it.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("{code} at {structural_location}")]
pub struct ConfigurationSnapshotFailure {
    /// Stable code from the contract registry.
    pub code: ConfigurationFailureCode,
    /// Manifest member vocabulary naming where the failure was found.
    pub structural_location: &'static str,
}

impl ConfigurationSnapshotFailure {
    /// Returns one failure at a named structural location.
    #[must_use]
    pub fn at(code: ConfigurationFailureCode, structural_location: &'static str) -> Self {
        Self { code, structural_location }
    }
}

/// A root-contained reference to one configuration source.
///
/// The spelling is the same on every platform: solidus-separated components
/// from the manifest grammar, no leading or trailing separator, and no way to
/// name anything above the configuration root. A backslash, an empty component,
/// a dot or dot-dot component, a drive or uniform-naming-convention prefix, and
/// a trailing separator all fail the component grammar rather than being
/// normalized away, because normalizing a reference is how one spelling becomes
/// two.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ConfigurationReference {
    /// The validated reference, exactly as it was written.
    reference: String,
}

impl ConfigurationReference {
    /// Validates one root-relative reference against the contract.
    ///
    /// # Errors
    ///
    /// Returns [`ConfigurationFailureCode::ConfigurationReferenceInvalid`] when
    /// the reference is empty, exceeds its byte or component bounds, or holds a
    /// component the manifest grammar does not accept.
    pub fn parse(reference: &str) -> Result<Self, ConfigurationSnapshotFailure> {
        let limits = &ProfileAuthenticationContract::embedded().limits;
        let refuse = || {
            ConfigurationSnapshotFailure::at(
                ConfigurationFailureCode::ConfigurationReferenceInvalid,
                "reference",
            )
        };
        if reference.is_empty()
            || reference.len() > narrow_limit(limits.maximum_configuration_reference_bytes)
        {
            return Err(refuse());
        }
        let components: Vec<&str> = reference.split(REFERENCE_SEPARATOR).collect();
        if components.len() > narrow_limit(limits.maximum_configuration_reference_components) {
            return Err(refuse());
        }
        let component_bytes = narrow_limit(limits.maximum_configuration_reference_component_bytes);
        if !components.iter().all(|component| is_portable_component(component, component_bytes)) {
            return Err(refuse());
        }
        Ok(Self { reference: reference.to_owned() })
    }

    /// Returns the reference exactly as it was written.
    #[must_use]
    pub fn as_text(&self) -> &str {
        &self.reference
    }

    /// Returns the reference's components, in order.
    pub fn components(&self) -> impl Iterator<Item = &str> {
        self.reference.split(REFERENCE_SEPARATOR)
    }
}

impl ::core::fmt::Display for ConfigurationReference {
    fn fmt(&self, formatter: &mut ::core::fmt::Formatter<'_>) -> ::core::fmt::Result {
        formatter.write_str(&self.reference)
    }
}

/// The exact digest of one configuration source's bytes.
///
/// A profile or credential source can hold a low-entropy secret, so its digest
/// identifies that secret to anyone who can enumerate the candidates. The value
/// therefore renders as a fixed redaction, carries no hexadecimal accessor, and
/// exists only to be compared with a freshly computed digest of the same
/// source inside the private commit inventory that published it.
#[derive(Clone, PartialEq, Eq)]
pub struct SourceDigest {
    /// The raw digest bytes.
    digest: [u8; RAW_DIGEST_BYTES],
}

impl SourceDigest {
    /// Parses one digest from its lowercase hexadecimal rendering.
    ///
    /// # Errors
    ///
    /// Returns [`ConfigurationFailureCode::ConfigurationValueInvalid`] when the
    /// rendering is not exactly the manifest's lowercase hexadecimal grammar.
    pub fn parse(rendered: &str) -> Result<Self, ConfigurationSnapshotFailure> {
        let refuse = || {
            ConfigurationSnapshotFailure::at(
                ConfigurationFailureCode::ConfigurationValueInvalid,
                "sha256",
            )
        };
        if rendered.len() != RENDERED_DIGEST_BYTES
            || !rendered.bytes().all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
        {
            return Err(refuse());
        }
        let mut digest = [0; RAW_DIGEST_BYTES];
        let (pairs, _) = rendered.as_bytes().as_chunks::<RENDERED_BYTES_PER_DIGEST_BYTE>();
        for (slot, pair) in digest.iter_mut().zip(pairs) {
            let text = core::str::from_utf8(pair).map_err(|_| refuse())?;
            *slot = u8::from_str_radix(text, DIGEST_RADIX).map_err(|_| refuse())?;
        }
        Ok(Self { digest })
    }

    /// Returns one digest from bytes a hash function already produced.
    #[must_use]
    pub fn from_raw(digest: [u8; RAW_DIGEST_BYTES]) -> Self {
        Self { digest }
    }

    /// Reports whether this digest and `observed` are the same value.
    ///
    /// Comparison is the only operation this value supports, because comparing
    /// a published digest with a freshly computed one is the only thing the
    /// commit protocol needs it for.
    #[must_use]
    pub fn matches(&self, observed: &Self) -> bool {
        self.digest == observed.digest
    }
}

impl ::core::fmt::Debug for SourceDigest {
    fn fmt(&self, formatter: &mut ::core::fmt::Formatter<'_>) -> ::core::fmt::Result {
        formatter.write_str(crate::secret_value::REDACTED_RENDERING)
    }
}

/// One source the commit inventory lists.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConfigurationSource {
    /// Root-relative reference of the source.
    pub reference: ConfigurationReference,
    /// Exact digest of the source's bytes at commit time.
    pub digest: SourceDigest,
}

/// The commit inventory of one configuration generation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConfigurationSnapshot {
    /// Sources the inventory lists, strictly ascending by reference bytes.
    sources: Vec<ConfigurationSource>,
}

/// The commit inventory exactly as the document spells it.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct SnapshotDocument {
    /// Format version the document declares.
    format_version: u64,
    /// Sources the document lists.
    sources: Vec<SourceEntry>,
}

/// One inventory entry exactly as the document spells it.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct SourceEntry {
    /// Root-relative reference the entry names.
    reference: String,
    /// Rendered digest the entry carries.
    sha256: String,
}

impl ConfigurationSnapshot {
    /// Parses one commit inventory document.
    ///
    /// # Errors
    ///
    /// Returns [`ConfigurationFailureCode::ConfigurationDocumentSyntaxInvalid`]
    /// when the bytes are not a valid document,
    /// [`ConfigurationFailureCode::ConfigurationDocumentShapeInvalid`] when the
    /// document holds an unknown, missing, or wrongly typed member,
    /// [`ConfigurationFailureCode::ConfigurationFormatUnsupported`] when it
    /// declares another format version, and
    /// [`ConfigurationFailureCode::ConfigurationValueInvalid`] when the source
    /// list is empty, longer than the contract allows, or not strictly
    /// ascending by reference bytes.
    pub fn parse(text: &str) -> Result<Self, ConfigurationSnapshotFailure> {
        let document: SnapshotDocument = parse_document(text)?;
        let limits = &ProfileAuthenticationContract::embedded().limits;
        if document.format_version != limits.supported_configuration_snapshot_format_version {
            return Err(ConfigurationSnapshotFailure::at(
                ConfigurationFailureCode::ConfigurationFormatUnsupported,
                "format_version",
            ));
        }
        if document.sources.is_empty()
            || document.sources.len() > narrow_limit(limits.maximum_configuration_snapshot_sources)
        {
            return Err(ConfigurationSnapshotFailure::at(
                ConfigurationFailureCode::ConfigurationValueInvalid,
                "sources",
            ));
        }
        let mut sources = Vec::with_capacity(document.sources.len());
        let mut previous: Option<String> = None;
        for entry in document.sources {
            let reference = ConfigurationReference::parse(&entry.reference)?;
            if previous.is_some_and(|earlier| earlier >= entry.reference) {
                return Err(ConfigurationSnapshotFailure::at(
                    ConfigurationFailureCode::ConfigurationValueInvalid,
                    "sources",
                ));
            }
            previous = Some(entry.reference);
            sources.push(ConfigurationSource {
                reference,
                digest: SourceDigest::parse(&entry.sha256)?,
            });
        }
        Ok(Self { sources })
    }

    /// Returns the listed sources, strictly ascending by reference bytes.
    #[must_use]
    pub fn sources(&self) -> &[ConfigurationSource] {
        &self.sources
    }

    /// Returns the entry naming `reference`, when the inventory lists it.
    #[must_use]
    pub fn source(&self, reference: &ConfigurationReference) -> Option<&ConfigurationSource> {
        self.sources.iter().find(|source| &source.reference == reference)
    }
}

/// Reports whether one reference component matches the manifest grammar.
///
/// The grammar is `[A-Za-z0-9][A-Za-z0-9._-]*` within its byte bound. A dot,
/// a dot-dot, an empty component, and a component opening with a separator or a
/// punctuation byte all fail its first character.
fn is_portable_component(component: &str, maximum_bytes: usize) -> bool {
    if component.is_empty() || component.len() > maximum_bytes {
        return false;
    }
    let mut characters = component.chars();
    let opens = characters.next().is_some_and(|first| first.is_ascii_alphanumeric());
    opens
        && characters
            .all(|character| character.is_ascii_alphanumeric() || "._-".contains(character))
}

/// Parses one document in two phases so a failure names its real cause.
///
/// Tokenizing first and mapping second separates a document that is not valid
/// source from one that is valid source with the wrong members. The parser's
/// own message is discarded either way, because it quotes source bytes and a
/// configuration source can hold a secret anywhere in it.
fn parse_document<Shape: serde::de::DeserializeOwned>(
    text: &str,
) -> Result<Shape, ConfigurationSnapshotFailure> {
    let value: toml::Value = toml::from_str(text).map_err(|_| {
        ConfigurationSnapshotFailure::at(
            ConfigurationFailureCode::ConfigurationDocumentSyntaxInvalid,
            "configuration_snapshot",
        )
    })?;
    value.try_into().map_err(|_| {
        ConfigurationSnapshotFailure::at(
            ConfigurationFailureCode::ConfigurationDocumentShapeInvalid,
            "configuration_snapshot",
        )
    })
}
