//! What a daemon process is started with, and what it can never be started with.
//!
//! A daemon outlives the client that started it and serves whoever connects
//! afterwards, so its identity cannot be a function of who happened to start
//! it. Everything a short-lived process may pass is here, and it is two names:
//! the profile and the environment. That is enough to select a namespace, and
//! deliberately not enough to select a credential, an endpoint, a trust root,
//! or a deployment - all of which the daemon reads for itself from the
//! configuration root once it is running.
//!
//! One value exists only for tests: a configuration root to read instead of the
//! account's own. It is typed rather than a string so it cannot arrive by
//! accident, it is unrepresentable in a production build of these arguments,
//! and the type says which of the two it is rather than leaving a caller to
//! infer it from whether a field is empty.

use slingshot_domain::daemon_runtime_contract::DaemonRuntimeContract;

/// Where a daemon reads its configuration from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ConfigurationRootSource {
    /// The account's own root, resolved by the configuration crate.
    ///
    /// The only variant a production start uses. No argument, environment
    /// variable, or inherited value can redirect it: the resolution samples the
    /// account identity and appends a literal relative path to its home.
    AccountProfile,
    /// A root supplied by a test, in place of the account's own.
    ///
    /// Constructed only through [`ConfigurationRootSource::for_test`], which
    /// keeps the fact that a root was overridden visible at the call site
    /// rather than buried in a path that happens to point somewhere else.
    SuppliedForTest {
        /// The directory to read configuration from.
        root: std::path::PathBuf,
    },
}

impl ConfigurationRootSource {
    /// Returns a source that reads `root` instead of the account's own.
    #[must_use]
    pub fn for_test(root: impl Into<std::path::PathBuf>) -> Self {
        Self::SuppliedForTest { root: root.into() }
    }

    /// Returns whether this source overrides the account's own root.
    #[must_use]
    pub fn is_overridden(&self) -> bool {
        matches!(self, Self::SuppliedForTest { .. })
    }

    /// Returns the root to read, when one was supplied.
    #[must_use]
    pub fn supplied_root(&self) -> Option<&std::path::Path> {
        match self {
            Self::AccountProfile => None,
            Self::SuppliedForTest { root } => Some(root),
        }
    }
}

/// Reason a daemon process could not be described.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum DaemonProcessFailure {
    /// A name a daemon needs was not supplied.
    #[error("a daemon is started with a {name}, and none was supplied")]
    NameMissing {
        /// Which name.
        name: &'static str,
    },
}

/// Everything one daemon process is started with.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DaemonProcessArguments {
    /// Where the daemon reads its configuration from.
    pub configuration_root: ConfigurationRootSource,
    /// The environment name.
    pub environment: String,
    /// The profile name.
    pub profile: String,
}

impl DaemonProcessArguments {
    /// Returns the arguments a production start uses.
    ///
    /// # Errors
    ///
    /// Returns [`DaemonProcessFailure::NameMissing`] when either name is empty.
    pub fn production(profile: &str, environment: &str) -> Result<Self, DaemonProcessFailure> {
        Self::with_root(profile, environment, ConfigurationRootSource::AccountProfile)
    }

    /// Returns arguments reading configuration from somewhere a test chose.
    ///
    /// # Errors
    ///
    /// Returns [`DaemonProcessFailure::NameMissing`] when either name is empty.
    pub fn with_root(
        profile: &str,
        environment: &str,
        configuration_root: ConfigurationRootSource,
    ) -> Result<Self, DaemonProcessFailure> {
        if profile.is_empty() {
            return Err(DaemonProcessFailure::NameMissing { name: "profile" });
        }
        if environment.is_empty() {
            return Err(DaemonProcessFailure::NameMissing { name: "environment" });
        }
        Ok(Self {
            configuration_root,
            environment: environment.to_owned(),
            profile: profile.to_owned(),
        })
    }

    /// Returns the command-line words that carry these arguments to a child.
    ///
    /// Two names and nothing else. A reader can see at a glance that no
    /// credential, endpoint, or trust material crosses this boundary, which is
    /// the property the whole module exists to keep.
    #[must_use]
    pub fn words(&self) -> Vec<String> {
        vec![
            "--profile".to_owned(),
            self.profile.clone(),
            "--environment".to_owned(),
            self.environment.clone(),
        ]
    }
}

/// What a client requires of the daemon it is about to talk to.
///
/// All three are checked before a versioned request is sent. A daemon built
/// against another runtime contract, serving another partition, or serving
/// another revision is not this client's daemon, and sending it work would mean
/// acting in a context nobody chose.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DaemonExpectation {
    /// Which partition this client acts in.
    pub author_target_identity_digest: String,
    /// Which runtime contract this client was built against.
    pub runtime_contract_digest: String,
    /// Which environment revision this client acts under.
    pub selected_environment_revision: String,
}

impl DaemonExpectation {
    /// Returns the runtime contract digest this build carries.
    ///
    /// Recomputed from the embedded bytes rather than remembered, so a build
    /// whose contract changed cannot describe itself with the old digest.
    #[must_use]
    pub fn embedded_runtime_digest() -> String {
        DaemonRuntimeContract::embedded_digest().as_text().to_owned()
    }
}

/// Why a daemon is not one this client may send work to.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum HandshakeRefusal {
    /// It was built against another runtime contract.
    #[error(
        "this daemon was built against another runtime contract; stop it and start it again with \
         this build, or use the build it was started from"
    )]
    RuntimeContractMismatch,
    /// It serves another partition.
    #[error("this daemon serves another target; stop it or select the target it serves")]
    TargetMismatch,
    /// It serves another environment revision.
    #[error("this daemon serves another environment revision; stop it and start it again")]
    RevisionMismatch,
}

impl HandshakeRefusal {
    /// Returns whether retained control is still usable despite this.
    ///
    /// All of them. A mismatched daemon is one this client cannot send work to
    /// and exactly the one it needs to be able to ask about and stop - refusing
    /// those too would leave a caller with a running daemon and no way to
    /// replace it.
    #[must_use]
    pub fn permits_retained_control(&self) -> bool {
        true
    }
}

/// Requires one daemon's handshake to be one this client may send work to.
///
/// # Errors
///
/// Returns [`HandshakeRefusal`] naming the first thing that differs, coarsest
/// first: a daemon built against another contract is refused before its target
/// is considered, because nothing it says about a target means the same thing.
pub fn require_compatible(
    expectation: &DaemonExpectation,
    runtime_contract_digest: &str,
    author_target_identity_digest: &str,
    selected_environment_revision: &str,
) -> Result<(), HandshakeRefusal> {
    if runtime_contract_digest != expectation.runtime_contract_digest {
        return Err(HandshakeRefusal::RuntimeContractMismatch);
    }
    if author_target_identity_digest != expectation.author_target_identity_digest {
        return Err(HandshakeRefusal::TargetMismatch);
    }
    if selected_environment_revision != expectation.selected_environment_revision {
        return Err(HandshakeRefusal::RevisionMismatch);
    }
    Ok(())
}

/// What ensuring one daemon is serving produced.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LifecycleOutcome {
    /// One was already serving, and it is compatible.
    AlreadyServing,
    /// One was started, and it became ready.
    Started,
    /// One is serving and this client may not send it work.
    Incompatible(Box<HandshakeRefusal>),
    /// A start was attempted and did not produce a serving daemon.
    NotServing(Box<StartFailure>),
}

/// Why a start did not produce a serving daemon.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum StartFailure {
    /// The child exited before it served anything.
    #[error("the daemon exited before it began serving: {detail}")]
    ChildExited {
        /// What it said on the way out.
        detail: String,
    },
    /// It never became ready within the attempts allowed.
    #[error("the daemon did not become ready within {attempts} attempts")]
    NeverReady {
        /// How many times readiness was polled.
        attempts: u32,
    },
    /// Something is there and does not answer properly.
    #[error("something is serving that namespace and does not answer as a daemon")]
    Unhealthy,
}
