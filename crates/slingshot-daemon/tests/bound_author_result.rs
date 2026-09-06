//! Actual result bytes cannot select an artifact from another local identity.

const DIGEST_HEX_CHARACTERS: usize = 64;
const RECOVERY_OBSERVED_AT: u64 = 2000;
const COMPLETE_PROGRESS_PERCENT: u64 = 100;
const PEER_READ_BUFFER_BYTES: usize = 4096;
const DECLARED_ARTIFACT_BYTES: u64 = 4096;

use slingshot_agent_connection::structured_job_result::{
    ArtifactEcho, ResultExpectation, TerminalResultDocument,
};
use slingshot_agent_protocol::wire_contract::ExpectedProvenance;
use slingshot_daemon::operation::artifact_completion::decode_bound_result;
use slingshot_domain::author_agent_transport_contract::AuthorAgentTransportContract;
use slingshot_domain::command::canonical_json::write_canonical;
use slingshot_domain::command::catalog::Command;
use slingshot_domain::command::schema::canonical_contract_digest;
use slingshot_domain::installation::InstallationIdentifier;
use slingshot_domain::operation_executor::ExecutionIdentity;
use slingshot_domain::selected_command_contract_identity::SelectedCommandContractIdentity;
use slingshot_storage::artifact_store::ArtifactIdentifier;

#[test]
fn result_context_comes_from_the_durable_owner_and_refuses_stale_revisions() {
    use slingshot_agent_connection::command_submission::{ExpectedArtifactManifest, Submission};
    use slingshot_agent_protocol::identity::WireOperationIdentity;
    use slingshot_daemon::operation::artifact_completion::decode_retained_result;
    use slingshot_domain::agent_identity::AgentEventStoreGeneration;
    use slingshot_domain::command_fingerprint::{CommandFingerprint, FingerprintInput};
    use slingshot_storage::database::{OperationDatabase, RequiredSettings};
    use slingshot_storage::operation_repository::{AdmissionRequest, OperationRepository};
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("results.sqlite3");
    let open = || {
        OperationRepository::new(
            OperationDatabase::open(
                &path,
                RequiredSettings {
                    page_bytes:
                        slingshot_domain::daemon_runtime_contract::DaemonRuntimeContract::embedded()
                            .limit("sqlite_page_bytes"),
                    database_pages: 262144,
                    busy_timeout_milliseconds:
                        slingshot_domain::daemon_runtime_contract::DaemonRuntimeContract::embedded()
                            .limit("database_busy_timeout_milliseconds"),
                },
            )
            .unwrap(),
        )
    };
    let operations = open();
    let identity = ExecutionIdentity {
        attempt: 1,
        author_target_identity_digest: "2".repeat(DIGEST_HEX_CHARACTERS),
        selected_environment_revision: "3".repeat(DIGEST_HEX_CHARACTERS),
        operation_identifier: "query-operation".to_owned(),
    };
    let provenance = ExpectedProvenance {
        canonical_json_contract_digest: canonical_contract_digest(),
        transport_contract_digest: AuthorAgentTransportContract::embedded_digest(),
        command_contract: SelectedCommandContractIdentity::installed("query_paths").unwrap(),
    };
    let arguments = r#"{"root_path":"/content/retained"}"#;
    let submission = Submission::build(
        &provenance,
        WireOperationIdentity::of(
            &identity.author_target_identity_digest,
            &identity.selected_environment_revision,
            &identity.operation_identifier,
            AgentEventStoreGeneration::of(7),
        ),
        "subscription-one",
        arguments,
        ExpectedArtifactManifest::empty(),
    )
    .unwrap();
    let admission = AdmissionRequest {
        author_target_identity: "opaque-target".to_owned(),
        author_target_identity_digest: identity.author_target_identity_digest.clone(),
        caller_identity: None,
        canonical_command: arguments.to_owned(),
        command_fingerprint: CommandFingerprint::derive(&FingerprintInput {
            author_target_identity_digest: identity.author_target_identity_digest.clone(),
            selected_environment_revision: identity.selected_environment_revision.clone(),
            canonical_command: arguments.to_owned(),
            command_wire_name: "query_paths".to_owned(),
            command_semantic_contract_version: "1.0.0".to_owned(),
        })
        .unwrap(),
        command_wire_name: "query_paths".to_owned(),
        daemon_runtime_contract_digest:
            slingshot_domain::daemon_runtime_contract::DaemonRuntimeContract::embedded_digest()
                .as_text()
                .to_owned(),
        installation_identifier: InstallationIdentifier::parse(&"1".repeat(DIGEST_HEX_CHARACTERS))
            .unwrap(),
        operation_identifier: identity.operation_identifier.clone(),
        selected_environment_revision: identity.selected_environment_revision.clone(),
        workflow_correlation_identifier: None,
    };
    let document = TerminalResultDocument {
        operation: submission.operation.clone(),
        daemon_subscription_identifier: submission.daemon_subscription_identifier.clone(),
        canonical_result: r#"{"matches":[{"repository_path":"/content/retained/a"}]}"#.to_owned(),
        declared_artifacts: vec![],
        provenance: provenance.provenance(),
        submitted_command_digest: submission.submitted_command_digest.clone(),
    };
    let body = serde_json::to_vec(&document).unwrap();
    assert!(decode_retained_result(&operations, 1, &identity, &submission, &body).is_err());
    operations.admit(&admission, 1000).unwrap();
    drop(operations);
    let operations = open();
    let before = operations
        .read(&identity.author_target_identity_digest, &identity.operation_identifier)
        .unwrap();
    assert_eq!(
        decode_retained_result(&operations, 1, &identity, &submission, &body)
            .unwrap()
            .canonical_result,
        document.canonical_result
    );
    assert!(decode_retained_result(&operations, 2, &identity, &submission, &body).is_err());
    let mut changed = submission.clone();
    changed.canonical_arguments = r#"{"root_path":"/content/another"}"#.to_owned();
    assert!(decode_retained_result(&operations, 1, &identity, &changed, &body).is_err());
    let changed = Submission::build(
        &provenance,
        submission.operation.clone(),
        "subscription-one",
        r#"{"root_path":"/content/another"}"#,
        ExpectedArtifactManifest::empty(),
    )
    .unwrap();
    assert!(decode_retained_result(&operations, 1, &identity, &changed, &body).is_err());
    assert_eq!(
        operations
            .read(&identity.author_target_identity_digest, &identity.operation_identifier)
            .unwrap(),
        before
    );
    // Exercise the publication transaction separately from remote-success
    // evidence: this storage API must not accept a stale child or owner.
    use slingshot_daemon::operation::durable_author_submission::prepare_initial_submission;
    use slingshot_domain::operation::{OperationLifecycleState, SuccessfulSettlement};
    use slingshot_domain::remote_job::JobEventSequence;
    use slingshot_storage::agent_job_repository::AgentJobRepository;
    let remote = AgentJobRepository::new(
        OperationDatabase::open(
            &path,
            RequiredSettings {
                page_bytes:
                    slingshot_domain::daemon_runtime_contract::DaemonRuntimeContract::embedded()
                        .limit("sqlite_page_bytes"),
                database_pages: 262144,
                busy_timeout_milliseconds:
                    slingshot_domain::daemon_runtime_contract::DaemonRuntimeContract::embedded()
                        .limit("database_busy_timeout_milliseconds"),
            },
        )
        .unwrap(),
    );
    drop(prepare_initial_submission(&remote, &identity, &submission, 1000).unwrap());
    let retained = remote
        .read(
            &identity.author_target_identity_digest,
            &submission.operation.agent_operation_identifier,
        )
        .unwrap()
        .unwrap();
    let settlement = SuccessfulSettlement {
        artifacts: vec![],
        inline_result: Some(document.canonical_result.clone()),
        expected_lifecycle_state: OperationLifecycleState::Queued,
        expected_revision: 1,
        settled_at_unix_milliseconds: RECOVERY_OBSERVED_AT,
    };
    let mut stale_owner = settlement.clone();
    stale_owner.expected_revision = 2;
    assert!(operations.settle_success_for_retained_agent(&retained, &stale_owner).is_err());
    remote.record_snapshot_watermark(&retained.identity, JobEventSequence::of(2)).unwrap();
    assert!(operations.settle_success_for_retained_agent(&retained, &settlement).is_err());
    assert_eq!(
        operations
            .read(&identity.author_target_identity_digest, &identity.operation_identifier)
            .unwrap(),
        before
    );
    let current = remote
        .read(
            &identity.author_target_identity_digest,
            &submission.operation.agent_operation_identifier,
        )
        .unwrap()
        .unwrap();
    use slingshot_daemon::operation::artifact_completion::publish_retained_inline_result;
    use slingshot_domain::operation::{
        OperationFact, RecoveryCategory, RecoveryExecutionEvidence, RecoveryFact,
    };
    assert!(
        publish_retained_inline_result(
            &operations,
            &current,
            1,
            &identity,
            &submission,
            &body,
            RECOVERY_OBSERVED_AT
        )
        .is_err(),
        "a valid payload alone is not persisted remote-success evidence"
    );
    operations
        .apply(
            &identity.author_target_identity_digest,
            &identity.operation_identifier,
            1,
            &OperationFact::Recovery {
                recovery: RecoveryFact {
                    attempt_count: 0,
                    category: RecoveryCategory::ResultAcquisition,
                    detail: "acquiring result".to_owned(),
                    evidence: RecoveryExecutionEvidence::AuthoritativeRemoteSuccess,
                    manual_resume_eligible: false,
                    retry_delay_milliseconds: 0,
                    retry_observed_at_unix_milliseconds: RECOVERY_OBSERVED_AT,
                },
            },
            RECOVERY_OBSERVED_AT,
        )
        .unwrap();
    let recovering = operations
        .read(&identity.author_target_identity_digest, &identity.operation_identifier)
        .unwrap();
    assert!(
        publish_retained_inline_result(
            &operations,
            &retained,
            2,
            &identity,
            &submission,
            &body,
            2001
        )
        .is_err(),
        "stale remote child"
    );
    assert!(
        publish_retained_inline_result(
            &operations,
            &current,
            1,
            &identity,
            &submission,
            &body,
            2001
        )
        .is_err(),
        "stale local owner"
    );
    let mut mismatched = document.clone();
    mismatched.canonical_result =
        r#"{"matches":[{"repository_path":"/content/another/a"}]}"#.to_owned();
    assert!(
        publish_retained_inline_result(
            &operations,
            &current,
            2,
            &identity,
            &submission,
            &serde_json::to_vec(&mismatched).unwrap(),
            2001
        )
        .is_err()
    );
    assert_eq!(
        operations
            .read(&identity.author_target_identity_digest, &identity.operation_identifier)
            .unwrap(),
        recovering
    );
    let mut large = document.clone();
    let padding = "x".repeat(
        (slingshot_agent_connection::structured_job_result::maximum_inline_machine_result_bytes()
            / 1000) as usize
            + 1,
    );
    large.canonical_result = write_canonical(&serde_json::json!({"matches":(0..1000).map(|index|
        serde_json::json!({"repository_path":format!("/content/retained/{index:04}-{padding}")})).collect::<Vec<_>>()})).unwrap();
    assert!(
        publish_retained_inline_result(
            &operations,
            &current,
            2,
            &identity,
            &submission,
            &serde_json::to_vec(&large).unwrap(),
            2001
        )
        .unwrap()
        .is_none(),
        "local externalization must remain unpublished"
    );
    assert_eq!(
        operations
            .read(&identity.author_target_identity_digest, &identity.operation_identifier)
            .unwrap(),
        recovering
    );
    let artifact_database = OperationDatabase::open_live(
        &path,
        RequiredSettings {
            page_bytes: slingshot_domain::daemon_runtime_contract::DaemonRuntimeContract::embedded(
            )
            .limit("sqlite_page_bytes"),
            database_pages: 262144,
            busy_timeout_milliseconds:
                slingshot_domain::daemon_runtime_contract::DaemonRuntimeContract::embedded()
                    .limit("database_busy_timeout_milliseconds"),
        },
    )
    .unwrap();
    let store =
        slingshot_storage::artifact_store::ArtifactStore::open(&root.path().join("artifacts"))
            .unwrap();
    let capacity = slingshot_storage::persistent_capacity::PersistentCapacityAccount::new(
        &artifact_database,
        slingshot_domain::persistent_capacity::PersistentCapacityPolicy::embedded(),
    );
    let snapshot = slingshot_storage::agent_job_repository::SuccessfulAgentSnapshot {
        observation: slingshot_domain::remote_job::RemoteJobObservation {
            state: slingshot_domain::remote_job::AgentJobState::Succeeded,
            applied_sequence: slingshot_domain::remote_job::JobEventSequence::of(
                current.snapshot_watermark.value() + 1,
            ),
            attempt: 1,
            progress: COMPLETE_PROGRESS_PERCENT,
        },
        physical_sling_job_identifiers: vec!["job-one".to_owned()],
        remaining_retention_milliseconds: 120000,
    };
    let mut failed_handoff = snapshot.clone();
    failed_handoff.observation.state = slingshot_domain::remote_job::AgentJobState::Running;
    for attempt in 0..2 {
        assert!(
            slingshot_daemon::operation::artifact_completion::publish_retained_snapshot_result(
                &operations,
                &current,
                2,
                &identity,
                &submission,
                &serde_json::to_vec(&large).unwrap(),
                2001,
                &failed_handoff,
                &store,
                &capacity,
            )
            .is_err()
        );
        assert_eq!(
            capacity.pending_publications().unwrap(),
            1,
            "failed retries reuse the same hold"
        );
        assert_eq!(
            capacity.usage().unwrap().committed_artifact_bytes,
            large.canonical_result.len() as u64
        );
        assert_eq!(capacity.usage().unwrap().reserved_artifact_bytes, 0);
        assert_eq!(
            operations
                .read(&identity.author_target_identity_digest, &identity.operation_identifier)
                .unwrap(),
            recovering
        );
        if attempt == 0 {
            // Model a retained hold whose file never reached publication. The
            // retry must rebuild from freshly validated bytes, not trust presence.
            use sha2::Digest as _;
            let digest = hex::encode(sha2::Sha256::digest(large.canonical_result.as_bytes()));
            std::fs::remove_file(root.path().join("artifacts").join("content").join(digest))
                .unwrap();
        }
    }
    drop(capacity);
    drop(artifact_database);
    let artifact_database = OperationDatabase::open_live(
        &path,
        RequiredSettings {
            page_bytes: slingshot_domain::daemon_runtime_contract::DaemonRuntimeContract::embedded(
            )
            .limit("sqlite_page_bytes"),
            database_pages: 262144,
            busy_timeout_milliseconds:
                slingshot_domain::daemon_runtime_contract::DaemonRuntimeContract::embedded()
                    .limit("database_busy_timeout_milliseconds"),
        },
    )
    .unwrap();
    let capacity = slingshot_storage::persistent_capacity::PersistentCapacityAccount::new(
        &artifact_database,
        slingshot_domain::persistent_capacity::PersistentCapacityPolicy::embedded(),
    );
    let settled =
        slingshot_daemon::operation::artifact_completion::publish_retained_snapshot_result(
            &operations,
            &current,
            2,
            &identity,
            &submission,
            &serde_json::to_vec(&large).unwrap(),
            2001,
            &snapshot,
            &store,
            &capacity,
        )
        .unwrap()
        .unwrap();
    assert_eq!(settled.record.lifecycle_state, OperationLifecycleState::Succeeded);
    assert!(settled.result_inline_bytes.is_none());
    let retained_after = remote
        .read(
            &identity.author_target_identity_digest,
            &submission.operation.agent_operation_identifier,
        )
        .unwrap()
        .unwrap();
    assert!(
        retained_after.remaining_retention_milliseconds < snapshot.remaining_retention_milliseconds,
        "local externalization time must reduce upstream retention"
    );
    let artifact = slingshot_storage::artifact_store::ArtifactAssociations::new(&artifact_database)
        .read(
            &identity.author_target_identity_digest,
            &identity.operation_identifier,
            "structured_result",
        )
        .unwrap()
        .unwrap();
    assert_eq!(artifact.byte_length, large.canonical_result.len() as u64);
    let mut reader = store.open_verified(&artifact).unwrap();
    let mut received = Vec::new();
    let mut buffer = [0_u8; PEER_READ_BUFFER_BYTES];
    loop {
        let count = reader.read_into(&mut buffer).unwrap();
        if count == 0 {
            break;
        }
        received.extend_from_slice(&buffer[..count]);
    }
    reader.finish().unwrap();
    assert_eq!(received, large.canonical_result.as_bytes());
    assert_eq!(capacity.usage().unwrap().committed_artifact_bytes, artifact.byte_length);
    assert_eq!(capacity.usage().unwrap().reserved_artifact_bytes, 0);
    let holds = capacity.pending_publications().unwrap();
    assert_eq!(holds, 0, "successful association consumes its publication hold");
    assert_eq!(settled.record.revision, 3);
    assert!(settled.record.outstanding_recovery.is_none());
    assert!(
        publish_retained_inline_result(
            &operations,
            &current,
            2,
            &identity,
            &submission,
            &body,
            2002
        )
        .is_err()
    );
    assert!(operations.settle_success_for_retained_agent(&current, &settlement).is_err());
    assert_eq!(
        operations
            .read(&identity.author_target_identity_digest, &identity.operation_identifier)
            .unwrap(),
        Some(settled)
    );
}

#[test]
fn artifact_identity_is_derived_from_local_context_not_trusted_from_the_result() {
    let installation = InstallationIdentifier::parse(&"1".repeat(DIGEST_HEX_CHARACTERS)).unwrap();
    let identity = ExecutionIdentity {
        attempt: 1,
        author_target_identity_digest: "2".repeat(DIGEST_HEX_CHARACTERS),
        selected_environment_revision: "3".repeat(DIGEST_HEX_CHARACTERS),
        operation_identifier: "retained-operation".to_owned(),
    };
    let expected = ResultExpectation {
        operation: slingshot_agent_protocol::identity::WireOperationIdentity::of(
            &identity.author_target_identity_digest,
            &identity.selected_environment_revision,
            &identity.operation_identifier,
            slingshot_domain::agent_identity::AgentEventStoreGeneration::of(7),
        ),
        daemon_subscription_identifier: "retained-subscription".to_owned(),
        expected_provenance: ExpectedProvenance {
            canonical_json_contract_digest: canonical_contract_digest(),
            transport_contract_digest: AuthorAgentTransportContract::embedded_digest(),
            command_contract: SelectedCommandContractIdentity::installed(
                "download_content_package",
            )
            .unwrap(),
        },
        submitted_command_digest: "4".repeat(DIGEST_HEX_CHARACTERS),
        wire_name: "download_content_package".to_owned(),
    };
    let command: Command = serde_json::from_value(serde_json::json!({
        "command":"download_content_package", "package_name":"example", "roots":["/content/example"]
    }))
    .unwrap();
    let artifact_identifier = ArtifactIdentifier::derive(
        &installation,
        &identity.author_target_identity_digest,
        &identity.operation_identifier,
        "content_package",
    );
    let payload = serde_json::json!({"artifact":{
        "identifier":artifact_identifier.as_text(), "slot":"content_package", "media_type":"application/zip",
        "byte_length":DECLARED_ARTIFACT_BYTES, "digest":"5".repeat(DIGEST_HEX_CHARACTERS), "suggested_file_name":"example.zip"
    }});
    let document = TerminalResultDocument {
        operation: expected.operation.clone(),
        daemon_subscription_identifier: expected.daemon_subscription_identifier.clone(),
        canonical_result: write_canonical(&payload).unwrap(),
        declared_artifacts: vec![ArtifactEcho {
            byte_length: DECLARED_ARTIFACT_BYTES,
            media_type: "application/zip".to_owned(),
            slot: "content_package".to_owned(),
            suggested_name: "example.zip".to_owned(),
        }],
        provenance: expected.expected_provenance.provenance(),
        submitted_command_digest: expected.submitted_command_digest.clone(),
    };
    let body = serde_json::to_vec(&document).unwrap();
    let checked =
        decode_bound_result(&body, &expected, &command, &installation, &identity).unwrap();
    assert_eq!(
        checked.remote_artifact.unwrap().identifier.as_text(),
        artifact_identifier.as_text()
    );
    let other_installation =
        InstallationIdentifier::parse(&"6".repeat(DIGEST_HEX_CHARACTERS)).unwrap();
    assert!(
        decode_bound_result(&body, &expected, &command, &other_installation, &identity).is_err()
    );
    for change_target in [true, false] {
        let mut moved = identity.clone();
        if change_target {
            moved.author_target_identity_digest = "7".repeat(DIGEST_HEX_CHARACTERS);
        } else {
            moved.operation_identifier = "another-operation".to_owned();
        }
        assert!(decode_bound_result(&body, &expected, &command, &installation, &moved).is_err());
    }
    let mut changed = document;
    let mut payload = payload;
    payload["artifact"]["identifier"] = "private-canary".into();
    changed.canonical_result = write_canonical(&payload).unwrap();
    let refusal = decode_bound_result(
        &serde_json::to_vec(&changed).unwrap(),
        &expected,
        &command,
        &installation,
        &identity,
    )
    .unwrap_err();
    assert!(!format!("{refusal:?} {refusal}").contains("private-canary"));
}
