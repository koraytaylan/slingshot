//! Admission, transition, lookup, and recovery, with every decision durable.
//!
//! Idempotency here is a property of committed rows rather than of anything the
//! caller remembers. A client that retries after a lost acknowledgement, a
//! daemon that restarts mid-flight, and two clients racing on one identifier all
//! reach the same row through the same rule: an operation is named by its target
//! partition and its identifier together, and a repeat is the same work only
//! when the selected environment revision and the command fingerprint also
//! match. Anything else wearing that name is a conflict, and a conflict changes
//! nothing.
//!
//! The partition is the opaque author-target digest, so one caller's identifier
//! against two targets is two operations - including two targets that differ
//! only by the opaque authentication principal behind the same deployment. That
//! is why replay can never cross a partition: the row it would replay is not
//! the row the caller asked about.
//!
//! Every write is a compare-and-set against the revision the caller last saw,
//! folded through [`OperationRecord`] so a transition's legality is decided
//! once, in the domain, rather than separately by each writer. A fold that
//! changes nothing commits nothing and keeps the revision, which is what makes
//! two writers recording one fact one recorded fact.

use rusqlite::OptionalExtension as _;
use serde::Serialize;
use serde::de::DeserializeOwned;
use slingshot_domain::command_fingerprint::{
    CommandFingerprint, RepeatedIdentifier, classify_repeat,
};
use slingshot_domain::daemon_runtime_contract::DaemonRuntimeContract;
use slingshot_domain::installation::InstallationIdentifier;
use slingshot_domain::operation::{
    LifecycleFailure, OperationFact, OperationLifecycleState, OperationRecord, RecoveryCategory,
    RecoveryExecutionEvidence, RecoveryFact, TerminalFailure, TerminalFailureDisposition,
    TerminalFailureKind,
};

use crate::database::{DatabaseFailure, OperationDatabase};
use crate::sqlite_statement_inventory::STATEMENTS;

/// The recovery evidence column value for unproved execution.
const EXECUTION_CERTAINTY_KIND: &str = "execution_certainty";

/// The recovery evidence column value for a proven remote success.
const AUTHORITATIVE_REMOTE_SUCCESS_KIND: &str = "authoritative_remote_success";

/// Rows one statement changes when it changes the row it named.
const ONE_ROW: usize = 1;
/// Returns the text of the inventoried statement with `purpose`.
///
/// Looking the text up rather than writing it here is what makes the inventory
/// the single place a statement exists: a statement that is not in the list
/// cannot be reached from this module at all.
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
    /// The fact does not belong to the operation as it stands.
    #[error(transparent)]
    Lifecycle(#[from] LifecycleFailure),
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

/// Encodes one unit-variant domain value as the bare word a column holds.
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
/// `IMMEDIATE` rather than the default, and the difference matters under
/// contention. A deferred transaction starts as a reader and asks for the write
/// lock when it first writes; two of them that both read and then both try to
/// upgrade cannot both be granted, and SQLite refuses the upgrade at once
/// rather than waiting, because waiting could not help - each is holding the
/// read lock the other needs. Taking the write lock up front makes contenders
/// queue on it instead, which is what the busy timeout is for.
fn write_transaction(
    connection: &rusqlite::Connection,
) -> Result<rusqlite::Transaction<'_>, RepositoryFailure> {
    Ok(rusqlite::Transaction::new_unchecked(connection, rusqlite::TransactionBehavior::Immediate)?)
}

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

/// One request to admit an operation.
///
/// Everything here is written in the first-admission transaction, before the
/// row is visible to a scheduler and therefore before any executor could act on
/// it. The installation identifier in particular is a snapshot rather than a
/// reference: a row that survives a reinstall has to say which installation
/// admitted it, and asking the current process afterwards would answer with the
/// wrong one.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AdmissionRequest {
    /// The opaque author-target identity, stored whole.
    pub author_target_identity: String,
    /// The digest of that identity, which is what partitions every table.
    pub author_target_identity_digest: String,
    /// Who asked, when a caller said.
    pub caller_identity: Option<String>,
    /// The canonical command text.
    pub canonical_command: String,
    /// The fingerprint of that command against that revision.
    pub command_fingerprint: CommandFingerprint,
    /// The command's wire name.
    pub command_wire_name: String,
    /// The runtime contract this daemon is running under.
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

/// Where an operation's result went.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ResultDisposition {
    /// Small enough to travel in the response itself.
    Inline,
    /// Kept as a content-addressed artifact.
    Artifact,
}

/// One operation as the repository holds it.
///
/// Every field is a decoded domain value rather than the text a column holds,
/// so a caller cannot read a lifecycle state this daemon does not have or a
/// terminal pairing the domain would refuse. The target digest travels on the
/// summary because it is not a secret; the identity it digests does not.
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
    /// The environment revision it was admitted against.
    pub selected_environment_revision: String,
    /// When it settled, if it has.
    pub settled_at_unix_milliseconds: Option<u64>,
    /// The workflow it belongs to, when it belongs to one.
    pub workflow_correlation_identifier: Option<String>,
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

/// One durable proof that a recovery resume was already applied.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RecoveryResumeReceipt {
    /// The revision the resume committed.
    pub applied_operation_revision: u64,
    /// The operation it resumed.
    pub operation_identifier: String,
    /// When it was recorded.
    pub recorded_at_unix_milliseconds: u64,
    /// The environment revision it was recorded against.
    pub selected_environment_revision: String,
    /// The source it is keyed by.
    pub source_fingerprint: String,
}

/// What recording a resume receipt did.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ResumeOutcome {
    /// The receipt committed with the revision it made eligible.
    Applied(Box<RecoveryResumeReceipt>),
    /// A receipt for that source was already committed, and this is it.
    Replayed(Box<RecoveryResumeReceipt>),
}

/// The operation ledger, reached only through its own vocabulary.
pub struct OperationRepository {
    /// The open database every call runs inside.
    database: OperationDatabase,
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
    /// Returns a repository over `database`.
    #[must_use]
    pub fn new(database: OperationDatabase) -> Self {
        Self { database }
    }

    /// Admits one operation, or returns the row that already answers for it.
    ///
    /// One `synchronous = FULL` transaction reserves the arrival sequence,
    /// writes the row as `queued`, and commits; the caller is told it was
    /// admitted only after the commit returns, so an acknowledged operation is
    /// always a durable one.
    ///
    /// A row already under that identifier is a replay when the selected
    /// environment revision and the fingerprint both match, and a conflict
    /// otherwise. A conflict writes nothing at all: the stored row is returned
    /// exactly as it stands, because the caller needs to see what is really
    /// there rather than a partial overwrite of it.
    ///
    /// # Errors
    ///
    /// Returns [`RepositoryFailure`] when a bounded field is too long or the
    /// database refuses.
    pub fn admit(
        &self,
        request: &AdmissionRequest,
        now_unix_milliseconds: u64,
    ) -> Result<AdmissionOutcome, RepositoryFailure> {
        if let Some(workflow) = request.workflow_correlation_identifier.as_deref() {
            require_within(
                "workflow_correlation_identifier",
                "maximum_workflow_correlation_identifier_bytes",
                workflow,
            )?;
        }
        let transaction = write_transaction(self.database.connection())?;
        let outcome = match self.read_within(
            &transaction,
            &request.author_target_identity_digest,
            &request.operation_identifier,
        )? {
            Some(stored) => Self::classify_stored(request, stored),
            None => {
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
    /// Returns [`RepositoryFailure`] when a stored value does not decode or the
    /// database refuses.
    pub fn read(
        &self,
        author_target_identity_digest: &str,
        operation_identifier: &str,
    ) -> Result<Option<OperationSummary>, RepositoryFailure> {
        let transaction = self.database.connection().unchecked_transaction()?;
        let found =
            self.read_within(&transaction, author_target_identity_digest, operation_identifier)?;
        transaction.commit()?;
        Ok(found)
    }

    /// Reads one operation and its outstanding recovery in one transaction.
    ///
    /// Both halves are read together because a summary that showed a state from
    /// one instant and a recovery fact from another would be a description of
    /// no moment that ever existed.
    fn read_within(
        &self,
        transaction: &rusqlite::Transaction<'_>,
        author_target_identity_digest: &str,
        operation_identifier: &str,
    ) -> Result<Option<OperationSummary>, RepositoryFailure> {
        let recovery =
            self.read_recovery(transaction, author_target_identity_digest, operation_identifier)?;
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
    ///
    /// Every caller of this has already established the row exists - it just
    /// wrote it, or read it a moment ago inside the same transaction. Its
    /// absence would mean the transaction is not seeing its own writes, which
    /// is worth a distinct failure rather than an empty answer.
    fn read_required(
        &self,
        transaction: &rusqlite::Transaction<'_>,
        author_target_identity_digest: &str,
        operation_identifier: &str,
    ) -> Result<OperationSummary, RepositoryFailure> {
        self.read_within(transaction, author_target_identity_digest, operation_identifier)?
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
        &self,
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
    /// The caller states the revision it last saw. A stale one writes nothing
    /// and says so, which is how two writers racing on the same operation
    /// produce one winner rather than a row neither of them described.
    ///
    /// A fold that changes nothing commits nothing: the revision stays where it
    /// is and the stored row is returned. That is what makes recording the same
    /// fact twice one recorded fact rather than two revisions of it.
    ///
    /// # Errors
    ///
    /// Returns [`RepositoryFailure::NoSuchOperation`],
    /// [`RepositoryFailure::RevisionMoved`], [`RepositoryFailure::TooLong`],
    /// [`RepositoryFailure::Lifecycle`] when the domain refuses the fact, or a
    /// database failure.
    pub fn apply(
        &self,
        author_target_identity_digest: &str,
        operation_identifier: &str,
        expected_revision: u64,
        fact: &OperationFact,
        now_unix_milliseconds: u64,
    ) -> Result<OperationSummary, RepositoryFailure> {
        Self::require_bounded(fact)?;
        let transaction = write_transaction(self.database.connection())?;
        let stored =
            self.read_required(&transaction, author_target_identity_digest, operation_identifier)?;
        require_revision(&stored, expected_revision)?;
        let folded = stored.record.fold(fact)?;
        let settled = Self::settlement(&stored, &folded, now_unix_milliseconds);
        if folded.revision != stored.record.revision {
            self.write_folded(&transaction, &stored, &folded, settled)?;
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
    /// An operation settles once. A fold that leaves an already terminal row
    /// terminal keeps the instant it settled at, because the moment work ended
    /// is not something a later write gets to restate.
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

    /// Records where an operation's result went, under compare-and-set.
    ///
    /// The disposition is settled separately from the lifecycle because it
    /// answers a different question. Reaching `Succeeded` says the work
    /// happened; this says where what it produced can be found, and the two are
    /// written by different parts of the daemon at different moments.
    ///
    /// # Errors
    ///
    /// Returns [`RepositoryFailure::NoSuchOperation`],
    /// [`RepositoryFailure::RevisionMoved`], or a database failure.
    pub fn record_result_disposition(
        &self,
        author_target_identity_digest: &str,
        operation_identifier: &str,
        expected_revision: u64,
        disposition: ResultDisposition,
    ) -> Result<OperationSummary, RepositoryFailure> {
        let transaction = write_transaction(self.database.connection())?;
        let stored =
            self.read_required(&transaction, author_target_identity_digest, operation_identifier)?;
        require_revision(&stored, expected_revision)?;
        if stored.result_disposition != Some(disposition) {
            let advanced =
                OperationRecord { revision: stored.record.revision + 1, ..stored.record.clone() };
            let carried =
                OperationSummary { result_disposition: Some(disposition), ..stored.clone() };
            self.write_folded(
                &transaction,
                &carried,
                &advanced,
                stored.settled_at_unix_milliseconds,
            )?;
        }
        let current =
            self.read_required(&transaction, author_target_identity_digest, operation_identifier)?;
        transaction.commit()?;
        Ok(current)
    }

    /// Returns every operation in one partition, in the order it arrived.
    ///
    /// Reopening reconstructs from this: nonterminal rows are the work still to
    /// do, in the order their callers asked for it, and terminal rows come back
    /// too because a client that asks about finished work deserves the answer
    /// rather than a shrug.
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
    /// The receipt is keyed by target and source fingerprint, so an identical
    /// resume request finds its own committed proof no matter what the
    /// operation has done since. That is the point of it: the answer to "did my
    /// resume take effect" cannot be reconstructed from the operation's current
    /// state, because later progress, another recovery cycle, or terminal
    /// settlement all look the same from outside.
    ///
    /// A replay is only a replay when the selected environment revision matches
    /// too. A receipt recorded against a revision this daemon no longer runs is
    /// not proof about the daemon that is running.
    ///
    /// # Errors
    ///
    /// Returns [`RepositoryFailure::ReceiptsExhausted`] when the operation
    /// already holds every receipt the contract allows,
    /// [`RepositoryFailure::NoSuchOperation`], or a database failure.
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
        if let Some(held) =
            Self::receipt_within(&transaction, author_target_identity_digest, source_fingerprint)?
        {
            transaction.commit()?;
            return Ok(ResumeOutcome::Replayed(Box::new(held)));
        }
        Self::require_receipt_capacity(
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
        let written =
            Self::receipt_within(&transaction, author_target_identity_digest, source_fingerprint)?
                .ok_or_else(|| RepositoryFailure::NoSuchOperation {
                    identifier: operation_identifier.to_owned(),
                })?;
        transaction.commit()?;
        Ok(ResumeOutcome::Applied(Box::new(written)))
    }

    /// Refuses a fresh receipt once an operation holds all it may.
    fn require_receipt_capacity(
        transaction: &rusqlite::Transaction<'_>,
        author_target_identity_digest: &str,
        operation_identifier: &str,
    ) -> Result<(), RepositoryFailure> {
        let allowed = DaemonRuntimeContract::embedded()
            .limit("maximum_recovery_resume_receipts_per_operation");
        let held: i64 = transaction.query_row(
            statement("count one operation's recovery-resume receipts"),
            rusqlite::params![author_target_identity_digest, operation_identifier],
            |row| row.get(0),
        )?;
        if u64::try_from(held).unwrap_or(u64::MAX) >= allowed {
            return Err(RepositoryFailure::ReceiptsExhausted { allowed });
        }
        Ok(())
    }

    /// Returns one recovery-resume receipt, or nothing.
    ///
    /// # Errors
    ///
    /// Returns [`RepositoryFailure`] when the database refuses.
    pub fn read_resume_receipt(
        &self,
        author_target_identity_digest: &str,
        source_fingerprint: &str,
    ) -> Result<Option<RecoveryResumeReceipt>, RepositoryFailure> {
        Self::receipt_within(
            self.database.connection(),
            author_target_identity_digest,
            source_fingerprint,
        )
    }

    /// Reads one receipt, inside whatever transaction the caller has open.
    fn receipt_within(
        connection: &rusqlite::Connection,
        author_target_identity_digest: &str,
        source_fingerprint: &str,
    ) -> Result<Option<RecoveryResumeReceipt>, RepositoryFailure> {
        let mut prepared = connection
            .prepare(statement("read one recovery-resume receipt by its source fingerprint"))?;
        let row = prepared
            .query_row(
                rusqlite::params![author_target_identity_digest, source_fingerprint],
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
