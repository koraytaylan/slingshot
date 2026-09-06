//! Where each filtered event stream has got to, and what that cost.
//!
//! Separate from the submissions it carries events about, and with no foreign
//! key to them. A stream legitimately carries events about work this daemon
//! does not hold - another daemon's submission on a shared subscription, or its
//! own submission whose response has not arrived yet - so the position has to
//! be recordable with nothing to hang it on. A constraint requiring a job would
//! either refuse those events or invent a row to satisfy itself, and both are
//! worse than a nullable association.
//!
//! # The position moves forward or not at all
//!
//! Every advance is conditional on the stored position in SQL rather than on
//! something the caller checked first. A replay and a stale event both leave
//! the ledger exactly where it was, without the caller having to decide which
//! it was looking at, and two writers racing cannot both advance.
//!
//! # One disagreement is one incident
//!
//! A position arriving twice with different contents means the stream and this
//! record disagree, which no further streaming resolves. It consumes one slot,
//! and reporting it again consumes none: charging capacity per report would let
//! an agent exhaust the ledger by repeating itself. The only way out is a
//! captured high-water position for the whole subscription, because the
//! disagreement is about the stream's own position and one job knows nothing
//! about the others sharing it.

use rusqlite::OptionalExtension as _;
use slingshot_domain::persistent_capacity::PersistentCapacityPolicy;

use crate::agent_job_repository::{
    AgentCapacityBounds, AgentRepositoryFailure, AgentSubmission, ONE_ROW, counted, statement,
    stored, write_transaction,
};
use crate::database::OperationDatabase;

/// Where one subscription has got to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SubscriptionLedgerRow {
    /// Which incarnation of the agent's store it is following.
    pub agent_event_store_generation: u64,
    /// Digest of an observed event, absent at a captured snapshot boundary.
    pub canonical_digest: Option<String>,
    /// The position everything beneath has been compacted away below.
    pub compacted_below_cursor: Option<String>,
    /// The position it sits at.
    pub cursor: Option<String>,
    /// Bytes its retained events come to.
    pub event_bytes: u64,
    /// How many events it retains.
    pub event_rows: u64,
    /// The position a reset captured, which replay resumes above.
    pub high_water_cursor: Option<String>,
    /// The one unresolved disagreement it holds, when it holds one.
    pub unresolved_incident: Option<String>,
    /// How many incident slots it has consumed.
    pub unresolved_incident_count: u64,
}

/// What folding one event into the ledger did.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LedgerOutcome {
    /// The fact belongs to another event-store generation and changed nothing.
    GenerationMismatch,
    /// The ledger now sits at a later position.
    Advanced,
    /// The same position with the same contents, so nothing moved.
    ExactReplay,
    /// An earlier position, which is history rather than news.
    StaleCursorOnly,
    /// The same position with different contents, which nothing here settles.
    IntegrityConflict,
}

/// One position in one stream, with whatever it turned out to be about.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EventFact {
    /// The event-store incarnation that produced this cursor.
    pub agent_event_store_generation: u64,
    /// Which submission it is about, when it is about one this daemon holds.
    pub agent_operation_identifier: Option<String>,
    /// What was at this position, canonically.
    pub canonical_digest: String,
    /// The position itself.
    pub cursor: String,
    /// How many bytes it accounts for.
    pub event_bytes: u64,
    /// Where it sits in that submission's own sequence, when it is about one.
    pub job_sequence: Option<u64>,
}

/// The subscription ledgers one database holds.
#[derive(Debug)]
pub struct AgentSubscriptionLedger {
    /// What this namespace may hold.
    bounds: AgentCapacityBounds,
    /// The database the positions live in.
    database: OperationDatabase,
}

/// One consistent durable view of the subscription that recovery must cover.
/// Reading it does not freeze membership or authorize reset installation.
pub struct SubscriptionRecoveryView<'database> {
    owner: &'database OperationDatabase,
    target: String,
    subscription: String,
    ledger: SubscriptionLedgerRow,
    members: Vec<AgentSubmission>,
    local_owners: Vec<Option<crate::operation_repository::OperationSummary>>,
    physical_jobs: Vec<Vec<String>>,
}

/// Consistent retained evidence for replay after a local and remote terminal end.
/// Kept separate from unsettled recovery membership so reset cannot resurrect it.
pub struct CompletedEventView<'database> {
    owner: &'database OperationDatabase,
    target: String,
    subscription: String,
    ledger: SubscriptionLedgerRow,
    member: AgentSubmission,
    local: crate::operation_repository::OperationSummary,
    physical: Vec<String>,
}
impl core::fmt::Debug for CompletedEventView<'_> {
    fn fmt(&self, formatter: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        formatter.write_str("CompletedEventView([redacted])")
    }
}
impl CompletedEventView<'_> {
    /// Exact retained submission, including its terminal observation and contracts.
    pub fn member(&self) -> &AgentSubmission {
        &self.member
    }
}

/// One already wire-validated active snapshot staged for whole-subscription reset.
/// This storage input proves neither authentication nor terminal settlement.
#[derive(Clone)]
pub struct ActiveResetSnapshot {
    /// Member identifier; inputs use the exact ordered recovery membership.
    pub agent_operation_identifier: String,
    /// Monotonic nonterminal remote facts.
    pub observation: slingshot_domain::remote_job::RemoteJobObservation,
    /// Complete sorted, bounded physical-job set.
    pub physical_sling_job_identifiers: Vec<String>,
    /// Remaining lifetime after request/body and staging time have elapsed.
    pub remaining_retention_milliseconds: u64,
    /// Subscription position through which this snapshot includes the job's effects.
    pub subscription_watermark: String,
}

impl core::fmt::Debug for ActiveResetSnapshot {
    fn fmt(&self, formatter: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        formatter.write_str("ActiveResetSnapshot([redacted])")
    }
}

impl core::fmt::Debug for SubscriptionRecoveryView<'_> {
    fn fmt(&self, formatter: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        formatter.write_str("SubscriptionRecoveryView([redacted])")
    }
}

impl SubscriptionRecoveryView<'_> {
    /// Cursor, generation and incident observed in the same transaction as membership.
    pub fn ledger(&self) -> &SubscriptionLedgerRow {
        &self.ledger
    }
    /// Every unsettled child, including prior generations and selected revisions.
    /// Terminal remote observations with a nonterminal local owner remain members.
    pub fn members(&self) -> &[AgentSubmission] {
        &self.members
    }
    /// Physical identities captured atomically with this member and the ledger.
    pub fn physical_jobs_for(&self, agent_operation_identifier: &str) -> Option<&[String]> {
        self.members
            .iter()
            .position(|member| {
                member.identity.agent_operation_identifier == agent_operation_identifier
            })
            .map(|index| self.physical_jobs[index].as_slice())
    }
}

impl AgentSubscriptionLedger {
    /// Reads completed replay evidence without adding it to recovery membership.
    ///
    /// # Errors
    /// Refuses missing subscriptions or database read failures.
    pub fn read_completed_event(
        &self,
        target: &str,
        subscription: &str,
        operation: &str,
    ) -> Result<Option<CompletedEventView<'_>>, AgentRepositoryFailure> {
        let transaction = self.database.connection().unchecked_transaction()?;
        let result = self.read_completed_within(&transaction, target, subscription, operation)?;
        transaction.commit()?;
        Ok(result)
    }

    fn read_completed_within(
        &self,
        transaction: &rusqlite::Transaction<'_>,
        target: &str,
        subscription: &str,
        operation: &str,
    ) -> Result<Option<CompletedEventView<'_>>, AgentRepositoryFailure> {
        let ledger = read_subscription(transaction, target, subscription)?
            .ok_or(AgentRepositoryFailure::SubscriptionMoved)?;
        let Some(member) =
            crate::agent_job_repository::read_submission(transaction, target, operation)?
        else {
            return Ok(None);
        };
        if member.identity.daemon_subscription_identifier != subscription
            || !member.observation.state.is_terminal()
            || member.terminal_disposition.is_none()
        {
            return Ok(None);
        }
        let Some(local) = crate::operation_repository::OperationRepository::read_within(
            transaction,
            target,
            &member.identity.operation_identifier,
        )?
        else {
            return Ok(None);
        };
        if !local.record.lifecycle_state.is_terminal()
            || local.selected_environment_revision != member.identity.selected_environment_revision
        {
            return Ok(None);
        }
        let physical = crate::agent_job_repository::physical_jobs(transaction, target, operation)?;
        Ok(Some(CompletedEventView {
            owner: &self.database,
            target: target.into(),
            subscription: subscription.into(),
            ledger,
            member,
            local,
            physical,
        }))
    }

    /// Advances only the cursor for a validated replay of already completed work.
    /// The ledger, terminal child, local owner and physical identities are all
    /// compared under the write lock. Neither completion nor retention is changed.
    ///
    /// # Errors
    /// Refuses stale views, uncovered sequences/identities or invalid event facts.
    pub fn record_completed_event_cursor(
        &self,
        expected: &CompletedEventView<'_>,
        fact: &EventFact,
        physical: &str,
        now: u64,
    ) -> Result<LedgerOutcome, AgentRepositoryFailure> {
        require_cursor_bound(&fact.cursor)?;
        if !core::ptr::eq(expected.owner, &self.database)
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
            || now < expected.member.recorded_at_unix_milliseconds
            || now > i64::MAX as u64
            || fact.event_bytes > i64::MAX as u64
            || fact.canonical_digest.len() != 64
            || !fact
                .canonical_digest
                .bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
        {
            return Err(AgentRepositoryFailure::Conflicted);
        }
        let transaction = write_transaction(self.database.connection())?;
        let current = self
            .read_completed_within(
                &transaction,
                &expected.target,
                &expected.subscription,
                &expected.member.identity.agent_operation_identifier,
            )?
            .ok_or(AgentRepositoryFailure::SubscriptionMoved)?;
        if current.ledger != expected.ledger
            || current.member != expected.member
            || current.local != expected.local
            || current.physical != expected.physical
        {
            return Err(AgentRepositoryFailure::SubscriptionMoved);
        }
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

    /// Revalidates a captured recovery view inside the caller's write transaction.
    /// The caller must first bind that transaction to this ledger's database.
    pub(crate) fn require_recovery_current(
        &self,
        transaction: &rusqlite::Transaction<'_>,
        expected: &SubscriptionRecoveryView<'_>,
    ) -> Result<(), AgentRepositoryFailure> {
        if !core::ptr::eq(expected.owner, &self.database) {
            return Err(AgentRepositoryFailure::SubscriptionMoved);
        }
        let current =
            self.read_recovery_within(transaction, &expected.target, &expected.subscription)?;
        if current.ledger != expected.ledger
            || current.members != expected.members
            || current.local_owners != expected.local_owners
            || current.physical_jobs != expected.physical_jobs
        {
            return Err(AgentRepositoryFailure::SubscriptionMoved);
        }
        Ok(())
    }

    /// Reads every retained unsettled child and the subscription ledger from one
    /// database snapshot. Keyset pages have a fixed bound; the total is checked
    /// against the namespace capacity. No generation or revision filter may hide
    /// a child from recovery. Installation must revalidate the complete membership.
    pub fn read_recovery_view(
        &self,
        target: &str,
        subscription: &str,
    ) -> Result<SubscriptionRecoveryView<'_>, AgentRepositoryFailure> {
        let transaction = self.database.connection().unchecked_transaction()?;
        let view = self.read_recovery_within(&transaction, target, subscription)?;
        transaction.commit()?;
        Ok(view)
    }

    fn read_recovery_within(
        &self,
        connection: &rusqlite::Transaction<'_>,
        target: &str,
        subscription: &str,
    ) -> Result<SubscriptionRecoveryView<'_>, AgentRepositoryFailure> {
        let ledger = read_subscription(connection, target, subscription)?.ok_or_else(|| {
            AgentRepositoryFailure::NoSuchSubscription { identifier: subscription.to_owned() }
        })?;
        let mut members = Vec::new();
        let mut local_owners = Vec::new();
        let mut physical_jobs = Vec::new();
        let mut after: Option<String> = None;
        loop {
            let mut prepared = connection.prepare(statement(
                "page unsettled subscription members across retained generations",
            ))?;
            let page = prepared
                .query_map((target, subscription, &after, &after), |row| row.get::<_, String>(0))?
                .collect::<Result<Vec<_>, _>>()?;
            if page.is_empty() {
                break;
            }
            for identifier in &page {
                if members.len() as u64 >= self.bounds.agent_submission_rows {
                    return Err(AgentRepositoryFailure::Exhausted {
                        allowed: self.bounds.agent_submission_rows,
                        subject: "recovery subscription members",
                    });
                }
                let member =
                    crate::agent_job_repository::read_submission(connection, target, identifier)?
                        .ok_or(AgentRepositoryFailure::Conflicted)?;
                local_owners.push(crate::operation_repository::OperationRepository::read_within(
                    connection,
                    target,
                    &member.identity.operation_identifier,
                )?);
                physical_jobs.push(crate::agent_job_repository::physical_jobs(
                    connection, target, identifier,
                )?);
                members.push(member);
            }
            after = page.last().cloned();
        }
        Ok(SubscriptionRecoveryView {
            owner: &self.database,
            target: target.to_owned(),
            subscription: subscription.to_owned(),
            ledger,
            members,
            local_owners,
            physical_jobs,
        })
    }

    /// Atomically reconciles a same-generation subscription whose complete
    /// membership remains active. Every local owner, remote child, physical set
    /// and ledger fact is revalidated under the transaction before any write.
    /// Terminal or generation-loss cases require their separate settlement path.
    pub fn install_active_snapshot_reset(
        &self,
        expected: &SubscriptionRecoveryView<'_>,
        generation: u64,
        cursor: &str,
        snapshots: &[ActiveResetSnapshot],
        now_unix_milliseconds: u64,
    ) -> Result<(), AgentRepositoryFailure> {
        require_boundary(expected, &self.database, generation, cursor)?;
        if generation != expected.ledger.agent_event_store_generation
            || snapshots.len() != expected.members.len()
            || snapshots.is_empty()
            || now_unix_milliseconds > i64::MAX as u64
        {
            return Err(AgentRepositoryFailure::Conflicted);
        }
        let transaction = write_transaction(self.database.connection())?;
        let current =
            self.read_recovery_within(&transaction, &expected.target, &expected.subscription)?;
        if current.ledger != expected.ledger
            || current.members != expected.members
            || current.local_owners != expected.local_owners
            || current.physical_jobs != expected.physical_jobs
        {
            return Err(AgentRepositoryFailure::SubscriptionMoved);
        }
        Self::require_reconciled(
            &transaction,
            &expected.target,
            &expected.subscription,
            &current.ledger,
        )?;
        for (index, (member, snapshot)) in expected.members.iter().zip(snapshots).enumerate() {
            let local =
                expected.local_owners[index].as_ref().ok_or(AgentRepositoryFailure::Conflicted)?;
            require_cursor_bound(&snapshot.subscription_watermark)?;
            if member.identity.agent_event_store_generation != generation
                || member.identity.agent_operation_identifier != snapshot.agent_operation_identifier
                || snapshot.subscription_watermark.as_str() < cursor
                || snapshot.observation.state.is_terminal()
                || snapshot.observation.applied_sequence.value() > i64::MAX as u64
                || snapshot.observation.attempt > i64::MAX as u64
                || snapshot.observation.progress > i64::MAX as u64
                || now_unix_milliseconds < member.recorded_at_unix_milliseconds
                || expected.physical_jobs[index]
                    .iter()
                    .any(|job| !snapshot.physical_sling_job_identifiers.contains(job))
            {
                return Err(AgentRepositoryFailure::Conflicted);
            }
            crate::agent_job_repository::associate_snapshot_within(
                &transaction,
                self.bounds,
                member,
                &snapshot.physical_sling_job_identifiers,
                snapshot.remaining_retention_milliseconds,
                now_unix_milliseconds,
                Some(snapshot.observation),
                Some(local.record.revision),
            )?;
        }
        install_boundary_within(&transaction, expected, generation, cursor)?;
        transaction.commit()?;
        Ok(())
    }
    /// Installs authenticated capture evidence when no jobs need reconciliation.
    /// The caller validates the remote evidence; this transaction independently
    /// checks database ownership, unchanged ledger and still-empty membership.
    /// Captures are boundaries, not events: no event digest is fabricated.
    pub fn install_empty_recovery(
        &self,
        expected: &SubscriptionRecoveryView<'_>,
        generation: u64,
        cursor: &str,
    ) -> Result<(), AgentRepositoryFailure> {
        require_boundary(expected, &self.database, generation, cursor)?;
        if !expected.members.is_empty() {
            return Err(AgentRepositoryFailure::Conflicted);
        }
        let transaction = write_transaction(self.database.connection())?;
        let held = read_subscription(&transaction, &expected.target, &expected.subscription)?
            .ok_or(AgentRepositoryFailure::SubscriptionMoved)?;
        if held != expected.ledger {
            return Err(AgentRepositoryFailure::SubscriptionMoved);
        }
        Self::require_reconciled(&transaction, &expected.target, &expected.subscription, &held)?;
        let unsettled: bool = transaction.query_row(
            statement("detect unsettled members before a ledger-only reset"),
            (&expected.target, &expected.subscription),
            |row| row.get(0),
        )?;
        if unsettled {
            return Err(AgentRepositoryFailure::Conflicted);
        }
        install_boundary_within(&transaction, expected, generation, cursor)?;
        transaction.commit()?;
        Ok(())
    }
    /// Returns a ledger over `database`, bounded by the embedded policy.
    #[must_use]
    pub fn new(database: OperationDatabase) -> Self {
        Self::bounded(database, PersistentCapacityPolicy::embedded())
    }

    /// Returns a ledger over `database`, bounded by `policy`.
    #[must_use]
    pub fn bounded(database: OperationDatabase, policy: PersistentCapacityPolicy) -> Self {
        Self { bounds: AgentCapacityBounds::derived_from(policy), database }
    }

    /// Returns what this namespace may hold.
    #[must_use]
    pub fn bounds(&self) -> AgentCapacityBounds {
        self.bounds
    }

    /// Returns the database these positions live in.
    #[must_use]
    pub fn database(&self) -> &OperationDatabase {
        &self.database
    }

    /// Opens one subscription ledger.
    ///
    /// # Errors
    ///
    /// Returns [`AgentRepositoryFailure::Exhausted`] or
    /// [`AgentRepositoryFailure::Statement`].
    pub fn open_subscription(
        &self,
        author_target_identity_digest: &str,
        daemon_subscription_identifier: &str,
        agent_event_store_generation: u64,
        recorded_at_unix_milliseconds: u64,
    ) -> Result<(), AgentRepositoryFailure> {
        let connection = self.database.connection();
        let transaction = write_transaction(connection)?;
        let held = counted(transaction.query_row(
            statement("count the subscription ledgers one target holds"),
            (author_target_identity_digest,),
            |row| row.get::<_, i64>(0),
        )?);
        if held >= self.bounds.subscription_rows {
            return Err(AgentRepositoryFailure::Exhausted {
                allowed: self.bounds.subscription_rows,
                subject: "subscriptions",
            });
        }
        transaction.execute(
            statement("open one subscription ledger"),
            (
                stored(agent_event_store_generation),
                author_target_identity_digest,
                daemon_subscription_identifier,
                stored(recorded_at_unix_milliseconds),
            ),
        )?;
        transaction.commit()?;
        Ok(())
    }

    /// Returns where one subscription has got to.
    ///
    /// # Errors
    ///
    /// Returns [`AgentRepositoryFailure::Statement`].
    pub fn read_subscription(
        &self,
        author_target_identity_digest: &str,
        daemon_subscription_identifier: &str,
    ) -> Result<Option<SubscriptionLedgerRow>, AgentRepositoryFailure> {
        let transaction = self.database.connection().unchecked_transaction()?;
        let found = read_subscription(
            &transaction,
            author_target_identity_digest,
            daemon_subscription_identifier,
        )?;
        if let Some(held) = &found {
            Self::require_reconciled(
                &transaction,
                author_target_identity_digest,
                daemon_subscription_identifier,
                held,
            )?;
        }
        transaction.commit()?;
        Ok(found)
    }

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
            || fact.canonical_digest.len() != 64
            || !fact
                .canonical_digest
                .bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
        {
            return Err(AgentRepositoryFailure::Conflicted);
        }
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
        let local =
            expected.local_owners[index].as_ref().ok_or(AgentRepositoryFailure::Conflicted)?;
        if expected.ledger.unresolved_incident.is_some()
            || member.identity.agent_event_store_generation != fact.agent_event_store_generation
            || expected.ledger.agent_event_store_generation != fact.agent_event_store_generation
            || local.selected_environment_revision != member.identity.selected_environment_revision
            || local.record.lifecycle_state.is_terminal()
            || local.record.outstanding_recovery.as_ref().is_some_and(|recovery|
                recovery.evidence == slingshot_domain::operation::RecoveryExecutionEvidence::AuthoritativeRemoteSuccess)
            || member.terminal_disposition.is_some()
            || observation.state.is_terminal()
            || fact.job_sequence != Some(observation.applied_sequence.value())
            || !observation.applied_sequence.immediately_follows(member.observation.applied_sequence)
            || observation.applied_sequence <= member.snapshot_watermark
            || member.observation.advanced(observation.state, observation.applied_sequence,
                observation.attempt, observation.progress).is_err()
            || AgentJobIdentifier::new(sling_job_identifier).is_err()
            || now < member.recorded_at_unix_milliseconds
            || [now, observation.applied_sequence.value(), observation.attempt,
                observation.progress, fact.event_bytes].into_iter().any(|n| i64::try_from(n).is_err())
            || fact.canonical_digest.len() != 64
            || !fact.canonical_digest.bytes().all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
        {
            return Err(AgentRepositoryFailure::Conflicted);
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
    fn record_event_within(
        &self,
        transaction: &rusqlite::Transaction<'_>,
        author_target_identity_digest: &str,
        daemon_subscription_identifier: &str,
        fact: &EventFact,
        recorded_at_unix_milliseconds: u64,
    ) -> Result<LedgerOutcome, AgentRepositoryFailure> {
        let held = read_subscription(
            &transaction,
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
            &transaction,
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

    /// Installs a captured high-water position only for a subscription with no
    /// unsettled children. Nonempty recovery requires the all-job atomic path.
    ///
    /// The only way out of a conflict. A single job's snapshot cannot do it,
    /// because the disagreement is about the subscription's own position and
    /// one job knows nothing about the others sharing the stream.
    ///
    /// # Errors
    ///
    /// Returns [`AgentRepositoryFailure::NoSuchSubscription`].
    pub fn install_high_water(
        &self,
        author_target_identity_digest: &str,
        daemon_subscription_identifier: &str,
        expected_generation: u64,
        expected_incident: Option<&str>,
        expected_cursor: Option<&str>,
        new_generation: u64,
        captured_cursor: &str,
        canonical_digest: &str,
    ) -> Result<(), AgentRepositoryFailure> {
        let connection = self.database.connection();
        let transaction = write_transaction(connection)?;
        let unsettled: bool = transaction.query_row(
            statement("detect unsettled members before a ledger-only reset"),
            (author_target_identity_digest, daemon_subscription_identifier),
            |row| row.get(0),
        )?;
        if unsettled {
            return Err(AgentRepositoryFailure::Conflicted);
        }
        let changed = transaction.execute(
            statement("install a captured high-water position on a subscription"),
            (
                stored(new_generation),
                canonical_digest,
                captured_cursor,
                captured_cursor,
                author_target_identity_digest,
                daemon_subscription_identifier,
                stored(expected_generation),
                expected_cursor,
                expected_incident,
                stored(new_generation),
            ),
        )?;
        if changed != ONE_ROW {
            let exists = read_subscription(
                &transaction,
                author_target_identity_digest,
                daemon_subscription_identifier,
            )?
            .is_some();
            return Err(if exists {
                AgentRepositoryFailure::SubscriptionMoved
            } else {
                AgentRepositoryFailure::NoSuchSubscription {
                    identifier: daemon_subscription_identifier.to_owned(),
                }
            });
        }
        transaction.execute(
            statement("remove a subscription generation's retained events"),
            (
                author_target_identity_digest,
                daemon_subscription_identifier,
                stored(expected_generation),
            ),
        )?;
        transaction.commit()?;
        Ok(())
    }

    /// Removes retained events below one position and records the floor.
    ///
    /// Never above the position the ledger sits at, so compaction cannot
    /// discard the history a reconnection is about to resume from.
    ///
    /// # Errors
    ///
    /// Returns [`AgentRepositoryFailure::NoSuchSubscription`].
    pub fn compact_below(
        &self,
        author_target_identity_digest: &str,
        daemon_subscription_identifier: &str,
        floor_cursor: &str,
    ) -> Result<u64, AgentRepositoryFailure> {
        let connection = self.database.connection();
        let transaction = write_transaction(connection)?;
        let held = read_subscription(
            &transaction,
            author_target_identity_digest,
            daemon_subscription_identifier,
        )?
        .ok_or_else(|| AgentRepositoryFailure::NoSuchSubscription {
            identifier: daemon_subscription_identifier.to_owned(),
        })?;
        Self::require_reconciled(
            &transaction,
            author_target_identity_digest,
            daemon_subscription_identifier,
            &held,
        )?;
        let removed = transaction.execute(
            statement("compact one subscription's events below a position"),
            (
                author_target_identity_digest,
                daemon_subscription_identifier,
                stored(held.agent_event_store_generation),
                floor_cursor,
            ),
        )?;
        let (rows, bytes) = transaction.query_row(
            statement("measure one subscription's retained events"),
            (
                author_target_identity_digest,
                daemon_subscription_identifier,
                stored(held.agent_event_store_generation),
            ),
            |row| Ok((counted(row.get::<_, i64>(0)?), counted(row.get::<_, i64>(1)?))),
        )?;
        let changed = transaction.execute(
            statement("record one subscription's compaction floor"),
            (
                floor_cursor,
                stored(bytes),
                stored(rows),
                author_target_identity_digest,
                daemon_subscription_identifier,
                stored(held.agent_event_store_generation),
            ),
        )?;
        if changed != ONE_ROW {
            return Err(AgentRepositoryFailure::NoSuchSubscription {
                identifier: daemon_subscription_identifier.to_owned(),
            });
        }
        transaction.commit()?;
        Ok(u64::try_from(removed).unwrap_or(0))
    }

    /// Refuses counters that do not equal the current generation's event rows.
    fn require_reconciled(
        transaction: &rusqlite::Transaction<'_>,
        author_target_identity_digest: &str,
        daemon_subscription_identifier: &str,
        held: &SubscriptionLedgerRow,
    ) -> Result<(), AgentRepositoryFailure> {
        let (rows, bytes) = transaction.query_row(
            statement("measure one subscription's retained events"),
            (
                author_target_identity_digest,
                daemon_subscription_identifier,
                stored(held.agent_event_store_generation),
            ),
            |row| Ok((counted(row.get::<_, i64>(0)?), counted(row.get::<_, i64>(1)?))),
        )?;
        if rows == held.event_rows && bytes == held.event_bytes {
            Ok(())
        } else {
            Err(AgentRepositoryFailure::EventCounterDrift)
        }
    }

    /// Returns the subscriptions no retained submission still needs.
    ///
    /// # Errors
    ///
    /// Returns [`AgentRepositoryFailure::Statement`].
    pub fn orphaned_subscriptions(
        &self,
        author_target_identity_digest: &str,
        limit: u64,
    ) -> Result<Vec<String>, AgentRepositoryFailure> {
        let connection = self.database.connection();
        let mut prepared = connection
            .prepare(statement("select the subscriptions no retained agent submission needs"))?;
        let rows = prepared.query_map(
            (author_target_identity_digest, author_target_identity_digest, stored(limit)),
            |row| row.get(0),
        )?;
        Ok(rows.collect::<Result<Vec<String>, rusqlite::Error>>()?)
    }

    /// Retires one subscription nothing retained still needs.
    ///
    /// # Errors
    ///
    /// Returns [`AgentRepositoryFailure::NoSuchSubscription`] when a retained
    /// submission still names it, which is the check that keeps shared replay
    /// truth alive for whichever job still depends on it.
    pub fn retire_subscription(
        &self,
        author_target_identity_digest: &str,
        daemon_subscription_identifier: &str,
    ) -> Result<(), AgentRepositoryFailure> {
        let changed = self.database.connection().execute(
            statement("retire one subscription no retained agent submission needs"),
            (
                author_target_identity_digest,
                daemon_subscription_identifier,
                author_target_identity_digest,
            ),
        )?;
        if changed != ONE_ROW {
            return Err(AgentRepositoryFailure::NoSuchSubscription {
                identifier: daemon_subscription_identifier.to_owned(),
            });
        }
        Ok(())
    }
}

fn require_cursor_bound(cursor: &str) -> Result<(), AgentRepositoryFailure> {
    let limit =
        slingshot_domain::author_agent_transport_contract::AuthorAgentTransportContract::embedded()
            .limit("maximum_agent_operation_identifier_bytes");
    if cursor.is_empty()
        || cursor.len() as u64 > limit
        || cursor.bytes().any(|byte| byte < 0x20 || byte == 0x7f)
        || cursor.starts_with(' ')
        || cursor.ends_with(' ')
    {
        return Err(AgentRepositoryFailure::Conflicted);
    }
    Ok(())
}

fn require_boundary(
    expected: &SubscriptionRecoveryView<'_>,
    owner: &OperationDatabase,
    generation: u64,
    cursor: &str,
) -> Result<(), AgentRepositoryFailure> {
    require_cursor_bound(cursor)?;
    if !core::ptr::eq(expected.owner, owner)
        || generation == 0
        || generation > i64::MAX as u64
        || (generation == expected.ledger.agent_event_store_generation
            && expected.ledger.cursor.as_deref().is_some_and(|held| cursor < held))
    {
        return Err(AgentRepositoryFailure::Conflicted);
    }
    Ok(())
}

fn install_boundary_within(
    transaction: &rusqlite::Transaction<'_>,
    expected: &SubscriptionRecoveryView<'_>,
    generation: u64,
    cursor: &str,
) -> Result<(), AgentRepositoryFailure> {
    let held = &expected.ledger;
    let changed = transaction.execute(
        statement("install a reconciled subscription boundary without inventing an event digest"),
        (
            stored(generation),
            cursor,
            cursor,
            &expected.target,
            &expected.subscription,
            stored(held.agent_event_store_generation),
            &held.cursor,
            &held.unresolved_incident,
        ),
    )?;
    if changed != ONE_ROW {
        return Err(AgentRepositoryFailure::SubscriptionMoved);
    }
    transaction.execute(
        statement("remove a subscription generation's retained events"),
        (&expected.target, &expected.subscription, stored(held.agent_event_store_generation)),
    )?;

    Ok(())
}

/// Returns what folding `fact` into `held` would do.
fn classify(held: &SubscriptionLedgerRow, fact: &EventFact) -> LedgerOutcome {
    let Some(cursor) = &held.cursor else {
        return LedgerOutcome::Advanced;
    };
    if fact.cursor > *cursor {
        return LedgerOutcome::Advanced;
    }
    if fact.cursor < *cursor {
        return LedgerOutcome::StaleCursorOnly;
    }
    if held.canonical_digest.is_none() && held.high_water_cursor.as_ref() == Some(cursor) {
        return LedgerOutcome::StaleCursorOnly;
    }
    if held.canonical_digest.as_deref() == Some(fact.canonical_digest.as_str()) {
        LedgerOutcome::ExactReplay
    } else {
        LedgerOutcome::IntegrityConflict
    }
}

/// Returns where one subscription has got to.
fn read_subscription(
    connection: &rusqlite::Connection,
    author_target_identity_digest: &str,
    daemon_subscription_identifier: &str,
) -> Result<Option<SubscriptionLedgerRow>, AgentRepositoryFailure> {
    let mut prepared = connection.prepare(statement("read one subscription ledger"))?;
    Ok(prepared
        .query_row((author_target_identity_digest, daemon_subscription_identifier), |row| {
            Ok(SubscriptionLedgerRow {
                agent_event_store_generation: counted(
                    row.get::<_, i64>("agent_event_store_generation")?,
                ),
                canonical_digest: row.get("canonical_digest")?,
                compacted_below_cursor: row.get("compacted_below_cursor")?,
                cursor: row.get("cursor")?,
                event_bytes: counted(row.get::<_, i64>("event_bytes")?),
                event_rows: counted(row.get::<_, i64>("event_rows")?),
                high_water_cursor: row.get("high_water_cursor")?,
                unresolved_incident: row.get("unresolved_incident")?,
                unresolved_incident_count: counted(row.get::<_, i64>("unresolved_incident_count")?),
            })
        })
        .optional()?)
}
