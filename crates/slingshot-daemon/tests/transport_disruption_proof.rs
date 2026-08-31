//! Every way one handoff can be cut, and what survives each of them.
//!
//! Sling delivers at least once, so duplicate physical records are ordinary and
//! the property worth proving is narrower and harder: one logical operation,
//! and at most one fenced attempt at a command effect, however the transport
//! behaves. The matrix cuts the exchange at each place it can be cut and asks
//! the same three questions of every row - what the daemon concluded, whether
//! it may send anything again, and whether a second effect became possible.
//!
//! # Two runs, one trace
//!
//! Each row runs twice against a fresh target root with the same injected seed,
//! and the two traces must be byte-identical. A conclusion that depended on a
//! real clock, on ambient randomness, or on what the previous row left behind
//! would show up here and nowhere else, which is why the comparison is on the
//! whole trace rather than on the outcome.
//!
//! # Unknown is not a licence
//!
//! After a request byte may have reached the author, no cut settles anything.
//! Each of those rows stays unknown and permits no further send: its resolution
//! is a lookup under the names already derived, and resending is precisely how
//! one command becomes two.

use slingshot_agent_connection::author_hypertext_transfer_protocol_policy::ResponseHead;
use slingshot_agent_connection::command_submission::{
    ANSWERED_STATUSES, Checkpoint, Exchange, ExpectedArtifactManifest, Submission,
    SubmissionAcknowledgement, SubmissionOutcome,
};
use slingshot_agent_protocol::identity::WireOperationIdentity;
use slingshot_agent_protocol::wire_contract::ExpectedProvenance;
use slingshot_daemon::operation::remote_submission::{
    AmbiguityOutcome, HandoffDisposition, disposition_of, resolve_ambiguity,
};
use slingshot_domain::agent_identity::AgentEventStoreGeneration;
use slingshot_domain::author_agent_transport_contract::AuthorAgentTransportContract;
use slingshot_domain::command::schema::canonical_contract_digest;
use slingshot_domain::remote_job::{JobEventSequence, RemoteJobObservation};
use slingshot_domain::selected_command_contract_identity::SelectedCommandContractIdentity;
use slingshot_storage::agent_job_repository::{
    AgentJobRepository, AgentSubmission, SubmissionContracts, SubmissionIdentity,
};
use slingshot_storage::database::{OperationDatabase, RequiredSettings};
use slingshot_storage::operation::remote_submission::{
    CheckpointOutcome, ClaimOutcome, checkpoint, claim, fence_facts,
};
use slingshot_test_support::fake_author::script::{
    PUBLISHER_PREFIXES, Script, ScriptedExchange, ScriptedResponse,
};
use slingshot_test_support::fake_author::server::{
    CredentialPolicy, FakeAuthor, IncomingRequest, OK_STATUS,
};

/// Where the matrix this suite runs lives.
const FIXTURES: &str = "tests/fixtures/transport-disruption-proof.jsonl";

/// Bytes one page occupies, from the runtime contract.
const PAGE_BYTES: u64 = 4096;

/// Pages the database may reach, from the runtime contract.
const DATABASE_PAGES: u64 = 262_144;

/// Milliseconds a busy connection waits, from the runtime contract.
const BUSY_TIMEOUT: u64 = 5000;

/// The partition every row runs in.
const TARGET: &str = "target-identity-digest-one";

/// The environment revision it runs under.
const REVISION: &str = "environment-revision-one";

/// The subscription carrying its events.
const SUBSCRIPTION: &str = "daemon-subscription-one";

/// The generation it runs under.
const GENERATION: u64 = 7;

/// The command every row submits.
const COMMAND: &str = "query_paths";

/// The local operation key the derivation starts from.
const LOCAL_OPERATION: &str = "local-operation-one";

/// Canonical arguments the submission carries.
const ARGUMENTS: &str = "{\"path\":\"/content/one\"}";

/// The credential every row presents, whose value nothing may record.
const CREDENTIAL: &str = "Basic dGhlLXNlY3JldC12YWx1ZQ==";

/// The submission route the author serves.
const SUBMIT_ROUTE: &str = "/bin/slingshot/agent/submit";

/// A status this build never validated.
const UNVALIDATED_STATUS: u16 = 418;

/// A status that settles nothing.
const THROTTLED_STATUS: u16 = 503;

/// The status that means this identifier already means something else.
const CONFLICT_STATUS: u16 = 409;

/// The lease the first worker holds.
const FIRST_FENCE: u64 = 3;

/// A later lease, which a replacement node would take.
const LATER_FENCE: u64 = 4;

/// The marker the no-return checkpoint is written as.
const STARTED: &str = "execution-started";

/// How long the agent promises to keep the results.
const RETENTION: u64 = 120_000;

/// Returns the settings every database here is opened under.
fn settings() -> RequiredSettings {
    RequiredSettings {
        page_bytes: PAGE_BYTES,
        database_pages: DATABASE_PAGES,
        busy_timeout_milliseconds: BUSY_TIMEOUT,
    }
}

/// Returns every row of the matrix.
fn rows() -> Vec<serde_json::Value> {
    let text = std::fs::read_to_string(FIXTURES).expect("the matrix is readable");
    text.lines().map(|line| serde_json::from_str(line).expect("each line is one row")).collect()
}

/// Returns what this build has, for the command every row submits.
fn installed() -> ExpectedProvenance {
    ExpectedProvenance {
        canonical_json_contract_digest: canonical_contract_digest(),
        command_contract: SelectedCommandContractIdentity::installed(COMMAND)
            .expect("the command is published"),
        transport_contract_digest: AuthorAgentTransportContract::embedded_digest(),
    }
}

/// Returns the submission this build derives.
fn submission() -> Submission {
    Submission::build(
        &installed(),
        WireOperationIdentity::of(
            TARGET,
            REVISION,
            LOCAL_OPERATION,
            AgentEventStoreGeneration::of(GENERATION),
        ),
        SUBSCRIPTION,
        ARGUMENTS,
        ExpectedArtifactManifest::empty(),
    )
    .expect("these arguments fit one submission")
}

/// Returns a head with nothing wrong with it.
fn clean_head() -> ResponseHead {
    ResponseHead {
        alternative_service_offered: false,
        content_coding: None,
        informational: false,
        location: None,
        protocol_version: "HTTP/1.1".to_owned(),
        trailers_declared: false,
    }
}

/// Returns the exchange one named cut produces.
fn exchange_of(cut: &str, submission: &Submission) -> Exchange {
    let mut exchange = Exchange {
        acknowledgement: Some(SubmissionAcknowledgement {
            agent_event_store_generation: GENERATION,
            agent_operation_identifier: submission.operation.agent_operation_identifier.clone(),
            author_target_identity_digest: TARGET.to_owned(),
            already_accepted: false,
            daemon_subscription_identifier: SUBSCRIPTION.to_owned(),
            granted_retention_milliseconds: RETENTION,
            non_execution: None,
            physical_sling_job_identifiers: vec!["sling-job-alpha".to_owned()],
            retired: false,
            submitted_command_digest: submission.submitted_command_digest.clone(),
        }),
        body_bytes: 0,
        elapsed_milliseconds: 1,
        framing_ambiguous: false,
        head: clean_head(),
        media_type: "application/json".to_owned(),
        retry_after_milliseconds: None,
        status: ANSWERED_STATUSES[0],
        trailer_section_present: false,
        trailing_bytes: false,
        unknown_fields: false,
    };
    match cut {
        "none" => {}
        "informational" => exchange.head.informational = true,
        "trailer-declared" => exchange.head.trailers_declared = true,
        "framing" => exchange.framing_ambiguous = true,
        "trailing-bytes" => exchange.trailing_bytes = true,
        "unvalidated-status" => exchange.status = UNVALIDATED_STATUS,
        "throttled" => exchange.status = THROTTLED_STATUS,
        "conflict" => exchange.status = CONFLICT_STATUS,
        "pre-byte" | "post-byte" => {}
        other => panic!("{other} is a cut this matrix does not stage"),
    }
    exchange
}

/// Returns what one cut concludes.
fn outcome_of(cut: &str, submission: &Submission) -> SubmissionOutcome {
    match cut {
        "pre-byte" => Submission::transport_failure(Checkpoint::NameResolution),
        "post-byte" => Submission::transport_failure(Checkpoint::RequestHead),
        _ => submission.interpret(&exchange_of(cut, submission)),
    }
}

/// Returns how one disposition is spelled in the matrix.
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

/// Returns everything one row does, in order, as text a comparison can read.
///
/// A trace rather than an outcome, because the properties at stake are about
/// what happened on the way: which fence was claimed, how many physical records
/// were recorded, and whether a second effect ever became possible.
fn trace(row: &serde_json::Value) -> Vec<String> {
    let cut = row["cut"].as_str().expect("a cut");
    let records = row["physical_records"].as_u64().expect("a count");
    let seed = row["seed"].as_u64().expect("a seed");
    let submission = submission();
    let database = OperationDatabase::open_in_memory(settings()).expect("a fresh target root");
    let repository = AgentJobRepository::new(database);
    let identity = seed_submission(&repository, &submission);
    let mut written = vec![format!("derived {}", submission.submitted_command_digest)];
    written.push(format!("operation {}", identity.agent_operation_identifier));

    for position in 0..records {
        repository
            .record_physical_job(&identity, &format!("sling-job-{position:04}"), seed)
            .expect("a physical record is ordinary");
    }
    let held =
        repository.physical_jobs(TARGET, &identity.agent_operation_identifier).expect("reads");
    written.push(format!("physical {}", held.join(",")));

    let outcome = outcome_of(cut, &submission);
    let disposition = disposition_of(&outcome);
    written.push(format!("disposition {}", disposition_spelling(&disposition)));
    written.push(format!("may-send {}", disposition.permits_another_send()));
    written.push(format!("needs-lookup {}", disposition.requires_lookup()));

    let store = repository.database();
    written.push(format!(
        "claim {:?}",
        claim(store, TARGET, &identity.agent_operation_identifier, FIRST_FENCE).expect("it claims")
    ));
    written.push(format!(
        "checkpoint {:?}",
        checkpoint(store, TARGET, &identity.agent_operation_identifier, FIRST_FENCE, STARTED)
            .expect("it answers")
    ));
    written.push(format!(
        "takeover {:?}",
        claim(store, TARGET, &identity.agent_operation_identifier, LATER_FENCE)
            .expect("it answers")
    ));
    let facts = fence_facts(store, TARGET, &identity.agent_operation_identifier)
        .expect("reads")
        .expect("it is there");
    written.push(format!("attempts {}", facts.outbox_attempts));
    written.push(format!("further-effect {}", facts.permits_another_effect()));

    let ambiguity = resolve_ambiguity(&held, &facts);
    written.push(match ambiguity {
        AmbiguityOutcome::Recorded { physical_sling_job_identifiers } => {
            format!("ambiguity recorded {}", physical_sling_job_identifiers.join(","))
        }
        AmbiguityOutcome::MayAttemptAgain => "ambiguity may-attempt-again".to_owned(),
        AmbiguityOutcome::FailClosed => "ambiguity fail-closed".to_owned(),
    });
    written
}

/// Seeds one submission and returns the identity it was written under.
fn seed_submission(repository: &AgentJobRepository, submission: &Submission) -> SubmissionIdentity {
    let contract = &installed().command_contract;
    let identity = SubmissionIdentity {
        agent_event_store_generation: GENERATION,
        agent_operation_identifier: submission.operation.agent_operation_identifier.clone(),
        author_target_identity_digest: TARGET.to_owned(),
        daemon_subscription_identifier: SUBSCRIPTION.to_owned(),
        operation_identifier: LOCAL_OPERATION.to_owned(),
        selected_environment_revision: REVISION.to_owned(),
    };
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
                submitted_command_digest: submission.submitted_command_digest.clone(),
            },
            identity: identity.clone(),
            observation: RemoteJobObservation::accepted(),
            recorded_at_unix_milliseconds: 0,
            remaining_retention_milliseconds: RETENTION,
            request_start_unix_milliseconds: 0,
            snapshot_watermark: JobEventSequence::of(0),
            terminal_disposition: None,
        })
        .expect("admitted");
    identity
}

#[test]
fn every_row_reaches_the_conclusion_it_states() {
    for row in rows() {
        let name = row["name"].as_str().expect("a name");
        let written = trace(&row);
        assert!(
            written
                .contains(&format!("disposition {}", row["outcome"].as_str().expect("an outcome"))),
            "{name}: {written:?}"
        );
        assert!(
            written.contains(&format!(
                "may-send {}",
                row["may_send_again"].as_bool().expect("an expectation")
            )),
            "{name}: only a bounded wait and a proof of nonexecution permit another send"
        );
    }
}

#[test]
fn running_a_row_twice_produces_the_same_trace_byte_for_byte() {
    for row in rows() {
        let name = row["name"].as_str().expect("a name");
        assert_eq!(
            trace(&row),
            trace(&row),
            "{name}: a conclusion that depended on a real clock or on the previous row would \
             show up here and nowhere else"
        );
    }
}

#[test]
fn duplicate_physical_records_never_become_a_second_effect() {
    for row in rows() {
        let name = row["name"].as_str().expect("a name");
        let written = trace(&row);
        assert!(
            written.contains(&"attempts 1".to_owned()),
            "{name}: however many physical records carried it, one effect was attempted"
        );
        assert!(
            written.contains(&"further-effect false".to_owned()),
            "{name}: and nothing after the checkpoint authorizes another"
        );
        assert!(
            written.contains(&format!("takeover {:?}", ClaimOutcome::AlreadyStarted)),
            "{name}: a replacement node takes nothing back after the start"
        );
        assert!(
            written.contains(&format!("checkpoint {:?}", CheckpointOutcome::Recorded)),
            "{name}: and the checkpoint is recorded exactly once"
        );
    }
}

#[test]
fn every_cut_after_the_request_bytes_stays_unknown_and_sends_nothing_again() {
    let post_byte = [
        "post-byte",
        "informational",
        "trailer-declared",
        "framing",
        "trailing-bytes",
        "unvalidated-status",
    ];
    let submission = submission();
    for cut in post_byte {
        let disposition = disposition_of(&outcome_of(cut, &submission));
        assert_eq!(disposition, HandoffDisposition::Unknown, "{cut}");
        assert!(
            !disposition.permits_another_send(),
            "{cut}: resending an unknown outcome is how one command becomes two"
        );
        assert!(disposition.requires_lookup(), "{cut}: its resolution is a lookup");
    }
    let before = disposition_of(&outcome_of("pre-byte", &submission));
    assert_eq!(before, HandoffDisposition::NotExecuted);
    assert!(
        before.permits_another_send(),
        "and only a proof that no byte was written may be sent again"
    );
}

#[test]
fn every_row_reconciles_by_the_tuple_it_already_has() {
    for row in rows() {
        let name = row["name"].as_str().expect("a name");
        let written = trace(&row);
        let derived = written
            .iter()
            .find(|line| line.starts_with("operation "))
            .expect("every trace names its operation");
        assert_eq!(
            derived,
            &format!("operation {}", submission().operation.agent_operation_identifier),
            "{name}: no cut allocates a replacement identifier"
        );
        assert!(
            written.iter().any(|line| line.starts_with("ambiguity ")),
            "{name}: and every cut is settled by asking about the same tuple"
        );
    }
}

#[test]
fn no_row_dials_a_publisher_or_records_a_credential() {
    let author = FakeAuthor::following(
        Script::of(vec![ScriptedExchange {
            response: ScriptedResponse::Respond { body: Vec::new(), status: OK_STATUS },
            route: SUBMIT_ROUTE.to_owned(),
        }])
        .expect("the submit route is the author's"),
        CredentialPolicy::Basic,
    );
    for row in rows() {
        trace(&row);
    }
    assert!(
        author.recording().requests().is_empty(),
        "running the whole matrix reaches the author for nothing, because every conclusion \
         here is drawn from what was already written down"
    );
    author.answer(&IncomingRequest {
        authorization: Some(CREDENTIAL.to_owned()),
        author_target_identity_digest: Some(TARGET.to_owned()),
        route: SUBMIT_ROUTE.to_owned(),
        selected_environment_revision: Some(REVISION.to_owned()),
    });
    for prefix in PUBLISHER_PREFIXES {
        author.answer(&IncomingRequest {
            authorization: Some(CREDENTIAL.to_owned()),
            author_target_identity_digest: Some(TARGET.to_owned()),
            route: format!("{prefix}/anything"),
            selected_environment_revision: Some(REVISION.to_owned()),
        });
    }
    let recording = author.recording();
    assert_eq!(recording.refused_routes().len(), PUBLISHER_PREFIXES.len());
    assert_eq!(recording.requests().len(), 1, "only the author's own route was served");
    assert!(
        recording.holds_no_credential_values(),
        "and the record says which kind was presented, never which value"
    );
}
