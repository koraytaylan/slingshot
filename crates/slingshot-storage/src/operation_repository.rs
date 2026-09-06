//! Admission, transition, lookup, and recovery, with every decision durable.
//!
//! Idempotency here is a property of committed rows rather than of anything a
//! caller remembers. An operation is named by its target partition and its
//! identifier together, and a repeat is the same work only when the selected
//! environment revision and the fingerprint also match; anything else wearing
//! that name is a conflict, and a conflict changes nothing. The partition is
//! the opaque author-target digest, so one identifier against two targets is
//! two operations - including two that differ only by the principal behind one
//! deployment - and replay never crosses one.
//!
//! Every write is a compare-and-set folded through [`OperationRecord`], so a
//! transition's legality is decided once, in the domain.

use rusqlite::OptionalExtension as _;
use serde::Serialize;
use serde::de::DeserializeOwned;
use slingshot_domain::command_fingerprint::{
    CommandFingerprint, RepeatedIdentifier, classify_repeat,
};
use slingshot_domain::daemon_runtime_contract::DaemonRuntimeContract;
use slingshot_domain::installation::InstallationIdentifier;
use slingshot_domain::operation::{
    LifecycleFailure, OperationFact, OperationLifecycleState, OperationRecord, ProducedArtifact,
    RecoveryCategory, RecoveryExecutionEvidence, RecoveryFact, SettlementFailure,
    SuccessfulSettlement, TerminalFailure, TerminalFailureDisposition, TerminalFailureKind,
};
pub use slingshot_domain::operation::{RecoveryResumeReceipt, ResultDisposition};

use slingshot_domain::persistent_capacity::{CapacityFacts, PersistentCapacityPolicy};

use crate::database::{DatabaseFailure, OperationDatabase};
use crate::persistent_capacity::{AccountingFailure, PersistentCapacityAccount};
use crate::sqlite_statement_inventory::STATEMENTS;

/// The recovery evidence column value for unproved execution.
const EXECUTION_CERTAINTY_KIND: &str = "execution_certainty";

/// The recovery evidence column value for a proven remote success.
const AUTHORITATIVE_REMOTE_SUCCESS_KIND: &str = "authoritative_remote_success";

/// Rows one statement changes when it changes the row it named.
const ONE_ROW: usize = 1;
/// Returns the text of the inventoried statement with `purpose`.
///
/// Looked up rather than written here, so a statement outside the inventory
/// cannot be reached from this module.
fn statement(purpose: &str) -> &'static str {
    STATEMENTS
        .iter()
        .find(|inventoried| inventoried.purpose == purpose)
        .map(|inventoried| inventoried.text)
        .unwrap_or_else(|| panic!("the inventory holds a statement for {purpose}"))
}

/// Reason a repository call could not do what it was asked.
#[derive(Debug, thiserror::Error)]
pub enum RepositoryFailure {
    /// A new row would exceed the scheduler's waiting-work bound.
    #[error(
        "the pending operation capacity is exhausted ({held} of {limit}, per caller: {per_caller})"
    )]
    PendingCapacity {
        /// Whether this is the requesting caller's bound rather than the target's.
        per_caller: bool,
        /// Waiting rows observed inside the admission transaction.
        held: u64,
        /// The applicable limit supplied by the owning runtime contract.
        limit: u64,
    },
    /// Remote evidence changed or disappeared before local mutation.
    #[error("the retained remote observation changed")]
    RemoteObservationMoved,
    /// The database itself refused.
    #[error(transparent)]
    Database(#[from] DatabaseFailure),
    /// The database refused one statement.
    #[error("the database refused a statement: {0}")]
    Statement(#[from] rusqlite::Error),
    /// A stored value could not be read back as the domain value it is.
    #[error("a stored {column} does not decode: {detail}")]
    NotDecodable {
        /// Column the value came from.
        column: &'static str,
        /// What the decoder said.
        detail: String,
    },
    /// The operation the call named is not in that partition.
    #[error("no operation named {identifier} in that target partition")]
    NoSuchOperation {
        /// Identifier the caller asked about.
        identifier: String,
    },
    /// The caller's expected revision is not the stored one.
    #[error("the operation moved on: expected revision {expected}, stored {stored}")]
    RevisionMoved {
        /// Revision the caller last saw.
        expected: u64,
        /// Revision the row holds.
        stored: u64,
    },
    /// The lifecycle the settlement observed has since changed.
    #[error("the operation lifecycle moved: expected {expected:?}, stored {stored:?}")]
    LifecycleMoved {
        /// Lifecycle the settlement was formed against.
        expected: OperationLifecycleState,
        /// Lifecycle observed inside its transaction.
        stored: OperationLifecycleState,
    },
    /// The fact does not belong to the operation as it stands.
    #[error(transparent)]
    Lifecycle(#[from] LifecycleFailure),
    /// The complete result does not have one valid representation.
    #[error(transparent)]
    Settlement(#[from] SettlementFailure),
    /// A bounded text arrived longer than its bound.
    #[error("{field} holds {actual} bytes, and the contract allows {allowed}")]
    TooLong {
        /// Which text.
        field: &'static str,
        /// How long it was.
        actual: usize,
        /// How long it may be.
        allowed: u64,
    },
    /// One operation already holds every resume receipt it may.
    #[error("this operation already holds the {allowed} resume receipts it may")]
    ReceiptsExhausted {
        /// How many it may hold.
        allowed: u64,
    },
    /// One digest was already recorded with a different verified byte length.
    #[error("artifact {digest} is {stored} bytes, not the {provided} bytes supplied")]
    ArtifactLengthConflict {
        /// Content digest whose immutable metadata conflicted.
        digest: String,
        /// Length already recorded for the digest.
        stored: u64,
        /// Length the settlement supplied.
        provided: u64,
    },
    /// The namespace could not take more, or could not be counted.
    #[error(transparent)]
    Capacity(#[from] AccountingFailure),
}

/// Encodes one domain value as the text a column holds.
fn encode<Value: Serialize>(value: &Value) -> Result<String, RepositoryFailure> {
    serde_json::to_string(value).map_err(|failure| RepositoryFailure::NotDecodable {
        column: "a domain value",
        detail: failure.to_string(),
    })
}

/// Decodes one column's text back into the domain value it is.
fn decode<Value: DeserializeOwned>(
    column: &'static str,
    text: &str,
) -> Result<Value, RepositoryFailure> {
    serde_json::from_str(text)
        .map_err(|failure| RepositoryFailure::NotDecodable { column, detail: failure.to_string() })
}

/// Encodes one unit-variant value as the bare word a column holds.
fn encode_word<Value: Serialize>(value: &Value) -> Result<String, RepositoryFailure> {
    let quoted = encode(value)?;
    Ok(quoted.trim_matches('"').to_owned())
}

/// Decodes one column's bare word back into the domain value it is.
fn decode_word<Value: DeserializeOwned>(
    column: &'static str,
    word: &str,
) -> Result<Value, RepositoryFailure> {
    decode(column, &format!("\"{word}\""))
}

/// Requires `text` to fit the contract limit named by `limit`.
fn require_within(field: &'static str, limit: &str, text: &str) -> Result<(), RepositoryFailure> {
    let allowed = DaemonRuntimeContract::embedded().limit(limit);
    let actual = text.len();
    if u64::try_from(actual).unwrap_or(u64::MAX) > allowed {
        return Err(RepositoryFailure::TooLong { field, actual, allowed });
    }
    Ok(())
}

/// Begins a transaction that will write.
///
/// `IMMEDIATE` rather than the default. A deferred transaction starts as a
/// reader and asks for the write lock when it first writes; two that both read
/// and then both try to upgrade cannot both be granted, and SQLite refuses at
/// once rather than waiting, because each holds the read lock the other needs.
/// Taking the write lock up front makes contenders queue instead.
fn write_transaction(
    connection: &rusqlite::Connection,
) -> Result<rusqlite::Transaction<'_>, RepositoryFailure> {
    Ok(rusqlite::Transaction::new_unchecked(connection, rusqlite::TransactionBehavior::Immediate)?)
}

/// What a compare-and-set write decides one row becomes.
///
/// `None` when it becomes what it already was, which commits nothing.
type Change = Option<(OperationSummary, OperationRecord, Option<u64>)>;

/// Requires the stored revision to be the one the caller last saw.
fn require_revision(
    stored: &OperationSummary,
    expected_revision: u64,
) -> Result<(), RepositoryFailure> {
    if stored.record.revision == expected_revision {
        return Ok(());
    }
    Err(RepositoryFailure::RevisionMoved {
        expected: expected_revision,
        stored: stored.record.revision,
    })
}

/// Current execution slots and pending bounds supplied by the namespace owner.
/// These facts are never taken from an operation wire request. The owner keeps
/// its scheduling lock held through admission so the active set cannot change.
#[derive(Debug, Clone, Copy)]
pub struct PendingAdmissionCapacity<'owner> {
    /// Operation identifiers currently holding an execution slot in this target.
    pub active_operations: &'owner std::collections::BTreeSet<String>,
    /// Maximum waiting operations across this target.
    pub global_pending: u64,
    /// Maximum waiting operations belonging to the requesting caller.
    pub pending_per_caller: u64,
}

/// One request to admit an operation.
///
/// Everything here is written in the first-admission transaction, before a
/// scheduler can see the row. The installation identifier is a snapshot: a row
/// surviving a reinstall has to say which installation admitted it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AdmissionRequest {
    /// The opaque author-target identity, stored whole.
    pub author_target_identity: String,
    /// The digest that partitions every table.
    pub author_target_identity_digest: String,
    /// Who asked, when a caller said.
    pub caller_identity: Option<String>,
    /// The canonical command text.
    pub canonical_command: String,
    /// That command's fingerprint against that revision.
    pub command_fingerprint: CommandFingerprint,
    /// The command's wire name.
    pub command_wire_name: String,
    /// The runtime contract this daemon runs under.
    pub daemon_runtime_contract_digest: String,
    /// The installation admitting this operation.
    pub installation_identifier: InstallationIdentifier,
    /// The identifier the caller chose.
    pub operation_identifier: String,
    /// The environment revision this daemon started from.
    pub selected_environment_revision: String,
    /// The workflow this belongs to, when it belongs to one.
    pub workflow_correlation_identifier: Option<String>,
}

/// One operation as the repository holds it.
///
/// Every field is a decoded domain value rather than a column's text, so a
/// caller cannot read a lifecycle state this daemon does not have. The target
/// digest travels here because it is not a secret; the identity it digests
/// does not.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OperationSummary {
    /// The partition this operation belongs to.
    pub author_target_identity_digest: String,
    /// Who asked, when a caller said.
    pub caller_identity: Option<String>,
    /// The fingerprint admitted with it, which never changes.
    pub command_fingerprint: CommandFingerprint,
    /// The command's wire name.
    pub command_wire_name: String,
    /// Where this operation sits in its partition's arrival order.
    pub enqueue_sequence: u64,
    /// The installation that admitted it.
    pub installation_identifier: InstallationIdentifier,
    /// The identifier the caller chose.
    pub operation_identifier: String,
    /// State, progress, recovery, and terminal failure, folded.
    pub record: OperationRecord,
    /// When it was admitted.
    pub recorded_at_unix_milliseconds: u64,
    /// Where its result went, once it has one.
    pub result_disposition: Option<ResultDisposition>,
    /// Canonical inline result bytes, when the result is inline.
    pub result_inline_bytes: Option<String>,
    /// The environment revision it was admitted against.
    pub selected_environment_revision: String,
    /// When it settled, if it has.
    pub settled_at_unix_milliseconds: Option<u64>,
    /// The workflow it belongs to, when it belongs to one.
    pub workflow_correlation_identifier: Option<String>,
}

/// Execution input read together with its lifecycle and revision in one
/// database snapshot. Reading this value does not claim or authorize execution;
/// the scheduler must still acquire the applicable durable fence.
pub struct RetainedExecutionInput {
    /// Identity and lifecycle from the same read transaction as the payload.
    pub summary: OperationSummary,
    /// Exact admitted bytes, preserved for submission and recovery.
    pub canonical_command: String,
    /// Runtime contract under which these bytes were admitted.
    pub daemon_runtime_contract_digest: String,
}

impl core::fmt::Debug for RetainedExecutionInput {
    fn fmt(&self, formatter: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        formatter.write_str("RetainedExecutionInput([redacted])")
    }
}

/// What admitting an operation did.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AdmissionOutcome {
    /// A new row committed, and this is it.
    Admitted(Box<OperationSummary>),
    /// The same work was already admitted, and this is that row.
    Replayed(Box<OperationSummary>),
    /// The identifier is taken by different work, and nothing changed.
    Conflict(Box<OperationSummary>),
}

impl AdmissionOutcome {
    /// Returns the row this outcome is about, whichever outcome it is.
    #[must_use]
    pub fn summary(&self) -> &OperationSummary {
        match self {
            Self::Admitted(summary) | Self::Replayed(summary) | Self::Conflict(summary) => summary,
        }
    }
}

/// What recording a resume receipt did.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ResumeOutcome {
    /// The receipt committed with the revision it made eligible.
    Applied(Box<RecoveryResumeReceipt>),
    /// A receipt for that source was already committed, and this is it.
    Replayed(Box<RecoveryResumeReceipt>),
    /// The operation no longer meets the request's eligibility preconditions.
    Refused(ResumeEligibilityRefusal),
}

/// Why an operation changed before an atomic resume receipt could commit.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ResumeEligibilityRefusal {
    /// The selected environment differs from the operation's durable one.
    EnvironmentRevision,
    /// A terminal operation cannot be resumed.
    Terminal,
    /// No recovery fact is outstanding.
    NotWaiting,
    /// A different recovery category is outstanding.
    Category {
        /// The category the durable recovery fact holds.
        holding: RecoveryCategory,
        /// The category the resume request named.
        named: RecoveryCategory,
    },
    /// The outstanding recovery is not manually resumable.
    NotManual,
    /// The operation revision moved after the request was formed.
    Revision {
        /// The revision the resume request was formed against.
        expected: u64,
        /// The revision durably observed inside the receipt transaction.
        observed: u64,
    },
}

/// The operation ledger, reached only through its own vocabulary.
pub struct OperationRepository {
    /// The open database every call runs inside.
    database: OperationDatabase,
    /// The bounds this namespace is held to.
    policy: PersistentCapacityPolicy,
}

/// Returns one column as a count, refusing a stored negative.
fn count(row: &rusqlite::Row<'_>, column: &str) -> Result<u64, RepositoryFailure> {
    let stored: i64 = row.get(column)?;
    u64::try_from(stored).map_err(|_| RepositoryFailure::NotDecodable {
        column: "a count",
        detail: format!("{stored} is below zero"),
    })
}

/// Returns the evidence the two recovery columns spell together.
fn evidence_from_columns(
    kind: &str,
    certainty: Option<&str>,
) -> Result<RecoveryExecutionEvidence, RepositoryFailure> {
    match (kind, certainty) {
        (EXECUTION_CERTAINTY_KIND, Some(spelling)) => {
            Ok(RecoveryExecutionEvidence::ExecutionCertainty {
                certainty: decode_word("evidence_certainty", spelling)?,
            })
        }
        (AUTHORITATIVE_REMOTE_SUCCESS_KIND, None) => {
            Ok(RecoveryExecutionEvidence::AuthoritativeRemoteSuccess)
        }
        _ => Err(RepositoryFailure::NotDecodable {
            column: "evidence_kind",
            detail: format!("{kind} does not pair with a certainty this way"),
        }),
    }
}

/// Returns the two recovery columns one evidence spells.
fn evidence_columns(
    evidence: RecoveryExecutionEvidence,
) -> Result<(&'static str, Option<String>), RepositoryFailure> {
    match evidence {
        RecoveryExecutionEvidence::ExecutionCertainty { certainty } => {
            Ok((EXECUTION_CERTAINTY_KIND, Some(encode_word(&certainty)?)))
        }
        RecoveryExecutionEvidence::AuthoritativeRemoteSuccess => {
            Ok((AUTHORITATIVE_REMOTE_SUCCESS_KIND, None))
        }
    }
}

/// Returns the terminal failure the three columns spell, when they spell one.
fn terminal_from_columns(
    kind: Option<String>,
    disposition: Option<String>,
    metadata: Option<String>,
) -> Result<Option<TerminalFailure>, RepositoryFailure> {
    match (kind, disposition) {
        (None, None) => Ok(None),
        (Some(kind), Some(disposition)) => Ok(Some(TerminalFailure {
            disposition: decode::<TerminalFailureDisposition>(
                "terminal_failure_disposition",
                &disposition,
            )?,
            kind: decode_word::<TerminalFailureKind>("terminal_failure_kind", &kind)?,
            metadata,
        })),
        _ => Err(RepositoryFailure::NotDecodable {
            column: "terminal_failure_kind",
            detail: "a terminal failure has both a kind and a disposition".to_owned(),
        }),
    }
}

impl OperationRepository {
    /// Returns a repository over `database`, held to the contract's bounds.
    #[must_use]
    pub fn new(database: OperationDatabase) -> Self {
        Self { database, policy: PersistentCapacityPolicy::embedded() }
    }

    /// Returns a repository holding its namespace to `policy` rather than to
    /// the contract's own bounds, which is what a test with reachable limits
    /// needs.
    #[must_use]
    pub fn bounded(database: OperationDatabase, policy: PersistentCapacityPolicy) -> Self {
        Self { database, policy }
    }

    /// Returns the database this repository reads and writes.
    #[must_use]
    pub fn database(&self) -> &OperationDatabase {
        &self.database
    }

    /// Admits one operation, or returns the row that already answers for it.
    ///
    /// One `synchronous = FULL` transaction reserves the arrival sequence,
    /// writes the row as `queued`, and commits; a caller is told it was
    /// admitted only after that commit returns. A row already under that name
    /// is a replay when the revision and fingerprint match, and otherwise a
    /// conflict, which writes nothing.
    ///
    /// # Errors
    ///
    /// Returns [`RepositoryFailure`] for a bounded field over its bound, a
    /// namespace at its capacity, or a refusal.
    pub fn admit(
        &self,
        request: &AdmissionRequest,
        now_unix_milliseconds: u64,
    ) -> Result<AdmissionOutcome, RepositoryFailure> {
        self.admit_checked(request, now_unix_milliseconds, None)
    }

    /// Admits under pending-work bounds in the same transaction as the insert.
    /// Replays and conflicts are resolved before capacity is consulted.
    ///
    /// # Errors
    ///
    /// Returns [`RepositoryFailure::PendingCapacity`] without inserting a row,
    /// or the same persistent/storage refusals as [`Self::admit`].
    pub fn admit_with_pending_capacity(
        &self,
        request: &AdmissionRequest,
        now_unix_milliseconds: u64,
        capacity: PendingAdmissionCapacity<'_>,
    ) -> Result<AdmissionOutcome, RepositoryFailure> {
        self.admit_checked(request, now_unix_milliseconds, Some(capacity))
    }

    fn admit_checked(
        &self,
        request: &AdmissionRequest,
        now_unix_milliseconds: u64,
        pending_capacity: Option<PendingAdmissionCapacity<'_>>,
    ) -> Result<AdmissionOutcome, RepositoryFailure> {
        if let Some(workflow) = request.workflow_correlation_identifier.as_deref() {
            require_within(
                "workflow_correlation_identifier",
                "maximum_workflow_correlation_identifier_bytes",
                workflow,
            )?;
        }
        let transaction = write_transaction(self.database.connection())?;
        let outcome = match Self::read_within(
            &transaction,
            &request.author_target_identity_digest,
            &request.operation_identifier,
        )? {
            Some(stored) => Self::classify_stored(request, stored),
            None => {
                if let Some(capacity) = pending_capacity {
                    Self::require_pending_room(&transaction, request, capacity)?;
                }
                self.require_room()?;
                self.insert(&transaction, request, now_unix_milliseconds)?;
                let admitted = self.read_required(
                    &transaction,
                    &request.author_target_identity_digest,
                    &request.operation_identifier,
                )?;
                AdmissionOutcome::Admitted(Box::new(admitted))
            }
        };
        transaction.commit()?;
        Ok(outcome)
    }

    fn require_pending_room(
        transaction: &rusqlite::Transaction<'_>,
        request: &AdmissionRequest,
        capacity: PendingAdmissionCapacity<'_>,
    ) -> Result<(), RepositoryFailure> {
        let mut query =
            transaction.prepare(statement("count waiting operations during admission"))?;
        let mut rows = query.query(rusqlite::params![request.author_target_identity_digest])?;
        let mut global = 0_u64;
        let mut caller = 0_u64;
        while let Some(row) = rows.next()? {
            let identifier: String = row.get(0)?;
            if capacity.active_operations.contains(&identifier) {
                continue;
            }
            global = global.saturating_add(1);
            let identity: Option<String> = row.get(1)?;
            if identity == request.caller_identity {
                caller = caller.saturating_add(1);
            }
        }
        for (held, limit, per_caller) in
            [(global, capacity.global_pending, false), (caller, capacity.pending_per_caller, true)]
        {
            if held >= limit {
                return Err(RepositoryFailure::PendingCapacity { held, limit, per_caller });
            }
        }
        Ok(())
    }

    /// Requires this namespace to have room for one more operation row.
    ///
    /// Asked only where a row would be created, so a replay consumes nothing
    /// and is never refused for space.
    fn require_room(&self) -> Result<(), RepositoryFailure> {
        self.account().require_room_for_operation()?;
        Ok(())
    }

    /// Returns the accounting this namespace is held to.
    fn account(&self) -> PersistentCapacityAccount<'_> {
        PersistentCapacityAccount::new(&self.database, self.policy)
    }

    /// Returns what a stored row makes of a repeated identifier.
    fn classify_stored(request: &AdmissionRequest, stored: OperationSummary) -> AdmissionOutcome {
        let repeat = classify_repeat(
            &stored.command_fingerprint,
            &stored.selected_environment_revision,
            &request.command_fingerprint,
            &request.selected_environment_revision,
        );
        match repeat {
            RepeatedIdentifier::Retry => AdmissionOutcome::Replayed(Box::new(stored)),
            RepeatedIdentifier::Conflict => AdmissionOutcome::Conflict(Box::new(stored)),
        }
    }

    /// Writes one new operation row inside `transaction`.
    fn insert(
        &self,
        transaction: &rusqlite::Transaction<'_>,
        request: &AdmissionRequest,
        now_unix_milliseconds: u64,
    ) -> Result<(), RepositoryFailure> {
        let sequence: i64 = transaction.query_row(
            statement("reserve the next enqueue sequence inside one target partition"),
            rusqlite::params![request.author_target_identity_digest],
            |row| row.get(0),
        )?;
        let admitted = OperationRecord::admitted();
        transaction.execute(
            statement("admit one operation"),
            rusqlite::params![
                request.author_target_identity,
                request.author_target_identity_digest,
                request.caller_identity,
                request.canonical_command,
                request.command_fingerprint.as_text(),
                request.command_wire_name,
                request.daemon_runtime_contract_digest,
                sequence,
                request.installation_identifier.as_text(),
                encode_word(&admitted.lifecycle_state)?,
                request.operation_identifier,
                i64::try_from(admitted.revision).unwrap_or(i64::MAX),
                i64::try_from(now_unix_milliseconds).unwrap_or(i64::MAX),
                request.selected_environment_revision,
                request.workflow_correlation_identifier,
            ],
        )?;
        Ok(())
    }

    /// Returns one operation, or nothing when that partition has no such row.
    ///
    /// # Errors
    ///
    /// Returns [`RepositoryFailure`] for an undecodable value or a refusal.
    pub fn read(
        &self,
        author_target_identity_digest: &str,
        operation_identifier: &str,
    ) -> Result<Option<OperationSummary>, RepositoryFailure> {
        let transaction = self.database.connection().unchecked_transaction()?;
        let found =
            Self::read_within(&transaction, author_target_identity_digest, operation_identifier)?;
        transaction.commit()?;
        Ok(found)
    }

    /// Reads one operation and its outstanding recovery in one transaction.
    ///
    /// Payload reads are separate from status reads so command arguments never
    /// travel through the public operation summary by accident.
    pub fn read_execution_input(
        &self,
        author_target_identity_digest: &str,
        operation_identifier: &str,
    ) -> Result<Option<RetainedExecutionInput>, RepositoryFailure> {
        let transaction = self.database.connection().unchecked_transaction()?;
        let Some(summary) =
            Self::read_within(&transaction, author_target_identity_digest, operation_identifier)?
        else {
            transaction.commit()?;
            return Ok(None);
        };
        let (canonical_command, daemon_runtime_contract_digest) = transaction.query_row(
            statement("read retained execution input inside its target partition"),
            (author_target_identity_digest, operation_identifier),
            |row| Ok((row.get(0)?, row.get(1)?)),
        )?;
        transaction.commit()?;
        Ok(Some(RetainedExecutionInput {
            summary,
            canonical_command,
            daemon_runtime_contract_digest,
        }))
    }

    /// Reads one operation and its outstanding recovery in one transaction.
    pub(crate) fn read_within(
        transaction: &rusqlite::Transaction<'_>,
        author_target_identity_digest: &str,
        operation_identifier: &str,
    ) -> Result<Option<OperationSummary>, RepositoryFailure> {
        let recovery =
            Self::read_recovery(transaction, author_target_identity_digest, operation_identifier)?;
        let digest = author_target_identity_digest.to_owned();
        let identifier = operation_identifier.to_owned();
        let mut statement =
            transaction.prepare(statement("read one operation inside its target partition"))?;
        let row = statement
            .query_row(
                rusqlite::params![author_target_identity_digest, operation_identifier],
                |row| {
                    Ok(Self::summarize(row, digest.clone(), identifier.clone(), recovery.clone()))
                },
            )
            .optional()?;
        row.transpose()
    }

    /// Reads one operation that has to be there.
    fn read_required(
        &self,
        transaction: &rusqlite::Transaction<'_>,
        author_target_identity_digest: &str,
        operation_identifier: &str,
    ) -> Result<OperationSummary, RepositoryFailure> {
        Self::read_within(transaction, author_target_identity_digest, operation_identifier)?
            .ok_or_else(|| RepositoryFailure::NoSuchOperation {
                identifier: operation_identifier.to_owned(),
            })
    }

    /// Returns the summary one row and its recovery fact make.
    fn summarize(
        row: &rusqlite::Row<'_>,
        author_target_identity_digest: String,
        operation_identifier: String,
        outstanding_recovery: Option<RecoveryFact>,
    ) -> Result<OperationSummary, RepositoryFailure> {
        let fingerprint: String = row.get("command_fingerprint")?;
        let installation: String = row.get("installation_identifier")?;
        let disposition: Option<String> = row.get("result_disposition")?;
        let inline_result: Option<String> = row.get("result_inline_bytes")?;
        let settled: Option<i64> = row.get("settled_at_unix_milliseconds")?;
        let record = OperationRecord {
            latest_progress: row.get("latest_progress")?,
            lifecycle_state: decode_word::<OperationLifecycleState>(
                "lifecycle_state",
                &row.get::<_, String>("lifecycle_state")?,
            )?,
            outstanding_recovery,
            revision: count(row, "operation_revision")?,
            terminal_failure: terminal_from_columns(
                row.get("terminal_failure_kind")?,
                row.get("terminal_failure_disposition")?,
                row.get("terminal_failure_metadata")?,
            )?,
        };
        Ok(OperationSummary {
            author_target_identity_digest,
            caller_identity: row.get("caller_identity")?,
            command_fingerprint: CommandFingerprint::parse(&fingerprint).map_err(|failure| {
                RepositoryFailure::NotDecodable {
                    column: "command_fingerprint",
                    detail: failure.to_string(),
                }
            })?,
            command_wire_name: row.get("command_wire_name")?,
            enqueue_sequence: count(row, "enqueue_sequence")?,
            installation_identifier: InstallationIdentifier::parse(&installation).map_err(
                |failure| RepositoryFailure::NotDecodable {
                    column: "installation_identifier",
                    detail: failure.to_string(),
                },
            )?,
            operation_identifier,
            record,
            recorded_at_unix_milliseconds: count(row, "recorded_at_unix_milliseconds")?,
            result_disposition: disposition
                .map(|spelling| decode_word::<ResultDisposition>("result_disposition", &spelling))
                .transpose()?,
            result_inline_bytes: inline_result,
            selected_environment_revision: row.get("selected_environment_revision")?,
            settled_at_unix_milliseconds: settled
                .map(|value| {
                    u64::try_from(value).map_err(|_| RepositoryFailure::NotDecodable {
                        column: "settled_at_unix_milliseconds",
                        detail: format!("{value} is below zero"),
                    })
                })
                .transpose()?,
            workflow_correlation_identifier: row.get("workflow_correlation_identifier")?,
        })
    }

    /// Reads the one recovery fact an operation is waiting on.
    fn read_recovery(
        transaction: &rusqlite::Transaction<'_>,
        author_target_identity_digest: &str,
        operation_identifier: &str,
    ) -> Result<Option<RecoveryFact>, RepositoryFailure> {
        let mut statement = transaction
            .prepare(statement("read the one recovery fact an operation is waiting on"))?;
        let row = statement
            .query_row(
                rusqlite::params![author_target_identity_digest, operation_identifier],
                |row| Ok(Self::recovery_from(row)),
            )
            .optional()?;
        row.transpose()
    }

    /// Returns the recovery fact one row spells.
    fn recovery_from(row: &rusqlite::Row<'_>) -> Result<RecoveryFact, RepositoryFailure> {
        let kind: String = row.get("evidence_kind")?;
        let certainty: Option<String> = row.get("evidence_certainty")?;
        Ok(RecoveryFact {
            attempt_count: u32::try_from(count(row, "attempt_count")?).unwrap_or(u32::MAX),
            category: decode_word::<RecoveryCategory>(
                "category",
                &row.get::<_, String>("category")?,
            )?,
            detail: row.get("detail")?,
            evidence: evidence_from_columns(&kind, certainty.as_deref())?,
            manual_resume_eligible: row.get::<_, i64>("manual_resume_eligible")? == 1,
            retry_delay_milliseconds: count(row, "retry_delay_milliseconds")?,
            retry_observed_at_unix_milliseconds: count(row, "retry_observed_at_unix_milliseconds")?,
        })
    }

    /// Folds one fact into an operation, under compare-and-set.
    ///
    /// A stale revision writes nothing and says so, which is how two writers
    /// racing on one operation produce a winner. A fold that changes nothing
    /// commits nothing.
    ///
    /// # Errors
    ///
    /// Returns [`RepositoryFailure`] naming the first rule the write breaks: a
    /// missing row, a stale revision, a bounded text over its bound, or a fact
    /// the domain refuses.
    pub fn apply(
        &self,
        author_target_identity_digest: &str,
        operation_identifier: &str,
        expected_revision: u64,
        fact: &OperationFact,
        now_unix_milliseconds: u64,
    ) -> Result<OperationSummary, RepositoryFailure> {
        Self::require_bounded(fact)?;
        self.mutate(
            author_target_identity_digest,
            operation_identifier,
            expected_revision,
            None,
            |stored| {
                let folded = stored.record.fold(fact)?;
                let settled = Self::settlement(stored, &folded, now_unix_milliseconds);
                Ok((folded.revision != stored.record.revision)
                    .then(|| (stored.clone(), folded, settled)))
            },
        )
    }

    /// Applies a local fact only while the exact remote child used to validate
    /// it remains current. Both records are checked in one write transaction.
    pub fn apply_for_retained_agent(
        &self,
        expected: &crate::agent_job_repository::AgentSubmission,
        expected_revision: u64,
        fact: &OperationFact,
        now_unix_milliseconds: u64,
    ) -> Result<OperationSummary, RepositoryFailure> {
        Self::require_bounded(fact)?;
        self.mutate(
            &expected.identity.author_target_identity_digest,
            &expected.identity.operation_identifier,
            expected_revision,
            Some(expected),
            |stored| {
                if stored.selected_environment_revision
                    != expected.identity.selected_environment_revision
                {
                    return Err(RepositoryFailure::RemoteObservationMoved);
                }
                let folded = stored.record.fold(fact)?;
                let settled = Self::settlement(stored, &folded, now_unix_milliseconds);
                Ok((folded.revision != stored.record.revision)
                    .then(|| (stored.clone(), folded, settled)))
            },
        )
    }

    /// Records one failed recovery attempt while the entire captured subscription
    /// context still matches. This changes only local recovery scheduling and
    /// cannot replace existing execution evidence or mark the operation terminal.
    pub fn record_subscription_probe_recovery(
        &self,
        ledger: &crate::agent_subscription_ledger::AgentSubscriptionLedger,
        expected: &crate::agent_subscription_ledger::SubscriptionRecoveryView<'_>,
        agent_operation_identifier: &str,
        expected_revision: u64,
        recovery: RecoveryFact,
        now: u64,
    ) -> Result<OperationSummary, RepositoryFailure> {
        let refuse = || RepositoryFailure::RemoteObservationMoved;
        if !self.database.shares_database_with(ledger.database()) || now > i64::MAX as u64 {
            return Err(refuse());
        }
        let member = expected
            .members()
            .iter()
            .find(|member| member.identity.agent_operation_identifier == agent_operation_identifier)
            .ok_or_else(refuse)?;
        if now < member.recorded_at_unix_milliseconds {
            return Err(refuse());
        }
        let fact = OperationFact::Recovery { recovery: recovery.clone() };
        Self::require_bounded(&fact)?;
        let transaction = write_transaction(self.database.connection())?;
        ledger.require_recovery_current(&transaction, expected).map_err(|_| refuse())?;
        let identity = &member.identity;
        let stored = self.read_required(
            &transaction,
            &identity.author_target_identity_digest,
            &identity.operation_identifier,
        )?;
        require_revision(&stored, expected_revision)?;
        let previous = stored.record.outstanding_recovery.as_ref();
        if stored.record.lifecycle_state.is_terminal()
            || stored.selected_environment_revision != identity.selected_environment_revision
            || previous.is_some_and(|held| held.evidence != recovery.evidence)
            || recovery.attempt_count
                != previous
                    .map_or(Some(1), |held| held.attempt_count.checked_add(1))
                    .ok_or_else(refuse)?
        {
            return Err(refuse());
        }
        let folded = stored.record.fold(&fact)?;
        self.write_folded(&transaction, &stored, &folded, Self::settlement(&stored, &folded, now))?;
        let result = self.read_required(
            &transaction,
            &identity.author_target_identity_digest,
            &identity.operation_identifier,
        )?;
        transaction.commit()?;
        Ok(result)
    }

    /// Settles unavailable prior-generation truth only while the complete
    /// subscription recovery view still matches in one write transaction.
    /// The caller must authenticate the generation change and establish missing
    /// physical truth first. Network refusal is not this evidence. Known success
    /// must instead continue its separate result-acquisition path.
    pub fn settle_unavailable_generation(
        &self,
        ledger: &crate::agent_subscription_ledger::AgentSubscriptionLedger,
        expected: &crate::agent_subscription_ledger::SubscriptionRecoveryView<'_>,
        agent_operation_identifier: &str,
        current_generation: u64,
        expected_revision: u64,
        now: u64,
    ) -> Result<OperationSummary, RepositoryFailure> {
        use slingshot_domain::operation::{
            OperationExecutionCertainty, RecoveryExecutionEvidence, TerminalFailure,
            TerminalFailureDisposition, TerminalFailureKind,
        };
        let refuse = || RepositoryFailure::RemoteObservationMoved;
        if !self.database.shares_database_with(ledger.database())
            || current_generation == 0
            || current_generation == expected.ledger().agent_event_store_generation
            || now > i64::MAX as u64
        {
            return Err(refuse());
        }
        let member = expected
            .members()
            .iter()
            .find(|member| member.identity.agent_operation_identifier == agent_operation_identifier)
            .ok_or_else(refuse)?;
        if member.identity.agent_event_store_generation == current_generation
            || now < member.recorded_at_unix_milliseconds
            || member.observation.state.is_terminal()
            || member.terminal_disposition.is_some()
        {
            return Err(refuse());
        }
        let transaction = write_transaction(self.database.connection())?;
        ledger.require_recovery_current(&transaction, expected).map_err(|_| refuse())?;
        let identity = &member.identity;
        let stored = self.read_required(
            &transaction,
            &identity.author_target_identity_digest,
            &identity.operation_identifier,
        )?;
        require_revision(&stored, expected_revision)?;
        if stored.record.lifecycle_state.is_terminal()
            || stored.selected_environment_revision != identity.selected_environment_revision
            || stored.record.outstanding_recovery.as_ref().is_some_and(|recovery| {
                matches!(
                    recovery.evidence,
                    RecoveryExecutionEvidence::AuthoritativeRemoteSuccess
                        | RecoveryExecutionEvidence::ExecutionCertainty {
                            certainty: OperationExecutionCertainty::ConfirmedNotExecuted
                        }
                )
            })
        {
            return Err(refuse());
        }
        let certainty = stored
            .record
            .outstanding_recovery
            .as_ref()
            .and_then(|recovery| match recovery.evidence {
                RecoveryExecutionEvidence::ExecutionCertainty { certainty }
                    if certainty != OperationExecutionCertainty::ConfirmedNotExecuted =>
                {
                    Some(certainty)
                }
                _ => None,
            })
            .unwrap_or(OperationExecutionCertainty::RemoteOutcomeUnknown);
        let fact = OperationFact::Terminal {
            failure: TerminalFailure {
                kind: TerminalFailureKind::RemoteStateLost,
                disposition: TerminalFailureDisposition::FailClosedIndeterminate { certainty },
                metadata: None,
            },
        };
        let folded = stored.record.fold(&fact)?;
        self.write_folded(&transaction, &stored, &folded, Some(now))?;
        let result = self.read_required(
            &transaction,
            &identity.author_target_identity_digest,
            &identity.operation_identifier,
        )?;
        transaction.commit()?;
        Ok(result)
    }

    /// Atomically records a command-validated no-effect refusal and its complete
    /// failed remote snapshot. Callers must authenticate and classify the failure
    /// first; unknown outcomes must never enter this method.
    pub fn settle_rejected_agent_snapshot(
        &self,
        expected: &crate::agent_job_repository::AgentSubmission,
        expected_revision: u64,
        snapshot: &crate::agent_job_repository::FailedAgentSnapshot,
        diagnosis: Option<crate::agent_job_repository::RejectedAgentDiagnosis>,
        now: u64,
    ) -> Result<OperationSummary, RepositoryFailure> {
        self.settle_failed_agent_snapshot(
            expected,
            expected_revision,
            snapshot,
            diagnosis,
            false,
            now,
        )
    }

    /// Atomically settles a command-validated positive replication admission
    /// count as remote failure, never as nonexecution or publisher delivery.
    pub fn settle_partial_admission_snapshot(
        &self,
        expected: &crate::agent_job_repository::AgentSubmission,
        expected_revision: u64,
        snapshot: &crate::agent_job_repository::FailedAgentSnapshot,
        now: u64,
    ) -> Result<OperationSummary, RepositoryFailure> {
        self.settle_failed_agent_snapshot(expected, expected_revision, snapshot, None, true, now)
    }

    fn settle_failed_agent_snapshot(
        &self,
        expected: &crate::agent_job_repository::AgentSubmission,
        expected_revision: u64,
        snapshot: &crate::agent_job_repository::FailedAgentSnapshot,
        diagnosis: Option<crate::agent_job_repository::RejectedAgentDiagnosis>,
        partial_admission: bool,
        now: u64,
    ) -> Result<OperationSummary, RepositoryFailure> {
        use slingshot_domain::operation::{
            OperationExecutionCertainty, RecoveryExecutionEvidence, TerminalFailure,
            TerminalFailureDisposition, TerminalFailureKind,
        };
        let refuse = || RepositoryFailure::RemoteObservationMoved;
        let transaction = write_transaction(self.database.connection())?;
        let identity = &expected.identity;
        let current = crate::agent_job_repository::read_submission(
            &transaction,
            &identity.author_target_identity_digest,
            &identity.agent_operation_identifier,
        )
        .map_err(|_| refuse())?;
        if current.as_ref() != Some(expected) || expected.terminal_disposition.is_some() {
            return Err(refuse());
        }
        let stored = self.read_required(
            &transaction,
            &identity.author_target_identity_digest,
            &identity.operation_identifier,
        )?;
        require_revision(&stored, expected_revision)?;
        if stored.selected_environment_revision != identity.selected_environment_revision
            || stored.record.lifecycle_state.is_terminal()
            || stored.record.outstanding_recovery.as_ref().is_some_and(|fact| {
                fact.evidence == RecoveryExecutionEvidence::AuthoritativeRemoteSuccess
            })
        {
            return Err(refuse());
        }
        let fact = OperationFact::Terminal {
            failure: TerminalFailure {
                kind: if partial_admission {
                    TerminalFailureKind::RemoteFailed
                } else {
                    TerminalFailureKind::Rejected
                },
                disposition: if partial_admission {
                    TerminalFailureDisposition::AuthoritativeRemoteFailure
                } else {
                    TerminalFailureDisposition::AuthoritativeNonExecution {
                        certainty: OperationExecutionCertainty::ConfirmedNotExecuted,
                    }
                },
                metadata: diagnosis.map(|diagnosis| diagnosis.as_text().to_owned()),
            },
        };
        Self::require_bounded(&fact)?;
        let folded = stored.record.fold(&fact)?;
        self.write_folded(&transaction, &stored, &folded, Some(now))?;
        crate::agent_job_repository::write_failed_snapshot(
            &transaction,
            expected,
            snapshot,
            partial_admission,
            now,
        )
        .map_err(|_| refuse())?;
        let result = self.read_required(
            &transaction,
            &identity.author_target_identity_digest,
            &identity.operation_identifier,
        )?;
        transaction.commit()?;
        Ok(result)
    }

    /// Records or reuses the first artifact-acquisition start while the local
    /// revision and full retained child still match. The anchor cannot drift
    /// to a different artifact or be refreshed by a retry.
    pub fn begin_artifact_acquisition(
        &self,
        expected: &crate::agent_job_repository::AgentSubmission,
        expected_revision: u64,
        artifact_identifier: &str,
        artifact_slot: &str,
        content_digest: &str,
        now: u64,
    ) -> Result<u64, RepositoryFailure> {
        let refuse = || RepositoryFailure::RemoteObservationMoved;
        if [artifact_identifier, content_digest].iter().any(|text| {
            text.len() != 64
                || !text.bytes().all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
        }) || !matches!(artifact_slot, "content_package" | "loaded_content_json")
        {
            return Err(refuse());
        }
        let now = i64::try_from(now).map_err(|_| refuse())?;
        let transaction = write_transaction(self.database.connection())?;
        let identity = &expected.identity;
        let current = crate::agent_job_repository::read_submission(
            &transaction,
            &identity.author_target_identity_digest,
            &identity.agent_operation_identifier,
        )
        .map_err(|_| refuse())?;
        if current.as_ref() != Some(expected) || expected.terminal_disposition.is_some() {
            return Err(refuse());
        }
        let stored = self.read_required(
            &transaction,
            &identity.author_target_identity_digest,
            &identity.operation_identifier,
        )?;
        require_revision(&stored, expected_revision)?;
        if stored.selected_environment_revision != identity.selected_environment_revision
            || stored.record.lifecycle_state.is_terminal()
            || !stored.record.outstanding_recovery.as_ref().is_some_and(|fact| fact.evidence == slingshot_domain::operation::RecoveryExecutionEvidence::AuthoritativeRemoteSuccess)
        { return Err(refuse()); }
        transaction.execute(
            statement("record the first artifact acquisition for one retained child"),
            rusqlite::params![
                artifact_identifier,
                artifact_slot,
                content_digest,
                now,
                identity.author_target_identity_digest,
                identity.agent_operation_identifier
            ],
        )?;
        let (identifier, slot, digest, started): (String, String, String, i64) = transaction
            .query_row(
                statement("read one retained artifact acquisition anchor"),
                rusqlite::params![
                    identity.author_target_identity_digest,
                    identity.agent_operation_identifier
                ],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
            )?;
        if identifier != artifact_identifier || slot != artifact_slot || digest != content_digest {
            return Err(refuse());
        }
        let started = u64::try_from(started).map_err(|_| refuse())?;
        transaction.commit()?;
        Ok(started)
    }

    /// Commits a complete successful result and its terminal lifecycle together.
    ///
    /// The operation, verified artifact associations, result representation,
    /// settlement time, and recovery clearing share one immediate transaction.
    /// Consequently a reader sees either the prior recoverable operation or a
    /// complete success, never a terminal row awaiting a second result write.
    pub fn settle_success(
        &self,
        author_target_identity_digest: &str,
        operation_identifier: &str,
        settlement: &SuccessfulSettlement,
    ) -> Result<OperationSummary, RepositoryFailure> {
        self.settle_success_guarded(
            author_target_identity_digest,
            operation_identifier,
            settlement,
            None,
            None,
            &[],
        )
    }

    /// Publishes a verified remote result only while the exact retained child
    /// and local owner revision still match, in the same immediate transaction.
    /// This is a persistence guard, not proof of wire/result validation.
    pub fn settle_success_for_retained_agent(
        &self,
        expected: &crate::agent_job_repository::AgentSubmission,
        settlement: &SuccessfulSettlement,
    ) -> Result<OperationSummary, RepositoryFailure> {
        self.settle_success_guarded(
            &expected.identity.author_target_identity_digest,
            &expected.identity.operation_identifier,
            settlement,
            Some(expected),
            None,
            &[],
        )
    }

    /// Atomically publishes the successful remote snapshot and complete local result.
    /// The exact child and local revision guards apply to all writes.
    pub fn settle_success_for_agent_snapshot(
        &self,
        expected: &crate::agent_job_repository::AgentSubmission,
        settlement: &SuccessfulSettlement,
        snapshot: &crate::agent_job_repository::SuccessfulAgentSnapshot,
    ) -> Result<OperationSummary, RepositoryFailure> {
        self.settle_success_guarded(
            &expected.identity.author_target_identity_digest,
            &expected.identity.operation_identifier,
            settlement,
            Some(expected),
            Some(snapshot),
            &[],
        )
    }

    /// Publishes a complete artifact result and consumes exactly its producer
    /// holds in the same guarded transaction. A failure preserves every hold.
    pub fn settle_success_for_publications(
        &self,
        expected: &crate::agent_job_repository::AgentSubmission,
        settlement: &SuccessfulSettlement,
        snapshot: &crate::agent_job_repository::SuccessfulAgentSnapshot,
        publications: &[crate::persistent_capacity::ArtifactPublication],
    ) -> Result<OperationSummary, RepositoryFailure> {
        if publications.len() != settlement.artifacts.len() || publications.is_empty() {
            return Err(RepositoryFailure::RemoteObservationMoved);
        }
        self.settle_success_guarded(
            &expected.identity.author_target_identity_digest,
            &expected.identity.operation_identifier,
            settlement,
            Some(expected),
            Some(snapshot),
            publications,
        )
    }

    fn settle_success_guarded(
        &self,
        author_target_identity_digest: &str,
        operation_identifier: &str,
        settlement: &SuccessfulSettlement,
        remote: Option<&crate::agent_job_repository::AgentSubmission>,
        snapshot: Option<&crate::agent_job_repository::SuccessfulAgentSnapshot>,
        publications: &[crate::persistent_capacity::ArtifactPublication],
    ) -> Result<OperationSummary, RepositoryFailure> {
        let disposition = settlement.disposition()?;
        if let Some(inline) = &settlement.inline_result {
            require_within("inline result", "maximum_inline_machine_result_bytes", inline)?;
        }
        let transaction = write_transaction(self.database.connection())?;
        if let Some(expected) = remote {
            let current = crate::agent_job_repository::read_submission(
                &transaction,
                &expected.identity.author_target_identity_digest,
                &expected.identity.agent_operation_identifier,
            )
            .map_err(|_| RepositoryFailure::RemoteObservationMoved)?;
            if current.as_ref() != Some(expected) || expected.terminal_disposition.is_some() {
                return Err(RepositoryFailure::RemoteObservationMoved);
            }
        }
        let stored =
            self.read_required(&transaction, author_target_identity_digest, operation_identifier)?;
        if remote.is_some_and(|expected| {
            stored.selected_environment_revision != expected.identity.selected_environment_revision
        }) {
            return Err(RepositoryFailure::RemoteObservationMoved);
        }
        require_revision(&stored, settlement.expected_revision)?;
        if stored.record.lifecycle_state != settlement.expected_lifecycle_state {
            return Err(RepositoryFailure::LifecycleMoved {
                expected: settlement.expected_lifecycle_state,
                stored: stored.record.lifecycle_state,
            });
        }
        let folded = stored.record.fold(&OperationFact::Lifecycle {
            lifecycle_state: OperationLifecycleState::Succeeded,
        })?;
        let folded = OperationRecord { outstanding_recovery: None, ..folded };
        for artifact in &settlement.artifacts {
            self.write_settled_artifact(
                &transaction,
                author_target_identity_digest,
                operation_identifier,
                artifact,
                settlement.settled_at_unix_milliseconds,
            )?;
        }
        let carried = OperationSummary {
            result_disposition: Some(disposition),
            result_inline_bytes: settlement.inline_result.clone(),
            ..stored.clone()
        };
        self.write_folded(
            &transaction,
            &carried,
            &folded,
            Some(settlement.settled_at_unix_milliseconds),
        )?;
        if let Some(snapshot) = snapshot {
            crate::agent_job_repository::write_successful_snapshot(
                &transaction,
                remote.ok_or(RepositoryFailure::RemoteObservationMoved)?,
                snapshot,
                settlement.settled_at_unix_milliseconds,
            )
            .map_err(|_| RepositoryFailure::RemoteObservationMoved)?;
        }
        for (publication, artifact) in publications.iter().zip(&settlement.artifacts) {
            let changed = transaction.execute(
                statement("consume one completed artifact publication"),
                rusqlite::params![
                    publication.identifier(),
                    artifact.artifact_identifier,
                    artifact.content_digest
                ],
            )?;
            if changed != 1 {
                return Err(RepositoryFailure::RemoteObservationMoved);
            }
        }
        let current =
            self.read_required(&transaction, author_target_identity_digest, operation_identifier)?;
        transaction.commit()?;
        Ok(current)
    }

    /// Writes one already-verified artifact association inside a success transaction.
    fn write_settled_artifact(
        &self,
        transaction: &rusqlite::Transaction<'_>,
        author_target_identity_digest: &str,
        operation_identifier: &str,
        artifact: &ProducedArtifact,
        now_unix_milliseconds: u64,
    ) -> Result<(), RepositoryFailure> {
        let byte_length = i64::try_from(artifact.byte_length).unwrap_or(i64::MAX);
        let existing: Option<i64> = transaction
            .query_row(
                statement("read one artifact blob's recorded length"),
                rusqlite::params![artifact.content_digest],
                |row| row.get(0),
            )
            .optional()?;
        if let Some(existing) = existing {
            let stored = u64::try_from(existing).map_err(|_| RepositoryFailure::NotDecodable {
                column: "artifact_blob.byte_length",
                detail: format!("{existing} is below zero"),
            })?;
            if stored != artifact.byte_length {
                return Err(RepositoryFailure::ArtifactLengthConflict {
                    digest: artifact.content_digest.clone(),
                    stored,
                    provided: artifact.byte_length,
                });
            }
        }
        transaction.execute(
            statement("record one artifact's content, once per digest"),
            rusqlite::params![
                byte_length,
                artifact.content_digest,
                i64::try_from(now_unix_milliseconds).unwrap_or(i64::MAX),
            ],
        )?;
        transaction.execute(
            statement("associate one artifact with the operation slot it fills"),
            rusqlite::params![
                artifact.artifact_identifier,
                artifact.artifact_slot,
                author_target_identity_digest,
                byte_length,
                artifact.content_digest,
                artifact.media_type,
                operation_identifier,
            ],
        )?;
        Ok(())
    }

    /// Runs one compare-and-set write, whatever the write turns out to be.
    ///
    /// Reading the row, holding the caller to its revision, and reading back
    /// what committed are the same for every write. `change` decides only what
    /// the row becomes, and answers with nothing when it becomes what it was.
    fn mutate(
        &self,
        author_target_identity_digest: &str,
        operation_identifier: &str,
        expected_revision: u64,
        remote: Option<&crate::agent_job_repository::AgentSubmission>,
        change: impl FnOnce(&OperationSummary) -> Result<Change, RepositoryFailure>,
    ) -> Result<OperationSummary, RepositoryFailure> {
        let transaction = write_transaction(self.database.connection())?;
        if let Some(expected) = remote {
            let current = crate::agent_job_repository::read_submission(
                &transaction,
                &expected.identity.author_target_identity_digest,
                &expected.identity.agent_operation_identifier,
            )
            .map_err(|_| RepositoryFailure::RemoteObservationMoved)?;
            if current.as_ref() != Some(expected) || expected.terminal_disposition.is_some() {
                return Err(RepositoryFailure::RemoteObservationMoved);
            }
        }
        let stored =
            self.read_required(&transaction, author_target_identity_digest, operation_identifier)?;
        require_revision(&stored, expected_revision)?;
        if let Some((carried, folded, settled)) = change(&stored)? {
            self.write_folded(&transaction, &carried, &folded, settled)?;
        }
        let current =
            self.read_required(&transaction, author_target_identity_digest, operation_identifier)?;
        transaction.commit()?;
        Ok(current)
    }

    /// Requires every bounded text a fact carries to fit its bound.
    fn require_bounded(fact: &OperationFact) -> Result<(), RepositoryFailure> {
        match fact {
            OperationFact::Progress { detail } => {
                require_within("progress detail", "maximum_progress_detail_bytes", detail)
            }
            OperationFact::Recovery { recovery } => {
                require_within("recovery detail", "maximum_recovery_detail_bytes", &recovery.detail)
            }
            OperationFact::Terminal { failure } => match failure.metadata.as_deref() {
                Some(metadata) => require_within(
                    "terminal failure metadata",
                    "maximum_terminal_failure_metadata_bytes",
                    metadata,
                ),
                None => Ok(()),
            },
            OperationFact::Lifecycle { .. } => Ok(()),
        }
    }

    /// Returns the settlement instant a fold produces, when it settles one.
    ///
    /// An operation settles once, so a fold leaving a terminal row terminal
    /// keeps the instant it settled at.
    fn settlement(
        stored: &OperationSummary,
        folded: &OperationRecord,
        now_unix_milliseconds: u64,
    ) -> Option<u64> {
        match (stored.record.lifecycle_state.is_terminal(), folded.lifecycle_state.is_terminal()) {
            (false, true) => Some(now_unix_milliseconds),
            (true, _) => stored.settled_at_unix_milliseconds,
            (false, false) => None,
        }
    }

    /// Writes one folded record, its recovery fact, and its settlement.
    fn write_folded(
        &self,
        transaction: &rusqlite::Transaction<'_>,
        stored: &OperationSummary,
        folded: &OperationRecord,
        settled_at_unix_milliseconds: Option<u64>,
    ) -> Result<(), RepositoryFailure> {
        let terminal = folded.terminal_failure.as_ref();
        let changed = transaction.execute(
            statement("record one folded operation under compare-and-set"),
            rusqlite::params![
                folded.latest_progress,
                encode_word(&folded.lifecycle_state)?,
                i64::try_from(folded.revision).unwrap_or(i64::MAX),
                stored.result_disposition.map(|held| encode_word(&held)).transpose()?,
                stored.result_inline_bytes,
                settled_at_unix_milliseconds.map(|at| i64::try_from(at).unwrap_or(i64::MAX)),
                terminal.map(|failure| encode(&failure.disposition)).transpose()?,
                terminal.map(|failure| encode_word(&failure.kind)).transpose()?,
                terminal.and_then(|failure| failure.metadata.clone()),
                stored.author_target_identity_digest,
                stored.operation_identifier,
                i64::try_from(stored.record.revision).unwrap_or(i64::MAX),
            ],
        )?;
        if changed != ONE_ROW {
            return Err(RepositoryFailure::RevisionMoved {
                expected: stored.record.revision,
                stored: stored.record.revision,
            });
        }
        self.write_recovery(transaction, stored, folded.outstanding_recovery.as_ref())
    }

    /// Writes or clears the one recovery fact an operation is waiting on.
    fn write_recovery(
        &self,
        transaction: &rusqlite::Transaction<'_>,
        stored: &OperationSummary,
        recovery: Option<&RecoveryFact>,
    ) -> Result<(), RepositoryFailure> {
        let Some(recovery) = recovery else {
            transaction.execute(
                statement("clear the recovery fact an operation is no longer waiting on"),
                rusqlite::params![
                    stored.author_target_identity_digest,
                    stored.operation_identifier
                ],
            )?;
            return Ok(());
        };
        let (kind, certainty) = evidence_columns(recovery.evidence)?;
        transaction.execute(
            statement("record the one recovery fact an operation is waiting on"),
            rusqlite::params![
                i64::from(recovery.attempt_count),
                stored.author_target_identity_digest,
                encode_word(&recovery.category)?,
                recovery.detail,
                certainty,
                kind,
                i64::from(recovery.manual_resume_eligible),
                stored.operation_identifier,
                i64::try_from(recovery.retry_delay_milliseconds).unwrap_or(i64::MAX),
                i64::try_from(recovery.retry_observed_at_unix_milliseconds).unwrap_or(i64::MAX),
            ],
        )?;
        Ok(())
    }

    /// Returns every operation in one partition, in the order it arrived.
    ///
    /// Nonterminal rows are the work still to do, in their callers' order.
    ///
    /// # Errors
    ///
    /// Returns [`RepositoryFailure`] when a stored value does not decode or the
    /// database refuses.
    pub fn reconstruct(
        &self,
        author_target_identity_digest: &str,
    ) -> Result<Vec<OperationSummary>, RepositoryFailure> {
        let transaction = self.database.connection().unchecked_transaction()?;
        let identifiers: Vec<String> = {
            let mut prepared = transaction
                .prepare(statement("reconstruct one target's operations in enqueue order"))?;
            let rows = prepared
                .query_map(rusqlite::params![author_target_identity_digest], |row| {
                    row.get::<_, String>(0)
                })?;
            rows.collect::<Result<Vec<String>, _>>()?
        };
        let mut found = Vec::with_capacity(identifiers.len());
        for identifier in identifiers {
            found.push(self.read_required(
                &transaction,
                author_target_identity_digest,
                &identifier,
            )?);
        }
        transaction.commit()?;
        Ok(found)
    }

    /// Records one recovery-resume receipt, or replays the one already there.
    ///
    /// Keyed by target and source fingerprint, so an identical request finds
    /// its own committed proof whatever the operation has done since. Whether a
    /// resume took effect cannot be reconstructed from current state.
    ///
    /// # Errors
    ///
    /// Returns [`RepositoryFailure::ReceiptsExhausted`] once the operation
    /// holds every receipt the contract allows, or a database failure.
    pub fn record_resume_receipt(
        &self,
        author_target_identity_digest: &str,
        operation_identifier: &str,
        source_fingerprint: &str,
        selected_environment_revision: &str,
        applied_operation_revision: u64,
        now_unix_milliseconds: u64,
    ) -> Result<ResumeOutcome, RepositoryFailure> {
        let transaction = write_transaction(self.database.connection())?;
        if let Some(held) = Self::receipt_within(
            &transaction,
            author_target_identity_digest,
            operation_identifier,
            source_fingerprint,
        )? {
            if held.selected_environment_revision != selected_environment_revision {
                return Ok(ResumeOutcome::Refused(ResumeEligibilityRefusal::EnvironmentRevision));
            }
            transaction.commit()?;
            return Ok(ResumeOutcome::Replayed(Box::new(held)));
        }
        self.require_receipt_capacity_within(
            &transaction,
            author_target_identity_digest,
            operation_identifier,
        )?;
        transaction.execute(
            statement("record one recovery-resume receipt"),
            rusqlite::params![
                i64::try_from(applied_operation_revision).unwrap_or(i64::MAX),
                author_target_identity_digest,
                operation_identifier,
                i64::try_from(now_unix_milliseconds).unwrap_or(i64::MAX),
                selected_environment_revision,
                source_fingerprint,
            ],
        )?;
        let written = Self::receipt_within(
            &transaction,
            author_target_identity_digest,
            operation_identifier,
            source_fingerprint,
        )?
        .ok_or_else(|| RepositoryFailure::NoSuchOperation {
            identifier: operation_identifier.to_owned(),
        })?;
        transaction.commit()?;
        Ok(ResumeOutcome::Applied(Box::new(written)))
    }

    /// Rechecks resumability and records one receipt in the same immediate transaction.
    ///
    /// Unlike [`Self::record_resume_receipt`], this is the product boundary: a
    /// stale caller can never commit an `Applied` receipt for a terminal,
    /// moved, or differently recovering operation.
    pub fn record_eligible_resume_receipt(
        &self,
        author_target_identity_digest: &str,
        operation_identifier: &str,
        source_fingerprint: &str,
        selected_environment_revision: &str,
        expected_recovery_category: RecoveryCategory,
        expected_operation_revision: u64,
        now_unix_milliseconds: u64,
    ) -> Result<ResumeOutcome, RepositoryFailure> {
        let transaction = write_transaction(self.database.connection())?;
        if let Some(held) = Self::receipt_within(
            &transaction,
            author_target_identity_digest,
            operation_identifier,
            source_fingerprint,
        )? {
            if held.selected_environment_revision != selected_environment_revision {
                return Ok(ResumeOutcome::Refused(ResumeEligibilityRefusal::EnvironmentRevision));
            }
            transaction.commit()?;
            return Ok(ResumeOutcome::Replayed(Box::new(held)));
        }
        let current =
            self.read_required(&transaction, author_target_identity_digest, operation_identifier)?;
        let refusal = if current.selected_environment_revision != selected_environment_revision {
            Some(ResumeEligibilityRefusal::EnvironmentRevision)
        } else if current.record.lifecycle_state.is_terminal() {
            Some(ResumeEligibilityRefusal::Terminal)
        } else if let Some(recovery) = &current.record.outstanding_recovery {
            if recovery.category != expected_recovery_category {
                Some(ResumeEligibilityRefusal::Category {
                    holding: recovery.category,
                    named: expected_recovery_category,
                })
            } else if !recovery.manual_resume_eligible {
                Some(ResumeEligibilityRefusal::NotManual)
            } else if current.record.revision != expected_operation_revision {
                Some(ResumeEligibilityRefusal::Revision {
                    expected: expected_operation_revision,
                    observed: current.record.revision,
                })
            } else {
                None
            }
        } else {
            Some(ResumeEligibilityRefusal::NotWaiting)
        };
        if let Some(refusal) = refusal {
            transaction.commit()?;
            return Ok(ResumeOutcome::Refused(refusal));
        }
        self.require_receipt_capacity_within(
            &transaction,
            author_target_identity_digest,
            operation_identifier,
        )?;
        transaction.execute(
            statement("record one recovery-resume receipt"),
            rusqlite::params![
                i64::try_from(expected_operation_revision).unwrap_or(i64::MAX),
                author_target_identity_digest,
                operation_identifier,
                i64::try_from(now_unix_milliseconds).unwrap_or(i64::MAX),
                selected_environment_revision,
                source_fingerprint,
            ],
        )?;
        let written = Self::receipt_within(
            &transaction,
            author_target_identity_digest,
            operation_identifier,
            source_fingerprint,
        )?
        .ok_or_else(|| RepositoryFailure::NoSuchOperation {
            identifier: operation_identifier.to_owned(),
        })?;
        transaction.commit()?;
        Ok(ResumeOutcome::Applied(Box::new(written)))
    }

    /// Refuses a fresh receipt once the transaction sees an operation at its bound.
    fn require_receipt_capacity_within(
        &self,
        connection: &rusqlite::Connection,
        author_target_identity_digest: &str,
        operation_identifier: &str,
    ) -> Result<(), RepositoryFailure> {
        let counted: i64 = connection.query_row(
            statement("count one operation's recovery-resume receipts"),
            rusqlite::params![author_target_identity_digest, operation_identifier],
            |row| row.get(0),
        )?;
        let held = u64::try_from(counted).map_err(|_| RepositoryFailure::NotDecodable {
            column: "recovery-resume receipt count",
            detail: format!("{counted} is not a non-negative count"),
        })?;
        let facts = CapacityFacts {
            held,
            limit: self.policy.recovery_resume_receipts_per_operation,
            wanted: 1,
        };
        if facts.fits() {
            Ok(())
        } else {
            Err(RepositoryFailure::ReceiptsExhausted { allowed: facts.limit })
        }
    }

    /// Activates the exact persisted resume receipt once, guarded by the local
    /// source revision and complete remote child. A stale/replayed receipt does
    /// not clear a later pause. This is not a scheduler lease or permission to POST.
    pub fn activate_retained_recovery(
        &self,
        expected: &crate::agent_job_repository::AgentSubmission,
        receipt: &RecoveryResumeReceipt,
        category: RecoveryCategory,
        now_unix_milliseconds: u64,
    ) -> Result<Option<OperationSummary>, RepositoryFailure> {
        let identity = &expected.identity;
        if receipt.operation_identifier != identity.operation_identifier
            || receipt.selected_environment_revision != identity.selected_environment_revision
            || now_unix_milliseconds < receipt.recorded_at_unix_milliseconds
        {
            return Err(RepositoryFailure::RemoteObservationMoved);
        }
        let transaction = write_transaction(self.database.connection())?;
        let persisted = Self::receipt_within(
            &transaction,
            &identity.author_target_identity_digest,
            &identity.operation_identifier,
            &receipt.source_fingerprint,
        )?;
        if persisted.as_ref() != Some(receipt) {
            return Err(RepositoryFailure::RemoteObservationMoved);
        }
        let stored = self.read_required(
            &transaction,
            &identity.author_target_identity_digest,
            &identity.operation_identifier,
        )?;
        if stored.record.revision != receipt.applied_operation_revision
            || stored.record.lifecycle_state.is_terminal()
        {
            return Ok(None);
        }
        let remote = crate::agent_job_repository::read_submission(
            &transaction,
            &identity.author_target_identity_digest,
            &identity.agent_operation_identifier,
        )
        .map_err(|_| RepositoryFailure::RemoteObservationMoved)?;
        if remote.as_ref() != Some(expected)
            || expected.terminal_disposition.is_some()
            || stored.selected_environment_revision != receipt.selected_environment_revision
        {
            return Err(RepositoryFailure::RemoteObservationMoved);
        }
        let recovery = stored
            .record
            .outstanding_recovery
            .as_ref()
            .filter(|fact| fact.manual_resume_eligible && fact.category == category)
            .ok_or(RepositoryFailure::RemoteObservationMoved)?;
        let fact = OperationFact::Recovery {
            recovery: RecoveryFact {
                manual_resume_eligible: false,
                attempt_count: 0,
                retry_delay_milliseconds: 0,
                retry_observed_at_unix_milliseconds: now_unix_milliseconds,
                detail: "recovery activated by a persisted resume receipt".to_owned(),
                ..recovery.clone()
            },
        };
        Self::require_bounded(&fact)?;
        let folded = stored.record.fold(&fact)?;
        self.write_folded(&transaction, &stored, &folded, None)?;
        let activated = self.read_required(
            &transaction,
            &identity.author_target_identity_digest,
            &identity.operation_identifier,
        )?;
        transaction.commit()?;
        Ok(Some(activated))
    }

    /// Returns one recovery-resume receipt, or nothing.
    ///
    /// # Errors
    ///
    /// Returns [`RepositoryFailure`] when the database refuses.
    pub fn read_resume_receipt(
        &self,
        author_target_identity_digest: &str,
        operation_identifier: &str,
        source_fingerprint: &str,
    ) -> Result<Option<RecoveryResumeReceipt>, RepositoryFailure> {
        Self::receipt_within(
            self.database.connection(),
            author_target_identity_digest,
            operation_identifier,
            source_fingerprint,
        )
    }

    /// Reads one receipt, inside whatever transaction the caller has open.
    fn receipt_within(
        connection: &rusqlite::Connection,
        author_target_identity_digest: &str,
        operation_identifier: &str,
        source_fingerprint: &str,
    ) -> Result<Option<RecoveryResumeReceipt>, RepositoryFailure> {
        let mut prepared = connection.prepare(statement(
            "read one recovery-resume receipt by operation and source fingerprint",
        ))?;
        let row = prepared
            .query_row(
                rusqlite::params![
                    author_target_identity_digest,
                    operation_identifier,
                    source_fingerprint
                ],
                |row| Ok(Self::receipt_from(row, source_fingerprint)),
            )
            .optional()?;
        row.transpose()
    }

    /// Returns the receipt one row spells.
    fn receipt_from(
        row: &rusqlite::Row<'_>,
        source_fingerprint: &str,
    ) -> Result<RecoveryResumeReceipt, RepositoryFailure> {
        Ok(RecoveryResumeReceipt {
            applied_operation_revision: count(row, "applied_operation_revision")?,
            operation_identifier: row.get("operation_identifier")?,
            recorded_at_unix_milliseconds: count(row, "recorded_at_unix_milliseconds")?,
            selected_environment_revision: row.get("selected_environment_revision")?,
            source_fingerprint: source_fingerprint.to_owned(),
        })
    }
}
