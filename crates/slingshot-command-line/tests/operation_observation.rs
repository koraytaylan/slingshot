//! Reading an operation, and the two things a read may not turn into.
//!
//! Observation is a read, so nothing here changes an operation and a caller who
//! stops watching changes nothing about what is running. Two combinations would
//! break that, and both are refused: naming a historical partition for
//! something that acts, and naming one for work that has not settled. Waiting
//! and resuming are actions in all but name - one holds the caller against work
//! still moving, the other schedules more of it.
//!
//! A resume names both the revision and the category the caller observed.
//! Either alone would let a resume written against one state apply to whatever
//! the operation had since become, which is precisely what a person reviewing a
//! paused operation is trying to avoid. A receipt already consumed schedules
//! nothing whatever the operation has become since, including after it ended.
//!
//! The transfer side is about one moment: the rename that makes a destination
//! appear. Before it nothing is visible and the private state resumes; after it
//! the destination is whole and a rerun re-renders rather than fetching again.
//! Nothing overwrites, because a destination that already exists is a collision
//! and treating it as a target would destroy a file this command did not make.

use std::io::Write;

use slingshot_command_line::artifact_download::{
    DownloadRefusal, PriorWork, Transfer, prior_work, publish,
};
use slingshot_command_line::artifact_staging_lock::{
    LOCK_SUFFIX, LockRefusal, SIDECAR_SUFFIX, STAGING_SUFFIX, StagingLock, names_beside, stem,
};
use slingshot_command_line::artifact_staging_metadata::{
    ResumeRefusal, StagedPayload, StagingRecord, TransferState,
};
use slingshot_command_line::operation_observation::{
    Observation, ObservationRefusal, ObservationRequest, Partition, PausedOperation, ResumeOutcome,
    ResumePreconditions, apply_receipt, require_permitted, require_resumable,
};

/// Where the vectors this suite is driven from live.
const FIXTURES: &str = "tests/fixtures/operation-observation";

/// The partition this client serves.
const TARGET: &str = "target-identity-digest-one";

/// One it served before.
const HISTORICAL_TARGET: &str = "target-identity-digest-two";

/// The revision it serves under.
const REVISION: &str = "environment-revision-one";

/// The operation these fixtures read.
const OPERATION: &str = "operation-one";

/// One artifact of it.
const ARTIFACT: &str = "artifact-one";

/// One maintenance result, which belongs to no operation.
const MAINTENANCE_RESULT: &str = "maintenance-result-one";

/// What one artifact digests to.
const DIGEST: &str = "expected-digest";

/// What a different body digests to.
const OTHER_DIGEST: &str = "another-digest";

/// How long it is.
const LENGTH: u64 = 4096;

/// How much of one artifact has arrived when a transfer is interrupted.
const PARTIAL_LENGTH: u64 = 1024;

/// How much of it a resumed transfer picks up from.
const HALF_LENGTH: u64 = LENGTH / 2;

/// Half of what arrived, for a record that disagrees with the file.
const HALF_PARTIAL: u64 = PARTIAL_LENGTH / 2;

/// The runtime contract digest a request carries.
const RUNTIME_DIGEST: &str = "runtime-contract-digest";

/// The receipt a resume quotes.
const RECEIPT: &str = "resume-receipt-one";

/// Returns every vector one fixture holds.
fn vectors(name: &str) -> Vec<serde_json::Value> {
    let path = format!("{FIXTURES}/{name}");
    let text = std::fs::read_to_string(&path).unwrap_or_else(|_| panic!("{path} is readable"));
    text.lines().map(|line| serde_json::from_str(line).expect("each line is one vector")).collect()
}

/// Returns the observation `spelling` names.
fn observation_named(spelling: &str) -> Observation {
    match spelling {
        "status" => Observation::Status,
        "wait" => Observation::Wait,
        "result" => Observation::Result,
        "artifact" => Observation::Artifact,
        "resume" => Observation::Resume,
        other => panic!("{other} is an observation this suite does not name"),
    }
}

/// Returns the artifact payload these fixtures stage.
fn artifact_payload() -> StagedPayload {
    StagedPayload::OperationArtifact {
        artifact_identifier: ARTIFACT.to_owned(),
        operation_identifier: OPERATION.to_owned(),
    }
}

/// Returns one record in `state` with `verified` bytes on disk.
fn record(state: TransferState, verified: u64) -> StagingRecord {
    StagingRecord {
        author_target_identity_digest: TARGET.to_owned(),
        content_digest: DIGEST.to_owned(),
        payload: artifact_payload(),
        selected_environment_revision: REVISION.to_owned(),
        state,
        total_length: LENGTH,
        verified_length: verified,
    }
}

#[test]
fn a_historical_partition_may_be_read_and_never_acted_on() {
    for vector in vectors("partitions.jsonl") {
        let name = vector["name"].as_str().expect("a name");
        let observation = observation_named(vector["observation"].as_str().expect("one"));
        let partition = if vector["historical"].as_bool().expect("an expectation") {
            Partition::Historical { author_target_identity_digest: HISTORICAL_TARGET.to_owned() }
        } else {
            Partition::Current { author_target_identity_digest: TARGET.to_owned() }
        };
        let request = ObservationRequest {
            daemon_runtime_contract_digest: RUNTIME_DIGEST.to_owned(),
            observation,
            operation_identifier: OPERATION.to_owned(),
            partition,
        };
        let produced = require_permitted(&request, vector["ended"].as_bool().expect("one"));
        let spelling = match produced {
            Ok(()) => "permitted",
            Err(ObservationRefusal::HistoryNotActionable) => "history-not-actionable",
            Err(ObservationRefusal::HistoryNotSettled) => "history-not-settled",
            Err(other) => panic!("{name}: {other}"),
        };
        assert_eq!(spelling, vector["outcome"].as_str().expect("an outcome"), "{name}");
    }
}

#[test]
fn only_a_resume_changes_anything_and_only_reads_may_look_at_history() {
    for observation in
        [Observation::Status, Observation::Wait, Observation::Result, Observation::Artifact]
    {
        assert!(!observation.changes_anything(), "{observation:?} is a read");
    }
    assert!(Observation::Resume.changes_anything());
    assert!(!Observation::Wait.permits_history(), "waiting holds a caller against moving work");
    assert!(!Observation::Resume.permits_history(), "and resuming schedules more of it");
}

#[test]
fn a_resume_names_both_the_revision_and_the_category_that_was_observed() {
    for vector in vectors("resumes.jsonl") {
        let name = vector["name"].as_str().expect("a name");
        let held = PausedOperation {
            paused_category: vector["paused"].as_str().map(str::to_owned),
            revision: vector["revision"].as_u64().expect("a revision"),
        };
        let preconditions = ResumePreconditions {
            observed_category: vector["observed_category"].as_str().expect("a category").to_owned(),
            observed_revision: vector["observed_revision"].as_u64().expect("a revision"),
        };
        let spelling = match require_resumable(&preconditions, &held) {
            Ok(()) => "permitted",
            Err(ObservationRefusal::RevisionMoved { .. }) => "revision-moved",
            Err(ObservationRefusal::CategoryChanged) => "category-changed",
            Err(ObservationRefusal::NotPaused) => "not-paused",
            Err(other) => panic!("{name}: {other}"),
        };
        assert_eq!(spelling, vector["outcome"].as_str().expect("an outcome"), "{name}");
    }
}

#[test]
fn a_receipt_already_applied_schedules_nothing_however_the_operation_has_moved() {
    assert_eq!(apply_receipt(&[], RECEIPT), ResumeOutcome::Scheduled);
    assert_eq!(
        apply_receipt(&[RECEIPT.to_owned()], RECEIPT),
        ResumeOutcome::Replayed,
        "replaying a resume into a finished operation would schedule recovery for work that is \
         over"
    );
}

#[test]
fn the_three_staging_files_are_derived_and_sit_beside_the_destination() {
    let destination = std::path::Path::new("/tmp/downloads/package.zip");
    let names = names_beside(destination, TARGET, REVISION, &artifact_payload());
    for path in [&names.lock, &names.sidecar, &names.staging] {
        assert_eq!(
            path.parent(),
            destination.parent(),
            "the publication is a rename, and a rename across filesystems is not atomic"
        );
    }
    assert!(names.lock.to_string_lossy().ends_with(LOCK_SUFFIX));
    assert!(names.sidecar.to_string_lossy().ends_with(SIDECAR_SUFFIX));
    assert!(names.staging.to_string_lossy().ends_with(STAGING_SUFFIX));
    let same = names_beside(destination, TARGET, REVISION, &artifact_payload());
    assert_eq!(names, same, "two invocations of one fetch collide on purpose");
    let other = names_beside(
        destination,
        TARGET,
        REVISION,
        &StagedPayload::MaintenanceResult {
            maintenance_result_identifier: MAINTENANCE_RESULT.to_owned(),
        },
    );
    assert_ne!(names, other, "and two different fetches never collide by accident");
    assert_ne!(
        stem(TARGET, REVISION, &artifact_payload()),
        stem(HISTORICAL_TARGET, REVISION, &artifact_payload()),
        "the target is part of the name, so one destination serves two partitions safely"
    );
}

#[test]
fn one_process_stages_a_transfer_and_the_next_is_told_who_has_it() {
    let directory = tempfile::tempdir().expect("a temporary directory");
    let path = directory.path().join("one.slingshot-lock");
    let held = StagingLock::take(&path).expect("nobody holds it");
    assert!(
        matches!(StagingLock::take(&path), Err(LockRefusal::Held)),
        "waiting would make a second invocation one that eventually starts, which a caller \
         cannot tell from the first"
    );
    drop(held);
    StagingLock::take(&path).expect("and the released lock is available again");
}

#[test]
fn a_record_resumes_only_against_facts_the_bytes_on_disk_agree_with() {
    let held = record(TransferState::Transferring, PARTIAL_LENGTH);
    assert_eq!(
        held.require_resumable(&artifact_payload(), TARGET, REVISION, PARTIAL_LENGTH),
        Ok(TransferState::Transferring)
    );
    assert_eq!(
        held.require_resumable(&artifact_payload(), TARGET, REVISION, HALF_PARTIAL),
        Err(ResumeRefusal::LengthDisagrees { actual: HALF_PARTIAL, recorded: PARTIAL_LENGTH }),
        "a record that matched on identity and disagreed on length would resume from a position \
         the file never reached"
    );
    assert_eq!(
        held.require_resumable(
            &StagedPayload::MaintenanceResult {
                maintenance_result_identifier: MAINTENANCE_RESULT.to_owned()
            },
            TARGET,
            REVISION,
            PARTIAL_LENGTH
        ),
        Err(ResumeRefusal::AnotherPayload),
        "the two identity shapes are disjoint, and nothing defaults one from the other"
    );
    assert_eq!(
        held.require_resumable(&artifact_payload(), HISTORICAL_TARGET, REVISION, PARTIAL_LENGTH),
        Err(ResumeRefusal::AnotherTarget)
    );
}

#[test]
fn a_transfer_verifies_as_it_goes_and_publishes_only_when_both_facts_agree() {
    let mut transfer = Transfer::of(LENGTH, DIGEST);
    transfer.absorb(LENGTH).expect("exactly what was promised");
    assert!(transfer.is_complete());
    assert_eq!(
        transfer.absorb(1),
        Err(DownloadRefusal::LengthDrifted { actual: LENGTH + 1, expected: LENGTH }),
        "a daemon that sent more has already cost the disk it was written to"
    );
    transfer.require_publishable(DIGEST).expect("length and digest agree");
    assert_eq!(transfer.require_publishable(OTHER_DIGEST), Err(DownloadRefusal::DigestDrifted));

    let mut partial = Transfer::of(LENGTH, DIGEST);
    partial.absorb(HALF_LENGTH).expect("half of it");
    assert_eq!(
        partial.require_publishable(DIGEST),
        Err(DownloadRefusal::LengthDrifted { actual: HALF_LENGTH, expected: LENGTH })
    );
    let resumed = Transfer::resumed(&record(TransferState::Transferring, HALF_LENGTH));
    assert_eq!(resumed.received(), HALF_LENGTH, "and a retry picks up where the record says");
}

#[test]
fn publication_happens_once_and_never_over_something_that_is_already_there() {
    let directory = tempfile::tempdir().expect("a temporary directory");
    let staging = directory.path().join("one.slingshot-partial");
    let destination = directory.path().join("one.zip");
    std::fs::write(&staging, b"the bytes").expect("the staging file writes");
    publish(&staging, &destination).expect("nothing is at the destination");
    assert_eq!(std::fs::read(&destination).expect("it is there"), b"the bytes");

    std::fs::write(&staging, b"other bytes").expect("a second staging file writes");
    assert_eq!(
        publish(&staging, &destination),
        Err(DownloadRefusal::DestinationOccupied),
        "treating an existing destination as a target would destroy a file this command did \
         not make"
    );
    assert_eq!(
        std::fs::read(&destination).expect("it is still there"),
        b"the bytes",
        "and the original is preserved"
    );
}

#[test]
fn a_rerun_does_only_what_is_left_and_re_renders_a_publication_it_already_made() {
    assert_eq!(prior_work(None, false), PriorWork::None);
    assert_eq!(
        prior_work(Some(&record(TransferState::Transferring, PARTIAL_LENGTH)), false),
        PriorWork::Resumable
    );
    assert_eq!(
        prior_work(Some(&record(TransferState::ReadyToPublish, LENGTH)), false),
        PriorWork::ReadyToPublish
    );
    assert_eq!(
        prior_work(Some(&record(TransferState::Published, LENGTH)), true),
        PriorWork::AlreadyPublished,
        "the original success is re-rendered rather than the artifact fetched again"
    );
    assert_eq!(
        prior_work(Some(&record(TransferState::Published, LENGTH)), false),
        PriorWork::None,
        "a published receipt whose destination does not match is an ordinary collision"
    );
}

#[test]
fn a_sidecar_round_trips_through_its_own_canonical_form() {
    let held = record(TransferState::ReadyToPublish, LENGTH);
    let written = serde_json::to_string(&held).expect("it serializes");
    let read: StagingRecord = serde_json::from_str(&written).expect("it reads back");
    assert_eq!(read, held, "a retry reads exactly what the interrupted attempt wrote");
    assert!(read.is_complete());
    let mut file = tempfile::NamedTempFile::new().expect("a temporary file");
    file.write_all(written.as_bytes()).expect("the sidecar writes");
    file.flush().expect("the sidecar lands");
    let from_disk: StagingRecord =
        serde_json::from_str(&std::fs::read_to_string(file.path()).expect("it reads"))
            .expect("it parses");
    assert_eq!(from_disk, held);
}
