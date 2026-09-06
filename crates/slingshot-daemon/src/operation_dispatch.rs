//! Bind wire requests before permitting access to one retained target partition.

use slingshot_local_protocol::foundation_contract::FoundationContract;
use slingshot_local_protocol::message::{OperationEnvelope, OperationRequest, OperationResponse};
use slingshot_local_protocol::operation_decode;
use slingshot_storage::operation_repository::OperationRepository;

use crate::operation_queries::{self, QueryFailure};
use crate::operation_submission::ServedTarget;

mod admission;
mod artifact;
mod listing;
mod maintenance;
mod recovery;
mod result;
mod wait;
pub use admission::PreparedAdmission;
pub use artifact::ArtifactResponseStream;
pub use maintenance::MaintenanceResponseStream;

/// A request whose version and selected-runtime identities were all checked.
/// Fields stay private so a dispatcher cannot manufacture an unchecked request.
#[derive(Debug)]
pub struct BoundRequest {
    envelope: OperationEnvelope,
    execution_available: bool,
}

impl BoundRequest {
    /// Validates a wire request without reading or changing the repository.
    ///
    /// Compatibility precedes interpretation of the request vocabulary; target,
    /// environment revision, and runtime contract precede every query or write.
    ///
    /// # Errors
    ///
    /// Returns a typed, public-safe protocol refusal. No parser excerpt or
    /// client-supplied binding is reflected in its diagnostic.
    pub fn decode(
        foundation: &FoundationContract,
        served: &ServedTarget,
        installed_versions: &[u32],
        payload: &[u8],
    ) -> Result<Self, OperationResponse> {
        let version =
            operation_decode::protocol_version(foundation, payload).map_err(|_| malformed())?;
        if !installed_versions.contains(&version) {
            return Err(OperationResponse::IncompatibleOperationProtocol {
                supported_operation_protocol_versions: installed_versions.to_vec(),
            });
        }
        let envelope = operation_decode::decode(foundation, payload).map_err(|_| malformed())?;
        if envelope.author_target_identity_digest != served.author_target_identity_digest {
            return Err(OperationResponse::TargetMismatch {
                author_target_identity_digest: served.author_target_identity_digest.clone(),
            });
        }
        if envelope.selected_environment_revision != served.selected_environment_revision {
            return Err(OperationResponse::RevisionMismatch {
                selected_environment_revision: served.selected_environment_revision.clone(),
            });
        }
        if envelope.daemon_runtime_contract_digest != served.daemon_runtime_contract_digest {
            return Err(OperationResponse::RuntimeContractDigestMismatch {
                daemon_runtime_contract_digest: served.daemon_runtime_contract_digest.clone(),
            });
        }
        // Maintenance has an operation-free address inside its request. It
        // cannot override the partition already selected by the envelope.
        let addressed_target = match &envelope.request {
            OperationRequest::MaintenanceResultMetadata {
                author_target_identity_digest, ..
            }
            | OperationRequest::MaintenanceResultRead { author_target_identity_digest, .. }
            | OperationRequest::TerminalMaintenancePreview {
                author_target_identity_digest, ..
            }
            | OperationRequest::TerminalMaintenanceApply {
                author_target_identity_digest, ..
            } => Some(author_target_identity_digest),
            _ => None,
        };
        if addressed_target.is_some_and(|target| target != &served.author_target_identity_digest) {
            return Err(OperationResponse::TargetMismatch {
                author_target_identity_digest: served.author_target_identity_digest.clone(),
            });
        }
        Ok(Self { envelope, execution_available: served.execution_available })
    }

    /// The closed request to dispatch after its enclosing identities matched.
    #[must_use]
    pub fn request(&self) -> &OperationRequest {
        &self.envelope.request
    }

    /// Answers status from the retained row, never from a cached admission.
    /// Returns `None` only when this request belongs to a different handler.
    #[must_use]
    pub fn status(&self, repository: &OperationRepository) -> Option<OperationResponse> {
        let OperationRequest::OperationStatus { operation_identifier } = self.request() else {
            return None;
        };
        if operation_identifier.is_empty() {
            return Some(malformed());
        }
        Some(
            match operation_queries::status(
                repository,
                &self.envelope.author_target_identity_digest,
                operation_identifier,
            ) {
                Ok(status) => {
                    // The domain's serde spelling is also the persisted vocabulary.
                    match serde_json::to_value(status.lifecycle_state) {
                        Ok(serde_json::Value::String(lifecycle_state)) => {
                            OperationResponse::Status {
                                lifecycle_state,
                                operation_identifier: status.operation_identifier,
                                operation_revision: status.revision,
                            }
                        }
                        _ => internal_failure(),
                    }
                }
                Err(QueryFailure::NoSuchOperation { .. }) => OperationResponse::MissingOperation {
                    operation_identifier: operation_identifier.clone(),
                },
                Err(_) => internal_failure(),
            },
        )
    }
}

fn malformed() -> OperationResponse {
    OperationResponse::MalformedFrame {
        detail: "the operation request is malformed or exceeds its declared bounds".to_owned(),
    }
}

fn internal_failure() -> OperationResponse {
    OperationResponse::InternalFailure {
        detail: "the retained operation could not be read".to_owned(),
    }
}

#[cfg(test)]
mod tests {
    mod results;
    use super::*;
    use slingshot_domain::daemon_runtime_contract::{DIGEST_OCTETS, DaemonRuntimeContract};

    fn served() -> ServedTarget {
        ServedTarget {
            author_target_identity_digest: "a".repeat(DIGEST_OCTETS * 2),
            selected_environment_revision: "b".repeat(DIGEST_OCTETS * 2),
            daemon_runtime_contract_digest: "c".repeat(DIGEST_OCTETS * 2),
            execution_available: true,
        }
    }

    fn envelope() -> serde_json::Value {
        let target = served();
        serde_json::json!({
            "author_target_identity_digest": target.author_target_identity_digest,
            "selected_environment_revision": target.selected_environment_revision,
            "daemon_runtime_contract_digest": target.daemon_runtime_contract_digest,
            "operation_protocol_version": DaemonRuntimeContract::embedded().operation_protocol_version,
            "request_identifier": "query",
            "request": { "request": "operation_status", "operation_identifier": "operation" }
        })
    }

    fn bind(value: &serde_json::Value) -> Result<BoundRequest, OperationResponse> {
        BoundRequest::decode(
            &FoundationContract::embedded(),
            &served(),
            &[DaemonRuntimeContract::embedded().operation_protocol_version as u32],
            &serde_json::to_vec(value).unwrap(),
        )
    }

    #[test]
    fn exact_binding_is_required_before_a_request_can_reach_a_repository() {
        assert!(bind(&envelope()).is_ok());
        for field in [
            "author_target_identity_digest",
            "selected_environment_revision",
            "daemon_runtime_contract_digest",
        ] {
            let mut value = envelope();
            value[field] = "f".repeat(DIGEST_OCTETS * 2).into();
            let response = bind(&value).unwrap_err();
            assert!(match field {
                "author_target_identity_digest" =>
                    matches!(response, OperationResponse::TargetMismatch { .. }),
                "selected_environment_revision" =>
                    matches!(response, OperationResponse::RevisionMismatch { .. }),
                _ => matches!(response, OperationResponse::RuntimeContractDigestMismatch { .. }),
            });
            assert!(
                !serde_json::to_string(&response).unwrap().contains(&"f".repeat(DIGEST_OCTETS * 2))
            );
        }
    }

    #[test]
    fn adjacent_versions_are_refused_without_interpreting_their_vocabulary() {
        let version = DaemonRuntimeContract::embedded().operation_protocol_version;
        for asked in [version - 1, version + 1] {
            let mut value = envelope();
            value["operation_protocol_version"] = asked.into();
            value["request"] = serde_json::json!({"future_request":"not understood"});
            assert!(matches!(
                bind(&value),
                Err(OperationResponse::IncompatibleOperationProtocol { .. })
            ));
        }
        let mut value = envelope();
        value["request"] = serde_json::json!({"future_request":"not understood"});
        assert!(matches!(bind(&value), Err(OperationResponse::MalformedFrame { .. })));
    }

    #[test]
    fn malformed_headers_and_an_uninstalled_surface_never_reach_status() {
        let foundation = FoundationContract::embedded();
        let payload = serde_json::to_vec(&envelope()).unwrap();
        assert_eq!(
            BoundRequest::decode(&foundation, &served(), &[], &payload).unwrap_err(),
            OperationResponse::IncompatibleOperationProtocol {
                supported_operation_protocol_versions: vec![],
            }
        );
        let version = DaemonRuntimeContract::embedded().operation_protocol_version as u32;
        for payload in [
            format!(
                r#"{{"operation_protocol_version":{version},"operation_protocol_version":{version}}}"#
            ),
            format!(
                r#"{{"operation_protocol_version":{version},"request":{{"secret":1,"\u0073ecret":2}}}}"#
            ),
        ] {
            let refused =
                BoundRequest::decode(&foundation, &served(), &[version], payload.as_bytes())
                    .unwrap_err();
            assert!(matches!(refused, OperationResponse::MalformedFrame { .. }));
            assert!(!serde_json::to_string(&refused).unwrap().contains("secret"));
        }
    }

    fn execute_envelope() -> serde_json::Value {
        let mut value = envelope();
        value["request"] = serde_json::json!({
            "request": "execute",
            "command": { "command": "query_paths", "root_path": "/content/example" },
            "operation_identifier": "operation",
            "workflow_correlation_identifier": "original-workflow"
        });
        value
    }

    #[test]
    fn every_operation_free_maintenance_address_must_match_the_outer_target() {
        for request in [
            serde_json::json!({"request":"maintenance_result_metadata", "maintenance_result_identifier":"result"}),
            serde_json::json!({"request":"maintenance_result_read", "maintenance_result_identifier":"result", "expected_content_digest":"e".repeat(DIGEST_OCTETS * 2), "preferred_chunk_bytes":1, "starting_byte_offset":0}),
            serde_json::json!({"request":"terminal_maintenance_preview", "before_unix_milliseconds":1, "maximum_operations":1}),
            serde_json::json!({"request":"terminal_maintenance_apply", "reviewed_manifest_digest":"e".repeat(DIGEST_OCTETS * 2)}),
        ] {
            let mut value = envelope();
            value["request"] = request;
            value["request"]["author_target_identity_digest"] =
                served().author_target_identity_digest.into();
            assert!(bind(&value).is_ok());
            for foreign in
                ["f".repeat(DIGEST_OCTETS * 2), "never-echo-this-target".to_owned(), String::new()]
            {
                value["request"]["author_target_identity_digest"] = foreign.into();
                let response = bind(&value).unwrap_err();
                assert_eq!(
                    response,
                    OperationResponse::TargetMismatch {
                        author_target_identity_digest: served().author_target_identity_digest,
                    }
                );
                assert!(!serde_json::to_string(&response).unwrap().contains("never-echo"));
            }
        }
    }

    #[test]
    fn admission_commits_before_acceptance_and_replays_after_reopen_at_capacity() {
        use slingshot_domain::installation::InstallationIdentifier;
        use slingshot_domain::persistent_capacity::PersistentCapacityPolicy;
        use slingshot_storage::database::{OperationDatabase, RequiredSettings};
        use slingshot_storage::operation_repository::RepositoryFailure;
        use slingshot_storage::persistent_capacity::AccountingFailure;

        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("admissions.sqlite");
        let limits = DaemonRuntimeContract::embedded();
        let open = || {
            OperationRepository::bounded(
                OperationDatabase::open(
                    &path,
                    RequiredSettings {
                        page_bytes: limits.limit("sqlite_page_bytes"),
                        database_pages: limits.limit("maximum_sqlite_database_pages"),
                        busy_timeout_milliseconds: limits
                            .limit("database_busy_timeout_milliseconds"),
                    },
                )
                .unwrap(),
                PersistentCapacityPolicy {
                    retained_operation_rows: 1,
                    ..PersistentCapacityPolicy::embedded()
                },
            )
        };
        let installation = InstallationIdentifier::parse(&"d".repeat(DIGEST_OCTETS * 2)).unwrap();
        let mut value = execute_envelope();
        let prepare = |value: &serde_json::Value| {
            bind(value).unwrap().prepare_admission(&installation).unwrap().unwrap()
        };
        let repository = open();
        assert!(matches!(
            prepare(&value).persist(&repository, 1).unwrap(),
            OperationResponse::Accepted { .. }
        ));
        let committed =
            repository.read(&served().author_target_identity_digest, "operation").unwrap().unwrap();
        assert_eq!(committed.record.revision, 1);
        let input = repository
            .read_execution_input(&served().author_target_identity_digest, "operation")
            .unwrap()
            .unwrap();
        let arguments: serde_json::Value = serde_json::from_str(&input.canonical_command).unwrap();
        assert_eq!(arguments["root_path"], "/content/example");
        assert!(arguments.get("command").is_none());
        drop(repository);
        let repository = open();
        value["request_identifier"] = "another-connection".into();
        value["request"]["workflow_correlation_identifier"] = "another-workflow".into();
        assert!(matches!(
            prepare(&value).persist(&repository, 2).unwrap(),
            OperationResponse::Replayed { .. }
        ));
        assert_eq!(
            repository.read(&served().author_target_identity_digest, "operation").unwrap().unwrap(),
            committed
        );
        value["request"]["command"]["root_path"] = "/different-work".into();
        assert!(matches!(
            prepare(&value).persist(&repository, 3).unwrap(),
            OperationResponse::IdentifierConflict { .. }
        ));
        value["request"]["operation_identifier"] = "second-operation".into();
        assert!(matches!(
            prepare(&value).persist(&repository, 4),
            Err(RepositoryFailure::Capacity(AccountingFailure::Refused(_)))
        ));
        assert!(
            repository
                .read(&served().author_target_identity_digest, "second-operation")
                .unwrap()
                .is_none()
        );
        assert_eq!(
            repository.read(&served().author_target_identity_digest, "operation").unwrap().unwrap(),
            committed
        );
    }

    #[test]
    fn unavailable_and_invalid_commands_cannot_be_prepared_for_admission() {
        use slingshot_domain::installation::InstallationIdentifier;
        let installation = InstallationIdentifier::parse(&"d".repeat(DIGEST_OCTETS * 2)).unwrap();
        let mut value = execute_envelope();
        value["request"]["command"] = serde_json::json!({"secret": "never echo this"});
        let unavailable = ServedTarget { execution_available: false, ..served() };
        let request = BoundRequest::decode(
            &FoundationContract::embedded(),
            &unavailable,
            &[DaemonRuntimeContract::embedded().operation_protocol_version as u32],
            &serde_json::to_vec(&value).unwrap(),
        )
        .unwrap();
        assert_eq!(
            request.prepare_admission(&installation).unwrap_err(),
            OperationResponse::ExecutorUnavailable
        );
        let refusal = bind(&value).unwrap().prepare_admission(&installation).unwrap_err();
        assert!(matches!(refusal, OperationResponse::MalformedFrame { .. }));
        assert!(!serde_json::to_string(&refusal).unwrap().contains("secret"));
        for (field, invalid) in [
            ("operation_identifier", String::new()),
            ("operation_identifier", "embedded\0separator".to_owned()),
            (
                "workflow_correlation_identifier",
                "x".repeat(
                    DaemonRuntimeContract::embedded()
                        .limit("maximum_workflow_correlation_identifier_bytes")
                        as usize
                        + 1,
                ),
            ),
        ] {
            let mut value = execute_envelope();
            value["request"][field] = invalid.into();
            assert!(matches!(
                bind(&value).unwrap().prepare_admission(&installation),
                Err(OperationResponse::MalformedFrame { .. })
            ));
        }
        let mut value = execute_envelope();
        value["request"]["command"]["root_path"] = "not-an-absolute-path".into();
        assert!(matches!(
            bind(&value).unwrap().prepare_admission(&installation),
            Err(OperationResponse::MalformedFrame { .. })
        ));
    }

    #[test]
    fn scheduled_admission_uses_embedded_limits_and_preserves_replay() {
        use slingshot_domain::installation::InstallationIdentifier;
        use slingshot_storage::database::{OperationDatabase, RequiredSettings};
        let limits = DaemonRuntimeContract::embedded();
        let repository = OperationRepository::new(
            OperationDatabase::open_in_memory(RequiredSettings {
                page_bytes: limits.limit("sqlite_page_bytes"),
                database_pages: limits.limit("maximum_sqlite_database_pages"),
                busy_timeout_milliseconds: limits.limit("database_busy_timeout_milliseconds"),
            })
            .unwrap(),
        );
        let installation = InstallationIdentifier::parse(&"d".repeat(DIGEST_OCTETS * 2)).unwrap();
        let active = std::collections::BTreeSet::new();
        let prepare = |identifier: &str| {
            let mut value = execute_envelope();
            value["request"]["operation_identifier"] = identifier.into();
            bind(&value).unwrap().prepare_admission(&installation).unwrap().unwrap()
        };
        for number in 0..limits.limit("maximum_pending_operations_per_caller") {
            assert!(matches!(
                prepare(&format!("operation-{number}"))
                    .persist_scheduled(&repository, &active, 1)
                    .unwrap(),
                OperationResponse::Accepted { .. }
            ));
        }
        assert!(matches!(
            prepare("overflow").persist_scheduled(&repository, &active, 2).unwrap(),
            OperationResponse::SchedulerCapacityExhausted { .. }
        ));
        assert!(
            repository.read(&served().author_target_identity_digest, "overflow").unwrap().is_none()
        );
        assert!(matches!(
            prepare("operation-0").persist_scheduled(&repository, &active, 3).unwrap(),
            OperationResponse::Replayed { .. }
        ));
        let active = std::collections::BTreeSet::from(["operation-0".to_owned()]);
        assert!(matches!(
            prepare("overflow").persist_scheduled(&repository, &active, 4).unwrap(),
            OperationResponse::Accepted { .. }
        ));
    }

    #[test]
    fn filtered_listing_continues_after_reopen_without_repeating_newer_work() {
        use slingshot_domain::installation::InstallationIdentifier;
        use slingshot_storage::database::{OperationDatabase, RequiredSettings};
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("listing.sqlite");
        let limits = DaemonRuntimeContract::embedded();
        let open = || {
            OperationRepository::new(
                OperationDatabase::open(
                    &path,
                    RequiredSettings {
                        page_bytes: limits.limit("sqlite_page_bytes"),
                        database_pages: limits.limit("maximum_sqlite_database_pages"),
                        busy_timeout_milliseconds: limits
                            .limit("database_busy_timeout_milliseconds"),
                    },
                )
                .unwrap(),
            )
        };
        let installation = InstallationIdentifier::parse(&"d".repeat(DIGEST_OCTETS * 2)).unwrap();
        let insert = |repository: &OperationRepository, identifier: &str| {
            let mut value = execute_envelope();
            value["request"]["operation_identifier"] = identifier.into();
            bind(&value)
                .unwrap()
                .prepare_admission(&installation)
                .unwrap()
                .unwrap()
                .persist_scheduled(repository, &std::collections::BTreeSet::new(), 1)
                .unwrap();
        };
        let repository = open();
        for identifier in ["first", "second", "third"] {
            insert(&repository, identifier);
        }
        let mut value = envelope();
        value["request"] = serde_json::json!({"request":"list_operations", "lifecycle_states":["queued"], "page_size":2});
        let Some(OperationResponse::ListPage { next_cursor: Some(cursor), operations }) =
            bind(&value).unwrap().list(&repository)
        else {
            panic!("a full page")
        };
        assert_eq!(operations, ["third", "second"]);
        drop(repository);
        let repository = open();
        insert(&repository, "newest");
        value["request"]["cursor"] = cursor.into();
        value["request"]["lifecycle_states"] = serde_json::json!(["queued", "queued"]);
        assert_eq!(
            bind(&value).unwrap().list(&repository),
            Some(OperationResponse::ListPage {
                next_cursor: None,
                operations: vec!["first".to_owned()]
            })
        );
        value["request"]["lifecycle_states"] = serde_json::json!(["succeeded"]);
        assert!(matches!(
            bind(&value).unwrap().list(&repository),
            Some(OperationResponse::MalformedFrame { .. })
        ));
        value["request"].as_object_mut().unwrap().remove("cursor");
        repository
            .settle_success(
                &served().author_target_identity_digest,
                "second",
                &slingshot_domain::operation::SuccessfulSettlement {
                    artifacts: vec![],
                    inline_result: Some("{}".to_owned()),
                    expected_lifecycle_state:
                        slingshot_domain::operation::OperationLifecycleState::Queued,
                    expected_revision: 1,
                    settled_at_unix_milliseconds: 2,
                },
            )
            .unwrap();
        assert_eq!(
            bind(&value).unwrap().list(&repository),
            Some(OperationResponse::ListPage {
                next_cursor: None,
                operations: vec!["second".to_owned()]
            })
        );
        let mut continued = value.clone();
        continued["request"]["page_size"] = 1.into();
        let Some(OperationResponse::ListPage { next_cursor: Some(cursor), .. }) =
            bind(&continued).unwrap().list(&repository)
        else {
            panic!("one terminal row supplies a cursor")
        };
        continued["request"]["cursor"] = cursor.into();
        for (field, changed) in [
            ("caller_identity", serde_json::json!("another-caller")),
            ("terminal", serde_json::json!(true)),
            ("workflow_correlation_identifier", serde_json::json!("another-workflow")),
        ] {
            let mut changed_request = continued.clone();
            changed_request["request"][field] = changed;
            assert!(matches!(
                bind(&changed_request).unwrap().list(&repository),
                Some(OperationResponse::MalformedFrame { .. })
            ));
        }
        value["request"]["page_size"] = 0.into();
        assert!(matches!(
            bind(&value).unwrap().list(&repository),
            Some(OperationResponse::MalformedFrame { .. })
        ));
        value["request"]["page_size"] = 1.into();
        value["request"]["lifecycle_states"] = serde_json::json!(["not-a-state"]);
        assert!(matches!(
            bind(&value).unwrap().list(&repository),
            Some(OperationResponse::MalformedFrame { .. })
        ));
        value["request"]["lifecycle_states"] = serde_json::json!([]);
        value["request"]["page_size"] = limits.limit("maximum_operation_list_page_size").into();
        assert!(matches!(
            bind(&value).unwrap().list(&repository),
            Some(OperationResponse::ListPage { .. })
        ));
        value["request"]["page_size"] =
            (limits.limit("maximum_operation_list_page_size") + 1).into();
        assert!(matches!(
            bind(&value).unwrap().list(&repository),
            Some(OperationResponse::MalformedFrame { .. })
        ));
        value["request"]["page_size"] = 1.into();
        value["request"]["lifecycle_states"] = serde_json::json!(vec![
            "queued";
            limits.limit("maximum_operation_list_filter_values")
                as usize
                + 1
        ]);
        assert!(matches!(
            bind(&value).unwrap().list(&repository),
            Some(OperationResponse::MalformedFrame { .. })
        ));
        value["request"]["lifecycle_states"] = serde_json::json!([]);
        value["request"]["cursor"] =
            "f".repeat(limits.limit("maximum_operation_list_cursor_bytes") as usize + 1).into();
        assert!(matches!(
            bind(&value).unwrap().list(&repository),
            Some(OperationResponse::MalformedFrame { .. })
        ));
    }

    #[test]
    fn invalid_listing_identity_filters_are_refused_before_reading_rows() {
        use slingshot_storage::database::{OperationDatabase, RequiredSettings};
        let limits = DaemonRuntimeContract::embedded();
        let repository = OperationRepository::new(
            OperationDatabase::open_in_memory(RequiredSettings {
                page_bytes: limits.limit("sqlite_page_bytes"),
                database_pages: limits.limit("maximum_sqlite_database_pages"),
                busy_timeout_milliseconds: limits.limit("database_busy_timeout_milliseconds"),
            })
            .unwrap(),
        );
        for (field, invalid) in [
            ("caller_identity", String::new()),
            ("caller_identity", "caller\0suffix".to_owned()),
            (
                "workflow_correlation_identifier",
                "w".repeat(
                    limits.limit("maximum_workflow_correlation_identifier_bytes") as usize + 1,
                ),
            ),
        ] {
            let mut value = envelope();
            value["request"] = serde_json::json!({"request":"list_operations", "page_size":1});
            value["request"][field] = invalid.into();
            assert!(matches!(
                bind(&value).unwrap().list(&repository),
                Some(OperationResponse::MalformedFrame { .. })
            ));
        }
    }

    #[test]
    fn recovery_wire_receipt_replays_with_current_state_after_settlement_and_reopen() {
        use slingshot_domain::installation::InstallationIdentifier;
        use slingshot_domain::operation::{
            OperationExecutionCertainty, OperationFact, OperationLifecycleState, RecoveryCategory,
            RecoveryExecutionEvidence, RecoveryFact, SuccessfulSettlement,
        };
        use slingshot_storage::database::{OperationDatabase, RequiredSettings};
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("recovery.sqlite");
        let limits = DaemonRuntimeContract::embedded();
        let open = || {
            OperationRepository::new(
                OperationDatabase::open(
                    &path,
                    RequiredSettings {
                        page_bytes: limits.limit("sqlite_page_bytes"),
                        database_pages: limits.limit("maximum_sqlite_database_pages"),
                        busy_timeout_milliseconds: limits
                            .limit("database_busy_timeout_milliseconds"),
                    },
                )
                .unwrap(),
            )
        };
        let repository = open();
        let installation = InstallationIdentifier::parse(&"d".repeat(DIGEST_OCTETS * 2)).unwrap();
        bind(&execute_envelope())
            .unwrap()
            .prepare_admission(&installation)
            .unwrap()
            .unwrap()
            .persist_scheduled(&repository, &std::collections::BTreeSet::new(), 1)
            .unwrap();
        let mut value = envelope();
        value["request"] = serde_json::json!({"request":"resume_operation_recovery", "operation_identifier":"operation", "expected_operation_revision":1, "expected_recovery_category":"operation_lookup"});
        assert!(matches!(
            bind(&value).unwrap().resume(&repository, 2).unwrap(),
            Some(OperationResponse::InvalidTransition { .. })
        ));
        let target = served().author_target_identity_digest;
        let waiting = repository
            .apply(
                &target,
                "operation",
                1,
                &OperationFact::Recovery {
                    recovery: RecoveryFact {
                        attempt_count: 1,
                        category: RecoveryCategory::OperationLookup,
                        detail: "lookup paused".to_owned(),
                        evidence: RecoveryExecutionEvidence::ExecutionCertainty {
                            certainty: OperationExecutionCertainty::SubmissionUnknown,
                        },
                        manual_resume_eligible: true,
                        retry_delay_milliseconds: 0,
                        retry_observed_at_unix_milliseconds: 2,
                    },
                },
                2,
            )
            .unwrap();
        assert!(matches!(
            bind(&value).unwrap().resume(&repository, 3).unwrap(),
            Some(OperationResponse::InvalidTransition { .. })
        ));
        value["request"]["expected_operation_revision"] = waiting.record.revision.into();
        value["request"]["expected_recovery_category"] = "artifact_transfer".into();
        assert!(matches!(
            bind(&value).unwrap().resume(&repository, 3).unwrap(),
            Some(OperationResponse::InvalidTransition { .. })
        ));
        assert_eq!(repository.read(&target, "operation").unwrap().unwrap(), waiting);
        value["request"]["expected_recovery_category"] = "operation_lookup".into();
        let Some(OperationResponse::RecoveryResumeApplied {
            resume_receipt_identifier,
            current_lifecycle_state,
            ..
        }) = bind(&value).unwrap().resume(&repository, 3).unwrap()
        else {
            panic!("an exact resume applies")
        };
        assert_eq!(current_lifecycle_state, "queued");
        let current = repository.read(&target, "operation").unwrap().unwrap();
        repository
            .settle_success(
                &target,
                "operation",
                &SuccessfulSettlement {
                    artifacts: vec![],
                    inline_result: Some("{}".to_owned()),
                    expected_lifecycle_state: OperationLifecycleState::Queued,
                    expected_revision: current.record.revision,
                    settled_at_unix_milliseconds: 4,
                },
            )
            .unwrap();
        drop(repository);
        let repository = open();
        assert_eq!(
            bind(&value).unwrap().resume(&repository, 5).unwrap(),
            Some(OperationResponse::RecoveryResumeReplayed {
                current_lifecycle_state: "succeeded".to_owned(),
                operation_identifier: "operation".to_owned(),
                resume_receipt_identifier,
            })
        );
        value["request"]["expected_recovery_category"] = "unknown-secret-category".into();
        let refused = bind(&value).unwrap().resume(&repository, 6).unwrap().unwrap();
        assert!(matches!(refused, OperationResponse::MalformedFrame { .. }));
        assert!(!serde_json::to_string(&refused).unwrap().contains("secret"));
        value["request"]["expected_recovery_category"] = "operation_lookup".into();
        value["request"]["operation_identifier"] = "missing".into();
        assert!(matches!(
            bind(&value).unwrap().resume(&repository, 7).unwrap(),
            Some(OperationResponse::MissingOperation { .. })
        ));
    }

    #[test]
    fn status_survives_repository_reopen_and_never_invents_a_missing_row() {
        use slingshot_domain::command_fingerprint::{CommandFingerprint, FingerprintInput};
        use slingshot_domain::installation::InstallationIdentifier;
        use slingshot_storage::database::{OperationDatabase, RequiredSettings};
        use slingshot_storage::operation_repository::AdmissionRequest;

        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("operations.sqlite");
        let limits = DaemonRuntimeContract::embedded();
        let open = || {
            OperationRepository::new(
                OperationDatabase::open(
                    &path,
                    RequiredSettings {
                        page_bytes: limits.limit("sqlite_page_bytes"),
                        database_pages: limits.limit("maximum_sqlite_database_pages"),
                        busy_timeout_milliseconds: limits
                            .limit("database_busy_timeout_milliseconds"),
                    },
                )
                .unwrap(),
            )
        };
        let request = bind(&envelope()).unwrap();
        let repository = open();
        assert!(matches!(
            request.status(&repository),
            Some(OperationResponse::MissingOperation { .. })
        ));
        let target = served();
        let canonical_command = r#"{"root_path":"/content/example"}"#;
        repository
            .admit(
                &AdmissionRequest {
                    author_target_identity: target.author_target_identity_digest.clone(),
                    author_target_identity_digest: target.author_target_identity_digest.clone(),
                    caller_identity: None,
                    canonical_command: canonical_command.to_owned(),
                    command_fingerprint: CommandFingerprint::derive(&FingerprintInput {
                        author_target_identity_digest: target.author_target_identity_digest,
                        canonical_command: canonical_command.to_owned(),
                        command_wire_name: "query_paths".to_owned(),
                        command_semantic_contract_version: "1.0.0".to_owned(),
                        selected_environment_revision: target.selected_environment_revision.clone(),
                    })
                    .unwrap(),
                    command_wire_name: "query_paths".to_owned(),
                    daemon_runtime_contract_digest: target.daemon_runtime_contract_digest,
                    installation_identifier: InstallationIdentifier::parse(
                        &"d".repeat(DIGEST_OCTETS * 2),
                    )
                    .unwrap(),
                    operation_identifier: "operation".to_owned(),
                    selected_environment_revision: target.selected_environment_revision,
                    workflow_correlation_identifier: None,
                },
                1,
            )
            .unwrap();
        let expected = Some(OperationResponse::Status {
            lifecycle_state: "queued".to_owned(),
            operation_identifier: "operation".to_owned(),
            operation_revision: 1,
        });
        assert_eq!(request.status(&repository), expected);
        drop(repository);
        assert_eq!(request.status(&open()), expected);
        let mut absent = envelope();
        absent["request"]["operation_identifier"] = "missing".into();
        assert!(matches!(
            bind(&absent).unwrap().status(&open()),
            Some(OperationResponse::MissingOperation { .. })
        ));
        absent["request"]["operation_identifier"] = "".into();
        assert!(matches!(
            bind(&absent).unwrap().status(&open()),
            Some(OperationResponse::MalformedFrame { .. })
        ));
    }
}
