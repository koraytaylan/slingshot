//! Transactional event writes for the subscription ledger.

use super::*;

impl AgentSubscriptionLedger {
    /// Folds one event into one subscription, and says what that did.
    ///
    /// The association is optional and is not a foreign key. An event about
    /// work this daemon does not hold still moves the stream on, and refusing
    /// it would leave the position stuck behind events that will never be
    /// associated with anything.
    ///
    /// # Errors
    ///
    /// Returns [`AgentRepositoryFailure::NoSuchSubscription`] or
    /// [`AgentRepositoryFailure::Exhausted`].
    pub fn record_event(
        &self,
        author_target_identity_digest: &str,
        daemon_subscription_identifier: &str,
        fact: &EventFact,
        recorded_at_unix_milliseconds: u64,
    ) -> Result<LedgerOutcome, AgentRepositoryFailure> {
        let connection = self.database.connection();
        let transaction = write_transaction(connection)?;
        let outcome = self.record_event_within(
            &transaction,
            author_target_identity_digest,
            daemon_subscription_identifier,
            fact,
            recorded_at_unix_milliseconds,
        )?;
        transaction.commit()?;
        Ok(outcome)
    }

    /// Records one unresolved event disagreement without moving any cursor or job.
    /// The complete captured view is rechecked under the write lock. Repeated
    /// reports preserve the first incident and consume no additional slot.
    ///
    /// # Errors
    /// Refuses a stale/foreign view, invalid cursor or inconsistent accounting.
    pub fn record_event_conflict(
        &self,
        expected: &SubscriptionRecoveryView<'_>,
        cursor: &str,
    ) -> Result<(), AgentRepositoryFailure> {
        require_cursor_bound(cursor)?;
        let transaction = write_transaction(self.database.connection())?;
        self.require_recovery_current(&transaction, expected)?;
        Self::require_reconciled(
            &transaction,
            &expected.target,
            &expected.subscription,
            &expected.ledger,
        )?;
        transaction.execute(
            statement("record one unresolved integrity incident on a subscription"),
            (cursor, &expected.target, &expected.subscription),
        )?;
        transaction.commit()?;
        Ok(())
    }

    /// Commits a validated cursor-only event against an unchanged complete view.
    /// Associated events must be at or below the retained job sequence; the
    /// caller must separately validate equal-sequence replay contents. Unknown
    /// operations carry no association and never create a job row here.
    ///
    /// # Errors
    /// Refuses stale views, unresolved incidents or an unapplied job sequence.
    pub fn record_cursor_event(
        &self,
        expected: &SubscriptionRecoveryView<'_>,
        fact: &EventFact,
        now: u64,
    ) -> Result<LedgerOutcome, AgentRepositoryFailure> {
        require_cursor_bound(&fact.cursor)?;
        if expected.ledger.unresolved_incident.is_some()
            || now > i64::MAX as u64
            || fact.event_bytes > i64::MAX as u64
        {
            return Err(AgentRepositoryFailure::Conflicted);
        }
        require_event_digest(&fact.canonical_digest)?;
        match (&fact.agent_operation_identifier, fact.job_sequence) {
            (None, None) => {}
            (Some(identifier), Some(sequence)) => {
                let member = expected
                    .members
                    .iter()
                    .find(|member| &member.identity.agent_operation_identifier == identifier)
                    .ok_or(AgentRepositoryFailure::Conflicted)?;
                if member.identity.agent_event_store_generation != fact.agent_event_store_generation
                    || sequence > member.observation.applied_sequence.value()
                    || sequence > i64::MAX as u64
                    || now < member.recorded_at_unix_milliseconds
                {
                    return Err(AgentRepositoryFailure::Conflicted);
                }
            }
            _ => return Err(AgentRepositoryFailure::Conflicted),
        }
        let transaction = write_transaction(self.database.connection())?;
        self.require_recovery_current(&transaction, expected)?;
        let outcome = self.record_event_within(
            &transaction,
            &expected.target,
            &expected.subscription,
            fact,
            now,
        )?;
        transaction.commit()?;
        Ok(outcome)
    }

    /// Atomically persists an already validated, contiguous nonterminal event.
    ///
    /// The captured view binds the ledger, local owners, submissions and physical
    /// associations. Every one is rechecked under the write lock. An event does
    /// not renew retention or settle a local operation. Terminal observations,
    /// sequence gaps and job-level replays use other reconciliation paths.
    ///
    /// # Errors
    ///
    /// Refuses stale views, invalid associations/transitions or exhausted capacity.
    /// Any failure rolls back the physical association, observation and cursor.
    pub fn record_active_event(
        &self,
        expected: &SubscriptionRecoveryView<'_>,
        fact: &EventFact,
        observation: slingshot_domain::remote_job::RemoteJobObservation,
        sling_job_identifier: &str,
        now: u64,
    ) -> Result<LedgerOutcome, AgentRepositoryFailure> {
        use slingshot_domain::remote_job::AgentJobIdentifier;
        require_cursor_bound(&fact.cursor)?;
        let index = expected
            .members
            .iter()
            .position(|member| {
                fact.agent_operation_identifier.as_deref()
                    == Some(member.identity.agent_operation_identifier.as_str())
            })
            .ok_or(AgentRepositoryFailure::Conflicted)?;
        let member = &expected.members[index];
        require_active_binding(expected, fact, index)?;
        require_active_transition(member, fact, &observation)?;
        if AgentJobIdentifier::new(sling_job_identifier).is_err()
            || now < member.recorded_at_unix_milliseconds
            || [
                now,
                observation.applied_sequence.value(),
                observation.attempt,
                observation.progress,
                fact.event_bytes,
            ]
            .into_iter()
            .any(|value| i64::try_from(value).is_err())
        {
            return Err(AgentRepositoryFailure::Conflicted);
        }
        require_event_digest(&fact.canonical_digest)?;
        let transaction = write_transaction(self.database.connection())?;
        self.require_recovery_current(&transaction, expected)?;
        let outcome = self.record_event_within(
            &transaction,
            &expected.target,
            &expected.subscription,
            fact,
            now,
        )?;
        if outcome == LedgerOutcome::Advanced {
            let physical = &expected.physical_jobs[index];
            if !physical.iter().any(|name| name == sling_job_identifier) {
                if physical.len() as u64 >= self.bounds.physical_job_rows {
                    return Err(AgentRepositoryFailure::Exhausted {
                        allowed: self.bounds.physical_job_rows,
                        subject: "physical Sling jobs",
                    });
                }
                let changed = transaction.execute(
                    statement("record one physical Sling job for one agent submission"),
                    (
                        &member.identity.agent_operation_identifier,
                        &expected.target,
                        stored(now),
                        sling_job_identifier,
                    ),
                )?;
                if changed != ONE_ROW {
                    return Err(AgentRepositoryFailure::Conflicted);
                }
            }
            let changed = transaction.execute(
                statement("fold one believed event into one agent submission"),
                (
                    stored(observation.applied_sequence.value()),
                    stored(observation.attempt),
                    observation.state.to_string(),
                    stored(observation.progress),
                    &expected.target,
                    &member.identity.agent_operation_identifier,
                    stored(member.observation.applied_sequence.value()),
                    observation.state.to_string(),
                    stored(observation.attempt),
                    stored(observation.progress),
                ),
            )?;
            if changed != ONE_ROW {
                return Err(AgentRepositoryFailure::Conflicted);
            }
        }
        transaction.commit()?;
        Ok(outcome)
    }

    /// Records a cursor inside the transaction that also owns its job writes.
    pub(super) fn record_event_within(
        &self,
        transaction: &rusqlite::Transaction<'_>,
        author_target_identity_digest: &str,
        daemon_subscription_identifier: &str,
        fact: &EventFact,
        recorded_at_unix_milliseconds: u64,
    ) -> Result<LedgerOutcome, AgentRepositoryFailure> {
        let held = read_subscription(
            transaction,
            author_target_identity_digest,
            daemon_subscription_identifier,
        )?
        .ok_or_else(|| AgentRepositoryFailure::NoSuchSubscription {
            identifier: daemon_subscription_identifier.to_owned(),
        })?;
        if held.agent_event_store_generation != fact.agent_event_store_generation {
            return Ok(LedgerOutcome::GenerationMismatch);
        }
        Self::require_reconciled(
            transaction,
            author_target_identity_digest,
            daemon_subscription_identifier,
            &held,
        )?;
        let outcome = classify(&held, fact);
        if matches!(outcome, LedgerOutcome::Advanced) {
            self.require_event_room(&held, fact.event_bytes)?;
            transaction.execute(
                statement("advance one subscription ledger to a later position"),
                (
                    &fact.canonical_digest,
                    &fact.cursor,
                    stored(fact.event_bytes),
                    author_target_identity_digest,
                    daemon_subscription_identifier,
                    stored(fact.agent_event_store_generation),
                    &fact.cursor,
                ),
            )?;
            transaction.execute(
                statement("record one subscription event"),
                (
                    stored(fact.agent_event_store_generation),
                    &fact.agent_operation_identifier,
                    author_target_identity_digest,
                    &fact.canonical_digest,
                    &fact.cursor,
                    daemon_subscription_identifier,
                    "advanced",
                    stored(fact.event_bytes),
                    fact.job_sequence.map(stored),
                    stored(recorded_at_unix_milliseconds),
                ),
            )?;
        }
        if matches!(outcome, LedgerOutcome::IntegrityConflict) {
            transaction.execute(
                statement("record one unresolved integrity incident on a subscription"),
                (&fact.cursor, author_target_identity_digest, daemon_subscription_identifier),
            )?;
        }
        Ok(outcome)
    }

    /// Requires room for one more event before one is written.
    fn require_event_room(
        &self,
        held: &SubscriptionLedgerRow,
        incoming_bytes: u64,
    ) -> Result<(), AgentRepositoryFailure> {
        if incoming_bytes > crate::agent_job_repository::BYTES_PER_EVENT {
            return Err(AgentRepositoryFailure::EventTooLarge {
                provided: incoming_bytes,
                allowed: crate::agent_job_repository::BYTES_PER_EVENT,
            });
        }
        if held.event_rows.checked_add(1).is_none_or(|rows| rows > self.bounds.event_rows) {
            return Err(AgentRepositoryFailure::Exhausted {
                allowed: self.bounds.event_rows,
                subject: "retained events",
            });
        }
        if held
            .event_bytes
            .checked_add(incoming_bytes)
            .is_none_or(|bytes| bytes > self.bounds.event_bytes)
        {
            return Err(AgentRepositoryFailure::Exhausted {
                allowed: self.bounds.event_bytes,
                subject: "retained event bytes",
            });
        }
        Ok(())
    }
}

pub(super) fn require_event_digest(digest: &str) -> Result<(), AgentRepositoryFailure> {
    if digest.len() != slingshot_domain::command_fingerprint::DIGEST_CHARACTERS
        || !digest.bytes().all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    {
        return Err(AgentRepositoryFailure::Conflicted);
    }
    Ok(())
}

pub(super) fn require_completed_identity(
    database: &OperationDatabase,
    expected: &CompletedEventView<'_>,
    fact: &EventFact,
    physical: &str,
) -> Result<(), AgentRepositoryFailure> {
    if !core::ptr::eq(expected.owner, database)
        || expected.ledger.unresolved_incident.is_some()
        || fact.agent_event_store_generation
            != expected.member.identity.agent_event_store_generation
        || fact.agent_event_store_generation != expected.ledger.agent_event_store_generation
        || fact.agent_operation_identifier.as_deref()
            != Some(expected.member.identity.agent_operation_identifier.as_str())
        || fact.job_sequence.is_none_or(|sequence| {
            sequence > expected.member.observation.applied_sequence.value()
                || sequence > i64::MAX as u64
        })
        || !expected.physical.iter().any(|name| name == physical)
    {
        return Err(AgentRepositoryFailure::Conflicted);
    }
    Ok(())
}

fn require_active_binding(
    expected: &SubscriptionRecoveryView<'_>,
    fact: &EventFact,
    index: usize,
) -> Result<(), AgentRepositoryFailure> {
    let member = &expected.members[index];
    let local = expected.local_owners[index].as_ref().ok_or(AgentRepositoryFailure::Conflicted)?;
    if expected.ledger.unresolved_incident.is_some()
        || member.identity.agent_event_store_generation != fact.agent_event_store_generation
        || expected.ledger.agent_event_store_generation != fact.agent_event_store_generation
        || local.selected_environment_revision != member.identity.selected_environment_revision
        || local.record.lifecycle_state.is_terminal()
        || local.record.outstanding_recovery.as_ref().is_some_and(|recovery|
            recovery.evidence == slingshot_domain::operation::RecoveryExecutionEvidence::AuthoritativeRemoteSuccess)
        || member.terminal_disposition.is_some()
    {
        return Err(AgentRepositoryFailure::Conflicted);
    }
    Ok(())
}

fn require_active_transition(
    member: &AgentSubmission,
    fact: &EventFact,
    observation: &slingshot_domain::remote_job::RemoteJobObservation,
) -> Result<(), AgentRepositoryFailure> {
    if observation.state.is_terminal()
        || fact.job_sequence != Some(observation.applied_sequence.value())
        || !observation.applied_sequence.immediately_follows(member.observation.applied_sequence)
        || observation.applied_sequence <= member.snapshot_watermark
        || member
            .observation
            .advanced(
                observation.state,
                observation.applied_sequence,
                observation.attempt,
                observation.progress,
            )
            .is_err()
    {
        return Err(AgentRepositoryFailure::Conflicted);
    }
    Ok(())
}
