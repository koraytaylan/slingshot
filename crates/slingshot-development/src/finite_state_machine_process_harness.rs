//! Running the pinned executor, this product, and a daemon as real processes.
//!
//! The integration boundary is executable behaviour rather than a shared
//! library, so the only proof that means anything is several real processes
//! talking to each other. This composes them: which executables are involved,
//! what each of them may see, and what happens to every one of them when a
//! scenario ends however it ends.
//!
//! # Nothing is found, everything is supplied
//!
//! No role resolves an executable from a home directory, a sibling checkout, a
//! source-local target directory, or anything installed. Each is supplied by
//! path, and a missing one refuses the scenario rather than falling back to
//! something that happens to be on this machine - which would prove a
//! compatibility claim about the wrong build.
//!
//! # A private root, and a hostile one left alone
//!
//! Every scenario acts under a root it created. A scenario that reached the
//! production root would be one whose evidence depends on the machine it ran
//! on, so a sentinel is placed in a decoy production root and its bytes are
//! compared afterwards: untouched, or the scenario proved nothing.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

/// The variable that supplies the pinned executor.
pub const EXECUTOR_VARIABLE: &str = "SLINGSHOT_FINITE_STATE_MACHINE_EXECUTABLE";

/// The variable that supplies this product's executable.
pub const PRODUCT_VARIABLE: &str = "SLINGSHOT_EXECUTABLE";

/// What a decoy production root holds, so a scenario reaching it is visible.
pub const SENTINEL_CONTENT: &str = "a scenario that reads this reached the wrong root";

/// What that decoy file is called.
pub const SENTINEL_FILE: &str = "production-sentinel.txt";

/// Variables a child never inherits.
///
/// Each of them lets something outside the scenario decide what a child does:
/// where it reads configuration, which toolchain it runs, what it links, and
/// where it writes. A scenario that inherited them would be measuring this
/// machine rather than this build.
pub const REMOVED_VARIABLES: &[&str] = &[
    "CARGO",
    "CARGO_BUILD_RUSTFLAGS",
    "CARGO_BUILD_TARGET_DIR",
    "CARGO_HOME",
    "CARGO_TARGET_DIR",
    "RUSTC_WRAPPER",
    "RUSTFLAGS",
    "RUSTUP_TOOLCHAIN",
    "RUST_LOG",
    "SLINGSHOT_CONFIGURATION_ROOT",
];

/// Which part one process plays.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Role {
    /// The pinned external executor.
    Executor,
    /// This product's protocol server.
    ProtocolServer,
    /// This product's daemon.
    Daemon,
}

impl Role {
    /// Returns which variable supplies this role's executable.
    #[must_use]
    pub fn variable(self) -> &'static str {
        match self {
            Self::Executor => EXECUTOR_VARIABLE,
            Self::ProtocolServer | Self::Daemon => PRODUCT_VARIABLE,
        }
    }
}

/// Why a scenario cannot run.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum HarnessRefusal {
    /// An executable this scenario needs was not supplied.
    #[error("{0} names no executable, and this scenario finds none for itself")]
    Unsupplied(String),
    /// A supplied path does not name a runnable executable.
    #[error("{path} is not a runnable executable: {detail}")]
    Unusable {
        /// What was supplied.
        path: PathBuf,
        /// What is wrong with it.
        detail: String,
    },
    /// The decoy production root was touched.
    #[error("a scenario reached the production root, so what it proved is about this machine")]
    ProductionRootTouched,
}

/// Where one scenario's processes act.
#[derive(Debug)]
pub struct ScenarioRoots {
    /// The root every process in this scenario acts under.
    private: PathBuf,
    /// A root nothing may touch, holding the sentinel.
    decoy: PathBuf,
}

impl ScenarioRoots {
    /// Creates a private root and a decoy production root beside it.
    ///
    /// # Errors
    ///
    /// Returns what the operating system said, unchanged.
    pub fn create(under: &Path) -> Result<Self, String> {
        let private = under.join("private");
        let decoy = under.join("decoy-production");
        for held in [&private, &decoy] {
            std::fs::create_dir_all(held).map_err(|failure| failure.to_string())?;
        }
        std::fs::write(decoy.join(SENTINEL_FILE), SENTINEL_CONTENT)
            .map_err(|failure| failure.to_string())?;
        Ok(Self { private, decoy })
    }

    /// Returns the root this scenario's processes act under.
    #[must_use]
    pub fn private(&self) -> &Path {
        &self.private
    }

    /// Returns the root nothing may touch.
    #[must_use]
    pub fn decoy(&self) -> &Path {
        &self.decoy
    }

    /// Requires the decoy root to be exactly as it was left.
    ///
    /// # Errors
    ///
    /// Returns [`HarnessRefusal::ProductionRootTouched`].
    pub fn require_untouched(&self) -> Result<(), HarnessRefusal> {
        let held = std::fs::read_to_string(self.decoy.join(SENTINEL_FILE)).unwrap_or_default();
        let alone =
            std::fs::read_dir(&self.decoy).map(|entries| entries.count() == 1).unwrap_or(false);
        if held == SENTINEL_CONTENT && alone {
            Ok(())
        } else {
            Err(HarnessRefusal::ProductionRootTouched)
        }
    }
}

/// Returns the environment one role's child sees.
///
/// Built rather than inherited, and built the same way for every role, so the
/// difference between two scenarios is what they did rather than what the
/// machine happened to be exporting.
#[must_use]
pub fn closed_environment(roots: &ScenarioRoots) -> BTreeMap<String, String> {
    BTreeMap::from([
        ("HOME".to_owned(), roots.private().to_string_lossy().into_owned()),
        ("PATH".to_owned(), String::new()),
    ])
}

/// Returns whether one variable is removed from every child's environment.
#[must_use]
pub fn is_removed(named: &str) -> bool {
    REMOVED_VARIABLES.contains(&named)
}

/// Returns the executable one role was supplied, if it was supplied one.
///
/// # Errors
///
/// Returns [`HarnessRefusal::Unsupplied`] when the variable names nothing and
/// [`HarnessRefusal::Unusable`] when what it names cannot be run.
pub fn supplied(role: Role) -> Result<PathBuf, HarnessRefusal> {
    let named = std::env::var(role.variable())
        .map_err(|_| HarnessRefusal::Unsupplied(role.variable().to_owned()))?;
    if named.trim().is_empty() {
        return Err(HarnessRefusal::Unsupplied(role.variable().to_owned()));
    }
    let path = PathBuf::from(named);
    slingshot_test_support::finite_state_machine_executable::FiniteStateMachineExecutable::at(
        path.clone(),
    )
    .map(|held| held.path().to_path_buf())
    .map_err(|failure| HarnessRefusal::Unusable { path, detail: failure.to_string() })
}

/// Reports whether every role this scenario needs was supplied.
#[must_use]
pub fn every_role_supplied(roles: &[Role]) -> bool {
    roles.iter().all(|role| supplied(*role).is_ok())
}
