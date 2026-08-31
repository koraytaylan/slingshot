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
