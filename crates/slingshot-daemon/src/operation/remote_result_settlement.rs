//! Writing one ending down, once, or not at all.
//!
//! This is the settlement Task 2202 hands an ending to. Its whole
//! responsibility is that the write is all-or-nothing: the snapshot, the state,
//! the result, and the artifact facts land together or none of them does, and a
//! caller that is refused keeps exactly what it was holding.
//!
//! # Capacity refusal is not failure
//!
//! A daemon with no room for a result has still learned that the work
//! succeeded, and forgetting that would be worse than not having the bytes. So
//! a refusal records the remote success and its retention facts and publishes
//! no local result, and the way out is maintenance and a resume that fetches
//! the same result through the same remote identities. It is never a
//! resubmission: the command already ran.

use slingshot_agent_connection::job_snapshot_reconciliation::{
    SettlementRefusal, TerminalFacts, TerminalSettlement,
};
use slingshot_agent_connection::structured_job_result::{
    LocalDisposition, ValidatedResult, local_disposition,
};

/// What settling one ending produced.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SettlementOutcome {
    /// The ending and its result are written down.
    Published {
        /// Where the result is kept.
        disposition: LocalDisposition,
    },
    /// The work succeeded remotely and there is no room to keep the result.
    ///
    /// Nonterminal on purpose. The operation is not finished locally, because
    /// nothing was published; but nothing about it is unknown either, and a
    /// later maintenance run plus a resume fetches the same result.
    PersistentCapacityUnavailable {
        /// How long the agent will still hold it.
        remaining_retention_milliseconds: u64,
    },
    /// It succeeded remotely and the result can no longer be fetched.
    ResultUnavailable,
}

/// Where a settlement writes, and how much room it has.
pub trait ResultStore: ::core::fmt::Debug {
    /// Returns whether there is room for `bytes` of result.
    fn has_room_for(&self, bytes: u64) -> bool;

    /// Writes one ending and its result together.
    ///
    /// # Errors
    ///
    /// Returns [`SettlementRefusal`] when the transaction will not commit,
    /// which must leave every fact exactly as it was.
    fn publish(
        &self,
        facts: &TerminalFacts,
        result: &ValidatedResult,
    ) -> Result<(), SettlementRefusal>;

    /// Records that the work succeeded remotely and nothing was published.
    fn record_capacity_unavailable(&self, facts: &TerminalFacts);
}

/// One settlement over one store.
#[derive(Debug)]
pub struct RemoteResultSettlement<'store> {
    /// What was validated, when a result came with the ending.
    result: Option<ValidatedResult>,
    /// Where it writes.
    store: &'store dyn ResultStore,
}

impl<'store> RemoteResultSettlement<'store> {
    /// Returns a settlement that will publish `result` into `store`.
    #[must_use]
    pub fn of(store: &'store dyn ResultStore, result: Option<ValidatedResult>) -> Self {
        Self { result, store }
    }

    /// Returns what settling `facts` produced.
    ///
    /// # Errors
    ///
    /// Returns [`SettlementRefusal`] when the store declines, which leaves
    /// every snapshot, job, and result fact where it was.
    pub fn settle_with_outcome(
        &self,
        facts: &TerminalFacts,
    ) -> Result<SettlementOutcome, SettlementRefusal> {
        let Some(result) = &self.result else {
            return Ok(SettlementOutcome::ResultUnavailable);
        };
        let bytes = u64::try_from(result.canonical_result.len()).unwrap_or(u64::MAX);
        if !self.store.has_room_for(bytes) {
            self.store.record_capacity_unavailable(facts);
            return Ok(SettlementOutcome::PersistentCapacityUnavailable {
                remaining_retention_milliseconds: facts.remaining_retention_milliseconds,
            });
        }
        self.store.publish(facts, result)?;
        Ok(SettlementOutcome::Published {
            disposition: local_disposition(bytes).unwrap_or(LocalDisposition::Inline),
        })
    }
}

impl TerminalSettlement for RemoteResultSettlement<'_> {
    fn settle(&self, facts: &TerminalFacts) -> Result<(), SettlementRefusal> {
        match self.settle_with_outcome(facts)? {
            SettlementOutcome::Published { .. } => Ok(()),
            SettlementOutcome::PersistentCapacityUnavailable { .. } => {
                Err(SettlementRefusal::Declined {
                    reason: "there is no room to keep this result".to_owned(),
                })
            }
            SettlementOutcome::ResultUnavailable => Err(SettlementRefusal::Declined {
                reason: "this ending arrived without a result to keep".to_owned(),
            }),
        }
    }
}
