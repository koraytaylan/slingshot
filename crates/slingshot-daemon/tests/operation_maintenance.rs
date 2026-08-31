//! Removing work that has ended, and proving nothing else ever goes.
//!
//! The absolute rule under every test here is that unfinished work is never
//! selected, whatever the criteria and however old everything around it is.
//! The second rule is that a person applies what they read: the digest of the
//! reviewed manifest is quoted back, and a target that moved on in between
//! produces a refusal rather than a removal nobody reviewed.

use serde_json::Value;
use slingshot_daemon::operation_maintenance::{MaintenancePreview, PreviewRequest, apply, preview};
use slingshot_domain::command_fingerprint::{CommandFingerprint, FingerprintInput};
use slingshot_domain::installation::InstallationIdentifier;
use slingshot_domain::operation::{
    OperationFact, OperationLifecycleState, TerminalFailure, TerminalFailureDisposition,
    TerminalFailureKind,
};
use slingshot_storage::database::{OperationDatabase, RequiredSettings};
use slingshot_storage::maintenance::{ApplyOutcome, MaintenanceFailure, ReceiptStage};
use slingshot_storage::operation_repository::{
    AdmissionOutcome, AdmissionRequest, OperationRepository,
};

/// Selection fixtures this test reads.
const SELECTION: &str = include_str!("fixtures/operation-maintenance/selection.jsonl");

/// Bytes one page occupies, from the runtime contract.
const PAGE_BYTES: u64 = 4096;

/// Pages the database may reach, from the runtime contract.
const DATABASE_PAGES: u64 = 262_144;

/// Milliseconds a busy connection waits, from the runtime contract.
const BUSY_TIMEOUT: u64 = 5000;

/// Two-character pairs in a sixty-four-character hexadecimal value.
const DIGEST_PAIRS: usize = 32;

/// Characters a sixty-four-character hexadecimal value has.
const DIGEST_CHARACTERS: usize = 64;

/// The environment revision these fixtures are admitted under.
const REVISION: &str = "revision-1";

/// The first partition these fixtures use.
const FIRST_PRINCIPAL: &str = "1d";

/// A second partition, for proving maintenance cannot cross one.
const SECOND_PRINCIPAL: &str = "2d";

/// A cutoff far past every fixture's settlement instant.
const EVERYTHING: u64 = 1_000;

/// Operations one request asks for at most.
const ASKED_FOR: u64 = 64;

/// Operations the settled fixtures admit and end.
const SETTLED_OPERATIONS: u64 = 3;

/// Rows a partition holds once one of them is still running.
const WITH_ONE_RUNNING: usize = 4;

/// A cutoff that excludes the later of two settlements.
const BEFORE_THE_SECOND: u64 = 2;

/// Returns one row's string member.
fn text<'row>(row: &'row Value, member: &str) -> &'row str {
    row[member].as_str().unwrap_or_else(|| panic!("{member} is a string in {row}"))
}

/// Returns every case of the fixture.
fn cases() -> Vec<Value> {
    SELECTION
        .lines()
        .map(|line| serde_json::from_str(line).expect("every fixture line is one object"))
        .collect()
}

/// Returns the settings every connection is held to.
fn settings() -> RequiredSettings {
    RequiredSettings {
        page_bytes: PAGE_BYTES,
        database_pages: DATABASE_PAGES,
        busy_timeout_milliseconds: BUSY_TIMEOUT,
    }
}

/// Returns the digest one principal's author target has.
fn partition(principal: &str) -> String {
    principal.repeat(DIGEST_PAIRS)
}

/// Admits one operation and returns its revision.
fn admit(repository: &OperationRepository, digest: &str, identifier: &str) -> u64 {
    let canonical = format!("{{\"paths\":[\"/{identifier}\"]}}");
    let asked = AdmissionRequest {
        author_target_identity: format!("opaque-identity-behind-{digest}"),
        author_target_identity_digest: digest.to_owned(),
        caller_identity: None,
        canonical_command: canonical.clone(),
        command_fingerprint: CommandFingerprint::derive(&FingerprintInput {
            author_target_identity_digest: digest.to_owned(),
            canonical_command: canonical,
            command_wire_name: "query_paths".to_owned(),
            command_semantic_contract_version: "1".to_owned(),
            selected_environment_revision: REVISION.to_owned(),
        })
        .expect("a derivable fingerprint"),
        command_wire_name: "query_paths".to_owned(),
        daemon_runtime_contract_digest: "c".repeat(DIGEST_CHARACTERS),
        installation_identifier: InstallationIdentifier::parse(&"a1".repeat(DIGEST_PAIRS))
            .expect("a legal identifier"),
        operation_identifier: identifier.to_owned(),
        selected_environment_revision: REVISION.to_owned(),
        workflow_correlation_identifier: None,
    };
    let outcome = repository.admit(&asked, 0).expect("an admission");
    assert!(matches!(outcome, AdmissionOutcome::Admitted(_)), "each fixture row admits");
    outcome.summary().record.revision
}

/// Admits one operation and settles it at `settled`.
fn settle(repository: &OperationRepository, digest: &str, identifier: &str, settled: u64) {
    let revision = admit(repository, digest, identifier);
    repository
        .apply(
            digest,
            identifier,
            revision,
            &OperationFact::Terminal {
                failure: TerminalFailure {
                    disposition: TerminalFailureDisposition::AuthoritativeRemoteFailure,
                    kind: TerminalFailureKind::RemoteFailed,
                    metadata: None,
                },
            },
            settled,
        )
        .expect("a settlement");
}

/// Admits one operation and leaves it running.
fn leave_running(repository: &OperationRepository, digest: &str, identifier: &str) {
    let mut revision = admit(repository, digest, identifier);
    for reached in [
        OperationLifecycleState::Submitting,
        OperationLifecycleState::Accepted,
        OperationLifecycleState::Running,
    ] {
        revision = repository
            .apply(
                digest,
                identifier,
                revision,
                &OperationFact::Lifecycle { lifecycle_state: reached },
                0,
            )
            .expect("a legal advance")
            .record
            .revision;
    }
}

/// Returns one preview of everything before `cutoff`.
fn previewed(database: &OperationDatabase, digest: &str, cutoff: u64) -> MaintenancePreview {
    preview(
        database,
        &PreviewRequest {
            author_target_identity_digest: digest.to_owned(),
            before_unix_milliseconds: cutoff,
            limit: ASKED_FOR,
        },
    )
    .expect("a preview")
}

#[test]
fn every_selection_fixture_proposes_exactly_what_it_states() {
    for case in cases() {
        let repository = OperationRepository::new(
            OperationDatabase::open_in_memory(settings()).expect("a database"),
        );
        let digest = partition(FIRST_PRINCIPAL);
        for index in 1..=case["terminal"].as_u64().expect("a count") {
            settle(&repository, &digest, &format!("ended-{index}"), index);
        }
        for index in 1..=case["nonterminal"].as_u64().expect("a count") {
            leave_running(&repository, &digest, &format!("running-{index}"));
        }

        let manifest =
            previewed(repository.database(), &digest, case["cutoff"].as_u64().expect("a cutoff"));
        assert_eq!(
            manifest.manifest.removals.len(),
            usize::try_from(case["removed"].as_u64().expect("a count")).expect("a countable count"),
            "{}",
            text(&case, "note")
        );
        assert!(
            manifest
                .manifest
                .removals
                .iter()
                .all(|removal| { removal.operation_identifier.starts_with("ended-") }),
            "{}: unfinished work is never proposed",
            text(&case, "note")
        );
    }
}

#[test]
fn a_preview_changes_nothing_and_applying_it_removes_exactly_what_it_said() {
    let repository = OperationRepository::new(
        OperationDatabase::open_in_memory(settings()).expect("a database"),
    );
    let digest = partition(FIRST_PRINCIPAL);
    for index in 1..=SETTLED_OPERATIONS {
        settle(&repository, &digest, &format!("ended-{index}"), index);
    }
    leave_running(&repository, &digest, "running-1");

    let reviewed = previewed(repository.database(), &digest, EVERYTHING);
    assert_eq!(
        reviewed.manifest.removals.len(),
        usize::try_from(SETTLED_OPERATIONS).expect("a countable count"),
        "three operations have ended"
    );
    assert_eq!(
        repository.reconstruct(&digest).expect("a reconstruction").len(),
        WITH_ONE_RUNNING,
        "and previewing removed none of them"
    );

    let applied = apply(repository.database(), &reviewed, EVERYTHING).expect("an apply");
    let ApplyOutcome::Applied(receipt) = applied else { panic!("a fresh manifest applies") };
    assert_eq!(receipt.released_operation_rows, SETTLED_OPERATIONS);
    assert_eq!(receipt.stage, ReceiptStage::DatabaseApplied);
    assert_eq!(receipt.application_receipt_identifier, reviewed.digest, "keyed by what was read");

    let remaining = repository.reconstruct(&digest).expect("a reconstruction");
    assert_eq!(remaining.len(), 1, "exactly the ended operations went");
    assert_eq!(
        remaining[0].operation_identifier, "running-1",
        "and the one still running is untouched"
    );
}

#[test]
fn applying_the_same_manifest_twice_removes_nothing_the_second_time() {
    let repository = OperationRepository::new(
        OperationDatabase::open_in_memory(settings()).expect("a database"),
    );
    let digest = partition(FIRST_PRINCIPAL);
    settle(&repository, &digest, "ended-1", 1);
    let reviewed = previewed(repository.database(), &digest, EVERYTHING);

    let first = apply(repository.database(), &reviewed, EVERYTHING).expect("an apply");
    assert!(matches!(first, ApplyOutcome::Applied(_)));
    let again = apply(repository.database(), &reviewed, EVERYTHING).expect("a repeat");
    let ApplyOutcome::Replayed(receipt) = again else {
        panic!("an exact repeat replays rather than reapplying: {again:?}");
    };
    assert_eq!(
        receipt.application_receipt_identifier, reviewed.digest,
        "and hands back the receipt the first one committed"
    );
    assert!(repository.reconstruct(&digest).expect("a reconstruction").is_empty());
}

#[test]
fn a_target_that_moved_on_refuses_rather_than_removing_what_nobody_reviewed() {
    let repository = OperationRepository::new(
        OperationDatabase::open_in_memory(settings()).expect("a database"),
    );
    let digest = partition(FIRST_PRINCIPAL);
    settle(&repository, &digest, "ended-1", 1);
    let reviewed = previewed(repository.database(), &digest, EVERYTHING);

    settle(&repository, &digest, "ended-2", BEFORE_THE_SECOND);
    let refused = apply(repository.database(), &reviewed, EVERYTHING);
    assert!(
        matches!(refused, Err(MaintenanceFailure::ManifestChanged { .. })),
        "a manifest that no longer describes the target is refused whole: {refused:?}"
    );
    assert_eq!(
        repository.reconstruct(&digest).expect("a reconstruction").len(),
        2,
        "and nothing was removed on the way to refusing"
    );
}

#[test]
fn maintenance_never_reaches_another_partition() {
    let repository = OperationRepository::new(
        OperationDatabase::open_in_memory(settings()).expect("a database"),
    );
    let here = partition(FIRST_PRINCIPAL);
    let elsewhere = partition(SECOND_PRINCIPAL);
    settle(&repository, &here, "ended-1", 1);
    settle(&repository, &elsewhere, "ended-1", 1);

    let reviewed = previewed(repository.database(), &here, EVERYTHING);
    assert_eq!(reviewed.manifest.removals.len(), 1, "one partition's preview sees its own rows");
    apply(repository.database(), &reviewed, EVERYTHING).expect("an apply");

    assert!(repository.reconstruct(&here).expect("a reconstruction").is_empty());
    assert_eq!(
        repository.reconstruct(&elsewhere).expect("a reconstruction").len(),
        1,
        "and the other partition's identically named row is still there"
    );
}

#[test]
fn two_previews_of_the_same_rows_digest_alike_and_of_different_rows_do_not() {
    let repository = OperationRepository::new(
        OperationDatabase::open_in_memory(settings()).expect("a database"),
    );
    let digest = partition(FIRST_PRINCIPAL);
    settle(&repository, &digest, "ended-1", 1);

    let once = previewed(repository.database(), &digest, EVERYTHING);
    let twice = previewed(repository.database(), &digest, EVERYTHING);
    assert_eq!(once.digest, twice.digest, "the same rows under the same cutoff read the same");

    settle(&repository, &digest, "ended-2", BEFORE_THE_SECOND);
    let wider = previewed(repository.database(), &digest, EVERYTHING);
    assert_ne!(once.digest, wider.digest, "and one more row is a different thing to apply");
    let narrower = previewed(repository.database(), &digest, BEFORE_THE_SECOND);
    assert_ne!(
        wider.digest, narrower.digest,
        "as is the same rows under a cutoff that excludes one"
    );
}

#[test]
fn a_preview_counts_what_approving_it_would_free() {
    let repository = OperationRepository::new(
        OperationDatabase::open_in_memory(settings()).expect("a database"),
    );
    let digest = partition(FIRST_PRINCIPAL);
    for index in 1..=SETTLED_OPERATIONS {
        settle(&repository, &digest, &format!("ended-{index}"), index);
    }
    let reviewed = previewed(repository.database(), &digest, EVERYTHING);
    let released = reviewed.released();
    assert_eq!(
        released.operation_rows,
        reviewed.manifest.released_operation_rows(),
        "the number a person reads before approving is the number the approval is about"
    );
    assert_eq!(
        released.agent_submission_rows,
        reviewed.manifest.released_agent_rows(),
        "a target with no remote work frees no remote rows"
    );
    assert_eq!(released.subscription_rows, 0, "and retires no shared subscription");
}
