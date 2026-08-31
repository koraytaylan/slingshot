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
    AgentCapacityBounds, AgentRepositoryFailure, ONE_ROW, counted, statement, stored,
    write_transaction,
};
use crate::database::OperationDatabase;

/// Where one subscription has got to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SubscriptionLedgerRow {
    /// Which incarnation of the agent's store it is following.
    pub agent_event_store_generation: u64,
    /// What was at the position it sits at.
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

impl AgentSubscriptionLedger {
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
        read_subscription(
            self.database.connection(),
            author_target_identity_digest,
            daemon_subscription_identifier,
        )
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
        let held = read_subscription(
            &transaction,
            author_target_identity_digest,
            daemon_subscription_identifier,
        )?
        .ok_or_else(|| AgentRepositoryFailure::NoSuchSubscription {
            identifier: daemon_subscription_identifier.to_owned(),
        })?;
        let outcome = classify(&held, fact);
        if matches!(outcome, LedgerOutcome::Advanced) {
            self.require_event_room(&held)?;
            transaction.execute(
                statement("advance one subscription ledger to a later position"),
                (
                    &fact.canonical_digest,
                    &fact.cursor,
                    stored(fact.event_bytes),
                    author_target_identity_digest,
                    daemon_subscription_identifier,
                    &fact.cursor,
                ),
            )?;
            transaction.execute(
                statement("record one subscription event"),
                (
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
        transaction.commit()?;
        Ok(outcome)
    }

    /// Requires room for one more event before one is written.
    fn require_event_room(
        &self,
        held: &SubscriptionLedgerRow,
    ) -> Result<(), AgentRepositoryFailure> {
        if held.event_rows >= self.bounds.event_rows {
            return Err(AgentRepositoryFailure::Exhausted {
                allowed: self.bounds.event_rows,
                subject: "retained events",
            });
        }
        if held.event_bytes >= self.bounds.event_bytes {
            return Err(AgentRepositoryFailure::Exhausted {
                allowed: self.bounds.event_bytes,
                subject: "retained event bytes",
            });
        }
        Ok(())
    }

    /// Installs a captured high-water position, clearing the incident it heals.
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
        agent_event_store_generation: u64,
        captured_cursor: &str,
        canonical_digest: &str,
    ) -> Result<(), AgentRepositoryFailure> {
        let changed = self.database.connection().execute(
            statement("install a captured high-water position on a subscription"),
            (
                stored(agent_event_store_generation),
                canonical_digest,
                captured_cursor,
                captured_cursor,
                author_target_identity_digest,
                daemon_subscription_identifier,
            ),
        )?;
        if changed != ONE_ROW {
            return Err(AgentRepositoryFailure::NoSuchSubscription {
                identifier: daemon_subscription_identifier.to_owned(),
            });
        }
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
        let removed = transaction.execute(
            statement("compact one subscription's events below a position"),
            (author_target_identity_digest, daemon_subscription_identifier, floor_cursor),
        )?;
        let (rows, bytes) = transaction.query_row(
            statement("measure one subscription's retained events"),
            (author_target_identity_digest, daemon_subscription_identifier),
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
