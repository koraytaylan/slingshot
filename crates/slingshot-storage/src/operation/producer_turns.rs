//! Persisted round-robin ordering within the scheduler's immediate transaction.

use std::collections::{BTreeMap, BTreeSet};

use crate::operation_repository::RepositoryFailure;
use crate::sqlite_statement_inventory::statement_text;

use super::scheduler_claim::QueuedCandidate;

/// The increment from one committed turn to its successor.
const NEXT_TURN: i64 = 1;

/// One transaction's current producer order; no process-local history is needed.
pub(super) struct ProducerTurns {
    /// Keys distinguish the shared default from every explicit identity.
    turns: BTreeMap<String, i64>,
}

impl ProducerTurns {
    /// Prunes inactive producers, appends arrivals in enqueue order and sorts
    /// the candidates stably, preserving each producer's own enqueue order.
    pub(super) fn order(
        transaction: &rusqlite::Transaction<'_>,
        target: &str,
        candidates: &mut [QueuedCandidate],
    ) -> Result<Self, RepositoryFailure> {
        let mut statement =
            transaction.prepare(statement_text("read producer turns for one target"))?;
        let turns = statement
            .query_map([target], |row| Ok((row.get("producer_key")?, row.get("turn_sequence")?)))?
            .collect::<Result<BTreeMap<String, i64>, _>>()?;
        let retained: BTreeSet<String> = candidates.iter().map(producer_key).collect();
        let mut held = Self { turns };
        for key in held.turns.keys().filter(|key| !retained.contains(*key)) {
            transaction.execute(
                statement_text("remove an inactive producer turn"),
                rusqlite::params![target, key],
            )?;
        }
        held.turns.retain(|key, _| retained.contains(key));
        for candidate in candidates.iter() {
            if !held.turns.contains_key(&producer_key(candidate)) {
                held.rotate(transaction, target, candidate)?;
            }
        }
        candidates.sort_by_key(|candidate| held.turns[&producer_key(candidate)]);
        Ok(held)
    }

    /// Retires a producer immediately when this claim consumes its last queued
    /// row. A new arrival between ticks must join behind other new producers.
    pub(super) fn claimed(
        &mut self,
        transaction: &rusqlite::Transaction<'_>,
        target: &str,
        candidate: &QueuedCandidate,
        candidates: &[QueuedCandidate],
    ) -> Result<(), RepositoryFailure> {
        self.rotate(transaction, target, candidate)?;
        if !candidates.iter().any(|other| {
            other.caller_identity == candidate.caller_identity
                && other.operation_identifier != candidate.operation_identifier
        }) {
            let key = producer_key(candidate);
            transaction.execute(
                statement_text("remove an inactive producer turn"),
                rusqlite::params![target, key],
            )?;
            self.turns.remove(&key);
        }
        Ok(())
    }

    /// Updates exactly one producer after a successful claim, in that same
    /// transaction. Overflow refuses and rolls back the claim and ordering.
    fn rotate(
        &mut self,
        transaction: &rusqlite::Transaction<'_>,
        target: &str,
        candidate: &QueuedCandidate,
    ) -> Result<(), RepositoryFailure> {
        let last = self.turns.values().copied().max().unwrap_or_default();
        let next = last.checked_add(NEXT_TURN).ok_or_else(|| RepositoryFailure::NotDecodable {
            column: "turn_sequence",
            detail: "producer turn sequence exhausted; no claim was committed".to_owned(),
        })?;
        let key = producer_key(candidate);
        transaction.execute(
            statement_text("append or rotate one producer turn"),
            rusqlite::params![target, key, next],
        )?;
        self.turns.insert(key, next);
        Ok(())
    }
}

/// Encodes the default and explicit identities without aliases.
fn producer_key(candidate: &QueuedCandidate) -> String {
    candidate.caller_identity.as_ref().map_or_else(String::new, |identity| format!(":{identity}"))
}
