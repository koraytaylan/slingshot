//! Supported platform matrix.
//!
//! The module reads `support/platforms.toml`, the only abstract supported-target
//! authority in the repository, and validates it against the closed capability
//! vocabulary declared here. The manifest assigns capabilities to rows; this
//! module owns the vocabulary and the artifact-layout rules, so a row cannot
//! invent an identifier and a capability cannot be spelled two ways.
//!
//! A real invocation observes at most the single row that matches the current
//! environment and labels the result untrusted. Concrete provider selectors,
//! runner images, linker or software-development-kit digests, cross-compiled
//! results, family labels, and aggregate success claims are refused.

use std::collections::BTreeSet;

use serde::Deserialize;
use sha2::{Digest, Sha256};

/// Format identifier the supported-target manifest must declare.
pub const MATRIX_FORMAT: &str = "slingshot.supported-platforms/1";

/// Label every current-environment observation carries.
pub const UNTRUSTED_OBSERVATION_LABEL: &str = "untrusted_current_native_observation";

/// Executable stem every release artifact uses.
pub const EXECUTABLE_STEM: &str = "slingshot";

/// Native smoke mode every row declares.
pub const NATIVE_SMOKE_MODE: &str = "direct";

/// Host class every row declares, because no row supports a cross-built host.
pub const NATIVE_HOST_CLASS: &str = "native";

/// Archive member that carries the owner-supplied licence text.
pub const LICENCE_ARCHIVE_MEMBER: &str = "LICENSE";

/// Archive member that carries the artifact checksums.
pub const CHECKSUM_ARCHIVE_MEMBER: &str = "SHA256SUMS";

/// The Linux row's triple.
///
/// Each row's triple is named rather than found by position in the supported
/// list, because the rules a row obeys are a property of that row and outlive
/// whether the matrix currently claims it. A row that leaves the matrix takes
/// its evidence with it and leaves its rules where a later row is held to them.
pub const LINUX_TARGET_TRIPLE: &str = "x86_64-unknown-linux-gnu";

/// The macOS row's triple.
pub const MACOS_TARGET_TRIPLE: &str = "aarch64-apple-darwin";

/// The Windows row's triple.
pub const WINDOWS_TARGET_TRIPLE: &str = "x86_64-pc-windows-msvc";

/// Exact supported target triples, in manifest order.
pub const SUPPORTED_TARGET_TRIPLES: &[&str] = &[LINUX_TARGET_TRIPLE, MACOS_TARGET_TRIPLE];

/// Operating system, architecture, executable suffix, and archive profile of
/// each supported triple, in the same order as [`SUPPORTED_TARGET_TRIPLES`].
const SUPPORTED_TARGET_LAYOUTS: &[(&str, &str, &str, &str)] =
    &[("linux", "x86_64", "", "tar.gz"), ("macos", "aarch64", "", "tar.gz")];

/// Provider-record trust decisions every row must expose without reduction.
const PROVIDER_TRUST_CAPABILITIES: &[&str] = &[
    "provider-record-distinguished-encoding-rules-bytes",
    "effective-server-authentication-purpose",
    "distrust-or-deny-decision",
    "application-policy-or-name-constraint",
    "unevaluable-decision",
    "conflicting-same-record-decisions",
];

/// Runtime capabilities the Unix rows must provide.
const UNIX_RUNTIME_CAPABILITIES: &[&str] = &[
    "owner-only-runtime-directory",
    "unix-domain-socket-endpoint",
    "distinct-advisory-owner-lock",
    "distinct-advisory-startup-election-lock",
    "peer-current-user-policy",
    "atomic-same-directory-readiness",
    "session-independent-detachment",
    "stable-supervised-child-cleanup",
];

/// Runtime capabilities the Windows row must provide.
const WINDOWS_RUNTIME_CAPABILITIES: &[&str] = &[
    "current-user-security-identifier-enforcement",
    "named-pipe-endpoint",
    "named-pipe-reject-remote-clients",
    "distinct-exclusive-owner-lock",
    "distinct-exclusive-startup-election-lock",
    "atomic-same-directory-readiness",
    "detached-process-creation",
    "stable-supervised-child-cleanup",
];

/// Filesystem capabilities the Linux row must provide.
const LINUX_FILESYSTEM_CAPABILITIES: &[&str] = &[
    "descriptor-relative-no-follow-traversal",
    "descriptor-bound-file-identity",
    "masked-posix-access-control-lists",
];

/// Filesystem capabilities the macOS row must provide.
const MACOS_FILESYSTEM_CAPABILITIES: &[&str] = &[
    "descriptor-relative-no-follow-traversal",
    "descriptor-bound-file-identity",
    "extended-access-control-lists",
];

/// Filesystem capabilities the Windows row must provide.
///
/// The identity requirement is volume-scoped rather than wider, because a safe
/// public Rust interface reaches the volume serial number and the file index
/// but not the wider record, and the workspace forbids unchecked code.
const WINDOWS_FILESYSTEM_CAPABILITIES: &[&str] = &[
    "no-follow-reparse-open",
    "security-descriptor-and-discretionary-access-control-list",
    "reparse-point-evidence",
    "link-count",
    "volume-serial-number",
    "volume-scoped-file-identifier",
];

/// Deterministic build-policy requirements every row must declare.
const BUILD_POLICY_CAPABILITIES: &[&str] = &[
    "exact-repository-rust-toolchain",
    "exact-target-triple",
    "incremental-compilation-disabled",
    "closed-build-environment",
    "source-path-remapping",
    "build-root-remapping",
    "source-date-from-source-object",
    "native-linker-system-root-or-software-development-kit-observation",
    "no-ambient-archive-program",
];

/// Tokens that mark an unfinished or invented manifest value.
const PLACEHOLDER_TOKENS: &[&str] = &["TODO", "TBD", "FIXME", "example", "<", ">", "?"];

/// Reason the supported-target manifest could not be read or observed.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum MatrixFailure {
    /// The manifest bytes are not a valid supported-target document.
    #[error("the supported-target manifest could not be read: {0}")]
    Unreadable(String),
    /// The current environment matches no supported row.
    #[error("the current environment matches no supported target row")]
    NoCurrentRow,
    /// The requested row is not the row the current environment matches.
    #[error("{requested} is not the current environment row {current}")]
    NotCurrentRow {
        /// Row the caller asked to observe.
        requested: String,
        /// Row the current environment actually matches.
        current: String,
    },
    /// The current environment does not match the row it claims to be.
    #[error("the current environment reports {observed}, but the row requires {required}")]
    EnvironmentMismatch {
        /// Value the current environment reports.
        observed: String,
        /// Value the row requires.
        required: String,
    },
}

/// The abstract supported-target manifest.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "kebab-case", deny_unknown_fields)]
pub struct SupportedPlatformMatrix {
    /// Format identifier of the manifest.
    pub format: String,
    /// One row per supported target.
    pub target: Vec<SupportedTarget>,
}

/// One abstract supported target.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "kebab-case", deny_unknown_fields)]
pub struct SupportedTarget {
    /// Exact target triple.
    pub triple: String,
    /// Native operating system a real invocation must run on.
    pub operating_system: String,
    /// Native architecture a real invocation must run on.
    pub architecture: String,
    /// Host class the row requires.
    pub host_class: String,
    /// Executable stem of the release artifact.
    pub executable_stem: String,
    /// Executable suffix of the release artifact.
    pub executable_suffix: String,
    /// Deterministic archive profile of the release artifact.
    pub archive_profile: String,
    /// Flat archive membership, in canonical order.
    pub archive_members: Vec<String>,
    /// Native smoke mode the release acceptance run uses.
    pub native_smoke_mode: String,
    /// Provider-record trust decisions the target must expose.
    pub provider_trust_capabilities: Vec<String>,
    /// Endpoint, lock, readiness, and supervision capabilities.
    pub runtime_capabilities: Vec<String>,
    /// Filesystem traversal, identity, and access-control capabilities.
    pub filesystem_capabilities: Vec<String>,
    /// Deterministic build-policy requirements.
    pub build_policy_capabilities: Vec<String>,
}

/// One deterministic policy observation of a target.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "kebab-case", deny_unknown_fields)]
pub struct PlatformObservation {
    /// Name the fixture gives the observation.
    pub name: String,
    /// Target triple the observation claims.
    pub triple: String,
    /// Operating system the observation reports.
    pub operating_system: String,
    /// Architecture the observation reports.
    pub architecture: String,
    /// Capabilities the observation reports as available.
    pub capabilities: Vec<String>,
    /// Whether the row must accept the observation.
    pub accepted: bool,
}

/// A set of deterministic policy observations.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "kebab-case", deny_unknown_fields)]
pub struct PlatformObservations {
    /// One entry per observation.
    pub observation: Vec<PlatformObservation>,
}

/// One explicitly untrusted observation of the current environment.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CurrentNativeObservation {
    /// Label that keeps the observation from being read as authority.
    pub label: &'static str,
    /// Target triple the current environment matches.
    pub triple: String,
    /// Operating system the current environment reports.
    pub operating_system: String,
    /// Architecture the current environment reports.
    pub architecture: String,
    /// Digest of the manifest bytes the observation was taken against.
    pub matrix_digest: String,
    /// Capabilities the matched row requires, in manifest order.
    pub required_capabilities: Vec<String>,
}

/// Returns the supported target triple of the current build, when one matches.
#[must_use]
pub const fn current_target_triple() -> Option<&'static str> {
    if cfg!(all(target_os = "linux", target_arch = "x86_64", target_env = "gnu")) {
        Some(LINUX_TARGET_TRIPLE)
    } else if cfg!(all(target_os = "macos", target_arch = "aarch64")) {
        Some(MACOS_TARGET_TRIPLE)
    } else if cfg!(all(target_os = "windows", target_arch = "x86_64", target_env = "msvc")) {
        Some(WINDOWS_TARGET_TRIPLE)
    } else {
        None
    }
}

/// Returns the lowercase digest of the manifest bytes an observation cites.
#[must_use]
pub fn matrix_digest(manifest: &[u8]) -> String {
    hex::encode(Sha256::digest(manifest))
}

/// Reads a supported-target manifest.
///
/// # Errors
///
/// Returns [`MatrixFailure::Unreadable`] when the bytes are not a supported
/// document, including when a row carries a field outside the closed schema.
pub fn parse_matrix(manifest: &str) -> Result<SupportedPlatformMatrix, MatrixFailure> {
    toml::from_str(manifest).map_err(|failure| MatrixFailure::Unreadable(failure.to_string()))
}

/// Returns the required capability identifiers of one row, grouped in order.
#[must_use]
pub fn required_capabilities(triple: &str) -> Vec<&'static str> {
    let mut required: Vec<&'static str> = PROVIDER_TRUST_CAPABILITIES.to_vec();
    let windows = triple == WINDOWS_TARGET_TRIPLE;
    required.extend(if windows { WINDOWS_RUNTIME_CAPABILITIES } else { UNIX_RUNTIME_CAPABILITIES });
    required.extend(match triple {
        value if value == LINUX_TARGET_TRIPLE => LINUX_FILESYSTEM_CAPABILITIES,
        value if value == MACOS_TARGET_TRIPLE => MACOS_FILESYSTEM_CAPABILITIES,
        _ => WINDOWS_FILESYSTEM_CAPABILITIES,
    });
    required.extend(BUILD_POLICY_CAPABILITIES);
    required
}

/// Reports every placeholder token a manifest value contains.
fn evaluate_placeholders(label: &str, value: &str) -> Vec<String> {
    PLACEHOLDER_TOKENS
        .iter()
        .filter(|token| value.contains(**token))
        .map(|token| format!("{label} contains the placeholder token {token}"))
        .collect()
}

/// Reports every layout value that disagrees with the triple's fixed layout.
fn evaluate_layout(row: &SupportedTarget, layout: (&str, &str, &str, &str)) -> Vec<String> {
    let (operating_system, architecture, suffix, profile) = layout;
    let mut violations = Vec::new();
    let expected = [
        ("operating system", row.operating_system.as_str(), operating_system),
        ("architecture", row.architecture.as_str(), architecture),
        ("host class", row.host_class.as_str(), NATIVE_HOST_CLASS),
        ("executable stem", row.executable_stem.as_str(), EXECUTABLE_STEM),
        ("executable suffix", row.executable_suffix.as_str(), suffix),
        ("archive profile", row.archive_profile.as_str(), profile),
        ("native smoke mode", row.native_smoke_mode.as_str(), NATIVE_SMOKE_MODE),
    ];
    for (label, found, wanted) in expected {
        if found != wanted {
            violations.push(format!("{} declares the {label} {found}, not {wanted}", row.triple));
        }
    }
    let members = [
        format!("{EXECUTABLE_STEM}{suffix}"),
        LICENCE_ARCHIVE_MEMBER.to_owned(),
        CHECKSUM_ARCHIVE_MEMBER.to_owned(),
    ];
    if row.archive_members != members {
        violations
            .push(format!("{} declares the archive members {:?}", row.triple, row.archive_members));
    }
    violations
}

/// Reports every capability group that differs from the row's required set.
fn evaluate_capabilities(row: &SupportedTarget) -> Vec<String> {
    let declared: Vec<&str> = row
        .provider_trust_capabilities
        .iter()
        .chain(&row.runtime_capabilities)
        .chain(&row.filesystem_capabilities)
        .chain(&row.build_policy_capabilities)
        .map(String::as_str)
        .collect();
    let required = required_capabilities(&row.triple);
    let mut violations = Vec::new();
    if declared.len() != required.len() {
        violations.push(format!("{} declares {} capabilities", row.triple, declared.len()));
    }
    let declared_set: BTreeSet<&str> = declared.iter().copied().collect();
    let required_set: BTreeSet<&str> = required.iter().copied().collect();
    for missing in required_set.difference(&declared_set) {
        violations.push(format!("{} omits the required capability {missing}", row.triple));
    }
    for additional in declared_set.difference(&required_set) {
        violations.push(format!("{} declares the unknown capability {additional}", row.triple));
    }
    violations
}

/// Reports every rule the supported-target manifest breaks.
#[must_use]
pub fn validate_matrix(matrix: &SupportedPlatformMatrix) -> Vec<String> {
    let mut violations = Vec::new();
    if matrix.format != MATRIX_FORMAT {
        violations.push(format!("the manifest declares the format {}", matrix.format));
    }
    let declared: Vec<&str> = matrix.target.iter().map(|row| row.triple.as_str()).collect();
    if declared != SUPPORTED_TARGET_TRIPLES {
        violations.push(format!("the manifest declares the target rows {declared:?}"));
        return violations;
    }
    for (row, layout) in matrix.target.iter().zip(SUPPORTED_TARGET_LAYOUTS) {
        violations.extend(evaluate_layout(row, *layout));
        violations.extend(evaluate_capabilities(row));
        violations.extend(evaluate_placeholders(&row.triple, &row.triple));
        for capability in required_capabilities(&row.triple) {
            violations.extend(evaluate_placeholders(&row.triple, capability));
        }
    }
    violations
}

/// Reports every requirement a deterministic policy observation fails.
#[must_use]
pub fn evaluate_observation(
    row: &SupportedTarget,
    observation: &PlatformObservation,
) -> Vec<String> {
    let mut violations = Vec::new();
    let identity = [
        (observation.triple.as_str(), row.triple.as_str()),
        (observation.operating_system.as_str(), row.operating_system.as_str()),
        (observation.architecture.as_str(), row.architecture.as_str()),
    ];
    for (observed, required) in identity {
        if observed != required {
            violations.push(format!(
                "{} reports {observed} where {required} is required",
                observation.name
            ));
        }
    }
    let available: BTreeSet<&str> = observation.capabilities.iter().map(String::as_str).collect();
    for capability in required_capabilities(&row.triple) {
        if !available.contains(capability) {
            violations.push(format!("{} lacks the capability {capability}", observation.name));
        }
    }
    violations
}

/// Observes the single row that matches the current environment.
///
/// The result is explicitly untrusted: it describes one machine that nobody has
/// attested, so it can never stand in for the owner-gated release evidence.
///
/// # Errors
///
/// Returns [`MatrixFailure::NoCurrentRow`] when the build target is not a
/// supported row, [`MatrixFailure::NotCurrentRow`] when `triple` names another
/// row, and [`MatrixFailure::EnvironmentMismatch`] when the running operating
/// system or architecture disagrees with the row.
pub fn observe_current_native(
    matrix: &SupportedPlatformMatrix,
    manifest: &[u8],
    triple: &str,
) -> Result<CurrentNativeObservation, MatrixFailure> {
    let current = current_target_triple().ok_or(MatrixFailure::NoCurrentRow)?;
    if triple != current {
        return Err(MatrixFailure::NotCurrentRow {
            requested: triple.to_owned(),
            current: current.to_owned(),
        });
    }
    let row = matrix
        .target
        .iter()
        .find(|candidate| candidate.triple == current)
        .ok_or(MatrixFailure::NoCurrentRow)?;
    let environment = [
        (std::env::consts::OS, row.operating_system.as_str()),
        (std::env::consts::ARCH, row.architecture.as_str()),
    ];
    for (observed, required) in environment {
        if observed != required {
            return Err(MatrixFailure::EnvironmentMismatch {
                observed: observed.to_owned(),
                required: required.to_owned(),
            });
        }
    }
    Ok(CurrentNativeObservation {
        label: UNTRUSTED_OBSERVATION_LABEL,
        triple: row.triple.clone(),
        operating_system: row.operating_system.clone(),
        architecture: row.architecture.clone(),
        matrix_digest: matrix_digest(manifest),
        required_capabilities: required_capabilities(&row.triple)
            .into_iter()
            .map(str::to_owned)
            .collect(),
    })
}
