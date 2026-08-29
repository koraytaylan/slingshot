//! The version every command declares, and the limits every command reads.
//!
//! `schemas/command-contract-limits-1.json` is the sole authority for both.
//! Every bound a command enforces, every keyword a schema emits, and every
//! vector an external agent runs comes from that file, so a value exists once
//! and a consumer that declared its own copy would be declaring a second
//! contract.
//!
//! The version grammar is deliberately narrower than Semantic Versioning 2.0.0
//! allows. A version becomes a segment of a schema identifier, so it is bounded
//! in bytes, bounded in identifiers, and restricted to an alphabet that needs no
//! escaping anywhere it is written. Within that, the specification's own
//! asymmetry is preserved rather than smoothed over: a core or numeric
//! prerelease identifier has one minimal spelling, while build metadata keeps
//! whatever spelling it was given - so `1.0.0+01` is a legal version and
//! `1.0.0-01` is not.

use std::collections::BTreeMap;

use serde::Deserialize;

/// Bytes of the committed manifest, embedded at compile time.
const EMBEDDED_MANIFEST: &str = include_str!("../../../../schemas/command-contract-limits-1.json");

/// Format identifier the manifest must declare.
pub const CONTRACT_FORMAT: &str = "slingshot.command-contract-limits/1";

/// Canonicalization the manifest must declare.
pub const CONTRACT_CANONICALIZATION: &str = "slingshot.schema-canonical/1";

/// Unit every duration in the manifest is stated in.
pub const CONTRACT_DURATION_UNIT: &str = "milliseconds";

/// Version every command this plan introduces declares.
pub const INITIAL_COMMAND_VERSION: &str = "1.0.0";

/// Core identifiers every version carries.
const CORE_IDENTIFIERS: usize = 3;

/// Members the contract document carries.
const MEMBER_COUNT: usize = 5;

/// Separator between two identifiers of one part.
const IDENTIFIER_SEPARATOR: char = '.';

/// Separator introducing the prerelease part.
const PRERELEASE_SEPARATOR: char = '-';

/// Separator introducing the build part.
const BUILD_SEPARATOR: char = '+';

/// Reason a command contract could not be read.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum CommandContractFailure {
    /// The manifest bytes are not a valid contract document.
    #[error("the command contract could not be read: {0}")]
    Unreadable(String),
    /// The manifest declares a format or canonicalization this build does not
    /// implement.
    #[error("the command contract declares {0}")]
    UnsupportedFormat(String),
    /// The manifest is not in the canonical form its readers regenerate.
    #[error("the command contract is not in canonical form")]
    NotCanonical,
    /// The manifest declares a command at a version this plan does not set.
    #[error("the command contract declares {command} at {version}")]
    UnexpectedCommandVersion {
        /// Command the manifest names.
        command: String,
        /// Version it names it at.
        version: String,
    },
}

/// Reason a version string is not a command semantic contract version.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum VersionFailure {
    /// The version is longer than the contract allows.
    #[error("the version is longer than the contract allows")]
    TooLong,
    /// The version names more identifiers than the contract allows.
    #[error("the version names more identifiers than the contract allows")]
    TooManyIdentifiers,
    /// A numeric identifier has more digits than the contract allows.
    #[error("a numeric identifier has more digits than the contract allows")]
    NumericTooLong,
    /// An identifier is empty, or uses a byte the alphabet does not hold.
    #[error("an identifier is empty or uses a byte outside the alphabet")]
    MalformedIdentifier,
    /// A core or numeric prerelease identifier is not minimally spelled.
    #[error("a numeric identifier is not minimally spelled")]
    NonMinimalNumber,
    /// The version does not have exactly three core identifiers.
    #[error("the version does not have exactly three core identifiers")]
    MalformedCore,
}

/// The complete command contract.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CommandContract {
    /// Canonicalization the manifest is written under.
    pub canonicalization: String,
    /// Version every command declares, by wire name.
    pub command_semantic_contract_versions: BTreeMap<String, String>,
    /// Unit every duration is stated in.
    pub duration_unit: String,
    /// Format identifier of the manifest document.
    pub format: String,
    /// Every numeric bound this plan enforces, by name.
    ///
    /// The names are the manifest's own, which is what makes it the single
    /// authority: a consumer asks for one by name rather than declaring a
    /// constant of its own that could drift.
    pub limits: BTreeMap<String, u64>,
}

impl CommandContract {
    /// Returns the contract embedded in this build.
    ///
    /// # Panics
    ///
    /// Panics when the embedded manifest is not a valid contract, which is a
    /// repository defect rather than a runtime condition.
    #[must_use]
    pub fn embedded() -> &'static Self {
        static PARSED: std::sync::OnceLock<CommandContract> = std::sync::OnceLock::new();
        PARSED.get_or_init(|| {
            Self::parse(EMBEDDED_MANIFEST).expect("the embedded command contract is valid")
        })
    }

    /// Returns the exact manifest bytes embedded in this build.
    #[must_use]
    pub fn embedded_manifest() -> &'static str {
        EMBEDDED_MANIFEST
    }

    /// Parses one manifest document.
    ///
    /// # Errors
    ///
    /// Returns [`CommandContractFailure::Unreadable`] when the bytes are not a
    /// contract document, [`CommandContractFailure::UnsupportedFormat`] when the
    /// document declares another format or canonicalization,
    /// [`CommandContractFailure::NotCanonical`] when the bytes are not the
    /// canonical rendering of what they parse to, and
    /// [`CommandContractFailure::UnexpectedCommandVersion`] when a command is
    /// declared at a version this plan does not set.
    pub fn parse(text: &str) -> Result<Self, CommandContractFailure> {
        let contract: Self = serde_json::from_str(text)
            .map_err(|failure| CommandContractFailure::Unreadable(failure.to_string()))?;
        if contract.format != CONTRACT_FORMAT {
            return Err(CommandContractFailure::UnsupportedFormat(contract.format));
        }
        if contract.canonicalization != CONTRACT_CANONICALIZATION
            || contract.duration_unit != CONTRACT_DURATION_UNIT
        {
            return Err(CommandContractFailure::UnsupportedFormat(contract.canonicalization));
        }
        if contract.render()? != text {
            return Err(CommandContractFailure::NotCanonical);
        }
        for (command, version) in &contract.command_semantic_contract_versions {
            if version != INITIAL_COMMAND_VERSION {
                return Err(CommandContractFailure::UnexpectedCommandVersion {
                    command: command.clone(),
                    version: version.clone(),
                });
            }
        }
        Ok(contract)
    }

    /// Renders this contract back into the manifest's canonical form.
    ///
    /// # Errors
    ///
    /// Returns [`CommandContractFailure::Unreadable`] when the contract cannot
    /// be rendered, which no parsed contract can fail to be.
    pub fn render(&self) -> Result<String, CommandContractFailure> {
        let mut rendered = serde_json::to_string(self)
            .map_err(|failure| CommandContractFailure::Unreadable(failure.to_string()))?;
        rendered.push('\n');
        Ok(rendered)
    }

    /// Returns the limit `name` bounds.
    ///
    /// # Panics
    ///
    /// Panics when the manifest declares no limit of that name, which means a
    /// caller has invented one rather than reading one.
    #[must_use]
    pub fn limit(&self, name: &str) -> u64 {
        *self
            .limits
            .get(name)
            .unwrap_or_else(|| panic!("the command contract declares no limit named {name}"))
    }
}

impl serde::Serialize for CommandContract {
    fn serialize<Target: serde::Serializer>(
        &self,
        serializer: Target,
    ) -> Result<Target::Ok, Target::Error> {
        use serde::ser::SerializeStruct;

        let mut document = serializer.serialize_struct("CommandContract", MEMBER_COUNT)?;
        document.serialize_field("canonicalization", &self.canonicalization)?;
        document.serialize_field(
            "command_semantic_contract_versions",
            &self.command_semantic_contract_versions,
        )?;
        document.serialize_field("duration_unit", &self.duration_unit)?;
        document.serialize_field("format", &self.format)?;
        document.serialize_field("limits", &self.limits)?;
        document.end()
    }
}

/// The version one command's meaning is published under.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommandSemanticContractVersion {
    /// The exact accepted spelling.
    version: String,
}

impl CommandSemanticContractVersion {
    /// Validates one version against the contract grammar and bounds.
    ///
    /// # Errors
    ///
    /// Returns the [`VersionFailure`] naming the first rule the spelling broke.
    pub fn parse(version: &str) -> Result<Self, VersionFailure> {
        let limits = &CommandContract::embedded().limits;
        let maximum_bytes = limits["maximum_command_semantic_contract_version_bytes"];
        let maximum_identifiers = limits["maximum_command_semantic_contract_version_identifiers"];
        let maximum_digits = limits["maximum_command_semantic_contract_version_numeric_digits"];
        if u64::try_from(version.len()).unwrap_or(u64::MAX) > maximum_bytes {
            return Err(VersionFailure::TooLong);
        }
        let (core, prerelease, build) = split_parts(version)?;
        let core: Vec<&str> = core.split(IDENTIFIER_SEPARATOR).collect();
        if core.len() != CORE_IDENTIFIERS {
            return Err(VersionFailure::MalformedCore);
        }
        for identifier in &core {
            accept_numeric(identifier, maximum_digits)?;
        }
        let prerelease: Vec<&str> = split_part(prerelease);
        for identifier in &prerelease {
            accept_prerelease(identifier, maximum_digits)?;
        }
        let build: Vec<&str> = split_part(build);
        for identifier in &build {
            accept_build(identifier)?;
        }
        let counted = CORE_IDENTIFIERS
            .checked_add(prerelease.len())
            .and_then(|total| total.checked_add(build.len()))
            .ok_or(VersionFailure::TooManyIdentifiers)?;
        if u64::try_from(counted).unwrap_or(u64::MAX) > maximum_identifiers {
            return Err(VersionFailure::TooManyIdentifiers);
        }
        Ok(Self { version: version.to_owned() })
    }

    /// Returns the version exactly as it was written.
    #[must_use]
    pub fn as_text(&self) -> &str {
        &self.version
    }
}

impl ::core::fmt::Display for CommandSemanticContractVersion {
    fn fmt(&self, formatter: &mut ::core::fmt::Formatter<'_>) -> ::core::fmt::Result {
        formatter.write_str(&self.version)
    }
}

/// Splits one version into its core, prerelease, and build parts.
///
/// The build separator is looked for first, because a prerelease separator may
/// legally appear inside build metadata but not the other way round.
fn split_parts(version: &str) -> Result<(&str, Option<&str>, Option<&str>), VersionFailure> {
    let (without_build, build) = match version.split_once(BUILD_SEPARATOR) {
        Some((head, build)) => (head, Some(build)),
        None => (version, None),
    };
    let (core, prerelease) = match without_build.split_once(PRERELEASE_SEPARATOR) {
        Some((head, prerelease)) => (head, Some(prerelease)),
        None => (without_build, None),
    };
    if core.is_empty() {
        return Err(VersionFailure::MalformedCore);
    }
    Ok((core, prerelease, build))
}

/// Returns the identifiers one optional part names.
fn split_part(part: Option<&str>) -> Vec<&str> {
    part.map(|part| part.split(IDENTIFIER_SEPARATOR).collect()).unwrap_or_default()
}

/// Accepts one minimally spelled numeric identifier.
fn accept_numeric(identifier: &str, maximum_digits: u64) -> Result<(), VersionFailure> {
    if identifier.is_empty() || !identifier.bytes().all(|byte| byte.is_ascii_digit()) {
        return Err(VersionFailure::MalformedIdentifier);
    }
    if u64::try_from(identifier.len()).unwrap_or(u64::MAX) > maximum_digits {
        return Err(VersionFailure::NumericTooLong);
    }
    if identifier.len() > 1 && identifier.starts_with('0') {
        return Err(VersionFailure::NonMinimalNumber);
    }
    Ok(())
}

/// Accepts one prerelease identifier.
///
/// An all-digit identifier is a number and obeys the numeric rules; anything
/// else must carry at least one letter or hyphen, which is what keeps a number
/// from being spelled two ways.
fn accept_prerelease(identifier: &str, maximum_digits: u64) -> Result<(), VersionFailure> {
    if identifier.bytes().all(|byte| byte.is_ascii_digit()) {
        return accept_numeric(identifier, maximum_digits);
    }
    accept_build(identifier)
}

/// Accepts one build identifier, whose spelling is preserved exactly.
fn accept_build(identifier: &str) -> Result<(), VersionFailure> {
    let usable = !identifier.is_empty()
        && identifier.bytes().all(|byte| byte.is_ascii_alphanumeric() || byte == b'-');
    if usable {
        return Ok(());
    }
    Err(VersionFailure::MalformedIdentifier)
}
