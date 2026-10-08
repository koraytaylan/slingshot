//! Retry timing through the production SQLite selection and claim transaction.
//!
//! Wall samples and monotonic readings move independently. No sleep determines
//! eligibility, and every boundary is checked immediately before and at expiry.

use super::{Observation, RetrySchedule};
use slingshot_domain::daemon_runtime_contract::DaemonRuntimeContract;
use slingshot_storage::database::{OperationDatabase, RequiredSettings};
use slingshot_storage::operation::scheduler_claim::{SelectedClaim, claim_next_queued_with};
use slingshot_storage::sqlite_statement_inventory::STATEMENTS;
use std::time::Duration;
use tokio::time::Instant;

/// Original persisted clock sample.
const OBSERVED: u64 = 100_000;
/// Chosen backoff duration.
const DELAY: u64 = 5_000;
/// Forward wall-clock correction deliberately larger than the entire backoff.
const FORWARD: u64 = 200_000;
/// Independently selected worker fence.
const FENCE: u64 = 7;
/// Canonical target identity used only by this fixture.
const TARGET: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";

/// Applies the same SQLite bounds as production to the isolated durable fixture.
fn settings() -> RequiredSettings {
    let contract = DaemonRuntimeContract::embedded();
    RequiredSettings {
        page_bytes: contract.limit("sqlite_page_bytes"),
        database_pages: contract.limit("maximum_sqlite_database_pages"),
        busy_timeout_milliseconds: contract.limit("database_busy_timeout_milliseconds"),
    }
}

/// Returns one inventoried production statement.
fn statement(purpose: &str) -> &'static str {
    STATEMENTS.iter().find(|statement| statement.purpose == purpose).unwrap().text
}

/// A durable queued retry plus ownership of its temporary directory.
struct Fixture {
    /// The real database passed to the production claim transaction.
    database: OperationDatabase,
    /// Keeps the database's parent alive until after its connection closes.
    _directory: tempfile::TempDir,
}

impl Fixture {
    /// Creates a retry with a durable wall observation and delay.
    fn new() -> Self {
        let directory = tempfile::tempdir().unwrap();
        let database =
            OperationDatabase::open(&directory.path().join("retry.sqlite"), settings()).unwrap();
        let connection = rusqlite::Connection::open(directory.path().join("retry.sqlite")).unwrap();
        let observed = i64::try_from(OBSERVED).unwrap();
        let delay = i64::try_from(DELAY).unwrap();
        connection
            .execute(
                statement("admit one operation"),
                rusqlite::params![
                    "identity",
                    TARGET,
                    Option::<String>::None,
                    "{}",
                    TARGET,
                    "query_paths",
                    TARGET,
                    1,
                    TARGET,
                    "queued",
                    "retry",
                    1,
                    observed,
                    TARGET,
                    Option::<String>::None,
                ],
            )
            .unwrap();
        connection
            .execute(
                statement("record the one recovery fact an operation is waiting on"),
                rusqlite::params![
                    1,
                    TARGET,
                    "operation_lookup",
                    "pending",
                    "remote_outcome_unknown",
                    "execution_certainty",
                    0,
                    "retry",
                    delay,
                    observed,
                ],
            )
            .unwrap();
        Self { database, _directory: directory }
    }

    /// Uses exactly the selector the live runtime supplies to the claim transaction.
    fn claim(
        &self,
        schedule: &mut RetrySchedule,
        wall: u64,
        monotonic: Instant,
    ) -> Option<SelectedClaim> {
        claim_next_queued_with(
            &self.database,
            TARGET,
            FENCE,
            wall.saturating_add(DELAY),
            wall,
            |candidates| schedule.select(candidates, wall, monotonic),
        )
        .unwrap()
    }
}

#[test]
fn a_backward_wall_correction_does_not_extend_the_live_retry_deadline() {
    let fixture = Fixture::new();
    let mut schedule = RetrySchedule::default();
    let started = Instant::now();
    assert!(fixture.claim(&mut schedule, OBSERVED, started).is_none());
    assert!(fixture.claim(&mut schedule, 0, started + Duration::from_millis(DELAY - 1)).is_none());
    let claim = fixture.claim(&mut schedule, 0, started + Duration::from_millis(DELAY)).unwrap();
    assert_eq!(claim.operation_identifier, "retry");
    assert!(
        fixture.claim(&mut schedule, 0, started + Duration::from_millis(DELAY)).is_none(),
        "the same operation cannot receive a second live claim"
    );
}

#[test]
fn a_forward_wall_correction_does_not_shorten_an_established_local_deadline() {
    let fixture = Fixture::new();
    let mut schedule = RetrySchedule::default();
    let started = Instant::now();
    assert!(fixture.claim(&mut schedule, OBSERVED, started).is_none());
    assert!(
        fixture.claim(&mut schedule, FORWARD, started + Duration::from_millis(DELAY - 1)).is_none()
    );
    assert!(
        fixture.claim(&mut schedule, FORWARD, started + Duration::from_millis(DELAY)).is_some()
    );
}

#[test]
fn restart_reconstruction_waits_at_most_the_original_delay_after_clock_rollback() {
    let fixture = Fixture::new();
    let started = Instant::now();
    let mut before_restart = RetrySchedule::default();
    assert!(fixture.claim(&mut before_restart, OBSERVED, started).is_none());
    drop(before_restart);
    let mut restarted = RetrySchedule::default();
    assert!(fixture.claim(&mut restarted, 0, started).is_none());
    assert!(fixture.claim(&mut restarted, 0, started + Duration::from_millis(DELAY - 1)).is_none());
    assert!(fixture.claim(&mut restarted, 0, started + Duration::from_millis(DELAY)).is_some());
}

#[test]
fn restart_reconstruction_credits_bounded_wall_evidence() {
    let fixture = Fixture::new();
    let mut restarted = RetrySchedule::default();
    assert!(fixture.claim(&mut restarted, FORWARD, Instant::now()).is_some());
}

#[test]
fn admitting_one_observation_never_waives_a_subsequent_retry_delay() {
    let started = Instant::now();
    let mut schedule = RetrySchedule::default();
    let first = Observation { observed: OBSERVED, delay: DELAY, attempt: 1 };
    schedule.admitted("retry", first, started);
    assert!(schedule.ready("retry", first, 0, started));
    let next = Observation { attempt: first.attempt + 1, ..first };
    assert!(!schedule.ready("retry", next, 0, started));
    assert!(!schedule.ready("retry", next, FORWARD, started + Duration::from_millis(DELAY - 1)));
    assert!(schedule.ready("retry", next, 0, started + Duration::from_millis(DELAY)));
    schedule.select(&[], 0, started);
    assert!(schedule.deadlines.is_empty(), "completed rows retain no timing history");
}

#[test]
fn durable_producer_turns_and_monotonic_retry_readiness_share_one_claim_path() {
    let fixture = Fixture::new();
    for (identifier, producer) in
        [("a-first", "a"), ("a-second", "a"), ("b-first", "b"), ("b-second", "b")]
    {
        fixture.add_ready(identifier, producer);
    }
    let started = Instant::now();
    let mut schedule = RetrySchedule::default();
    assert_eq!(
        fixture.claim(&mut schedule, OBSERVED, started).unwrap().operation_identifier,
        "a-first"
    );
    drop(schedule);
    let mut restarted = RetrySchedule::default();
    assert_eq!(
        fixture.claim(&mut restarted, OBSERVED, started).unwrap().operation_identifier,
        "b-first"
    );
    let elapsed = started + Duration::from_millis(DELAY);
    for expected in ["retry", "a-second", "b-second"] {
        assert_eq!(
            fixture.claim(&mut restarted, OBSERVED, elapsed).unwrap().operation_identifier,
            expected
        );
    }
}

impl Fixture {
    /// Uses the actual versioned decoder and capacity-checked admission path.
    fn add_ready(&self, identifier: &str, producer: &str) {
        use crate::operation_dispatch::BoundRequest;
        use crate::operation_submission::ServedTarget;
        use slingshot_domain::installation::InstallationIdentifier;
        use slingshot_domain::producer_identity::ProducerIdentity;
        use slingshot_local_protocol::foundation_contract::FoundationContract;
        use slingshot_local_protocol::message::OperationResponse;
        use slingshot_storage::operation_repository::OperationRepository;

        let contract = DaemonRuntimeContract::embedded();
        let digest = DaemonRuntimeContract::embedded_digest();
        let served = ServedTarget {
            author_target_identity_digest: TARGET.to_owned(),
            selected_environment_revision: TARGET.to_owned(),
            daemon_runtime_contract_digest: digest.as_text().to_owned(),
            execution_available: true,
        };
        let envelope = serde_json::json!({
            "operation_protocol_version": contract.operation_protocol_version,
            "author_target_identity_digest": TARGET,
            "selected_environment_revision": TARGET,
            "daemon_runtime_contract_digest": digest.as_text(),
            "request_identifier": identifier,
            "request": {
                "request": "execute",
                "command": {"command": "query_paths", "root_path": "/content/example"},
                "operation_identifier": identifier,
                "caller_identity": ProducerIdentity::from_label(producer).unwrap().as_text(),
            }
        });
        let request = BoundRequest::decode(
            &FoundationContract::embedded(),
            &served,
            &[contract.operation_protocol_version as u32],
            &serde_json::to_vec(&envelope).unwrap(),
        )
        .unwrap();
        let prepared = request
            .prepare_admission(&InstallationIdentifier::parse(TARGET).unwrap())
            .unwrap()
            .unwrap();
        let repository = OperationRepository::new(
            OperationDatabase::open(&self._directory.path().join("retry.sqlite"), settings())
                .unwrap(),
        );
        assert!(matches!(
            prepared
                .persist_scheduled(&repository, &std::collections::BTreeSet::new(), OBSERVED)
                .unwrap(),
            OperationResponse::Accepted { .. }
        ));
    }
}
