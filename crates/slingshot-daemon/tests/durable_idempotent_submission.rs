//! One command, handed over once, however many physical records carry it.
//!
//! Sling delivers at least once, so the honest position is that a submission
//! may leave several physical Sling records behind and must still amount to one
//! logical operation with at most one command effect. That is not achieved by
//! hoping the transport behaves; it is achieved by a durable fence whose
//! condition is checked by the database, and by refusing to derive a
//! replacement identity when a request's fate is unclear.
//!
//! So the crash boundaries are the subject. A daemon that died before the call,
//! during it, and after it must reach three different conclusions from the same
//! lookup, and only one of them permits sending anything again.
//!
//! The other half is order. What this build has is checked before the network
//! is touched, so a build whose contracts moved finds out before it reaches a
//! credential provider or a socket - both of which are observable from outside.

use slingshot_agent_connection::command_submission::{
    Checkpoint, ExpectedArtifactManifest, NonExecution, Submission, SubmissionOutcome,
};
use slingshot_agent_protocol::identity::WireOperationIdentity;
use slingshot_agent_protocol::wire_contract::ExpectedProvenance;
use slingshot_daemon::operation::remote_submission::{
    AmbiguityOutcome, HandoffDisposition, PreflightRefusal, SuppliedIdentity, classify_supplied,
    disposition_of, require_resendable, resolve_ambiguity,
};
use slingshot_domain::agent_identity::AgentEventStoreGeneration;
use slingshot_domain::author_agent_transport_contract::AuthorAgentTransportContract;
use slingshot_domain::command::schema::canonical_contract_digest;
use slingshot_domain::selected_command_contract_identity::SelectedCommandContractIdentity;
use slingshot_storage::agent_job_repository::AgentJobRepository;
use slingshot_storage::database::{OperationDatabase, RequiredSettings};
use slingshot_storage::operation::remote_submission::{
    CheckpointOutcome, ClaimOutcome, FenceFacts, checkpoint, claim, fence_facts,
};

/// Where the vectors this suite is driven from live.
const FIXTURES: &str = "tests/fixtures/durable-idempotent-submission.jsonl";

/// Bytes one page occupies, from the runtime contract.
const PAGE_BYTES: u64 = 4096;

/// Pages the database may reach, from the runtime contract.
const DATABASE_PAGES: u64 = 262_144;

/// Milliseconds a busy connection waits, from the runtime contract.
const BUSY_TIMEOUT: u64 = 5000;

/// The partition every submission here belongs to.
const TARGET: &str = "target-identity-digest-one";

/// The environment revision it was submitted under.
const REVISION: &str = "environment-revision-one";

/// Another environment revision, which old work predates.
const OTHER_REVISION: &str = "environment-revision-two";

/// The subscription carrying its events.
const SUBSCRIPTION: &str = "daemon-subscription-one";

/// The command being submitted.
const COMMAND: &str = "query_paths";

/// The generation it was submitted under.
const GENERATION: u64 = 7;

/// A later generation, after the agent's store was rebuilt.
const LATER_GENERATION: u64 = 8;

/// The local operation key the derivation starts from.
const LOCAL_OPERATION: &str = "local-operation-one";

/// Canonical arguments the submission carries.
const ARGUMENTS: &str = "{\"path\":\"/content/one\"}";

/// Other canonical arguments, which derive a different submission.
const OTHER_ARGUMENTS: &str = "{\"path\":\"/content/two\"}";

/// The lease one worker holds.
const FIRST_FENCE: u64 = 3;

/// A later lease, which takes the claim from the first.
const LATER_FENCE: u64 = 4;

/// The marker the no-return checkpoint is written as.
const STARTED: &str = "execution-started";

/// How long a throttled retry waits.
const RETRY_DELAY: u64 = 5_000;

/// Returns the settings every database here is opened under.
fn settings() -> RequiredSettings {
    RequiredSettings {
        page_bytes: PAGE_BYTES,
        database_pages: DATABASE_PAGES,
        busy_timeout_milliseconds: BUSY_TIMEOUT,
    }
}

/// Returns every vector of `kind`.
fn vectors_of(kind: &str) -> Vec<serde_json::Value> {
    let text = std::fs::read_to_string(FIXTURES).expect("the vectors are readable");
    text.lines()
        .map(|line| serde_json::from_str::<serde_json::Value>(line).expect("one vector a line"))
        .filter(|vector| vector["kind"].as_str() == Some(kind))
        .collect()
}

/// Returns what this build has, for the command being submitted.
fn installed() -> ExpectedProvenance {
    ExpectedProvenance {
        canonical_json_contract_digest: canonical_contract_digest(),
        command_contract: SelectedCommandContractIdentity::installed(COMMAND)
            .expect("the command is published"),
        transport_contract_digest: AuthorAgentTransportContract::embedded_digest(),
    }
}

/// Returns the submission this build derives from `arguments`.
fn derived(expected: &ExpectedProvenance, arguments: &str) -> Submission {
    Submission::build(
        expected,
        WireOperationIdentity::of(
            TARGET,
            REVISION,
            LOCAL_OPERATION,
            AgentEventStoreGeneration::of(GENERATION),
        ),
        SUBSCRIPTION,
        arguments,
        ExpectedArtifactManifest::empty(),
    )
    .expect("these arguments fit one submission")
}

/// Returns the submission every vector is about.
fn submission() -> Submission {
    derived(&installed(), ARGUMENTS)
}

/// Returns a fence that has claimed nothing.
fn unclaimed() -> FenceFacts {
    FenceFacts { execution_checkpoint: None, outbox_attempts: 0, worker_fence: None }
}

/// Returns a fence whose work has passed the point of no return.
fn started() -> FenceFacts {
    FenceFacts {
        execution_checkpoint: Some(STARTED.to_owned()),
        outbox_attempts: 1,
        worker_fence: Some(FIRST_FENCE),
    }
}

/// Returns the outcome `spelling` names.
fn outcome_named(spelling: &str) -> SubmissionOutcome {
    match spelling {
        "accepted" => SubmissionOutcome::Accepted {
            physical_sling_job_identifiers: vec!["sling-job-alpha".to_owned()],
            remaining_retention_milliseconds: RETRY_DELAY,
        },
        "duplicate" => SubmissionOutcome::Duplicate {
            physical_sling_job_identifiers: vec!["sling-job-alpha".to_owned()],
            remaining_retention_milliseconds: RETRY_DELAY,
        },
        "authoritative-non-execution" => {
            SubmissionOutcome::AuthoritativeNonExecution { non_execution: NonExecution::Semantic }
        }
        "confirmed-not-executed" => {
            SubmissionOutcome::ConfirmedNotExecuted { checkpoint: Checkpoint::NameResolution }
        }
        "recovery-window-expired" => SubmissionOutcome::RecoveryWindowExpired,
        "conflict" => SubmissionOutcome::Conflict,
        "retry-after" => SubmissionOutcome::RetryAfter { milliseconds: RETRY_DELAY },
        "submission-unknown" => SubmissionOutcome::SubmissionUnknown {
            cause: slingshot_agent_connection::command_submission::UnknownCause::Framing,
        },
        other => panic!("{other} is an outcome this suite does not name"),
    }
}

/// Returns how one disposition is spelled in the vectors.
fn disposition_spelling(disposition: &HandoffDisposition) -> &'static str {
    match disposition {
        HandoffDisposition::Accepted => "accepted",
        HandoffDisposition::Duplicate => "duplicate",
        HandoffDisposition::NotExecuted => "not-executed",
        HandoffDisposition::RecoveryWindowExpired => "recovery-window-expired",
        HandoffDisposition::Conflict => "conflict",
        HandoffDisposition::RetryAfter { .. } => "retry-after",
        HandoffDisposition::Unknown => "unknown",
    }
}

#[test]
fn a_supplied_target_and_revision_mean_exactly_what_the_vectors_say() {
    for vector in vectors_of("supplied") {
        let name = vector["name"].as_str().expect("a name");
        let classification = classify_supplied(
            TARGET,
            REVISION,
            vector["target"].as_str().expect("a target"),
            vector["revision"].as_str().expect("a revision"),
        );
        let spelling = match classification {
            SuppliedIdentity::Compatible => "compatible",
            SuppliedIdentity::RevisionChanged => "revision-changed",
            SuppliedIdentity::TargetDisjoint => "target-disjoint",
        };
        assert_eq!(
            spelling,
            vector["classification"].as_str().expect("a classification"),
            "{name}: work against another author is not old work at all"
        );
    }
}

#[test]
fn a_build_whose_derivation_moved_refuses_before_it_reaches_anything_observable() {
    let persisted = submission();
    let rebuilt = derived(&installed(), OTHER_ARGUMENTS);
    assert_eq!(
        require_resendable(&persisted, &rebuilt, TARGET, REVISION, GENERATION, &unclaimed()),
        Err(PreflightRefusal::DerivationDrifted),
        "a socket or a credential provider reached first would be observable from outside"
    );
    let mut contract_moved = installed();
    contract_moved.transport_contract_digest = "another-transport-digest".to_owned();
    assert_eq!(
        require_resendable(
            &persisted,
            &derived(&contract_moved, ARGUMENTS),
            TARGET,
            REVISION,
            GENERATION,
            &unclaimed()
        ),
        Err(PreflightRefusal::DerivationDrifted),
        "contract-only drift changes the submitted digest and nothing else needs to"
    );
    assert_eq!(
        persisted.canonical_arguments, ARGUMENTS,
        "a refusal preserves the bytes, so a build that agrees finds them as they were"
    );
}

#[test]
fn an_equal_target_and_revision_stay_resendable_and_a_changed_one_does_not() {
    let persisted = submission();
    let rebuilt = submission();
    require_resendable(&persisted, &rebuilt, TARGET, REVISION, GENERATION, &unclaimed())
        .expect("a genuine same-principal refresh changes none of these values");
    assert_eq!(
        require_resendable(&persisted, &rebuilt, TARGET, OTHER_REVISION, GENERATION, &unclaimed()),
        Err(PreflightRefusal::RevisionChanged)
    );
    assert_eq!(
        require_resendable(
            &persisted,
            &rebuilt,
            "target-identity-digest-two",
            REVISION,
            GENERATION,
            &unclaimed()
        ),
        Err(PreflightRefusal::TargetDisjoint)
    );
    assert_eq!(
        require_resendable(&persisted, &rebuilt, TARGET, REVISION, LATER_GENERATION, &unclaimed()),
        Err(PreflightRefusal::GenerationChanged {
            current: LATER_GENERATION,
            persisted: GENERATION
        }),
        "a rebuilt store means the persisted names name nothing, so no replacement is derived"
    );
}

#[test]
fn nothing_after_the_checkpoint_authorizes_another_send() {
    let persisted = submission();
    assert_eq!(
        require_resendable(&persisted, &submission(), TARGET, REVISION, GENERATION, &started()),
        Err(PreflightRefusal::AlreadyStarted),
        "a lease that expired after the work started is not a licence to start it again"
    );
    assert!(!started().permits_another_effect());
    assert!(unclaimed().permits_another_effect());
}

#[test]
fn every_crash_boundary_reaches_the_conclusion_its_vector_states() {
    for vector in vectors_of("crash") {
        let name = vector["name"].as_str().expect("a name");
        let records: Vec<String> = vector["physical_records"]
            .as_array()
            .expect("a list")
            .iter()
            .map(|value| value.as_str().expect("an identifier").to_owned())
            .collect();
        let fence = if vector["started"].as_bool().expect("an expectation") {
            started()
        } else {
            unclaimed()
        };
        let outcome = resolve_ambiguity(&records, &fence);
        let spelling = match &outcome {
            AmbiguityOutcome::Recorded { .. } => "recorded",
            AmbiguityOutcome::MayAttemptAgain => "may-attempt-again",
            AmbiguityOutcome::FailClosed => "fail-closed",
        };
        assert_eq!(spelling, vector["resolution"].as_str().expect("a resolution"), "{name}");
        if let AmbiguityOutcome::Recorded { physical_sling_job_identifiers } = outcome {
            let mut sorted = physical_sling_job_identifiers.clone();
            sorted.sort();
            sorted.dedup();
            assert_eq!(
                physical_sling_job_identifiers, sorted,
                "{name}: several physical records for one submission are recorded as a set"
            );
        }
    }
}

#[test]
fn every_outcome_maps_to_one_disposition_and_only_two_permit_another_send() {
    for vector in vectors_of("disposition") {
        let name = vector["name"].as_str().expect("a name");
        let disposition = disposition_of(&outcome_named(vector["outcome"].as_str().expect("one")));
        assert_eq!(
            disposition_spelling(&disposition),
            vector["disposition"].as_str().expect("a disposition"),
            "{name}"
        );
        assert_eq!(
            disposition.permits_another_send(),
            vector["may-send"].as_bool().expect("an expectation"),
            "{name}: resending an unknown outcome is how one command becomes two"
        );
        assert_eq!(
            disposition.requires_lookup(),
            vector["needs-lookup"].as_bool().expect("an expectation"),
            "{name}"
        );
    }
}

#[test]
fn two_workers_racing_produce_at_most_one_effect_attempt() {
    let root = tempfile::tempdir().expect("a temporary directory");
    let path = root.path().join("operations.sqlite3");
    let repository =
        AgentJobRepository::new(OperationDatabase::open(&path, settings()).expect("opened"));
    let identifier = seed(&repository);
    let database = repository.database();

    assert_eq!(
        claim(database, TARGET, &identifier, FIRST_FENCE).expect("it claims"),
        ClaimOutcome::Claimed
    );
    assert_eq!(
        claim(database, TARGET, &identifier, FIRST_FENCE).expect("it answers"),
        ClaimOutcome::Fenced,
        "an equal fence takes nothing, so two workers on one lease do not both hold it"
    );
    assert_eq!(
        claim(database, TARGET, &identifier, LATER_FENCE).expect("it claims"),
        ClaimOutcome::Claimed,
        "a later lease takes over, which is how a replacement node picks the work up"
    );
    assert_eq!(
        checkpoint(database, TARGET, &identifier, FIRST_FENCE, STARTED).expect("it answers"),
        CheckpointOutcome::Refused,
        "a stale fence cannot record the checkpoint"
    );
    assert_eq!(
        checkpoint(database, TARGET, &identifier, LATER_FENCE, STARTED).expect("it records"),
        CheckpointOutcome::Recorded
    );
    assert_eq!(
        checkpoint(database, TARGET, &identifier, LATER_FENCE, STARTED).expect("it answers"),
        CheckpointOutcome::Refused,
        "and it records once, whatever asks again"
    );
    let facts = fence_facts(database, TARGET, &identifier);
    let facts = facts.expect("reads").expect("it is there");
    assert_eq!(facts.outbox_attempts, 1, "an attempt is counted when an effect may exist");
    assert!(facts.has_started() && !facts.permits_another_effect());
}

#[test]
fn a_lease_that_expires_after_the_start_takes_nothing_back() {
    let root = tempfile::tempdir().expect("a temporary directory");
    let path = root.path().join("operations.sqlite3");
    let repository =
        AgentJobRepository::new(OperationDatabase::open(&path, settings()).expect("opened"));
    let identifier = seed(&repository);
    let database = repository.database();
    claim(database, TARGET, &identifier, FIRST_FENCE).expect("it claims");
    checkpoint(database, TARGET, &identifier, FIRST_FENCE, STARTED).expect("it records");
    assert_eq!(
        claim(database, TARGET, &identifier, LATER_FENCE).expect("it answers"),
        ClaimOutcome::AlreadyStarted,
        "a requeue, a retry, a restart, and a replacement node are all the same story"
    );
    drop(repository);

    let reopened =
        AgentJobRepository::new(OperationDatabase::open(&path, settings()).expect("reopened"));
    let facts = fence_facts(reopened.database(), TARGET, &identifier);
    let facts = facts.expect("reads").expect("it is there");
    assert!(facts.has_started(), "the no-return checkpoint survives the restart it exists for");
    assert_eq!(facts.outbox_attempts, 1);
}

/// Seeds one submission and returns what it is called at the agent.
fn seed(repository: &AgentJobRepository) -> String {
    use slingshot_domain::remote_job::{JobEventSequence, RemoteJobObservation};
    use slingshot_storage::agent_job_repository::{
        AgentSubmission, SubmissionContracts, SubmissionIdentity,
    };
    let derived = submission();
    let identity = SubmissionIdentity {
        agent_event_store_generation: GENERATION,
        agent_operation_identifier: derived.operation.agent_operation_identifier.clone(),
        author_target_identity_digest: TARGET.to_owned(),
        daemon_subscription_identifier: SUBSCRIPTION.to_owned(),
        operation_identifier: LOCAL_OPERATION.to_owned(),
        selected_environment_revision: REVISION.to_owned(),
    };
    let contract = &installed().command_contract;
    repository
        .submit(&AgentSubmission {
            canonical_submission: ARGUMENTS.to_owned(),
            contracts: SubmissionContracts {
                argument_schema_digest: contract.argument_schema_digest.clone(),
                author_agent_transport_contract_digest:
                    AuthorAgentTransportContract::embedded_digest(),
                command_canonical_json_contract_digest: canonical_contract_digest(),
                command_contract_limits_digest: contract.command_contract_limits_digest.clone(),
                command_semantic_contract_version: contract
                    .command_semantic_contract_version
                    .clone(),
                command_wire_name: contract.command_wire_name.clone(),
                result_schema_digest: contract.result_schema_digest.clone(),
                submitted_command_digest: derived.submitted_command_digest.clone(),
            },
            identity: identity.clone(),
            observation: RemoteJobObservation::accepted(),
            recorded_at_unix_milliseconds: 0,
            remaining_retention_milliseconds: RETRY_DELAY,
            request_start_unix_milliseconds: 0,
            snapshot_watermark: JobEventSequence::of(0),
            terminal_disposition: None,
        })
        .expect("admitted");
    identity.agent_operation_identifier
}
