//! The daemon and the simulated author, agreeing about one whole operation.
//!
//! Every module this plan built is proved on its own. What is left to prove is
//! that they agree: that the digest one side derives is the digest the other
//! authenticates, that the route one side builds is a route the other serves,
//! and that the position one side records is the position the other issued. A
//! workspace of individually correct modules that disagree at the seams is the
//! failure this suite exists to find.
//!
//! # What is and is not claimed
//!
//! The simulator answers the protocol, and nothing here claims it runs Java, a
//! Sling instance, or a JCR. What is proved is the agreement about names,
//! digests, routes, positions, and dispositions - which is exactly the part a
//! real agent could not be trusted to tell us we got wrong.
//!
//! # What the recording is for
//!
//! Every scenario ends by scanning what the author was asked. A publisher-
//! shaped route or a proxy would be a finding whatever else was true, and a
//! credential value appearing anywhere in the record would be one too.

use std::sync::Arc;

use slingshot_agent_connection::capability_discovery::{
    AdvertisedCapabilities, DiscoveryRefusal, RequiredCapabilities,
};
use slingshot_agent_connection::command_submission::{
    ExpectedArtifactManifest, Submission, SubmissionAcknowledgement, SubmissionOutcome,
};
use slingshot_agent_connection::job_event_reducer::{
    AssociationBinding, JobDisposition, ObservedJobEvent, RetainedJob, reduce,
};
use slingshot_agent_connection::job_snapshot_reconciliation::{
    Convergence, JobSnapshot, LookupAnswer, SnapshotEcho, SnapshotExpectation, reconcile,
};
use slingshot_agent_connection::subscription_event_fold::{
    SubscriptionDisposition, SubscriptionFact, SubscriptionFold,
};
use slingshot_agent_protocol::identity::WireOperationIdentity;
use slingshot_agent_protocol::job_contract::JobEventKind;
use slingshot_agent_protocol::wire_contract::ExpectedProvenance;
use slingshot_domain::agent_identity::AgentEventStoreGeneration;
use slingshot_domain::author_agent_transport_contract::AuthorAgentTransportContract;
use slingshot_domain::command::schema::canonical_contract_digest;
use slingshot_domain::remote_job::{
    AgentJobState, EventStreamCursor, JobEventSequence, RemoteJobObservation,
};
use slingshot_domain::selected_command_contract_identity::SelectedCommandContractIdentity;
use slingshot_test_support::fake_author::recording::CredentialKind;
use slingshot_test_support::fake_author::script::{
    AUTHOR_ROUTES, PUBLISHER_PREFIXES, Script, ScriptedExchange, ScriptedResponse,
};
use slingshot_test_support::fake_author::server::{
    Answer, CredentialPolicy, FakeAuthor, IncomingRequest, OK_STATUS,
};

/// Where the scenarios this suite is driven from live.
const FIXTURES: &str = "tests/fixtures/author-agent-conformance";

/// The partition every scenario runs in.
const TARGET: &str = "target-identity-digest-one";

/// The environment revision it runs under.
const REVISION: &str = "environment-revision-one";

/// The subscription carrying its events.
const SUBSCRIPTION: &str = "daemon-subscription-one";

/// The generation it runs under.
const GENERATION: u64 = 7;

/// The command every scenario submits.
const COMMAND: &str = "query_paths";

/// The local operation key the derivation starts from.
const LOCAL_OPERATION: &str = "local-operation-one";

/// Canonical arguments the submission carries.
const ARGUMENTS: &str = "{\"path\":\"/content/one\"}";

/// A Basic credential, whose value nothing may record.
const BASIC_CREDENTIAL: &str = "Basic dGhlLXNlY3JldC12YWx1ZQ==";

/// A Bearer credential, whose value nothing may record.
const BEARER_CREDENTIAL: &str = "Bearer a-token-value-nothing-records";

/// A digest substituted where a real one belongs.
const SUBSTITUTED_DIGEST: &str = "0000000000000000000000000000000000000000000000000000000000000000";

/// The capabilities route the author serves.
const CAPABILITIES_ROUTE: &str = "/bin/slingshot/agent/capabilities";

/// The submission route the author serves.
const SUBMIT_ROUTE: &str = "/bin/slingshot/agent/submit";

/// What one event was at its position.
const EVENT_CONTENTS: &str = "contents-of-this-position";

/// The sequence a retained job has already applied.
const HELD_SEQUENCE: u64 = 3;

/// The sequence a snapshot covers, one past what the stream delivered.
const SNAPSHOT_SEQUENCE: u64 = 4;

/// Returns every vector one fixture holds.
fn vectors(name: &str) -> Vec<serde_json::Value> {
    let path = format!("{FIXTURES}/{name}");
    let text = std::fs::read_to_string(&path).unwrap_or_else(|_| panic!("{path} is readable"));
    text.lines().map(|line| serde_json::from_str(line).expect("each line is one vector")).collect()
}

/// Returns what this build has, for the command every scenario submits.
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

/// Returns what the author advertises, which this build must agree with.
fn advertised() -> AdvertisedCapabilities {
    let installed = installed();
    AdvertisedCapabilities {
        agent_event_store_generation: GENERATION,
        canonical_json_contract_digest: installed.canonical_json_contract_digest.clone(),
        command_contracts: vec![(&installed.command_contract).into()],
        continuation_authority_ready: true,
        transport_contract_digest: installed.transport_contract_digest.clone(),
    }
}

/// Returns one author following a script that serves every route once.
fn author(policy: CredentialPolicy) -> FakeAuthor {
    let exchanges = AUTHOR_ROUTES
        .iter()
        .map(|route| ScriptedExchange {
            response: ScriptedResponse::Respond { body: Vec::new(), status: OK_STATUS },
            route: (*route).to_owned(),
        })
        .collect();
    FakeAuthor::following(Script::of(exchanges).expect("every route is the author's"), policy)
}

/// Returns one request for `route`, carrying `authorization`.
fn asked(route: &str, authorization: Option<&str>) -> IncomingRequest {
    IncomingRequest {
        authorization: authorization.map(str::to_owned),
        author_target_identity_digest: Some(TARGET.to_owned()),
        route: route.to_owned(),
        selected_environment_revision: Some(REVISION.to_owned()),
    }
}

/// Returns the credential `spelling` names.
fn credential_named(spelling: &str) -> Option<&'static str> {
    match spelling {
        "basic" => Some(BASIC_CREDENTIAL),
        "bearer" => Some(BEARER_CREDENTIAL),
        "wrong" => Some("Negotiate something-else"),
        "none" => None,
        other => panic!("{other} is a credential this suite does not name"),
    }
}

/// Returns what must agree before a job's state moves.
fn binding() -> AssociationBinding {
    AssociationBinding {
        expected_provenance: installed(),
        selected_environment_revision: REVISION.to_owned(),
        submitted_command_digest: submission().submitted_command_digest,
    }
}

#[test]
fn the_daemon_and_the_author_agree_about_which_contracts_are_installed() {
    let required = RequiredCapabilities::of(
        installed().command_contract,
        &canonical_contract_digest(),
        Some(GENERATION),
    );
    required
        .require_compatible(&advertised())
        .expect("what this build has is what the author advertises");
    for vector in vectors("provenance-drift.jsonl") {
        let name = vector["name"].as_str().expect("a name");
        let mut drifted = advertised();
        drift(vector["field"].as_str().expect("a field"), &mut drifted);
        assert!(
            required.require_compatible(&drifted).is_err(),
            "{name}: a contract that differs on one field is a different contract"
        );
    }
    let mut rebuilt = advertised();
    rebuilt.agent_event_store_generation = GENERATION + 1;
    assert!(matches!(
        required.require_compatible(&rebuilt),
        Err(DiscoveryRefusal::GenerationChanged { .. })
    ));
}

/// Substitutes one advertised field for something else.
fn drift(field: &str, advertised: &mut AdvertisedCapabilities) {
    let contract = advertised.command_contracts.first_mut().expect("one contract");
    match field {
        "transport_contract_digest" => {
            advertised.transport_contract_digest = SUBSTITUTED_DIGEST.to_owned();
        }
        "canonical_json_contract_digest" => {
            advertised.canonical_json_contract_digest = SUBSTITUTED_DIGEST.to_owned();
        }
        "argument_schema_digest" => {
            contract.argument_schema_digest = SUBSTITUTED_DIGEST.to_owned();
        }
        "result_schema_digest" => contract.result_schema_digest = SUBSTITUTED_DIGEST.to_owned(),
        "command_contract_limits_digest" => {
            contract.command_contract_limits_digest = SUBSTITUTED_DIGEST.to_owned();
        }
        "command_semantic_contract_version" => {
            contract.command_semantic_contract_version = "second".to_owned();
        }
        "command_wire_name" => contract.command_wire_name = "create_page".to_owned(),
        other => panic!("{other} is a field this suite does not substitute"),
    }
}

#[test]
fn every_credential_kind_is_answered_the_way_its_scenario_says() {
    for vector in vectors("credentials.jsonl") {
        let name = vector["name"].as_str().expect("a name");
        let policy = if name.starts_with("cloud") {
            CredentialPolicy::Bearer
        } else {
            CredentialPolicy::Basic
        };
        let author = author(policy);
        let credential = credential_named(vector["credential"].as_str().expect("a credential"));
        let answer = author.answer(&asked(CAPABILITIES_ROUTE, credential));
        let accepted = !matches!(answer, Answer::Unauthenticated);
        assert_eq!(
            accepted,
            vector["accepted"].as_bool().expect("an expectation"),
            "{name}: {answer:?}"
        );
        assert!(
            author.recording().holds_no_credential_values(),
            "{name}: the record says which kind was presented and never which value"
        );
    }
}

#[test]
fn every_route_the_daemon_asks_for_is_one_the_author_serves() {
    let author = author(CredentialPolicy::Basic);
    for vector in vectors("routes.jsonl") {
        let route = vector["route"].as_str().expect("a route");
        let served = vector["served"].as_bool().expect("an expectation");
        let answer = author.answer(&asked(route, Some(BASIC_CREDENTIAL)));
        let reached = !matches!(answer, Answer::RouteRefused);
        assert_eq!(
            reached,
            served || !PUBLISHER_PREFIXES.iter().any(|p| route.starts_with(p)),
            "{route}: a publisher-shaped route is a finding whatever else is true"
        );
        if !served && PUBLISHER_PREFIXES.iter().any(|prefix| route.starts_with(prefix)) {
            assert!(
                author.recording().refused_routes().contains(&route.to_owned()),
                "{route}: and it is recorded as one"
            );
        }
    }
    assert!(
        author.recording().holds_no_credential_values(),
        "and no credential value reaches the record along the way"
    );
}

#[test]
fn one_submission_is_acknowledged_under_exactly_the_names_this_build_derived() {
    let submission = submission();
    let author = author(CredentialPolicy::Basic);
    let answer = author.answer(&asked(SUBMIT_ROUTE, Some(BASIC_CREDENTIAL)));
    assert!(matches!(answer, Answer::Responded { .. }), "the author serves the route");

    let acknowledgement = SubmissionAcknowledgement {
        agent_event_store_generation: GENERATION,
        agent_operation_identifier: submission.operation.agent_operation_identifier.clone(),
        author_target_identity_digest: TARGET.to_owned(),
        already_accepted: false,
        daemon_subscription_identifier: SUBSCRIPTION.to_owned(),
        granted_retention_milliseconds: AuthorAgentTransportContract::embedded()
            .limit("maximum_persisted_remaining_retention_milliseconds"),
        non_execution: None,
        physical_sling_job_identifiers: vec!["sling-job-alpha".to_owned()],
        retired: false,
        submitted_command_digest: submission.submitted_command_digest.clone(),
    };
    let exchange = exchange_of(acknowledgement);
    assert!(
        matches!(submission.interpret(&exchange), SubmissionOutcome::Accepted { .. }),
        "the digest this build derived is the digest the author echoes"
    );
    assert_eq!(
        submission.operation.agent_operation_identifier,
        submission_again().operation.agent_operation_identifier,
        "and a restart derives the same operation identifier from the same inputs"
    );
}

/// Returns the submission a restarted daemon would derive.
fn submission_again() -> Submission {
    submission()
}

/// Returns a clean exchange carrying `acknowledgement`.
fn exchange_of(
    acknowledgement: SubmissionAcknowledgement,
) -> slingshot_agent_connection::command_submission::Exchange {
    use slingshot_agent_connection::author_hypertext_transfer_protocol_policy::ResponseHead;
    use slingshot_agent_connection::command_submission::{ANSWERED_STATUSES, Exchange};
    Exchange {
        acknowledgement: Some(acknowledgement),
        body_bytes: 0,
        elapsed_milliseconds: 1,
        framing_ambiguous: false,
        head: ResponseHead {
            alternative_service_offered: false,
            content_coding: None,
            informational: false,
            location: None,
            protocol_version: "HTTP/1.1".to_owned(),
            trailers_declared: false,
        },
        media_type: "application/json".to_owned(),
        retry_after_milliseconds: None,
        status: ANSWERED_STATUSES[0],
        trailer_section_present: false,
        trailing_bytes: false,
        unknown_fields: false,
    }
}

#[test]
fn the_two_folds_agree_about_every_event_the_author_emits() {
    let submission = submission();
    for vector in vectors("events.jsonl") {
        let name = vector["name"].as_str().expect("a name");
        let sequence = vector["sequence"].as_u64().expect("a sequence");
        let held = vector["held"].as_bool().expect("an expectation").then(|| RetainedJob {
            observation: RemoteJobObservation {
                applied_sequence: JobEventSequence::of(HELD_SEQUENCE),
                attempt: 0,
                progress: 0,
                state: AgentJobState::Queued,
            },
            snapshot_watermark: JobEventSequence::of(HELD_SEQUENCE),
        });
        let event = ObservedJobEvent {
            attempt: 0,
            correlation: None,
            kind: JobEventKind::Progress,
            progress: 0,
            selected_environment_revision: REVISION.to_owned(),
            sequence: JobEventSequence::of(sequence),
        };
        let (job, _) = reduce(held.as_ref(), &binding(), &event)
            .unwrap_or_else(|refusal| panic!("{name}: {refusal}"));
        assert_eq!(job_spelling(job), vector["job"].as_str().expect("a disposition"), "{name}");

        let fold = SubscriptionFold::opened(SUBSCRIPTION, GENERATION);
        let (subscription, _) = fold
            .folded(&SubscriptionFact {
                agent_event_store_generation: GENERATION,
                canonical_digest: EVENT_CONTENTS.to_owned(),
                cursor: EventStreamCursor::new(&format!("cursor-{sequence:04}"))
                    .expect("a short cursor"),
                daemon_subscription_identifier: SUBSCRIPTION.to_owned(),
            })
            .expect("the event is this subscription's");
        assert_eq!(
            subscription_spelling(subscription),
            vector["subscription"].as_str().expect("a disposition"),
            "{name}: the stream moves even when the job does not"
        );
    }
    assert_eq!(
        binding().submitted_command_digest,
        submission.submitted_command_digest,
        "and both folds are bound to the submission this build actually made"
    );
}

/// Returns how one job disposition is spelled in the vectors.
fn job_spelling(disposition: JobDisposition) -> &'static str {
    match disposition {
        JobDisposition::Applied => "applied",
        JobDisposition::ExactReplay => "exact-replay",
        JobDisposition::StaleCursorOnly => "stale-cursor-only",
        JobDisposition::NeedsSnapshot => "needs-snapshot",
        JobDisposition::IntegrityConflictNeedsReconciliation => "integrity-conflict",
    }
}

/// Returns how one subscription disposition is spelled in the vectors.
fn subscription_spelling(disposition: SubscriptionDisposition) -> &'static str {
    match disposition {
        SubscriptionDisposition::Advanced => "advanced",
        SubscriptionDisposition::ExactReplay => "exact-replay",
        SubscriptionDisposition::StaleCursorOnly => "stale-cursor-only",
        SubscriptionDisposition::IntegrityConflictNeedsReconciliation => "integrity-conflict",
    }
}

#[test]
fn a_snapshot_converges_the_work_the_stream_left_unfinished() {
    let submission = submission();
    let expectation = SnapshotExpectation {
        agent_event_store_generation: GENERATION,
        agent_operation_identifier: submission.operation.agent_operation_identifier.clone(),
        author_target_identity_digest: TARGET.to_owned(),
        daemon_subscription_identifier: SUBSCRIPTION.to_owned(),
        expected_provenance: installed(),
        selected_environment_revision: REVISION.to_owned(),
        submitted_command_digest: submission.submitted_command_digest.clone(),
    };
    let snapshot = JobSnapshot {
        attempt: 1,
        echo: SnapshotEcho {
            agent_event_store_generation: GENERATION,
            agent_operation_identifier: submission.operation.agent_operation_identifier.clone(),
            author_target_identity_digest: TARGET.to_owned(),
            daemon_subscription_identifier: SUBSCRIPTION.to_owned(),
            provenance: installed().provenance(),
            selected_environment_revision: REVISION.to_owned(),
            submitted_command_digest: submission.submitted_command_digest.clone(),
        },
        granted_retention_milliseconds: AuthorAgentTransportContract::embedded()
            .limit("maximum_persisted_remaining_retention_milliseconds"),
        kind: JobEventKind::Succeeded,
        physical_sling_job_identifiers: vec!["sling-job-alpha".to_owned()],
        progress: 0,
        sequence: JobEventSequence::of(SNAPSHOT_SEQUENCE),
    };
    let converged = reconcile(
        &expectation,
        RemoteJobObservation::accepted(),
        &LookupAnswer::Found(Box::new(snapshot)),
        0,
        1,
    )
    .expect("the answer is about this submission");
    assert!(
        matches!(converged, Convergence::ReadyToSettle(_)),
        "the digest the submission derived is the digest the snapshot has to echo"
    );
}

#[test]
fn nothing_a_scenario_does_reaches_a_publisher_or_records_a_credential() {
    let author = author(CredentialPolicy::Bearer);
    for route in AUTHOR_ROUTES {
        author.answer(&asked(route, Some(BEARER_CREDENTIAL)));
    }
    for prefix in PUBLISHER_PREFIXES {
        author.answer(&asked(&format!("{prefix}/anything"), Some(BEARER_CREDENTIAL)));
    }
    let recording = Arc::clone(&author.recording());
    assert_eq!(
        recording.refused_routes().len(),
        PUBLISHER_PREFIXES.len(),
        "every publisher-shaped route is refused and recorded as one"
    );
    for request in recording.requests() {
        assert_eq!(request.credential_kind, CredentialKind::Bearer);
        assert!(AUTHOR_ROUTES.contains(&request.route.as_str()), "and served routes are author's");
    }
    assert!(recording.holds_no_credential_values());
    assert!(author.script_is_exhausted(), "and the scenario used exactly what it scripted");
}
