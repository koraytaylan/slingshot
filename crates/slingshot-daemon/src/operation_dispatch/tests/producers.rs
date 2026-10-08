//! Real versioned admission enforces independent producer queues and immutable ownership.

use std::collections::BTreeSet;

use slingshot_domain::installation::InstallationIdentifier;
use slingshot_domain::producer_identity::ProducerIdentity;
use slingshot_storage::database::{OperationDatabase, RequiredSettings};

use super::{bind, execute_envelope, served};
use crate::operation_dispatch::PreparedAdmission;
use slingshot_domain::daemon_runtime_contract::DaemonRuntimeContract;
use slingshot_local_protocol::message::OperationResponse;
use slingshot_storage::operation_repository::OperationRepository;

const NOW: u64 = 1_000;

struct Fixture {
    root: tempfile::TempDir,
}

impl Fixture {
    fn new() -> Self {
        Self { root: tempfile::tempdir().unwrap() }
    }

    fn repository(&self) -> OperationRepository {
        let limits = DaemonRuntimeContract::embedded();
        OperationRepository::new(
            OperationDatabase::open(
                &self.root.path().join("admissions.sqlite"),
                RequiredSettings {
                    page_bytes: limits.limit("sqlite_page_bytes"),
                    database_pages: limits.limit("maximum_sqlite_database_pages"),
                    busy_timeout_milliseconds: limits.limit("database_busy_timeout_milliseconds"),
                },
            )
            .unwrap(),
        )
    }

    fn admit(&self, operation: &str, producer: Option<&str>) -> OperationResponse {
        prepare(operation, producer)
            .unwrap()
            .persist_scheduled(&self.repository(), &BTreeSet::new(), NOW)
            .unwrap()
    }
}

fn prepare(
    operation: &str,
    producer: Option<&str>,
) -> Result<PreparedAdmission, OperationResponse> {
    let mut value = execute_envelope();
    value["request"]["operation_identifier"] = operation.into();
    if let Some(producer) = producer {
        value["request"]["caller_identity"] = producer.into();
    }
    let installation =
        InstallationIdentifier::parse(&served().author_target_identity_digest).unwrap();
    bind(&value)?.prepare_admission(&installation).map(Option::unwrap)
}

#[test]
fn producer_capacity_is_isolated_and_global_capacity_remains_bounded_after_reopen() {
    let fixture = Fixture::new();
    let limits = DaemonRuntimeContract::embedded();
    let per_caller = limits.limit("maximum_pending_operations_per_caller");
    let global = limits.limit("maximum_global_pending_operations");
    assert_eq!(global % per_caller, 0);
    for producer in 0..global / per_caller {
        let identity = ProducerIdentity::from_label(&format!("producer-{producer}")).unwrap();
        for sequence in 0..per_caller {
            assert!(matches!(
                fixture.admit(&format!("{producer}-{sequence}"), Some(identity.as_text())),
                OperationResponse::Accepted { .. }
            ));
        }
        assert!(matches!(
            fixture.admit(&format!("overflow-{producer}"), Some(identity.as_text())),
            OperationResponse::SchedulerCapacityExhausted { .. }
        ));
    }
    let newcomer = ProducerIdentity::from_label("newcomer").unwrap();
    assert!(matches!(
        fixture.admit("global-overflow", Some(newcomer.as_text())),
        OperationResponse::SchedulerCapacityExhausted { .. }
    ));
    assert!(matches!(
        fixture.admit("default-overflow", None),
        OperationResponse::SchedulerCapacityExhausted { .. }
    ));
    let repository = fixture.repository();
    let before = repository.read(&served().author_target_identity_digest, "0-0").unwrap().unwrap();
    assert!(matches!(
        fixture.admit("0-0", Some(newcomer.as_text())),
        OperationResponse::Replayed { .. }
    ));
    assert_eq!(
        repository.read(&served().author_target_identity_digest, "0-0").unwrap().unwrap(),
        before
    );
}

#[test]
fn omitted_producers_share_capacity_and_replays_do_not_transfer_to_explicit_producers() {
    let fixture = Fixture::new();
    let limit = DaemonRuntimeContract::embedded().limit("maximum_pending_operations_per_caller");
    for sequence in 0..limit {
        assert!(matches!(
            fixture.admit(&format!("default-{sequence}"), None),
            OperationResponse::Accepted { .. }
        ));
    }
    assert!(matches!(
        fixture.admit("overflow", None),
        OperationResponse::SchedulerCapacityExhausted { .. }
    ));
    let identity = ProducerIdentity::from_label("build").unwrap();
    assert!(matches!(
        fixture.admit("explicit", Some(identity.as_text())),
        OperationResponse::Accepted { .. }
    ));
    assert!(matches!(
        fixture.admit("default-0", Some(identity.as_text())),
        OperationResponse::Replayed { .. }
    ));
    assert_eq!(
        fixture
            .repository()
            .read(&served().author_target_identity_digest, "default-0")
            .unwrap()
            .unwrap()
            .caller_identity,
        None
    );
}

#[test]
fn malformed_producer_identity_is_refused_before_repository_access_without_echo() {
    for identity in
        ["", "private-label", "AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA"]
    {
        let response = prepare("operation", Some(identity)).unwrap_err();
        assert!(matches!(response, OperationResponse::MalformedFrame { .. }));
        assert!(!serde_json::to_string(&response).unwrap().contains("private-label"));
    }
}
