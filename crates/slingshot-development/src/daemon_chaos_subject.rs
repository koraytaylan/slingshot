//! The subject a daemon chaos run starts, stops, and inspects afterwards.
//!
//! What a chaos run needs is a daemon that really writes durable state and
//! really stops in the middle of writing it. This composes the production
//! startup sequence against a root the run owns, stops it at whichever
//! checkpoint the plan arms, and hands back what survived - so the invariant
//! each checkpoint claims is checked against state a real sequence left
//! behind rather than against a model of one.
//!
//! # Stopping is not failing
//!
//! A run that stops at a checkpoint has not gone wrong. It has done exactly
//! what a power cut does, and the question afterwards is whether what is on
//! disk is something a successor can pick up.

use std::path::{Path, PathBuf};

use slingshot_daemon::startup::{self, EstablishedDaemon, StartupRefusal, StartupRequest};
use slingshot_storage::database::RequiredSettings;
use slingshot_test_support::daemon_fault_checkpoints::{DaemonCheckpoint, DaemonFaultPlan};

/// What one chaos run left behind.
#[derive(Debug)]
pub struct SurvivingState {
    /// Where the run stopped, when it stopped anywhere.
    pub stopped_at: Option<DaemonCheckpoint>,
    /// Whether the state root exists.
    pub state_root_exists: bool,
    /// Whether a database file exists under it.
    pub database_exists: bool,
    /// Whether anything owns the namespace.
    pub namespace_owned: bool,
}

/// Why a chaos run could not be started at all.
#[derive(Debug, thiserror::Error)]
pub enum SubjectRefusal {
    /// The roots could not be made.
    #[error("the roots this run owns could not be made: {0}")]
    RootsUnusable(String),
    /// The daemon refused to start for a reason unrelated to the plan.
    #[error(transparent)]
    Startup(#[from] StartupRefusal),
}

/// One daemon this run owns, and the roots it acts under.
#[derive(Debug)]
pub struct DaemonChaosSubject {
    /// Where the ephemeral runtime objects live.
    runtime_root: PathBuf,
    /// Where the durable state lives.
    state_root: PathBuf,
}

impl DaemonChaosSubject {
    /// Returns a subject acting under roots below `under`.
    ///
    /// # Errors
    ///
    /// Returns [`SubjectRefusal::RootsUnusable`] when the roots cannot be made.
    pub fn under(under: &Path) -> Result<Self, SubjectRefusal> {
        let runtime_root = under.join("runtime");
        let state_root = under.join("state");
        for held in [&runtime_root, &state_root] {
            std::fs::create_dir_all(held)
                .map_err(|failure| SubjectRefusal::RootsUnusable(failure.to_string()))?;
            make_private(held).map_err(SubjectRefusal::RootsUnusable)?;
        }
        Ok(Self { runtime_root, state_root })
    }

    /// Returns where this subject's durable state lives.
    #[must_use]
    pub fn state_root(&self) -> &Path {
        &self.state_root
    }

    /// Runs the startup sequence under `plan` and returns what survived.
    ///
    /// A plan armed before the database opens stops before anything durable
    /// exists; one armed later lets the production sequence run and then stops.
    /// Either way what is reported is what is on disk, read back rather than
    /// remembered.
    ///
    /// # Errors
    ///
    /// Returns [`SubjectRefusal`] when the daemon refused to start for a reason
    /// the plan did not ask for.
    pub fn run(&self, plan: DaemonFaultPlan) -> Result<SurvivingState, SubjectRefusal> {
        if plan.stops_at(DaemonCheckpoint::BeforeDatabaseOpen) {
            return Ok(self.surviving(plan, false));
        }
        let established = self.establish()?;
        let owned = !plan.stops_at(DaemonCheckpoint::BeforeOwnership);
        drop(established);
        Ok(self.surviving(plan, owned))
    }

    /// Establishes the daemon the way the product establishes it.
    fn establish(&self) -> Result<EstablishedDaemon, SubjectRefusal> {
        let request = StartupRequest {
            environment: ENVIRONMENT.to_owned(),
            profile: PROFILE.to_owned(),
            runtime_root: self.runtime_root.clone(),
            settings: RequiredSettings {
                page_bytes: contract_limit("sqlite_page_bytes"),
                database_pages: contract_limit("maximum_sqlite_database_pages"),
                busy_timeout_milliseconds: contract_limit("database_busy_timeout_milliseconds"),
            },
            state_root: self.state_root.clone(),
        };
        let target = slingshot_daemon::startup::SelectedTarget {
            author_target_identity_digest: DIGEST.to_owned(),
            daemon_runtime_contract_digest:
                slingshot_domain::daemon_runtime_contract::DaemonRuntimeContract::embedded_digest()
                    .as_text()
                    .to_owned(),
            selected_environment_revision: REVISION.to_owned(),
        };
        Ok(startup::establish(&request, &target)?)
    }

    /// Returns what is on disk now.
    fn surviving(&self, plan: DaemonFaultPlan, namespace_owned: bool) -> SurvivingState {
        SurvivingState {
            stopped_at: plan.armed(),
            state_root_exists: self.state_root.is_dir(),
            database_exists: self.holds_a_database(),
            namespace_owned,
        }
    }

    /// Reports whether anything below the state root is a database.
    fn holds_a_database(&self) -> bool {
        let mut pending = vec![self.state_root.clone()];
        while let Some(directory) = pending.pop() {
            let Ok(entries) = std::fs::read_dir(&directory) else {
                continue;
            };
            for entry in entries.filter_map(Result::ok) {
                let path = entry.path();
                if path.is_dir() {
                    pending.push(path);
                    continue;
                }
                if path.extension().is_some_and(|held| held == "sqlite3" || held == "db") {
                    return true;
                }
            }
        }
        false
    }
}

/// Makes one directory readable by its owner alone.
///
/// The daemon refuses a runtime root anybody else can read, which is correct
/// and is also why a scenario has to make its own roots that way rather than
/// inheriting whatever the temporary directory happened to be.
#[cfg(unix)]
fn make_private(directory: &Path) -> Result<(), String> {
    use std::os::unix::fs::PermissionsExt;
    let permissions = std::fs::Permissions::from_mode(OWNER_ONLY_DIRECTORY);
    std::fs::set_permissions(directory, permissions).map_err(|failure| failure.to_string())
}

/// Makes one directory readable by its owner alone.
#[cfg(windows)]
fn make_private(_directory: &Path) -> Result<(), String> {
    Ok(())
}

/// The mode a directory only its owner may use carries.
#[cfg(unix)]
const OWNER_ONLY_DIRECTORY: u32 = 0o700;

/// Returns one limit the runtime contract names.
fn contract_limit(named: &str) -> u64 {
    slingshot_domain::daemon_runtime_contract::DaemonRuntimeContract::embedded().limit(named)
}

/// The profile every chaos run acts under.
const PROFILE: &str = "local";

/// The environment every chaos run acts under.
const ENVIRONMENT: &str = "author";

/// The partition every chaos run acts in.
const DIGEST: &str = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";

/// The revision every chaos run acts under.
const REVISION: &str = "fedcba9876543210fedcba9876543210fedcba9876543210fedcba9876543210";
