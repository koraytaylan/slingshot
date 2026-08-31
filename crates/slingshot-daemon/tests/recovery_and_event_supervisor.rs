//! Progress while the daemon is alive, instead of restarting it.
//!
//! The subject is what a daemon is allowed to conclude from its own behaviour.
//! Running out of automatic attempts says something about this process, not
//! about the agent, so exhaustion ends an operation only when nothing was
//! executed and otherwise pauses work a person can release. Anything else would
//! be reporting patience as a remote fact.
//!
//! The category mapping is checked for completeness rather than by example:
//! every failure category this build publishes must map to exactly one
//! disposition, and one it does not publish must map to nothing. A default
//! would be a guess about a failure whose meaning is unknown, and both possible
//! guesses - retry it, or call it rejected - are wrong in the case that matters.
//!
//! Scheduling is deterministic given the injected sample, and a restart
//! reconstructs the remaining wait by clamping the wall-clock residual into the
//! wait that was chosen, so a clock that jumps moves scheduling and nothing
//! else.

use slingshot_daemon::operation::recovery_and_event_supervisor::{
    ArtifactUnavailability, CategoryDisposition, DueWork, ExecutionCertainty, Exhaustion,
    OUTCOME_UNKNOWN_CATEGORIES, REMOTE_FAILURE_CATEGORIES, RecoveryAndEventSupervisor,
    ResumeOutcome, ResumeRefusal, ResumeRequest, RetryCategory, RetrySchedule,
    STAGING_CLEANUP_CATEGORY, artifact_disposition, automatic_attempt_cap, disposition_for,
    jitter_ceiling_milliseconds, on_exhaustion, permits_rebuild, preserves_maintenance_diagnosis,
    published_categories, resumed_delay_milliseconds, schedule,
};
use slingshot_domain::author_agent_transport_contract::AuthorAgentTransportContract;

/// Where the vectors this suite is driven from live.
const FIXTURES: &str = "tests/fixtures/recovery-and-event-supervisor";

/// The partition this supervisor is over.
const TARGET: &str = "target-identity-digest-one";

/// The environment revision this work was submitted under.
const REVISION: &str = "environment-revision-one";

/// Another environment revision, which this work was not submitted under.
const OTHER_REVISION: &str = "environment-revision-two";

/// One instant, for the vectors that need one.
const NOW: u64 = 1_700_000_000_000;

/// The operation revision a resume quotes.
const OPERATION_REVISION: u64 = 4;

/// An operation revision that has since moved.
const MOVED_REVISION: u64 = 5;

/// What one resume receipt is called.
const RECEIPT: &str = "resume-receipt-one";

/// A category this build does not publish.
const UNPUBLISHED_CATEGORY: &str = "a_failure_this_build_never_heard_of";

/// Returns every vector one fixture holds.
fn vectors(name: &str) -> Vec<serde_json::Value> {
    let path = format!("{FIXTURES}/{name}");
    let text = std::fs::read_to_string(&path).unwrap_or_else(|_| panic!("{path} is readable"));
    text.lines().map(|line| serde_json::from_str(line).expect("each line is one vector")).collect()
}

/// Returns how one disposition is spelled in the vectors.
fn disposition_spelling(disposition: CategoryDisposition) -> &'static str {
    match disposition {
        CategoryDisposition::Rejected => "rejected",
        CategoryDisposition::RemoteFailed => "remote-failed",
        CategoryDisposition::RecoveryRequired => "recovery-required",
        CategoryDisposition::ResultUnavailable => "result-unavailable",
        CategoryDisposition::RecoveryWindowExpired => "recovery-window-expired",
    }
}

/// Returns the certainty `spelling` names.
fn certainty_named(spelling: &str) -> ExecutionCertainty {
    match spelling {
        "confirmed-not-executed" => ExecutionCertainty::ConfirmedNotExecuted,
        "remote-outcome-unknown" => ExecutionCertainty::RemoteOutcomeUnknown,
        "authoritative-remote-success" => ExecutionCertainty::AuthoritativeRemoteSuccess,
        other => panic!("{other} is a certainty this suite does not name"),
    }
}

/// Returns the unavailability `spelling` names.
fn unavailability_named(spelling: &str) -> ArtifactUnavailability {
    match spelling {
        "identified-missing-beyond-grace" => ArtifactUnavailability::IdentifiedMissingBeyondGrace,
        "identified-retention-expired" => ArtifactUnavailability::IdentifiedRetentionExpired,
        "bare" => ArtifactUnavailability::Bare,
        "malformed" => ArtifactUnavailability::Malformed,
        "mismatched" => ArtifactUnavailability::Mismatched,
        other => panic!("{other} is an unavailability this suite does not name"),
    }
}

/// Returns the resume a person asks for, in `category`.
fn resume_request(category: RetryCategory, supplied_revision: &str, quoted: u64) -> ResumeRequest {
    ResumeRequest {
        agent_operation_identifier: "ambiguous".to_owned(),
        category,
        quoted_operation_revision: quoted,
        receipt_identifier: RECEIPT.to_owned(),
        supplied_revision: supplied_revision.to_owned(),
    }
}

/// Returns one piece of work of `category`, due at `eligible`.
fn work(named: &str, category: RetryCategory, eligible: u64) -> DueWork {
    DueWork {
        agent_operation_identifier: named.to_owned(),
        category,
        eligible_at_unix_milliseconds: eligible,
        paused: false,
    }
}

#[test]
fn every_published_failure_category_maps_to_exactly_one_disposition() {
    let published = published_categories();
    assert!(!published.is_empty(), "this build publishes commands, so it publishes failures");
    for category in &published {
        let disposition = disposition_for(category)
            .unwrap_or_else(|| panic!("{category} is published and must map to something"));
        let expected = if OUTCOME_UNKNOWN_CATEGORIES.contains(&category.as_str()) {
            CategoryDisposition::RecoveryRequired
        } else if REMOTE_FAILURE_CATEGORIES.contains(&category.as_str()) {
            CategoryDisposition::RemoteFailed
        } else {
            CategoryDisposition::Rejected
        };
        assert_eq!(disposition, expected, "{category}");
    }
    assert_eq!(
        disposition_for(UNPUBLISHED_CATEGORY),
        None,
        "a default would be a guess about a failure whose meaning this build does not know"
    );
}

#[test]
fn the_categories_that_settle_nothing_are_named_and_no_others_are() {
    for vector in vectors("category-dispositions.jsonl") {
        let category = vector["category"].as_str().expect("a category");
        let disposition = disposition_for(category).expect("each vector names a published one");
        assert_eq!(
            disposition_spelling(disposition),
            vector["disposition"].as_str().expect("a disposition"),
            "{category}: {}",
            vector["note"].as_str().unwrap_or_default()
        );
    }
    for category in OUTCOME_UNKNOWN_CATEGORIES {
        assert!(
            published_categories().contains(&(*category).to_owned()),
            "{category} is a category this build actually publishes"
        );
    }
}

#[test]
fn a_staging_area_that_could_not_be_cleaned_up_is_not_a_reason_to_build_again() {
    assert_eq!(disposition_for(STAGING_CLEANUP_CATEGORY), Some(CategoryDisposition::Rejected));
    assert!(preserves_maintenance_diagnosis(STAGING_CLEANUP_CATEGORY));
    assert!(
        !permits_rebuild(STAGING_CLEANUP_CATEGORY),
        "rebuilding would leave a second staging area beside the first"
    );
    assert!(permits_rebuild("filevault_package_failed"));
    assert!(!preserves_maintenance_diagnosis("filevault_package_failed"));
}

#[test]
fn only_a_fully_identified_answer_may_end_an_artifact_acquisition() {
    for vector in vectors("artifact-unavailability.jsonl") {
        let named = vector["unavailability"].as_str().expect("an unavailability");
        assert_eq!(
            disposition_spelling(artifact_disposition(unavailability_named(named))),
            vector["disposition"].as_str().expect("a disposition"),
            "{named}: a wire response must not choose a disposition it was never asked for"
        );
    }
}

#[test]
fn exhaustion_ends_an_operation_only_when_nothing_was_executed() {
    for vector in vectors("exhaustion.jsonl") {
        let named = vector["certainty"].as_str().expect("a certainty");
        let expected = match vector["exhaustion"].as_str().expect("an exhaustion") {
            "authoritative-non-execution" => Exhaustion::AuthoritativeNonExecution,
            "recovery-required" => Exhaustion::RecoveryRequired,
            other => panic!("{other} is an exhaustion this suite does not name"),
        };
        assert_eq!(
            on_exhaustion(certainty_named(named)),
            expected,
            "{named}: a retry budget is a fact about this daemon, not about the agent"
        );
    }
    assert!(automatic_attempt_cap() > 0, "there is a budget, and it is the contract's");
}

#[test]
fn the_delay_sequence_follows_the_samples_and_stays_inside_its_named_caps() {
    for vector in vectors("jitter.jsonl") {
        let attempt = vector["attempt"].as_u64().expect("an attempt");
        let sample = vector["sample"].as_u64().expect("a sample");
        assert_eq!(
            jitter_ceiling_milliseconds(attempt),
            vector["ceiling"].as_u64().expect("a ceiling")
        );
        let written = schedule(RetryCategory::AmbiguousSubmission, attempt, sample, NOW);
        assert_eq!(
            written.chosen_delay_milliseconds,
            vector["delay"].as_u64().expect("a delay"),
            "the same sample chooses the same delay every time"
        );
        assert!(written.chosen_delay_milliseconds <= written.jitter_ceiling_milliseconds);
        assert_eq!(
            written.eligible_at_unix_milliseconds,
            NOW + written.chosen_delay_milliseconds,
            "the wall clock appears only as the instant a restart reconstructs from"
        );
    }
    let contract = AuthorAgentTransportContract::embedded();
    assert!(
        jitter_ceiling_milliseconds(automatic_attempt_cap())
            <= contract.limit("retry_jitter_cap_milliseconds")
    );
}

#[test]
fn a_restart_reconstructs_only_the_wait_it_persisted() {
    for vector in vectors("restarts.jsonl") {
        let name = vector["name"].as_str().expect("a name");
        let written = RetrySchedule {
            attempt: 1,
            category: RetryCategory::EventReconnect,
            chosen_delay_milliseconds: vector["chosen"].as_u64().expect("a delay"),
            eligible_at_unix_milliseconds: vector["eligible_at"].as_u64().expect("an instant"),
            jitter_ceiling_milliseconds: jitter_ceiling_milliseconds(1),
        };
        assert_eq!(
            resumed_delay_milliseconds(&written, vector["now"].as_u64().expect("an instant")),
            vector["resumed"].as_u64().expect("what remains"),
            "{name}: a virtual clock moves scheduling and never an identity or a certainty"
        );
    }
}

#[test]
fn due_work_is_served_fairly_across_categories_rather_than_by_deadline_alone() {
    let mut supervisor = RecoveryAndEventSupervisor::over(TARGET);
    supervisor.hold(work("chatty-stream", RetryCategory::EventReconnect, NOW));
    supervisor.hold(work("chatty-stream-again", RetryCategory::EventReconnect, NOW));
    supervisor.hold(work("chatty-stream-thrice", RetryCategory::EventReconnect, NOW));
    supervisor.hold(work("waiting-submission", RetryCategory::AmbiguousSubmission, NOW + 1));

    let first = supervisor.next_due(NOW).expect("something is due");
    assert_eq!(first.category, RetryCategory::EventReconnect);
    let second = supervisor.next_due(NOW + 1).expect("something is due");
    assert_eq!(
        second.category,
        RetryCategory::AmbiguousSubmission,
        "a stream that drops constantly must not starve the operation somebody is watching"
    );
    assert_eq!(supervisor.served().get(&RetryCategory::EventReconnect), Some(&1));
    assert_eq!(supervisor.served().get(&RetryCategory::AmbiguousSubmission), Some(&1));
    assert!(supervisor.next_due(NOW - 1).is_none(), "nothing before its deadline is due");
}

#[test]
fn paused_work_waits_for_a_person_and_the_right_receipt_releases_it() {
    let mut supervisor = RecoveryAndEventSupervisor::over(TARGET);
    supervisor.hold(work("ambiguous", RetryCategory::AmbiguousSubmission, NOW));
    assert!(supervisor.pause("ambiguous"));
    assert!(supervisor.next_due(NOW).is_none(), "an exhausted policy waits rather than concludes");

    assert_eq!(
        supervisor.resume(
            &resume_request(RetryCategory::AmbiguousSubmission, OTHER_REVISION, OPERATION_REVISION),
            REVISION,
            OPERATION_REVISION
        ),
        Err(ResumeRefusal::WrongSelectedRevision)
    );
    assert_eq!(
        supervisor.resume(
            &resume_request(RetryCategory::AmbiguousSubmission, REVISION, OPERATION_REVISION),
            REVISION,
            MOVED_REVISION
        ),
        Err(ResumeRefusal::StaleOperationRevision {
            quoted: OPERATION_REVISION,
            stored: MOVED_REVISION
        })
    );
    assert_eq!(
        supervisor.resume(
            &resume_request(RetryCategory::ArtifactAcquisition, REVISION, OPERATION_REVISION),
            REVISION,
            OPERATION_REVISION
        ),
        Err(ResumeRefusal::WrongCategory)
    );
    assert!(supervisor.next_due(NOW).is_none(), "a refused resume changes nothing");

    assert_eq!(
        supervisor
            .resume(
                &resume_request(RetryCategory::AmbiguousSubmission, REVISION, OPERATION_REVISION),
                REVISION,
                OPERATION_REVISION
            )
            .expect("a fresh receipt wakes it"),
        ResumeOutcome::Woken(RetryCategory::AmbiguousSubmission)
    );
    assert!(supervisor.next_due(NOW).is_some());
}

#[test]
fn an_exact_receipt_replay_wakes_nothing_a_second_time() {
    let mut supervisor = RecoveryAndEventSupervisor::over(TARGET);
    supervisor.hold(work("ambiguous", RetryCategory::AmbiguousSubmission, NOW));
    supervisor.pause("ambiguous");
    supervisor
        .resume(
            &resume_request(RetryCategory::AmbiguousSubmission, REVISION, OPERATION_REVISION),
            REVISION,
            OPERATION_REVISION,
        )
        .expect("the first time wakes it");
    supervisor.pause("ambiguous");
    assert_eq!(
        supervisor
            .resume(
                &resume_request(RetryCategory::AmbiguousSubmission, REVISION, OPERATION_REVISION),
                REVISION,
                OPERATION_REVISION
            )
            .expect("a replay is an answer, not an error"),
        ResumeOutcome::Replayed
    );
    assert!(
        supervisor.next_due(NOW).is_none(),
        "an exact replay is side-effect-free, so what a person paused stays paused"
    );
}

#[test]
fn shutting_down_lets_go_of_work_and_cancels_nothing_remote() {
    let mut supervisor = RecoveryAndEventSupervisor::over(TARGET);
    supervisor.hold(work("running", RetryCategory::ResultAcquisition, NOW));
    supervisor.hold(work("waiting", RetryCategory::SnapshotPoll, NOW));
    assert!(supervisor.accepts_new_work());
    let detached = supervisor.detach();
    assert_eq!(detached.len(), 2, "everything held is handed back rather than dropped silently");
    assert!(!supervisor.accepts_new_work());
    assert!(
        supervisor.work().is_empty() && supervisor.next_due(NOW).is_none(),
        "a Sling job this daemon started is the agent's to finish"
    );
    assert_eq!(supervisor.partition(), TARGET);
}
