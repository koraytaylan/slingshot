use super::*;
use slingshot_domain::installation::InstallationIdentifier;
use slingshot_domain::operation::*;
use slingshot_storage::artifact_store::{
    ArtifactIdentifier, CANONICAL_JSON_MEDIA_TYPE, STRUCTURED_RESULT_SLOT,
};
use slingshot_storage::database::{OperationDatabase, RequiredSettings};

#[test]
fn maintenance_metadata_survives_removal_of_operations_and_database_reopen() {
    use base64::Engine as _;
    use slingshot_domain::persistent_capacity::PersistentCapacityPolicy;
    use slingshot_storage::artifact_store::{ArtifactStore, InstallationRequest};
    use slingshot_storage::maintenance_results::{
        record_application_result, record_current_preview,
    };
    use slingshot_storage::persistent_capacity::PersistentCapacityAccount;
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("maintenance-metadata.sqlite");
    let repository = open(&path);
    let target = served().author_target_identity_digest;
    let store = ArtifactStore::open(root.path()).unwrap();
    let blob = store
        .install(
            &InstallationRequest {
                artifact_slot: "content_package".to_owned(),
                author_target_identity_digest: target.clone(),
                descriptor: None,
                installation_identifier: installation(),
                media_type: CANONICAL_JSON_MEDIA_TYPE.to_owned(),
                operation_identifier: "operation".to_owned(),
            },
            &mut b"{}".as_slice(),
        )
        .unwrap();
    let content = blob.content_digest;
    admit(&repository, "operation");
    // An operation and maintenance share verified content. Maintenance access
    // must remain operation-free after that producing operation is removed.
    settle(
        &repository,
        "operation",
        Some("{}".to_owned()),
        vec![ProducedArtifact {
            artifact_identifier: ArtifactIdentifier::derive(
                &installation(),
                &target,
                "operation",
                "content_package",
            )
            .as_text()
            .to_owned(),
            artifact_slot: "content_package".to_owned(),
            byte_length: 2,
            content_digest: content.clone(),
            media_type: CANONICAL_JSON_MEDIA_TYPE.to_owned(),
        }],
    );
    let database = repository.database();
    let account = PersistentCapacityAccount::new(database, PersistentCapacityPolicy::embedded());
    let preview = slingshot_storage::maintenance::preview(database, &target, 3, 1).unwrap();
    assert_eq!(preview.released_operation_rows(), 1);
    let retained =
        record_current_preview(database, &account, &target, &preview.digest(), &content, 2)
            .unwrap()
            .current;
    let describe = |repository: &OperationRepository, identifier: &str| {
        let mut value = envelope();
        value["request"] = serde_json::json!({"request":"maintenance_result_metadata", "author_target_identity_digest":target, "maintenance_result_identifier":identifier});
        bind(&value).unwrap().maintenance_metadata(repository.database()).unwrap()
    };
    let OperationResponse::MaintenanceResultMetadata { description } =
        describe(&repository, retained.identifier.as_text())
    else {
        panic!("preview metadata")
    };
    assert_eq!(description.retention_owner, "current_preview");
    slingshot_storage::maintenance::apply(database, &preview, 4).unwrap();
    assert!(repository.read(&target, "operation").unwrap().is_none());
    let application =
        record_application_result(database, &account, &target, &preview.digest(), &content, 2)
            .unwrap()
            .result;
    let completed = slingshot_storage::maintenance::complete_cleanup_with_store(
        database,
        &store,
        &target,
        &preview.digest(),
    )
    .unwrap();
    assert_eq!(completed.stage, slingshot_storage::maintenance::ReceiptStage::Completed);
    assert!(root.path().join("content").join(&content).is_file());
    drop(account);
    drop(repository);
    let repository = open(&path);
    for (identifier, kind, revision) in [
        (retained.identifier.as_text(), "preview", 2),
        (application.identifier.as_text(), "application", 1),
    ] {
        let response = describe(&repository, identifier);
        let OperationResponse::MaintenanceResultMetadata { description } = &response else {
            panic!("retained metadata")
        };
        assert_eq!(description.author_target_identity_digest, target);
        assert_eq!(description.retention_owner, "application_receipt");
        assert_eq!(description.kind, kind);
        assert_eq!(description.association_revision, revision);
        assert_eq!(description.reviewed_source_digest, preview.digest());
        let wire = serde_json::to_value(response).unwrap();
        assert!(wire["description"].get("operation_identifier").is_none());
    }
    assert!(matches!(
        describe(&repository, &"f".repeat(DIGEST_OCTETS * 2)),
        OperationResponse::MissingMaintenanceResult { .. }
    ));
    assert!(matches!(
        describe(&repository, "private-invalid-identifier"),
        OperationResponse::MalformedFrame { .. }
    ));
    assert!(bind(&envelope()).unwrap().maintenance_metadata(repository.database()).is_none());
    let mut value = envelope();
    value["request"] = serde_json::json!({"request":"maintenance_result_read", "author_target_identity_digest":target,
        "maintenance_result_identifier":application.identifier.as_text(), "expected_content_digest":content,
        "preferred_chunk_bytes":1, "starting_byte_offset":0});
    for offset in 0..=2 {
        value["request"]["starting_byte_offset"] = offset.into();
        let mut stream =
            bind(&value).unwrap().maintenance_read(repository.database(), &store).unwrap().unwrap();
        assert!(matches!(stream.next(), Some(OperationResponse::MaintenanceResultStart { .. })));
        let mut bytes = Vec::new();
        let mut ended = false;
        for response in stream {
            match response {
                OperationResponse::MaintenanceResultChunk { body } => {
                    assert!(!ended);
                    assert_eq!(body.starting_byte_offset, offset + bytes.len() as u64);
                    bytes.extend(
                        base64::engine::general_purpose::STANDARD
                            .decode(body.encoded_bytes)
                            .unwrap(),
                    );
                }
                OperationResponse::MaintenanceResultEnd => {
                    assert!(!ended);
                    ended = true;
                }
                other => panic!("unexpected maintenance frame {other:?}"),
            }
        }
        assert!(ended);
        assert_eq!(bytes, b"{}"[offset as usize..]);
    }
    value["request"]["starting_byte_offset"] = 3.into();
    assert!(bind(&value).unwrap().maintenance_read(repository.database(), &store).is_err());
    value["request"]["starting_byte_offset"] = 0.into();
    value["request"]["expected_content_digest"] = "f".repeat(DIGEST_OCTETS * 2).into();
    assert!(bind(&value).unwrap().maintenance_read(repository.database(), &store).is_err());
    value["request"]["expected_content_digest"] = content.clone().into();
    let before = describe(&repository, application.identifier.as_text());
    let mut stream =
        bind(&value).unwrap().maintenance_read(repository.database(), &store).unwrap().unwrap();
    assert!(matches!(stream.next(), Some(OperationResponse::MaintenanceResultStart { .. })));
    std::fs::OpenOptions::new()
        .write(true)
        .open(root.path().join("content").join(&content))
        .unwrap()
        .set_len(0)
        .unwrap();
    let frames: Vec<_> = stream.collect();
    assert!(matches!(frames.last(), Some(OperationResponse::InternalFailure { .. })));
    assert!(!frames.iter().any(|frame| matches!(frame, OperationResponse::MaintenanceResultEnd)));
    assert!(bind(&value).unwrap().maintenance_read(repository.database(), &store).is_err());
    assert_eq!(describe(&repository, application.identifier.as_text()), before);
}

#[tokio::test]
async fn bound_wait_reads_persisted_state_before_registering_and_refuses_without_capacity_leaks() {
    use crate::operation_wait::runtime::RuntimeWaiters;
    use crate::operation_wait::{WaitBounds, WaitUpdate};
    use tokio_util::sync::CancellationToken;
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("wait.sqlite");
    let repository = open(&path);
    admit(&repository, "operation");
    drop(repository);
    let repository = open(&path);
    let waiters = RuntimeWaiters::new(CancellationToken::new());
    let mut value = envelope();
    value["request"] = serde_json::json!({"request":"wait", "operation_identifier":"operation"});
    let mut reader = bind(&value).unwrap().wait(&repository, &waiters).unwrap().unwrap();
    assert_eq!(
        reader.next(&CancellationToken::new()).await,
        Some(WaitUpdate::Progress { detail: "queued".to_owned(), revision: 1 })
    );
    assert_eq!(waiters.attached(), 1);
    drop(reader);
    for identifier in ["", "bad\0identifier", "missing"] {
        value["request"]["operation_identifier"] = identifier.into();
        assert!(bind(&value).unwrap().wait(&repository, &waiters).is_err());
        assert_eq!(waiters.attached(), 0);
    }
    value["request"]["operation_identifier"] = "operation".into();
    value["request"]["observed_revision"] = 2.into();
    assert!(matches!(
        bind(&value).unwrap().wait(&repository, &waiters),
        Err(OperationResponse::MalformedFrame { .. })
    ));
    assert_eq!(waiters.attached(), 0);
    value["request"]["observed_revision"] = 1.into();
    let count = WaitBounds::embedded()
        .waiters_per_operation
        .min(u64::from(FoundationContract::embedded().server.connection_capacity));
    let mut readers = Vec::new();
    for _ in 0..count {
        readers.push(bind(&value).unwrap().wait(&repository, &waiters).unwrap().unwrap());
    }
    assert!(matches!(
        bind(&value).unwrap().wait(&repository, &waiters),
        Err(OperationResponse::WaiterCapacityExhausted { .. })
    ));
    assert_eq!(
        repository
            .read(&served().author_target_identity_digest, "operation")
            .unwrap()
            .unwrap()
            .record
            .revision,
        1
    );
    drop(readers);
    settle(&repository, "operation", Some("{}".to_owned()), vec![]);
    let mut reader = bind(&value).unwrap().wait(&repository, &waiters).unwrap().unwrap();
    assert_eq!(
        reader.next(&CancellationToken::new()).await,
        Some(WaitUpdate::Terminal { revision: 2 })
    );
    assert_eq!(waiters.attached(), 0);
    value["request"]["observed_revision"] = 2.into();
    let mut reader = bind(&value).unwrap().wait(&repository, &waiters).unwrap().unwrap();
    assert!(reader.next(&CancellationToken::new()).await.is_none());
    assert_eq!(waiters.attached(), 0);
}

#[test]
fn artifact_wire_stream_is_target_qualified_resumable_and_finishes_only_once() {
    use base64::Engine as _;
    use slingshot_storage::artifact_store::{ArtifactStore, InstallationRequest};
    let root = tempfile::tempdir().unwrap();
    let repository = open(&root.path().join("stream.sqlite"));
    let store = ArtifactStore::open(root.path()).unwrap();
    let target = served().author_target_identity_digest;
    admit(&repository, "operation");
    admit(&repository, "other");
    let content = b"verified artifact content";
    let metadata = store
        .install(
            &InstallationRequest {
                artifact_slot: "command_artifact".to_owned(),
                author_target_identity_digest: target.clone(),
                descriptor: None,
                installation_identifier: installation(),
                media_type: "application/octet-stream".to_owned(),
                operation_identifier: "operation".to_owned(),
            },
            &mut content.as_slice(),
        )
        .unwrap();
    settle(
        &repository,
        "operation",
        Some("{}".to_owned()),
        vec![ProducedArtifact {
            artifact_identifier: metadata.artifact_identifier.as_text().to_owned(),
            artifact_slot: metadata.artifact_slot.clone(),
            byte_length: metadata.byte_length,
            content_digest: metadata.content_digest.clone(),
            media_type: metadata.media_type.clone(),
        }],
    );
    settle(&repository, "other", Some("{}".to_owned()), vec![]);
    let before = repository.read(&target, "operation").unwrap();
    let mut value = envelope();
    value["request"] = serde_json::json!({"request":"artifact_read", "operation_identifier":"operation", "artifact_identifier":metadata.artifact_identifier.as_text(), "expected_content_digest":metadata.content_digest, "preferred_chunk_bytes":3, "starting_byte_offset":0});
    for offset in [0, 1, content.len()] {
        value["request"]["starting_byte_offset"] = offset.into();
        let mut stream =
            bind(&value).unwrap().artifact(&repository, &installation(), &store).unwrap().unwrap();
        assert!(matches!(stream.next(), Some(OperationResponse::ArtifactStart { .. })));
        let mut actual = Vec::new();
        let mut end = false;
        for frame in stream.by_ref() {
            match frame {
                OperationResponse::ArtifactChunk { body } => {
                    assert!(!end);
                    assert_eq!(body.starting_byte_offset as usize, offset + actual.len());
                    let bytes = base64::engine::general_purpose::STANDARD
                        .decode(body.encoded_bytes)
                        .unwrap();
                    assert!(bytes.len() <= 3);
                    actual.extend(bytes);
                }
                OperationResponse::ArtifactEnd => {
                    assert!(!end);
                    end = true;
                }
                other => panic!("unexpected {other:?}"),
            }
        }
        assert!(end);
        assert_eq!(actual, content[offset..]);
        assert!(stream.next().is_none());
    }
    value["request"]["operation_identifier"] = "other".into();
    assert!(bind(&value).unwrap().artifact(&repository, &installation(), &store).is_err());
    value["request"]["operation_identifier"] = "operation".into();
    value["request"]["starting_byte_offset"] = (content.len() + 1).into();
    assert!(matches!(
        bind(&value).unwrap().artifact(&repository, &installation(), &store),
        Err(OperationResponse::MalformedFrame { .. })
    ));
    value["request"]["starting_byte_offset"] = 0.into();
    value["request"]["expected_content_digest"] = "f".repeat(DIGEST_OCTETS * 2).into();
    assert!(matches!(
        bind(&value).unwrap().artifact(&repository, &installation(), &store),
        Err(OperationResponse::MalformedFrame { .. })
    ));
    value["request"]["expected_content_digest"] = metadata.content_digest.clone().into();
    let mut stream =
        bind(&value).unwrap().artifact(&repository, &installation(), &store).unwrap().unwrap();
    assert!(matches!(stream.next(), Some(OperationResponse::ArtifactStart { .. })));
    // Mutation after the verified start must never be followed by ArtifactEnd.
    std::fs::OpenOptions::new()
        .write(true)
        .open(root.path().join("content").join(&metadata.content_digest))
        .unwrap()
        .set_len(0)
        .unwrap();
    let responses: Vec<_> = stream.collect();
    assert!(matches!(responses.last(), Some(OperationResponse::InternalFailure { .. })));
    assert!(!responses.iter().any(|response| matches!(response, OperationResponse::ArtifactEnd)));
    assert!(bind(&value).unwrap().artifact(&repository, &installation(), &store).is_err());
    assert_eq!(repository.read(&target, "operation").unwrap(), before);
}

fn installation() -> InstallationIdentifier {
    InstallationIdentifier::parse(&"d".repeat(DIGEST_OCTETS * 2)).unwrap()
}

fn open(path: &std::path::Path) -> OperationRepository {
    let limits = DaemonRuntimeContract::embedded();
    OperationRepository::new(
        OperationDatabase::open(
            path,
            RequiredSettings {
                page_bytes: limits.limit("sqlite_page_bytes"),
                database_pages: limits.limit("maximum_sqlite_database_pages"),
                busy_timeout_milliseconds: limits.limit("database_busy_timeout_milliseconds"),
            },
        )
        .unwrap(),
    )
}

fn admit(repository: &OperationRepository, identifier: &str) {
    let mut value = execute_envelope();
    value["request"]["operation_identifier"] = identifier.into();
    bind(&value)
        .unwrap()
        .prepare_admission(&installation())
        .unwrap()
        .unwrap()
        .persist(repository, 1)
        .unwrap();
}

fn answer(repository: &OperationRepository, identifier: &str) -> OperationResponse {
    let mut value = envelope();
    value["request"] = serde_json::json!({"request":"result", "operation_identifier":identifier});
    bind(&value).unwrap().result(repository, &installation()).unwrap()
}

fn settle(
    repository: &OperationRepository,
    identifier: &str,
    inline: Option<String>,
    artifacts: Vec<ProducedArtifact>,
) {
    repository
        .settle_success(
            &served().author_target_identity_digest,
            identifier,
            &SuccessfulSettlement {
                artifacts,
                inline_result: inline,
                expected_lifecycle_state: OperationLifecycleState::Queued,
                expected_revision: 1,
                settled_at_unix_milliseconds: 2,
            },
        )
        .unwrap();
}

#[test]
fn pending_inline_and_invalid_retained_results_are_read_without_mutation_after_reopen() {
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("results.sqlite");
    let repository = open(&path);
    admit(&repository, "pending");
    assert_eq!(
        answer(&repository, "pending"),
        OperationResponse::Status {
            lifecycle_state: "queued".to_owned(),
            operation_identifier: "pending".to_owned(),
            operation_revision: 1,
        }
    );
    let maximum =
        DaemonRuntimeContract::embedded().limit("maximum_inline_machine_result_bytes") as usize;
    let exact = format!("\"{}\"", "x".repeat(maximum - 2));
    for (identifier, text) in [
        ("inline", "{\"paths\":[]}"),
        ("exact", exact.as_str()),
        ("invalid", "not JSON"),
        ("duplicate", "{\"a\":1,\"a\":1}"),
        ("noncanonical", "{ \"a\":1}"),
    ] {
        admit(&repository, identifier);
        settle(&repository, identifier, Some(text.to_owned()), vec![]);
    }
    drop(repository);
    let repository = open(&path);
    for identifier in ["inline", "exact", "invalid", "duplicate", "noncanonical"] {
        let before = repository.read(&served().author_target_identity_digest, identifier).unwrap();
        let response = answer(&repository, identifier);
        if matches!(identifier, "inline" | "exact") {
            let OperationResponse::ResultInline { operation_identifier, result } = response else {
                panic!("inline success")
            };
            assert_eq!(operation_identifier, identifier);
            assert_eq!(
                serde_json::to_string(&result).unwrap(),
                before.as_ref().unwrap().result_inline_bytes.as_ref().unwrap().as_str()
            );
        } else {
            assert!(matches!(response, OperationResponse::InternalFailure { .. }));
            assert!(!serde_json::to_string(&response).unwrap().contains("JSON"));
        }
        assert_eq!(
            repository.read(&served().author_target_identity_digest, identifier).unwrap(),
            before
        );
    }
    assert!(matches!(answer(&repository, "missing"), OperationResponse::MissingOperation { .. }));
    for identifier in ["", "bad\0identifier"] {
        assert!(matches!(
            answer(&repository, identifier),
            OperationResponse::MalformedFrame { .. }
        ));
    }
    assert!(bind(&envelope()).unwrap().result(&repository, &installation()).is_none());
}

#[test]
fn artifact_result_requires_the_structured_slot_and_installation_bound_identity() {
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("artifacts.sqlite");
    let repository = open(&path);
    let target = served().author_target_identity_digest;
    for (identifier, slot, foreign) in [
        ("artifact", STRUCTURED_RESULT_SLOT, false),
        ("missing-slot", "command_artifact", false),
        ("foreign", STRUCTURED_RESULT_SLOT, true),
    ] {
        admit(&repository, identifier);
        let artifact_identifier = ArtifactIdentifier::derive(
            &installation(),
            &target,
            if foreign { "another-operation" } else { identifier },
            slot,
        )
        .as_text()
        .to_owned();
        settle(
            &repository,
            identifier,
            None,
            vec![ProducedArtifact {
                artifact_identifier,
                artifact_slot: slot.to_owned(),
                byte_length: DaemonRuntimeContract::embedded()
                    .limit("maximum_inline_machine_result_bytes")
                    + 1,
                content_digest: "e".repeat(DIGEST_OCTETS * 2),
                media_type: CANONICAL_JSON_MEDIA_TYPE.to_owned(),
            }],
        );
    }
    drop(repository);
    let repository = open(&path);
    for identifier in ["artifact", "missing-slot", "foreign"] {
        let before = repository.read(&target, identifier).unwrap();
        let response = answer(&repository, identifier);
        if identifier == "artifact" {
            assert!(
                matches!(response, OperationResponse::ResultArtifact { operation_identifier, .. } if operation_identifier == identifier)
            );
        } else {
            assert!(matches!(response, OperationResponse::InternalFailure { .. }));
        }
        assert_eq!(repository.read(&target, identifier).unwrap(), before);
    }
}

#[test]
fn recovery_and_terminal_results_preserve_the_domains_conditional_evidence() {
    let root = tempfile::tempdir().unwrap();
    let repository = open(&root.path().join("evidence.sqlite"));
    let target = served().author_target_identity_digest;
    for (identifier, category, evidence) in [
        (
            "unknown",
            RecoveryCategory::OperationLookup,
            RecoveryExecutionEvidence::ExecutionCertainty {
                certainty: OperationExecutionCertainty::SubmissionUnknown,
            },
        ),
        (
            "proven",
            RecoveryCategory::ResultAcquisition,
            RecoveryExecutionEvidence::AuthoritativeRemoteSuccess,
        ),
    ] {
        admit(&repository, identifier);
        repository
            .apply(
                &target,
                identifier,
                1,
                &OperationFact::Recovery {
                    recovery: RecoveryFact {
                        attempt_count: 1,
                        category,
                        detail: "paused".to_owned(),
                        evidence,
                        manual_resume_eligible: true,
                        retry_delay_milliseconds: 0,
                        retry_observed_at_unix_milliseconds: 2,
                    },
                },
                2,
            )
            .unwrap();
        let OperationResponse::RecoveryRequired { evidence: actual, .. } =
            answer(&repository, identifier)
        else {
            panic!("recovery response")
        };
        assert_eq!(serde_json::to_value(actual).unwrap(), serde_json::to_value(evidence).unwrap());
    }
    let nonexecution = TerminalFailureDisposition::AuthoritativeNonExecution {
        certainty: OperationExecutionCertainty::ConfirmedNotExecuted,
    };
    let unknown = TerminalFailureDisposition::FailClosedIndeterminate {
        certainty: OperationExecutionCertainty::RemoteOutcomeUnknown,
    };
    for (index, (kind, disposition)) in [
        (TerminalFailureKind::Rejected, nonexecution),
        (TerminalFailureKind::RemoteFailed, TerminalFailureDisposition::AuthoritativeRemoteFailure),
        (
            TerminalFailureKind::ResultUnavailable,
            TerminalFailureDisposition::AuthoritativeRemoteSuccess,
        ),
        (TerminalFailureKind::RecoveryWindowExpired, unknown),
        (TerminalFailureKind::RemoteStateLost, unknown),
        (TerminalFailureKind::IntegrityFailure, unknown),
        (TerminalFailureKind::RetryPolicyExhausted, nonexecution),
    ]
    .into_iter()
    .enumerate()
    {
        let identifier = format!("terminal-{index}");
        admit(&repository, &identifier);
        repository
            .apply(
                &target,
                &identifier,
                1,
                &OperationFact::Terminal {
                    failure: TerminalFailure { kind, disposition, metadata: None },
                },
                2,
            )
            .unwrap();
        let OperationResponse::TerminalFailure {
            kind: actual_kind,
            disposition: actual_disposition,
            ..
        } = answer(&repository, &identifier)
        else {
            panic!("terminal response")
        };
        assert_eq!(serde_json::to_value(actual_kind).unwrap(), serde_json::to_value(kind).unwrap());
        assert_eq!(
            serde_json::to_value(actual_disposition).unwrap(),
            serde_json::to_value(disposition).unwrap()
        );
    }
}
