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

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::process::Command;

use serde::{Deserialize, Serialize};
use sha2::Digest as _;

use crate::cargo_executable;

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

/// Where the container mounts the cache every gate resolves against.
pub const CACHE_MOUNT: &str = "/cache";

/// Where the container mounts the verified pinned external source.
pub const FINITE_STATE_MACHINE_MOUNT: &str = "/finite-state-machine";

/// Where the container mounts the authenticated platform evidence.
pub const PLATFORM_EVIDENCE_MOUNT: &str = "/platform-evidence";

/// Where the container mounts the same-run owner review record.
pub const REVIEW_RECORD_MOUNT: &str = "/review-record.json";

/// What a run writes its decision as.
pub const MANIFEST_FILE_NAME: &str = "acceptance.json";

/// The command that runs the gates and writes down what they decided.
///
/// Declared here rather than beside the dispatcher because the script inside
/// the container invokes it by name and nothing held the two together - which
/// is how a release came to ask for a command no executable carried.
pub const RUN_COMMAND: &str = "run-release-acceptance";

/// What a run writes one gate's report as, after the gate's own name.
pub const REPORT_FILE_SUFFIX: &str = ".report";

/// The package that carries this repository's own commands.
const DEVELOPMENT_PACKAGE: &str = "slingshot-development";

/// The package that owns the command contract.
const DOMAIN_PACKAGE: &str = "slingshot-domain";

/// Cargo's subcommand that runs one of this repository's own commands.
const RUN_SUBCOMMAND: &str = "run";

/// Cargo's subcommand that runs one integration target.
const TEST_SUBCOMMAND: &str = "test";

/// Cargo's flag that refuses to change the committed lockfile.
const LOCKED_FLAG: &str = "--locked";

/// Cargo's flag that refuses to change the lockfile or the cache.
const FROZEN_FLAG: &str = "--frozen";

/// Cargo's flag that refuses the network.
const OFFLINE_FLAG: &str = "--offline";

/// Cargo's flag that keeps a gate's report to what the gate itself wrote.
const QUIET_FLAG: &str = "--quiet";

/// Cargo's flag that names a package.
const PACKAGE_FLAG: &str = "--package";

/// Cargo's flag that names one integration target.
const TEST_TARGET_FLAG: &str = "--test";

/// What separates Cargo's own arguments from the ones it passes on.
const ARGUMENT_SEPARATOR: &str = "--";

/// What separates a path from its content in a tree digest.
const TREE_DIGEST_SEPARATOR: u8 = 0;

/// How much of a file a digest reads at a time.
const FILE_READ_WINDOW_BYTES: usize = 65_536;

/// What running one gate is.
///
/// Naming the kind rather than the whole invocation keeps the flags that make a
/// gate offline in one place, where a row somebody adds later cannot omit them.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GateSubject {
    /// A command this repository's own executable carries.
    RepositoryCommand(&'static [&'static str]),
    /// One package's integration target.
    IntegrationTarget {
        /// Which package owns it.
        package: &'static str,
        /// Which target it is.
        target: &'static str,
    },
    /// A script this repository commits.
    Script {
        /// Which script it is, relative to the source root.
        path: &'static str,
        /// What the script is given.
        arguments: &'static [&'static str],
    },
}

/// One gate one acceptance run holds, and what holding it is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AcceptanceGate {
    /// Which gate it is.
    pub name: &'static str,
    /// What running it is.
    pub subject: GateSubject,
}

impl AcceptanceGate {
    /// Returns the program that runs this gate.
    ///
    /// Cargo exports its own path while it runs, and this executable was
    /// started by one, so a Cargo gate uses that Cargo rather than whichever a
    /// search path finds first. A script gate is a file of the source being
    /// decided, so it is resolved against that source and nothing else.
    #[must_use]
    pub fn program(&self, source_root: &Path) -> PathBuf {
        match self.subject {
            GateSubject::Script { path, .. } => source_root.join(path),
            GateSubject::RepositoryCommand(_) | GateSubject::IntegrationTarget { .. } => {
                cargo_executable()
            }
        }
    }

    /// Returns the arguments this gate is run with.
    ///
    /// Every Cargo gate is locked, frozen, and offline: the container has no
    /// network, and a gate that resolved anything would refuse for the
    /// isolation working rather than for anything about this revision.
    #[must_use]
    pub fn arguments(&self) -> Vec<String> {
        let owned = |held: Vec<&str>| held.into_iter().map(str::to_owned).collect();
        match self.subject {
            GateSubject::RepositoryCommand(passed) => {
                let mut held = vec![
                    RUN_SUBCOMMAND,
                    LOCKED_FLAG,
                    FROZEN_FLAG,
                    OFFLINE_FLAG,
                    QUIET_FLAG,
                    PACKAGE_FLAG,
                    DEVELOPMENT_PACKAGE,
                    ARGUMENT_SEPARATOR,
                ];
                held.extend_from_slice(passed);
                owned(held)
            }
            GateSubject::IntegrationTarget { package, target } => owned(vec![
                TEST_SUBCOMMAND,
                LOCKED_FLAG,
                FROZEN_FLAG,
                OFFLINE_FLAG,
                QUIET_FLAG,
                PACKAGE_FLAG,
                package,
                TEST_TARGET_FLAG,
                target,
            ]),
            GateSubject::Script { arguments, .. } => owned(arguments.to_vec()),
        }
    }
}

/// Every gate one acceptance run holds, in the order it holds them.
///
/// Ordered rather than merely enumerated, because the order is part of the
/// answer: source policy before anything is built from the source, the
/// contracts before the things that consume them, and the compatibility gate
/// last because it is the only one that runs another project's code.
pub const REQUIRED_GATES: &[AcceptanceGate] = &[
    AcceptanceGate {
        name: "source-policy",
        subject: GateSubject::RepositoryCommand(&["source-policy"]),
    },
    AcceptanceGate {
        name: "dependency-direction",
        subject: GateSubject::RepositoryCommand(&["dependency-direction"]),
    },
    AcceptanceGate {
        name: "workspace-module-map",
        subject: GateSubject::IntegrationTarget {
            package: DEVELOPMENT_PACKAGE,
            target: "workspace_module_map",
        },
    },
    AcceptanceGate {
        name: "release-metadata",
        subject: GateSubject::IntegrationTarget {
            package: DEVELOPMENT_PACKAGE,
            target: "release_metadata",
        },
    },
    AcceptanceGate {
        name: "release-attestation-policy",
        subject: GateSubject::IntegrationTarget {
            package: DEVELOPMENT_PACKAGE,
            target: "release_attestation_policy",
        },
    },
    AcceptanceGate {
        name: "locked-source-cache",
        subject: GateSubject::RepositoryCommand(&[
            "verify-locked-source-cache",
            "--cache-set",
            CACHE_MOUNT,
        ]),
    },
    AcceptanceGate {
        name: "command-contract",
        subject: GateSubject::IntegrationTarget {
            package: DOMAIN_PACKAGE,
            target: "command_contract_limits",
        },
    },
    AcceptanceGate {
        name: "protocol-compatibility",
        subject: GateSubject::IntegrationTarget {
            package: DEVELOPMENT_PACKAGE,
            target: "protocol_compatibility",
        },
    },
    AcceptanceGate {
        name: "configuration-and-storage-compatibility",
        subject: GateSubject::IntegrationTarget {
            package: DEVELOPMENT_PACKAGE,
            target: "configuration_and_storage_compatibility",
        },
    },
    AcceptanceGate {
        name: "platform-runtime",
        subject: GateSubject::IntegrationTarget {
            package: DEVELOPMENT_PACKAGE,
            target: "platform_runtime_contract",
        },
    },
    AcceptanceGate {
        name: "release-artifact-contract",
        subject: GateSubject::IntegrationTarget {
            package: DEVELOPMENT_PACKAGE,
            target: "release_artifact_contract",
        },
    },
    AcceptanceGate {
        name: "finite-state-machine-compatibility",
        subject: GateSubject::Script {
            path: "scripts/check_finite_state_machine_compatibility",
            arguments: &[
                "--finite-state-machine-source",
                FINITE_STATE_MACHINE_MOUNT,
                "--cargo-home-seed",
                CACHE_MOUNT,
            ],
        },
    },
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
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
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
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
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

/// What a gate that did not hold, and a revision that is not releasable, is
/// written as.
///
/// One word for both, because there is nothing between them: a gate that
/// refused makes the revision unreleasable, and a gate that could not be run at
/// all refused as surely as one that ran and said no.
pub const REFUSED: &str = "refused";

/// What a revision that may be released is written as.
pub const RELEASABLE: &str = "releasable";

/// The variable the exact revision being decided arrives in.
pub const SOURCE_COMMIT_VARIABLE: &str = "SLINGSHOT_ACCEPTANCE_SOURCE_COMMIT";

/// The variable the exact tree that revision names arrives in.
pub const SOURCE_TREE_VARIABLE: &str = "SLINGSHOT_ACCEPTANCE_SOURCE_TREE";

/// The variable the provider run that produced the decision arrives in.
pub const PROVIDER_RUN_VARIABLE: &str = "SLINGSHOT_ACCEPTANCE_PROVIDER_RUN";

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
    /// The run was told nothing about what it is deciding.
    #[error("{0} says which run this is, and this run was told no such thing")]
    RunUnbound(String),
    /// What the run decided could not be written down.
    #[error("this decision could not be written: {0}")]
    Unwritable(String),
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
        if !REQUIRED_GATES.iter().any(|required| required.name == recorded.name) {
            return Err(AcceptanceRefusal::GateUnknown(recorded.name.clone()));
        }
        if !seen.insert(recorded.name.clone()) {
            return Err(AcceptanceRefusal::GateRepeated(recorded.name.clone()));
        }
    }
    for required in REQUIRED_GATES {
        if !seen.contains(required.name) {
            return Err(AcceptanceRefusal::GateMissing(required.name.to_owned()));
        }
    }
    for (position, required) in REQUIRED_GATES.iter().enumerate() {
        let recorded = &manifest.gates[position];
        if recorded.name != required.name {
            return Err(AcceptanceRefusal::GateOutOfOrder {
                expected: required.name.to_owned(),
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

/// What one run was told about itself.
///
/// None of it is discoverable from inside, which is the point: a run that could
/// work out which revision it was could be persuaded it was a different one.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RunIdentity {
    /// The exact revision it is about.
    pub source_commit: String,
    /// The exact tree that revision names.
    pub source_tree: String,
    /// Which provider run produced it.
    pub provider_run: String,
}

/// What one run binds its decision to.
///
/// What the run was told, beside what it read for itself from the inputs it was
/// mounted: a decision that named an input without saying which bytes it read
/// would bind nothing at all.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RunBinding {
    /// Which row coordinated it.
    pub coordinator_row: String,
    /// What the isolation contract digests to.
    pub isolation_sha256: String,
    /// What the platform evidence digests to.
    pub platform_evidence_sha256: String,
    /// What the review record every input is bound to digests to.
    pub rustsec_review_record_sha256: String,
    /// What the run was told about itself.
    pub identity: RunIdentity,
}

/// What running one gate produced.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GateRun {
    /// Which gate it is.
    pub name: String,
    /// Whether the gate itself concluded that it held.
    pub held: bool,
    /// Everything the gate wrote, which is what its digest is over.
    pub report: Vec<u8>,
}

/// Returns the variables this run has.
///
/// Read once, into a value a caller holds, so what the decision was told can be
/// handed to it rather than fetched again where no assertion can reach.
#[must_use]
pub fn environment() -> BTreeMap<String, String> {
    std::env::vars().collect()
}

/// Returns what one run was told about itself.
///
/// A variable that is absent and one that is present and blank are the same
/// thing said two ways, and both are a run that was told nothing.
///
/// # Errors
///
/// Returns [`AcceptanceRefusal::RunUnbound`] naming a value the run was not
/// told, starting with the revision, because a decision that guessed its
/// revision would be a decision about something else.
pub fn told(environment: &BTreeMap<String, String>) -> Result<RunIdentity, AcceptanceRefusal> {
    let read = |variable: &str| {
        environment
            .get(variable)
            .map(|held| held.trim().to_owned())
            .filter(|held| !held.is_empty())
            .ok_or_else(|| AcceptanceRefusal::RunUnbound(variable.to_owned()))
    };
    Ok(RunIdentity {
        source_commit: read(SOURCE_COMMIT_VARIABLE)?,
        source_tree: read(SOURCE_TREE_VARIABLE)?,
        provider_run: read(PROVIDER_RUN_VARIABLE)?,
    })
}

/// Returns what one sequence of bytes digests to.
#[must_use]
pub fn digest_of_bytes(bytes: &[u8]) -> String {
    hex::encode(sha2::Sha256::digest(bytes))
}

/// Returns what every file under one directory digests to, together.
///
/// Sorted by relative path, each path fed in beside what its own bytes digest
/// to, so a file added, removed, renamed, or moved changes the answer. Not the
/// cache surveyor: that one leaves out the manifest it writes, and evidence has
/// nothing left out of it.
///
/// # Errors
///
/// Returns [`AcceptanceRefusal::Unreadable`] naming the first thing under the
/// directory this decision cannot read or cannot name.
pub fn digest_of_tree(root: &Path) -> Result<String, AcceptanceRefusal> {
    let mut relatives = Vec::new();
    collect_files(root, root, &mut relatives)?;
    relatives.sort();
    let mut digest = sha2::Sha256::new();
    for relative in &relatives {
        digest.update(relative.as_bytes());
        digest.update([TREE_DIGEST_SEPARATOR]);
        digest.update(digest_of_file(&root.join(relative))?.as_bytes());
    }
    Ok(hex::encode(digest.finalize()))
}

/// Returns what one file's bytes digest to, without holding them all at once.
///
/// The evidence a run is given includes the image the gates are running in, and
/// reading a file that size into memory to hash it would make the decision's own
/// footprint depend on how large the things it reads are.
fn digest_of_file(path: &Path) -> Result<String, AcceptanceRefusal> {
    use std::io::Read as _;

    let mut held = std::fs::File::open(path).map_err(|failure| unreadable(path, &failure))?;
    let mut digest = sha2::Sha256::new();
    let mut window = vec![0; FILE_READ_WINDOW_BYTES];
    loop {
        let read = held.read(&mut window).map_err(|failure| unreadable(path, &failure))?;
        if read == 0 {
            return Ok(hex::encode(digest.finalize()));
        }
        digest.update(&window[..read]);
    }
}

/// Collects every file under `directory`, named relative to `root`.
fn collect_files(
    root: &Path,
    directory: &Path,
    collected: &mut Vec<String>,
) -> Result<(), AcceptanceRefusal> {
    let listing =
        std::fs::read_dir(directory).map_err(|failure| unreadable(directory, &failure))?;
    for entry in listing {
        let entry = entry.map_err(|failure| unreadable(directory, &failure))?;
        let path = entry.path();
        let kind = entry.file_type().map_err(|failure| unreadable(&path, &failure))?;
        if kind.is_dir() {
            collect_files(root, &path, collected)?;
            continue;
        }
        // A link, whichever kind. Digesting one binds what it points at rather
        // than what is here, so two trees that differ in where a member came
        // from would digest the same; walking one could walk a cycle.
        if kind.is_symlink() {
            return Err(AcceptanceRefusal::Unreadable(format!(
                "{} points somewhere else, and a digest would bind where it points",
                path.display()
            )));
        }
        let relative = path.strip_prefix(root).unwrap_or(&path);
        let Some(named) = relative.to_str() else {
            return Err(AcceptanceRefusal::Unreadable(format!(
                "{} is not a path this decision can name",
                relative.display()
            )));
        };
        collected.push(named.to_owned());
    }
    Ok(())
}

/// Returns one input's bytes.
fn read_bytes(path: &Path) -> Result<Vec<u8>, AcceptanceRefusal> {
    std::fs::read(path).map_err(|failure| unreadable(path, &failure))
}

/// Returns the refusal for a path this decision was given and cannot read.
fn unreadable(path: &Path, failure: &std::io::Error) -> AcceptanceRefusal {
    AcceptanceRefusal::Unreadable(format!("{}: {failure}", path.display()))
}

/// Runs one gate and returns what it concluded.
///
/// A gate that could not be started at all did not hold, and its report says
/// why. There is no third answer: recording "could not run" as anything but a
/// refusal would let a missing tool, a target nobody spelled right, or an input
/// the container was never given read as evidence about this revision.
#[must_use]
pub fn run_gate(gate: &AcceptanceGate, source_root: &Path) -> GateRun {
    let program = gate.program(source_root);
    let started = Command::new(&program).args(gate.arguments()).current_dir(source_root).output();
    match started {
        Ok(finished) => {
            let mut report = finished.stdout;
            report.extend_from_slice(&finished.stderr);
            GateRun { name: gate.name.to_owned(), held: finished.status.success(), report }
        }
        Err(failure) => GateRun {
            name: gate.name.to_owned(),
            held: false,
            report: format!("{} could not be started: {failure}\n", program.display()).into_bytes(),
        },
    }
}

/// Returns what one run of the gates concluded.
///
/// Every gate that ran is recorded, whichever way it went. Stopping at the
/// first refusal would leave the gates after it absent, and a manifest with a
/// gate absent is refused for the one missing rather than the one that refused,
/// which tells whoever reads it the wrong thing.
#[must_use]
pub fn conclude(binding: &RunBinding, runs: &[GateRun]) -> AcceptanceManifest {
    let gates: Vec<GateOutcome> = runs
        .iter()
        .map(|run| GateOutcome {
            name: run.name.clone(),
            outcome: if run.held { HELD } else { REFUSED }.to_owned(),
            report_sha256: digest_of_bytes(&run.report),
        })
        .collect();
    // Unanimous among all of them, not among however many happened to run. A
    // run that recorded three gates and held all three has not agreed about
    // anything, and an outcome that only counted what it was handed would say
    // it had.
    let unanimous = gates.len() == REQUIRED_GATES.len()
        && gates.iter().all(|recorded| recorded.outcome == HELD);
    AcceptanceManifest {
        coordinator_row: binding.coordinator_row.clone(),
        format: MANIFEST_FORMAT.to_owned(),
        gates,
        isolation_sha256: binding.isolation_sha256.clone(),
        outcome: if unanimous { RELEASABLE } else { REFUSED }.to_owned(),
        platform_evidence_sha256: binding.platform_evidence_sha256.clone(),
        provider_run: binding.identity.provider_run.clone(),
        rustsec_review_record_sha256: binding.rustsec_review_record_sha256.clone(),
        source_commit: binding.identity.source_commit.clone(),
        source_tree: binding.identity.source_tree.clone(),
    }
}

/// Returns what this run binds its decision to.
///
/// # Errors
///
/// Returns [`AcceptanceRefusal::RunUnbound`] when the run was told nothing
/// about itself and [`AcceptanceRefusal`] otherwise naming the input the
/// container was given that could not be read.
pub fn bind(source_root: &Path) -> Result<RunBinding, AcceptanceRefusal> {
    // What the run was told, before anything it has to read. A run that does
    // not know which revision it is about has nothing to learn from the inputs,
    // and saying so first is the refusal that explains itself.
    let identity = told(&environment())?;
    let isolation = read_bytes(&source_root.join(CONTAINER_PATH))?;
    let held = parse_container(&String::from_utf8_lossy(&isolation))?;
    Ok(RunBinding {
        coordinator_row: held.coordinator.triple,
        isolation_sha256: digest_of_bytes(&isolation),
        platform_evidence_sha256: digest_of_tree(Path::new(PLATFORM_EVIDENCE_MOUNT))?,
        rustsec_review_record_sha256: digest_of_file(Path::new(REVIEW_RECORD_MOUNT))?,
        identity,
    })
}

/// Runs every gate, records what each concluded, and returns the decision.
///
/// Each report is written beside the manifest as it is produced: a manifest
/// that digests a report nobody kept binds a document nobody can read back.
///
/// # Errors
///
/// Returns [`AcceptanceRefusal`] when the run was told nothing about itself,
/// when an input the container was given cannot be read, or when a report
/// cannot be written beside the decision.
pub fn decide(
    source_root: &Path,
    destination: &Path,
) -> Result<AcceptanceManifest, AcceptanceRefusal> {
    let binding = bind(source_root)?;
    let mut runs = Vec::with_capacity(REQUIRED_GATES.len());
    for gate in REQUIRED_GATES {
        let run = run_gate(gate, source_root);
        let report = destination.join(format!("{}{REPORT_FILE_SUFFIX}", run.name));
        std::fs::write(&report, &run.report).map_err(|failure| {
            AcceptanceRefusal::Unwritable(format!("{}: {failure}", report.display()))
        })?;
        runs.push(run);
    }
    Ok(conclude(&binding, &runs))
}

/// Writes what one run decided and returns where it was written.
///
/// # Errors
///
/// Returns [`AcceptanceRefusal::Unwritable`] when the decision cannot be
/// rendered or cannot be written where it belongs.
pub fn record(
    manifest: &AcceptanceManifest,
    destination: &Path,
) -> Result<PathBuf, AcceptanceRefusal> {
    let path = destination.join(MANIFEST_FILE_NAME);
    let mut rendered = serde_json::to_string_pretty(manifest)
        .map_err(|failure| AcceptanceRefusal::Unwritable(failure.to_string()))?;
    rendered.push('\n');
    std::fs::write(&path, rendered).map_err(|failure| {
        AcceptanceRefusal::Unwritable(format!("{}: {failure}", path.display()))
    })?;
    Ok(path)
}
