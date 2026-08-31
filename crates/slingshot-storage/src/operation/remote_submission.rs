//! The fence that makes one logical effect at most one.
//!
//! Sling delivers at least once and a cluster runs more than one daemon, so
//! "may I run this now" cannot be answered by whichever process asked most
//! recently. It is answered by a durable conditional write, and the condition
//! is the point.
//!
//! # Nothing takes the claim back after the checkpoint
//!
//! A higher fence takes the right to start from a lower one, which is how a
//! replacement node picks up work whose holder went away. But once the
//! checkpoint is recorded, no fence takes anything: a lease that expired after
//! the work started is not a licence for somebody else to start it again, and a
//! requeue, a retry, a restart, and a node replacement are all the same story.
//! After it, an unresolved outcome stays unresolved.
//!
//! # Attempts are counted when an effect may have happened
//!
//! The outbox attempt count moves with the checkpoint rather than with the
//! claim, so it counts the times a command effect may exist rather than the
//! times one was contemplated. A count that moved on claiming would make a
//! daemon that crashed before starting look like one that had already run.

use rusqlite::OptionalExtension as _;

use crate::agent_job_repository::{
    AgentRepositoryFailure, ONE_ROW, counted, statement, stored, write_transaction,
};
use crate::database::OperationDatabase;

/// What claiming the right to start produced.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ClaimOutcome {
    /// This worker now holds the right to start.
    Claimed,
    /// The work has started, so nothing may claim it.
    AlreadyStarted,
    /// Another worker holds a fence at least as high.
    Fenced,
}

/// What recording the no-return checkpoint produced.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CheckpointOutcome {
    /// The checkpoint is recorded, and this is the only time it will be.
    Recorded,
    /// Somebody else holds the fence, or it is already recorded.
    Refused,
}

/// What one submission's fence says.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FenceFacts {
    /// The no-return checkpoint, once starting has happened.
    pub execution_checkpoint: Option<String>,
    /// How many times an effect may exist.
    pub outbox_attempts: u64,
    /// Which worker holds the right to start.
    pub worker_fence: Option<u64>,
}

impl FenceFacts {
    /// Returns whether the work has passed the point of no return.
    #[must_use]
    pub fn has_started(&self) -> bool {
        self.execution_checkpoint.is_some()
    }

    /// Returns whether anything may still authorize a command effect.
    ///
    /// Only before the checkpoint. Afterwards the honest answer to an
    /// unresolved outcome is that nobody knows, not that it may be tried again.
    #[must_use]
    pub fn permits_another_effect(&self) -> bool {
        !self.has_started()
    }
}

/// Claims the right to start one submission at `fence`.
///
/// # Errors
///
/// Returns [`AgentRepositoryFailure::Statement`] when the database refuses.
pub fn claim(
    database: &OperationDatabase,
    author_target_identity_digest: &str,
    agent_operation_identifier: &str,
    fence: u64,
) -> Result<ClaimOutcome, AgentRepositoryFailure> {
    let connection = database.connection();
    let transaction = write_transaction(connection)?;
    let changed = transaction.execute(
        statement("claim the right to start one agent submission"),
        (stored(fence), author_target_identity_digest, agent_operation_identifier, stored(fence)),
    )?;
    if changed == ONE_ROW {
        transaction.commit()?;
        return Ok(ClaimOutcome::Claimed);
    }
    let held = read_fence(&transaction, author_target_identity_digest, agent_operation_identifier)?;
    Ok(match held {
        Some(facts) if facts.has_started() => ClaimOutcome::AlreadyStarted,
        _ => ClaimOutcome::Fenced,
    })
}

/// Records the no-return checkpoint, if this worker still holds the fence.
///
/// # Errors
///
/// Returns [`AgentRepositoryFailure::Statement`] when the database refuses.
pub fn checkpoint(
    database: &OperationDatabase,
    author_target_identity_digest: &str,
    agent_operation_identifier: &str,
    fence: u64,
    marker: &str,
) -> Result<CheckpointOutcome, AgentRepositoryFailure> {
    let changed = database.connection().execute(
        statement("record the no-return checkpoint on one agent submission"),
        (marker, author_target_identity_digest, agent_operation_identifier, stored(fence)),
    )?;
    Ok(if changed == ONE_ROW { CheckpointOutcome::Recorded } else { CheckpointOutcome::Refused })
}

/// Returns what one submission's fence says.
///
/// # Errors
///
/// Returns [`AgentRepositoryFailure::Statement`] when the database refuses.
pub fn fence_facts(
    database: &OperationDatabase,
    author_target_identity_digest: &str,
    agent_operation_identifier: &str,
) -> Result<Option<FenceFacts>, AgentRepositoryFailure> {
    read_fence(database.connection(), author_target_identity_digest, agent_operation_identifier)
}

/// Returns what one submission's fence says, over any connection.
fn read_fence(
    connection: &rusqlite::Connection,
    author_target_identity_digest: &str,
    agent_operation_identifier: &str,
) -> Result<Option<FenceFacts>, AgentRepositoryFailure> {
    let mut prepared =
        connection.prepare(statement("read one agent submission's execution fence"))?;
    Ok(prepared
        .query_row((author_target_identity_digest, agent_operation_identifier), |row| {
            Ok(FenceFacts {
                execution_checkpoint: row.get("execution_checkpoint")?,
                outbox_attempts: counted(row.get::<_, i64>("outbox_attempts")?),
                worker_fence: row.get::<_, Option<i64>>("worker_fence")?.map(counted),
            })
        })
        .optional()?)
}
