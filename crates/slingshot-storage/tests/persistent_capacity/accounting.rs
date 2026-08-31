//! Counting real rows, at the bound and one past it.
//!
//! Every count comes from the table that is authoritative for it, so these also
//! prove the thing a counter could not: reopening changes nothing, because
//! there was never a second copy of the truth to fall out of step.

use slingshot_domain::command_fingerprint::{CommandFingerprint, FingerprintInput};
use slingshot_domain::installation::InstallationIdentifier;
use slingshot_domain::operation::{
    OperationFact, RecoveryCategory, RecoveryExecutionEvidence, RecoveryFact,
};
use slingshot_domain::persistent_capacity::{CapacityRefusal, PersistentCapacityPolicy};
use slingshot_storage::database::OperationDatabase;
use slingshot_storage::operation_repository::{
    AdmissionOutcome, AdmissionRequest, OperationRepository, RepositoryFailure,
};
use slingshot_storage::persistent_capacity::AccountingFailure;

use crate::fixtures::*;

/// Operation rows the small fixture policy allows.
const SMALL_OPERATION_ROWS: u64 = 3;

/// Bytes the small fixture policy allows committed and reserved together.
const SMALL_ARTIFACT_BYTES: u64 = 1024;

/// Bytes one artifact may occupy under the small fixture policy.
const SMALL_INDIVIDUAL_BYTES: u64 = 512;

/// Maintenance-application receipts the small fixture policy allows.
const SMALL_MAINTENANCE_RECEIPTS: u64 = 2;

/// Maintenance-result associations the small fixture policy allows.
const SMALL_MAINTENANCE_ASSOCIATIONS: u64 = 5;

/// Resume receipts one operation may hold under the small fixture policy.
const SMALL_RESUME_RECEIPTS: u64 = 2;

/// Contenders the reservation test runs.
const CONTENDERS: usize = 8;

/// Characters a sixty-four-character hexadecimal value has.
const DIGEST_CHARACTERS: usize = 64;

/// The divisor the aggregate test halves a budget by.
const HALF: u64 = 2;

/// Returns a policy small enough that a test can actually reach its bounds.
fn small_policy() -> PersistentCapacityPolicy {
    PersistentCapacityPolicy {
        committed_plus_reserved_artifact_bytes: SMALL_ARTIFACT_BYTES,
        individual_artifact_bytes: SMALL_INDIVIDUAL_BYTES,
        maintenance_application_receipts_per_target: SMALL_MAINTENANCE_RECEIPTS,
        maintenance_result_associations_per_target: SMALL_MAINTENANCE_ASSOCIATIONS,
        retained_operation_rows: SMALL_OPERATION_ROWS,
        recovery_resume_receipts_per_operation: SMALL_RESUME_RECEIPTS,
    }
}

/// Returns one admission request for the fixture partition.
fn admission(digest: &str, operation: &str) -> AdmissionRequest {
    let canonical = format!("{{\"paths\":[\"/{operation}\"]}}");
    AdmissionRequest {
        author_target_identity: format!("opaque-identity-behind-{digest}"),
        author_target_identity_digest: digest.to_owned(),
        caller_identity: None,
        canonical_command: canonical.clone(),
        command_fingerprint: CommandFingerprint::derive(&FingerprintInput {
            author_target_identity_digest: digest.to_owned(),
            canonical_command: canonical,
            command_wire_name: "query_paths".to_owned(),
            command_semantic_contract_version: "1".to_owned(),
            selected_environment_revision: "revision-1".to_owned(),
        })
        .expect("a derivable fingerprint"),
        command_wire_name: "query_paths".to_owned(),
        daemon_runtime_contract_digest: "c".repeat(DIGEST_CHARACTERS),
        installation_identifier: InstallationIdentifier::parse(&"a1".repeat(DIGEST_PAIRS))
            .expect("a legal identifier"),
        operation_identifier: operation.to_owned(),
        selected_environment_revision: "revision-1".to_owned(),
        workflow_correlation_identifier: None,
    }
}

/// Writes one operation row straight into the table this counts.
fn seed_operation(database: &slingshot_storage::database::OperationDatabase, index: usize) {
    database
        .connection()
        .execute(
            "INSERT INTO operation \
             (author_target_identity, author_target_identity_digest, canonical_command, \
              command_fingerprint, command_wire_name, daemon_runtime_contract_digest, \
              enqueue_sequence, installation_identifier, lifecycle_state, \
              operation_identifier, operation_revision, recorded_at_unix_milliseconds, \
              selected_environment_revision) \
             VALUES ('opaque', ?, '{}', 'f', 'query_paths', 'd', ?, 'i', 'queued', ?, 1, 0, 'r')",
            rusqlite::params![
                partition(FIRST_PRINCIPAL),
                i64::try_from(index).expect("a countable index"),
                format!("operation-{index}")
            ],
        )
        .expect("a seeded operation");
}

/// Writes one blob straight into the table whose lengths this sums.
fn seed_blob(
    database: &slingshot_storage::database::OperationDatabase,
    digest: &str,
    byte_length: u64,
) {
    database
        .connection()
        .execute(
            "INSERT INTO artifact_blob (byte_length, content_digest, \
             recorded_at_unix_milliseconds) VALUES (?, ?, ?)",
            rusqlite::params![
                i64::try_from(byte_length).expect("a countable length"),
                digest,
                i64::try_from(NOW).expect("a countable instant")
            ],
        )
        .expect("a seeded blob");
}

/// Writes one resume receipt straight into the table this counts.
fn seed_resume_receipt(database: &slingshot_storage::database::OperationDatabase, index: u64) {
    database
        .connection()
        .execute(
            "INSERT INTO recovery_resume_receipt \
             (applied_operation_revision, author_target_identity_digest, operation_identifier, \
              recorded_at_unix_milliseconds, selected_environment_revision, source_fingerprint) \
             VALUES (1, ?, 'operation-0', 0, 'revision-1', ?)",
            rusqlite::params![partition(FIRST_PRINCIPAL), format!("source-{index}")],
        )
        .expect("a seeded receipt");
}

#[test]
fn an_empty_namespace_is_holding_nothing() {
    let held = database();
    let account = account(&held, small_policy());
    let usage = account.usage().expect("a usage");
    assert_eq!(usage.operation_rows, 0);
    assert_eq!(usage.committed_artifact_bytes, 0);
    assert_eq!(usage.reserved_artifact_bytes, 0);
}

#[test]
fn the_row_at_the_bound_fits_and_the_one_past_it_refuses() {
    let held = database();
    let account = account(&held, small_policy());
    for index in 0..usize::try_from(SMALL_OPERATION_ROWS).expect("a countable bound") {
        account.require_room_for_operation().expect("room below the bound");
        seed_operation(&held, index);
    }
    assert_eq!(
        account.usage().expect("a usage").operation_rows,
        SMALL_OPERATION_ROWS,
        "the namespace is exactly at its bound"
    );

    let refused = account.require_room_for_operation();
    assert!(
        matches!(
            refused,
            Err(AccountingFailure::Refused(CapacityRefusal::OperationRows { facts }))
                if facts.held == SMALL_OPERATION_ROWS && facts.limit == SMALL_OPERATION_ROWS
        ),
        "one more is refused, with what is held and what the bound is: {refused:?}"
    );
    assert_eq!(
        account.usage().expect("a usage").operation_rows,
        SMALL_OPERATION_ROWS,
        "and the refusal deleted nothing to make room"
    );
}

#[test]
fn a_refusal_says_which_maintenance_would_release_something() {
    let held = database();
    let account = account(&held, small_policy());
    for index in 0..usize::try_from(SMALL_OPERATION_ROWS).expect("a countable bound") {
        seed_operation(&held, index);
    }
    let refused = account.require_room_for_operation().expect_err("a refusal");
    let sentence = refused.to_string();
    assert!(
        sentence.contains("terminal-operation maintenance"),
        "a caller told only that it is stuck has not been told what to do: {sentence}"
    );
}

#[test]
fn one_artifact_past_the_individual_bound_reserves_nothing() {
    let held = database();
    let account = account(&held, small_policy());
    account
        .reserve_artifact(None, SMALL_INDIVIDUAL_BYTES)
        .expect("the largest artifact this policy allows");
    let refused = account.reserve_artifact(None, SMALL_INDIVIDUAL_BYTES + 1);
    assert!(
        matches!(
            refused,
            Err(AccountingFailure::Refused(CapacityRefusal::ArtifactTooLarge { limit, .. }))
                if limit == SMALL_INDIVIDUAL_BYTES
        ),
        "one byte further is refused before anything is held: {refused:?}"
    );
}

#[test]
fn committed_and_reserved_bytes_count_against_one_bound_together() {
    let held = database();
    let account = account(&held, small_policy());
    seed_blob(&held, &"a".repeat(DIGEST_CHARACTERS), SMALL_ARTIFACT_BYTES / HALF);
    assert_eq!(
        account.usage().expect("a usage").committed_artifact_bytes,
        SMALL_ARTIFACT_BYTES / HALF,
        "committed content is counted from the rows that hold it"
    );

    let reservation = account
        .reserve_artifact(None, SMALL_ARTIFACT_BYTES / HALF)
        .expect("the rest of the budget")
        .expect("a reservation");
    assert_eq!(
        account.usage().expect("a usage").reserved_artifact_bytes,
        SMALL_ARTIFACT_BYTES / HALF,
        "and a reservation holds the rest"
    );

    let refused = account.reserve_artifact(None, 1);
    assert!(
        matches!(
            refused,
            Err(AccountingFailure::Refused(CapacityRefusal::ArtifactBytes { facts }))
                if facts.held == SMALL_ARTIFACT_BYTES
        ),
        "one further byte crosses the aggregate: {refused:?}"
    );

    account.release(reservation);
    account
        .reserve_artifact(None, SMALL_ARTIFACT_BYTES / HALF)
        .expect("an abandoned reservation holds nothing afterwards");
}

#[test]
fn committing_a_reservation_neither_double_counts_nor_loses_its_bytes() {
    let held = database();
    let account = account(&held, small_policy());
    let digest = "b".repeat(DIGEST_CHARACTERS);
    let reservation = account
        .reserve_artifact(Some(&digest), SMALL_INDIVIDUAL_BYTES)
        .expect("a reservation")
        .expect("content nothing has committed yet");

    seed_blob(&held, &digest, SMALL_INDIVIDUAL_BYTES);
    account.commit(reservation);
    let usage = account.usage().expect("a usage");
    assert_eq!(
        usage.committed_artifact_bytes, SMALL_INDIVIDUAL_BYTES,
        "the bytes are now counted by the table that holds them"
    );
    assert_eq!(usage.reserved_artifact_bytes, 0, "and by nothing else");
}

#[test]
fn content_already_committed_reserves_no_second_allocation() {
    let held = database();
    let account = account(&held, small_policy());
    let digest = "c".repeat(DIGEST_CHARACTERS);
    seed_blob(&held, &digest, SMALL_INDIVIDUAL_BYTES);

    let duplicate = account
        .reserve_artifact(Some(&digest), SMALL_INDIVIDUAL_BYTES)
        .expect("a duplicate is not a refusal");
    assert!(duplicate.is_none(), "identical content is already being counted, so nothing is held");
    assert_eq!(
        account.usage().expect("a usage").reserved_artifact_bytes,
        0,
        "and the aggregate did not move"
    );
}

#[test]
fn reopening_reconstructs_every_count_and_no_reservation() {
    let directory = tempfile::tempdir().expect("a directory");
    let path = directory.path().join("operations.sqlite3");
    let digest = "d".repeat(DIGEST_CHARACTERS);
    {
        let held = slingshot_storage::database::OperationDatabase::open(&path, settings())
            .expect("a database");
        let account = account(&held, small_policy());
        seed_operation(&held, 0);
        seed_blob(&held, &digest, SMALL_INDIVIDUAL_BYTES);
        account.reserve_artifact(None, SMALL_INDIVIDUAL_BYTES).expect("a reservation in progress");
    }

    let held = slingshot_storage::database::OperationDatabase::open(&path, settings())
        .expect("a reopened database");
    let account = account(&held, small_policy());
    let usage = account.usage().expect("a usage");
    assert_eq!(usage.operation_rows, 1, "committed rows come back");
    assert_eq!(usage.committed_artifact_bytes, SMALL_INDIVIDUAL_BYTES, "and committed bytes do");
    assert_eq!(
        usage.reserved_artifact_bytes, 0,
        "while an installation that a restart interrupted is not in progress any more"
    );
}

#[test]
fn the_receipt_and_association_bounds_refuse_at_their_own_counts() {
    let held = database();
    let account = account(&held, small_policy());
    let digest = partition(FIRST_PRINCIPAL);
    seed_operation(&held, 0);

    let receipts = small_policy().recovery_resume_receipts_per_operation;
    for index in 0..receipts {
        account
            .require_room_for_resume_receipt(&digest, "operation-0")
            .expect("room below the bound");
        seed_resume_receipt(&held, index);
    }
    let refused = account.require_room_for_resume_receipt(&digest, "operation-0");
    assert!(
        matches!(
            refused,
            Err(AccountingFailure::Refused(CapacityRefusal::ResumeReceipts { facts }))
                if facts.held == receipts
        ),
        "one receipt past the bound is refused: {refused:?}"
    );

    let associations = small_policy().maintenance_result_associations_per_target;
    let wanted = associations + 1;
    let refused = account.require_room_for_maintenance_associations(&digest, wanted);
    assert!(
        matches!(
            refused,
            Err(AccountingFailure::Refused(CapacityRefusal::MaintenanceAssociations { facts }))
                if facts.wanted == wanted && facts.held == 0
        ),
        "a request for more associations than a target may hold is refused whole: {refused:?}"
    );
    account
        .require_room_for_maintenance_associations(&digest, associations)
        .expect("and exactly the bound fits");
}

#[test]
fn contending_reservations_cannot_overcommit_the_aggregate() {
    let held = database();
    let account = account(&held, small_policy());
    let each = SMALL_INDIVIDUAL_BYTES;

    let taken: Vec<bool> =
        (0..CONTENDERS).map(|_| account.reserve_artifact(None, each).is_ok()).collect();
    let granted = taken.iter().filter(|won| **won).count();
    let allowed = usize::try_from(SMALL_ARTIFACT_BYTES / each).expect("a countable bound");
    assert_eq!(granted, allowed, "exactly as many succeeded as the aggregate has room for");
    assert_eq!(
        account.usage().expect("a usage").reserved_artifact_bytes,
        SMALL_ARTIFACT_BYTES,
        "and the total reached the bound without crossing it"
    );
    assert!(
        taken[allowed..].iter().all(|won| !*won),
        "every contender after the bound was refused rather than admitted and rolled back"
    );
}

#[test]
fn admission_refuses_at_the_bound_and_a_replay_still_gets_its_row() {
    let directory = tempfile::tempdir().expect("a directory");
    let path = directory.path().join("operations.sqlite3");
    let digest = partition(FIRST_PRINCIPAL);
    let repository = OperationRepository::bounded(
        OperationDatabase::open(&path, settings()).expect("a database"),
        small_policy(),
    );

    let mut admitted = Vec::new();
    for index in 0..SMALL_OPERATION_ROWS {
        let asked = admission(&digest, &format!("operation-{index}"));
        let outcome = repository.admit(&asked, NOW).expect("room below the bound");
        assert!(matches!(outcome, AdmissionOutcome::Admitted(_)), "each one admits");
        admitted.push(asked);
    }

    let over = admission(&digest, "one-too-many");
    let refused = repository.admit(&over, NOW);
    assert!(
        matches!(
            refused,
            Err(RepositoryFailure::Capacity(AccountingFailure::Refused(
                CapacityRefusal::OperationRows { .. }
            )))
        ),
        "the namespace is full and says so: {refused:?}"
    );
    assert!(
        repository.read(&digest, "one-too-many").expect("a read").is_none(),
        "and created no row on the way to refusing"
    );

    let replay = repository.admit(&admitted[0], NOW + 1).expect("a replay at the bound");
    assert!(
        matches!(replay, AdmissionOutcome::Replayed(_)),
        "an exact repeat still finds its row, because it consumes no capacity: {replay:?}"
    );
}

#[test]
fn a_namespace_at_its_bound_keeps_every_row_it_has() {
    let directory = tempfile::tempdir().expect("a directory");
    let path = directory.path().join("operations.sqlite3");
    let digest = partition(FIRST_PRINCIPAL);
    let repository = OperationRepository::bounded(
        OperationDatabase::open(&path, settings()).expect("a database"),
        small_policy(),
    );
    for index in 0..SMALL_OPERATION_ROWS {
        repository
            .admit(&admission(&digest, &format!("operation-{index}")), NOW)
            .expect("room below the bound");
    }
    repository.admit(&admission(&digest, "one-too-many"), NOW).expect_err("a refusal");

    let reconstructed = repository.reconstruct(&digest).expect("a reconstruction");
    assert_eq!(
        reconstructed.len(),
        usize::try_from(SMALL_OPERATION_ROWS).expect("a countable bound"),
        "reaching a bound deleted nothing to make room"
    );
    assert!(reconstructed.iter().all(|row| row.record.revision == 1), "and rewrote nothing either");
}

#[test]
fn capacity_refusal_leaves_proven_remote_work_nonterminal_and_unpublished() {
    let directory = tempfile::tempdir().expect("a directory");
    let path = directory.path().join("operations.sqlite3");
    let digest = partition(FIRST_PRINCIPAL);
    let repository = OperationRepository::bounded(
        OperationDatabase::open(&path, settings()).expect("a database"),
        small_policy(),
    );
    let asked = admission(&digest, "operation-0");
    let admitted = repository.admit(&asked, NOW).expect("an admission");
    let fingerprint = admitted.summary().command_fingerprint.clone();
    let revision = admitted.summary().record.revision;

    // The remote provably succeeded, and the result does not fit. The account
    // opens its own connection to the same file, the way a collaborator of the
    // repository does rather than a part of it.
    let counted = OperationDatabase::open(&path, settings()).expect("a second connection");
    let account = account(&counted, small_policy());
    let refused = account.reserve_artifact(None, SMALL_ARTIFACT_BYTES + 1);
    assert!(
        matches!(
            refused,
            Err(AccountingFailure::Refused(CapacityRefusal::ArtifactTooLarge { .. }))
        ),
        "the result cannot be stored: {refused:?}"
    );

    let waiting = repository
        .apply(
            &digest,
            "operation-0",
            revision,
            &OperationFact::Recovery {
                recovery: RecoveryFact {
                    attempt_count: 1,
                    category: RecoveryCategory::PersistentCapacityUnavailable,
                    detail: "the result does not fit".to_owned(),
                    evidence: RecoveryExecutionEvidence::AuthoritativeRemoteSuccess,
                    manual_resume_eligible: true,
                    retry_delay_milliseconds: 0,
                    retry_observed_at_unix_milliseconds: NOW,
                },
            },
            NOW,
        )
        .expect("the operation stays nonterminal, waiting on capacity");

    assert!(
        !waiting.record.lifecycle_state.is_terminal(),
        "work the remote did is not finished work, but it is not failed work either"
    );
    let outstanding = waiting.record.outstanding_recovery.as_ref().expect("a recovery fact");
    assert_eq!(outstanding.category, RecoveryCategory::PersistentCapacityUnavailable);
    assert_eq!(
        outstanding.evidence,
        RecoveryExecutionEvidence::AuthoritativeRemoteSuccess,
        "and the record says the remote succeeded rather than inventing a doubt about it"
    );
    assert!(outstanding.manual_resume_eligible, "so a person can retry the local half");
    assert!(waiting.record.terminal_failure.is_none(), "nothing was settled");
    assert_eq!(waiting.result_disposition, None, "and no result slot was published");
    assert_eq!(
        waiting.command_fingerprint, fingerprint,
        "with the command's identity untouched, so a resume repeats no remote work"
    );
}
