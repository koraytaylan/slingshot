//! Durable facts about work somebody else is running.
//!
//! Two things are proved. The first is that everything a resubmission would
//! have to derive is written down and comes back byte for byte after a reopen:
//! the contracts, the arguments, the revision, the generation, the digest. A
//! restart that could not re-derive those would either refuse to resume
//! anything or resume under a name it had guessed at.
//!
//! The second is that idempotency lives in the statements rather than in
//! anything a caller remembers. A cursor advances only to a later position, a
//! fold applies only to the sequence it expected, an ending lands only on a row
//! that has not ended, and a physical job records the same name twice without
//! complaint. So every replay here is exercised twice and asked to change
//! nothing the second time.
//!
//! Throughout, the subscription ledger is kept honest about the case that makes
//! it awkward: a stream carries events about work this daemon does not hold,
//! and the position must move anyway.

use slingshot_domain::command_fingerprint::DIGEST_CHARACTERS;
use slingshot_domain::installation::IDENTIFIER_CHARACTERS;
use slingshot_domain::persistent_capacity::PersistentCapacityPolicy;
use slingshot_domain::remote_job::{AgentJobState, JobEventSequence, RemoteJobObservation};
use slingshot_storage::agent_job_repository::{
    AgentCapacityBounds, AgentJobRepository, AgentRepositoryFailure, AgentSubmission,
    BYTES_PER_EVENT, PHYSICAL_JOBS_PER_SUBMISSION, SubmissionContracts, SubmissionIdentity,
    SubmissionOutcome,
};
use slingshot_storage::agent_subscription_ledger::{
    AgentSubscriptionLedger, EventFact, LedgerOutcome,
};
use slingshot_storage::database::{OperationDatabase, RequiredSettings};
use slingshot_storage::maintenance;

#[path = "agent_job_repository/activation_guards.rs"]
mod activation_guards;

/// Where the vectors this suite is driven from live.
const FIXTURES: &str = "tests/fixtures/agent-job-storage";

/// Bytes one page occupies, from the runtime contract.
const PAGE_BYTES: u64 = 4096;

/// Pages the database may reach, from the runtime contract.
const DATABASE_PAGES: u64 = 262_144;

/// Milliseconds a busy connection waits, from the runtime contract.
const BUSY_TIMEOUT: u64 = 5000;

/// The partition every fact here belongs to.
const TARGET: &str = "target-identity-digest-one";

/// Another partition, to prove nothing reaches across.
const ANOTHER_TARGET: &str = "target-identity-digest-two";

/// The subscription carrying these events.
const SUBSCRIPTION: &str = "daemon-subscription-one";

/// The generation these facts belong to.
const GENERATION: u64 = 7;

/// A later generation, after the agent's store was rebuilt.
const LATER_GENERATION: u64 = 8;

/// One instant, for the facts that need one.
const NOW: u64 = 1_700_000_000_000;

/// How long the agent promises to keep one submission's results.
const RETENTION: u64 = 120_000;

/// A sequence a fold advances to.
const SECOND_SEQUENCE: u64 = 2;

/// A later sequence, for a watermark.
const FIFTH_SEQUENCE: u64 = 5;

/// How far along a job that has reported once says it is.
const SOME_PROGRESS: u64 = 40;

/// How many times one disagreement is reported.
const REPEATED_REPORTS: u64 = 3;

/// How many positions a compaction fixture records before compacting.
const RECORDED_POSITIONS: u64 = 4;

/// How many positions a compaction leaves behind.
const RETAINED_POSITIONS: u64 = 2;

/// Returns the settings every database here is opened under.
fn settings() -> RequiredSettings {
    RequiredSettings {
        page_bytes: PAGE_BYTES,
        database_pages: DATABASE_PAGES,
        busy_timeout_milliseconds: BUSY_TIMEOUT,
    }
}

/// Returns one migrated database in memory.
fn migrated() -> OperationDatabase {
    OperationDatabase::open_in_memory(settings()).expect("a migrated database")
}

/// Returns a repository over a fresh in-memory database.
fn repository() -> AgentJobRepository {
    AgentJobRepository::new(migrated())
}

/// Returns a ledger over a fresh in-memory database with one subscription open.
fn ledger() -> AgentSubscriptionLedger {
    let ledger = AgentSubscriptionLedger::new(migrated());
    ledger.open_subscription(TARGET, SUBSCRIPTION, GENERATION, NOW).expect("one subscription");
    ledger
}

/// Returns every vector one fixture holds.
fn vectors(name: &str) -> Vec<serde_json::Value> {
    let path = format!("{FIXTURES}/{name}");
    let text = std::fs::read_to_string(&path).unwrap_or_else(|_| panic!("{path} is readable"));
    text.lines().map(|line| serde_json::from_str(line).expect("each line is one vector")).collect()
}

/// Returns the identity one submission has in `target`.
fn identity_in(target: &str, named: &str) -> SubmissionIdentity {
    SubmissionIdentity {
        agent_event_store_generation: GENERATION,
        agent_operation_identifier: format!("agent-operation-{named}"),
        author_target_identity_digest: target.to_owned(),
        daemon_subscription_identifier: SUBSCRIPTION.to_owned(),
        operation_identifier: format!("local-operation-{named}"),
        selected_environment_revision: "environment-revision-one".to_owned(),
    }
}

/// Returns the contracts one submission was made under.
fn contracts(named: &str) -> SubmissionContracts {
    SubmissionContracts {
        argument_schema_digest: "argument-schema-digest".to_owned(),
        author_agent_transport_contract_digest: "transport-contract-digest".to_owned(),
        command_canonical_json_contract_digest: "canonical-contract-digest".to_owned(),
        command_contract_limits_digest: "limits-digest".to_owned(),
        command_semantic_contract_version: "1".to_owned(),
        command_wire_name: "query_paths".to_owned(),
        result_schema_digest: "result-schema-digest".to_owned(),
        submitted_command_digest: format!("submitted-digest-{named}"),
    }
}

/// Returns one submission against `target`.
fn submission_in(target: &str, named: &str) -> AgentSubmission {
    AgentSubmission {
        canonical_submission: format!("{{\"path\":\"/content/{named}\"}}"),
        contracts: contracts(named),
        identity: identity_in(target, named),
        observation: RemoteJobObservation::accepted(),
        recorded_at_unix_milliseconds: NOW,
        remaining_retention_milliseconds: RETENTION,
        request_start_unix_milliseconds: NOW,
        snapshot_watermark: JobEventSequence::of(0),
        terminal_disposition: None,
    }
}

/// Returns one submission against the partition everything else uses.
fn submission(named: &str) -> AgentSubmission {
    submission_in(TARGET, named)
}

/// Returns the observation one running job with `attempt` and `progress` has.
fn running(sequence: u64, attempt: u64, progress: u64) -> RemoteJobObservation {
    RemoteJobObservation {
        applied_sequence: JobEventSequence::of(sequence),
        attempt,
        progress,
        state: AgentJobState::Running,
    }
}

/// Returns one event fact at `cursor`.
fn fact(cursor: &str, digest: &str) -> EventFact {
    fact_in(GENERATION, cursor, digest)
}

/// Returns one event fact in the specified event-store generation.
fn fact_in(generation: u64, cursor: &str, digest: &str) -> EventFact {
    EventFact {
        agent_event_store_generation: generation,
        agent_operation_identifier: None,
        canonical_digest: digest.to_owned(),
        cursor: cursor.to_owned(),
        event_bytes: PAGE_BYTES,
        job_sequence: None,
    }
}

/// Returns the outcome `spelling` names.
fn outcome_named(spelling: &str) -> LedgerOutcome {
    match spelling {
        "advanced" => LedgerOutcome::Advanced,
        "exact-replay" => LedgerOutcome::ExactReplay,
        "stale-cursor-only" => LedgerOutcome::StaleCursorOnly,
        "integrity-conflict" => LedgerOutcome::IntegrityConflict,
        other => panic!("{other} is an outcome this suite does not name"),
    }
}

/// Returns the state `spelling` names.
fn state_named(spelling: &str) -> AgentJobState {
    match spelling {
        "queued" => AgentJobState::Queued,
        "running" => AgentJobState::Running,
        other => panic!("{other} is a state these vectors do not use"),
    }
}

/// Admits the independently retained local owner needed by a product reset.
fn admit_reset_owner(path: &std::path::Path, child: &AgentSubmission) {
    use slingshot_domain::{
        command_fingerprint::{CommandFingerprint, FingerprintInput},
        installation::InstallationIdentifier,
    };
    use slingshot_storage::operation_repository::{AdmissionRequest, OperationRepository};
    let identity = &child.identity;
    OperationRepository::new(OperationDatabase::open(path, settings()).unwrap())
        .admit(
            &AdmissionRequest {
                author_target_identity: "opaque-target".into(),
                author_target_identity_digest: identity.author_target_identity_digest.clone(),
                caller_identity: None,
                canonical_command: "{}".into(),
                command_fingerprint: CommandFingerprint::derive(&FingerprintInput {
                    author_target_identity_digest: identity.author_target_identity_digest.clone(),
                    canonical_command: "{}".into(),
                    command_wire_name: "query_paths".into(),
                    command_semantic_contract_version: "1".into(),
                    selected_environment_revision: identity.selected_environment_revision.clone(),
                })
                .unwrap(),
                command_wire_name: "query_paths".into(),
                daemon_runtime_contract_digest: "c".repeat(DIGEST_CHARACTERS),
                installation_identifier: InstallationIdentifier::parse(
                    &"a1".repeat(IDENTIFIER_CHARACTERS / "a1".len()),
                )
                .unwrap(),
                operation_identifier: identity.operation_identifier.clone(),
                selected_environment_revision: identity.selected_environment_revision.clone(),
                workflow_correlation_identifier: None,
            },
            NOW,
        )
        .unwrap();
}

#[path = "agent_job_repository/snapshots.rs"]
mod snapshots;
#[path = "agent_job_repository/associations.rs"]
mod associations;
#[path = "agent_job_repository/recovery.rs"]
mod recovery;
#[path = "agent_job_repository/events.rs"]
mod events;
