//! Product admission ordering across a real selected-author socket and SQLite.

use sha2::{Digest, Sha256};
use slingshot_agent_connection::authentication::environment_provider::*;
use slingshot_agent_connection::transport_policy::{
    AuthorTrustInput, IdentityManagementTrustInput,
};
use slingshot_configuration::platform_trust::*;
use slingshot_configuration::profile_loader::{ConfigurationDiagnostic, load_profiles};
use slingshot_configuration::profile_selection::{RequestedSelection, resolve};
use slingshot_domain::profile::*;
use slingshot_domain::secret_value::SecretValue;
use slingshot_domain::selected_environment_revision::*;

struct Store(Vec<ProviderRecord>);

#[tokio::test]
async fn runtime_concrete_executor_preserves_paused_recovery_and_shutdown() {
    use slingshot_daemon::{ownership::{Acquisition,DaemonOwnership}, runtime_builder::{RuntimeBuilder,RuntimeExecutionRefusal}, runtime_namespace::RuntimeNamespace};
    use slingshot_domain::{operation::{OperationFact,RecoveryFact,RecoveryCategory,RecoveryExecutionEvidence,OperationExecutionCertainty}, operation_executor::{ExecutionIdentity,OperationExecutorOutcome,ProgressPort}};
    use slingshot_local_protocol::foundation_contract::FoundationContract;
    use slingshot_agent_protocol::{identity::WireOperationIdentity,wire_contract::ExpectedProvenance};
    use slingshot_agent_connection::command_submission::{Submission,ExpectedArtifactManifest};
    struct Progress;
    impl ProgressPort for Progress { fn report(&self, _: &str) {} }
    let root = tempfile::tempdir().unwrap();
    let contract = FoundationContract::embedded();
    let namespace = RuntimeNamespace::name(&contract,&root.path().join("runtime"),"remote-site","staging").unwrap();
    namespace.create_runtime_directory().unwrap();
    let Acquisition::Owned(owner) = DaemonOwnership::acquire(&contract,namespace.clone()).unwrap() else {panic!("fresh namespace")};
    let runtime = RuntimeBuilder::new(selected_snapshot("http://127.0.0.1:9"),*owner,root.path().join("state"),slingshot_storage::database::RequiredSettings {
        page_bytes:4096,database_pages:262144,busy_timeout_milliseconds:5000,
    }).unwrap().establish_durable().unwrap();
    let target = runtime.context().target();
    let identity = ExecutionIdentity {attempt:1,author_target_identity_digest:target.author_target_identity_digest.clone(),selected_environment_revision:target.selected_environment_revision.clone(),operation_identifier:"paused".into()};
    let expected = ExpectedProvenance {
        command_contract:slingshot_domain::selected_command_contract_identity::SelectedCommandContractIdentity::installed("query_paths").unwrap(),
        canonical_json_contract_digest:slingshot_domain::command::schema::canonical_contract_digest(),
        transport_contract_digest:slingshot_domain::author_agent_transport_contract::AuthorAgentTransportContract::embedded_digest(),
    };
    let canonical = "{\"root_path\":\"/retained\"}";
    let submission = Submission::build(&expected,WireOperationIdentity::of(&identity.author_target_identity_digest,&identity.selected_environment_revision,&identity.operation_identifier,slingshot_domain::agent_identity::AgentEventStoreGeneration::of(7)),"subscription",canonical,ExpectedArtifactManifest::empty()).unwrap();
    let fingerprint = slingshot_domain::command_fingerprint::CommandFingerprint::derive(&slingshot_domain::command_fingerprint::FingerprintInput {
        author_target_identity_digest:identity.author_target_identity_digest.clone(),canonical_command:canonical.into(),command_wire_name:"query_paths".into(),command_semantic_contract_version:expected.command_contract.command_semantic_contract_version.clone(),selected_environment_revision:identity.selected_environment_revision.clone(),
    }).unwrap();
    runtime.operations().admit(&slingshot_storage::operation_repository::AdmissionRequest {
        author_target_identity:"retained-identity".into(),author_target_identity_digest:identity.author_target_identity_digest.clone(),caller_identity:None,canonical_command:canonical.into(),command_fingerprint:fingerprint,command_wire_name:"query_paths".into(),daemon_runtime_contract_digest:target.daemon_runtime_contract_digest.clone(),installation_identifier:runtime.installation().clone(),operation_identifier:identity.operation_identifier.clone(),selected_environment_revision:identity.selected_environment_revision.clone(),workflow_correlation_identifier:None,
    },1).unwrap();
    let recovery = RecoveryFact {
        attempt_count:slingshot_daemon::operation::recovery_and_event_supervisor::automatic_attempt_cap() as u32,
        category:RecoveryCategory::OperationLookup,detail:"persisted pause".into(),evidence:RecoveryExecutionEvidence::ExecutionCertainty {certainty:OperationExecutionCertainty::RemoteOutcomeUnknown},manual_resume_eligible:true,retry_delay_milliseconds:42,retry_observed_at_unix_milliseconds:1,
    };
    runtime.operations().apply(&identity.author_target_identity_digest,&identity.operation_identifier,1,&OperationFact::Recovery {recovery:recovery.clone()},1).unwrap();
    let before = runtime.operations().read(&identity.author_target_identity_digest,&identity.operation_identifier).unwrap();
    assert_eq!(runtime.execute_retained(&identity,submission.clone(),&Progress).await.unwrap(),OperationExecutorOutcome::RecoveryRequired {recovery});
    assert_eq!(runtime.operations().read(&identity.author_target_identity_digest,&identity.operation_identifier).unwrap(),before);
    assert!(runtime.remote().read_for_local_operation(&identity.author_target_identity_digest,&identity.operation_identifier).unwrap().is_none());
    let mut foreign = identity.clone(); foreign.selected_environment_revision = "foreign".into();
    assert_eq!(runtime.execute_retained(&foreign,submission.clone(),&Progress).await.unwrap_err(),RuntimeExecutionRefusal::Binding);
    runtime.request_shutdown();
    assert_eq!(runtime.execute_retained(&identity,submission,&Progress).await.unwrap_err(),RuntimeExecutionRefusal::Cancelled);
    assert!(!namespace.readiness_path().exists());
}

#[test]
fn runtime_builder_never_invents_identity_beside_unregistered_state() {
    use slingshot_daemon::{ownership::{Acquisition, DaemonOwnership}, runtime_builder::{RuntimeBuilder, RuntimeBuildRefusal}, runtime_namespace::RuntimeNamespace};
    use slingshot_local_protocol::foundation_contract::FoundationContract;
    use slingshot_storage::{database::RequiredSettings, installation_state::InstallationState};
    let directory = tempfile::tempdir().unwrap();
    let state_root = directory.path().join("state");
    let contract = FoundationContract::embedded();
    let namespace = RuntimeNamespace::name(&contract, &directory.path().join("runtime"), "remote-site", "staging").unwrap();
    namespace.create_runtime_directory().unwrap();
    let paths = namespace.beneath(&state_root);
    paths.create().unwrap();
    let marker = paths.artifact_root().join("retained-evidence");
    std::fs::write(&marker, b"must not be adopted or removed").unwrap();
    let Acquisition::Owned(owner) = DaemonOwnership::acquire(&contract, namespace.clone()).unwrap() else { panic!("fresh namespace") };
    let builder = RuntimeBuilder::new(selected_snapshot("http://127.0.0.1:9"), *owner, state_root.clone(), RequiredSettings {
        page_bytes:4096, database_pages:262144, busy_timeout_milliseconds:5000,
    }).unwrap();
    assert_eq!(builder.establish_durable().unwrap_err(), RuntimeBuildRefusal::Installation);
    assert!(!InstallationState::at(&state_root).record_path().exists());
    assert!(!paths.database_path().exists());
    assert_eq!(std::fs::read(marker).unwrap(), b"must not be adopted or removed");
    assert!(!namespace.readiness_path().exists());
    assert!(matches!(DaemonOwnership::acquire(&contract, namespace).unwrap(), Acquisition::Owned(_)));
}

#[test]
fn runtime_builder_establishes_and_reopens_only_ledger_bound_state() {
    use slingshot_daemon::{ownership::{Acquisition, DaemonOwnership}, runtime_builder::{RuntimeBuilder, RuntimeBuildRefusal}, runtime_namespace::RuntimeNamespace};
    use slingshot_domain::installation::{InstallationIdentifier, InstallationRecord, TargetRegistration};
    use slingshot_local_protocol::foundation_contract::FoundationContract;
    use slingshot_storage::{database::RequiredSettings, installation_state::InstallationState};
    for defect in ["none", "missing-ledger", "missing-database", "foreign-identity", "unregistered", "staged-existing", "staged-absent", "corrupt-ledger", "artifact-content-blocked", "diagnostic-root-blocked", "abandoned-stage"] {
        let directory = tempfile::tempdir().unwrap();
        let state_root = directory.path().join("state");
        let contract = FoundationContract::embedded();
        let namespace = RuntimeNamespace::name(&contract, &directory.path().join("runtime"), "remote-site", "staging").unwrap();
        namespace.create_runtime_directory().unwrap();
        let build = || {
            let Acquisition::Owned(owner) = DaemonOwnership::acquire(&contract, namespace.clone()).unwrap() else { panic!("namespace must be released") };
            RuntimeBuilder::new(selected_snapshot("http://127.0.0.1:9"), *owner, state_root.clone(), RequiredSettings {
                page_bytes: 4096, database_pages: 262144, busy_timeout_milliseconds: 5000,
            }).unwrap()
        };
        let durable = build().establish_durable().unwrap();
        let identity = durable.installation().clone();
        assert_eq!(durable.database().installation_identifier().unwrap(), Some(identity.clone()));
        assert!(durable.database().shares_database_with(durable.remote().database()));
        assert!(durable.database().shares_database_with(durable.subscriptions().database()));
        assert!(durable.capacity().belongs_to(durable.operations().database()));
        assert_eq!(durable.capacity().usage().unwrap().operation_rows, 0);
        assert_eq!(durable.diagnostics().health().unwrap().total_bytes, 0);
        let cancellation = durable.cancellation_scope();
        let separate_cancellation = durable.cancellation_scope();
        cancellation.cancel();
        assert!(!separate_cancellation.is_cancelled(), "a child cancelled the runtime");
        if defect == "none" {
            let target = &durable.context().target().author_target_identity_digest;
            let approved = slingshot_storage::maintenance::preview(durable.database(), target, 1000, 1).unwrap();
            slingshot_storage::maintenance::apply(durable.database(), &approved, 1000).unwrap();
            let canonical = "{\"root_path\":\"/retained\"}";
            let revision = &durable.context().target().selected_environment_revision;
            let fingerprint = slingshot_domain::command_fingerprint::CommandFingerprint::derive(&slingshot_domain::command_fingerprint::FingerprintInput {
                author_target_identity_digest:target.clone(), canonical_command:canonical.into(), command_wire_name:"query_paths".into(),
                command_semantic_contract_version:"1".into(), selected_environment_revision:revision.clone(),
            }).unwrap();
            durable.operations().admit(&slingshot_storage::operation_repository::AdmissionRequest {
                author_target_identity: "retained-test-identity".into(), author_target_identity_digest:target.clone(), caller_identity:Some("caller".into()),
                canonical_command:canonical.into(), command_fingerprint:fingerprint, command_wire_name:"query_paths".into(),
                daemon_runtime_contract_digest:durable.context().target().daemon_runtime_contract_digest.clone(), installation_identifier:identity.clone(),
                operation_identifier:"retained-local".into(), selected_environment_revision:revision.clone(), workflow_correlation_identifier:None,
            }, 1000).unwrap();
            let bytes = b"{\"matches\":[]}";
            let digest = hex::encode(Sha256::digest(bytes));
            let request = slingshot_storage::artifact_store::InstallationRequest {
                artifact_slot:slingshot_storage::artifact_store::STRUCTURED_RESULT_SLOT.into(), author_target_identity_digest:target.clone(), descriptor:None,
                installation_identifier:identity.clone(),media_type:"application/json".into(),operation_identifier:"retained-local".into(),
            };
            let capacity = durable.capacity();
            let reservation = capacity.reserve_artifact(Some(&digest),bytes.len() as u64).unwrap();
            let stage = durable.artifacts().stage_verified(&request,&mut &bytes[..],bytes.len() as u64,&digest).unwrap();
            capacity.retain_staged_publication(&stage,reservation,123).unwrap();
            drop(stage);
        }
        assert_eq!(format!("{durable:?}"), "DurableRuntime([redacted])");
        assert_eq!(durable.context().namespace().digest(), namespace.digest());
        let paths = durable.paths().clone();
        let ledger = InstallationState::at(&state_root);
        assert_eq!(paths.installation_record_path(), ledger.record_path());
        assert_eq!(ledger.read().unwrap().registration(&namespace.key()), Some(TargetRegistration::Registered));
        assert!(!namespace.readiness_path().exists());
        assert!(matches!(DaemonOwnership::acquire(&contract, namespace.clone()).unwrap(), Acquisition::AlreadyOwned(_)));
        drop(durable);
        assert!(separate_cancellation.is_cancelled(), "runtime drop did not cancel its children");
        let mut record = ledger.read().unwrap();
        match defect {
            "missing-ledger" => std::fs::remove_file(ledger.record_path()).unwrap(),
            "missing-database" => std::fs::remove_file(paths.database_path()).unwrap(),
            "foreign-identity" => {
                record.installation_identifier = InstallationIdentifier::parse(&"f".repeat(64)).unwrap();
                ledger.replace(&record).unwrap();
            }
            "unregistered" => ledger.replace(&InstallationRecord::new(identity.clone())).unwrap(),
            "staged-existing" | "staged-absent" => {
                record.targets.insert(namespace.key(), TargetRegistration::Initializing);
                ledger.replace(&record).unwrap();
                if defect == "staged-absent" { std::fs::remove_file(paths.database_path()).unwrap(); }
            }
            "corrupt-ledger" => std::fs::write(ledger.record_path(), b"not-json").unwrap(),
            "abandoned-stage" => {
                use slingshot_storage::artifact_store::{CONTENT_DIRECTORY,STAGING_SUFFIX};
                let path = paths.artifact_root().join(CONTENT_DIRECTORY).join(format!("6ca0aa08-3d39-4b18-bfbe-f2bd859947a0{STAGING_SUFFIX}"));
                let mut options = std::fs::OpenOptions::new();
                options.write(true).create_new(true);
                #[cfg(unix)] { use std::os::unix::fs::OpenOptionsExt as _; options.mode(0o600); }
                use std::io::Write as _;
                options.open(path).unwrap().write_all(b"abandoned partial bytes").unwrap();
            }
            "artifact-content-blocked" => {
                let content = paths.artifact_root().join(slingshot_storage::artifact_store::CONTENT_DIRECTORY);
                std::fs::remove_dir(&content).unwrap();
                std::fs::write(content, b"not a directory").unwrap();
            }
            "diagnostic-root-blocked" => {
                std::fs::remove_dir(paths.diagnostic_root()).unwrap();
                std::fs::write(paths.diagnostic_root(), b"not a directory").unwrap();
            }
            _ => {},
        }
        let ledger_before = std::fs::read(ledger.record_path()).ok();
        let database_before = std::fs::read(paths.database_path()).ok();
        let result = build().establish_durable();
        if matches!(defect, "none" | "staged-existing" | "staged-absent" | "abandoned-stage") {
            let resumed = result.unwrap();
            assert_eq!(resumed.recovered_stages(), if defect == "abandoned-stage" {1} else {0});
            if defect == "none" {
                assert_eq!(resumed.maintenance_recovery().len(), 1);
                assert_eq!(resumed.maintenance_recovery()[0].stage, slingshot_storage::maintenance::ReceiptStage::Completed);
                assert_eq!(resumed.recovered_operations().len(), 1);
                let recovered = &resumed.recovered_operations()[0];
                assert_eq!(recovered.input.summary.operation_identifier, "retained-local");
                assert_eq!(recovered.input.canonical_command, "{\"root_path\":\"/retained\"}");
                assert_eq!(recovered.input.summary.record.revision, 1);
                assert_eq!(recovered.input.summary.record.lifecycle_state, slingshot_domain::operation::OperationLifecycleState::Queued);
                assert!(recovered.remote.is_none());
                assert_eq!(resumed.pending_publications().len(),1);
                let publication = &resumed.pending_publications()[0];
                assert_eq!(publication.operation_identifier,"retained-local");
                assert_eq!(publication.artifact_slot,slingshot_storage::artifact_store::STRUCTURED_RESULT_SLOT);
                assert_eq!(publication.publication.recorded_at_unix_milliseconds,123);
                assert_eq!(resumed.capacity().pending_publications().unwrap(),1);
            }
            assert_eq!(resumed.installation(), &identity);
            assert_eq!(resumed.database().installation_identifier().unwrap(), Some(identity));
            assert_eq!(ledger.read().unwrap().registration(&namespace.key()), Some(TargetRegistration::Registered));
            let cancellation = resumed.cancellation_scope();
            let selected = resumed.context().target().clone();
            let mut service = slingshot_daemon::service::DaemonService::from_runtime(contract.clone(), resumed);
            let published = service.ownership_mut().identity().unwrap();
            assert_eq!(published.author_target_identity_digest, selected.author_target_identity_digest);
            assert_eq!(published.selected_environment_revision, selected.selected_environment_revision);
            assert_eq!(published.daemon_runtime_contract_digest, selected.daemon_runtime_contract_digest);
            assert_eq!(published.retained_control_version, contract.control.version);
            assert!(published.supported_operation_versions.is_empty());
            let service = std::sync::Arc::new(service);
            let connection = std::sync::Arc::clone(&service);
            let nonce = service.readiness_nonce();
            drop(service);
            assert!(!cancellation.is_cancelled(), "service creator released a live connection's runtime");
            assert!(matches!(DaemonOwnership::acquire(&contract, namespace.clone()).unwrap(), Acquisition::AlreadyOwned(_)));
            assert!(!namespace.readiness_path().exists(), "service conversion published readiness");
            std::thread::spawn(move || {
                assert_eq!(connection.readiness_nonce(), nonce);
                drop(connection);
            }).join().unwrap();
            assert!(cancellation.is_cancelled(), "last service owner did not cancel the runtime");
        } else {
            assert!(matches!(result.unwrap_err(), RuntimeBuildRefusal::Installation | RuntimeBuildRefusal::Database | RuntimeBuildRefusal::Resources));
            assert_eq!(std::fs::read(ledger.record_path()).ok(), ledger_before);
            assert_eq!(std::fs::read(paths.database_path()).ok(), database_before);
        }
        assert!(!namespace.readiness_path().exists());
        assert!(matches!(DaemonOwnership::acquire(&contract, namespace.clone()).unwrap(), Acquisition::Owned(_)));
    }
}

#[tokio::test]
async fn runtime_builder_binds_snapshot_names_and_holds_ownership_without_readiness() {
    use slingshot_daemon::{ownership::{Acquisition,DaemonOwnership}, runtime_builder::{RuntimeBuilder,RuntimeBuildRefusal}, runtime_namespace::RuntimeNamespace};
    use slingshot_local_protocol::foundation_contract::FoundationContract;
    use slingshot_storage::database::RequiredSettings;
    use tokio::time::{timeout,Duration};
    for (profile,environment,accepted) in [("remote-site","staging",true),("other-site","staging",false),("remote-site","production",false)] {
        let directory = tempfile::tempdir().unwrap();
        let runtime_root = directory.path().join("runtime");
        let state_root = directory.path().join("state");
        let contract = FoundationContract::embedded();
        let namespace = RuntimeNamespace::name(&contract,&runtime_root,profile,environment).unwrap();
        namespace.create_runtime_directory().unwrap();
        let Acquisition::Owned(owner) = DaemonOwnership::acquire(&contract,namespace.clone()).unwrap() else {panic!("fresh namespace owned")};
        let author = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let snapshot = selected_snapshot(&format!("http://{}",author.local_addr().unwrap()));
        assert_eq!(snapshot.profile_name().as_text(),"remote-site");
        assert_eq!(snapshot.environment_name().as_text(),"staging");
        let target = snapshot.target().to_string();
        let revision = snapshot.revision().to_string();
        let builder = RuntimeBuilder::new(snapshot,*owner,state_root.clone(),RequiredSettings {
            page_bytes:4096,database_pages:262144,busy_timeout_milliseconds:5000,
        });
        if accepted {
            let builder = builder.unwrap();
            assert_eq!(builder.target().author_target_identity_digest,target);
            assert_eq!(builder.target().selected_environment_revision,revision);
            assert_eq!(builder.target().daemon_runtime_contract_digest,slingshot_domain::daemon_runtime_contract::DaemonRuntimeContract::embedded_digest().as_text());
            assert_eq!(builder.authentication().snapshot().target().to_string(),target);
            assert_eq!(builder.namespace().digest(),namespace.digest());
            assert_eq!(builder.state_root(),state_root);
            assert_eq!(format!("{builder:?}"),"RuntimeBuilder([redacted])");
            assert!(matches!(DaemonOwnership::acquire(&contract,namespace.clone()).unwrap(),Acquisition::AlreadyOwned(_)));
            drop(builder);
        } else {
            assert_eq!(builder.unwrap_err(),RuntimeBuildRefusal::OwnershipMismatch);
        }
        assert!(!state_root.exists(),"initial context created durable state");
        assert!(!namespace.readiness_path().exists(),"initial context published readiness");
        assert!(timeout(Duration::from_millis(10),author.accept()).await.is_err());
        assert!(matches!(DaemonOwnership::acquire(&contract,namespace).unwrap(),Acquisition::Owned(_)),"ownership did not unwind");
    }
}

#[tokio::test]
async fn selected_live_events_commit_only_the_believed_prefix() {
    use slingshot_daemon::operation::{selected_event_attachment::{attach_selected_events_with_authentication as attach_selected_events, SelectedEventAttachmentOutcome}, subscription_reset::ResetTransport};
    use slingshot_agent_connection::{command_submission::{ExpectedArtifactManifest, Submission}, selected_author_transport::SelectedAuthorTransport,
        server_sent_event_decoder::{StreamItem, OperationStreamExpectation}, selected_author_http::FiniteHttpFailure};
    use slingshot_agent_protocol::{identity::WireOperationIdentity, wire_contract::ExpectedProvenance};
    use slingshot_daemon::operation::{durable_author_submission::prepare_initial_submission, durable_author_event::{fold_selected_event, DurableEventOutcome}};
    use slingshot_domain::{agent_identity::AgentEventStoreGeneration, operation_executor::ExecutionIdentity, command_fingerprint::{CommandFingerprint, FingerprintInput}, installation::InstallationIdentifier};
    use slingshot_storage::{agent_job_repository::AgentJobRepository, agent_subscription_ledger::AgentSubscriptionLedger, database::{OperationDatabase, RequiredSettings}, operation_repository::{AdmissionRequest, OperationRepository}};
    use tokio::{io::{AsyncReadExt, AsyncWriteExt}, time::{timeout, Duration}};
    for owned in [false, true] {
    for mode in 0..5 {
    let http2=mode==1;
    let selected_protocol=match mode {0=>ResetTransport::Http1,1=>ResetTransport::Http2,_=>ResetTransport::Automatic};
        for defect in ["", "gap", "regression", "terminal", "terminal-no-result-repeat", "terminal-no-result-cap", "terminal-capability-error", "terminal-lookup-error", "terminal-lookup-error-cap", "terminal-lookup-error-success", "terminal-wait-cancel", "terminal-complete", "terminal-complete-failed", "terminal-old", "terminal-active", "terminal-watermark", "terminal-physical", "terminal-local-race", "terminal-physical-race", "terminal-owner", "cursor", "unknown", "replay", "stale", "job-conflict", "cursor-gap-conflict", "cursor-stale-conflict", "terminal-foreign-auth"] {
            if defect == "terminal-foreign-auth" && mode >= 3 { continue; }
            let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
            let endpoint = format!("http://{}/aem", listener.local_addr().unwrap());
            let provider = provider(&endpoint);
            let async_provider = async_provider(&endpoint);
            let transport = SelectedAuthorTransport::new(provider.snapshot().author_connection()).unwrap();
            let (authentication, _) = provider.authenticate(&endpoint, 1, &NoTokenSource).unwrap();
            let identity = ExecutionIdentity { attempt: 1, operation_identifier: "event-local".into(), author_target_identity_digest: provider.snapshot().target().to_string(), selected_environment_revision: provider.snapshot().revision().to_string() };
            let expected = ExpectedProvenance {
                command_contract: slingshot_domain::selected_command_contract_identity::SelectedCommandContractIdentity::installed("query_paths").unwrap(),
                canonical_json_contract_digest: slingshot_domain::command::schema::canonical_contract_digest(),
                transport_contract_digest: slingshot_domain::author_agent_transport_contract::AuthorAgentTransportContract::embedded_digest(),
            };
            let root = tempfile::tempdir().unwrap(); let path = root.path().join("events.sqlite3");
            let settings = || RequiredSettings { page_bytes: 4096, database_pages: 262144, busy_timeout_milliseconds: 5000 };
            let repository = AgentJobRepository::new(OperationDatabase::open(&path, settings()).unwrap());
            let operations = OperationRepository::new(OperationDatabase::open(&path, settings()).unwrap());
            let ledger = AgentSubscriptionLedger::new(OperationDatabase::open(&path, settings()).unwrap());
            let target = &identity.author_target_identity_digest;
            ledger.open_subscription(target, "subscription-one", 7, 1000).unwrap();
            if owned {
                let empty = ledger.read_recovery_view(target,"subscription-one").unwrap();
                ledger.install_empty_recovery(&empty,7,"cursor-000").unwrap();
            }
            let canonical = r#"{"root_path":"/content/example"}"#;
            operations.admit(&AdmissionRequest {
                author_target_identity: "opaque-target".into(), author_target_identity_digest: target.clone(), caller_identity: None,
                canonical_command: canonical.into(), command_fingerprint: CommandFingerprint::derive(&FingerprintInput {
                    author_target_identity_digest: target.clone(), canonical_command: canonical.into(), command_wire_name: "query_paths".into(),
                    command_semantic_contract_version: expected.command_contract.command_semantic_contract_version.clone(), selected_environment_revision: identity.selected_environment_revision.clone(),
                }).unwrap(), command_wire_name: "query_paths".into(),
                daemon_runtime_contract_digest: slingshot_domain::daemon_runtime_contract::DaemonRuntimeContract::embedded_digest().as_text().into(),
                installation_identifier: InstallationIdentifier::parse(&"a1".repeat(32)).unwrap(), operation_identifier: identity.operation_identifier.clone(),
                selected_environment_revision: identity.selected_environment_revision.clone(), workflow_correlation_identifier: None,
            }, 1000).unwrap();
            let submission = Submission::build(&expected, WireOperationIdentity::of(target, &identity.selected_environment_revision, &identity.operation_identifier, AgentEventStoreGeneration::of(7)), "subscription-one", canonical, ExpectedArtifactManifest::empty()).unwrap();
            drop(prepare_initial_submission(&repository, &identity, &submission, 1000).unwrap().unwrap());
            let before = repository.read(target, &submission.operation.agent_operation_identifier).unwrap().unwrap();
            let first_sequence = before.observation.applied_sequence.value() + 1;
            if ["terminal-lookup-error-cap","terminal-lookup-error-success","terminal-wait-cancel","terminal-no-result-repeat","terminal-no-result-cap"].contains(&defect) {
                use slingshot_domain::operation::{OperationFact,RecoveryFact,RecoveryCategory,RecoveryExecutionEvidence,OperationExecutionCertainty};
                let known_success=defect=="terminal-lookup-error-success" || defect.starts_with("terminal-no-result");
                if known_success {
                    repository.fold_event(&before.identity,before.observation.applied_sequence,slingshot_domain::remote_job::RemoteJobObservation {
                        applied_sequence:slingshot_domain::remote_job::JobEventSequence::of(first_sequence),state:slingshot_domain::remote_job::AgentJobState::Running,attempt:2,progress:40,
                    }).unwrap(); repository.record_physical_job(&before.identity,"event-job",1000).unwrap();
                }
                operations.apply(target,&identity.operation_identifier,1,&OperationFact::Recovery {recovery:RecoveryFact {
                    attempt_count:if known_success && defect!="terminal-no-result-cap" {1} else {slingshot_daemon::operation::recovery_and_event_supervisor::automatic_attempt_cap() as u32-1},
                    category:if known_success {RecoveryCategory::ResultAcquisition} else {RecoveryCategory::OperationLookup},
                    evidence:if known_success {RecoveryExecutionEvidence::AuthoritativeRemoteSuccess} else {RecoveryExecutionEvidence::ExecutionCertainty {certainty:OperationExecutionCertainty::RemoteOutcomeUnknown}},
                    detail:"retained retry".into(),manual_resume_eligible:false,
                    retry_delay_milliseconds:if defect=="terminal-wait-cancel" {slingshot_domain::author_agent_transport_contract::AuthorAgentTransportContract::embedded().limit("retry_jitter_cap_milliseconds")} else {0},
                    retry_observed_at_unix_milliseconds:if defect=="terminal-wait-cancel" {1_000_000} else {1000},
                }},1000).unwrap();
            }
            let first = serde_json::json!({"agent_event_store_generation":7, "agent_operation_identifier":submission.operation.agent_operation_identifier,
                "daemon_subscription_identifier":"subscription-one", "sling_job_identifier":"event-job", "kind":"progress", "state":"running",
                "sequence":first_sequence, "attempt":2, "progress":40});
            let mut second = first.clone(); second["sequence"] = (first_sequence + 1).into();
            second.as_object_mut().unwrap().remove("attempt"); second.as_object_mut().unwrap().remove("progress");
            match defect {
                "gap" => second["sequence"] = (first_sequence + 2).into(),
                "regression" => second["progress"] = 0.into(),
                name if name.starts_with("terminal") => { let state=if name=="terminal-complete-failed" {"failed"} else {"succeeded"}; second["kind"] = state.into(); second["state"] = state.into(); second["terminal"] = serde_json::json!({"provenance":submission.provenance, "submitted_command_digest":submission.submitted_command_digest}); },
                "unknown" => second["agent_operation_identifier"] = "b".repeat(64).into(),
                "replay" => second["sequence"] = first_sequence.into(),
                "stale" => second["sequence"] = (first_sequence - 1).into(),
                "job-conflict" => { second["sequence"] = first_sequence.into(); second["progress"] = 41.into(); },
                "cursor-gap-conflict" => second["sequence"] = (first_sequence + 2).into(),
                "cursor-stale-conflict" => second["sequence"] = (first_sequence - 1).into(),
                _ => {},
            }
            let body = format!("id:cursor-001\ndata:{first}\n\n{}data:{second}\n\n: after\n\n", if defect == "cursor" { "" } else if defect.starts_with("cursor-") { "id:cursor-001\n" } else { "id:cursor-002\n" });
            let peer = async {
                let (mut socket, _) = listener.accept().await.unwrap(); let mut request = Vec::new();
                if http2 {
                    let mut preface = [0;39]; socket.read_exact(&mut preface).await.unwrap();
                    socket.write_all(&[0,0,0,4,0,0,0,0,0]).await.unwrap();
                    let mut header = [0;9]; socket.read_exact(&mut header).await.unwrap(); socket.write_all(&header).await.unwrap();
                    socket.read_exact(&mut header).await.unwrap(); assert_eq!((header[3],header[4]),(1,5));
                    let length = usize::from(header[0]) << 16 | usize::from(header[1]) << 8 | usize::from(header[2]);
                    assert!(length <= 16384); request.resize(length,0); socket.read_exact(&mut request).await.unwrap();
                } else { while !request.ends_with(b"\r\n\r\n") { request.push(socket.read_u8().await.unwrap()); assert!(request.len() <= 8192); } }
                let route = "/aem/bin/slingshot-agent/events?agent_event_store_generation=7&daemon_subscription_identifier=subscription-one";
                assert!(request.windows(route.len()).any(|bytes| bytes == route.as_bytes()));
                authentication.lend_value_bytes(|value| assert!(request.windows(value.len()).any(|bytes| bytes == value)));
                if owned { assert!(request.windows(b"cursor-000".len()).any(|bytes| bytes == b"cursor-000")); }
                if http2 {
                    let mut block = vec![0x88];
                    for (name,value) in [("content-type","text/event-stream".to_owned()),("content-length",body.len().to_string())] {
                        block.extend_from_slice(&[0,name.len() as u8]); block.extend_from_slice(name.as_bytes()); block.push(value.len() as u8); block.extend_from_slice(value.as_bytes());
                    }
                    for (kind,flags,bytes) in [(1,4,block.as_slice()),(0,1,body.as_bytes())] {
                        let length = (bytes.len() as u32).to_be_bytes(); socket.write_all(&[length[1],length[2],length[3],kind,flags,0,0,0,1]).await.unwrap(); socket.write_all(bytes).await.unwrap();
                    }
                    let mut close = Vec::new(); let _ = socket.read_to_end(&mut close).await;
                } else { socket.write_all(format!("HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nContent-Length: {}\r\n\r\n{body}",body.len()).as_bytes()).await.unwrap(); }
            };
            let mut outcomes = Vec::new(); let mut heartbeat = false; let mut owned_reason = None; let mut terminal_ticket = None;
            let clock = AuthenticationClock(std::cell::Cell::new(0));
            use slingshot_daemon::operation::author_authentication::AuthorAuthentication;
            let selected_authentication = if mode==4 {AuthorAuthentication::AsyncProvider {provider:&async_provider,clock:&NoTokenClocks,utc:&NoTokenClocks}} else if mode==3 {AuthorAuthentication::Provider {provider:&provider,source:&NoTokenSource,clock:&clock}}
                else {AuthorAuthentication::Fixed {authentication:&authentication,protocol:selected_protocol}};
            let resolver = |operation: &str| Ok(OperationStreamExpectation { daemon_subscription_identifier:"subscription-one".into(), agent_event_store_generation:7,
                agent_operation_identifier:operation.into(), expected_provenance:expected.clone(), submitted_command_digest:submission.submitted_command_digest.clone() });
            let consume = |item| {
                let StreamItem::Event(event) = item else { heartbeat = true; return Ok(()) };
                assert!(fold_selected_event(&ledger,&operations,&transport,&identity,"another-subscription",&event,1001).is_err());
                let outcome = fold_selected_event(&ledger,&operations,&transport,&identity,"subscription-one",&event,1001).map_err(|_| FiniteHttpFailure::Body)?;
                outcomes.push(outcome);
                if matches!(outcome,DurableEventOutcome::Recorded(_)) { Ok(()) } else { Err(FiniteHttpFailure::Body) }
            };
            let request = async {
                if owned {
                    drop(consume);
                    match attach_selected_events(&ledger,&operations,&transport,&identity,"subscription-one",selected_authentication,1000).await {
                        Ok(SelectedEventAttachmentOutcome::Transport(outcome)) => Ok(outcome),
                        Ok(SelectedEventAttachmentOutcome::TerminalRecovery(ticket)) => {
                            assert_eq!(ticket.agent_operation_identifier(),submission.operation.agent_operation_identifier);
                            terminal_ticket = Some(ticket); owned_reason = Some(DurableEventOutcome::NeedsTerminalLookup);
                            Err(FiniteHttpFailure::Body)
                        }
                        Ok(SelectedEventAttachmentOutcome::Recovery {agent_operation_identifier,reason}) => {
                            assert_eq!(agent_operation_identifier.as_deref(),Some(submission.operation.agent_operation_identifier.as_str()));
                            owned_reason = Some(reason); Err(FiniteHttpFailure::Body)
                        }
                        Err(_) => Err(FiniteHttpFailure::Body),
                    }
                }
                else if mode==4 {transport.events_authenticated_async(&identity,"subscription-one",7,None,&async_provider,&NoTokenClocks,&NoTokenClocks,resolver,consume).await}
                else if mode==3 {transport.events_authenticated(&identity,"subscription-one",7,None,&provider,&NoTokenSource,0,resolver,consume).await}
                else if http2 { transport.events_http2(&identity,"subscription-one",7,None,&authentication,resolver,consume).await }
                else { transport.events_http1(&identity,"subscription-one",7,None,&authentication,resolver,consume).await }
            };
            let (_, result) = timeout(Duration::from_secs(5), async { tokio::join!(peer,request) }).await.unwrap();
            if let Some(reason) = owned_reason { outcomes.push(reason); }
            let success = ["","unknown","replay","stale"].contains(&defect);
            assert_eq!(result.is_ok(),success,"{owned}/{http2}/{defect}: {result:?}"); if !owned { assert_eq!(heartbeat,success); }
            let after = repository.read(target,&submission.operation.agent_operation_identifier).unwrap().unwrap();
            assert_eq!(after.observation.applied_sequence.value(),first_sequence + u64::from(defect.is_empty()));
            assert_eq!((after.observation.attempt,after.observation.progress),(2,40));
            assert_eq!(after.remaining_retention_milliseconds,before.remaining_retention_milliseconds);
            assert_eq!(after.request_start_unix_milliseconds,before.request_start_unix_milliseconds);
            assert_eq!(repository.physical_jobs(target,&submission.operation.agent_operation_identifier).unwrap(),["event-job"]);
            let held = ledger.read_subscription(target,"subscription-one").unwrap().unwrap();
            assert_eq!(held.cursor.as_deref(),Some(if success {"cursor-002"} else {"cursor-001"}));
            assert_eq!(held.event_rows,if success {2} else {1});
            if defect == "terminal" { assert_eq!(outcomes.last(),Some(&DurableEventOutcome::NeedsTerminalLookup)); }
            if let Some(ticket) = terminal_ticket {
                assert_eq!(format!("{ticket:?}"),"CapturedTerminalEvent([redacted])");
                if defect == "terminal-foreign-auth" {
                    let foreign = EnvironmentAuthenticationProvider::new(selected_snapshot_with_publisher(&endpoint,"http://another-publisher.example.com"),2);
                    assert_eq!(foreign.snapshot().target(),provider.snapshot().target());
                    assert_ne!(foreign.snapshot().revision(),provider.snapshot().revision());
                    let (foreign_fixed,_) = foreign.authenticate(&endpoint,0,&NoTokenSource).unwrap();
                    let local_before = operations.read(target,&identity.operation_identifier).unwrap();
                    let remote_before = repository.read(target,&submission.operation.agent_operation_identifier).unwrap();
                    assert!(ticket.reconcile(&repository,&transport,&foreign_fixed,None).await.is_err());
                    assert_eq!(operations.read(target,&identity.operation_identifier).unwrap(),local_before);
                    assert_eq!(repository.read(target,&submission.operation.agent_operation_identifier).unwrap(),remote_before);
                    assert_eq!(ledger.read_subscription(target,"subscription-one").unwrap().unwrap(),held);
                    assert!(timeout(Duration::from_millis(20),listener.accept()).await.is_err());
                    continue;
                }
                if defect == "terminal-local-race" {
                    let local = operations.read(target,&identity.operation_identifier).unwrap().unwrap();
                    operations.apply(target,&identity.operation_identifier,local.record.revision,
                        &slingshot_domain::operation::OperationFact::Progress {detail:"new owner observation".into()},1002).unwrap();
                }
                if defect == "terminal-physical-race" {
                    repository.record_physical_job(&before.identity,"concurrent-job",1002).unwrap();
                }
                let local_before = operations.read(target,&identity.operation_identifier).unwrap().unwrap();
                let remote_before = repository.read(target,&submission.operation.agent_operation_identifier).unwrap().unwrap();
                let other = AgentJobRepository::new(OperationDatabase::open(&root.path().join("other-owner.sqlite3"),settings()).unwrap());
                let capabilities = serde_json::json!({"format":"slingshot.agent/1","agent_event_store_generation":if defect=="terminal-capability-error" {9} else {7},
                    "canonical_json_contract_digest":expected.canonical_json_contract_digest,"transport_contract_digest":expected.transport_contract_digest,
                    "command_contracts":[slingshot_agent_protocol::identity::WireContractIdentity::from(&expected.command_contract)],"continuation_authority_ready":true});
                let mut snapshot = serde_json::json!({"provenance":submission.provenance,"agent_event_store_generation":if defect.starts_with("terminal-lookup-error") {9} else {7},
                    "agent_operation_identifier":submission.operation.agent_operation_identifier,"author_target_identity_digest":target,
                    "selected_environment_revision":identity.selected_environment_revision,"daemon_subscription_identifier":"subscription-one",
                    "submitted_command_digest":submission.submitted_command_digest,
                    "subscription_watermark":if defect == "terminal-watermark" {"cursor-001"} else {"cursor-002"},
                    "physical_sling_job_identifiers":[if defect == "terminal-physical" {"other-job"} else {"event-job"}],
                    "granted_retention_milliseconds":120000,"attempt":2,"progress":40,
                    "sequence":if defect == "terminal-old" {first_sequence} else {first_sequence+1},
                    "kind":if defect == "terminal-active" {"progress"} else if defect == "terminal-complete-failed" {"failed"} else {"succeeded"}});
                if defect == "terminal-complete" {
                    snapshot["terminal_result"] = serde_json::json!({"operation":submission.operation,
                        "daemon_subscription_identifier":"subscription-one","provenance":submission.provenance,
                        "submitted_command_digest":submission.submitted_command_digest,"canonical_result":r#"{"matches":[]}"#,"declared_artifacts":[]});
                }
                if defect == "terminal-complete-failed" {
                    snapshot["terminal_failure"] = serde_json::json!({"operation":submission.operation,
                        "daemon_subscription_identifier":"subscription-one","provenance":submission.provenance,
                        "submitted_command_digest":submission.submitted_command_digest,"canonical_failure":r#"{"failure":"root_not_found","root_path":"/content/example"}"#});
                }
                let peer = async {
                    if ["terminal-local-race","terminal-physical-race","terminal-owner"].contains(&defect) { return; }
                    for (route,document) in [("/aem/bin/slingshot-agent/capabilities".to_owned(),capabilities),
                        (format!("/aem/bin/slingshot-agent/operations/lookup?agent_operation_identifier={}",submission.operation.agent_operation_identifier),snapshot)] {
                        if defect=="terminal-capability-error" && route.contains("/operations/") { break; }
                        let body = document.to_string(); let (mut socket,_) = listener.accept().await.unwrap(); let mut request = Vec::new();
                        if http2 {
                            let mut preface = [0;39]; socket.read_exact(&mut preface).await.unwrap(); assert_eq!(&preface[..24],b"PRI * HTTP/2.0\r\n\r\nSM\r\n\r\n");
                            socket.write_all(&[0,0,0,4,0,0,0,0,0]).await.unwrap();
                            let mut header = [0;9]; socket.read_exact(&mut header).await.unwrap(); socket.write_all(&header).await.unwrap();
                            socket.read_exact(&mut header).await.unwrap(); assert_eq!((header[3],header[4]),(1,5));
                            let length = usize::from(header[0]) << 16 | usize::from(header[1]) << 8 | usize::from(header[2]); assert!(length<=16384);
                            request.resize(length,0); socket.read_exact(&mut request).await.unwrap();
                        } else { while !request.ends_with(b"\r\n\r\n") { request.push(socket.read_u8().await.unwrap()); assert!(request.len()<=8192); } }
                        assert!(request.windows(route.len()).any(|bytes| bytes == route.as_bytes()));
                        authentication.lend_value_bytes(|value| assert!(request.windows(value.len()).any(|bytes| bytes==value)));
                        if http2 {
                            let mut block = vec![0x88]; for (name,value) in [("content-type","application/json".to_owned()),("content-length",body.len().to_string())] {
                                block.extend_from_slice(&[0,name.len() as u8]); block.extend_from_slice(name.as_bytes()); block.push(value.len() as u8); block.extend_from_slice(value.as_bytes());
                            }
                            for (kind,flags,bytes) in [(1,4,block.as_slice()),(0,1,body.as_bytes())] {
                                let length=(bytes.len() as u32).to_be_bytes(); socket.write_all(&[length[1],length[2],length[3],kind,flags,0,0,0,1]).await.unwrap(); socket.write_all(bytes).await.unwrap();
                            }
                            let mut close=Vec::new(); let _=socket.read_to_end(&mut close).await;
                        } else { socket.write_all(format!("HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n{body}",body.len()).as_bytes()).await.unwrap(); }
                    }
                };
                let (recovered,()) = if defect=="terminal-wait-cancel" {
                    tokio::time::pause();
                    let mut waiting=Box::pin(ticket.reconcile_saved(&repository,&transport,None));
                    std::future::poll_fn(|context| {
                        assert!(std::future::Future::poll(waiting.as_mut(),context).is_pending());
                        std::task::Poll::Ready(())
                    }).await;
                    drop(waiting); tokio::time::resume();
                    (Err(slingshot_daemon::operation::durable_author_event::DurableEventRefusal),())
                } else {timeout(Duration::from_secs(5),async { tokio::join!(ticket.reconcile_saved(if defect == "terminal-owner" {&other} else {&repository},&transport,None),peer) }).await.unwrap()};
                assert_eq!(recovered.is_ok(),["terminal","terminal-no-result-repeat","terminal-no-result-cap","terminal-complete","terminal-complete-failed"].contains(&defect),"{http2}/{defect}: {recovered:?}");
                assert_eq!(ledger.read_subscription(target,"subscription-one").unwrap().unwrap(),held);
                let local_after=operations.read(target,&identity.operation_identifier).unwrap().unwrap();
                if defect == "terminal" || defect.starts_with("terminal-no-result") {
                    let retry=local_after.record.outstanding_recovery.as_ref().unwrap();
                    assert_eq!(retry.evidence,slingshot_domain::operation::RecoveryExecutionEvidence::AuthoritativeRemoteSuccess);
                    assert_eq!(retry.category,slingshot_domain::operation::RecoveryCategory::ResultAcquisition);
                    assert_eq!(retry.attempt_count,local_before.record.outstanding_recovery.as_ref().map_or(1,|previous|previous.attempt_count+1));
                    assert_eq!(retry.manual_resume_eligible,defect=="terminal-no-result-cap");
                    if retry.manual_resume_eligible {assert_eq!(retry.retry_delay_milliseconds,0);}
                    else {assert!(retry.retry_delay_milliseconds<=slingshot_daemon::operation::recovery_and_event_supervisor::jitter_ceiling_milliseconds(u64::from(retry.attempt_count)));}
                    assert!(!local_after.record.lifecycle_state.is_terminal());
                    assert_eq!(repository.read(target,&submission.operation.agent_operation_identifier).unwrap().unwrap(),remote_before);
                    if defect == "terminal-no-result-cap" {
                        use slingshot_daemon::{author_agent_operation_executor::{AuthorAgentOperationExecutor,ProductAuthorPorts},retained_author_protocol::RetainedAuthorProtocol};
                        use slingshot_domain::operation_executor::{OperationExecutor,OperationExecutorOutcome,ProgressPort};
                        use slingshot_storage::{artifact_store::ArtifactStore,persistent_capacity::PersistentCapacityAccount};
                        struct Progress;
                        impl ProgressPort for Progress { fn report(&self,_:&str) {} }
                        let store=ArtifactStore::open(&root.path().join("paused-artifacts")).unwrap();
                        let command=serde_json::from_value(serde_json::json!({"command":"query_paths","root_path":"/content/example"})).unwrap();
                        for (now,attempt) in [(1000,identity.attempt),(9_000_000,99)] {
                            // A fresh database handle and a later invocation must
                            // not manufacture another automatic acquisition cycle.
                            let reopened_operations=OperationRepository::new(OperationDatabase::open(&path,settings()).unwrap());
                            let reopened_remote=AgentJobRepository::new(OperationDatabase::open(&path,settings()).unwrap());
                            let capacity=PersistentCapacityAccount::new(reopened_operations.database(),slingshot_domain::persistent_capacity::PersistentCapacityPolicy::embedded());
                            let mut resumed_identity=identity.clone(); resumed_identity.attempt=attempt;
                            let protocol=RetainedAuthorProtocol::new(&reopened_operations,&reopened_remote,&store,&capacity,&authentication,resumed_identity.clone(),submission.clone(),now).unwrap();
                            let ports=ProductAuthorPorts::new(provider.snapshot().author_connection(),&protocol).unwrap();
                            let outcome=timeout(Duration::from_secs(1),AuthorAgentOperationExecutor::over(&ports).execute(&resumed_identity,&command,&Progress)).await.unwrap();
                            assert_eq!(outcome,OperationExecutorOutcome::RecoveryRequired { recovery:retry.clone() });
                            assert_eq!(reopened_operations.read(target,&identity.operation_identifier).unwrap().unwrap(),local_after);
                            assert_eq!(reopened_remote.read(target,&submission.operation.agent_operation_identifier).unwrap().unwrap(),remote_before);
                            assert_eq!(ledger.read_subscription(target,"subscription-one").unwrap().unwrap(),held);
                            assert!(timeout(Duration::from_millis(10),listener.accept()).await.is_err(),"paused concrete adapter opened a socket");
                        }
                    }
                } else if defect.starts_with("terminal-complete") {
                    assert!(local_after.record.lifecycle_state.is_terminal());
                    if defect == "terminal-complete" {
                        assert_eq!(local_after.record.lifecycle_state,slingshot_domain::operation::OperationLifecycleState::Succeeded);
                        assert_eq!(local_after.result_inline_bytes.as_deref(),Some(r#"{"matches":[]}"#));
                    } else {
                        assert_eq!(local_after.record.terminal_failure.as_ref().unwrap().kind,slingshot_domain::operation::TerminalFailureKind::Rejected);
                        assert_eq!(local_after.result_inline_bytes,None);
                    }
                    let completed = repository.read(target,&submission.operation.agent_operation_identifier).unwrap().unwrap();
                    assert!(ledger.read_recovery_view(target,"subscription-one").unwrap().members().is_empty());
                    for conflict in [false,true] {
                        let replay_ledger = AgentSubscriptionLedger::new(OperationDatabase::open(&path,settings()).unwrap());
                        let replay_operations = OperationRepository::new(OperationDatabase::open(&path,settings()).unwrap());
                        let mut replay = second.clone(); if conflict { replay["progress"] = 41.into(); }
                        let cursor = if conflict {"cursor-003"} else {"cursor-002"};
                        let previous = if conflict {"cursor-002"} else {"cursor-001"};
                        let body = format!("id:{cursor}\ndata:{replay}\n\n");
                        let peer = async {
                            let (mut socket,_) = listener.accept().await.unwrap(); let mut request=Vec::new();
                            if http2 {
                                let mut preface=[0;39]; socket.read_exact(&mut preface).await.unwrap(); socket.write_all(&[0,0,0,4,0,0,0,0,0]).await.unwrap();
                                let mut header=[0;9]; socket.read_exact(&mut header).await.unwrap(); socket.write_all(&header).await.unwrap();
                                socket.read_exact(&mut header).await.unwrap(); let length=usize::from(header[0])<<16|usize::from(header[1])<<8|usize::from(header[2]);
                                assert!(length<=16384); request.resize(length,0); socket.read_exact(&mut request).await.unwrap();
                            } else {while !request.ends_with(b"\r\n\r\n") {request.push(socket.read_u8().await.unwrap()); assert!(request.len()<=8192);}}
                            assert!(request.windows(previous.len()).any(|bytes|bytes==previous.as_bytes()));
                            authentication.lend_value_bytes(|value|assert!(request.windows(value.len()).any(|bytes|bytes==value)));
                            if http2 {
                                let mut block=vec![0x88]; for (name,value) in [("content-type","text/event-stream".to_owned()),("content-length",body.len().to_string())] {
                                    block.extend_from_slice(&[0,name.len() as u8]); block.extend_from_slice(name.as_bytes()); block.push(value.len() as u8); block.extend_from_slice(value.as_bytes());
                                }
                                for (kind,flags,bytes) in [(1,4,block.as_slice()),(0,1,body.as_bytes())] {let length=(bytes.len() as u32).to_be_bytes(); socket.write_all(&[length[1],length[2],length[3],kind,flags,0,0,0,1]).await.unwrap(); socket.write_all(bytes).await.unwrap();}
                                let mut close=Vec::new(); let _=socket.read_to_end(&mut close).await;
                            } else {socket.write_all(format!("HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nContent-Length: {}\r\n\r\n{body}",body.len()).as_bytes()).await.unwrap();}
                        };
                        let (outcome,())=timeout(Duration::from_secs(5),async {tokio::join!(attach_selected_events(&replay_ledger,&replay_operations,&transport,&identity,"subscription-one",selected_authentication,2000),peer)}).await.unwrap();
                        let outcome=outcome.unwrap();
                        if conflict {assert!(matches!(outcome,SelectedEventAttachmentOutcome::Recovery {reason:DurableEventOutcome::NeedsIntegrityRecovery,..}));}
                        else {assert!(matches!(outcome,SelectedEventAttachmentOutcome::Transport(_)));}
                        assert_eq!(repository.read(target,&submission.operation.agent_operation_identifier).unwrap().unwrap(),completed);
                        assert_eq!(replay_operations.read(target,&identity.operation_identifier).unwrap().unwrap(),local_after);
                        let replayed=replay_ledger.read_subscription(target,"subscription-one").unwrap().unwrap();
                        assert_eq!(replayed.cursor.as_deref(),Some("cursor-002"));
                        assert_eq!(replayed.event_rows,2);
                        assert_eq!(replayed.unresolved_incident.as_deref(),if conflict {Some("cursor-003")} else {None});
                    }
                } else if ["terminal-capability-error","terminal-old","terminal-active","terminal-watermark","terminal-physical"].contains(&defect) || defect.starts_with("terminal-lookup-error") {
                    let retry=local_after.record.outstanding_recovery.as_ref().unwrap();
                    assert_eq!(local_after.record.revision,local_before.record.revision+1);
                    assert_eq!(retry.attempt_count,local_before.record.outstanding_recovery.as_ref().map_or(1,|held|held.attempt_count+1));
                    assert_eq!(retry.evidence,local_before.record.outstanding_recovery.as_ref().map_or(
                        slingshot_domain::operation::RecoveryExecutionEvidence::ExecutionCertainty {certainty:slingshot_domain::operation::OperationExecutionCertainty::RemoteOutcomeUnknown},|held|held.evidence));
                    assert_eq!(retry.category,if defect=="terminal-lookup-error-success" {slingshot_domain::operation::RecoveryCategory::ResultAcquisition} else {slingshot_domain::operation::RecoveryCategory::OperationLookup});
                    assert!(retry.retry_delay_milliseconds<=slingshot_daemon::operation::recovery_and_event_supervisor::jitter_ceiling_milliseconds(u64::from(retry.attempt_count)));
                    assert_eq!(retry.manual_resume_eligible,defect=="terminal-lookup-error-cap");
                    if retry.manual_resume_eligible {assert_eq!(retry.retry_delay_milliseconds,0);}
                    assert!(!local_after.record.lifecycle_state.is_terminal());
                    assert_eq!(repository.read(target,&submission.operation.agent_operation_identifier).unwrap().unwrap(),remote_before);
                } else {
                    assert_eq!(local_after,local_before);
                    assert_eq!(repository.read(target,&submission.operation.agent_operation_identifier).unwrap().unwrap(),remote_before);
                }
                assert!(timeout(Duration::from_millis(10),listener.accept()).await.is_err());
            }
            if defect == "gap" { assert_eq!(outcomes.last(),Some(&DurableEventOutcome::NeedsSnapshot)); }
            if defect.ends_with("conflict") {
                assert_eq!(outcomes.last(),Some(&DurableEventOutcome::NeedsIntegrityRecovery));
                let reopened = AgentSubscriptionLedger::new(OperationDatabase::open(&path,settings()).unwrap());
                let view = reopened.read_recovery_view(target,"subscription-one").unwrap();
                assert_eq!(view.ledger().unresolved_incident.as_deref(),Some(if defect == "job-conflict" {"cursor-002"} else {"cursor-001"}));
                assert_eq!(view.ledger().unresolved_incident_count,1);
                reopened.record_event_conflict(&view,"cursor-999").unwrap();
                assert_eq!(reopened.read_subscription(target,"subscription-one").unwrap().unwrap(),*view.ledger());
                if owned {
                    let stopped = attach_selected_events(&reopened,&operations,&transport,&identity,"subscription-one",selected_authentication,1002).await.unwrap();
                    assert!(matches!(stopped,SelectedEventAttachmentOutcome::Recovery {agent_operation_identifier:None,reason:DurableEventOutcome::NeedsIntegrityRecovery}));
                    assert!(timeout(Duration::from_millis(20),listener.accept()).await.is_err());
                    if mode == 4 && defect == "job-conflict" {
                        for revision_only in [false, true] {
                            let foreign_endpoint = if revision_only {endpoint.clone()} else {format!("{endpoint}/foreign")};
                            let publisher = if revision_only {"http://another-publisher.example.com"} else {"http://publish.example.com"};
                            let foreign = EnvironmentAuthenticationProvider::new(selected_snapshot_with_publisher(&foreign_endpoint, publisher), 2);
                            let foreign_async = slingshot_agent_connection::authentication::environment_provider::AsyncEnvironmentAuthenticationProvider::new_async(selected_snapshot_with_publisher(&foreign_endpoint, publisher)).unwrap();
                            assert_eq!(foreign.snapshot().target() == provider.snapshot().target(), revision_only);
                            assert_ne!(foreign.snapshot().revision(), provider.snapshot().revision());
                            let (foreign_fixed, _) = foreign.authenticate(&foreign_endpoint, 0, &NoTokenSource).unwrap();
                            for policy in [
                                AuthorAuthentication::Fixed {authentication:&foreign_fixed,protocol:ResetTransport::Automatic},
                                AuthorAuthentication::Provider {provider:&foreign,source:&NoTokenSource,clock:&NoTokenClocks},
                                AuthorAuthentication::AsyncProvider {provider:&foreign_async,clock:&NoTokenClocks,utc:&NoTokenClocks},
                            ] {
                                assert!(attach_selected_events(&reopened,&operations,&transport,&identity,"subscription-one",policy,1002).await.is_err(), "foreign policy returned retained recovery");
                                assert_eq!(reopened.read_subscription(target,"subscription-one").unwrap().unwrap(),*view.ledger());
                            }
                            assert!(timeout(Duration::from_millis(20),listener.accept()).await.is_err());
                        }
                    }
                }
            } else { assert_eq!(held.unresolved_incident,None); }
            if owned && defect.is_empty() {
                rusqlite::Connection::open(&path).unwrap().execute("UPDATE agent_operation SET argument_schema_digest = 'drift'",[]).unwrap();
                assert!(attach_selected_events(&ledger,&operations,&transport,&identity,"subscription-one",selected_authentication,1002).await.is_err());
                assert!(timeout(Duration::from_millis(20),listener.accept()).await.is_err());
                assert_eq!(ledger.read_subscription(target,"subscription-one").unwrap().unwrap(),held);
            }
        }
    }
    }
}

#[tokio::test]
async fn subscription_reset_stages_two_authenticated_snapshots_before_atomic_installation() {
    use slingshot_agent_connection::{command_submission::{ExpectedArtifactManifest, Submission}, selected_author_transport::SelectedAuthorTransport};
    use slingshot_agent_protocol::{identity::WireOperationIdentity, wire_contract::ExpectedProvenance};
    use slingshot_daemon::operation::{durable_author_submission::prepare_initial_submission, subscription_reset::{reset_active_subscription, ResetTransport, SubscriptionResetOutcome}};
    use slingshot_domain::{agent_identity::AgentEventStoreGeneration, operation_executor::ExecutionIdentity, command_fingerprint::{CommandFingerprint, FingerprintInput}, installation::InstallationIdentifier};
    use slingshot_storage::{agent_job_repository::AgentJobRepository, agent_subscription_ledger::{AgentSubscriptionLedger, EventFact}, database::{OperationDatabase, RequiredSettings}, operation_repository::{AdmissionRequest, OperationRepository}};
    use tokio::{io::{AsyncReadExt, AsyncWriteExt}, time::{timeout, Duration}};
    for mode in 0..5 {
    let http2=mode==1;
    let selected_protocol=match mode {0=>ResetTransport::Http1,1=>ResetTransport::Http2,_=>ResetTransport::Automatic};
        for defect in ["", "older", "digest", "truncated", "capture", "cancel", "retained-contract", "retained-bytes", "location", "missing", "retired", "terminal", "missing-stale", "missing-owner", "terminal-expired", "terminal-failed-expired", "paused-exhausted", "paused-capacity", "paused-after-capture", "eligible-not-paused", "generation-missing", "generation-unanswered", "generation-recovered", "generation-conflict", "generation-coherent", "generation-empty", "generation-shrunk", "generation-regression", "generation-reordered", "generation-held-conflict", "generation-held-older", "generation-settle-owner", "generation-settle-stale", "generation-settle-terminal", "generation-settle-known-success", "generation-finish", "generation-already-settled", "generation-unanswered-cap", "generation-unanswered-success", "generation-unanswered-stale", "generation-unanswered-dispatch", "generation-unanswered-dispatch-stale", "generation-unanswered-dispatch-cancel"] {
            let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
            let endpoint = format!("http://{}/aem", listener.local_addr().unwrap());
            let provider = provider(&endpoint);
            let async_provider = async_provider(&endpoint);
            let transport = SelectedAuthorTransport::new(provider.snapshot().author_connection()).unwrap();
            let (authentication, _) = provider.authenticate(&endpoint, 1, &NoTokenSource).unwrap();
            let selection = ExecutionIdentity { attempt: 1, operation_identifier: "local-0".into(), author_target_identity_digest: provider.snapshot().target().to_string(), selected_environment_revision: provider.snapshot().revision().to_string() };
            let expected = ExpectedProvenance {
                command_contract: slingshot_domain::selected_command_contract_identity::SelectedCommandContractIdentity::installed("query_paths").unwrap(),
                canonical_json_contract_digest: slingshot_domain::command::schema::canonical_contract_digest(),
                transport_contract_digest: slingshot_domain::author_agent_transport_contract::AuthorAgentTransportContract::embedded_digest(),
            };
            let root = tempfile::tempdir().unwrap(); let path = root.path().join("reset.sqlite3");
            let settings = || RequiredSettings { page_bytes: 4096, database_pages: 262144, busy_timeout_milliseconds: 5000 };
            let repository = AgentJobRepository::new(OperationDatabase::open(&path, settings()).unwrap());
            let operations = OperationRepository::new(OperationDatabase::open(&path, settings()).unwrap());
            let ledger = AgentSubscriptionLedger::new(OperationDatabase::open(&path, settings()).unwrap());
            let target = &selection.author_target_identity_digest;
            ledger.open_subscription(target, "subscription-one", 7, 1000).unwrap();
            let mut event = EventFact { agent_event_store_generation: 7, agent_operation_identifier: None, canonical_digest: "old".into(), cursor: "cursor-005".into(), event_bytes: 3, job_sequence: None };
            ledger.record_event(target, "subscription-one", &event, 1000).unwrap(); event.canonical_digest = "other".into();
            ledger.record_event(target, "subscription-one", &event, 1000).unwrap();
            let mut submissions = Vec::new();
            for number in 0..2 {
                let mut identity = selection.clone(); identity.operation_identifier = format!("local-{number}");
                let canonical = r#"{"root_path":"/content/example"}"#;
                operations.admit(&AdmissionRequest {
                    author_target_identity: "opaque-target".into(), author_target_identity_digest: target.clone(), caller_identity: None,
                    canonical_command: canonical.into(), command_fingerprint: CommandFingerprint::derive(&FingerprintInput {
                        author_target_identity_digest: target.clone(), canonical_command: canonical.into(), command_wire_name: "query_paths".into(),
                        command_semantic_contract_version: expected.command_contract.command_semantic_contract_version.clone(), selected_environment_revision: identity.selected_environment_revision.clone(),
                    }).unwrap(), command_wire_name: "query_paths".into(),
                    daemon_runtime_contract_digest: slingshot_domain::daemon_runtime_contract::DaemonRuntimeContract::embedded_digest().as_text().into(),
                    installation_identifier: InstallationIdentifier::parse(&"a1".repeat(32)).unwrap(), operation_identifier: identity.operation_identifier.clone(),
                    selected_environment_revision: identity.selected_environment_revision.clone(), workflow_correlation_identifier: None,
                }, 1000).unwrap();
                let submission = Submission::build(&expected, WireOperationIdentity::of(target, &identity.selected_environment_revision, &identity.operation_identifier, AgentEventStoreGeneration::of(7)), "subscription-one", canonical, ExpectedArtifactManifest::empty()).unwrap();
                drop(prepare_initial_submission(&repository, &identity, &submission, 1000).unwrap().unwrap());
                submissions.push(submission);
            }
            submissions.sort_by(|a, b| a.operation.agent_operation_identifier.cmp(&b.operation.agent_operation_identifier));
            let first_local = repository.read(target, &submissions[0].operation.agent_operation_identifier).unwrap().unwrap().identity.operation_identifier;
            if defect.starts_with("generation-") && !["generation-empty", "generation-finish"].contains(&defect) {
                let retained = repository.read(target, &submissions[0].operation.agent_operation_identifier).unwrap().unwrap();
                repository.acknowledge(&retained, &["job-1".into(), "job-2".into()], 120000, 1000).unwrap();
                if defect.starts_with("generation-held-") {
                    use slingshot_domain::remote_job::{AgentJobState, JobEventSequence, RemoteJobObservation};
                    let retained = repository.read(target, &submissions[0].operation.agent_operation_identifier).unwrap().unwrap();
                    repository.reconcile_active_snapshot(&retained, &["job-1".into(), "job-2".into()], RemoteJobObservation {
                        state: AgentJobState::Running, applied_sequence: JobEventSequence::of(if defect == "generation-held-older" {4} else {3}), attempt: 1, progress: 10,
                    }, 120000, 1000).unwrap();
                }
            }
            let set_recovery = || {
                use slingshot_domain::operation::{OperationFact, RecoveryFact, RecoveryCategory, RecoveryExecutionEvidence, OperationExecutionCertainty};
                let capacity = defect == "paused-capacity";
                let success = ["generation-settle-known-success", "generation-unanswered-success"].contains(&defect);
                operations.apply(target, &first_local, 1, &OperationFact::Recovery { recovery: RecoveryFact {
                    attempt_count: if capacity || success || defect.starts_with("generation-unanswered-dispatch") || ["eligible-not-paused", "generation-settle-stale", "generation-unanswered-stale"].contains(&defect) {0} else {slingshot_daemon::operation::recovery_and_event_supervisor::automatic_attempt_cap() as u32 - u32::from(defect == "generation-unanswered-cap")},
                    category: if capacity {RecoveryCategory::PersistentCapacityUnavailable} else if success {RecoveryCategory::ResultAcquisition} else {RecoveryCategory::OperationLookup}, detail: "held recovery".into(),
                    evidence: if capacity || success {RecoveryExecutionEvidence::AuthoritativeRemoteSuccess} else {RecoveryExecutionEvidence::ExecutionCertainty { certainty: OperationExecutionCertainty::RemoteOutcomeUnknown }},
                    manual_resume_eligible: true, retry_delay_milliseconds: if defect.starts_with("generation-unanswered-dispatch") {50} else {0}, retry_observed_at_unix_milliseconds: 1000,
                } }, 1000).unwrap();
            };
            if defect.starts_with("generation-unanswered-dispatch") || ["paused-exhausted", "paused-capacity", "eligible-not-paused", "generation-settle-known-success", "generation-unanswered-success", "generation-unanswered-cap"].contains(&defect) { set_recovery(); }
            if defect == "retained-contract" {
                rusqlite::Connection::open(&path).unwrap().execute_batch("UPDATE agent_operation SET command_contract_limits_digest = 'changed';").unwrap();
            }
            if defect == "retained-bytes" {
                rusqlite::Connection::open(&path).unwrap().execute_batch("UPDATE agent_operation SET canonical_submission = ' ' || canonical_submission;").unwrap();
            }
            if defect == "generation-already-settled" {
                use slingshot_domain::operation::{OperationFact, TerminalFailure, TerminalFailureKind, TerminalFailureDisposition, OperationExecutionCertainty};
                for member in ledger.read_recovery_view(target, "subscription-one").unwrap().members() {
                    operations.apply(target, &member.identity.operation_identifier, 1, &OperationFact::Terminal { failure: TerminalFailure {
                        kind: TerminalFailureKind::RemoteStateLost,
                        disposition: TerminalFailureDisposition::FailClosedIndeterminate { certainty: OperationExecutionCertainty::RemoteOutcomeUnknown }, metadata: None,
                    } }, 1000).unwrap();
                }
            }
            let before = ledger.read_recovery_view(target, "subscription-one").unwrap();
            let (cancel_ready, ready) = tokio::sync::oneshot::channel();
            let scheduled_at = std::cell::Cell::new(None::<tokio::time::Instant>);
            let peer = async {
                let mut cancel_ready = Some(cancel_ready);
                for stage in 0..if defect.starts_with("retained-") || ["paused-exhausted", "paused-capacity"].contains(&defect) {0} else if ["capture", "paused-after-capture", "generation-empty", "generation-finish", "generation-already-settled", "generation-unanswered-dispatch-stale", "generation-unanswered-dispatch-cancel"].contains(&defect) {1} else {3} {
                    let (mut socket, _) = listener.accept().await.unwrap();
                    if stage == 1 && defect == "generation-unanswered-dispatch" {
                        assert!(scheduled_at.get().unwrap().elapsed() >= Duration::from_millis(50));
                    }
                    let mut request = Vec::new();
                    if http2 {
                        let mut preface = [0; 39]; socket.read_exact(&mut preface).await.unwrap();
                        assert_eq!(&preface[..24], b"PRI * HTTP/2.0\r\n\r\nSM\r\n\r\n");
                        socket.write_all(&[0,0,0,4,0,0,0,0,0]).await.unwrap();
                        let mut header = [0; 9]; socket.read_exact(&mut header).await.unwrap(); socket.write_all(&header).await.unwrap();
                        socket.read_exact(&mut header).await.unwrap(); assert_eq!((header[3], header[4]), (1, 5));
                        let length = usize::from(header[0]) << 16 | usize::from(header[1]) << 8 | usize::from(header[2]);
                        assert!(length <= 16384); request.resize(length, 0); socket.read_exact(&mut request).await.unwrap();
                    } else {
                        while !request.ends_with(b"\r\n\r\n") { request.push(socket.read_u8().await.unwrap()); assert!(request.len() <= 8192); }
                        assert!(request.starts_with(b"GET "));
                    }
                    let route = if stage == 0 { "/aem/bin/slingshot-agent/events/high-water?agent_event_store_generation=7&daemon_subscription_identifier=subscription-one".into() }
                        else if defect.starts_with("generation-") { format!("/aem/bin/slingshot-agent/jobs/snapshot?sling_job_identifier=job-{stage}") }
                        else { format!("/aem/bin/slingshot-agent/operations/lookup?agent_operation_identifier={}", submissions[stage-1].operation.agent_operation_identifier) };
                    assert!(request.windows(route.len()).any(|bytes| bytes == route.as_bytes()));
                    authentication.lend_value_bytes(|value| assert!(request.windows(value.len()).any(|bytes| bytes == value)));
                    assert_eq!(ledger.read_subscription(target, "subscription-one").unwrap().unwrap(), *before.ledger());
                    assert_eq!(ledger.read_recovery_view(target, "subscription-one").unwrap().members(), before.members());
                    if defect == "paused-after-capture" { set_recovery(); }
                    if defect == "cancel" && stage == 2 {
                        cancel_ready.take().unwrap().send(()).unwrap();
                        let mut discarded = Vec::new(); let _ = socket.read_to_end(&mut discarded).await;
                        break;
                    }
                    let mut body = if stage == 0 { serde_json::json!({
                        "format":"slingshot.agent/1", "transport_contract_digest":expected.transport_contract_digest,
                        "daemon_subscription_identifier":if defect == "capture" {"wrong"} else {"subscription-one"},
                        "agent_event_store_generation":7, "high_water_cursor":"cursor-010",
                    }) } else {
                        let submission = &submissions[if defect.starts_with("generation-") {0} else {stage-1}];
                        serde_json::json!({ "provenance":submission.provenance, "agent_event_store_generation":7,
                            "agent_operation_identifier":submission.operation.agent_operation_identifier, "author_target_identity_digest":target,
                            "selected_environment_revision":selection.selected_environment_revision, "daemon_subscription_identifier":"subscription-one",
                            "submitted_command_digest":if defect == "digest" && stage == 2 {"0".repeat(64)} else {submission.submitted_command_digest.clone()},
                            "subscription_watermark":if defect == "older" && stage == 2 {"cursor-009"} else {"cursor-010"},
                            "physical_sling_job_identifiers":[format!("job-{stage}")], "granted_retention_milliseconds":120000,
                            "attempt":1, "progress":10, "sequence":3, "kind":"progress",
                        })
                    };
                    let status = if defect.starts_with("generation-") {
                        if stage == 0 {
                            body = serde_json::json!({"format":"slingshot.agent/1", "transport_contract_digest":expected.transport_contract_digest,
                                "daemon_subscription_identifier":"subscription-one", "requested_agent_event_store_generation":7,
                                "requested_last_event_identifier":null, "agent_event_store_generation":8, "high_water_cursor":"cursor-010", "reason":"generation_changed"});
                            409
                        } else if defect == "generation-missing" || defect.starts_with("generation-unanswered") || (defect == "generation-recovered" && stage == 1) {
                            body = serde_json::json!({"format":"slingshot.agent/1", "transport_contract_digest":expected.transport_contract_digest,
                                "kind":"missing", "agent_event_store_generation":if defect.starts_with("generation-unanswered") && stage == 2 {9} else {8}, "sling_job_identifier":format!("job-{stage}")});
                            404
                        } else {
                            body["physical_sling_job_identifiers"] = serde_json::json!(["job-1", "job-2"]);
                            if defect == "generation-held-conflict" { body["progress"] = serde_json::json!(11); }
                            if defect == "generation-settle-terminal" { body["kind"] = serde_json::json!("succeeded"); }
                            if stage == 2 && defect == "generation-conflict" { body["progress"] = serde_json::json!(11); }
                            if stage == 2 && defect == "generation-coherent" { body["progress"] = serde_json::json!(11); body["sequence"] = serde_json::json!(4); }
                            if stage == 2 && defect == "generation-shrunk" { body["physical_sling_job_identifiers"] = serde_json::json!(["job-2"]); }
                            if stage == 2 && defect == "generation-regression" { body["progress"] = serde_json::json!(9); body["sequence"] = serde_json::json!(4); }
                            if stage == 1 && defect == "generation-reordered" { body["progress"] = serde_json::json!(11); body["sequence"] = serde_json::json!(4); }
                            200
                        }
                    } else if stage == 2 && defect.starts_with("missing") {
                        body = serde_json::json!({"kind":"missing", "format":"slingshot.agent/1", "transport_contract_digest":expected.transport_contract_digest,
                            "agent_event_store_generation":7, "agent_operation_identifier":submissions[1].operation.agent_operation_identifier, "author_target_identity_digest":target});
                        404
                    } else if stage == 2 && defect == "retired" {
                        for field in ["subscription_watermark", "physical_sling_job_identifiers", "granted_retention_milliseconds", "attempt", "progress", "sequence"] { body.as_object_mut().unwrap().remove(field); }
                        body["kind"] = serde_json::json!("retired"); 410
                    } else {
                        if stage == 2 && defect.starts_with("terminal") { body["kind"] = serde_json::json!("succeeded"); }
                        if stage == 2 && defect.ends_with("expired") { body["granted_retention_milliseconds"] = serde_json::json!(500); }
                        if stage == 2 && defect == "terminal-failed-expired" {
                            body["kind"] = serde_json::json!("failed");
                            body["terminal_failure"] = serde_json::json!({
                                "operation":submissions[1].operation, "daemon_subscription_identifier":"subscription-one",
                                "provenance":submissions[1].provenance, "submitted_command_digest":submissions[1].submitted_command_digest,
                                "canonical_failure":r#"{"failure":"root_not_found","root_path":"/content/example"}"#,
                            });
                        }
                        200
                    };
                    let body = serde_json::to_vec(&body).unwrap();
                    let payload = if defect == "truncated" && stage == 2 { &body[..body.len()-1] } else { &body };
                    if http2 {
                        let mut block = vec![0, 7]; block.extend_from_slice(b":status"); block.push(3); block.extend_from_slice(status.to_string().as_bytes());
                        for (name, value) in [("content-type", "application/json".to_owned()), ("content-length", body.len().to_string())] {
                            block.extend_from_slice(&[0, name.len() as u8]); block.extend_from_slice(name.as_bytes()); block.push(value.len() as u8); block.extend_from_slice(value.as_bytes());
                        }
                        if defect == "location" && stage == 2 { block.extend_from_slice(b"\x00\x08location\x0a/elsewhere"); }
                        for (kind, flags, bytes) in [(1, 4, block.as_slice()), (0, 1, payload)] {
                            let length = (bytes.len() as u32).to_be_bytes(); let header = [length[1], length[2], length[3], kind, flags, 0,0,0,1];
                            socket.write_all(&header).await.unwrap(); socket.write_all(bytes).await.unwrap();
                        }
                        let mut close = Vec::new(); let _ = socket.read_to_end(&mut close).await;
                    } else {
                        socket.write_all(format!("HTTP/1.1 {status} Reply\r\nContent-Type: application/json\r\nContent-Length: {}\r\n{}\r\n", body.len(), if defect == "location" && stage == 2 {"Location: /elsewhere\r\n"} else {""}).as_bytes()).await.unwrap();
                        socket.write_all(payload).await.unwrap();
                    }
                    socket.shutdown().await.unwrap();
                }
            };
            let clock = AuthenticationClock(std::cell::Cell::new(0));
            let request = async {
                use slingshot_daemon::operation::author_authentication::AuthorAuthentication;
                let selected_authentication = if mode==4 {AuthorAuthentication::AsyncProvider {provider:&async_provider,clock:&NoTokenClocks,utc:&NoTokenClocks}} else if mode==3 {AuthorAuthentication::Provider {provider:&provider,source:&NoTokenSource,clock:&clock}}
                    else {AuthorAuthentication::Fixed {authentication:&authentication,protocol:selected_protocol}};
                let attempt = async {if mode>=3 {
                    slingshot_daemon::operation::subscription_reset::reset_active_subscription_with_authentication(
                        &ledger,&operations,&transport,&selection,"subscription-one",
                        selected_authentication,1000).await
                } else {reset_active_subscription(&ledger, &operations, &transport, &selection, "subscription-one", &authentication,
                    selected_protocol, 1000).await}};
                if defect == "cancel" {
                    tokio::select! { result = attempt => Ok(result), _ = ready => Err(()) }
                } else {
                    let result = timeout(Duration::from_secs(5), attempt).await.map_err(|_| ())?;
                    if defect.starts_with("generation-") && defect != "generation-already-settled" {
                        use slingshot_daemon::operation::generation_loss_probe::{probe_generation_loss_with_authentication as probe_generation_loss, PhysicalRecoveryStatus};
                        let SubscriptionResetOutcome::GenerationChanged(reset) = result.as_ref().unwrap() else { panic!("generation change not captured: {defect}"); };
                        let mut identity = selection.clone(); identity.operation_identifier = first_local.clone();
                        if ["generation-recovered", "generation-coherent", "generation-reordered", "generation-missing", "generation-empty", "generation-conflict", "generation-unanswered-success"].contains(&defect) {
                            use slingshot_daemon::operation::generation_recovery_dispatch::{ScheduledGenerationRecovery, GenerationDispatchOutcome};
                            if mode == 4 && defect == "generation-recovered" {
                                for revision_only in [false, true] {
                                    let foreign_endpoint = if revision_only {endpoint.clone()} else {format!("{endpoint}/foreign")};
                                    let publisher = if revision_only {"http://another-publisher.example.com"} else {"http://publish.example.com"};
                                    let foreign = EnvironmentAuthenticationProvider::new(selected_snapshot_with_publisher(&foreign_endpoint, publisher), 2);
                                    let foreign_async = slingshot_agent_connection::authentication::environment_provider::AsyncEnvironmentAuthenticationProvider::new_async(selected_snapshot_with_publisher(&foreign_endpoint, publisher)).unwrap();
                                    assert_eq!(foreign.snapshot().target() == provider.snapshot().target(), revision_only);
                                    assert_ne!(foreign.snapshot().revision(), provider.snapshot().revision());
                                    let (foreign_fixed, _) = foreign.authenticate(&foreign_endpoint, 0, &NoTokenSource).unwrap();
                                    let before = operations.read(target,&first_local).unwrap();
                                    let held = ledger.read_subscription(target,"subscription-one").unwrap();
                                    for policy in [
                                        AuthorAuthentication::Fixed {authentication:&foreign_fixed,protocol:ResetTransport::Automatic},
                                        AuthorAuthentication::Provider {provider:&foreign,source:&NoTokenSource,clock:&NoTokenClocks},
                                        AuthorAuthentication::AsyncProvider {provider:&foreign_async,clock:&NoTokenClocks,utc:&NoTokenClocks},
                                    ] {
                                        assert!(ScheduledGenerationRecovery::new_with_authentication(&ledger,&operations,&repository,&transport,policy,reset,&identity,1000).is_err(), "foreign generation recovery policy captured");
                                        assert_eq!(operations.read(target,&first_local).unwrap(),before);
                                        assert_eq!(ledger.read_subscription(target,"subscription-one").unwrap(),held);
                                    }
                                }
                            }
                            let outcome = ScheduledGenerationRecovery::new_with_authentication(&ledger, &operations, &repository, &transport, selected_authentication, reset, &identity,
                                1000).unwrap().run(None).await.unwrap();
                            match (defect, outcome) {
                                ("generation-recovered" | "generation-coherent" | "generation-reordered", GenerationDispatchOutcome::Reconciled(slingshot_agent_connection::selected_author_lookup::OperationLookupReceipt::Found(found))) => {
                                    assert_eq!(found.snapshot.sequence.value(), if defect == "generation-recovered" {3} else {4});
                                },
                                ("generation-missing" | "generation-empty", GenerationDispatchOutcome::Settled(local)) => {
                                    assert_eq!(local.record.terminal_failure.unwrap().kind, slingshot_domain::operation::TerminalFailureKind::RemoteStateLost);
                                },
                                ("generation-conflict", GenerationDispatchOutcome::NeedsIntegrityRecovery) => {},
                                ("generation-unanswered-success", GenerationDispatchOutcome::Deferred(local)) => {
                                    assert_eq!(local.record.outstanding_recovery.unwrap().evidence, slingshot_domain::operation::RecoveryExecutionEvidence::AuthoritativeRemoteSuccess);
                                },
                                _ => panic!("wrong scheduled recovery outcome: {defect}"),
                            }
                            return Ok(result);
                        }
                        if defect.starts_with("generation-unanswered-dispatch") {
                            use slingshot_daemon::operation::generation_recovery_dispatch::{ScheduledGenerationRecovery, GenerationDispatchOutcome};
                            if defect.ends_with("cancel") { tokio::time::pause(); }
                            scheduled_at.set(Some(tokio::time::Instant::now()));
                            let scheduled = ScheduledGenerationRecovery::new_with_authentication(&ledger, &operations, &repository, &transport, selected_authentication, reset, &identity,
                                if defect == "generation-unanswered-dispatch" {900} else {1000}).unwrap();
                            if defect.ends_with("stale") {
                                let mut recovery = operations.read(target, &first_local).unwrap().unwrap().record.outstanding_recovery.unwrap();
                                recovery.detail = "changed while scheduled".into();
                                operations.apply(target, &first_local, 2, &slingshot_domain::operation::OperationFact::Recovery {recovery}, 1000).unwrap();
                                assert!(scheduled.run(None).await.is_err());
                            } else if defect.ends_with("cancel") {
                                let mut waiting = Box::pin(scheduled.run(None));
                                std::future::poll_fn(|context| {
                                    assert!(std::future::Future::poll(waiting.as_mut(), context).is_pending());
                                    std::task::Poll::Ready(())
                                }).await;
                                drop(waiting);
                                tokio::time::resume();
                            } else {
                                let GenerationDispatchOutcome::Deferred(local) = scheduled.run(None).await.unwrap() else {panic!("unanswered dispatch did not defer");};
                                let recovery = local.record.outstanding_recovery.unwrap();
                                assert_eq!(recovery.attempt_count, 1);
                                assert!(recovery.retry_observed_at_unix_milliseconds >= 1050);
                            }
                            return Ok(result);
                        }
                        let report = probe_generation_loss(&ledger, &operations, &transport, &identity, reset, selected_authentication).await.unwrap();
                        assert_eq!(report.status(), match defect {
                            "generation-missing" => PhysicalRecoveryStatus::AllMissing,
                            name if name.starts_with("generation-unanswered") => PhysicalRecoveryStatus::Unanswered,
                            "generation-conflict" | "generation-shrunk" | "generation-regression" | "generation-held-conflict" | "generation-held-older" => PhysicalRecoveryStatus::ConflictingSnapshots,
                            "generation-empty" | "generation-finish" => PhysicalRecoveryStatus::NoPhysicalJobs,
                            _ => PhysicalRecoveryStatus::Recovered,
                        }, "{defect} http2={http2}");
                        if defect == "generation-finish" {
                            use slingshot_daemon::operation::subscription_reset::finish_generation_reset;
                            assert!(finish_generation_reset(&ledger, &operations, &transport, &selection, reset).is_err());
                            report.settle_unavailable(&transport, 1000).unwrap();
                            assert!(finish_generation_reset(&ledger, &operations, &transport, &selection, reset).is_err());
                            identity.operation_identifier = before.members()[1].identity.operation_identifier.clone();
                            let second = probe_generation_loss(&ledger, &operations, &transport, &identity, reset, selected_authentication).await.unwrap();
                            assert_eq!(second.status(), PhysicalRecoveryStatus::NoPhysicalJobs);
                            second.settle_unavailable(&transport, 1000).unwrap();
                            let cursor = finish_generation_reset(&ledger, &operations, &transport, &selection, reset).unwrap();
                            assert_eq!(cursor.as_text(), "cursor-010");
                            assert!(finish_generation_reset(&ledger, &operations, &transport, &selection, reset).is_err());
                        } else if defect.starts_with("generation-unanswered") {
                            if defect == "generation-unanswered-stale" { set_recovery(); }
                            let deferred = report.defer_unanswered(&transport, 1000);
                            if defect == "generation-unanswered-stale" { assert!(deferred.is_err()); }
                            else {
                                let local = deferred.unwrap(); assert!(!local.record.lifecycle_state.is_terminal());
                                let recovery = local.record.outstanding_recovery.as_ref().unwrap();
                                let cap = slingshot_daemon::operation::recovery_and_event_supervisor::automatic_attempt_cap();
                                assert_eq!(u64::from(recovery.attempt_count), if defect == "generation-unanswered-cap" {cap} else {1});
                                assert_eq!(recovery.manual_resume_eligible, defect == "generation-unanswered-cap");
                                assert!(recovery.retry_delay_milliseconds <= slingshot_daemon::operation::recovery_and_event_supervisor::jitter_ceiling_milliseconds(u64::from(recovery.attempt_count)));
                                if defect == "generation-unanswered-cap" {
                                    assert_eq!(recovery.retry_delay_milliseconds, 0);
                                    assert!(probe_generation_loss(&ledger, &operations, &transport, &identity, reset, selected_authentication).await.is_err());
                                }
                            }
                        } else if ["generation-missing", "generation-empty", "generation-conflict"].contains(&defect) {
                            let settled = report.settle_unavailable(&transport, 1000);
                            if ["generation-missing", "generation-empty"].contains(&defect) {
                                use slingshot_domain::operation::{TerminalFailureKind, TerminalFailureDisposition, OperationExecutionCertainty};
                                let local = settled.unwrap();
                                let failure = local.record.terminal_failure.unwrap();
                                assert_eq!(failure.kind, TerminalFailureKind::RemoteStateLost);
                                assert_eq!(failure.disposition, TerminalFailureDisposition::FailClosedIndeterminate { certainty: OperationExecutionCertainty::RemoteOutcomeUnknown });
                            } else { assert!(settled.is_err()); }
                        } else if ["generation-recovered", "generation-coherent", "generation-reordered"].contains(&defect) || defect.starts_with("generation-settle-") {
                            if defect == "generation-settle-stale" { set_recovery(); }
                            let another = AgentJobRepository::new(OperationDatabase::open(&root.path().join("physical-other.sqlite3"), settings()).unwrap());
                            let reconciled = report.reconcile_saved(if defect == "generation-settle-owner" {&another} else {&repository}, &transport, 1000, None).await;
                            if ["generation-settle-owner", "generation-settle-stale", "generation-settle-known-success"].contains(&defect) { assert!(reconciled.is_err()); }
                            else {
                                let slingshot_agent_connection::selected_author_lookup::OperationLookupReceipt::Found(found) = reconciled.unwrap() else { panic!("found receipt lost"); };
                                assert_eq!(found.snapshot.sequence.value(), if ["generation-coherent", "generation-reordered"].contains(&defect) {4} else {3});
                                assert!(found.remaining_retention_milliseconds > 0 && found.remaining_retention_milliseconds < 120000);
                            }
                        } else {
                        let results = report.into_results();
                        assert_eq!(results.len(), if defect == "generation-empty" {0} else {2});
                        for (index, result) in results.into_iter().enumerate() {
                            assert_eq!(result.identifier(), format!("job-{}", index + 1));
                            if let Ok(slingshot_agent_connection::selected_author_lookup::PhysicalLookupReceipt::Found(found)) = result.into_result() {
                                assert!(found.remaining_retention_milliseconds > 0 && found.remaining_retention_milliseconds < 120000);
                            }
                        }
                        }
                        if defect != "generation-finish" {
                            assert!(slingshot_daemon::operation::subscription_reset::finish_generation_reset(&ledger, &operations, &transport, &selection, reset).is_err());
                        }
                    }
                    Ok(result)
                }
            };
            let (result, ()) = timeout(Duration::from_secs(10), async { tokio::join!(request, peer) }).await.unwrap();
            let reopened = AgentSubscriptionLedger::new(OperationDatabase::open(&path, settings()).unwrap());
            let after = reopened.read_recovery_view(target, "subscription-one").unwrap();
            if defect.is_empty() || defect == "eligible-not-paused" {
                let SubscriptionResetOutcome::Installed { generation, cursor } = result.unwrap().unwrap() else { panic!("reset not installed"); };
                assert_eq!(generation, 7); assert_eq!(cursor.as_text(), "cursor-010"); assert_eq!(after.ledger().cursor.as_deref(), Some("cursor-010"));
                assert_eq!(after.ledger().unresolved_incident, None);
                for child in after.members() { assert_eq!(child.observation.applied_sequence.value(), 3); assert!(child.remaining_retention_milliseconds > 0 && child.remaining_retention_milliseconds <= 120000); }
            } else if ["generation-finish", "generation-already-settled"].contains(&defect) {
                if defect == "generation-finish" {
                    assert!(matches!(result.unwrap().unwrap(), SubscriptionResetOutcome::GenerationChanged(_)));
                } else {
                    let SubscriptionResetOutcome::Installed {generation, cursor} = result.unwrap().unwrap() else { panic!("settled membership failed to resume"); };
                    assert_eq!(generation, 8); assert_eq!(cursor.as_text(), "cursor-010");
                    for submission in &submissions {
                        let child = repository.read(target, &submission.operation.agent_operation_identifier).unwrap().unwrap();
                        assert_eq!(child.identity.agent_event_store_generation, 7);
                        assert!(child.terminal_disposition.is_none());
                    }
                }
                assert!(after.members().is_empty());
                assert_eq!(after.ledger().agent_event_store_generation, 8);
                assert_eq!(after.ledger().cursor.as_deref(), Some("cursor-010"));
                assert_eq!(after.ledger().unresolved_incident, None);
                for child in before.members() {
                    assert_eq!(repository.read(target, &child.identity.agent_operation_identifier).unwrap().as_ref(), Some(child));
                    assert!(operations.read(target, &child.identity.operation_identifier).unwrap().unwrap().record.lifecycle_state.is_terminal());
                }
            } else if defect.starts_with("generation-") {
                assert!(matches!(result.unwrap().unwrap(), SubscriptionResetOutcome::GenerationChanged(_)));
                assert_eq!(after.ledger(), before.ledger());
                if ["generation-recovered", "generation-coherent", "generation-reordered"].contains(&defect) {
                    let child = repository.read(target, &submissions[0].operation.agent_operation_identifier).unwrap().unwrap();
                    assert_eq!(child.observation.applied_sequence.value(), if defect == "generation-recovered" {3} else {4});
                    assert_eq!(child.observation.progress, if defect == "generation-recovered" {10} else {11});
                    assert_eq!(child.identity.agent_event_store_generation, 7);
                    assert_eq!(after.members()[1], before.members()[1]);
                } else if ["generation-missing", "generation-empty"].contains(&defect) {
                    assert_eq!(after.members(), &before.members()[1..]);
                    assert_eq!(repository.read(target, &submissions[0].operation.agent_operation_identifier).unwrap().as_ref(), Some(&before.members()[0]));
                } else { assert_eq!(after.members(), before.members()); }
                assert_eq!(repository.physical_jobs(target, &submissions[0].operation.agent_operation_identifier).unwrap().as_slice(), before.physical_jobs_for(&submissions[0].operation.agent_operation_identifier).unwrap());
                let local = operations.read(target, &first_local).unwrap().unwrap();
                assert_eq!(local.record.revision, if ["generation-settle-terminal", "generation-unanswered-cap", "generation-unanswered-success", "generation-unanswered-dispatch", "generation-unanswered-dispatch-stale"].contains(&defect) {3} else if defect.starts_with("generation-unanswered") || ["generation-settle-stale", "generation-settle-known-success", "generation-missing", "generation-empty"].contains(&defect) {2} else {1}, "{defect}");
                if ["generation-settle-terminal", "generation-settle-known-success", "generation-unanswered-success"].contains(&defect) {
                    assert_eq!(local.record.outstanding_recovery.as_ref().unwrap().evidence, slingshot_domain::operation::RecoveryExecutionEvidence::AuthoritativeRemoteSuccess);
                }
            } else if defect.starts_with("paused-") {
                let SubscriptionResetOutcome::NeedsOperationRecovery { operation_identifier } = result.unwrap().unwrap() else { panic!("paused member was not deferred"); };
                assert_eq!(operation_identifier, first_local);
                assert_eq!(after.ledger(), before.ledger()); assert_eq!(after.members(), before.members());
                let local = operations.read(target, &first_local).unwrap().unwrap();
                assert_eq!(local.record.revision, 2);
                assert!(local.record.outstanding_recovery.as_ref().unwrap().manual_resume_eligible);
            } else if defect.starts_with("missing") || defect.starts_with("terminal") || defect == "retired" {
                use slingshot_domain::operation::{OperationFact, RecoveryFact, RecoveryCategory, RecoveryExecutionEvidence, OperationExecutionCertainty};
                assert_eq!(after.ledger(), before.ledger()); assert_eq!(after.members(), before.members());
                let SubscriptionResetOutcome::CapturedOperationRecovery(handoff) = result.unwrap().unwrap() else { panic!("missing recovery handoff"); };
                assert_eq!(format!("{handoff:?}"), "CapturedResetRecovery([redacted])");
                let operation = handoff.operation_identifier().to_owned();
                if defect == "missing-stale" {
                    operations.apply(target, &operation, 1, &OperationFact::Recovery { recovery: RecoveryFact {
                        attempt_count: 0, category: RecoveryCategory::OperationLookup, detail: "moved".into(),
                        evidence: RecoveryExecutionEvidence::ExecutionCertainty { certainty: OperationExecutionCertainty::RemoteOutcomeUnknown },
                        manual_resume_eligible: false, retry_delay_milliseconds: 0, retry_observed_at_unix_milliseconds: 1000,
                    } }, 1000).unwrap();
                }
                let original = operations.read(target, &operation).unwrap().unwrap();
                if defect.ends_with("expired") { tokio::time::sleep(Duration::from_millis(600)).await; }
                let another = AgentJobRepository::new(OperationDatabase::open(&root.path().join("another.sqlite3"), settings()).unwrap());
                let reconciled = if mode>=3 {handoff.reconcile_saved(if defect=="missing-owner" {&another} else {&repository},&transport,None).await}
                    else {handoff.reconcile(if defect == "missing-owner" {&another} else {&repository}, &transport, &authentication, None).await};
                let held = operations.read(target, &operation).unwrap().unwrap();
                if ["missing-stale", "missing-owner"].contains(&defect) {
                    assert!(reconciled.is_err()); assert_eq!(held, original);
                } else {
                    assert!(reconciled.is_ok(), "{defect}"); assert!(held.record.revision > original.record.revision);
                    if defect.ends_with("expired") {
                        let slingshot_agent_connection::selected_author_lookup::OperationLookupReceipt::Found(found) = reconciled.as_ref().unwrap() else { panic!("terminal receipt disappeared"); };
                        assert_eq!(found.remaining_retention_milliseconds, 0);
                    }
                    if ["retired", "terminal-failed-expired"].contains(&defect) { assert!(held.record.lifecycle_state.is_terminal()); }
                    else {
                        let recovery = held.record.outstanding_recovery.as_ref().unwrap();
                        assert_eq!(recovery.category, if defect.starts_with("terminal") {RecoveryCategory::ResultAcquisition} else {RecoveryCategory::OperationLookup});
                        if defect.starts_with("terminal") { assert_eq!(recovery.evidence, RecoveryExecutionEvidence::AuthoritativeRemoteSuccess); }
                    }
                }
                assert_eq!(ledger.read_subscription(target, "subscription-one").unwrap().unwrap(), *before.ledger());
                for child in before.members().iter().filter(|child| child.identity.operation_identifier != operation) {
                    assert_eq!(repository.read(target, &child.identity.agent_operation_identifier).unwrap().as_ref(), Some(child));
                }
            } else {
                if defect == "cancel" { assert!(result.is_err()); } else { assert!(result.unwrap().is_err(), "http2={http2} defect={defect}"); }
                assert_eq!(after.ledger(), before.ledger()); assert_eq!(after.members(), before.members());
            }
            assert!(timeout(Duration::from_millis(10), listener.accept()).await.is_err(), "unexpected request: {defect} http2={http2}");
        }
    }
}

#[tokio::test]
async fn retained_package_completion_binds_download_and_atomic_success() {
    retained_artifact_completion("download_content_package", false, false, false).await;
}

#[tokio::test]
async fn retained_package_http2_lookup_keeps_discovery_snapshot_and_artifact_on_http2() {
    retained_artifact_completion_case("download_content_package", false, false, false, None, true, false).await;
}

#[tokio::test]
async fn async_provider_retained_completion_preserves_atomic_publication() {
    retained_artifact_completion_case("download_content_package", false, false, false, None, false, true).await;
    retained_artifact_completion_case("load_content_as_json", false, false, false, None, false, true).await;
}

#[tokio::test]
async fn retained_loaded_completion_binds_download_and_atomic_success() {
    retained_artifact_completion("load_content_as_json", false, false, false).await;
}

#[tokio::test]
async fn retained_artifact_capacity_refusal_pauses_before_network() {
    retained_artifact_completion("download_content_package", true, false, false).await;
    retained_artifact_completion("load_content_as_json", true, false, false).await;
}

#[tokio::test]
async fn retained_artifact_retirement_requires_verified_identity_and_preserves_success() {
    retained_artifact_completion("download_content_package", false, true, false).await;
    retained_artifact_completion("load_content_as_json", false, true, false).await;
}

#[tokio::test]
async fn concrete_failed_lookup_consumes_retry_budget_and_preserves_remote_success() {
    retained_artifact_completion("download_content_package", false, false, true).await;
}

async fn retained_artifact_completion(
    wire: &str,
    refuse_capacity: bool,
    retire: bool,
    failed_lookup: bool,
) {
    retained_artifact_completion_case(wire, refuse_capacity, retire, failed_lookup, None, false, false).await;
}

#[tokio::test]
async fn selected_failure_lookup_settles_only_validated_no_effect_and_keeps_unknown_open() {
    for (wire, failure) in [
        ("load_content_as_json", r#"{"failure":"not_found","path":"/content/example"}"#),
        ("find_open_service_gateway_initiative_configurations", r#"{"failure":"configuration_lookup_failed"}"#),
        ("find_sling_jobs", r#"{"failure":"job_inventory_failed"}"#),
        ("find_workflow_instances", r#"{"failure":"workflow_inventory_failed"}"#),
        ("list_open_service_gateway_initiative_bundles", r#"{"failure":"bundle_inventory_failed"}"#),
        ("list_open_service_gateway_initiative_components", r#"{"failure":"component_inventory_failed"}"#),
        ("list_replication_agents", r#"{"failure":"agent_inventory_failed"}"#),
        ("list_resource_mappings", r#"{"failure":"mapping_inventory_failed"}"#),
        ("list_sling_job_queues", r#"{"failure":"job_inventory_failed"}"#),
        ("list_workflow_models", r#"{"failure":"workflow_inventory_failed"}"#),
        ("read_content_fragment", r#"{"failure":"variation_not_found","fragment_path":"/content/dam/example/offer"}"#),
        ("list_child_pages", r#"{"failure":"root_access_denied","root_path":"/content/example"}"#),
        ("list_group_members", r#"{"failure":"group_not_found","group_identifier":"authors"}"#),
        ("list_asset_renditions", r#"{"asset_path":"/content/dam/example/logo.png","failure":"asset_invalid"}"#),
        ("inspect_replication_queue", r#"{"agent_identifier":"publish","failure":"queue_inventory_failed"}"#),
        ("list_group_members", r#"{"budget":"property_values","failure":"discovery_budget_exceeded"}"#),
        ("inspect_sling_job", r#"{"failure":"job_not_found","job_identifier":"2024/01/01/example-job-1"}"#),
        ("inspect_workflow_instance", r#"{"failure":"instance_access_denied","instance_identifier":"/var/workflow/instances/server0/2024-01-01/request-for-activation_1"}"#),
        ("inspect_replication_agent", r#"{"agent_identifier":"publish","failure":"agent_not_found"}"#),
        ("resolve_resource_path", r#"{"failure":"request_address_rejected","subject":"https://example.test/en/report.html"}"#),
        ("map_resource_path", r#"{"failure":"resolution_budget_exceeded","subject":"/content/example"}"#),
        ("update_open_service_gateway_initiative_configuration", r#"{"failure":"configuration_lookup_failed","persistent_identifier":"com.example.service.Configuration"}"#),
        ("delete_open_service_gateway_initiative_configuration", r#"{"failure":"platform_control_outcome_unknown","persistent_identifier":"com.example.service.Configuration"}"#),
        ("set_open_service_gateway_initiative_bundle_state", r#"{"failure":"bundle_not_found","symbolic_name":"com.example.bundle"}"#),
        ("cancel_sling_job", r#"{"failure":"job_not_found","job_identifier":"2024/01/01/example-job-1"}"#),
        ("start_workflow", r#"{"failure":"platform_control_outcome_unknown","model_identifier":"/var/workflow/models/request-for-activation/jcr:content/model"}"#),
        ("terminate_workflow_instance", r#"{"failure":"instance_not_found","instance_identifier":"/var/workflow/instances/server0/2024-01-01/request-for-activation_1"}"#),
        ("set_workflow_instance_suspension", r#"{"failure":"platform_control_outcome_unknown","instance_identifier":"/var/workflow/instances/server0/2024-01-01/request-for-activation_1"}"#),
        ("flush_replication_queue", r#"{"agent_identifier":"publish","failure":"queue_expectation_mismatch"}"#),
        ("retry_replication_queue_entry", r#"{"agent_identifier":"publish","entry_identifier":"queue-entry-1","failure":"entry_not_found"}"#),
        ("create_user", r#"{"authorizable_identifier":"author","failure":"authorizable_already_exists"}"#),
        ("create_group", r#"{"authorizable_identifier":"author","failure":"mutation_outcome_unknown"}"#),
        ("delete_authorizable", r#"{"authorizable_identifier":"author","failure":"group_has_members"}"#),
        ("update_user_profile", r#"{"authorizable_identifier":"author","failure":"authorizable_not_found"}"#),
        ("set_user_disabled", r#"{"authorizable_identifier":"author","failure":"mutation_outcome_unknown"}"#),
        ("add_group_member", r#"{"failure":"member_not_found","group_identifier":"content-authors","member_identifier":"author"}"#),
        ("remove_group_member", r#"{"failure":"group_not_found","group_identifier":"content-authors","member_identifier":"author"}"#),
        ("create_content_fragment", r#"{"failure":"parent_not_found","target_path":"/content/dam/example/fragments/offer"}"#),
        ("update_content_fragment", r#"{"failure":"variation_not_found","fragment_path":"/content/dam/example/fragments/offer"}"#),
        ("delete_content_fragment", r#"{"failure":"fragment_is_referenced","fragment_path":"/content/dam/example/fragments/offer"}"#),
        ("create_experience_fragment", r#"{"failure":"parent_not_found","target_path":"/content/experience-fragments/example/hero"}"#),
        ("update_experience_fragment", r#"{"failure":"variation_not_found","variation_path":"/content/experience-fragments/example/hero/web"}"#),
        ("delete_experience_fragment", r#"{"failure":"mutation_outcome_unknown","fragment_path":"/content/experience-fragments/example/hero"}"#),
        ("create_asset", r#"{"failure":"parent_not_found","target_path":"/content/dam/example/logo.png"}"#),
        ("create_asset", r#"{"failure":"mutation_outcome_unknown","target_path":"/content/dam/example/logo.png"}"#),
        ("create_asset_folder", r#"{"failure":"parent_not_found","target_path":"/content/dam/example"}"#),
        ("move_asset", r#"{"destination_path":"/content/dam/archive/logo.png","failure":"source_not_found","source_path":"/content/dam/example/logo.png"}"#),
        ("delete_asset", r#"{"asset_path":"/content/dam/example/logo.png","failure":"asset_is_referenced"}"#),
        ("update_asset_metadata", r#"{"asset_path":"/content/dam/example/logo.png","failure":"asset_not_found"}"#),
        ("update_page", r#"{"failure":"page_not_found","page_path":"/content/example"}"#),
        ("update_page", r#"{"failure":"mutation_outcome_unknown","page_path":"/content/example"}"#),
        ("move_page", r#"{"destination_path":"/content/archive/example","failure":"source_not_found","source_path":"/content/example"}"#),
        ("delete_page", r#"{"failure":"target_is_referenced","page_path":"/content/example"}"#),
        ("update_component", r#"{"component_path":"/content/example/jcr:content/text","failure":"property_rejected"}"#),
        ("delete_component", r#"{"component_path":"/content/example/jcr:content/text","failure":"component_not_found"}"#),
        ("delete_component", r#"{"component_path":"/content/example/jcr:content/text","failure":"mutation_outcome_unknown"}"#),
        ("reorder_component", r#"{"component_path":"/content/example/jcr:content/text","failure":"sibling_not_found"}"#),
        ("replicate_content", r#"{"failure":"source_not_found","source_path":"/content/example"}"#),
        ("replicate_content", r#"{"accepted_item_count":0,"current_path":"/content/example","failure":"admission_rejected","remaining_item_count":2}"#),
        ("replicate_content", r#"{"accepted_item_count":1,"current_path":"/content/example/child","failure":"admission_budget_exceeded","remaining_item_count":1}"#),
        ("replicate_content", r#"{"accepted_item_count":1,"current_path":"/content/example/child","failure":"admission_outcome_unknown","remaining_item_count":1}"#),
        ("replicate_content", r#"{"accepted_item_count":1,"current_path":"/other","failure":"admission_rejected","remaining_item_count":1}"#),
        ("query_paths", r#"{"failure":"root_not_found","root_path":"/content/example"}"#),
        ("find_pages_by_template", r#"{"failure":"root_access_denied","root_path":"/content/example"}"#),
        ("find_pages_containing_phrase", r#"{"budget":"candidate_nodes","failure":"discovery_budget_exceeded"}"#),
        ("find_pages_using_components", r#"{"failure":"root_not_found","root_path":"/content/example"}"#),
        ("find_assets_by_metadata", r#"{"failure":"root_access_denied","root_path":"/content/example"}"#),
        ("find_assets_referenced_by_page", r#"{"failure":"page_invalid","page_path":"/content/example"}"#),
        ("query_paths", r#"{"failure":"root_not_found","root_path":"/other"}"#),
        ("inspect_open_service_gateway_initiative_configuration", r#"{"failure":"configuration_lookup_failed"}"#),
        ("inspect_open_service_gateway_initiative_configuration", r#"{"failure":"configuration_lookup_failed","private_payload":"must-not-escape"}"#),
        ("load_content_as_json", r#"{"failure":"not_found","path":"/other"}"#),
        ("download_content_package", r#"{"failure":"artifact_publication_failed"}"#),
        ("download_content_package", r#"{"failure":"staging_cleanup_failed"}"#),
        ("download_content_package", r#"{"failure":"artifact_publication_outcome_unknown"}"#),
        ("create_page", r#"{"failure":"parent_not_found","target_path":"/content/example/new"}"#),
        ("create_page", r#"{"failure":"mutation_outcome_unknown","target_path":"/content/example/new"}"#),
        ("add_component", r#"{"failure":"parent_not_orderable","target_path":"/content/example/jcr:content/text"}"#),
        ("add_component", r#"{"failure":"mutation_outcome_unknown","target_path":"/content/example/jcr:content/text"}"#),
    ] {
        retained_artifact_completion_case(wire, false, false, false, Some(failure), false, false).await;
    }
}

async fn retained_artifact_completion_case(
    wire: &str,
    refuse_capacity: bool,
    retire: bool,
    failed_lookup: bool,
    failure: Option<&str>,
    http2: bool,
    asynchronous: bool,
) {
    async fn http2_request(socket: &mut tokio::net::TcpStream) -> Vec<u8> {
        use tokio::io::{AsyncReadExt,AsyncWriteExt};
        let mut preface = [0;39]; socket.read_exact(&mut preface).await.unwrap();
        assert_eq!(&preface[..24],b"PRI * HTTP/2.0\r\n\r\nSM\r\n\r\n");
        socket.write_all(&[0,0,0,4,0,0,0,0,0]).await.unwrap();
        let mut header = [0;9]; socket.read_exact(&mut header).await.unwrap(); socket.write_all(&header).await.unwrap();
        socket.read_exact(&mut header).await.unwrap(); assert_eq!((header[3],header[4]),(1,5));
        let length = usize::from(header[0]) << 16 | usize::from(header[1]) << 8 | usize::from(header[2]);
        assert!(length <= 16384); let mut head = vec![0;length]; socket.read_exact(&mut head).await.unwrap(); head
    }
    async fn http2_response(socket: &mut tokio::net::TcpStream, media: &str, body: &[u8]) {
        use tokio::io::{AsyncReadExt,AsyncWriteExt};
        let mut block = vec![0x88];
        for (name,value) in [("content-type",media.to_owned()),("content-length",body.len().to_string())] {
            block.extend_from_slice(&[0,name.len() as u8]); block.extend_from_slice(name.as_bytes()); block.push(value.len() as u8); block.extend_from_slice(value.as_bytes());
        }
        assert!(body.len() <= 16384);
        for (kind,flags,bytes) in [(1,4,block.as_slice()),(0,1,body)] {
            let length = (bytes.len() as u32).to_be_bytes(); socket.write_all(&[length[1],length[2],length[3],kind,flags,0,0,0,1]).await.unwrap(); socket.write_all(bytes).await.unwrap();
        }
        let mut close = Vec::new(); let _ = socket.read_to_end(&mut close).await;
    }
    use slingshot_agent_connection::{
        command_submission::{ExpectedArtifactManifest, ManifestKind, Submission},
        selected_author_transport::SelectedAuthorTransport,
        structured_job_result::{ArtifactEcho, TerminalResultDocument},
    };
    use slingshot_agent_protocol::{
        identity::WireOperationIdentity, wire_contract::ExpectedProvenance,
    };
    use slingshot_daemon::operation::{
        artifact_completion::complete_retained_snapshot_result,
        durable_author_submission::prepare_initial_submission,
    };
    use slingshot_domain::{
        agent_identity::AgentEventStoreGeneration,
        command_fingerprint::{CommandFingerprint, FingerprintInput},
        installation::InstallationIdentifier,
        operation::*,
        operation_executor::ExecutionIdentity,
        remote_job::*,
    };
    use slingshot_storage::{
        agent_job_repository::{AgentJobRepository, SuccessfulAgentSnapshot},
        artifact_store::{ArtifactIdentifier, ArtifactStore},
        database::{OperationDatabase, RequiredSettings},
        operation_repository::{AdmissionRequest, OperationRepository},
        persistent_capacity::PersistentCapacityAccount,
    };
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::time::{Duration, timeout};
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let async_provider = async_provider(&format!("http://{}", listener.local_addr().unwrap()));
    let provider = provider(&format!("http://{}", listener.local_addr().unwrap()));
    let transport = SelectedAuthorTransport::new(provider.snapshot().author_connection()).unwrap();
    let (authentication, _) =
        provider.authenticate(provider.snapshot().author().as_text(), 0, &NoTokenSource).unwrap();
    let identity = ExecutionIdentity {
        attempt: 1,
        operation_identifier: "package-one".to_owned(),
        author_target_identity_digest: provider.snapshot().target().to_string(),
        selected_environment_revision: provider.snapshot().revision().to_string(),
    };
    let loaded = wire == "load_content_as_json";
    let (slot, media, name, manifest, maximum) = if loaded {
        (
            "loaded_content_json",
            "application/json",
            "loaded-content.json",
            ManifestKind::Load,
            slingshot_domain::command::artifact::maximum_loaded_content_artifact_bytes(),
        )
    } else {
        (
            "content_package",
            "application/zip",
            "example.zip",
            ManifestKind::Package,
            slingshot_domain::command::artifact::maximum_package_output_bytes(),
        )
    };
    let provenance = ExpectedProvenance {
        command_contract: slingshot_domain::selected_command_contract_identity::SelectedCommandContractIdentity::installed(wire).unwrap(),
        canonical_json_contract_digest: slingshot_domain::command::schema::canonical_contract_digest(),
        transport_contract_digest: slingshot_domain::author_agent_transport_contract::AuthorAgentTransportContract::embedded_digest(),
    };
    let arguments = match wire {
        "load_content_as_json" => r#"{"depth":0,"path":"/content/example"}"#,
        "find_open_service_gateway_initiative_configurations" => "{}",
        "find_sling_jobs" => r#"{"states":["error"]}"#,
        "find_workflow_instances" => r#"{"states":["running"]}"#,
        "list_open_service_gateway_initiative_bundles" => "{}",
        "list_open_service_gateway_initiative_components" => "{}",
        "list_replication_agents" => "{}",
        "list_resource_mappings" => "{}",
        "list_sling_job_queues" => "{}",
        "list_workflow_models" => "{}",
        "read_content_fragment" => r#"{"fragment_path":"/content/dam/example/offer","variation_name":"web"}"#,
        "list_child_pages" => r#"{"root_path":"/content/example"}"#,
        "list_group_members" => r#"{"group_identifier":"authors","include_indirect":false}"#,
        "list_asset_renditions" => r#"{"asset_path":"/content/dam/example/logo.png"}"#,
        "inspect_replication_queue" => r#"{"agent_identifier":"publish"}"#,
        "inspect_sling_job" => r#"{"job_identifier":"2024/01/01/example-job-1"}"#,
        "inspect_workflow_instance" => r#"{"instance_identifier":"/var/workflow/instances/server0/2024-01-01/request-for-activation_1"}"#,
        "inspect_replication_agent" => r#"{"agent_identifier":"publish"}"#,
        "resolve_resource_path" => r#"{"include_trace":false,"request_address":"https://example.test/en/report.html"}"#,
        "map_resource_path" => r#"{"include_trace":false,"repository_path":"/content/example"}"#,
        "update_open_service_gateway_initiative_configuration" => r#"{"assignments":{"host":{"cardinality":"scalar","type":"string","value":"example.test"}},"persistent_identifier":"com.example.service.Configuration"}"#,
        "delete_open_service_gateway_initiative_configuration" => r#"{"persistent_identifier":"com.example.service.Configuration"}"#,
        "set_open_service_gateway_initiative_bundle_state" => r#"{"symbolic_name":"com.example.bundle","transition":"start"}"#,
        "cancel_sling_job" => r#"{"job_identifier":"2024/01/01/example-job-1"}"#,
        "start_workflow" => r#"{"model_identifier":"/var/workflow/models/request-for-activation/jcr:content/model","payload_path":"/content/example/en/report"}"#,
        "terminate_workflow_instance" => r#"{"instance_identifier":"/var/workflow/instances/server0/2024-01-01/request-for-activation_1"}"#,
        "set_workflow_instance_suspension" => r#"{"instance_identifier":"/var/workflow/instances/server0/2024-01-01/request-for-activation_1","requested_state":"suspended"}"#,
        "flush_replication_queue" => r#"{"agent_identifier":"publish","expected_entry_count":1}"#,
        "retry_replication_queue_entry" => r#"{"agent_identifier":"publish","entry_identifier":"queue-entry-1"}"#,
        "create_user" => r#"{"authorizable_identifier":"author"}"#,
        "create_group" => r#"{"authorizable_identifier":"author"}"#,
        "delete_authorizable" => r#"{"authorizable_identifier":"author","expected_kind":"group"}"#,
        "update_user_profile" => r#"{"authorizable_identifier":"author","removed_property_names":["givenName"]}"#,
        "set_user_disabled" => r#"{"authorizable_identifier":"author","disabled":true}"#,
        "add_group_member" => r#"{"group_identifier":"content-authors","member_identifier":"author"}"#,
        "remove_group_member" => r#"{"group_identifier":"content-authors","member_identifier":"author"}"#,
        "create_content_fragment" => r#"{"model_path":"/conf/example/settings/dam/cfm/models/offer","name":"offer","parent_path":"/content/dam/example/fragments"}"#,
        "update_content_fragment" => r#"{"elements":{"title":"Spring offer"},"fragment_path":"/content/dam/example/fragments/offer","variation_name":"web"}"#,
        "delete_content_fragment" => r#"{"fragment_path":"/content/dam/example/fragments/offer","reference_policy":"refuse_when_referenced"}"#,
        "create_experience_fragment" => r#"{"name":"hero","parent_path":"/content/experience-fragments/example","template_path":"/conf/example/settings/wcm/templates/experience-fragment","variation_name":"web"}"#,
        "update_experience_fragment" => r#"{"title":"Hero","variation_path":"/content/experience-fragments/example/hero/web"}"#,
        "delete_experience_fragment" => r#"{"fragment_path":"/content/experience-fragments/example/hero","reference_policy":"refuse_when_referenced"}"#,
        "create_asset" => r#"{"name":"logo.png","parent_path":"/content/dam/example","payload":{"encoded_content":"aGVsbG8=","media_type":"image/png"}}"#,
        "create_asset_folder" => r#"{"name":"example","parent_path":"/content/dam"}"#,
        "move_asset" => r#"{"adjust_references":true,"destination_path":"/content/dam/archive/logo.png","source_path":"/content/dam/example/logo.png"}"#,
        "delete_asset" => r#"{"asset_path":"/content/dam/example/logo.png","reference_policy":"refuse_when_referenced"}"#,
        "update_asset_metadata" => r#"{"asset_path":"/content/dam/example/logo.png","removed_property_names":["dc:title"]}"#,
        "replicate_content" => r#"{"path":"/content/example","recursive":true}"#,
        "update_page" => r#"{"page_path":"/content/example","title":"Example"}"#,
        "move_page" => r#"{"adjust_references":true,"destination_path":"/content/archive/example","source_path":"/content/example"}"#,
        "delete_page" => r#"{"page_path":"/content/example","reference_policy":"refuse_when_referenced"}"#,
        "update_component" => r#"{"component_path":"/content/example/jcr:content/text","removed_property_names":["text"]}"#,
        "delete_component" => r#"{"component_path":"/content/example/jcr:content/text"}"#,
        "reorder_component" => r#"{"component_path":"/content/example/jcr:content/text","placement":{"mode":"before","sibling_name":"image"}}"#,
        "create_page" => r#"{"page_name":"new","parent_path":"/content/example","template_path":"/conf/example/templates/page","title":"New"}"#,
        "add_component" => r#"{"component_name":"text","content_parent":"content_root","page_path":"/content/example","resource_type":"example/components/text"}"#,
        "inspect_open_service_gateway_initiative_configuration" => r#"{"persistent_identifier":"example.service"}"#,
        "query_paths" | "find_assets_by_metadata" => r#"{"root_path":"/content/example"}"#,
        "find_pages_by_template" => r#"{"root_path":"/content/example","template_path":"/conf/example/template"}"#,
        "find_pages_containing_phrase" => r#"{"phrase":"annual report","root_path":"/content/example"}"#,
        "find_pages_using_components" => r#"{"match_mode":"any","resource_types":["example/components/text"],"root_path":"/content/example"}"#,
        "find_assets_referenced_by_page" => r#"{"page_path":"/content/example"}"#,
        _ => r#"{"package_name":"example","roots":["/content/example"]}"#,
    };
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
        if matches!(wire, "load_content_as_json" | "download_content_package") { ExpectedArtifactManifest::declaring(manifest, 1, maximum).unwrap() }
        else { ExpectedArtifactManifest::empty() },
    )
    .unwrap();
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("package.sqlite3");
    let settings = || RequiredSettings {
        page_bytes: 4096,
        database_pages: 262144,
        busy_timeout_milliseconds: 5000,
    };
    let mut operations =
        OperationRepository::new(OperationDatabase::open(&path, settings()).unwrap());
    let installation = InstallationIdentifier::parse(&"a1".repeat(32)).unwrap();
    operations.admit(&AdmissionRequest {
        author_target_identity: "opaque-target".to_owned(), author_target_identity_digest: identity.author_target_identity_digest.clone(), caller_identity: None,
        canonical_command: arguments.to_owned(), command_fingerprint: CommandFingerprint::derive(&FingerprintInput {
            author_target_identity_digest: identity.author_target_identity_digest.clone(), selected_environment_revision: identity.selected_environment_revision.clone(), canonical_command: arguments.to_owned(), command_wire_name: wire.to_owned(), command_semantic_contract_version: "1.0.0".to_owned(),
        }).unwrap(), command_wire_name: wire.to_owned(), daemon_runtime_contract_digest: slingshot_domain::daemon_runtime_contract::DaemonRuntimeContract::embedded_digest().as_text().to_owned(), installation_identifier: installation.clone(), operation_identifier: identity.operation_identifier.clone(), selected_environment_revision: identity.selected_environment_revision.clone(), workflow_correlation_identifier: None,
    }, 1000).unwrap();
    let mut remote =
        AgentJobRepository::new(OperationDatabase::open_live(&path, settings()).unwrap());
    drop(prepare_initial_submission(&remote, &identity, &submission, 1000).unwrap());
    let mut retained = remote
        .read(
            &identity.author_target_identity_digest,
            &submission.operation.agent_operation_identifier,
        )
        .unwrap()
        .unwrap();
    if let Some(failure) = failure {
        let invalid = failure.contains("/other") || failure.contains("private_payload");
        let unknown = failure.contains("outcome_unknown");
        let partial = wire == "replicate_content" && failure.contains("\"accepted_item_count\":1") && !unknown && !invalid;
        let document = slingshot_agent_protocol::terminal_failure::TerminalFailureDocument {
            operation: submission.operation.clone(),
            daemon_subscription_identifier: submission.daemon_subscription_identifier.clone(),
            canonical_failure: failure.to_owned(), provenance: submission.provenance.clone(),
            submitted_command_digest: submission.submitted_command_digest.clone(),
        };
        let capabilities = serde_json::json!({
            "format":"slingshot.agent/1", "agent_event_store_generation":7,
            "canonical_json_contract_digest":provenance.canonical_json_contract_digest,
            "transport_contract_digest":provenance.transport_contract_digest,
            "command_contracts":[slingshot_agent_protocol::identity::WireContractIdentity::from(&provenance.command_contract)],
            "continuation_authority_ready":true,
        });
        let snapshot = serde_json::json!({
            "provenance":submission.provenance,"agent_event_store_generation":7,
            "agent_operation_identifier":submission.operation.agent_operation_identifier,
            "author_target_identity_digest":identity.author_target_identity_digest,
            "selected_environment_revision":identity.selected_environment_revision,
            "daemon_subscription_identifier":"subscription-one",
            "submitted_command_digest":submission.submitted_command_digest,
            "subscription_watermark":"cursor-010", "physical_sling_job_identifiers":["job-one"],"granted_retention_milliseconds":120000,
            "attempt":1,"progress":0,"sequence":2,"kind":"failed","terminal_failure":document,
        });
        let peer = async {
            for (route, payload) in [
                ("/bin/slingshot-agent/capabilities".to_owned(), capabilities),
                (format!("/bin/slingshot-agent/operations/lookup?agent_operation_identifier={}", submission.operation.agent_operation_identifier), snapshot),
            ] {
                let (mut socket, _) = listener.accept().await.unwrap();
                let mut head = Vec::new();
                while !head.ends_with(b"\r\n\r\n") { head.push(socket.read_u8().await.unwrap()); }
                assert!(String::from_utf8(head).unwrap().starts_with(&format!("GET {route} HTTP/1.1\r\n")));
                let body = payload.to_string();
                socket.write_all(format!("HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",body.len()).as_bytes()).await.unwrap();
                socket.shutdown().await.unwrap();
            }
        };
        use slingshot_daemon::author_agent_operation_executor::{AuthorAgentOperationExecutor, ProductAuthorPorts};
        use slingshot_domain::operation_executor::{OperationExecutor, OperationExecutorOutcome, ProgressPort};
        struct Progress;
        impl ProgressPort for Progress { fn report(&self, _: &str) {} }
        let capacity = PersistentCapacityAccount::new(operations.database(), slingshot_domain::persistent_capacity::PersistentCapacityPolicy::embedded());
        let store = ArtifactStore::open(&root.path().join("failure-artifacts")).unwrap();
        let clock = AuthenticationClock(std::cell::Cell::new(0));
        let protocol = slingshot_daemon::retained_author_protocol::RetainedAuthorProtocol::new_with_authentication(
            &operations, &remote, &store, &capacity,
            slingshot_daemon::operation::author_authentication::AuthorAuthentication::Provider {provider:&provider,source:&NoTokenSource,clock:&clock},
            identity.clone(), submission.clone(), 2000,
        ).unwrap_or_else(|error| panic!("{wire}: {error:?}"));
        let ports = ProductAuthorPorts::new(provider.snapshot().author_connection(), &protocol).unwrap();
        let mut arguments: serde_json::Value = serde_json::from_str(&submission.canonical_arguments).unwrap();
        arguments.as_object_mut().unwrap().insert("command".to_owned(), wire.into());
        let command = serde_json::from_value(arguments).unwrap();
        let executor = AuthorAgentOperationExecutor::over(&ports);
        let (answer, ()) = timeout(Duration::from_secs(10), async {
            tokio::join!(executor.execute(&identity, &command, &Progress), peer)
        }).await.unwrap();
        assert_eq!(matches!(answer, OperationExecutorOutcome::RecoveryRequired { .. }), invalid || unknown);
        let reopened = OperationRepository::new(OperationDatabase::open_live(&path, settings()).unwrap());
        let local = reopened.read(&identity.author_target_identity_digest, &identity.operation_identifier).unwrap().unwrap();
        let child = remote.read(&identity.author_target_identity_digest, &submission.operation.agent_operation_identifier).unwrap().unwrap();
        if invalid || unknown {
            assert!(local.record.terminal_failure.is_none());
            let recovery = local.record.outstanding_recovery.unwrap();
            assert_eq!(recovery.attempt_count, 1);
            assert_eq!(recovery.evidence, RecoveryExecutionEvidence::ExecutionCertainty { certainty: OperationExecutionCertainty::RemoteOutcomeUnknown });
            assert!(child.terminal_disposition.is_none());
            assert_eq!(child, retained);
        } else {
            let failure = local.record.terminal_failure.unwrap();
            assert_eq!(failure.kind, if partial { TerminalFailureKind::RemoteFailed } else { TerminalFailureKind::Rejected });
            let needs_maintenance = document.canonical_failure.contains("staging_cleanup_failed");
            assert_eq!(failure.metadata.as_deref(), needs_maintenance.then_some(
                slingshot_storage::agent_job_repository::RejectedAgentDiagnosis::PackageStagingCleanupRequired.as_text()
            ));
            assert_eq!(failure.disposition, if partial { TerminalFailureDisposition::AuthoritativeRemoteFailure } else { TerminalFailureDisposition::AuthoritativeNonExecution { certainty: OperationExecutionCertainty::ConfirmedNotExecuted } });
            assert_eq!(child.observation.state, AgentJobState::Failed);
            assert_eq!(child.terminal_disposition.as_deref(), Some(if partial { "authoritative-remote-failure" } else { "authoritative-nonexecution" }));
            let repeated = executor.execute(&identity, &command, &Progress).await;
            assert!(matches!(repeated, OperationExecutorOutcome::TerminalFailure { .. }));
        }
        assert!(timeout(Duration::from_millis(20), listener.accept()).await.is_err(), "failure handling sent replacement work");
        return;
    }
    // Persisted success is injected in this coordinator test; the existing
    // lookup socket fixture separately proves acquisition of that evidence.
    operations
        .apply(
            &identity.author_target_identity_digest,
            &identity.operation_identifier,
            1,
            &OperationFact::Recovery {
                recovery: RecoveryFact {
                    category: RecoveryCategory::ResultAcquisition,
                    evidence: RecoveryExecutionEvidence::AuthoritativeRemoteSuccess,
                    attempt_count: 0,
                    detail: "acquiring result".to_owned(),
                    manual_resume_eligible: false,
                    retry_delay_milliseconds: 0,
                    retry_observed_at_unix_milliseconds: 2000,
                },
            },
            2000,
        )
        .unwrap();
    let valid_bytes = if loaded {
        slingshot_domain::command::canonical_json::write_canonical(&serde_json::json!({
            "children":[],"children_truncated":false,"path":"/content/example",
            "properties":{"p":{"cardinality":"multiple","property_type":"string","values":vec!["x".repeat(32768);9]}}
        })).unwrap()
    } else {
        "abc".to_owned()
    };
    if loaded {
        assert!(valid_bytes.len() > 262144);
    }
    let invalid_bytes = if loaded { valid_bytes.replacen('x', "y", 1) } else { "abd".to_owned() };
    let digest = hex::encode(Sha256::digest(valid_bytes.as_bytes()));
    let artifact = ArtifactIdentifier::derive(
        &installation,
        &identity.author_target_identity_digest,
        &identity.operation_identifier,
        slot,
    );
    let mut logical = serde_json::json!({"artifact":{"identifier":artifact.as_text(),"slot":slot,"media_type":media,"byte_length":valid_bytes.len(),"digest":digest,"suggested_file_name":name}});
    if loaded {
        logical["disposition"] = "artifact".into();
        logical["path"] = "/content/example".into();
    }
    let canonical_result =
        slingshot_domain::command::canonical_json::write_canonical(&logical).unwrap();
    let document = TerminalResultDocument {
        operation: submission.operation.clone(),
        daemon_subscription_identifier: submission.daemon_subscription_identifier.clone(),
        canonical_result: canonical_result.clone(),
        declared_artifacts: vec![ArtifactEcho {
            byte_length: valid_bytes.len() as u64,
            media_type: media.to_owned(),
            slot: slot.to_owned(),
            suggested_name: name.to_owned(),
        }],
        provenance: provenance.provenance(),
        submitted_command_digest: submission.submitted_command_digest.clone(),
    };
    let body = serde_json::to_vec(&document).unwrap();
    let snapshot = SuccessfulAgentSnapshot {
        observation: RemoteJobObservation {
            state: AgentJobState::Succeeded,
            applied_sequence: JobEventSequence::of(2),
            attempt: 1,
            progress: 100,
        },
        physical_sling_job_identifiers: vec!["job-one".to_owned()],
        remaining_retention_milliseconds: 120000,
    };
    let database = OperationDatabase::open_live(&path, settings()).unwrap();
    let capacity = PersistentCapacityAccount::new(
        &database,
        slingshot_domain::persistent_capacity::PersistentCapacityPolicy::embedded(),
    );
    let store_root = root.path().join("artifacts");
    let store = ArtifactStore::open(&store_root).unwrap();
    if failed_lookup {
        use slingshot_daemon::author_agent_operation_executor::{
            AgentSettlement, AuthorAgentProtocol,
        };
        use slingshot_daemon::operation::recovery_and_event_supervisor::{
            automatic_attempt_cap, jitter_ceiling_milliseconds,
        };
        let mut now = 3000;
        let competing =
            OperationRepository::new(OperationDatabase::open_live(&path, settings()).unwrap());
        for attempt in 0..=automatic_attempt_cap() {
            let clock = AuthenticationClock(std::cell::Cell::new(0));
            let protocol = slingshot_daemon::retained_author_protocol::RetainedAuthorProtocol::new_with_authentication(
                &operations,
                &remote,
                &store,
                &capacity,
                slingshot_daemon::operation::author_authentication::AuthorAuthentication::Provider {provider:&provider,source:&NoTokenSource,clock:&clock},
                identity.clone(),
                submission.clone(),
                now,
            )
            .unwrap();
            let peer = async {
                let (mut socket, _) = listener.accept().await.unwrap();
                let mut head = Vec::new();
                while !head.ends_with(b"\r\n\r\n") {
                    head.push(socket.read_u8().await.unwrap());
                }
                assert!(head.starts_with(b"GET /bin/slingshot-agent/capabilities HTTP/1.1\r\n"));
                if attempt == 0 {
                    competing
                        .apply(
                            &identity.author_target_identity_digest,
                            &identity.operation_identifier,
                            2,
                            &OperationFact::Progress {
                                detail: "newer owner observation".to_owned(),
                            },
                            now,
                        )
                        .unwrap();
                }
                socket.write_all(b"HTTP/1.1 503 Unavailable\r\nContent-Type: application/json\r\nContent-Length: 2\r\n\r\n{}").await.unwrap();
            };
            let (settlement, ()) = timeout(Duration::from_secs(5), async {
                tokio::join!(protocol.settle(&transport, &identity), peer)
            })
            .await
            .unwrap();
            let AgentSettlement::Outstanding { recovery } = settlement else {
                panic!("failed lookup remains recoverable")
            };
            assert_eq!(u64::from(recovery.attempt_count), attempt);
            assert_eq!(recovery.evidence, RecoveryExecutionEvidence::AuthoritativeRemoteSuccess);
            assert_eq!(recovery.category, RecoveryCategory::ResultAcquisition);
            assert_eq!(recovery.manual_resume_eligible, attempt == automatic_attempt_cap());
            if attempt == 0 {
                assert_eq!(
                    recovery.retry_observed_at_unix_milliseconds, 2000,
                    "failed exchange overwrote a newer local revision"
                );
                assert_eq!(recovery.detail, "acquiring result");
            } else {
                assert!(recovery.retry_observed_at_unix_milliseconds >= now);
            }
            assert!(recovery.retry_delay_milliseconds <= jitter_ceiling_milliseconds(attempt));
            let saved = operations
                .read(&identity.author_target_identity_digest, &identity.operation_identifier)
                .unwrap()
                .unwrap();
            assert_eq!(saved.record.revision, 3 + attempt);
            assert_eq!(saved.record.outstanding_recovery.as_ref(), Some(&recovery));
            now = now
                .max(
                    recovery
                        .retry_observed_at_unix_milliseconds
                        .saturating_add(recovery.retry_delay_milliseconds)
                        .saturating_add(1),
                )
                .saturating_add(1);
            if recovery.manual_resume_eligible {
                assert_eq!(
                    protocol.settle(&transport, &identity).await,
                    AgentSettlement::Outstanding { recovery }
                );
                assert!(timeout(Duration::from_millis(10), listener.accept()).await.is_err());
            }
            operations =
                OperationRepository::new(OperationDatabase::open_live(&path, settings()).unwrap());
        }
        return;
    }
    {
        use slingshot_daemon::retained_author_protocol::RetainedAuthorProtocol;
        let unrelated = OperationDatabase::open_in_memory(settings()).unwrap();
        let unrelated_remote =
            AgentJobRepository::new(OperationDatabase::open_in_memory(settings()).unwrap());
        let unrelated_capacity = PersistentCapacityAccount::new(
            &unrelated,
            slingshot_domain::persistent_capacity::PersistentCapacityPolicy::embedded(),
        );
        assert!(
            RetainedAuthorProtocol::new(
                &operations,
                &unrelated_remote,
                &store,
                &capacity,
                &authentication,
                identity.clone(),
                submission.clone(),
                2001
            )
            .is_err()
        );
        assert!(
            RetainedAuthorProtocol::new(
                &operations,
                &remote,
                &store,
                &unrelated_capacity,
                &authentication,
                identity.clone(),
                submission.clone(),
                2001
            )
            .is_err()
        );
        assert!(timeout(Duration::from_millis(10), listener.accept()).await.is_err());
    }
    assert!(
        complete_retained_snapshot_result(
            &operations,
            &retained,
            1,
            &identity,
            &submission,
            &body,
            2001,
            &snapshot,
            &store,
            &capacity,
            &transport,
            &authentication
        )
        .await
        .is_err()
    );
    assert!(timeout(Duration::from_millis(10), listener.accept()).await.is_err());
    if refuse_capacity {
        let mut policy =
            slingshot_domain::persistent_capacity::PersistentCapacityPolicy::embedded();
        policy.individual_artifact_bytes = 0;
        let unavailable = PersistentCapacityAccount::new(&database, policy);
        assert!(
            complete_retained_snapshot_result(
                &operations,
                &retained,
                2,
                &identity,
                &submission,
                &body,
                2001,
                &snapshot,
                &store,
                &unavailable,
                &transport,
                &authentication
            )
            .await
            .unwrap()
            .is_none()
        );
        let paused = operations
            .read(&identity.author_target_identity_digest, &identity.operation_identifier)
            .unwrap()
            .unwrap();
        assert_eq!(paused.record.revision, 3);
        let recovery = paused.record.outstanding_recovery.unwrap();
        assert_eq!(recovery.category, RecoveryCategory::PersistentCapacityUnavailable);
        assert_eq!(recovery.evidence, RecoveryExecutionEvidence::AuthoritativeRemoteSuccess);
        assert!(recovery.manual_resume_eligible);
        assert_eq!(capacity.pending_publications().unwrap(), 0);
        assert_eq!(capacity.usage().unwrap().reserved_artifact_bytes, 0);
        assert_eq!(capacity.usage().unwrap().committed_artifact_bytes, 0);
        assert_eq!(std::fs::read_dir(store_root.join("content")).unwrap().count(), 0);
        // Restored disk capacity does not silently bypass the durable pause.
        assert!(
            complete_retained_snapshot_result(
                &operations,
                &retained,
                3,
                &identity,
                &submission,
                &body,
                2002,
                &snapshot,
                &store,
                &capacity,
                &transport,
                &authentication
            )
            .await
            .is_err()
        );
        assert!(timeout(Duration::from_millis(10), listener.accept()).await.is_err());
        use slingshot_daemon::author_agent_operation_executor::{
            AgentSettlement, ArtifactCompletion, AuthorAgentProtocol,
        };
        let protocol = slingshot_daemon::retained_author_protocol::RetainedAuthorProtocol::new(
            &operations,
            &remote,
            &store,
            &capacity,
            &authentication,
            identity.clone(),
            submission.clone(),
            2002,
        )
        .unwrap();
        let saved = operations
            .read(&identity.author_target_identity_digest, &identity.operation_identifier)
            .unwrap()
            .unwrap()
            .record
            .outstanding_recovery
            .unwrap();
        assert_eq!(
            protocol.settle(&transport, &identity).await,
            AgentSettlement::Outstanding { recovery: saved.clone() }
        );
        assert_eq!(
            protocol.complete_artifacts(&transport, &identity).await,
            ArtifactCompletion::Recovery { recovery: saved }
        );
        assert!(
            timeout(Duration::from_millis(10), listener.accept()).await.is_err(),
            "concrete protocol bypassed capacity pause"
        );
        return;
    }
    if retire {
        let mut revision = 2;
        let mut anchor = None;
        for (status, reason, wrong_identity, terminal) in [
            (410, "retention_expired", true, false),
            (404, "missing", false, false),
            (
                if loaded { 404 } else { 410 },
                if loaded { "missing" } else { "retention_expired" },
                false,
                true,
            ),
        ] {
            let now = if terminal && loaded {
                anchor.unwrap()
                    + slingshot_agent_connection::artifact_download::missing_grace_milliseconds()
            } else {
                2001
            };
            let unavailable = serde_json::json!({
                "provenance":submission.provenance,"agent_event_store_generation":7,
                "agent_operation_identifier":submission.operation.agent_operation_identifier,
                "artifact_identifier":if wrong_identity { "4".repeat(64) } else { artifact.as_text().to_owned() },
                "artifact_slot":slot,"reason":reason,
            }).to_string();
            let peer = async {
                let (mut socket, _) = listener.accept().await.unwrap();
                let mut head = Vec::new();
                while !head.ends_with(b"\r\n\r\n") {
                    head.push(socket.read_u8().await.unwrap());
                }
                assert!(
                    String::from_utf8(head)
                        .unwrap()
                        .starts_with("GET /bin/slingshot-agent/operations/")
                );
                socket.write_all(format!("HTTP/1.1 {status} Error\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n{unavailable}", unavailable.len()).as_bytes()).await.unwrap();
            };
            let (result, ()) = timeout(Duration::from_secs(5), async {
                tokio::join!(
                    complete_retained_snapshot_result(
                        &operations,
                        &retained,
                        revision,
                        &identity,
                        &submission,
                        &body,
                        now,
                        &snapshot,
                        &store,
                        &capacity,
                        &transport,
                        &authentication
                    ),
                    peer
                )
            })
            .await
            .unwrap();
            assert_eq!(result.is_ok(), !wrong_identity);
            if !wrong_identity {
                revision += 1;
            }
            let current = operations
                .read(&identity.author_target_identity_digest, &identity.operation_identifier)
                .unwrap()
                .unwrap();
            assert_eq!(current.record.revision, revision);
            if terminal {
                let failure = current.record.terminal_failure.unwrap();
                assert_eq!(failure.kind, TerminalFailureKind::ResultUnavailable);
                assert_eq!(
                    failure.disposition,
                    TerminalFailureDisposition::AuthoritativeRemoteSuccess
                );
            } else {
                assert_eq!(
                    current.record.outstanding_recovery.unwrap().evidence,
                    RecoveryExecutionEvidence::AuthoritativeRemoteSuccess
                );
                let observed = operations
                    .begin_artifact_acquisition(
                        &retained,
                        revision,
                        artifact.as_text(),
                        slot,
                        &digest,
                        999999,
                    )
                    .unwrap();
                if let Some(anchor) = anchor {
                    assert_eq!(observed, anchor, "retry refreshed grace");
                }
                anchor = Some(observed);
                assert!(
                    operations
                        .begin_artifact_acquisition(
                            &retained,
                            revision - 1,
                            artifact.as_text(),
                            slot,
                            &digest,
                            999999
                        )
                        .is_err(),
                    "stale local revision cannot authorize acquisition"
                );
                assert!(
                    operations
                        .begin_artifact_acquisition(
                            &retained,
                            revision,
                            &"5".repeat(64),
                            slot,
                            &digest,
                            999999
                        )
                        .is_err()
                );
                operations =
                    OperationRepository::new(OperationDatabase::open(&path, settings()).unwrap());
                remote = AgentJobRepository::new(
                    OperationDatabase::open_live(&path, settings()).unwrap(),
                );
                retained = remote
                    .read(
                        &identity.author_target_identity_digest,
                        &submission.operation.agent_operation_identifier,
                    )
                    .unwrap()
                    .unwrap();
            }
            assert_eq!(capacity.pending_publications().unwrap(), 0);
            assert_eq!(capacity.usage().unwrap().reserved_artifact_bytes, 0);
            assert_eq!(capacity.usage().unwrap().committed_artifact_bytes, 0);
            assert_eq!(std::fs::read_dir(store_root.join("content")).unwrap().count(), 0);
        }
        assert!(
            complete_retained_snapshot_result(
                &operations,
                &retained,
                revision,
                &identity,
                &submission,
                &body,
                2002,
                &snapshot,
                &store,
                &capacity,
                &transport,
                &authentication
            )
            .await
            .is_err()
        );
        assert!(timeout(Duration::from_millis(10), listener.accept()).await.is_err());
        return;
    }
    for (bytes, accepted, race, revision) in [
        (invalid_bytes.as_str(), false, false, 2),
        (valid_bytes.as_str(), false, true, 2),
        (valid_bytes.as_str(), true, false, 3),
    ] {
        let peer = async {
            if accepted {
                let capabilities = serde_json::json!({
                    "format":"slingshot.agent/1", "agent_event_store_generation":7,
                    "canonical_json_contract_digest":provenance.canonical_json_contract_digest,
                    "transport_contract_digest":provenance.transport_contract_digest,
                    "command_contracts":[slingshot_agent_protocol::identity::WireContractIdentity::from(&provenance.command_contract)],
                    "continuation_authority_ready":true,
                }).to_string();
                let found = serde_json::json!({
                    "provenance":submission.provenance, "agent_event_store_generation":7,
                    "agent_operation_identifier":submission.operation.agent_operation_identifier,
                    "author_target_identity_digest":identity.author_target_identity_digest,
                    "selected_environment_revision":identity.selected_environment_revision,
                    "daemon_subscription_identifier":"subscription-one",
                    "submitted_command_digest":submission.submitted_command_digest,
                    "subscription_watermark":"cursor-010", "physical_sling_job_identifiers":["job-one"], "granted_retention_milliseconds":120000,
                    "attempt":1,"progress":100,"sequence":2,"kind":"succeeded","terminal_result":document,
                }).to_string();
                for (route, payload) in [
                    ("/bin/slingshot-agent/capabilities".to_owned(), capabilities),
                    (
                        format!(
                            "/bin/slingshot-agent/operations/lookup?agent_operation_identifier={}",
                            submission.operation.agent_operation_identifier
                        ),
                        found,
                    ),
                ] {
                    let (mut socket, _) = listener.accept().await.unwrap();
                    if http2 {
                        let head = http2_request(&mut socket).await;
                        assert!(head.windows(route.len()).any(|bytes| bytes == route.as_bytes()));
                        authentication.lend_value_bytes(|value| assert!(head.windows(value.len()).any(|bytes| bytes == value)));
                        http2_response(&mut socket,"application/json",payload.as_bytes()).await;
                        continue;
                    }
                    let mut head = Vec::new();
                    while !head.ends_with(b"\r\n\r\n") {
                        head.push(socket.read_u8().await.unwrap());
                    }
                    assert!(
                        String::from_utf8(head)
                            .unwrap()
                            .starts_with(&format!("GET {route} HTTP/1.1\r\n"))
                    );
                    socket.write_all(format!("HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n{payload}", payload.len()).as_bytes()).await.unwrap();
                }
            }
            let (mut socket, _) = listener.accept().await.unwrap();
            if accepted && http2 {
                let head = http2_request(&mut socket).await;
                let route = format!("/bin/slingshot-agent/operations/{}/artifacts/{slot}",submission.operation.agent_operation_identifier);
                assert!(head.windows(route.len()).any(|bytes| bytes == route.as_bytes()));
                authentication.lend_value_bytes(|value| assert!(head.windows(value.len()).any(|bytes| bytes == value)));
                tokio::time::sleep(Duration::from_millis(30)).await;
                http2_response(&mut socket,media,bytes.as_bytes()).await;
                return;
            }
            let mut head = Vec::new();
            while !head.ends_with(b"\r\n\r\n") {
                head.push(socket.read_u8().await.unwrap());
            }
            assert!(String::from_utf8(head).unwrap().starts_with(&format!(
                "GET /bin/slingshot-agent/operations/{}/artifacts/{slot} HTTP/1.1",
                submission.operation.agent_operation_identifier
            )));
            if race {
                operations
                    .apply(
                        &identity.author_target_identity_digest,
                        &identity.operation_identifier,
                        revision,
                        &OperationFact::Progress { detail: "newer owner observation".to_owned() },
                        2001,
                    )
                    .unwrap();
            }
            if accepted {
                tokio::time::sleep(Duration::from_millis(30)).await;
            }
            socket.write_all(format!("HTTP/1.1 200 OK\r\nContent-Type: {media}\r\nContent-Length: {}\r\n\r\n{bytes}", bytes.len()).as_bytes()).await.unwrap();
        };
        let (result, ()) = timeout(Duration::from_secs(5), async {
            tokio::join!(
                async {
                    if accepted {
                        use slingshot_daemon::author_agent_operation_executor::{AuthorAgentOperationExecutor, ProductAuthorPorts};
                        use slingshot_domain::operation_executor::{OperationExecutor, OperationExecutorOutcome, ProgressPort};
                        struct Progress;
                        impl ProgressPort for Progress { fn report(&self, _: &str) {} }
                        let clock = AuthenticationClock(std::cell::Cell::new(0));
                        use slingshot_daemon::operation::author_authentication::AuthorAuthentication;
                        let policy = if asynchronous {AuthorAuthentication::AsyncProvider {provider:&async_provider,clock:&NoTokenClocks,utc:&NoTokenClocks}}
                            else if http2 {AuthorAuthentication::Fixed {authentication:&authentication,protocol:slingshot_daemon::operation::subscription_reset::ResetTransport::Http2}}
                            else {AuthorAuthentication::Provider {provider:&provider,source:&NoTokenSource,clock:&clock}};
                        let protocol = slingshot_daemon::retained_author_protocol::RetainedAuthorProtocol::new_with_authentication(
                            &operations, &remote, &store, &capacity, policy,
                            identity.clone(), submission.clone(), 2001,
                        ).unwrap();
                        assert_eq!(format!("{protocol:?}"), "RetainedAuthorProtocol([redacted])");
                        let ports = ProductAuthorPorts::new(provider.snapshot().author_connection(), &protocol).unwrap();
                        let mut arguments: serde_json::Value = serde_json::from_str(&submission.canonical_arguments).unwrap();
                        arguments.as_object_mut().unwrap().insert("command".to_owned(), wire.into());
                        let command = serde_json::from_value(arguments).unwrap();
                        let outcome = AuthorAgentOperationExecutor::over(&ports).execute(&identity, &command, &Progress).await;
                        if !http2 && !asynchronous {assert_eq!(clock.0.get(),3,"capability, lookup and artifact each acquire provider authentication");}
                        let OperationExecutorOutcome::Succeeded { artifacts, inline_result } = outcome else { panic!("verified local result publishes") };
                        assert_eq!(inline_result, Some(canonical_result.clone()));
                        assert_eq!(artifacts.len(), 1);
                        assert_eq!(artifacts[0].content_digest, digest);
                        assert_eq!(artifacts[0].artifact_slot, slot);
                        Ok(())
                    } else {
                        complete_retained_snapshot_result(
                    &operations,
                    &retained,
                    revision,
                    &identity,
                    &submission,
                    &body,
                    2001,
                    &snapshot,
                    &store,
                    &capacity,
                    &transport,
                    &authentication
                        ).await.map(|_| ())
                    }
                },
                peer
            )
        })
        .await
        .unwrap();
        assert_eq!(result.is_ok(), accepted);
        assert_eq!(capacity.pending_publications().unwrap(), u64::from(race));
        assert_eq!(capacity.usage().unwrap().reserved_artifact_bytes, 0);
        let current = operations
            .read(&identity.author_target_identity_digest, &identity.operation_identifier)
            .unwrap()
            .unwrap();
        if accepted {
            let remote_end = remote
                .read(
                    &identity.author_target_identity_digest,
                    &submission.operation.agent_operation_identifier,
                )
                .unwrap()
                .unwrap();
            assert!(
                remote_end.remaining_retention_milliseconds <= 119970,
                "artifact transfer time was not deducted"
            );
            assert_eq!(current.record.lifecycle_state, OperationLifecycleState::Succeeded);
            assert_eq!(current.record.revision, 4);
            assert_eq!(current.result_disposition, Some(ResultDisposition::Inline));
            assert_eq!(current.result_inline_bytes.as_deref(), Some(canonical_result.as_str()));
            assert_eq!(
                std::fs::read(store_root.join("content").join(&digest)).unwrap(),
                valid_bytes.as_bytes()
            );
            assert_eq!(
                remote
                    .read(
                        &identity.author_target_identity_digest,
                        &submission.operation.agent_operation_identifier
                    )
                    .unwrap()
                    .unwrap()
                    .observation
                    .state,
                AgentJobState::Succeeded
            );
            use slingshot_daemon::author_agent_operation_executor::{
                ArtifactCompletion, AuthorAgentProtocol,
            };
            let protocol = slingshot_daemon::retained_author_protocol::RetainedAuthorProtocol::new(
                &operations,
                &remote,
                &store,
                &capacity,
                &authentication,
                identity.clone(),
                submission.clone(),
                3000,
            )
            .unwrap();
            std::fs::write(store_root.join("content").join(&digest), invalid_bytes.as_bytes())
                .unwrap();
            let ArtifactCompletion::Recovery { recovery } =
                protocol.complete_artifacts(&transport, &identity).await
            else {
                panic!("changed content cannot be published")
            };
            assert_eq!(recovery.evidence, RecoveryExecutionEvidence::AuthoritativeRemoteSuccess);
            assert_eq!(recovery.category, RecoveryCategory::ResultAcquisition);
            assert!(timeout(Duration::from_millis(10), listener.accept()).await.is_err());
        } else {
            assert_eq!(current.record.revision, revision + u64::from(race));
            assert_eq!(
                current.record.outstanding_recovery.unwrap().evidence,
                RecoveryExecutionEvidence::AuthoritativeRemoteSuccess
            );
            assert_eq!(
                std::fs::read_dir(store_root.join("content")).unwrap().count(),
                usize::from(race)
            );
            assert_eq!(
                remote
                    .read(
                        &identity.author_target_identity_digest,
                        &submission.operation.agent_operation_identifier
                    )
                    .unwrap()
                    .unwrap()
                    .observation
                    .state,
                AgentJobState::Queued
            );
        }
        if race {
            operations =
                OperationRepository::new(OperationDatabase::open(&path, settings()).unwrap());
            remote =
                AgentJobRepository::new(OperationDatabase::open_live(&path, settings()).unwrap());
            retained = remote
                .read(
                    &identity.author_target_identity_digest,
                    &submission.operation.agent_operation_identifier,
                )
                .unwrap()
                .unwrap();
            assert_eq!(capacity.pending_publications().unwrap(), 1);
        }
    }
}

#[tokio::test]
async fn remote_staging_reserves_first_and_never_publishes_unproved_bytes() {
    use slingshot_agent_connection::{
        artifact_download::ExpectedArtifact,
        command_submission::{ExpectedArtifactManifest, Submission},
        selected_author_transport::SelectedAuthorTransport,
    };
    use slingshot_agent_protocol::{
        identity::WireOperationIdentity, wire_contract::ExpectedProvenance,
    };
    use slingshot_daemon::operation::artifact_completion::{
        RemoteStageRefusal, stage_remote_artifact_with_authentication as stage_remote_artifact,
    };
    use slingshot_domain::{
        agent_identity::AgentEventStoreGeneration, operation_executor::ExecutionIdentity,
        persistent_capacity::PersistentCapacityPolicy,
    };
    use slingshot_storage::{
        artifact_store::{ArtifactStore, InstallationRequest},
        database::{OperationDatabase, RequiredSettings},
        persistent_capacity::PersistentCapacityAccount,
    };
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::time::{Duration, timeout};
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let provider = provider(&format!("http://{}", listener.local_addr().unwrap()));
    let transport = SelectedAuthorTransport::new(provider.snapshot().author_connection()).unwrap();
    let clock = AuthenticationClock(std::cell::Cell::new(0));
    let authentication = slingshot_daemon::operation::author_authentication::AuthorAuthentication::Provider {
        provider: &provider, source: &NoTokenSource, clock: &clock,
    };
    assert_eq!(format!("{authentication:?}"), "AuthorAuthentication([redacted])");
    let identity = ExecutionIdentity {
        attempt: 1,
        operation_identifier: "local-stage".to_owned(),
        author_target_identity_digest: provider.snapshot().target().to_string(),
        selected_environment_revision: provider.snapshot().revision().to_string(),
    };
    // This exercises transfer/staging only. Terminal command-manifest binding
    // remains the caller's separate prerequisite, not evidence from this test.
    let provenance = ExpectedProvenance {
        command_contract: slingshot_domain::selected_command_contract_identity::SelectedCommandContractIdentity::installed("query_paths").unwrap(),
        canonical_json_contract_digest: slingshot_domain::command::schema::canonical_contract_digest(),
        transport_contract_digest: slingshot_domain::author_agent_transport_contract::AuthorAgentTransportContract::embedded_digest(),
    };
    let submission = Submission::build(
        &provenance,
        WireOperationIdentity::of(
            &identity.author_target_identity_digest,
            &identity.selected_environment_revision,
            &identity.operation_identifier,
            AgentEventStoreGeneration::of(7),
        ),
        "subscription-one",
        r#"{"root_path":"/content/example"}"#,
        ExpectedArtifactManifest::empty(),
    )
    .unwrap();
    let expected = ExpectedArtifact {
        artifact_digest: hex::encode(Sha256::digest(b"abc")),
        artifact_slot: "content_package".to_owned(),
        byte_length: 3,
        media_type: "application/zip".to_owned(),
    };
    let request = InstallationRequest {
        installation_identifier: slingshot_domain::installation::InstallationIdentifier::parse(
            &"a1".repeat(32),
        )
        .unwrap(),
        author_target_identity_digest: identity.author_target_identity_digest.clone(),
        operation_identifier: identity.operation_identifier.clone(),
        artifact_slot: expected.artifact_slot.clone(),
        media_type: expected.media_type.clone(),
        descriptor: None,
    };
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("stage.sqlite3");
    let settings = || RequiredSettings {
        page_bytes: 4096,
        database_pages: 262144,
        busy_timeout_milliseconds: 5000,
    };
    let database = OperationDatabase::open(&path, settings()).unwrap();
    let store_root = root.path().join("artifacts");
    let store = ArtifactStore::open(&store_root).unwrap();
    let mut policy = PersistentCapacityPolicy::embedded();
    policy.individual_artifact_bytes = 0;
    let refused = PersistentCapacityAccount::new(&database, policy);
    assert!(matches!(
        stage_remote_artifact(
            &transport,
            &identity,
            &submission,
            authentication,
            &expected,
            &request,
            &store,
            &refused,
            0
        )
        .await,
        Err(RemoteStageRefusal::Capacity)
    ));
    assert!(timeout(Duration::from_millis(10), listener.accept()).await.is_err());
    assert_eq!(std::fs::read_dir(store_root.join("content")).unwrap().count(), 0);
    let capacity = PersistentCapacityAccount::new(&database, PersistentCapacityPolicy::embedded());
    assert_eq!(clock.0.get(),0,"capacity refusal must precede provider authentication");
    let (fixed_authentication, _) = provider.authenticate(provider.snapshot().author().as_text(),0,&NoTokenSource).unwrap();
    for defect in ["", "short", "digest", "long", "media", "location", "capacity"] {
        use slingshot_daemon::operation::{artifact_completion::stage_remote_artifact_over, subscription_reset::ResetTransport};
        let isolated = tempfile::tempdir().unwrap();
        let database = OperationDatabase::open(&isolated.path().join("http2-stage.sqlite3"),settings()).unwrap();
        let capacity = PersistentCapacityAccount::new(&database,PersistentCapacityPolicy::embedded());
        let mut policy = PersistentCapacityPolicy::embedded(); policy.individual_artifact_bytes = 0;
        let refused = PersistentCapacityAccount::new(&database,policy);
        let store_root = isolated.path().join("artifacts"); let store = ArtifactStore::open(&store_root).unwrap();
        let transfer = stage_remote_artifact_over(&transport,&identity,&submission,&fixed_authentication,
            &expected,&request,&store,if defect == "capacity" {&refused} else {&capacity},0,ResetTransport::Http2);
        if defect == "capacity" {
            assert!(matches!(transfer.await,Err(RemoteStageRefusal::Capacity)));
            assert!(timeout(Duration::from_millis(10),listener.accept()).await.is_err());
            continue;
        }
        let peer = async {
            let (mut socket,_) = listener.accept().await.unwrap();
            let mut preface = [0;39]; socket.read_exact(&mut preface).await.unwrap();
            assert_eq!(&preface[..24],b"PRI * HTTP/2.0\r\n\r\nSM\r\n\r\n");
            socket.write_all(&[0,0,0,4,0,0,0,0,0]).await.unwrap();
            let mut header = [0;9]; socket.read_exact(&mut header).await.unwrap(); socket.write_all(&header).await.unwrap();
            socket.read_exact(&mut header).await.unwrap(); assert_eq!((header[3],header[4]),(1,5));
            let length = usize::from(header[0]) << 16 | usize::from(header[1]) << 8 | usize::from(header[2]);
            assert!(length <= 16384); let mut head = vec![0;length]; socket.read_exact(&mut head).await.unwrap();
            fixed_authentication.lend_value_bytes(|value| assert!(head.windows(value.len()).any(|bytes| bytes == value)));
            let route = format!("/bin/slingshot-agent/operations/{}/artifacts/{}",submission.operation.agent_operation_identifier,expected.artifact_slot);
            assert!(head.windows(route.len()).any(|bytes| bytes == route.as_bytes()));
            let mut block = vec![0x88];
            for (name,value) in [("content-type",if defect == "media" {"text/plain"} else {"application/zip"}),("content-length","3")] {
                block.extend_from_slice(&[0,name.len() as u8]); block.extend_from_slice(name.as_bytes()); block.push(value.len() as u8); block.extend_from_slice(value.as_bytes());
            }
            if defect == "location" { block.extend_from_slice(b"\x00\x08location\x0a/elsewhere"); }
            let body = match defect {"short" => "ab", "digest" => "abd", "long" => "abc!", _ => "abc"};
            for (kind,flags,bytes) in [(1,4,block.as_slice()),(0,1,body.as_bytes())] {
                let length = (bytes.len() as u32).to_be_bytes(); socket.write_all(&[length[1],length[2],length[3],kind,flags,0,0,0,1]).await.unwrap(); socket.write_all(bytes).await.unwrap();
            }
            let mut close = Vec::new(); let _ = socket.read_to_end(&mut close).await;
        };
        let (result,()) = timeout(Duration::from_secs(5),async {tokio::join!(transfer,peer)}).await.unwrap();
        assert_eq!(result.is_ok(),defect.is_empty(),"{defect}");
        drop(result);
        assert_eq!(capacity.pending_publications().unwrap(),u64::from(defect.is_empty()));
        assert_eq!(capacity.usage().unwrap().reserved_artifact_bytes,0);
        assert_eq!(capacity.usage().unwrap().committed_artifact_bytes,if defect.is_empty() {3} else {0});
        assert_eq!(std::fs::read_dir(store_root.join("content")).unwrap().count(),0);
    }
    for body in ["ab", "abd", "abc!"] {
        let peer = async {
            let (mut socket, _) = listener.accept().await.unwrap();
            let mut head = Vec::new();
            while !head.ends_with(b"\r\n\r\n") {
                head.push(socket.read_u8().await.unwrap());
            }
            socket.write_all(format!("HTTP/1.1 200 OK\r\nContent-Type: application/zip\r\nContent-Length: 3\r\n\r\n{body}").as_bytes()).await.unwrap();
        };
        let (result, ()) = timeout(Duration::from_secs(5), async {
            tokio::join!(
                stage_remote_artifact(
                    &transport,
                    &identity,
                    &submission,
                    authentication,
                    &expected,
                    &request,
                    &store,
                    &capacity,
                    0
                ),
                peer
            )
        })
        .await
        .unwrap();
        assert!(result.is_err());
        assert_eq!(capacity.pending_publications().unwrap(), 0);
        assert_eq!(capacity.usage().unwrap().reserved_artifact_bytes, 0);
        assert_eq!(capacity.usage().unwrap().committed_artifact_bytes, 0);
        assert_eq!(std::fs::read_dir(store_root.join("content")).unwrap().count(), 0);
        // A full reservation remains possible: the failed attempt leaked none.
        drop(capacity.reserve_artifact(Some(&expected.artifact_digest), 3).unwrap());
    }
    for (status, reason) in [(404, "missing"), (410, "retention_expired")] {
        let artifact_identifier = slingshot_storage::artifact_store::ArtifactIdentifier::derive(
            &request.installation_identifier,
            &request.author_target_identity_digest,
            &request.operation_identifier,
            &request.artifact_slot,
        );
        let body = serde_json::json!({
            "provenance":submission.provenance,"agent_event_store_generation":7,
            "agent_operation_identifier":submission.operation.agent_operation_identifier,
            "artifact_identifier":artifact_identifier.as_text(),"artifact_slot":request.artifact_slot,"reason":reason,
        }).to_string();
        let peer = async {
            let (mut socket, _) = listener.accept().await.unwrap();
            let mut head = Vec::new();
            while !head.ends_with(b"\r\n\r\n") {
                head.push(socket.read_u8().await.unwrap());
            }
            socket.write_all(format!("HTTP/1.1 {status} Error\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n{body}", body.len()).as_bytes()).await.unwrap();
        };
        let (result, ()) = timeout(Duration::from_secs(5), async {
            tokio::join!(
                stage_remote_artifact(
                    &transport,
                    &identity,
                    &submission,
                    authentication,
                    &expected,
                    &request,
                    &store,
                    &capacity,
                    0
                ),
                peer
            )
        })
        .await
        .unwrap();
        let Err(RemoteStageRefusal::Unavailable { evidence, .. }) = result else {
            panic!("verified unavailable evidence was lost")
        };
        assert_eq!(
            evidence.reason(),
            if status == 404 {
                slingshot_agent_protocol::artifact_unavailable::UnavailableReason::Missing
            } else {
                slingshot_agent_protocol::artifact_unavailable::UnavailableReason::RetentionExpired
            }
        );
        assert_eq!(capacity.usage().unwrap().reserved_artifact_bytes, 0);
        assert_eq!(capacity.usage().unwrap().committed_artifact_bytes, 0);
        assert_eq!(capacity.pending_publications().unwrap(), 0);
        assert_eq!(std::fs::read_dir(store_root.join("content")).unwrap().count(), 0);
    }
    let (sent, partial) = tokio::sync::oneshot::channel();
    let peer = async {
        let (mut socket, _) = listener.accept().await.unwrap();
        let mut head = Vec::new();
        while !head.ends_with(b"\r\n\r\n") {
            head.push(socket.read_u8().await.unwrap());
        }
        socket
            .write_all(
                b"HTTP/1.1 200 OK\r\nContent-Type: application/zip\r\nContent-Length: 3\r\n\r\nab",
            )
            .await
            .unwrap();
        sent.send(()).unwrap();
        assert!(socket.read_u8().await.is_err());
    };
    let client = async {
        let transfer = stage_remote_artifact(
            &transport,
            &identity,
            &submission,
            authentication,
            &expected,
            &request,
            &store,
            &capacity,
            0,
        );
        tokio::pin!(transfer);
        tokio::select! {
            _ = &mut transfer => panic!("partial staging completed"),
            _ = partial => {}
        }
        assert!(timeout(Duration::from_millis(20), &mut transfer).await.is_err());
    };
    timeout(Duration::from_secs(5), async {
        tokio::join!(client, peer);
    })
    .await
    .unwrap();
    assert_eq!(capacity.usage().unwrap().reserved_artifact_bytes, 0);
    assert_eq!(capacity.usage().unwrap().committed_artifact_bytes, 0);
    assert_eq!(capacity.pending_publications().unwrap(), 0);
    assert_eq!(std::fs::read_dir(store_root.join("content")).unwrap().count(), 0);
    let mut load_provenance = provenance.clone();
    load_provenance.command_contract = slingshot_domain::selected_command_contract_identity::SelectedCommandContractIdentity::installed("load_content_as_json").unwrap();
    let load_submission = Submission::build(
        &load_provenance,
        submission.operation.clone(),
        "subscription-one",
        r#"{"depth":1,"path":"/content/example"}"#,
        ExpectedArtifactManifest::declaring(
            slingshot_agent_connection::command_submission::ManifestKind::Load,
            1,
            16_777_216,
        )
        .unwrap(),
    )
    .unwrap();
    // Correct transport length/digest cannot make noncanonical loaded JSON
    // eligible for a durable publication hold.
    for body in [
        "{} ",
        r#"{"b":0,"a":1}"#,
        r#"{"a":0,"a":1}"#,
        "{}",
        r#"{"children":[],"children_truncated":false,"path":"/content/other","properties":{}}"#,
    ] {
        let loaded = ExpectedArtifact {
            artifact_slot: "loaded_content_json".to_owned(),
            media_type: "application/json".to_owned(),
            artifact_digest: hex::encode(Sha256::digest(body.as_bytes())),
            byte_length: body.len() as u64,
        };
        let mut loaded_request = request.clone();
        loaded_request.artifact_slot = loaded.artifact_slot.clone();
        loaded_request.media_type = loaded.media_type.clone();
        let peer = async {
            let (mut socket, _) = listener.accept().await.unwrap();
            let mut head = Vec::new();
            while !head.ends_with(b"\r\n\r\n") {
                head.push(socket.read_u8().await.unwrap());
            }
            socket.write_all(format!("HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n{body}", body.len()).as_bytes()).await.unwrap();
        };
        let (result, ()) = timeout(Duration::from_secs(5), async {
            tokio::join!(
                stage_remote_artifact(
                    &transport,
                    &identity,
                    &load_submission,
                    authentication,
                    &loaded,
                    &loaded_request,
                    &store,
                    &capacity,
                    0
                ),
                peer
            )
        })
        .await
        .unwrap();
        assert!(result.is_err());
        assert_eq!(capacity.usage().unwrap().reserved_artifact_bytes, 0);
        assert_eq!(capacity.usage().unwrap().committed_artifact_bytes, 0);
        assert_eq!(capacity.pending_publications().unwrap(), 0);
        assert_eq!(std::fs::read_dir(store_root.join("content")).unwrap().count(), 0);
    }
    {
        // Valid data must pass, too. This separate capacity namespace keeps
        // the subsequent package retry assertions independent.
        let loaded_database =
            OperationDatabase::open(&root.path().join("loaded.sqlite3"), settings()).unwrap();
        let loaded_capacity =
            PersistentCapacityAccount::new(&loaded_database, PersistentCapacityPolicy::embedded());
        let body = r#"{"children":[],"children_truncated":false,"path":"/content/example","properties":{"p":{"cardinality":"multiple","property_type":"string","values":["one","two"]}}}"#;
        let loaded = ExpectedArtifact {
            artifact_slot: "loaded_content_json".to_owned(),
            media_type: "application/json".to_owned(),
            artifact_digest: hex::encode(Sha256::digest(body.as_bytes())),
            byte_length: body.len() as u64,
        };
        let mut loaded_request = request.clone();
        loaded_request.artifact_slot = loaded.artifact_slot.clone();
        loaded_request.media_type = loaded.media_type.clone();
        let peer = async {
            let (mut socket, _) = listener.accept().await.unwrap();
            let mut head = Vec::new();
            while !head.ends_with(b"\r\n\r\n") {
                head.push(socket.read_u8().await.unwrap());
            }
            socket.write_all(format!("HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n{body}",body.len()).as_bytes()).await.unwrap();
        };
        let (result, ()) = timeout(Duration::from_secs(5), async {
            tokio::join!(
                stage_remote_artifact(
                    &transport,
                    &identity,
                    &load_submission,
                    authentication,
                    &loaded,
                    &loaded_request,
                    &store,
                    &loaded_capacity,
                    0
                ),
                peer
            )
        })
        .await
        .unwrap();
        let (stage, _) = result.unwrap();
        assert_eq!(stage.metadata().byte_length, body.len() as u64);
        assert_eq!(loaded_capacity.pending_publications().unwrap(), 1);
        drop(stage);
        assert_eq!(std::fs::read_dir(store_root.join("content")).unwrap().count(), 0);
    }
    let mut previous = None;
    for _ in 0..2 {
        let peer = async {
            let (mut socket, _) = listener.accept().await.unwrap();
            let mut head = Vec::new();
            while !head.ends_with(b"\r\n\r\n") {
                head.push(socket.read_u8().await.unwrap());
            }
            socket.write_all(b"HTTP/1.1 200 OK\r\nContent-Type: application/zip\r\nTransfer-Encoding: chunked\r\n\r\n3\r\nabc\r\n0\r\n\r\n").await.unwrap();
        };
        let (result, ()) = timeout(Duration::from_secs(5), async {
            tokio::join!(
                stage_remote_artifact(
                    &transport,
                    &identity,
                    &submission,
                    authentication,
                    &expected,
                    &request,
                    &store,
                    &capacity,
                    0
                ),
                peer
            )
        })
        .await
        .unwrap();
        let (stage, publication) = result.unwrap();
        assert!(!store_root.join("content").join(&expected.artifact_digest).exists());
        assert_eq!(stage.metadata().content_digest, expected.artifact_digest);
        assert_eq!(capacity.pending_publications().unwrap(), 1);
        assert_eq!(capacity.usage().unwrap().reserved_artifact_bytes, 0);
        assert_eq!(capacity.usage().unwrap().committed_artifact_bytes, 3);
        if let Some(previous) = &previous {
            assert_eq!(publication.identifier(), previous);
        }
        previous = Some(publication.identifier().to_owned());
        drop(stage);
        assert_eq!(std::fs::read_dir(store_root.join("content")).unwrap().count(), 0);
    }
    drop(capacity);
    drop(database);
    let reopened = OperationDatabase::open(&path, settings()).unwrap();
    assert_eq!(
        PersistentCapacityAccount::new(&reopened, PersistentCapacityPolicy::embedded())
            .pending_publications()
            .unwrap(),
        1
    );
}
impl PlatformTrustSource for Store {
    fn records(&self) -> Result<Vec<ProviderRecord>, ConfigurationDiagnostic> {
        Ok(self.0.clone())
    }
}

fn provider(endpoint: &str) -> EnvironmentAuthenticationProvider {
    EnvironmentAuthenticationProvider::new(selected_snapshot(endpoint), 1)
}

fn async_provider(endpoint: &str) -> slingshot_agent_connection::authentication::environment_provider::AsyncEnvironmentAuthenticationProvider {
    slingshot_agent_connection::authentication::environment_provider::AsyncEnvironmentAuthenticationProvider::new_async(selected_snapshot(endpoint)).unwrap()
}

fn selected_snapshot(endpoint: &str) -> SelectedEnvironmentSnapshot {
    selected_snapshot_with_publisher(endpoint, "http://publish.example.com")
}

fn selected_snapshot_with_publisher(endpoint: &str, publisher: &str) -> SelectedEnvironmentSnapshot {
    use slingshot_configuration::testing::credential_filesystem::ScriptedFilesystem;
    let profile = include_str!(
        "../../slingshot-test-support/fixtures/profile-directories/ordered/profiles/mike.toml"
    )
    .replace("http://author.example.com", endpoint)
    .replace("http://publish.example.com", publisher)
    .replace("allow_insecure_author_transport = true\n", "");
    let inventory = format!(
        "format_version = 1\n[[sources]]\nreference = \"profiles/mike.toml\"\nsha256 = \"{}\"\n",
        hex::encode(Sha256::digest(profile.as_bytes()))
    );
    let loaded = load_profiles(
        ScriptedFilesystem::new()
            .with_directory("profiles")
            .with_source("profiles/mike.toml", profile.as_bytes())
            .with_source("configuration-snapshot.toml", inventory.as_bytes()),
    )
    .unwrap();
    let selection = resolve(
        &loaded,
        &RequestedSelection {
            profile: Some(ProfileName::parse("remote-site").unwrap()),
            environment: Some(EnvironmentName::parse("staging").unwrap()),
        },
    )
    .unwrap();
    let chosen = selection.environment_of(&loaded);
    let certificates = slingshot_configuration::additional_certificate_authority::AdditionalAuthorCertificates::parse(
        include_bytes!("../../slingshot-test-support/fixtures/additional-certificate-authority/one-authority.pem")).unwrap();
    let platform = PlatformTrustSnapshot::take(&Store(
        certificates
            .certificates()
            .iter()
            .map(|der| ProviderRecord {
                der: der.clone(),
                decision: ProviderDecision::UnconditionallyTrustedForServerAuthentication,
            })
            .collect(),
    ))
    .unwrap();
    let identity_management = IdentityManagementTrustInput::from_platform(&platform).unwrap();
    let author_trust = AuthorTrustInput::from_platform_and_extension(&platform, None).unwrap();
    let EnvironmentAuthentication::BasicCredentials { user_name, .. } = chosen.authentication()
    else {
        panic!("Basic fixture")
    };
    let principal = AuthenticationPrincipalIdentity::basic("basic", user_name.as_text()).unwrap();
    let target = AuthorTargetIdentityDigest::build(
        chosen.deployment().as_text(),
        chosen.author_connection_target().as_text(),
        principal,
    )
    .unwrap();
    let revision = SelectedEnvironmentRevision::build(&RevisionFields {
        profile_name: selection.profile_name().as_text().to_owned(),
        environment_name: selection.environment_name().as_text().to_owned(),
        profile_source_reference: selection.profile_source().as_text().to_owned(),
        selection_source_reference: None,
        author_target_identity: target,
        publisher_base_address: chosen.publisher_metadata().as_text().to_owned(),
        authentication_method: "basic".to_owned(),
        credential_source_reference: None,
        certificate_source_reference: None,
        proxy_policy: "direct_without_ambient_discovery".to_owned(),
        allow_insecure_author_transport: false,
        canonical_metascope_set: CanonicalMetascopeSet::empty(),
        identity_management_trust_policy_identity: identity_management.identity(),
        author_trust_policy_identity: author_trust.identity(),
    })
    .unwrap();
    SelectedEnvironmentSnapshot::assemble(
            &selection,
            SnapshotMaterial {
                author: chosen.author_connection_target().clone(),
                publisher: chosen.publisher_metadata().clone(),
                deployment: chosen.deployment(),
                authentication: SnapshotAuthentication::BasicCredentials {
                    user_name: user_name.clone(),
                    password: SecretValue::from_text("not-a-real-password".to_owned()),
                },
                principal,
                target,
                revision,
                identity_management_trust: identity_management,
                author_trust,
            },
    )
}

struct NoTokenSource;
struct NoTokenClocks;
impl slingshot_agent_connection::authentication::identity_management_exchange::MonotonicClock for NoTokenClocks {
    fn reading_milliseconds(&self) -> u64 { panic!("Basic async policy must not sample token clocks") }
}
impl slingshot_agent_connection::authentication::token_assertion::CoordinatedUniversalTimeClock for NoTokenClocks {
    fn sample(&self) -> Option<u64> { panic!("Basic async policy must not issue assertions") }
}
struct AuthenticationClock(std::cell::Cell<usize>);
impl slingshot_agent_connection::authentication::identity_management_exchange::MonotonicClock for AuthenticationClock {
    fn reading_milliseconds(&self) -> u64 {
        self.0.set(self.0.get()+1);
        0
    }
}
impl slingshot_agent_connection::authentication::access_token_cache::AccessTokenSource
    for NoTokenSource
{
    fn exchange(
        &self,
    ) -> Result<
        slingshot_agent_connection::authentication::identity_management_exchange::AccessToken,
        slingshot_agent_connection::authentication::identity_management_exchange::ExchangeFailure,
    > {
        panic!("Basic must not exchange tokens")
    }
}

#[tokio::test]
async fn selected_admission_orders_preflight_persistence_post_and_restart_recovery() {
    use slingshot_agent_connection::command_submission::{
        ExpectedArtifactManifest, Submission, SubmissionOutcome,
    };
    use slingshot_agent_connection::selected_author_transport::SelectedAuthorTransport;
    use slingshot_agent_protocol::{
        identity::WireOperationIdentity, wire_contract::ExpectedProvenance,
    };
    use slingshot_daemon::operation::durable_author_submission::{
        prepare_initial_submission, submit_initial,
    };
    use slingshot_domain::{
        agent_identity::AgentEventStoreGeneration, operation_executor::ExecutionIdentity,
    };
    use slingshot_storage::{
        agent_job_repository::AgentJobRepository,
        database::{OperationDatabase, RequiredSettings},
    };
    use tokio::{
        io::{AsyncReadExt, AsyncWriteExt},
        time::{Duration, timeout},
    };
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let endpoint = format!("http://{}/aem", listener.local_addr().unwrap());
    let provider = provider(&endpoint);
    let transport = SelectedAuthorTransport::new(provider.snapshot().author_connection()).unwrap();
    let identity = ExecutionIdentity {
        attempt: 1,
        operation_identifier: "local-one".to_owned(),
        author_target_identity_digest: provider.snapshot().target().to_string(),
        selected_environment_revision: provider.snapshot().revision().to_string(),
    };
    let expected = ExpectedProvenance { command_contract: slingshot_domain::selected_command_contract_identity::SelectedCommandContractIdentity::installed("query_paths").unwrap(),
        canonical_json_contract_digest: slingshot_domain::command::schema::canonical_contract_digest(),
        transport_contract_digest: slingshot_domain::author_agent_transport_contract::AuthorAgentTransportContract::embedded_digest() };
    let submission = Submission::build(
        &expected,
        WireOperationIdentity::of(
            &identity.author_target_identity_digest,
            &identity.selected_environment_revision,
            &identity.operation_identifier,
            AgentEventStoreGeneration::of(7),
        ),
        "subscription-one",
        r#"{"root_path":"/content/example"}"#,
        ExpectedArtifactManifest::empty(),
    )
    .unwrap();
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("admission.sqlite3");
    let settings = || RequiredSettings {
        page_bytes: 4096,
        database_pages: 262144,
        busy_timeout_milliseconds: 5000,
    };
    let repository = AgentJobRepository::new(OperationDatabase::open(&path, settings()).unwrap());
    use slingshot_domain::command_fingerprint::{CommandFingerprint, FingerprintInput};
    use slingshot_domain::installation::InstallationIdentifier;
    use slingshot_storage::operation_repository::{AdmissionRequest, OperationRepository};
    let operations = OperationRepository::new(OperationDatabase::open(&path, settings()).unwrap());
    let admit_local = |identity: &ExecutionIdentity| {
        operations
            .admit(
                &AdmissionRequest {
                    author_target_identity: "opaque-target".to_owned(),
                    author_target_identity_digest: identity.author_target_identity_digest.clone(),
                    caller_identity: None,
                    canonical_command: r#"{"root_path":"/content/example"}"#.to_owned(),
                    command_fingerprint: CommandFingerprint::derive(&FingerprintInput {
                        author_target_identity_digest: identity
                            .author_target_identity_digest
                            .clone(),
                        canonical_command: r#"{"root_path":"/content/example"}"#.to_owned(),
                        command_wire_name: "query_paths".to_owned(),
                        command_semantic_contract_version: expected
                            .command_contract
                            .command_semantic_contract_version
                            .clone(),
                        selected_environment_revision: identity
                            .selected_environment_revision
                            .clone(),
                    })
                    .unwrap(),
                    command_wire_name: "query_paths".to_owned(),
                    daemon_runtime_contract_digest: slingshot_domain::daemon_runtime_contract::DaemonRuntimeContract::embedded_digest().as_text().to_owned(),
                    installation_identifier: InstallationIdentifier::parse(&"a1".repeat(32))
                        .unwrap(),
                    operation_identifier: identity.operation_identifier.clone(),
                    selected_environment_revision: identity.selected_environment_revision.clone(),
                    workflow_correlation_identifier: None,
                },
                1000,
            )
            .unwrap();
    };
    admit_local(&identity);
    let (authentication, _) = provider.authenticate(&endpoint, 1, &NoTokenSource).unwrap();
    let changed = Submission::build(
        &expected,
        submission.operation.clone(),
        "subscription-one",
        r#"{"different":true}"#,
        ExpectedArtifactManifest::empty(),
    )
    .unwrap();
    assert!(
        submit_initial(
            &repository,
            &operations,
            1,
            &transport,
            &identity,
            &changed,
            &authentication,
            1
        )
        .await
        .is_err()
    );
    assert!(
        submit_initial(
            &repository,
            &operations,
            2,
            &transport,
            &identity,
            &submission,
            &authentication,
            1
        )
        .await
        .is_err()
    );
    assert!(
        repository
            .read(
                &identity.author_target_identity_digest,
                &submission.operation.agent_operation_identifier
            )
            .unwrap()
            .is_none()
    );
    assert!(
        timeout(Duration::from_millis(20), listener.accept()).await.is_err(),
        "local drift reached the author"
    );
    let compatible = serde_json::json!({
        "format": "slingshot.agent/1",
        "agent_event_store_generation": 7,
        "canonical_json_contract_digest": expected.canonical_json_contract_digest,
        "transport_contract_digest": expected.transport_contract_digest,
        "command_contracts": [slingshot_agent_protocol::identity::WireContractIdentity::from(&expected.command_contract)],
        "continuation_authority_ready": true,
    });
    let drift_vectors: Vec<serde_json::Value> = include_str!("fixtures/author-agent-conformance/provenance-drift.jsonl")
        .lines().map(|line| serde_json::from_str(line).unwrap()).collect();
    let async_selected_provider = async_provider(&endpoint);
    for asynchronous in [false, true] {
    for field in drift_vectors.iter().map(|vector| vector["field"].as_str().unwrap())
        .chain(["agent_event_store_generation", "continuation_authority_ready"]) {
    let mut incompatible = compatible.clone();
    match field {
        "agent_event_store_generation" => incompatible[field] = serde_json::json!(8),
        "continuation_authority_ready" => incompatible[field] = serde_json::json!(false),
        "transport_contract_digest" | "canonical_json_contract_digest" => incompatible[field] = serde_json::json!("0".repeat(64)),
        "command_wire_name" => incompatible["command_contracts"][0][field] = serde_json::json!("create_page"),
        "command_semantic_contract_version" => incompatible["command_contracts"][0][field] = serde_json::json!("second"),
        "argument_schema_digest" | "result_schema_digest" | "command_contract_limits_digest" => incompatible["command_contracts"][0][field] = serde_json::json!("0".repeat(64)),
        _ => panic!("unmapped conformance provenance field: {field}"),
    }
    let incompatible = incompatible.to_string();
    let peer = async {
        let (mut socket, _) = listener.accept().await.unwrap();
        let mut head = Vec::new();
        while !head.ends_with(b"\r\n\r\n") {
            head.push(socket.read_u8().await.unwrap());
        }
        let head = String::from_utf8(head).unwrap();
        assert!(head.starts_with("GET /aem/bin/slingshot-agent/capabilities HTTP/1.1\r\n"));
        assert!(head.contains("Authorization: Basic "));
        socket
            .write_all(
                format!("HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n{incompatible}", incompatible.len()).as_bytes(),
            )
            .await
            .unwrap();
    };
    let (result, ()) = timeout(Duration::from_secs(5), async {
        tokio::join!(
            async {
            if asynchronous {
                slingshot_daemon::operation::durable_author_submission::submit_initial_with_authentication(
                    &repository, &operations, 1, &transport, &identity, &submission,
                    slingshot_daemon::operation::author_authentication::AuthorAuthentication::AsyncProvider {
                        provider: &async_selected_provider, clock: &NoTokenClocks, utc: &NoTokenClocks,
                    }, 1,
                ).await
            } else { submit_initial(
                &repository,
                &operations,
                1,
                &transport,
                &identity,
                &submission,
                &authentication,
                1
            ).await }
            },
            peer
        )
    })
    .await
    .unwrap();
    assert!(result.is_err(), "accepted drift {field}, async={asynchronous}");
    assert!(
        repository
            .read(
                &identity.author_target_identity_digest,
                &submission.operation.agent_operation_identifier
            )
            .unwrap()
            .is_none()
    );
    assert!(timeout(Duration::from_millis(20), listener.accept()).await.is_err());
    }
    }
    drop(prepare_initial_submission(&repository, &identity, &submission, 1).unwrap().unwrap());
    drop(repository);
    let repository = AgentJobRepository::new(OperationDatabase::open(&path, settings()).unwrap());
    let retained = repository
        .read(
            &identity.author_target_identity_digest,
            &submission.operation.agent_operation_identifier,
        )
        .unwrap()
        .unwrap();
    assert!(matches!(
        submit_initial(
            &repository,
            &operations,
            1,
            &transport,
            &identity,
            &submission,
            &authentication,
            2
        )
        .await
        .unwrap(),
        SubmissionOutcome::SubmissionUnknown { .. }
    ));
    assert_eq!(
        repository
            .read(
                &identity.author_target_identity_digest,
                &submission.operation.agent_operation_identifier
            )
            .unwrap()
            .unwrap(),
        retained
    );
    assert!(timeout(Duration::from_millis(20), listener.accept()).await.is_err());
    let async_provider = async_provider(&endpoint);
    for case in 0..16 {
        let asynchronous = case >= 13;
        let scenario = if asynchronous {case - 3} else {case};
        let provider_owned = scenario >= 10;
        let clock = AuthenticationClock(std::cell::Cell::new(0));
        let http2 = (6..9).contains(&scenario);
        let truncate = scenario == 1 || scenario == 7 || scenario == 11;
        let stale_before_post = scenario == 2 || scenario == 8 || scenario == 12;
        let identity = ExecutionIdentity {
            operation_identifier: if asynchronous {match scenario {10=>"async-accepted",11=>"async-lost-ack",_=>"async-stale-before-post"}} else if provider_owned {match scenario {10=>"provider-accepted",11=>"provider-lost-ack",_=>"provider-stale-before-post"}} else if scenario==9 {"automatic-accepted"} else if http2 {
                match scenario {6=>"h2-accepted",7=>"h2-lost-ack",_=>"h2-stale-before-post"}
            } else if scenario == 5 {
                "capacity-paused-result"
            } else if scenario == 4 {
                "externalized-result"
            } else if scenario == 3 {
                "inline-result"
            } else if stale_before_post {
                "stale-before-post"
            } else if truncate {
                "lost-ack"
            } else {
                "accepted"
            }
            .to_owned(),
            ..identity.clone()
        };
        admit_local(&identity);
        let submission = Submission::build(
            &expected,
            WireOperationIdentity::of(
                &identity.author_target_identity_digest,
                &identity.selected_environment_revision,
                &identity.operation_identifier,
                AgentEventStoreGeneration::of(7),
            ),
            "subscription-one",
            r#"{"root_path":"/content/example"}"#,
            ExpectedArtifactManifest::empty(),
        )
        .unwrap();
        let capability = compatible.to_string();
        let acknowledgement = serde_json::json!({
            "provenance": submission.provenance,
            "selected_environment_revision": identity.selected_environment_revision,
            "agent_event_store_generation": 7,
            "agent_operation_identifier": submission.operation.agent_operation_identifier,
            "author_target_identity_digest": identity.author_target_identity_digest,
            "already_accepted": false,
            "daemon_subscription_identifier": "subscription-one",
            "granted_retention_milliseconds": 120000,
            "physical_sling_job_identifiers": ["job-one"],
            "retired": false,
            "submitted_command_digest": submission.submitted_command_digest,
        })
        .to_string();
        let observer = AgentJobRepository::new(OperationDatabase::open(&path, settings()).unwrap());
        let concurrent =
            OperationRepository::new(OperationDatabase::open(&path, settings()).unwrap());
        use slingshot_daemon::author_agent_operation_executor::{AuthorPorts, ProductAuthorPorts};
        use slingshot_daemon::operation::remote_submission::HandoffDisposition;
        let store = slingshot_storage::artifact_store::ArtifactStore::open(
            &root.path().join("initial-artifacts"),
        )
        .unwrap();
        let capacity = slingshot_storage::persistent_capacity::PersistentCapacityAccount::new(
            operations.database(),
            slingshot_domain::persistent_capacity::PersistentCapacityPolicy::embedded(),
        );
        use slingshot_daemon::operation::author_authentication::AuthorAuthentication;
        if scenario == 0 {
            for revision_only in [false, true] {
                let foreign_endpoint = if revision_only {endpoint.clone()} else {format!("{endpoint}/foreign")};
                let foreign_publisher = if revision_only {"http://another-publisher.example.com"} else {"http://publish.example.com"};
                let foreign = EnvironmentAuthenticationProvider::new(selected_snapshot_with_publisher(&foreign_endpoint, foreign_publisher), 2);
                let foreign_async = slingshot_agent_connection::authentication::environment_provider::AsyncEnvironmentAuthenticationProvider::new_async(selected_snapshot_with_publisher(&foreign_endpoint, foreign_publisher)).unwrap();
                assert_eq!(foreign.snapshot().target() == provider.snapshot().target(), revision_only);
                assert_ne!(foreign.snapshot().revision(), provider.snapshot().revision());
                let (foreign_fixed, _) = foreign.authenticate(&foreign_endpoint, 0, &NoTokenSource).unwrap();
                let before = repository.read(&identity.author_target_identity_digest, &submission.operation.agent_operation_identifier).unwrap();
                for policy in [
                    AuthorAuthentication::Fixed {authentication:&foreign_fixed, protocol:slingshot_daemon::operation::subscription_reset::ResetTransport::Automatic},
                    AuthorAuthentication::Provider {provider:&foreign,source:&NoTokenSource,clock:&NoTokenClocks},
                    AuthorAuthentication::AsyncProvider {provider:&foreign_async,clock:&NoTokenClocks,utc:&NoTokenClocks},
                ] {
                    assert!(slingshot_daemon::retained_author_protocol::RetainedAuthorProtocol::new_with_authentication(
                        &operations, &repository, &store, &capacity, policy, identity.clone(), submission.clone(), 1000,
                    ).is_err(), "foreign authentication accepted at construction: revision_only={revision_only}");
                }
                assert_eq!(repository.read(&identity.author_target_identity_digest, &submission.operation.agent_operation_identifier).unwrap(), before);
                assert!(timeout(Duration::from_millis(20), listener.accept()).await.is_err());
            }
        }
        let authentication_policy = if asynchronous {
            AuthorAuthentication::AsyncProvider {provider:&async_provider,clock:&NoTokenClocks,utc:&NoTokenClocks}
        } else if provider_owned {
            AuthorAuthentication::Provider {provider:&provider,source:&NoTokenSource,clock:&clock}
        } else {AuthorAuthentication::Fixed {authentication:&authentication,protocol:
            if scenario==9 {slingshot_daemon::operation::subscription_reset::ResetTransport::Automatic} else if http2 {slingshot_daemon::operation::subscription_reset::ResetTransport::Http2} else {slingshot_daemon::operation::subscription_reset::ResetTransport::Http1}}};
        let protocol = slingshot_daemon::retained_author_protocol::RetainedAuthorProtocol::new_with_authentication(
            &operations,
            &repository,
            &store,
            &capacity,
            authentication_policy,
            identity.clone(),
            submission.clone(),
            1000,
        )
        .unwrap();
        let ports =
            ProductAuthorPorts::new(provider.snapshot().author_connection(), &protocol).unwrap();
        let command = serde_json::from_value(
            serde_json::json!({"command":"query_paths","root_path":"/content/example"}),
        )
        .unwrap();
        let other_command = serde_json::from_value(
            serde_json::json!({"command":"query_paths","root_path":"/content/other"}),
        )
        .unwrap();
        assert_eq!(ports.submit(&identity, &other_command).await, HandoffDisposition::Conflict);
        assert!(
            timeout(Duration::from_millis(10), listener.accept()).await.is_err(),
            "command substitution reached the author"
        );
        let peer = async {
            for stage in 0..if stale_before_post { 3 } else { 4 } {
                let (mut socket, _) = listener.accept().await.unwrap();
                if http2 {
                    let mut preface=[0;39]; socket.read_exact(&mut preface).await.unwrap();
                    assert_eq!(&preface[..24],b"PRI * HTTP/2.0\r\n\r\nSM\r\n\r\n");
                    socket.write_all(&[0,0,0,4,0,0,0,0,0]).await.unwrap();
                    let mut frame=[0;9]; socket.read_exact(&mut frame).await.unwrap();
                    assert_eq!(frame,[0,0,0,4,1,0,0,0,0]); socket.write_all(&frame).await.unwrap();
                    socket.read_exact(&mut frame).await.unwrap(); assert_eq!((frame[3],frame[4]),(1,if stage==3 {4} else {5}));
                    let length=usize::from(frame[0])<<16|usize::from(frame[1])<<8|usize::from(frame[2]);
                    assert!(length<=16384); let mut head=vec![0;length]; socket.read_exact(&mut head).await.unwrap();
                    authentication.lend_value_bytes(|value|assert!(head.windows(value.len()).any(|part|part==value)));
                    let retained=observer.read(&identity.author_target_identity_digest,&submission.operation.agent_operation_identifier).unwrap();
                    if stage==0 {assert!(retained.is_none(),"HTTP/2 admission preceded capability");}
                    else {assert_eq!(retained.unwrap().canonical_submission.as_bytes(),submission.wire_body().unwrap());}
                    let route=match stage {0|1=>"/aem/bin/slingshot-agent/capabilities",2=>"/aem/libs/granite/csrf/token.json",_=>"/aem/bin/slingshot-agent/jobs"};
                    assert!(head.windows(route.len()).any(|part|part==route.as_bytes()));
                    if stage==2 && stale_before_post {
                        concurrent.apply(&identity.author_target_identity_digest,&identity.operation_identifier,1,
                            &slingshot_domain::operation::OperationFact::Progress {detail:"concurrent local change".into()},1000).unwrap();
                    }
                    if stage==3 {
                        for value in ["one-use-token",submission.operation.agent_operation_identifier.as_str()] {
                            assert!(head.windows(value.len()).any(|part|part==value.as_bytes()));
                        }
                        let mut received=Vec::new();
                        loop {
                            socket.read_exact(&mut frame).await.unwrap(); assert_eq!(frame[3],0);
                            let length=usize::from(frame[0])<<16|usize::from(frame[1])<<8|usize::from(frame[2]);
                            assert!(length<=16384); let start=received.len(); received.resize(start+length,0); socket.read_exact(&mut received[start..]).await.unwrap();
                            if frame[4]&1!=0 {break;}
                        }
                        assert_eq!(received,submission.wire_body().unwrap());
                    }
                    let body=match stage {0|1=>capability.as_str(),2=>r#"{"token":"one-use-token"}"#,_=>acknowledgement.as_str()};
                    let mut block=if stage==3 {vec![8,3,b'2',b'0',b'2']} else {vec![0x88]};
                    for (name,value) in [("content-type","application/json".to_owned()),("content-length",body.len().to_string())] {
                        block.extend_from_slice(&[0,name.len() as u8]); block.extend_from_slice(name.as_bytes()); block.push(value.len() as u8); block.extend_from_slice(value.as_bytes());
                    }
                    let sent=if stage==3 && truncate {&body.as_bytes()[..body.len()/2]} else {body.as_bytes()};
                    for (kind,flags,bytes) in [(1,4,block.as_slice()),(0,1,sent)] {
                        let length=(bytes.len() as u32).to_be_bytes(); socket.write_all(&[length[1],length[2],length[3],kind,flags,0,0,0,1]).await.unwrap(); socket.write_all(bytes).await.unwrap();
                    }
                    let mut close=Vec::new(); let _=socket.read_to_end(&mut close).await;
                    continue;
                }
                let mut head = Vec::new();
                while !head.ends_with(b"\r\n\r\n") {
                    head.push(socket.read_u8().await.unwrap());
                }
                let head = String::from_utf8(head).unwrap();
                assert!(head.contains("Authorization: Basic "));
                let retained = observer
                    .read(
                        &identity.author_target_identity_digest,
                        &submission.operation.agent_operation_identifier,
                    )
                    .unwrap();
                if stage == 0 {
                    assert!(retained.is_none(), "admission preceded capability");
                } else {
                    assert_eq!(
                        retained.unwrap().canonical_submission.as_bytes(),
                        submission.wire_body().unwrap()
                    );
                }
                let (body, status) = match stage {
                    0 | 1 => {
                        assert!(
                            head.starts_with(
                                "GET /aem/bin/slingshot-agent/capabilities HTTP/1.1\r\n"
                            )
                        );
                        (capability.as_str(), 200)
                    }
                    2 => {
                        if stale_before_post {
                            concurrent
                                .apply(
                                    &identity.author_target_identity_digest,
                                    &identity.operation_identifier,
                                    1,
                                    &slingshot_domain::operation::OperationFact::Progress {
                                        detail: "concurrent local change".to_owned(),
                                    },
                                    1000,
                                )
                                .unwrap();
                        }
                        assert!(
                            head.starts_with("GET /aem/libs/granite/csrf/token.json HTTP/1.1\r\n")
                        );
                        (r#"{"token":"one-use-token"}"#, 200)
                    }
                    _ => {
                        assert!(
                            head.starts_with("POST /aem/bin/slingshot-agent/jobs HTTP/1.1\r\n")
                        );
                        assert!(head.contains("csrf-token: one-use-token\r\n"));
                        assert!(head.contains(&format!(
                            "idempotency-key: {}\r\n",
                            submission.operation.agent_operation_identifier
                        )));
                        let length: usize = head
                            .lines()
                            .find_map(|line| line.strip_prefix("Content-Length: "))
                            .unwrap()
                            .parse()
                            .unwrap();
                        let mut body = vec![0; length];
                        socket.read_exact(&mut body).await.unwrap();
                        assert_eq!(body, submission.wire_body().unwrap());
                        (acknowledgement.as_str(), 202)
                    }
                };
                let declared = body.len();
                let sent = if stage == 3 && truncate { &body[..body.len() / 2] } else { body };
                socket.write_all(format!("HTTP/1.1 {status} OK\r\nContent-Type: application/json\r\nContent-Length: {declared}\r\n\r\n{sent}").as_bytes()).await.unwrap();
            }
        };
        let (result, ()) = timeout(Duration::from_secs(5), async {
            tokio::join!(ports.submit(&identity, &command), peer)
        })
        .await
        .unwrap();
        if provider_owned && !asynchronous {assert_eq!(clock.0.get(),3,"capability-before/after admission and submission sample fresh clock readings");}
        if stale_before_post {
            assert_eq!(
                result,
                HandoffDisposition::Unknown,
                "revision changed during token acquisition"
            );
            assert!(
                observer
                    .read(
                        &identity.author_target_identity_digest,
                        &submission.operation.agent_operation_identifier
                    )
                    .unwrap()
                    .is_some(),
                "pending child must remain recoverable"
            );
            assert!(
                observer
                    .physical_jobs(
                        &identity.author_target_identity_digest,
                        &submission.operation.agent_operation_identifier
                    )
                    .unwrap()
                    .is_empty()
            );
            assert!(
                timeout(Duration::from_millis(20), listener.accept()).await.is_err(),
                "stale token preflight caused a POST"
            );
            continue;
        }
        if truncate {
            assert_eq!(result, HandoffDisposition::Unknown);
        } else {
            assert_eq!(result, HandoffDisposition::Accepted);
        }
        let retained = observer
            .read(
                &identity.author_target_identity_digest,
                &submission.operation.agent_operation_identifier,
            )
            .unwrap()
            .unwrap();
        assert!(retained.terminal_disposition.is_none());
        let jobs = observer
            .physical_jobs(
                &identity.author_target_identity_digest,
                &submission.operation.agent_operation_identifier,
            )
            .unwrap();
        assert_eq!(jobs.len(), if truncate { 0 } else { 1 });
        if !truncate {
            assert!(retained.remaining_retention_milliseconds > 0);
        }
        drop(observer);
        let reopened = AgentJobRepository::new(OperationDatabase::open(&path, settings()).unwrap());
        if provider_owned {
            let observed = clock.0.get();
            let repeated = slingshot_daemon::operation::durable_author_submission::submit_initial_with_authentication(
                &reopened,&operations,1,&transport,&identity,&submission,
                authentication_policy,2000,
            ).await.unwrap();
            assert!(matches!(repeated,SubmissionOutcome::SubmissionUnknown {
                cause:slingshot_agent_connection::command_submission::UnknownCause::LookupRequired,
            }));
            assert_eq!(clock.0.get(),observed,"existing child must not acquire authentication");
        }
        let resumed_protocol =
            slingshot_daemon::retained_author_protocol::RetainedAuthorProtocol::new_with_authentication(
                &operations,
                &reopened,
                &store,
                &capacity,
                authentication_policy,
                identity.clone(),
                submission.clone(),
                2000,
            )
            .unwrap();
        let resumed_ports =
            ProductAuthorPorts::new(provider.snapshot().author_connection(), &resumed_protocol)
                .unwrap();
        let before_resume_clock = clock.0.get();
        let handoff = resumed_ports.submit(&identity, &command).await;
        assert_eq!(clock.0.get(),before_resume_clock,"retained product restart must not acquire credentials");
        assert_eq!(
            handoff,
            slingshot_daemon::operation::remote_submission::HandoffDisposition::ReconcileRetained
        );
        assert!(handoff.requires_lookup());
        assert!(!handoff.permits_another_send());
        assert_eq!(
            slingshot_daemon::author_agent_operation_executor::outcome_of_handoff(&handoff),
            None
        );
        assert_eq!(
            reopened
                .read(
                    &identity.author_target_identity_digest,
                    &submission.operation.agent_operation_identifier
                )
                .unwrap()
                .unwrap(),
            retained
        );
        assert!(
            timeout(Duration::from_millis(20), listener.accept()).await.is_err(),
            "restart caused another request"
        );
        if http2 || scenario>=9 { continue; }
        use slingshot_daemon::operation::durable_author_lookup::lookup_retained_operation;
        use slingshot_domain::operation::{
            OperationExecutionCertainty, OperationFact, RecoveryCategory,
            RecoveryExecutionEvidence, RecoveryFact,
        };
        let operations =
            OperationRepository::new(OperationDatabase::open(&path, settings()).unwrap());
        let writer = OperationRepository::new(OperationDatabase::open(&path, settings()).unwrap());
        admit_local(&identity);
        let snapshot = serde_json::json!({
            "provenance": submission.provenance,
            "agent_event_store_generation": 7,
            "agent_operation_identifier": submission.operation.agent_operation_identifier,
            "author_target_identity_digest": identity.author_target_identity_digest,
            "selected_environment_revision": identity.selected_environment_revision,
            "daemon_subscription_identifier": "subscription-one",
            "submitted_command_digest": submission.submitted_command_digest,
            "subscription_watermark":"cursor-010", "physical_sling_job_identifiers": [if truncate { "job-two" } else { "job-one" }],
            "granted_retention_milliseconds": 120000,
            "attempt": 1, "progress": 10, "sequence": 2, "kind": "progress",
        })
        .to_string();
        for revision in [1, 2] {
            let peer = async {
                for stage in 0..2 {
                    let (mut socket, _) = listener.accept().await.unwrap();
                    let mut head = Vec::new();
                    while !head.ends_with(b"\r\n\r\n") {
                        head.push(socket.read_u8().await.unwrap());
                    }
                    let head = String::from_utf8(head).unwrap();
                    let body = if stage == 0 {
                        assert!(
                            head.starts_with(
                                "GET /aem/bin/slingshot-agent/capabilities HTTP/1.1\r\n"
                            )
                        );
                        &capability
                    } else {
                        assert!(head.starts_with(&format!("GET /aem/bin/slingshot-agent/operations/lookup?agent_operation_identifier={} HTTP/1.1\r\n", submission.operation.agent_operation_identifier)));
                        if revision == 1 {
                            writer.apply(&identity.author_target_identity_digest, &identity.operation_identifier, 1,
                                &OperationFact::Recovery { recovery: RecoveryFact {
                                    attempt_count: 1, category: RecoveryCategory::OperationLookup,
                                    detail: "concurrent recovery".to_owned(),
                                    evidence: RecoveryExecutionEvidence::ExecutionCertainty { certainty: OperationExecutionCertainty::SubmissionUnknown },
                                    manual_resume_eligible: false, retry_delay_milliseconds: 0,
                                    retry_observed_at_unix_milliseconds: 2000,
                                }}, 2000).unwrap();
                        }
                        &snapshot
                    };
                    socket.write_all(format!("HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n{body}", body.len()).as_bytes()).await.unwrap();
                }
            };
            let (result, ()) = timeout(Duration::from_secs(5), async {
                tokio::join!(
                    lookup_retained_operation(
                        &reopened,
                        &operations,
                        revision,
                        &transport,
                        &identity,
                        &submission,
                        &authentication,
                        2000
                    ),
                    peer
                )
            })
            .await
            .unwrap();
            assert_eq!(result.is_ok(), revision == 2);
            let current = reopened
                .read(
                    &identity.author_target_identity_digest,
                    &submission.operation.agent_operation_identifier,
                )
                .unwrap()
                .unwrap();
            if revision == 1 {
                assert_eq!(current, retained, "late snapshot changed the child");
                assert_eq!(
                    reopened
                        .physical_jobs(
                            &identity.author_target_identity_digest,
                            &submission.operation.agent_operation_identifier
                        )
                        .unwrap(),
                    jobs
                );
            } else {
                assert_eq!(current.snapshot_watermark.value(), 2);
                assert_eq!(current.observation.progress, 10);
            }
            assert!(
                timeout(Duration::from_millis(20), listener.accept()).await.is_err(),
                "lookup caused a POST"
            );
        }
        if scenario >= 3 {
            let artifact_database = OperationDatabase::open_live(&path, settings()).unwrap();
            let mut policy =
                slingshot_domain::persistent_capacity::PersistentCapacityPolicy::embedded();
            if scenario == 5 {
                policy.individual_artifact_bytes = 0;
            }
            let capacity = slingshot_storage::persistent_capacity::PersistentCapacityAccount::new(
                &artifact_database,
                policy,
            );
            let store = slingshot_storage::artifact_store::ArtifactStore::open(
                &path.with_extension("artifacts"),
            )
            .unwrap();
            let successful_payload = if scenario >= 4 {
                slingshot_domain::command::canonical_json::write_canonical(&serde_json::json!({"matches":
                    (0..1000).map(|index| serde_json::json!({"repository_path": format!("/content/example/{index:04}")})).collect::<Vec<_>>()
                })).unwrap()
            } else {
                r#"{"matches":[]}"#.to_owned()
            };
            for (payload, expected_revision, valid) in [
                (r#"{"matches":[{"repository_path":"/another-request"}]}"#, 2, false),
                (successful_payload.as_str(), 3, true),
            ] {
                let mut succeeded: serde_json::Value = serde_json::from_str(&snapshot).unwrap();
                succeeded["kind"] = serde_json::json!("succeeded");
                succeeded["sequence"] = serde_json::json!(3);
                succeeded["terminal_result"] = serde_json::json!({
                    "operation": submission.operation,
                    "daemon_subscription_identifier": submission.daemon_subscription_identifier,
                    "provenance": submission.provenance,
                    "submitted_command_digest": submission.submitted_command_digest,
                    "canonical_result": payload, "declared_artifacts": []
                });
                let succeeded = succeeded.to_string();
                let peer = async {
                    for body in [&capability, &succeeded] {
                        let (mut socket, _) = listener.accept().await.unwrap();
                        let mut head = Vec::new();
                        while !head.ends_with(b"\r\n\r\n") {
                            head.push(socket.read_u8().await.unwrap());
                        }
                        assert!(
                            String::from_utf8(head)
                                .unwrap()
                                .starts_with("GET /aem/bin/slingshot-agent/")
                        );
                        socket.write_all(format!("HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n{body}", body.len()).as_bytes()).await.unwrap();
                    }
                };
                let (result, ()) = timeout(Duration::from_secs(5), async {
                    tokio::join!(
                        slingshot_daemon::operation::durable_author_lookup::lookup_retained_operation_with_completion(
                            &reopened,
                            &operations,
                            expected_revision,
                            &transport,
                            &identity,
                            &submission,
                            &authentication,
                            3000,
                            Some((&store, &capacity)),
                        ),
                        peer
                    )
                })
                .await
                .unwrap();
                assert_eq!(result.is_ok(), valid);
                let held = operations
                    .read(&identity.author_target_identity_digest, &identity.operation_identifier)
                    .unwrap()
                    .unwrap();
                if valid && scenario == 5 {
                    assert!(!held.record.lifecycle_state.is_terminal());
                    assert_eq!(held.record.revision, 4);
                    let paused = held.record.outstanding_recovery.as_ref().unwrap();
                    assert_eq!(paused.category, RecoveryCategory::PersistentCapacityUnavailable);
                    assert_eq!(
                        paused.evidence,
                        RecoveryExecutionEvidence::AuthoritativeRemoteSuccess
                    );
                    assert!(paused.manual_resume_eligible);
                    assert_eq!(paused.attempt_count, 0);
                    assert_eq!(capacity.pending_publications().unwrap(), 0);
                    assert!(
                        timeout(
                            Duration::from_secs(1),
                            lookup_retained_operation(
                                &reopened,
                                &operations,
                                4,
                                &transport,
                                &identity,
                                &submission,
                                &authentication,
                                4000
                            )
                        )
                        .await
                        .unwrap()
                        .is_err()
                    );
                    let retained = reopened
                        .read(
                            &identity.author_target_identity_digest,
                            &submission.operation.agent_operation_identifier,
                        )
                        .unwrap()
                        .unwrap();
                    let snapshot =
                        slingshot_storage::agent_job_repository::SuccessfulAgentSnapshot {
                            observation: slingshot_domain::remote_job::RemoteJobObservation {
                                state: slingshot_domain::remote_job::AgentJobState::Succeeded,
                                applied_sequence:
                                    slingshot_domain::remote_job::JobEventSequence::of(3),
                                attempt: 1,
                                progress: 10,
                            },
                            physical_sling_job_identifiers: vec!["job-one".to_owned()],
                            remaining_retention_milliseconds: 120000,
                        };
                    // Restoring capacity alone is not a manual-resume request.
                    let available = slingshot_storage::persistent_capacity::PersistentCapacityAccount::new(
                        &artifact_database, slingshot_domain::persistent_capacity::PersistentCapacityPolicy::embedded());
                    let envelope: serde_json::Value = serde_json::from_str(&succeeded).unwrap();
                    assert!(slingshot_daemon::operation::artifact_completion::publish_retained_snapshot_result(
                        &operations, &retained, 4, &identity, &submission,
                        &serde_json::to_vec(&envelope["terminal_result"]).unwrap(), 4000,
                        &snapshot, &store, &available,
                    ).is_err());
                    assert_eq!(
                        operations
                            .read(
                                &identity.author_target_identity_digest,
                                &identity.operation_identifier
                            )
                            .unwrap(),
                        Some(held)
                    );
                    assert_eq!(available.pending_publications().unwrap(), 0);
                    use slingshot_daemon::operation::durable_author_lookup::activate_retained_resume;
                    use slingshot_daemon::operation_recovery::{
                        ResumeRequest, ResumeResponse, resume,
                    };
                    let mut request = ResumeRequest {
                        author_target_identity_digest: identity
                            .author_target_identity_digest
                            .clone(),
                        expected_recovery_category: RecoveryCategory::PersistentCapacityUnavailable,
                        expected_revision: 4,
                        operation_identifier: identity.operation_identifier.clone(),
                        selected_environment_revision: identity
                            .selected_environment_revision
                            .clone(),
                    };
                    let ResumeResponse::Applied(receipt) =
                        resume(&operations, &request, 4001).unwrap()
                    else {
                        panic!("fresh receipt");
                    };
                    let mut forged = (*receipt).clone();
                    forged.recorded_at_unix_milliseconds += 1;
                    assert!(
                        activate_retained_resume(
                            &reopened,
                            &operations,
                            &transport,
                            &identity,
                            &submission,
                            &forged,
                            request.expected_recovery_category,
                            4002
                        )
                        .is_err()
                    );
                    let mut moved_child = retained.clone();
                    moved_child.snapshot_watermark =
                        slingshot_domain::remote_job::JobEventSequence::of(99);
                    assert!(
                        operations
                            .activate_retained_recovery(
                                &moved_child,
                                &receipt,
                                request.expected_recovery_category,
                                4002
                            )
                            .is_err()
                    );
                    let activated = activate_retained_resume(
                        &reopened,
                        &operations,
                        &transport,
                        &identity,
                        &submission,
                        &receipt,
                        request.expected_recovery_category,
                        4002,
                    )
                    .unwrap()
                    .unwrap();
                    assert_eq!(activated.record.revision, 5);
                    assert!(
                        !activated
                            .record
                            .outstanding_recovery
                            .as_ref()
                            .unwrap()
                            .manual_resume_eligible
                    );
                    assert!(
                        activate_retained_resume(
                            &reopened,
                            &operations,
                            &transport,
                            &identity,
                            &submission,
                            &receipt,
                            request.expected_recovery_category,
                            4002
                        )
                        .unwrap()
                        .is_none()
                    );
                    assert!(matches!(
                        resume(&operations, &request, 4002).unwrap(),
                        ResumeResponse::Replayed(_)
                    ));
                    let body = serde_json::to_vec(&envelope["terminal_result"]).unwrap();
                    // Capacity can still be absent after an authorized resume.
                    assert!(slingshot_daemon::operation::artifact_completion::publish_retained_snapshot_result(
                        &operations, &retained, 5, &identity, &submission, &body, 4003, &snapshot, &store, &capacity,
                    ).unwrap().is_none());
                    let paused_again = operations
                        .read(
                            &identity.author_target_identity_digest,
                            &identity.operation_identifier,
                        )
                        .unwrap()
                        .unwrap();
                    assert_eq!(paused_again.record.revision, 6);
                    assert!(
                        paused_again
                            .record
                            .outstanding_recovery
                            .as_ref()
                            .unwrap()
                            .manual_resume_eligible
                    );
                    assert!(
                        activate_retained_resume(
                            &reopened,
                            &operations,
                            &transport,
                            &identity,
                            &submission,
                            &receipt,
                            request.expected_recovery_category,
                            4004
                        )
                        .unwrap()
                        .is_none()
                    );
                    assert_eq!(
                        operations
                            .read(
                                &identity.author_target_identity_digest,
                                &identity.operation_identifier
                            )
                            .unwrap(),
                        Some(paused_again)
                    );
                    request.expected_revision = 6;
                    let ResumeResponse::Applied(next_receipt) =
                        resume(&operations, &request, 4004).unwrap()
                    else {
                        panic!("new cycle receipt");
                    };
                    let activated = activate_retained_resume(
                        &reopened,
                        &operations,
                        &transport,
                        &identity,
                        &submission,
                        &next_receipt,
                        request.expected_recovery_category,
                        4005,
                    )
                    .unwrap()
                    .unwrap();
                    assert_eq!(activated.record.revision, 7);
                    let completed = slingshot_daemon::operation::artifact_completion::publish_retained_snapshot_result(
                        &operations, &retained, 7, &identity, &submission, &body, 4006, &snapshot, &store, &available,
                    ).unwrap().unwrap();
                    assert_eq!(completed.record.revision, 8);
                    assert!(completed.record.lifecycle_state.is_terminal());
                    assert!(completed.record.terminal_failure.is_none());
                    assert!(
                        activate_retained_resume(
                            &reopened,
                            &operations,
                            &transport,
                            &identity,
                            &submission,
                            &next_receipt,
                            request.expected_recovery_category,
                            4007
                        )
                        .unwrap()
                        .is_none()
                    );
                    continue;
                }
                assert_eq!(held.record.lifecycle_state.is_terminal(), valid);
                if valid {
                    assert_eq!(held.record.revision, 4);
                    if scenario == 4 {
                        assert!(held.result_inline_bytes.is_none());
                        let artifact =
                            slingshot_storage::artifact_store::ArtifactAssociations::new(
                                &artifact_database,
                            )
                            .read(
                                &identity.author_target_identity_digest,
                                &identity.operation_identifier,
                                "structured_result",
                            )
                            .unwrap()
                            .unwrap();
                        let mut reader = store.open_verified(&artifact).unwrap();
                        let mut received = Vec::new();
                        let mut buffer = [0_u8; 4096];
                        loop {
                            let count = reader.read_into(&mut buffer).unwrap();
                            if count == 0 {
                                break;
                            }
                            received.extend_from_slice(&buffer[..count]);
                        }
                        reader.finish().unwrap();
                        assert_eq!(received, payload.as_bytes());
                        assert_eq!(capacity.pending_publications().unwrap(), 0);
                    } else {
                        assert_eq!(held.result_inline_bytes.as_deref(), Some(payload));
                    }
                    assert!(held.record.outstanding_recovery.is_none());
                    assert!(held.record.terminal_failure.is_none());
                    let restarted = AgentJobRepository::new(
                        OperationDatabase::open_live(&path, settings()).unwrap(),
                    );
                    let remote = restarted
                        .read(
                            &identity.author_target_identity_digest,
                            &submission.operation.agent_operation_identifier,
                        )
                        .unwrap()
                        .unwrap();
                    assert_eq!(
                        remote.observation.state,
                        slingshot_domain::remote_job::AgentJobState::Succeeded
                    );
                    assert_eq!(remote.snapshot_watermark.value(), 3);
                    assert_eq!(
                        remote.terminal_disposition.as_deref(),
                        Some("authoritative-remote-success")
                    );
                } else {
                    assert_eq!(held.record.revision, 3);
                    assert!(held.result_inline_bytes.is_none());
                    assert_eq!(
                        held.record.outstanding_recovery.unwrap().evidence,
                        RecoveryExecutionEvidence::AuthoritativeRemoteSuccess
                    );
                }
            }
            assert!(
                timeout(Duration::from_millis(20), listener.accept()).await.is_err(),
                "result acquisition caused another POST"
            );
            continue;
        }
        let revision = if truncate {
            let mut succeeded: serde_json::Value = serde_json::from_str(&snapshot).unwrap();
            succeeded["kind"] = serde_json::json!("succeeded");
            succeeded["sequence"] = serde_json::json!(3);
            let succeeded = succeeded.to_string();
            let event_writer =
                AgentJobRepository::new(OperationDatabase::open(&path, settings()).unwrap());
            for (local_revision, concurrent_event) in [(2, true), (2, false), (4, false)] {
                let peer = async {
                    for stage in 0..2 {
                        let (mut socket, _) = listener.accept().await.unwrap();
                        let mut head = Vec::new();
                        while !head.ends_with(b"\r\n\r\n") {
                            head.push(socket.read_u8().await.unwrap());
                        }
                        assert!(
                            String::from_utf8(head)
                                .unwrap()
                                .starts_with("GET /aem/bin/slingshot-agent/")
                        );
                        if stage == 1 && concurrent_event {
                            let child = event_writer
                                .read(
                                    &identity.author_target_identity_digest,
                                    &submission.operation.agent_operation_identifier,
                                )
                                .unwrap()
                                .unwrap();
                            event_writer
                                .record_snapshot_watermark(
                                    &child.identity,
                                    slingshot_domain::remote_job::JobEventSequence::of(3),
                                )
                                .unwrap();
                        }
                        let body = if stage == 0 { &capability } else { &succeeded };
                        socket.write_all(format!("HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n{body}", body.len()).as_bytes()).await.unwrap();
                    }
                };
                let (result, ()) = timeout(Duration::from_secs(5), async {
                    tokio::join!(
                        lookup_retained_operation(
                            &reopened,
                            &operations,
                            local_revision,
                            &transport,
                            &identity,
                            &submission,
                            &authentication,
                            3000
                        ),
                        peer
                    )
                })
                .await
                .unwrap();
                assert_eq!(result.is_ok(), !concurrent_event);
                let held = operations
                    .read(&identity.author_target_identity_digest, &identity.operation_identifier)
                    .unwrap()
                    .unwrap();
                if concurrent_event {
                    assert_eq!(
                        held.record.revision, 2,
                        "stale remote evidence changed the local operation"
                    );
                    assert_ne!(
                        held.record.outstanding_recovery.unwrap().evidence,
                        RecoveryExecutionEvidence::AuthoritativeRemoteSuccess
                    );
                    continue;
                }
                assert_eq!(held.record.revision, if local_revision == 2 { 4 } else { 5 }, "missing result did not consume acquisition attempt");
                assert!(!held.record.lifecycle_state.is_terminal());
                let recovery = held.record.outstanding_recovery.unwrap();
                assert_eq!(recovery.category, RecoveryCategory::ResultAcquisition);
                assert_eq!(
                    recovery.evidence,
                    RecoveryExecutionEvidence::AuthoritativeRemoteSuccess
                );
                assert_eq!(recovery.attempt_count, if local_revision == 2 { 1 } else { 2 });
            }
            5
        } else {
            2
        };
        let changed = Submission::build(
            &expected,
            submission.operation.clone(),
            "subscription-one",
            r#"{"different":true}"#,
            ExpectedArtifactManifest::empty(),
        )
        .unwrap();
        let echo = slingshot_agent_connection::job_snapshot_reconciliation::SnapshotEcho {
            provenance: changed.provenance.clone(),
            agent_event_store_generation: changed.operation.agent_event_store_generation,
            agent_operation_identifier: changed.operation.agent_operation_identifier.clone(),
            author_target_identity_digest: identity.author_target_identity_digest.clone(),
            selected_environment_revision: identity.selected_environment_revision.clone(),
            daemon_subscription_identifier: changed.daemon_subscription_identifier.clone(),
            submitted_command_digest: changed.submitted_command_digest.clone(),
        };
        let before = operations
            .read(&identity.author_target_identity_digest, &identity.operation_identifier)
            .unwrap()
            .unwrap();
        assert!(
            slingshot_daemon::operation::durable_author_lookup::record_retired_lookup(
                &operations,
                &retained,
                &identity,
                &changed,
                &echo,
                revision,
                3000
            )
            .is_err(),
            "matching remote echoes must not settle different local command bytes"
        );
        assert!(
            lookup_retained_operation(
                &reopened,
                &operations,
                revision,
                &transport,
                &identity,
                &changed,
                &authentication,
                3000
            )
            .await
            .is_err()
        );
        assert_eq!(
            operations
                .read(&identity.author_target_identity_digest, &identity.operation_identifier)
                .unwrap()
                .unwrap(),
            before
        );
        assert!(timeout(Duration::from_millis(20), listener.accept()).await.is_err());
        let retired = serde_json::json!({
            "kind": "retired", "provenance": submission.provenance,
            "agent_event_store_generation": 7,
            "agent_operation_identifier": submission.operation.agent_operation_identifier,
            "author_target_identity_digest": identity.author_target_identity_digest,
            "selected_environment_revision": identity.selected_environment_revision,
            "daemon_subscription_identifier": "subscription-one",
            "submitted_command_digest": submission.submitted_command_digest,
        })
        .to_string();
        for concurrent_change in [true, false] {
            let event_writer =
                AgentJobRepository::new(OperationDatabase::open(&path, settings()).unwrap());
            let peer = async {
                for stage in 0..2 {
                    let (mut socket, _) = listener.accept().await.unwrap();
                    let mut head = Vec::new();
                    while !head.ends_with(b"\r\n\r\n") {
                        head.push(socket.read_u8().await.unwrap());
                    }
                    let head = String::from_utf8(head).unwrap();
                    assert!(head.starts_with("GET /aem/bin/slingshot-agent/"));
                    let (status, body) =
                        if stage == 0 { (200, &capability) } else { (410, &retired) };
                    if stage == 1 && concurrent_change {
                        let child = event_writer
                            .read(
                                &identity.author_target_identity_digest,
                                &submission.operation.agent_operation_identifier,
                            )
                            .unwrap()
                            .unwrap();
                        event_writer
                            .record_snapshot_watermark(
                                &child.identity,
                                slingshot_domain::remote_job::JobEventSequence::of(4),
                            )
                            .unwrap();
                    }
                    socket.write_all(format!("HTTP/1.1 {status} OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n{body}", body.len()).as_bytes()).await.unwrap();
                }
            };
            let (result, ()) = timeout(Duration::from_secs(5), async {
                tokio::join!(
                    lookup_retained_operation(
                        &reopened,
                        &operations,
                        revision,
                        &transport,
                        &identity,
                        &submission,
                        &authentication,
                        3000
                    ),
                    peer
                )
            })
            .await
            .unwrap();
            assert_eq!(result.is_ok(), !concurrent_change);
            if concurrent_change {
                assert_eq!(
                    operations
                        .read(
                            &identity.author_target_identity_digest,
                            &identity.operation_identifier
                        )
                        .unwrap()
                        .unwrap(),
                    before,
                    "late tombstone settled changed remote evidence"
                );
            }
        }
        let final_record = operations
            .read(&identity.author_target_identity_digest, &identity.operation_identifier)
            .unwrap()
            .unwrap();
        let failure = final_record.record.terminal_failure.as_ref().unwrap();
        use slingshot_domain::operation::{TerminalFailureDisposition, TerminalFailureKind};
        assert_eq!(
            failure.kind,
            if truncate {
                TerminalFailureKind::ResultUnavailable
            } else {
                TerminalFailureKind::RecoveryWindowExpired
            }
        );
        assert_eq!(
            failure.disposition,
            if truncate {
                TerminalFailureDisposition::AuthoritativeRemoteSuccess
            } else {
                TerminalFailureDisposition::FailClosedIndeterminate {
                    certainty: OperationExecutionCertainty::RemoteOutcomeUnknown,
                }
            }
        );
        assert!(final_record.record.lifecycle_state.is_terminal());
        assert!(
            lookup_retained_operation(
                &reopened,
                &operations,
                final_record.record.revision,
                &transport,
                &identity,
                &submission,
                &authentication,
                4000
            )
            .await
            .is_err()
        );
        assert!(timeout(Duration::from_millis(20), listener.accept()).await.is_err());
        assert!(
            !reopened
                .removable_submissions(&identity.author_target_identity_digest, 3000, 100)
                .unwrap()
                .iter()
                .any(|(identifier, _)| identifier
                    == &submission.operation.agent_operation_identifier),
            "recent local settlement must retain its child"
        );
        assert!(
            reopened
                .removable_submissions(&identity.author_target_identity_digest, 10000, 100)
                .unwrap()
                .iter()
                .any(|(identifier, _)| identifier
                    == &submission.operation.agent_operation_identifier)
        );
        if truncate {
            let reviewed = slingshot_storage::maintenance::preview(
                reopened.database(),
                &identity.author_target_identity_digest,
                10000,
                100,
            )
            .unwrap();
            assert!(
                reviewed.agent_removals.iter().any(|removal| removal.agent_operation_identifier
                    == submission.operation.agent_operation_identifier)
            );
            slingshot_storage::maintenance::apply(reopened.database(), &reviewed, 10000).unwrap();
            assert!(
                reopened
                    .read(
                        &identity.author_target_identity_digest,
                        &submission.operation.agent_operation_identifier
                    )
                    .unwrap()
                    .is_none()
            );
            assert!(
                operations
                    .read(&identity.author_target_identity_digest, &identity.operation_identifier)
                    .unwrap()
                    .is_none()
            );
            continue;
        }
        reopened
            .remove_ended(
                &identity.author_target_identity_digest,
                &submission.operation.agent_operation_identifier,
            )
            .unwrap();
        assert!(
            reopened
                .read(
                    &identity.author_target_identity_digest,
                    &submission.operation.agent_operation_identifier
                )
                .unwrap()
                .is_none()
        );
        assert_eq!(
            operations
                .read(&identity.author_target_identity_digest, &identity.operation_identifier)
                .unwrap()
                .unwrap(),
            final_record
        );
    }
}
