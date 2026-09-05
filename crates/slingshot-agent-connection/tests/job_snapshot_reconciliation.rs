//! Asking the agent what is true, and believing only what is about this work.
//!
//! The stream is not a record. After a reconnection or a restart this daemon
//! asks, and the whole difficulty is in what it is allowed to conclude from the
//! answer. Three things are proved.
//!
//! Every echo is checked before anything about the job is read, one substituted
//! field at a time. The case that makes this necessary is quiet: a result
//! produced by the same command with different arguments is validly shaped,
//! correctly sequenced, and completely wrong, and only the submitted digest
//! tells them apart.
//!
//! Reconciliation never rolls backwards. A snapshot older than what is already
//! applied is old news rather than a correction, and converges to nothing.
//!
//! An ending is decided here and written down elsewhere. The settlement is
//! injected, and a settlement that declines leaves every fact where it was -
//! proved by declining and then looking at what the caller was holding.

use slingshot_agent_connection::job_snapshot_reconciliation::{
    Convergence, GenerationLossCertainty, HIGH_WATER_ROUTE, JobSnapshot, LOOKUP_ROUTE,
    LookupAnswer, OPERATION_QUERY_MEMBER, PHYSICAL_JOB_ROUTE, ReconciliationOutcome,
    ReconciliationRefusal, SLING_JOB_QUERY_MEMBER, SettlementRefusal, SnapshotEcho,
    SnapshotExpectation, TerminalFacts, TerminalSettlement, certainty_after_generation_change,
    encoded_once, high_water_route, lookup_route, missing_grace_milliseconds, physical_job_routes,
    reconcile, reconcile_and_settle,
};
use slingshot_agent_protocol::identity::DocumentProvenance;
use slingshot_agent_protocol::job_contract::JobEventKind;
use slingshot_agent_protocol::wire_contract::ExpectedProvenance;
use slingshot_domain::author_agent_transport_contract::AuthorAgentTransportContract;
use slingshot_domain::command::schema::canonical_contract_digest;
use slingshot_domain::remote_job::{AgentJobState, JobEventSequence, RemoteJobObservation};
use slingshot_domain::selected_command_contract_identity::SelectedCommandContractIdentity;
use std::cell::RefCell;

/// Where the vectors this suite is driven from live.
const FIXTURES: &str = "tests/fixtures/job-snapshot-reconciliation";

/// The partition every answer here belongs to.
const TARGET: &str = "target-identity-digest-one";

/// The subscription carrying these events.
const SUBSCRIPTION: &str = "daemon-subscription-one";

/// The generation these facts belong to.
const GENERATION: u64 = 7;

/// A later generation, after the agent's store was rebuilt.
const LATER_GENERATION: u64 = 8;

/// What the operation is called at the agent.
const AGENT_OPERATION: &str = "agent-operation-alpha";

/// The environment revision it was submitted under.
const REVISION: &str = "environment-revision-one";

/// The command these answers are about.
const COMMAND: &str = "query_paths";

/// Another published command, to substitute one wire name for another.
const OTHER_COMMAND: &str = "create_page";

/// The submission these answers are about.
const SUBMITTED_DIGEST: &str = "1111111111111111111111111111111111111111111111111111111111111111";

/// A digest substituted where a real one belongs.
const SUBSTITUTED_DIGEST: &str = "0000000000000000000000000000000000000000000000000000000000000000";

/// When the request that submitted this started.
const SUBMITTED_AT: u64 = 1_700_000_000_000;

/// How long the agent promises to keep the results.
const GRANTED_RETENTION: u64 = 120_000;

/// How long after submitting a prompt reconciliation happens.
const SOON: u64 = 400;

/// The sequence a running job sits at before it is asked about.
const RUNNING_SEQUENCE: u64 = 1;

/// The sequence an answer that ends the job carries.
const ENDING_SEQUENCE: u64 = 2;

/// A sequence further along than anything this daemon applied.
const AHEAD_SEQUENCE: u64 = 5;

/// How many attempts a job that has started once has had.
const ONE_ATTEMPT: u64 = 1;

/// How far along a running job says it is.
const SOME_PROGRESS: u64 = 40;

/// How far along a finished job says it got.
const COMPLETE_PROGRESS: u64 = 100;

/// The physical Sling jobs the agent says are carrying this.
const PHYSICAL_JOBS: &[&str] = &["sling-job-alpha", "sling-job-beta"];

/// Returns every vector one fixture holds.
fn vectors(name: &str) -> Vec<serde_json::Value> {
    let path = format!("{FIXTURES}/{name}");
    let text = std::fs::read_to_string(&path).unwrap_or_else(|_| panic!("{path} is readable"));
    text.lines().map(|line| serde_json::from_str(line).expect("each line is one vector")).collect()
}

/// Returns what this build has, for the command these answers are about.
fn installed_provenance() -> ExpectedProvenance {
    ExpectedProvenance {
        canonical_json_contract_digest: canonical_contract_digest(),
        command_contract: SelectedCommandContractIdentity::installed(COMMAND)
            .expect("the command is published"),
        transport_contract_digest: AuthorAgentTransportContract::embedded_digest(),
    }
}

/// Returns what this daemon knows about the submission it asks after.
fn expectation() -> SnapshotExpectation {
    SnapshotExpectation {
        agent_event_store_generation: GENERATION,
        agent_operation_identifier: AGENT_OPERATION.to_owned(),
        author_target_identity_digest: TARGET.to_owned(),
        daemon_subscription_identifier: SUBSCRIPTION.to_owned(),
        expected_provenance: installed_provenance(),
        selected_environment_revision: REVISION.to_owned(),
        submitted_command_digest: SUBMITTED_DIGEST.to_owned(),
    }
}

#[test]
fn absence_requires_a_complete_same_partition_document_not_a_status_alone() {
    use slingshot_agent_connection::selected_author_lookup::decode_lookup_absence;
    let expected = expectation();
    let missing = serde_json::json!({
        "kind": "missing", "format": "slingshot.agent/1",
        "transport_contract_digest": expected.expected_provenance.transport_contract_digest,
        "agent_event_store_generation": expected.agent_event_store_generation,
        "agent_operation_identifier": expected.agent_operation_identifier,
        "author_target_identity_digest": expected.author_target_identity_digest,
    });
    let retired = serde_json::json!({
        "kind": "retired", "provenance": expected.expected_provenance.provenance(),
        "agent_event_store_generation": expected.agent_event_store_generation,
        "agent_operation_identifier": expected.agent_operation_identifier,
        "author_target_identity_digest": expected.author_target_identity_digest,
        "daemon_subscription_identifier": expected.daemon_subscription_identifier,
        "selected_environment_revision": expected.selected_environment_revision,
        "submitted_command_digest": expected.submitted_command_digest,
    });
    for (status, document) in [(404, missing), (410, retired)] {
        let bytes = serde_json::to_vec(&document).unwrap();
        let accepted = decode_lookup_absence(status, &bytes, &expected).unwrap();
        assert!(matches!(
            (status, accepted),
            (404, LookupAnswer::Missing) | (410, LookupAnswer::Retired(_))
        ));
        for wrong_status in [200, 401, 403, 500, if status == 404 { 410 } else { 404 }] {
            assert!(decode_lookup_absence(wrong_status, &bytes, &expected).is_err());
        }
        for member in document.as_object().unwrap().keys() {
            let mut changed = document.clone();
            changed.as_object_mut().unwrap().remove(member);
            assert!(
                decode_lookup_absence(status, &serde_json::to_vec(&changed).unwrap(), &expected)
                    .is_err()
            );
            let mut changed = document.clone();
            changed[member] = serde_json::json!("another");
            assert!(
                decode_lookup_absence(status, &serde_json::to_vec(&changed).unwrap(), &expected)
                    .is_err()
            );
        }
        let mut extra = document.clone();
        extra["unknown"] = serde_json::json!(true);
        assert!(
            decode_lookup_absence(status, &serde_json::to_vec(&extra).unwrap(), &expected).is_err()
        );
        assert!(decode_lookup_absence(status, b"{}", &expected).is_err());
        assert!(decode_lookup_absence(status, b"", &expected).is_err());
    }
}

#[test]
fn network_snapshot_decoder_requires_all_echoes_and_a_bounded_ordered_job_set() {
    use slingshot_agent_connection::job_snapshot_reconciliation::decode_snapshot;
    let mut expected = expectation();
    expected.agent_operation_identifier = "a".repeat(64);
    expected.author_target_identity_digest = "b".repeat(64);
    expected.selected_environment_revision = "c".repeat(64);
    expected.submitted_command_digest = "d".repeat(64);
    let document = serde_json::json!({
        "provenance": expected.expected_provenance.provenance(),
        "agent_event_store_generation": expected.agent_event_store_generation,
        "agent_operation_identifier": expected.agent_operation_identifier,
        "author_target_identity_digest": expected.author_target_identity_digest,
        "selected_environment_revision": expected.selected_environment_revision,
        "daemon_subscription_identifier": expected.daemon_subscription_identifier,
        "submitted_command_digest": expected.submitted_command_digest,
        "kind": "progress", "sequence": 2, "attempt": 1, "progress": 10,
        "subscription_watermark":"cursor-010", "physical_sling_job_identifiers": ["job-a", "job-b"],
        "granted_retention_milliseconds": 120000
    });
    let decode =
        |value: &serde_json::Value| decode_snapshot(&serde_json::to_vec(value).unwrap(), &expected);
    assert_eq!(decode(&document).unwrap().progress, 10);
    assert_eq!(decode(&document).unwrap().subscription_watermark.as_text(), "cursor-010");
    assert_eq!(format!("{:?}", decode(&document).unwrap()), "JobSnapshot([redacted])");
    let mut missing_watermark = document.clone();
    missing_watermark.as_object_mut().unwrap().remove("subscription_watermark");
    assert!(decode(&missing_watermark).is_err());
    for invalid in [serde_json::Value::Null, serde_json::json!(10), serde_json::json!(""),
        serde_json::json!("x".repeat(97)), serde_json::json!("é".repeat(49)),
        serde_json::json!("bad\r\ncursor"), serde_json::json!(" leading"), serde_json::json!("trailing ")] {
        let mut changed = document.clone(); changed["subscription_watermark"] = invalid;
        assert!(decode(&changed).is_err());
    }
    let duplicated = serde_json::to_string(&document).unwrap().replacen('{', "{\"subscription_watermark\":\"cursor-010\",", 1);
    assert!(decode_snapshot(duplicated.as_bytes(), &expected).is_err());
    let mut successful = document.clone();
    let mut failed = document.clone();
    failed["kind"] = "failed".into();
    failed["terminal_failure"] = serde_json::json!({
        "operation": {
            "agent_event_store_generation": expected.agent_event_store_generation,
            "agent_operation_identifier": expected.agent_operation_identifier,
            "author_target_identity_digest": expected.author_target_identity_digest,
            "selected_environment_revision": expected.selected_environment_revision
        },
        "daemon_subscription_identifier": expected.daemon_subscription_identifier,
        "provenance": expected.expected_provenance.provenance(),
        "submitted_command_digest": expected.submitted_command_digest,
        "canonical_failure": "{\"failure\":\"not_found\"}"
    });
    assert!(decode(&failed).unwrap().terminal_failure.is_some());
    let mut changed = failed.clone();
    changed["terminal_failure"]["operation"]["agent_event_store_generation"] = 999.into();
    assert!(decode(&changed).is_err());
    let mut changed = failed.clone();
    changed["terminal_failure"]["provenance"]["transport_contract_digest"] = "e".repeat(64).into();
    assert!(decode(&changed).is_err());
    for kind in ["accepted", "started", "progress", "succeeded"] {
        let mut changed = failed.clone();
        changed["kind"] = kind.into();
        assert!(decode(&changed).is_err());
    }
    for member in [
        "agent_operation_identifier",
        "author_target_identity_digest",
        "selected_environment_revision",
    ] {
        let mut changed = failed.clone();
        changed["terminal_failure"]["operation"][member] = "e".repeat(64).into();
        assert!(decode(&changed).is_err());
    }
    for member in ["daemon_subscription_identifier", "submitted_command_digest"] {
        let mut changed = failed.clone();
        changed["terminal_failure"][member] = "e".repeat(64).into();
        assert!(decode(&changed).is_err());
    }
    let mut changed = failed.clone();
    changed["terminal_failure"] = serde_json::Value::Null;
    assert!(decode(&changed).is_err());
    successful["kind"] = serde_json::json!("succeeded");
    successful["terminal_result"] = serde_json::json!({
        "operation": {
            "agent_event_store_generation": expected.agent_event_store_generation,
            "agent_operation_identifier": expected.agent_operation_identifier,
            "author_target_identity_digest": expected.author_target_identity_digest,
            "selected_environment_revision": expected.selected_environment_revision
        },
        "daemon_subscription_identifier": expected.daemon_subscription_identifier,
        "provenance": expected.expected_provenance.provenance(),
        "submitted_command_digest": expected.submitted_command_digest,
        "canonical_result": "{\"matches\":[]}", "declared_artifacts": []
    });
    assert_eq!(
        decode(&successful).unwrap().terminal_result.unwrap().canonical_result,
        "{\"matches\":[]}"
    );
    for kind in ["succeeded", "failed"] {
        let mut mixed = successful.clone();
        mixed["kind"] = kind.into();
        mixed["terminal_failure"] = failed["terminal_failure"].clone();
        assert!(decode(&mixed).is_err(), "a snapshot cannot carry success and failure together");
    }
    for kind in ["accepted", "started", "progress", "failed"] {
        let mut changed = successful.clone();
        changed["kind"] = serde_json::json!(kind);
        assert!(decode(&changed).is_err());
    }
    for member in [
        "agent_operation_identifier",
        "author_target_identity_digest",
        "selected_environment_revision",
    ] {
        let mut changed = successful.clone();
        changed["terminal_result"]["operation"][member] = serde_json::json!("e".repeat(64));
        assert!(decode(&changed).is_err());
    }
    let mut null = successful.clone();
    null["terminal_result"] = serde_json::Value::Null;
    assert!(decode(&null).is_err());
    for member in document.as_object().unwrap().keys() {
        let mut missing = document.clone();
        missing.as_object_mut().unwrap().remove(member);
        assert!(decode(&missing).is_err(), "missing {member}");
    }
    for member in [
        "agent_operation_identifier",
        "author_target_identity_digest",
        "selected_environment_revision",
        "submitted_command_digest",
    ] {
        let mut changed = document.clone();
        changed[member] = serde_json::json!("e".repeat(64));
        assert!(decode(&changed).is_err(), "changed {member}");
    }
    for invalid in [
        serde_json::json!([]),
        serde_json::json!(["job-b", "job-a"]),
        serde_json::json!(["job-a", "job-a"]),
        serde_json::json!([""]),
        serde_json::json!(["x".repeat(1025)]),
    ] {
        let mut changed = document.clone();
        changed["physical_sling_job_identifiers"] = invalid;
        assert!(decode(&changed).is_err());
    }
    let mut extra = document.clone();
    extra["unrecognized"] = serde_json::json!(true);
    assert!(decode(&extra).is_err());
    let mut drift = document;
    drift["provenance"]["transport_contract_digest"] = serde_json::json!("f".repeat(64));
    assert!(decode(&drift).is_err());
}

/// Per-job sequence and subscription cursor are distinct ordering domains.
#[test]
fn reset_coverage_uses_subscription_watermark_not_job_sequence() {
    use slingshot_agent_connection::selected_author_exchange::{CollectedFiniteResponse, validate_collected_finite_response};
    use slingshot_agent_connection::subscription_high_water::decode_high_water;
    use slingshot_agent_connection::server_sent_event_decoder::EventStreamCursor;
    let response = validate_collected_finite_response(CollectedFiniteResponse {
        response: http::Response::builder().status(200).version(http::Version::HTTP_2)
            .header("content-type", "application/json")
            .body(serde_json::to_vec(&serde_json::json!({
                "format":"slingshot.agent/1", "transport_contract_digest":AuthorAgentTransportContract::embedded_digest(),
                "daemon_subscription_identifier":SUBSCRIPTION, "agent_event_store_generation":GENERATION,
                "high_water_cursor":"cursor-010"
            })).unwrap()).unwrap(),
        framing_ambiguous: false, trailer_section_present: false, trailing_bytes: false,
    }).unwrap();
    let capture = decode_high_water(&response, SUBSCRIPTION, GENERATION).unwrap();
    for (watermark, covered) in [("cursor-009", false), ("cursor-010", true), ("cursor-011", true)] {
        for sequence in [0, 1, u64::MAX] {
            let mut value = snapshot(JobEventKind::Progress, sequence, 1, 10);
            value.subscription_watermark = EventStreamCursor::new(watermark, 96).unwrap();
            assert_eq!(capture.require_snapshot_coverage(&value).is_ok(), covered);
        }
    }
    let mut value = snapshot(JobEventKind::Progress, 100, 1, 10);
    value.subscription_watermark = EventStreamCursor::new("z\r\nunsafe", 96).unwrap();
    assert!(capture.require_snapshot_coverage(&value).is_err());
    value.subscription_watermark = EventStreamCursor::new("cursor-010", 96).unwrap();
    value.echo.agent_event_store_generation += 1;
    assert!(capture.require_snapshot_coverage(&value).is_err());
    value.echo.agent_event_store_generation = GENERATION;
    value.echo.daemon_subscription_identifier = "other".into();
    assert!(capture.require_snapshot_coverage(&value).is_err());
}

/// Returns the echo a truthful answer carries.
fn echo() -> SnapshotEcho {
    SnapshotEcho {
        agent_event_store_generation: GENERATION,
        agent_operation_identifier: AGENT_OPERATION.to_owned(),
        author_target_identity_digest: TARGET.to_owned(),
        daemon_subscription_identifier: SUBSCRIPTION.to_owned(),
        provenance: installed_provenance().provenance(),
        selected_environment_revision: REVISION.to_owned(),
        submitted_command_digest: SUBMITTED_DIGEST.to_owned(),
    }
}

/// Returns one snapshot of `kind` at `sequence`.
fn snapshot(kind: JobEventKind, sequence: u64, attempt: u64, progress: u64) -> JobSnapshot {
    JobSnapshot {
        subscription_watermark: slingshot_agent_connection::server_sent_event_decoder::EventStreamCursor::new("cursor-010", 96).unwrap(),
        terminal_result: None,
        terminal_failure: None,
        attempt,
        echo: echo(),
        granted_retention_milliseconds: GRANTED_RETENTION,
        kind,
        physical_sling_job_identifiers: PHYSICAL_JOBS.iter().map(|job| (*job).to_owned()).collect(),
        progress,
        sequence: JobEventSequence::of(sequence),
    }
}

/// Returns what is held about a job in `state` at `sequence`.
fn held(state: AgentJobState, sequence: u64, attempt: u64, progress: u64) -> RemoteJobObservation {
    RemoteJobObservation {
        applied_sequence: JobEventSequence::of(sequence),
        attempt,
        progress,
        state,
    }
}

/// Returns the kind `spelling` names.
fn kind_named(spelling: &str) -> JobEventKind {
    match spelling {
        "accepted" => JobEventKind::Accepted,
        "started" => JobEventKind::Started,
        "progress" => JobEventKind::Progress,
        "succeeded" => JobEventKind::Succeeded,
        "failed" => JobEventKind::Failed,
        other => panic!("{other} is a kind this suite does not name"),
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

/// Returns how one convergence is spelled in the vectors.
fn convergence_spelling(convergence: &Convergence) -> &'static str {
    match convergence {
        Convergence::Unchanged => "unchanged",
        Convergence::Advanced(_) => "advanced",
        Convergence::StaleSnapshot => "stale-snapshot",
        Convergence::ReadyToSettle(_) => "ready-to-settle",
        Convergence::RecoveryWindowExpired => "recovery-window-expired",
        Convergence::GraceRequired { .. } => "grace-required",
        Convergence::Indeterminate => "indeterminate",
    }
}

/// Returns how one refusal is spelled in the vectors.
fn refusal_spelling(refusal: &ReconciliationRefusal) -> &'static str {
    match refusal {
        ReconciliationRefusal::AnotherTarget => "another-target",
        ReconciliationRefusal::AnotherGeneration { .. } => "another-generation",
        ReconciliationRefusal::AnotherOperation => "another-operation",
        ReconciliationRefusal::AnotherSubscription => "another-subscription",
        ReconciliationRefusal::AnotherRevision => "another-revision",
        ReconciliationRefusal::AnotherSubmission => "another-submission",
        ReconciliationRefusal::Provenance(_) => "provenance",
        other => panic!("{other} is a refusal this suite does not stage"),
    }
}

/// A settlement that records what it was offered and answers as it was told.
#[derive(Debug)]
struct RecordingSettlement {
    /// What it was offered, in order.
    offered: RefCell<Vec<TerminalFacts>>,
    /// Whether it declines.
    declines: bool,
}

impl RecordingSettlement {
    /// Returns a settlement that writes whatever it is offered.
    fn accepting() -> Self {
        Self { offered: RefCell::new(Vec::new()), declines: false }
    }

    /// Returns a settlement that will not write anything.
    fn declining() -> Self {
        Self { offered: RefCell::new(Vec::new()), declines: true }
    }
}

impl TerminalSettlement for RecordingSettlement {
    fn settle(&self, facts: &TerminalFacts) -> Result<(), SettlementRefusal> {
        self.offered.borrow_mut().push(facts.clone());
        if self.declines {
            return Err(SettlementRefusal::Declined {
                reason: "the transaction would not commit".to_owned(),
            });
        }
        Ok(())
    }
}

#[test]
fn every_answer_converges_exactly_as_its_vector_states() {
    for vector in vectors("convergence.jsonl") {
        let name = vector["name"].as_str().expect("a name");
        let observed = held(
            state_named(vector["held_state"].as_str().expect("a state")),
            vector["held_sequence"].as_u64().expect("a sequence"),
            vector["held_attempt"].as_u64().expect("an attempt"),
            vector["held_progress"].as_u64().expect("a progress"),
        );
        let answer = LookupAnswer::Found(Box::new(snapshot(
            kind_named(vector["snapshot_kind"].as_str().expect("a kind")),
            vector["snapshot_sequence"].as_u64().expect("a sequence"),
            vector["attempt"].as_u64().expect("an attempt"),
            vector["progress"].as_u64().expect("a progress"),
        )));
        let convergence =
            reconcile(&expectation(), observed, &answer, SUBMITTED_AT, SUBMITTED_AT + SOON)
                .unwrap_or_else(|refusal| panic!("{name}: {refusal}"));
        assert_eq!(
            convergence_spelling(&convergence),
            vector["convergence"].as_str().expect("a convergence"),
            "{name}"
        );
    }
}

#[test]
fn a_stale_snapshot_rolls_nothing_backwards() {
    let observed = held(AgentJobState::Running, AHEAD_SEQUENCE, ONE_ATTEMPT, SOME_PROGRESS);
    let behind =
        LookupAnswer::Found(Box::new(snapshot(JobEventKind::Accepted, ENDING_SEQUENCE, 0, 0)));
    assert_eq!(
        reconcile(&expectation(), observed, &behind, SUBMITTED_AT, SUBMITTED_AT + SOON)
            .expect("old news is believable"),
        Convergence::StaleSnapshot,
        "rolling back would undo events this daemon has already acted on"
    );
}

#[test]
fn an_answer_about_anything_else_is_refused_one_field_at_a_time() {
    for vector in vectors("echoes.jsonl") {
        let name = vector["name"].as_str().expect("a name");
        let field = vector["field"].as_str().expect("a field");
        let answer = LookupAnswer::Found(Box::new(substituted(field)));
        let refusal = reconcile(
            &expectation(),
            held(AgentJobState::Queued, RUNNING_SEQUENCE, 0, 0),
            &answer,
            SUBMITTED_AT,
            SUBMITTED_AT + SOON,
        )
        .expect_err("a substituted echo is about other work");
        assert_eq!(
            refusal_spelling(&refusal),
            vector["refusal"].as_str().expect("a refusal"),
            "{name}"
        );
    }
}

/// Returns one snapshot with `field` substituted for something else.
fn substituted(field: &str) -> JobSnapshot {
    let mut answer =
        snapshot(JobEventKind::Succeeded, ENDING_SEQUENCE, ONE_ATTEMPT, COMPLETE_PROGRESS);
    if !substituted_beside_the_provenance(field, &mut answer) {
        substituted_within_the_provenance(field, &mut answer.echo.provenance);
    }
    answer
}

/// Substitutes one echoed field the provenance does not carry.
fn substituted_beside_the_provenance(field: &str, answer: &mut JobSnapshot) -> bool {
    match field {
        "author_target_identity_digest" => {
            answer.echo.author_target_identity_digest = "target-identity-digest-two".to_owned();
        }
        "agent_event_store_generation" => {
            answer.echo.agent_event_store_generation = LATER_GENERATION;
        }
        "agent_operation_identifier" => {
            answer.echo.agent_operation_identifier = "agent-operation-beta".to_owned();
        }
        "daemon_subscription_identifier" => {
            answer.echo.daemon_subscription_identifier = "another-subscription".to_owned();
        }
        "selected_environment_revision" => {
            answer.echo.selected_environment_revision = "environment-revision-two".to_owned();
        }
        "submitted_command_digest" => {
            answer.echo.submitted_command_digest = SUBSTITUTED_DIGEST.to_owned();
        }
        _ => return false,
    }
    true
}

/// Substitutes one field of the provenance an answer carries.
fn substituted_within_the_provenance(field: &str, provenance: &mut DocumentProvenance) {
    match field {
        "transport_contract_digest" => {
            provenance.transport_contract_digest = SUBSTITUTED_DIGEST.to_owned();
        }
        "canonical_json_contract_digest" => {
            provenance.canonical_json_contract_digest = SUBSTITUTED_DIGEST.to_owned();
        }
        "argument_schema_digest" => {
            provenance.command_contract.argument_schema_digest = SUBSTITUTED_DIGEST.to_owned();
        }
        "result_schema_digest" => {
            provenance.command_contract.result_schema_digest = SUBSTITUTED_DIGEST.to_owned();
        }
        "command_contract_limits_digest" => {
            provenance.command_contract.command_contract_limits_digest =
                SUBSTITUTED_DIGEST.to_owned();
        }
        "command_semantic_contract_version" => {
            provenance.command_contract.command_semantic_contract_version = "second".to_owned();
        }
        "command_wire_name" => {
            provenance.command_contract.command_wire_name = OTHER_COMMAND.to_owned();
        }
        other => panic!("{other} is a substitution this suite does not stage"),
    }
}

#[test]
fn only_a_fully_echoed_retirement_means_the_window_has_closed() {
    let answer = LookupAnswer::Retired(Box::new(echo()));
    assert_eq!(
        reconcile(
            &expectation(),
            held(AgentJobState::Running, AHEAD_SEQUENCE, ONE_ATTEMPT, SOME_PROGRESS),
            &answer,
            SUBMITTED_AT,
            SUBMITTED_AT + SOON
        )
        .expect("a matching retirement is an answer"),
        Convergence::RecoveryWindowExpired
    );
    let mut elsewhere = echo();
    elsewhere.submitted_command_digest = SUBSTITUTED_DIGEST.to_owned();
    assert_eq!(
        reconcile(
            &expectation(),
            held(AgentJobState::Running, AHEAD_SEQUENCE, ONE_ATTEMPT, SOME_PROGRESS),
            &LookupAnswer::Retired(Box::new(elsewhere)),
            SUBMITTED_AT,
            SUBMITTED_AT + SOON
        ),
        Err(ReconciliationRefusal::AnotherSubmission),
        "a retirement about another submission closes nothing"
    );
}

#[test]
fn a_missing_operation_is_given_its_grace_and_then_stays_unknown() {
    let grace = missing_grace_milliseconds();
    let within = reconcile(
        &expectation(),
        held(AgentJobState::Queued, RUNNING_SEQUENCE, 0, 0),
        &LookupAnswer::Missing,
        SUBMITTED_AT,
        SUBMITTED_AT + grace - 1,
    )
    .expect("a missing answer is an answer");
    assert_eq!(
        within,
        Convergence::GraceRequired { until_unix_milliseconds: SUBMITTED_AT + grace }
    );
    let past = reconcile(
        &expectation(),
        held(AgentJobState::Queued, RUNNING_SEQUENCE, 0, 0),
        &LookupAnswer::Missing,
        SUBMITTED_AT,
        SUBMITTED_AT + grace,
    )
    .expect("a missing answer is an answer");
    assert_eq!(
        past,
        Convergence::Indeterminate,
        "propagation delay becoming a lost operation is what the grace prevents, and nothing more"
    );
}

#[test]
fn a_terminal_answer_whose_retention_is_spent_settles_nothing() {
    let answer = LookupAnswer::Found(Box::new(snapshot(
        JobEventKind::Succeeded,
        ENDING_SEQUENCE,
        ONE_ATTEMPT,
        COMPLETE_PROGRESS,
    )));
    assert_eq!(
        reconcile(
            &expectation(),
            held(AgentJobState::Running, RUNNING_SEQUENCE, ONE_ATTEMPT, SOME_PROGRESS),
            &answer,
            SUBMITTED_AT,
            SUBMITTED_AT + GRANTED_RETENTION
        ),
        Err(ReconciliationRefusal::RetentionExpired),
        "equality is expired, because zero remaining time promises nothing"
    );
}

#[test]
fn an_ending_is_offered_to_the_settlement_and_a_refusal_changes_nothing() {
    let observed = held(AgentJobState::Running, RUNNING_SEQUENCE, ONE_ATTEMPT, SOME_PROGRESS);
    let answer = LookupAnswer::Found(Box::new(snapshot(
        JobEventKind::Succeeded,
        ENDING_SEQUENCE,
        ONE_ATTEMPT,
        COMPLETE_PROGRESS,
    )));
    let declining = RecordingSettlement::declining();
    let outcome = reconcile_and_settle(
        &expectation(),
        observed,
        &answer,
        SUBMITTED_AT,
        SUBMITTED_AT + SOON,
        &declining,
    )
    .expect("the answer is about this submission");
    assert!(matches!(outcome, ReconciliationOutcome::SettlementDeclined(_)));
    assert_eq!(declining.offered.borrow().len(), 1, "it was offered exactly once");
    let offered = declining.offered.borrow()[0].clone();
    assert_eq!(offered.observation.state, AgentJobState::Succeeded);
    assert_eq!(offered.physical_sling_job_identifiers, PHYSICAL_JOBS.to_vec());
    assert_eq!(offered.submitted_command_digest, SUBMITTED_DIGEST);
    assert_eq!(
        offered.remaining_retention_milliseconds,
        GRANTED_RETENTION - SOON,
        "the settlement is told what is left, counted from the request that made it"
    );
    assert_eq!(
        observed.state,
        AgentJobState::Running,
        "a declined settlement leaves the caller holding exactly what it held"
    );

    let accepting = RecordingSettlement::accepting();
    let settled = reconcile_and_settle(
        &expectation(),
        observed,
        &answer,
        SUBMITTED_AT,
        SUBMITTED_AT + SOON,
        &accepting,
    )
    .expect("the answer is about this submission");
    assert!(matches!(settled, ReconciliationOutcome::Settled(_)));
}

#[test]
fn nothing_but_an_ending_is_ever_offered_to_the_settlement() {
    let settlement = RecordingSettlement::accepting();
    for answer in [
        LookupAnswer::Missing,
        LookupAnswer::Retired(Box::new(echo())),
        LookupAnswer::Found(Box::new(snapshot(
            JobEventKind::Progress,
            ENDING_SEQUENCE,
            ONE_ATTEMPT,
            SOME_PROGRESS,
        ))),
    ] {
        reconcile_and_settle(
            &expectation(),
            held(AgentJobState::Running, RUNNING_SEQUENCE, ONE_ATTEMPT, SOME_PROGRESS),
            &answer,
            SUBMITTED_AT,
            SUBMITTED_AT + SOON,
            &settlement,
        )
        .expect("each is about this submission");
    }
    assert!(
        settlement.offered.borrow().is_empty(),
        "a settlement that were offered a running job would write an ending nobody reported"
    );
}

#[test]
fn every_query_value_is_encoded_exactly_once() {
    for vector in vectors("query-encoding.jsonl") {
        let name = vector["name"].as_str().expect("a name");
        let value = vector["value"].as_str().expect("a value");
        assert_eq!(
            encoded_once(value),
            vector["encoded"].as_str().expect("an encoding"),
            "{name}: a separator surviving into a value would let it choose its own route"
        );
    }
    let routes = physical_job_routes(&["job&other=1".to_owned()]).expect("one route");
    assert_eq!(
        routes[0],
        format!("{PHYSICAL_JOB_ROUTE}?{SLING_JOB_QUERY_MEMBER}=job%26other%3D1"),
        "a decoded separator never alters which route is selected"
    );
}

#[test]
fn a_recovery_asks_the_same_questions_in_the_same_order_every_time() {
    let scrambled = vec![
        "sling-job-gamma".to_owned(),
        "sling-job-alpha".to_owned(),
        "sling-job-gamma".to_owned(),
    ];
    let routes = physical_job_routes(&scrambled).expect("these fit");
    assert_eq!(
        routes,
        vec![
            format!("{PHYSICAL_JOB_ROUTE}?{SLING_JOB_QUERY_MEMBER}=sling-job-alpha"),
            format!("{PHYSICAL_JOB_ROUTE}?{SLING_JOB_QUERY_MEMBER}=sling-job-gamma"),
        ],
        "sorted and distinct, so two recoveries of one submission ask the same questions"
    );
    let allowed =
        AuthorAgentTransportContract::embedded().limit("maximum_physical_sling_job_matches");
    let many: Vec<String> =
        (0..=allowed).map(|position| format!("sling-job-{position:04}")).collect();
    assert!(matches!(
        physical_job_routes(&many),
        Err(ReconciliationRefusal::TooManyPhysicalJobs { .. })
    ));
}

#[test]
fn the_lookup_and_reset_routes_are_fixed_and_carry_only_what_they_must() {
    let route = lookup_route(AGENT_OPERATION).expect("this identifier is short");
    assert_eq!(route, format!("{LOOKUP_ROUTE}?{OPERATION_QUERY_MEMBER}={AGENT_OPERATION}"));
    let allowed =
        AuthorAgentTransportContract::embedded().limit("maximum_agent_operation_identifier_bytes");
    assert!(matches!(
        lookup_route(&"a".repeat(allowed as usize + 1)),
        Err(ReconciliationRefusal::IdentifierTooLong { .. })
    ));
    let reset = high_water_route(SUBSCRIPTION, GENERATION);
    assert_eq!(
        reset,
        format!(
            "{HIGH_WATER_ROUTE}?agent_event_store_generation={GENERATION}\
             &daemon_subscription_identifier={SUBSCRIPTION}"
        ),
        "the reset asks about exactly the stream the events came from"
    );
}

#[test]
fn a_rebuilt_store_keeps_an_unreachable_agent_apart_from_a_lost_operation() {
    let persisted: Vec<String> = PHYSICAL_JOBS.iter().map(|job| (*job).to_owned()).collect();
    let recovered =
        snapshot(JobEventKind::Succeeded, ENDING_SEQUENCE, ONE_ATTEMPT, COMPLETE_PROGRESS);
    assert_eq!(
        certainty_after_generation_change(&persisted, Some(recovered.clone()), 0),
        GenerationLossCertainty::Recovered(Box::new(recovered))
    );
    assert_eq!(
        certainty_after_generation_change(&persisted, None, persisted.len()),
        GenerationLossCertainty::KnownMissing,
        "every job answering no such job is evidence"
    );
    assert_eq!(
        certainty_after_generation_change(&persisted, None, persisted.len() - 1),
        GenerationLossCertainty::EvidenceFreeAmbiguous,
        "and one that did not answer is the absence of it"
    );
    assert_eq!(
        certainty_after_generation_change(&[], None, 0),
        GenerationLossCertainty::RemoteStateLost,
        "having nothing to ask is the only case that concludes the work is gone"
    );
}
