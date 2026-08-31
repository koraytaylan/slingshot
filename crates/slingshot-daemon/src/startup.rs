//! Establishing what this daemon serves, before anything can reach it.
//!
//! Startup is an ordered sequence whose whole point is that it fails closed.
//! Every step either establishes a fact or refuses, and a refusal leaves no
//! endpoint, no readiness record, and not one changed byte of durable state. A
//! client that can see readiness can therefore assume everything below it
//! already held.
//!
//! The order is not arbitrary. The namespace comes first because it names where
//! everything else lives. Installation identity comes next, because a database
//! must be able to say which installation created it. The database follows, and
//! only then the audit, because the audit is a question about rows.
//!
//! The audit is the step worth explaining. A daemon serves exactly one author
//! target at one environment revision, and the durable state it opens may hold
//! work admitted by some earlier daemon under a different identity. Finished
//! work is history and stays queryable - a client asking what happened deserves
//! an answer. Unfinished work is different: it was admitted by something that
//! knew a different set of facts about who it was talking to, and adopting it
//! would mean executing against a security context nobody chose. So any
//! unfinished row under another target or revision refuses startup, unchanged
//! and unreconciled, and a person decides what to do about it.
//!
//! The daemon reads its target from the configuration root rather than from a
//! client's arguments. A short-lived process that could hand a long-lived one
//! its endpoint or credentials would make the daemon's identity a function of
//! whoever started it, which is exactly the property this refuses to have.

use slingshot_storage::database::{DatabaseFailure, OperationDatabase, RequiredSettings};
use slingshot_storage::installation_state::InstallationStateFailure;
use slingshot_storage::sqlite_statement_inventory::statement_text;

use crate::runtime_namespace::{NamespaceFailure, PersistentTargetPaths, RuntimeNamespace};

/// Purpose of the statement the cross-partition audit runs.
///
/// The text lives in the storage inventory, which is the one place a statement
/// exists; the question is asked here, because auditing is what startup does
/// and not what a repository does.
pub const UNFINISHED_PARTITIONS_STATEMENT: &str =
    "list every partition holding work that has not ended";

/// What this daemon selected, and therefore what it will serve.
///
/// Opaque values consumed whole. Nothing here reconstructs a principal, a
/// metascope, or a trust root from them, because a digest that could be taken
/// apart would be a digest that had stopped being opaque.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SelectedTarget {
    /// The opaque author-target identity digest.
    pub author_target_identity_digest: String,
    /// The digest of the runtime contract this daemon runs under.
    pub daemon_runtime_contract_digest: String,
    /// The environment revision this selection is at.
    pub selected_environment_revision: String,
}

/// One partition's unfinished work, as the audit found it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UnfinishedPartition {
    /// The target that admitted it.
    pub author_target_identity_digest: String,
    /// The revision it was admitted under.
    pub selected_environment_revision: String,
}

/// Reason a daemon refused to start.
///
/// Each one names the step that refused, because "startup failed" tells an
/// operator nothing about which invariant was unavailable.
#[derive(Debug, thiserror::Error)]
pub enum StartupRefusal {
    /// The profile and environment do not name a namespace.
    #[error("the namespace could not be named: {0}")]
    Namespace(#[from] NamespaceFailure),
    /// The installation state could not be established.
    #[error("the installation identity could not be established: {0}")]
    Installation(#[from] InstallationStateFailure),
    /// The database could not be opened or brought current.
    #[error("the operation database could not be opened: {0}")]
    Database(#[from] DatabaseFailure),
    /// A SQLite invariant this daemon requires could not be verified.
    #[error("the {invariant} invariant could not be verified, so nothing was bound")]
    InvariantUnavailable {
        /// Which invariant.
        invariant: &'static str,
    },
    /// Durable state holds unfinished work belonging to somebody else.
    #[error(
        "{count} unfinished operations belong to another target or revision; \
         startup changed nothing, and they are neither failed nor reconciled here"
    )]
    ForeignWorkOutstanding {
        /// How many partitions hold it.
        count: usize,
        /// The partitions, so an operator can see whose work it is.
        partitions: Vec<UnfinishedPartition>,
    },
}

/// What one startup needs before it can establish anything.
#[derive(Debug, Clone)]
pub struct StartupRequest {
    /// The environment name, which with the profile names the namespace.
    pub environment: String,
    /// The profile name.
    pub profile: String,
    /// The ephemeral per-user runtime root.
    pub runtime_root: std::path::PathBuf,
    /// The settings every database connection is held to.
    pub settings: RequiredSettings,
    /// The persistent per-user state root.
    pub state_root: std::path::PathBuf,
}

/// The facts a completed startup established.
#[derive(Debug)]
pub struct EstablishedDaemon {
    /// The open, current database.
    pub database: OperationDatabase,
    /// The namespace this daemon owns.
    pub namespace: RuntimeNamespace,
    /// Where this target's durable state lives.
    pub paths: PersistentTargetPaths,
    /// What this daemon serves.
    pub target: SelectedTarget,
}

/// Establishes everything a daemon needs before it may bind an endpoint.
///
/// The steps run in one order and stop at the first refusal. Nothing is bound,
/// published, or executed here: this returns the facts, and the caller decides
/// what to do with them.
///
/// # Errors
///
/// Returns [`StartupRefusal`] naming the step that refused. Every refusal
/// leaves durable state exactly as it found it.
pub fn establish(
    request: &StartupRequest,
    target: &SelectedTarget,
) -> Result<EstablishedDaemon, StartupRefusal> {
    let contract = slingshot_local_protocol::foundation_contract::FoundationContract::embedded();
    let namespace = RuntimeNamespace::name(
        &contract,
        &request.runtime_root,
        &request.profile,
        &request.environment,
    )?;
    namespace.create_runtime_directory()?;
    let paths = namespace.beneath(&request.state_root);
    paths.create()?;

    let database = OperationDatabase::open(&paths.database_path(), request.settings)?;
    require_verifiable_invariants(&database)?;
    require_no_foreign_work(&database, target)?;
    Ok(EstablishedDaemon { database, namespace, paths, target: target.clone() })
}

/// Requires every SQLite invariant this daemon depends on to be verifiable.
///
/// Unverifiable is refused exactly like violated. A daemon that shipped
/// because it could not check something would be a daemon whose guarantees
/// depend on the check having been reachable, which is not a guarantee.
fn require_verifiable_invariants(database: &OperationDatabase) -> Result<(), StartupRefusal> {
    database
        .require_compile_options()
        .map_err(|_| StartupRefusal::InvariantUnavailable { invariant: "pinned SQLite build" })?;
    database
        .schema_version()
        .map_err(|_| StartupRefusal::InvariantUnavailable { invariant: "schema version" })?;
    Ok(())
}

/// Refuses to start over unfinished work belonging to another identity.
///
/// Finished work is left alone and stays queryable. Unfinished work under
/// another target or revision is not failed, reconciled, or adopted: it is
/// reported, and startup stops.
fn require_no_foreign_work(
    database: &OperationDatabase,
    target: &SelectedTarget,
) -> Result<(), StartupRefusal> {
    let unfinished = unfinished_partitions(database)?;
    let foreign: Vec<UnfinishedPartition> = unfinished
        .into_iter()
        .filter(|partition| {
            partition.author_target_identity_digest != target.author_target_identity_digest
                || partition.selected_environment_revision != target.selected_environment_revision
        })
        .collect();
    if foreign.is_empty() {
        return Ok(());
    }
    Err(StartupRefusal::ForeignWorkOutstanding { count: foreign.len(), partitions: foreign })
}

/// Returns every partition holding work that has not ended.
///
/// # Errors
///
/// Returns [`StartupRefusal::InvariantUnavailable`] when the rows cannot be
/// read, because an audit that could not run is an audit that did not pass.
pub fn unfinished_partitions(
    database: &OperationDatabase,
) -> Result<Vec<UnfinishedPartition>, StartupRefusal> {
    let unavailable =
        || StartupRefusal::InvariantUnavailable { invariant: "cross-partition audit" };
    let mut prepared = database
        .connection()
        .prepare(statement_text(UNFINISHED_PARTITIONS_STATEMENT))
        .map_err(|_| unavailable())?;
    let rows = prepared
        .query_map([], |row| {
            Ok(UnfinishedPartition {
                author_target_identity_digest: row.get(0)?,
                selected_environment_revision: row.get(1)?,
            })
        })
        .map_err(|_| unavailable())?;
    rows.collect::<Result<Vec<UnfinishedPartition>, _>>().map_err(|_| unavailable())
}
