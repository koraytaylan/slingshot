//! Whether one source revision is releasable, decided once and in isolation.
//!
//! Acceptance is the last question, and it is a different question from any of
//! the gates it runs. Each gate answers something about a part; acceptance
//! answers whether every part was answered, by the right thing, about the same
//! revision, in an environment where none of them could have reached anything
//! they should not have.
//!
//! # Offline mode is not isolation
//!
//! Cargo's offline mode is a flag one program honours. A build script, a test,
//! or a tool a test spawns is free to open a socket, read a host path, or write
//! into an input it was given. So the gates run inside a container that denies
//! those things to everything in it, and every flag that makes the denial real
//! is written down and checked rather than inherited from whatever the runtime
//! defaults to this year.
//!
//! # A gate that did not run is not a gate that passed
//!
//! The inventory is closed and ordered. A missing entry, a repeated one, one
//! out of order, one that refused, and one about another revision are five
//! different defects with one consequence, and each is refused by name so that
//! whoever reads the refusal knows which it was.

use std::collections::BTreeSet;

use serde::Deserialize;

/// Where the isolation contract lives.
pub const CONTAINER_PATH: &str = "support/release-acceptance-container.toml";

/// Where the acceptance manifest schema lives.
pub const SCHEMA_PATH: &str = "schemas/release/release-acceptance-manifest.schema.json";

/// The format the isolation contract declares.
pub const CONTAINER_FORMAT: &str = "slingshot.release-acceptance-container/1";

/// The format an acceptance manifest declares.
pub const MANIFEST_FORMAT: &str = "slingshot.release-acceptance/1";

/// What the network is set to, and the only thing it may be set to.
pub const NETWORK_NONE: &str = "none";

/// The capability set a run drops, and the only one it may drop.
pub const DROP_EVERYTHING: &str = "ALL";

/// What a pinned image digest begins with.
const DIGEST_PREFIX: &str = "sha256:";

/// Every gate one acceptance run holds, in the order it holds them.
///
/// Ordered rather than merely enumerated, because the order is part of the
/// answer: source policy before anything is built from the source, the
/// contracts before the things that consume them, and the compatibility gate
/// last because it is the only one that runs another project's code.
pub const REQUIRED_GATES: &[&str] = &[
    "source-policy",
    "dependency-direction",
    "workspace-module-map",
    "release-metadata",
    "release-attestation-policy",
    "locked-source-cache",
    "command-contract",
    "protocol-compatibility",
    "configuration-and-storage-compatibility",
    "platform-runtime",
    "release-artifact-contract",
    "finite-state-machine-compatibility",
];

/// The isolation one acceptance run happens inside.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "kebab-case", deny_unknown_fields)]
pub struct AcceptanceContainer {
    /// Which owner-declared row coordinates.
    pub coordinator: CoordinatorRow,
    /// What the run has of its environment.
    pub environment: EnvironmentPolicy,
    /// The format this document declares.
    pub format: String,
    /// The immutable image it runs.
    pub image: ImagePolicy,
    /// Every denial that makes the isolation real.
    pub isolation: IsolationPolicy,
    /// What is mounted, and how.
    pub mounts: MountPolicy,
    /// The runtime that enforces it.
    pub runtime: RuntimePolicy,
}

/// Which owner-declared row coordinates.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "kebab-case", deny_unknown_fields)]
pub struct CoordinatorRow {
    /// Which runner it is.
    pub runner_selector: String,
    /// Which target it is.
    pub triple: String,
}

/// What the run has of its environment.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "kebab-case", deny_unknown_fields)]
pub struct EnvironmentPolicy {
    /// The only variables that exist inside.
    pub allowed: Vec<String>,
}

/// The immutable image one acceptance run runs.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "kebab-case", deny_unknown_fields)]
pub struct ImagePolicy {
    /// The immutable digest that is loaded.
    pub digest: String,
    /// The local layout member the digest is loaded from.
    pub local_oci_layout_member: String,
    /// Whether the runtime may fetch it.
    pub pull: bool,
    /// The reference the digest was chosen from.
    pub reference: String,
}

/// Every denial that makes the isolation real.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "kebab-case", deny_unknown_fields)]
pub struct IsolationPolicy {
    /// Capabilities added back, which is none of them.
    pub add_capabilities: Vec<String>,
    /// Capabilities dropped.
    pub drop_capabilities: Vec<String>,
    /// Host devices exposed, which is none of them.
    pub host_devices: Vec<String>,
    /// Whether an engine socket is reachable.
    pub host_engine_socket: bool,
    /// Host namespaces joined, which is none of them.
    pub host_namespaces: Vec<String>,
    /// What the network is.
    pub network: String,
    /// Whether privileges can be gained.
    pub no_new_privileges: bool,
    /// Whether the run is privileged.
    pub privileged: bool,
    /// Whether the root filesystem is read-only.
    pub read_only_root: bool,
    /// The unprivileged account it runs as.
    pub user: String,
}

/// What is mounted, and how.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "kebab-case", deny_unknown_fields)]
pub struct MountPolicy {
    /// Every input, mounted read-only.
    pub read_only: Vec<String>,
    /// How large the temporary filesystems may be.
    pub temporary_filesystem_bytes: u64,
    /// The one writable root that leaves the container.
    pub writable_output_root: String,
    /// The writable root a build works in, which does not leave the container.
    ///
    /// Separate from the output root because what a build produces is not
    /// evidence, and separate from the temporary filesystem because that one is
    /// held in memory and a build of this workspace is far larger than a
    /// machine should be asked to hold.
    pub writable_build_root: String,
}

/// The runtime that enforces the isolation.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "kebab-case", deny_unknown_fields)]
pub struct RuntimePolicy {
    /// Whether it runs without a daemon.
    pub daemonless: bool,
    /// What it is called.
    pub name: String,
    /// Whether it runs without privilege.
    pub rootless: bool,
    /// Exactly which version of it.
    pub version: String,
}

/// One gate's outcome inside an acceptance run.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GateOutcome {
    /// Which gate it is.
    pub name: String,
    /// Whether it held.
    pub outcome: String,
    /// What its report digests to.
    pub report_sha256: String,
}

/// What one acceptance run concluded.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AcceptanceManifest {
    /// Which row coordinated it.
    pub coordinator_row: String,
    /// The format this manifest declares.
    pub format: String,
    /// Every gate, in order.
    pub gates: Vec<GateOutcome>,
    /// What the isolation contract digests to.
    pub isolation_sha256: String,
    /// Whether the revision is releasable.
    pub outcome: String,
    /// What the platform evidence digests to.
    pub platform_evidence_sha256: String,
    /// Which provider run produced it.
    pub provider_run: String,
    /// The review record every input is bound to.
    pub rustsec_review_record_sha256: String,
    /// The exact revision it is about.
    pub source_commit: String,
    /// The exact tree that revision names.
    pub source_tree: String,
}

/// What a gate that held is written as.
pub const HELD: &str = "held";

/// What a revision that may be released is written as.
pub const RELEASABLE: &str = "releasable";

/// Why an acceptance run, or the isolation behind it, is refused.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum AcceptanceRefusal {
    /// A document could not be read.
    #[error("this document could not be read: {0}")]
    Unreadable(String),
    /// It declares another format.
    #[error("{expected} was expected and {held} is declared")]
    ForeignFormat {
        /// What was expected.
        expected: &'static str,
        /// What is declared.
        held: String,
    },
    /// The isolation would not deny what it claims to deny.
    #[error("{0}, so a descendant of this run could reach what acceptance says it cannot")]
    IsolationWeakened(String),
    /// A gate the inventory names did not run.
    #[error("{0} is a gate acceptance requires and this run does not record")]
    GateMissing(String),
    /// A gate ran twice.
    #[error("{0} is recorded more than once, so one of them decided nothing")]
    GateRepeated(String),
    /// The gates are not in the order acceptance runs them.
    #[error("{held} is recorded where {expected} belongs")]
    GateOutOfOrder {
        /// What belongs there.
        expected: String,
        /// What is there.
        held: String,
    },
    /// A gate ran and refused.
    #[error("{0} refused, and a revision with a refused gate is not releasable")]
    GateRefused(String),
    /// A gate this inventory does not name was recorded.
    #[error("{0} is recorded and acceptance requires no such gate")]
    GateUnknown(String),
    /// The manifest is about another revision.
    #[error("this manifest is about {held}, and this run is about {expected}")]
    RevisionDrift {
        /// What this run is about.
        expected: String,
        /// What the manifest is about.
        held: String,
    },
    /// The manifest concluded something its gates do not support.
    #[error("this manifest concludes {0} on evidence that does not support it")]
    OutcomeUnsupported(String),
}

/// Returns the isolation contract one document carries.
///
/// # Errors
///
/// Returns [`AcceptanceRefusal`] naming the first thing that stops it being an
/// isolation this build would run inside.
pub fn parse_container(text: &str) -> Result<AcceptanceContainer, AcceptanceRefusal> {
    let held: AcceptanceContainer = toml::from_str(text)
        .map_err(|failure| AcceptanceRefusal::Unreadable(failure.to_string()))?;
    if held.format != CONTAINER_FORMAT {
        return Err(AcceptanceRefusal::ForeignFormat {
            expected: CONTAINER_FORMAT,
            held: held.format,
        });
    }
    require_isolation_real(&held)?;
    Ok(held)
}

/// Requires every denial the contract claims to be one it actually makes.
fn require_isolation_real(held: &AcceptanceContainer) -> Result<(), AcceptanceRefusal> {
    require_nothing_reachable(&held.isolation)?;
    require_nothing_granted(&held.isolation)?;
    require_image_and_runtime(held)
}

/// Requires nothing outside the container to be reachable from inside it.
fn require_nothing_reachable(isolation: &IsolationPolicy) -> Result<(), AcceptanceRefusal> {
    let weakened = |what: String| AcceptanceRefusal::IsolationWeakened(what);
    if isolation.network != NETWORK_NONE {
        return Err(weakened(format!("the network is {}", isolation.network)));
    }
    if isolation.host_engine_socket {
        return Err(weakened("an engine socket is reachable".to_owned()));
    }
    if !isolation.host_namespaces.is_empty() {
        return Err(weakened("a host namespace is joined".to_owned()));
    }
    if !isolation.host_devices.is_empty() {
        return Err(weakened("a host device is exposed".to_owned()));
    }
    Ok(())
}

/// Requires the run to hold no authority beyond running the gates.
fn require_nothing_granted(isolation: &IsolationPolicy) -> Result<(), AcceptanceRefusal> {
    let weakened = |what: &str| AcceptanceRefusal::IsolationWeakened(what.to_owned());
    if isolation.privileged {
        return Err(weakened("the run is privileged"));
    }
    if !isolation.no_new_privileges {
        return Err(weakened("privileges can be gained inside"));
    }
    if !isolation.read_only_root {
        return Err(weakened("the root filesystem is writable"));
    }
    if !isolation.add_capabilities.is_empty() {
        return Err(weakened("a capability is added back"));
    }
    if isolation.drop_capabilities != [DROP_EVERYTHING] {
        return Err(weakened("something short of every capability is dropped"));
    }
    Ok(())
}

/// Requires the image and the runtime to be the ones that were reviewed.
fn require_image_and_runtime(held: &AcceptanceContainer) -> Result<(), AcceptanceRefusal> {
    let weakened = |what: &str| AcceptanceRefusal::IsolationWeakened(what.to_owned());
    if !held.runtime.rootless || !held.runtime.daemonless {
        return Err(weakened("the runtime is not rootless and daemonless"));
    }
    if held.image.pull {
        return Err(weakened("the image is fetched rather than loaded from what was transferred"));
    }
    if !held.image.digest.starts_with(DIGEST_PREFIX) {
        return Err(weakened("the image is named rather than pinned"));
    }
    if held.mounts.writable_output_root.trim().is_empty() {
        return Err(weakened("nothing is writable, so a run could produce no evidence"));
    }
    if held.mounts.writable_build_root.trim().is_empty() {
        return Err(weakened("a build has nowhere to work, so no gate could run"));
    }
    Ok(())
}

/// Returns the manifest one document carries.
///
/// # Errors
///
/// Returns [`AcceptanceRefusal::Unreadable`] for a document this cannot read
/// and [`AcceptanceRefusal::ForeignFormat`] for another format.
pub fn parse_manifest(text: &str) -> Result<AcceptanceManifest, AcceptanceRefusal> {
    let held: AcceptanceManifest = serde_json::from_str(text)
        .map_err(|failure| AcceptanceRefusal::Unreadable(failure.to_string()))?;
    if held.format != MANIFEST_FORMAT {
        return Err(AcceptanceRefusal::ForeignFormat {
            expected: MANIFEST_FORMAT,
            held: held.format,
        });
    }
    Ok(held)
}

/// Requires one manifest to record every gate, once, in order, all holding.
///
/// # Errors
///
/// Returns [`AcceptanceRefusal`] naming which of the five ways a gate record
/// can be wrong this one is.
pub fn require_complete(manifest: &AcceptanceManifest) -> Result<(), AcceptanceRefusal> {
    let mut seen = BTreeSet::new();
    for recorded in &manifest.gates {
        if !REQUIRED_GATES.contains(&recorded.name.as_str()) {
            return Err(AcceptanceRefusal::GateUnknown(recorded.name.clone()));
        }
        if !seen.insert(recorded.name.clone()) {
            return Err(AcceptanceRefusal::GateRepeated(recorded.name.clone()));
        }
    }
    for required in REQUIRED_GATES {
        if !seen.contains(*required) {
            return Err(AcceptanceRefusal::GateMissing((*required).to_owned()));
        }
    }
    for (position, required) in REQUIRED_GATES.iter().enumerate() {
        let recorded = &manifest.gates[position];
        if recorded.name != *required {
            return Err(AcceptanceRefusal::GateOutOfOrder {
                expected: (*required).to_owned(),
                held: recorded.name.clone(),
            });
        }
        if recorded.outcome != HELD {
            return Err(AcceptanceRefusal::GateRefused(recorded.name.clone()));
        }
    }
    if manifest.outcome != RELEASABLE {
        return Err(AcceptanceRefusal::OutcomeUnsupported(manifest.outcome.clone()));
    }
    Ok(())
}

/// Requires one manifest to be about the revision being accepted.
///
/// # Errors
///
/// Returns [`AcceptanceRefusal::RevisionDrift`] when it is about another one.
pub fn require_revision(
    manifest: &AcceptanceManifest,
    source_commit: &str,
) -> Result<(), AcceptanceRefusal> {
    if manifest.source_commit != source_commit {
        return Err(AcceptanceRefusal::RevisionDrift {
            expected: source_commit.to_owned(),
            held: manifest.source_commit.clone(),
        });
    }
    Ok(())
}
