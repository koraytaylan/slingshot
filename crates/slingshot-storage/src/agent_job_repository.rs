//! Durable facts about work the agent is running, and where its stream got to.
//!
//! Two subjects, deliberately not joined. One is the remote submission: what
//! was sent, under which contracts, to which target, and what has become of it.
//! The other is the subscription ledger: the position one filtered event stream
//! has reached. A stream carries events about work this daemon does not hold,
//! so its position has to be recordable without a job to hang it on, and a
//! foreign key from the ledger to a job would make the honest case impossible
//! to write down.
//!
//! # Everything a resubmission needs is here
//!
//! The contract columns are not bookkeeping. A restart re-derives the identity
//! from what the build has and compares it against what is stored; a build
//! whose schemas, limits, version, wire name, byte contract, or transport
//! contract has moved must not resume somebody else's submission under its own
//! name, and the only way to notice is to have written down what the original
//! was made under.
//!
//! # Idempotency is a property of rows
//!
//! Every write here is conditional in SQL rather than guarded by something the
//! caller remembers. A cursor advances only to a strictly later position, a
//! fold applies only to the sequence it expected to find, a settlement lands
//! only on a row that has not ended, and a physical job records the same name
//! twice without complaint. So a replay is a no-op because the statement says
//! so, and a conflict changes nothing for the same reason.
//!
//! # Capacity is derived, not invented
//!
//! Every bound below comes from Plan 0004's retained-operation maximum through
//! checked arithmetic, so a deployment that sized its ledger once has sized
//! this too. A count that would overflow saturates into a refusal rather than
//! wrapping into a permission.

use rusqlite::OptionalExtension as _;
use slingshot_domain::persistent_capacity::PersistentCapacityPolicy;
use slingshot_domain::remote_job::{AgentJobState, JobEventSequence, RemoteJobObservation};

use crate::database::OperationDatabase;
use crate::operation_repository::RepositoryFailure;
use crate::sqlite_statement_inventory::statement_text;

/// Physical Sling jobs one logical submission may be carried by.
pub const PHYSICAL_JOBS_PER_SUBMISSION: u64 = 32;

/// Events one subscription may retain per submission it could be carrying.
pub const EVENTS_PER_SUBMISSION: u64 = 256;

/// Bytes one retained event may account for.
pub const BYTES_PER_EVENT: u64 = 1_048_576;

/// Unresolved integrity incidents one subscription may hold at a time.
pub const INCIDENT_SLOTS_PER_SUBSCRIPTION: u64 = 1;

/// Rows one statement changes when it changes the row it named.
const ONE_ROW: usize = 1;

/// Returns the text of the inventoried statement with `purpose`.
fn statement(purpose: &str) -> &'static str {
    statement_text(purpose)
}

/// Begins a transaction that will write.
///
/// `IMMEDIATE`, because a deferred transaction starts as a reader and asks for
/// the write lock when it first writes. Two that both read and then both try to
/// upgrade cannot both be granted, and SQLite refuses at once rather than
/// waiting, because each already holds the read lock the other needs.
fn write_transaction(
    connection: &rusqlite::Connection,
) -> Result<rusqlite::Transaction<'_>, AgentRepositoryFailure> {
    Ok(rusqlite::Transaction::new_unchecked(connection, rusqlite::TransactionBehavior::Immediate)?)
}

/// What this namespace may hold, derived from the operation ledger's own bound.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AgentCapacityBounds {
    /// Remote submissions one target may hold.
    pub agent_submission_rows: u64,
    /// Bytes one subscription's retained events may come to.
    pub event_bytes: u64,
    /// Events one subscription may retain.
    pub event_rows: u64,
    /// Unresolved incidents one subscription may hold.
    pub incident_slots: u64,
    /// Physical Sling jobs one submission may be carried by.
    pub physical_job_rows: u64,
    /// Subscriptions one target may hold.
    pub subscription_rows: u64,
}

impl AgentCapacityBounds {
    /// Returns the bounds `policy`'s retained-operation maximum implies.
    ///
    /// One submission per retained operation, plus the reserve the operation
    /// ledger already keeps for work in flight. Saturating rather than
    /// wrapping: a bound that overflowed into a small number would refuse
    /// everything, and one that wrapped into a large number would refuse
    /// nothing, and only the first is safe to get wrong.
    #[must_use]
    pub fn derived_from(policy: PersistentCapacityPolicy) -> Self {
        let retained = policy.retained_operation_rows;
        Self {
            agent_submission_rows: retained,
            event_bytes: EVENTS_PER_SUBMISSION.saturating_mul(BYTES_PER_EVENT),
            event_rows: EVENTS_PER_SUBMISSION,
            incident_slots: INCIDENT_SLOTS_PER_SUBSCRIPTION,
            physical_job_rows: PHYSICAL_JOBS_PER_SUBMISSION,
            subscription_rows: retained,
        }
    }
}

/// Why one repository call could not do what it was asked.
#[derive(Debug, thiserror::Error)]
pub enum AgentRepositoryFailure {
    /// The database refused one statement.
    #[error("the database refused a statement: {0}")]
    Statement(#[from] rusqlite::Error),
    /// Something in the operation ledger refused.
    #[error(transparent)]
    Ledger(#[from] RepositoryFailure),
    /// A stored value could not be read back as the domain value it is.
    #[error("a stored {column} does not decode")]
    NotDecodable {
        /// Which column held it.
        column: &'static str,
    },
    /// The submission the call named is not in that partition.
    #[error("no agent submission named {identifier} in that target partition")]
    NoSuchSubmission {
        /// What the caller named.
        identifier: String,
    },
    /// The subscription the call named is not in that partition.
    #[error("no subscription named {identifier} in that target partition")]
    NoSuchSubscription {
        /// What the caller named.
        identifier: String,
    },
    /// Something already holds that name and is not the same work.
    #[error("that name already names different work in this partition")]
    Conflicted,
    /// One of the derived bounds is already reached.
    #[error("this namespace already holds the {allowed} {subject} it may")]
    Exhausted {
        /// How many it may hold.
        allowed: u64,
        /// What was being counted.
        subject: &'static str,
    },
}

/// Which contracts one submission was made under.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SubmissionContracts {
    /// Digest of the argument schema.
    pub argument_schema_digest: String,
    /// Digest of the transport contract.
    pub author_agent_transport_contract_digest: String,
    /// Digest of the canonical byte contract.
    pub command_canonical_json_contract_digest: String,
    /// Digest of the limits manifest.
    pub command_contract_limits_digest: String,
    /// The semantic version the contract is at.
    pub command_semantic_contract_version: String,
    /// The name the command answers to.
    pub command_wire_name: String,
    /// Digest of the result schema.
    pub result_schema_digest: String,
    /// The digest binding all of it to these arguments.
    pub submitted_command_digest: String,
}

/// Which work one submission is, and where it lives.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SubmissionIdentity {
    /// Which incarnation of the agent's store it belongs to.
    pub agent_event_store_generation: u64,
    /// What it is called at the agent.
    pub agent_operation_identifier: String,
    /// The partition it belongs to.
    pub author_target_identity_digest: String,
    /// Which subscription carries its events.
    pub daemon_subscription_identifier: String,
    /// What it is called locally.
    pub operation_identifier: String,
    /// The environment revision it was submitted under.
    pub selected_environment_revision: String,
}

/// One remote submission, as it is stored.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgentSubmission {
    /// The exact bytes that were sent.
    pub canonical_submission: String,
    /// Which contracts it was made under.
    pub contracts: SubmissionContracts,
    /// Which work it is.
    pub identity: SubmissionIdentity,
    /// What is known about the job running it.
    pub observation: RemoteJobObservation,
    /// When it was written down.
    pub recorded_at_unix_milliseconds: u64,
    /// How long the agent's results survive, counted from request start.
    pub remaining_retention_milliseconds: u64,
    /// When the request that made it started.
    pub request_start_unix_milliseconds: u64,
    /// The sequence a snapshot has already accounted for.
    pub snapshot_watermark: JobEventSequence,
    /// How it ended, when it has.
    pub terminal_disposition: Option<String>,
}

/// What admitting one submission did.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SubmissionOutcome {
    /// It was not there, and now it is.
    Admitted,
    /// It was already there, made under exactly the same everything.
    ExactReplay,
}

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

/// The agent-job facts one database holds.
#[derive(Debug)]
pub struct AgentJobRepository {
    /// What this namespace may hold.
    bounds: AgentCapacityBounds,
    /// The database the facts live in.
    database: OperationDatabase,
}

impl AgentJobRepository {
    /// Returns a repository over `database`, bounded by the embedded policy.
    #[must_use]
    pub fn new(database: OperationDatabase) -> Self {
        Self::bounded(database, PersistentCapacityPolicy::embedded())
    }

    /// Returns a repository over `database`, bounded by `policy`.
    #[must_use]
    pub fn bounded(database: OperationDatabase, policy: PersistentCapacityPolicy) -> Self {
        Self { bounds: AgentCapacityBounds::derived_from(policy), database }
    }

    /// Returns what this namespace may hold.
    #[must_use]
    pub fn bounds(&self) -> AgentCapacityBounds {
        self.bounds
    }

    /// Returns the database these facts live in.
    #[must_use]
    pub fn database(&self) -> &OperationDatabase {
        &self.database
    }

    /// Writes one submission down, or says it is already written down.
    ///
    /// A repeat is the same work only when every stored value agrees. Anything
    /// else wearing that name is a conflict and changes nothing, because a
    /// submission is named by a digest over the contracts and the arguments and
    /// two different submissions cannot honestly share one.
    ///
    /// # Errors
    ///
    /// Returns [`AgentRepositoryFailure::Conflicted`] or
    /// [`AgentRepositoryFailure::Exhausted`].
    pub fn submit(
        &self,
        submission: &AgentSubmission,
    ) -> Result<SubmissionOutcome, AgentRepositoryFailure> {
        let connection = self.database.connection();
        let transaction = write_transaction(connection)?;
        let identity = &submission.identity;
        if let Some(stored) = read_submission(
            &transaction,
            &identity.author_target_identity_digest,
            &identity.agent_operation_identifier,
        )? {
            return if &stored == submission {
                Ok(SubmissionOutcome::ExactReplay)
            } else {
                Err(AgentRepositoryFailure::Conflicted)
            };
        }
        let held: u64 = transaction.query_row(
            statement("count the agent submissions one target holds"),
            (&identity.author_target_identity_digest,),
            |row| row.get(0),
        )?;
        if held >= self.bounds.agent_submission_rows {
            return Err(AgentRepositoryFailure::Exhausted {
                allowed: self.bounds.agent_submission_rows,
                subject: "agent submissions",
            });
        }
        insert_submission(&transaction, submission)?;
        transaction.commit()?;
        Ok(SubmissionOutcome::Admitted)
    }

    /// Returns the submission that partition holds under that name.
    ///
    /// # Errors
    ///
    /// Returns [`AgentRepositoryFailure::Statement`] or
    /// [`AgentRepositoryFailure::NotDecodable`].
    pub fn read(
        &self,
        author_target_identity_digest: &str,
        agent_operation_identifier: &str,
    ) -> Result<Option<AgentSubmission>, AgentRepositoryFailure> {
        read_submission(
            self.database.connection(),
            author_target_identity_digest,
            agent_operation_identifier,
        )
    }

    /// Records one more physical Sling job carrying one submission.
    ///
    /// Recording the same name twice changes nothing, which is what
    /// at-least-once delivery looks like when it is handled rather than merely
    /// survived. What is bounded is how many distinct records one logical
    /// submission may accumulate, because an unbounded requeue loop would
    /// otherwise grow this table without limit.
    ///
    /// # Errors
    ///
    /// Returns [`AgentRepositoryFailure::NoSuchSubmission`] or
    /// [`AgentRepositoryFailure::Exhausted`].
    pub fn record_physical_job(
        &self,
        identity: &SubmissionIdentity,
        sling_job_identifier: &str,
        recorded_at_unix_milliseconds: u64,
    ) -> Result<(), AgentRepositoryFailure> {
        let connection = self.database.connection();
        let transaction = write_transaction(connection)?;
        let held = physical_jobs(
            &transaction,
            &identity.author_target_identity_digest,
            &identity.agent_operation_identifier,
        )?;
        if held.iter().any(|name| name == sling_job_identifier) {
            return Ok(());
        }
        if u64::try_from(held.len()).unwrap_or(u64::MAX) >= self.bounds.physical_job_rows {
            return Err(AgentRepositoryFailure::Exhausted {
                allowed: self.bounds.physical_job_rows,
                subject: "physical Sling jobs",
            });
        }
        let changed = transaction.execute(
            statement("record one physical Sling job for one agent submission"),
            (
                &identity.agent_operation_identifier,
                &identity.author_target_identity_digest,
                recorded_at_unix_milliseconds,
                sling_job_identifier,
            ),
        )?;
        if changed != ONE_ROW {
            return Err(AgentRepositoryFailure::NoSuchSubmission {
                identifier: identity.agent_operation_identifier.clone(),
            });
        }
        transaction.commit()?;
        Ok(())
    }

    /// Returns the physical Sling jobs one submission is carried by, sorted.
    ///
    /// # Errors
    ///
    /// Returns [`AgentRepositoryFailure::Statement`].
    pub fn physical_jobs(
        &self,
        author_target_identity_digest: &str,
        agent_operation_identifier: &str,
    ) -> Result<Vec<String>, AgentRepositoryFailure> {
        physical_jobs(
            self.database.connection(),
            author_target_identity_digest,
            agent_operation_identifier,
        )
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
        let held: u64 = transaction.query_row(
            statement("count the subscription ledgers one target holds"),
            (author_target_identity_digest,),
            |row| row.get(0),
        )?;
        if held >= self.bounds.subscription_rows {
            return Err(AgentRepositoryFailure::Exhausted {
                allowed: self.bounds.subscription_rows,
                subject: "subscriptions",
            });
        }
        transaction.execute(
            statement("open one subscription ledger"),
            (
                agent_event_store_generation,
                author_target_identity_digest,
                daemon_subscription_identifier,
                recorded_at_unix_milliseconds,
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
                    fact.event_bytes,
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
                    fact.event_bytes,
                    fact.job_sequence,
                    recorded_at_unix_milliseconds,
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

    /// Applies one believed event to one submission's durable state.
    ///
    /// Conditional on the sequence the caller expected to find, so two folds
    /// racing on one row cannot both succeed and neither can apply an event to
    /// a row that has already moved past it.
    ///
    /// # Errors
    ///
    /// Returns [`AgentRepositoryFailure::NoSuchSubmission`] when the row has
    /// moved on, ended, or is not there.
    pub fn fold_event(
        &self,
        identity: &SubmissionIdentity,
        expected_sequence: JobEventSequence,
        observation: RemoteJobObservation,
    ) -> Result<(), AgentRepositoryFailure> {
        let changed = self.database.connection().execute(
            statement("fold one believed event into one agent submission"),
            (
                observation.applied_sequence.value(),
                observation.attempt,
                observation.state.to_string(),
                observation.progress,
                &identity.author_target_identity_digest,
                &identity.agent_operation_identifier,
                expected_sequence.value(),
            ),
        )?;
        require_one_row(changed, &identity.agent_operation_identifier)
    }

    /// Raises one submission's snapshot watermark, and never lowers it.
    ///
    /// # Errors
    ///
    /// Returns [`AgentRepositoryFailure::NoSuchSubmission`] when the stored
    /// watermark already covers more than this one.
    pub fn record_snapshot_watermark(
        &self,
        identity: &SubmissionIdentity,
        watermark: JobEventSequence,
    ) -> Result<(), AgentRepositoryFailure> {
        let changed = self.database.connection().execute(
            statement("record one snapshot watermark on one agent submission"),
            (
                watermark.value(),
                &identity.author_target_identity_digest,
                &identity.agent_operation_identifier,
                watermark.value(),
            ),
        )?;
        require_one_row(changed, &identity.agent_operation_identifier)
    }

    /// Ends one submission, once.
    ///
    /// # Errors
    ///
    /// Returns [`AgentRepositoryFailure::NoSuchSubmission`] when the row has
    /// already ended, which is what makes an ending immutable in the store as
    /// well as in the domain.
    pub fn settle(
        &self,
        identity: &SubmissionIdentity,
        observation: RemoteJobObservation,
        remaining_retention_milliseconds: u64,
        terminal_disposition: &str,
    ) -> Result<(), AgentRepositoryFailure> {
        let changed = self.database.connection().execute(
            statement("settle one agent submission"),
            (
                observation.applied_sequence.value(),
                observation.attempt,
                observation.state.to_string(),
                observation.progress,
                remaining_retention_milliseconds,
                terminal_disposition,
                &identity.author_target_identity_digest,
                &identity.agent_operation_identifier,
            ),
        )?;
        require_one_row(changed, &identity.agent_operation_identifier)
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
                agent_event_store_generation,
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
        let (rows, bytes): (u64, u64) = transaction.query_row(
            statement("measure one subscription's retained events"),
            (author_target_identity_digest, daemon_subscription_identifier),
            |row| Ok((row.get(0)?, row.get(1)?)),
        )?;
        let changed = transaction.execute(
            statement("record one subscription's compaction floor"),
            (
                floor_cursor,
                bytes,
                rows,
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

    /// Returns the ended submissions one maintenance run would remove.
    ///
    /// # Errors
    ///
    /// Returns [`AgentRepositoryFailure::Statement`].
    pub fn removable_submissions(
        &self,
        author_target_identity_digest: &str,
        before_unix_milliseconds: u64,
        limit: u64,
    ) -> Result<Vec<(String, String)>, AgentRepositoryFailure> {
        let connection = self.database.connection();
        let mut prepared = connection
            .prepare(statement("select the agent submissions one maintenance run would remove"))?;
        let rows = prepared.query_map(
            (author_target_identity_digest, before_unix_milliseconds, limit),
            |row| {
                Ok((row.get("agent_operation_identifier")?, row.get("submitted_command_digest")?))
            },
        )?;
        Ok(rows.collect::<Result<Vec<(String, String)>, rusqlite::Error>>()?)
    }

    /// Removes one ended submission and everything that hangs off it.
    ///
    /// # Errors
    ///
    /// Returns [`AgentRepositoryFailure::NoSuchSubmission`] when the row is not
    /// there or has not ended, which is how nonterminal work survives a run
    /// that thought it had selected it.
    pub fn remove_ended(
        &self,
        author_target_identity_digest: &str,
        agent_operation_identifier: &str,
    ) -> Result<(), AgentRepositoryFailure> {
        let changed = self.database.connection().execute(
            statement("remove one ended agent submission"),
            (author_target_identity_digest, agent_operation_identifier),
        )?;
        require_one_row(changed, agent_operation_identifier)
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
            (author_target_identity_digest, author_target_identity_digest, limit),
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

/// Requires one statement to have changed the row it named.
fn require_one_row(changed: usize, identifier: &str) -> Result<(), AgentRepositoryFailure> {
    if changed == ONE_ROW {
        Ok(())
    } else {
        Err(AgentRepositoryFailure::NoSuchSubmission { identifier: identifier.to_owned() })
    }
}

/// Returns the state `spelling` names.
fn state_named(spelling: &str) -> Result<AgentJobState, AgentRepositoryFailure> {
    match spelling {
        "queued" => Ok(AgentJobState::Queued),
        "running" => Ok(AgentJobState::Running),
        "succeeded" => Ok(AgentJobState::Succeeded),
        "failed" => Ok(AgentJobState::Failed),
        _ => Err(AgentRepositoryFailure::NotDecodable { column: "job_state" }),
    }
}

/// Writes one submission's row.
fn insert_submission(
    connection: &rusqlite::Connection,
    submission: &AgentSubmission,
) -> Result<(), AgentRepositoryFailure> {
    let contracts = &submission.contracts;
    let identity = &submission.identity;
    let observation = &submission.observation;
    connection.execute(
        statement("admit one agent submission"),
        rusqlite::params![
            identity.agent_event_store_generation,
            identity.agent_operation_identifier,
            observation.applied_sequence.value(),
            contracts.argument_schema_digest,
            observation.attempt,
            contracts.author_agent_transport_contract_digest,
            identity.author_target_identity_digest,
            submission.canonical_submission,
            contracts.command_canonical_json_contract_digest,
            contracts.command_contract_limits_digest,
            contracts.command_semantic_contract_version,
            contracts.command_wire_name,
            identity.daemon_subscription_identifier,
            observation.state.to_string(),
            identity.operation_identifier,
            observation.progress,
            submission.recorded_at_unix_milliseconds,
            submission.remaining_retention_milliseconds,
            submission.request_start_unix_milliseconds,
            contracts.result_schema_digest,
            identity.selected_environment_revision,
            submission.snapshot_watermark.value(),
            contracts.submitted_command_digest,
        ],
    )?;
    Ok(())
}

/// Returns the submission one partition holds under that name.
fn read_submission(
    connection: &rusqlite::Connection,
    author_target_identity_digest: &str,
    agent_operation_identifier: &str,
) -> Result<Option<AgentSubmission>, AgentRepositoryFailure> {
    let mut prepared =
        connection.prepare(statement("read one agent submission inside its target partition"))?;
    let held = prepared
        .query_row((author_target_identity_digest, agent_operation_identifier), |row| {
            Ok((
                row.get::<_, String>("job_state")?,
                AgentSubmission {
                    canonical_submission: row.get("canonical_submission")?,
                    contracts: SubmissionContracts {
                        argument_schema_digest: row.get("argument_schema_digest")?,
                        author_agent_transport_contract_digest: row
                            .get("author_agent_transport_contract_digest")?,
                        command_canonical_json_contract_digest: row
                            .get("command_canonical_json_contract_digest")?,
                        command_contract_limits_digest: row
                            .get("command_contract_limits_digest")?,
                        command_semantic_contract_version: row
                            .get("command_semantic_contract_version")?,
                        command_wire_name: row.get("command_wire_name")?,
                        result_schema_digest: row.get("result_schema_digest")?,
                        submitted_command_digest: row.get("submitted_command_digest")?,
                    },
                    identity: SubmissionIdentity {
                        agent_event_store_generation: row.get("agent_event_store_generation")?,
                        agent_operation_identifier: agent_operation_identifier.to_owned(),
                        author_target_identity_digest: author_target_identity_digest.to_owned(),
                        daemon_subscription_identifier: row
                            .get("daemon_subscription_identifier")?,
                        operation_identifier: row.get("operation_identifier")?,
                        selected_environment_revision: row.get("selected_environment_revision")?,
                    },
                    observation: RemoteJobObservation {
                        applied_sequence: JobEventSequence::of(row.get("applied_sequence")?),
                        attempt: row.get("attempt")?,
                        progress: row.get("progress")?,
                        state: AgentJobState::Queued,
                    },
                    recorded_at_unix_milliseconds: row.get("recorded_at_unix_milliseconds")?,
                    remaining_retention_milliseconds: row
                        .get("remaining_retention_milliseconds")?,
                    request_start_unix_milliseconds: row.get("request_start_unix_milliseconds")?,
                    snapshot_watermark: JobEventSequence::of(row.get("snapshot_watermark")?),
                    terminal_disposition: row.get("terminal_disposition")?,
                },
            ))
        })
        .optional()?;
    let Some((spelling, mut submission)) = held else {
        return Ok(None);
    };
    submission.observation.state = state_named(&spelling)?;
    Ok(Some(submission))
}

/// Returns the physical Sling jobs one submission is carried by.
fn physical_jobs(
    connection: &rusqlite::Connection,
    author_target_identity_digest: &str,
    agent_operation_identifier: &str,
) -> Result<Vec<String>, AgentRepositoryFailure> {
    let mut prepared = connection
        .prepare(statement("read one agent submission's physical Sling jobs in order"))?;
    let rows = prepared
        .query_map((author_target_identity_digest, agent_operation_identifier), |row| row.get(0))?;
    Ok(rows.collect::<Result<Vec<String>, rusqlite::Error>>()?)
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
                agent_event_store_generation: row.get("agent_event_store_generation")?,
                canonical_digest: row.get("canonical_digest")?,
                compacted_below_cursor: row.get("compacted_below_cursor")?,
                cursor: row.get("cursor")?,
                event_bytes: row.get("event_bytes")?,
                event_rows: row.get("event_rows")?,
                high_water_cursor: row.get("high_water_cursor")?,
                unresolved_incident: row.get("unresolved_incident")?,
                unresolved_incident_count: row.get("unresolved_incident_count")?,
            })
        })
        .optional()?)
}
