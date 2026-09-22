//! The composition a product build runs work through.
//!
//! The subject is the order and the single decision each stage makes. Every
//! piece it uses is proved on its own elsewhere; what could still be wrong here
//! is that a stage runs before the one it depends on, or that a stage concludes
//! something no stage was entitled to conclude.
//!
//! So the ports are driven directly and the assertions are about which of them
//! were reached. An execution that never got past the handoff must not have
//! asked the agent what happened, and one whose result never arrived must not
//! have tried to fetch artifacts - because reaching a later stage means having
//! believed the earlier one.
//!
//! The second claim is that nothing unresolved is reported as an ending. A
//! submission whose fate is unclear, a stream that dropped, an artifact that is
//! not there yet: each is outstanding work, because settling an operation on
//! this daemon's own difficulty reports a local problem as a remote fact.

use std::cell::RefCell;

use slingshot_daemon::author_agent_operation_executor::{
    AgentSettlement, ArtifactCompletion, AuthorAgentOperationExecutor, AuthorPorts,
    COMPLETING_DETAIL, SUBMITTING_DETAIL, SUPERVISING_DETAIL, outcome_of_handoff,
};
use slingshot_daemon::operation::remote_submission::HandoffDisposition;
use slingshot_daemon::startup::{SelectedTarget, StartupRefusal, install_executor};
use slingshot_domain::command::catalog::Command;
use slingshot_domain::command::query_paths::QueryPathsCommand;
use slingshot_domain::command::repository_path::RepositoryPath;
use slingshot_domain::operation::{
    OperationExecutionCertainty, RecoveryCategory, RecoveryExecutionEvidence,
    TerminalFailureDisposition, TerminalFailureKind,
};
use slingshot_domain::operation_executor::{
    ExecutionFuture, ExecutionIdentity, OperationExecutor, OperationExecutorOutcome, ProgressPort,
};
use slingshot_storage::database::{OperationDatabase, RequiredSettings};

/// Two-character pairs in a sixty-four-character hexadecimal value.
const DIGEST_PAIRS: usize = 32;

/// The instant a recovery fact in this suite was observed at.
const OBSERVED_AT: u64 = 123_456;

/// Bytes one page occupies, from the runtime contract.
const PAGE_BYTES: u64 = 4096;

/// Pages the database may reach, from the runtime contract.
const DATABASE_PAGES: u64 = 262_144;

/// Milliseconds a busy connection waits, from the runtime contract.
const BUSY_TIMEOUT: u64 = 5000;

/// The environment revision this daemon serves.
const REVISION: &str = "environment-revision-one";

/// The runtime contract digest this daemon serves under.
const RUNTIME_DIGEST: &str = "runtime-contract-digest";

/// What a throttled answer asks this daemon to wait.
const RETRY_DELAY: u64 = 5_000;

/// The canonical result a successful execution produces.
const INLINE_RESULT: &str = "{\"paths\":[]}";

/// A path one query asks about.
const QUERY_ROOT: &str = "/content";

/// Ports that answer as they were told and record what was asked.
#[derive(Debug)]
struct ScriptedPorts {
    /// What the artifact stage answers.
    artifacts: ArtifactCompletion,
    /// What was asked, in order.
    asked: RefCell<Vec<&'static str>>,
    /// What the handoff answers.
    handoff: HandoffDisposition,
    /// What the settlement answers.
    settlement: AgentSettlement,
}

impl ScriptedPorts {
    /// Returns ports that accept, settle as told, and publish nothing.
    fn answering(handoff: HandoffDisposition, settlement: AgentSettlement) -> Self {
        Self {
            artifacts: ArtifactCompletion::Published { artifacts: Vec::new() },
            asked: RefCell::new(Vec::new()),
            handoff,
            settlement,
        }
    }
}

impl AuthorPorts for ScriptedPorts {
    fn submit<'a>(
        &'a self,
        _identity: &'a ExecutionIdentity,
        _command: &'a Command,
    ) -> ExecutionFuture<'a, HandoffDisposition> {
        Box::pin(async move {
            self.asked.borrow_mut().push("submit");
            tokio::task::yield_now().await;
            self.handoff.clone()
        })
    }

    fn settle<'a>(
        &'a self,
        _identity: &'a ExecutionIdentity,
    ) -> ExecutionFuture<'a, AgentSettlement> {
        Box::pin(async move {
            self.asked.borrow_mut().push("settle");
            self.settlement.clone()
        })
    }

    fn complete_artifacts<'a>(
        &'a self,
        _identity: &'a ExecutionIdentity,
    ) -> ExecutionFuture<'a, ArtifactCompletion> {
        Box::pin(async move {
            self.asked.borrow_mut().push("complete");
            self.artifacts.clone()
        })
    }
}

/// Dropping local execution while handoff is pending cannot create a terminal result.
#[test]
fn dropping_a_pending_handoff_does_not_settle_or_publish() {
    use std::task::{Context, Poll, Waker};

    let ports = ScriptedPorts::answering(
        HandoffDisposition::Accepted,
        AgentSettlement::Succeeded { inline_result: Some(INLINE_RESULT.to_owned()) },
    );
    let executor = AuthorAgentOperationExecutor::over(&ports);
    let identity = identity();
    let command = command();
    let progress = RecordedProgress::default();
    let mut execution = executor.execute(&identity, &command, &progress);
    assert!(ports.asked.borrow().is_empty());
    assert!(matches!(
        execution.as_mut().poll(&mut Context::from_waker(Waker::noop())),
        Poll::Pending
    ));
    drop(execution);
    assert_eq!(*ports.asked.borrow(), vec!["submit"]);
    assert_eq!(*progress.reported.borrow(), vec![SUBMITTING_DETAIL]);
}

/// A progress port that remembers what it was told.
#[derive(Debug, Default)]
struct RecordedProgress {
    /// What was reported, in order.
    reported: RefCell<Vec<String>>,
}

impl ProgressPort for RecordedProgress {
    fn report(&self, detail: &str) {
        self.reported.borrow_mut().push(detail.to_owned());
    }
}

/// Returns one execution identity.
fn identity() -> ExecutionIdentity {
    ExecutionIdentity {
        attempt: 1,
        author_target_identity_digest: "ab".repeat(DIGEST_PAIRS),
        selected_environment_revision: REVISION.to_owned(),
        operation_identifier: "operation-one".to_owned(),
    }
}

/// Returns one command to run.
fn command() -> Command {
    Command::QueryPaths(QueryPathsCommand {
        primary_node_type: None,
        property_predicates: None,
        result_window: None,
        root_path: RepositoryPath::parse(QUERY_ROOT).expect("a repository path"),
    })
}

/// Returns what one execution through `ports` produced.
fn executed(ports: &ScriptedPorts) -> (OperationExecutorOutcome, Vec<String>) {
    let progress = RecordedProgress::default();
    let runtime =
        tokio::runtime::Builder::new_current_thread().enable_all().build().expect("test runtime");
    let outcome = runtime.block_on(AuthorAgentOperationExecutor::over(ports).execute(
        &identity(),
        &command(),
        &progress,
    ));
    let reported = progress.reported.borrow().clone();
    (outcome, reported)
}

/// Returns the settings a database here is opened under.
fn settings() -> RequiredSettings {
    RequiredSettings {
        page_bytes: PAGE_BYTES,
        database_pages: DATABASE_PAGES,
        busy_timeout_milliseconds: BUSY_TIMEOUT,
    }
}

/// Returns the target this daemon serves.
fn served() -> SelectedTarget {
    SelectedTarget {
        author_target_identity_digest: "ab".repeat(DIGEST_PAIRS),
        daemon_runtime_contract_digest: RUNTIME_DIGEST.to_owned(),
        selected_environment_revision: REVISION.to_owned(),
    }
}

#[test]
fn a_successful_execution_reaches_every_stage_in_order() {
    let ports = ScriptedPorts::answering(
        HandoffDisposition::Accepted,
        AgentSettlement::Succeeded { inline_result: Some(INLINE_RESULT.to_owned()) },
    );
    let (outcome, reported) = executed(&ports);
    assert_eq!(
        outcome,
        OperationExecutorOutcome::Succeeded {
            artifacts: Vec::new(),
            inline_result: Some(INLINE_RESULT.to_owned())
        }
    );
    assert_eq!(
        ports.asked.borrow().clone(),
        vec!["submit", "settle", "complete"],
        "reaching a later stage means having believed the earlier one"
    );
    assert_eq!(reported, vec![SUBMITTING_DETAIL, SUPERVISING_DETAIL, COMPLETING_DETAIL]);
}

#[test]
fn a_handoff_that_settles_nothing_never_asks_the_agent_what_happened() {
    for disposition in [
        HandoffDisposition::NotExecuted,
        HandoffDisposition::RetryAfter { milliseconds: RETRY_DELAY },
        HandoffDisposition::Unknown { cause: None },
        HandoffDisposition::Conflict,
        HandoffDisposition::RecoveryWindowExpired,
    ] {
        let ports = ScriptedPorts::answering(
            disposition.clone(),
            AgentSettlement::Succeeded { inline_result: None },
        );
        let (outcome, _) = executed(&ports);
        assert_eq!(
            ports.asked.borrow().clone(),
            vec!["submit"],
            "{disposition:?}: nothing after a handoff that already answered"
        );
        // The retry delay is drawn from a bounded random range, so the two facts are compared on
        // everything except that one number: two calls to the same function legitimately produce
        // two different delays, and asserting equality would be asserting that a jitter source is
        // not random.
        let (Some(concluded), Some(expected)) = (Some(outcome), outcome_of_handoff(&disposition))
        else {
            panic!("{disposition:?}: the executor concludes what the handoff does")
        };
        assert_eq!(
            without_delay(&concluded),
            without_delay(&expected),
            "{disposition:?}: the executor concludes exactly what the handoff does"
        );
    }
}

/// Returns one outcome with its retry delay removed, which is the jittered field.
fn without_delay(outcome: &OperationExecutorOutcome) -> OperationExecutorOutcome {
    match outcome {
        OperationExecutorOutcome::RecoveryRequired { recovery } => {
            let mut held = recovery.clone();
            held.retry_delay_milliseconds = 0;
            OperationExecutorOutcome::RecoveryRequired { recovery: held }
        }
        other => other.clone(),
    }
}

#[test]
fn an_answer_without_a_usable_submission_leaves_the_queue() {
    let ports = ScriptedPorts::answering(
        HandoffDisposition::Unknown { cause: None },
        AgentSettlement::Succeeded { inline_result: None },
    );
    let (outcome, _) = executed(&ports);
    let OperationExecutorOutcome::TerminalFailure { failure } = outcome else {
        panic!("an answer that cannot be used ends the operation")
    };
    assert_eq!(
        failure.kind,
        slingshot_domain::operation::TerminalFailureKind::RetryPolicyExhausted
    );
    assert_eq!(*ports.asked.borrow(), vec!["submit"]);
}

#[test]
fn a_failed_capability_check_proves_nothing_ran_and_names_its_remedy() {
    use slingshot_agent_connection::capability_discovery::CapabilityExchangeRefusal;
    for (refusal, kind, remedy) in [
        (
            CapabilityExchangeRefusal::Unanswered,
            TerminalFailureKind::RetryPolicyExhausted,
            "new operation key is safe to send",
        ),
        (
            CapabilityExchangeRefusal::Unauthenticated,
            TerminalFailureKind::Rejected,
            "check-configuration",
        ),
        (CapabilityExchangeRefusal::Refused(403), TerminalFailureKind::Rejected, "status 403"),
        (
            CapabilityExchangeRefusal::Incompatible,
            TerminalFailureKind::Rejected,
            "deploy the agent built with this client",
        ),
    ] {
        let ports = ScriptedPorts::answering(
            HandoffDisposition::CapabilityCheckFailed { refusal },
            AgentSettlement::Succeeded { inline_result: None },
        );
        let (outcome, _) = executed(&ports);
        let OperationExecutorOutcome::TerminalFailure { failure } = outcome else {
            panic!("{refusal:?}: a check that failed before sending ends the operation")
        };
        assert_eq!(failure.kind, kind, "{refusal:?}");
        assert_eq!(
            failure.disposition,
            TerminalFailureDisposition::AuthoritativeNonExecution {
                certainty: OperationExecutionCertainty::ConfirmedNotExecuted,
            },
            "{refusal:?}: nothing was sent, so nothing ran"
        );
        assert!(
            slingshot_domain::operation::terminal_pairing_is_legal(
                failure.kind,
                failure.disposition
            ),
            "{refusal:?}"
        );
        let detail = failure.metadata.unwrap_or_default();
        assert!(detail.contains(remedy), "{refusal:?} does not name its remedy: {detail}");
        assert_eq!(*ports.asked.borrow(), vec!["submit"], "{refusal:?}");
    }
}

#[test]
fn a_named_unknown_cause_is_kept_on_recovery_and_never_looks_like_lookup_required() {
    use slingshot_agent_connection::command_submission::{SubmissionOutcome, UnknownCause};
    use slingshot_daemon::operation::remote_submission::disposition_of;

    let cause = UnknownCause::UnvalidatedStatus;
    let handoff = disposition_of(&SubmissionOutcome::SubmissionUnknown { cause });
    assert_eq!(handoff, HandoffDisposition::Unknown { cause: Some(cause) });
    assert!(handoff.requires_lookup());
    assert!(!handoff.permits_another_send());
    let lookup = disposition_of(&SubmissionOutcome::SubmissionUnknown {
        cause: UnknownCause::LookupRequired,
    });
    assert_eq!(lookup, HandoffDisposition::ReconcileRetained);
    assert_eq!(outcome_of_handoff(&lookup), None);

    let ports =
        ScriptedPorts::answering(handoff, AgentSettlement::Succeeded { inline_result: None });
    let (outcome, _) = executed(&ports);
    let OperationExecutorOutcome::TerminalFailure { failure } = outcome else {
        panic!("a named unknown cause from an answer ends the operation")
    };
    assert_eq!(failure.metadata.as_deref(), Some(cause.spelling().as_str()));
    assert_eq!(*ports.asked.borrow(), vec!["submit"], "an answered submission does not settle");
}

#[test]
fn an_existing_child_enters_lookup_without_claiming_acceptance_or_permitting_resubmission() {
    use slingshot_agent_connection::command_submission::{SubmissionOutcome, UnknownCause};
    use slingshot_daemon::operation::remote_submission::disposition_of;
    use slingshot_domain::operation::RecoveryFact;

    let handoff = disposition_of(&SubmissionOutcome::SubmissionUnknown {
        cause: UnknownCause::LookupRequired,
    });
    assert_eq!(handoff, HandoffDisposition::ReconcileRetained);
    assert!(handoff.requires_lookup());
    assert!(!handoff.permits_another_send());
    assert_eq!(outcome_of_handoff(&handoff), None);
    let recovery = RecoveryFact {
        category: RecoveryCategory::OperationLookup,
        evidence: RecoveryExecutionEvidence::ExecutionCertainty {
            certainty: OperationExecutionCertainty::RemoteOutcomeUnknown,
        },
        attempt_count: 3,
        detail: String::new(),
        manual_resume_eligible: false,
        retry_delay_milliseconds: 234,
        retry_observed_at_unix_milliseconds: 567,
    };
    let ports = ScriptedPorts::answering(
        handoff,
        AgentSettlement::Outstanding { recovery: recovery.clone() },
    );
    let (outcome, _) = executed(&ports);
    assert_eq!(outcome, OperationExecutorOutcome::RecoveryRequired { recovery });
    assert_eq!(*ports.asked.borrow(), vec!["submit", "settle"]);

    // An ambiguous response from this attempt still yields recovery; it is
    // not an instruction to bypass that recovery's scheduling or backoff.
    let ambiguous =
        disposition_of(&SubmissionOutcome::SubmissionUnknown { cause: UnknownCause::Body });
    assert_eq!(ambiguous, HandoffDisposition::Unknown { cause: Some(UnknownCause::Body) });
    assert!(outcome_of_handoff(&ambiguous).is_some());
}

#[test]
fn settlement_preserves_the_complete_recovery_decision_without_fetching_artifacts() {
    use slingshot_domain::operation::RecoveryFact;
    for (category, evidence, paused) in [
        (
            RecoveryCategory::ResultAcquisition,
            RecoveryExecutionEvidence::AuthoritativeRemoteSuccess,
            false,
        ),
        (
            RecoveryCategory::PersistentCapacityUnavailable,
            RecoveryExecutionEvidence::AuthoritativeRemoteSuccess,
            true,
        ),
        (
            RecoveryCategory::OperationLookup,
            RecoveryExecutionEvidence::ExecutionCertainty {
                certainty: OperationExecutionCertainty::RemoteOutcomeUnknown,
            },
            true,
        ),
    ] {
        let recovery = RecoveryFact {
            category,
            evidence,
            attempt_count: 7,
            detail: "retained recovery decision".to_owned(),
            manual_resume_eligible: paused,
            retry_delay_milliseconds: if paused { 0 } else { 1234 },
            retry_observed_at_unix_milliseconds: 987654,
        };
        assert!(category.admits(evidence));
        let ports = ScriptedPorts::answering(
            HandoffDisposition::Duplicate,
            AgentSettlement::Outstanding { recovery: recovery.clone() },
        );
        let (outcome, reported) = executed(&ports);
        assert_eq!(outcome, OperationExecutorOutcome::RecoveryRequired { recovery });
        assert_eq!(*ports.asked.borrow(), vec!["submit", "settle"]);
        assert_eq!(reported, vec![SUBMITTING_DETAIL, SUPERVISING_DETAIL]);
    }
}

#[test]
fn settlement_preserves_terminal_effect_evidence_without_fetching_artifacts() {
    use slingshot_domain::operation::TerminalFailure;
    for (kind, disposition) in [
        (
            TerminalFailureKind::ResultUnavailable,
            TerminalFailureDisposition::AuthoritativeRemoteSuccess,
        ),
        (TerminalFailureKind::RemoteFailed, TerminalFailureDisposition::AuthoritativeRemoteFailure),
        (
            TerminalFailureKind::Rejected,
            TerminalFailureDisposition::AuthoritativeNonExecution {
                certainty: OperationExecutionCertainty::ConfirmedNotExecuted,
            },
        ),
        (
            TerminalFailureKind::RemoteStateLost,
            TerminalFailureDisposition::FailClosedIndeterminate {
                certainty: OperationExecutionCertainty::RemoteOutcomeUnknown,
            },
        ),
    ] {
        let failure = TerminalFailure { kind, disposition, metadata: None };
        let ports = ScriptedPorts::answering(
            HandoffDisposition::Accepted,
            AgentSettlement::Terminal { failure: failure.clone() },
        );
        let (outcome, _) = executed(&ports);
        assert_eq!(outcome, OperationExecutorOutcome::TerminalFailure { failure });
        assert_eq!(*ports.asked.borrow(), vec!["submit", "settle"]);
    }
}

#[test]
fn a_throttled_handoff_leaves_the_queue() {
    let ports = ScriptedPorts::answering(
        HandoffDisposition::RetryAfter { milliseconds: RETRY_DELAY },
        AgentSettlement::Succeeded { inline_result: None },
    );
    let (outcome, _) = executed(&ports);
    let OperationExecutorOutcome::TerminalFailure { failure } = outcome else {
        panic!("a retryable status is an answer, so the operation ends")
    };
    assert_eq!(failure.kind, TerminalFailureKind::RetryPolicyExhausted);
    assert_eq!(failure.metadata.as_deref(), Some("the author answered with a retryable status"));
}

#[test]
fn the_two_answers_no_further_asking_improves_fail_closed() {
    for (disposition, kind) in [
        (HandoffDisposition::RecoveryWindowExpired, TerminalFailureKind::RemoteStateLost),
        (HandoffDisposition::Conflict, TerminalFailureKind::IntegrityFailure),
    ] {
        let ports = ScriptedPorts::answering(
            disposition,
            AgentSettlement::Succeeded { inline_result: None },
        );
        let (outcome, _) = executed(&ports);
        let OperationExecutorOutcome::TerminalFailure { failure } = outcome else {
            panic!("{kind:?} ends the execution")
        };
        assert_eq!(failure.kind, kind);
        assert!(
            matches!(
                failure.disposition,
                TerminalFailureDisposition::FailClosedIndeterminate { .. }
            ),
            "failing closed says nobody can tell rather than guessing which way"
        );
        assert!(failure.disposition.is_consistent());
    }
}

#[test]
fn an_agent_refusal_ends_the_execution_as_a_proven_nonexecution() {
    let ports = ScriptedPorts::answering(
        HandoffDisposition::Accepted,
        AgentSettlement::NotExecuted { category: "access_denied".to_owned() },
    );
    let (outcome, _) = executed(&ports);
    let OperationExecutorOutcome::TerminalFailure { failure } = outcome else {
        panic!("a refusal ends it")
    };
    assert_eq!(failure.kind, TerminalFailureKind::Rejected);
    assert_eq!(
        failure.disposition,
        TerminalFailureDisposition::AuthoritativeNonExecution {
            certainty: OperationExecutionCertainty::ConfirmedNotExecuted
        }
    );
    assert_eq!(ports.asked.borrow().clone(), vec!["submit", "settle"], "and fetches nothing");
}

#[test]
fn a_remote_failure_stays_distinct_from_a_refusal_because_it_may_have_done_something() {
    let ports = ScriptedPorts::answering(
        HandoffDisposition::Duplicate,
        AgentSettlement::Failed { category: "repository_commit_failed".to_owned() },
    );
    let (outcome, _) = executed(&ports);
    let OperationExecutorOutcome::TerminalFailure { failure } = outcome else {
        panic!("a remote failure ends it")
    };
    assert_eq!(failure.kind, TerminalFailureKind::RemoteFailed);
    assert_eq!(failure.disposition, TerminalFailureDisposition::AuthoritativeRemoteFailure);
    assert_eq!(
        failure.metadata.as_deref(),
        Some("repository_commit_failed"),
        "the category the agent named travels with it"
    );
}

#[test]
fn an_artifact_that_will_not_publish_never_retracts_the_success_it_belongs_to() {
    let mut ports = ScriptedPorts::answering(
        HandoffDisposition::Accepted,
        AgentSettlement::Succeeded { inline_result: None },
    );
    let expected = slingshot_domain::operation::RecoveryFact {
        category: RecoveryCategory::ArtifactTransfer,
        evidence: RecoveryExecutionEvidence::AuthoritativeRemoteSuccess,
        attempt_count: 4,
        detail: "artifact acquisition pending".to_owned(),
        manual_resume_eligible: false,
        retry_delay_milliseconds: 2345,
        retry_observed_at_unix_milliseconds: 6789,
    };
    ports.artifacts = ArtifactCompletion::Recovery { recovery: expected.clone() };
    let (outcome, _) = executed(&ports);
    let OperationExecutorOutcome::RecoveryRequired { recovery } = outcome else {
        panic!("a retrieval that failed is outstanding work")
    };
    assert_eq!(recovery.category, RecoveryCategory::ArtifactTransfer);
    assert_eq!(
        recovery.evidence,
        RecoveryExecutionEvidence::AuthoritativeRemoteSuccess,
        "the work succeeded, and a local retrieval failing does not un-succeed it"
    );
    assert!(recovery.category.admits(recovery.evidence));
    assert_eq!(recovery, expected, "completion cannot reset durable recovery policy");
}

#[test]
fn artifact_unavailability_preserves_success_and_discards_the_inline_result() {
    let mut ports = ScriptedPorts::answering(
        HandoffDisposition::Accepted,
        AgentSettlement::Succeeded { inline_result: Some(INLINE_RESULT.to_owned()) },
    );
    ports.artifacts = ArtifactCompletion::Unavailable;
    let (outcome, _) = executed(&ports);
    assert_eq!(
        outcome,
        OperationExecutorOutcome::TerminalFailure {
            failure: slingshot_domain::operation::TerminalFailure {
                kind: TerminalFailureKind::ResultUnavailable,
                disposition: TerminalFailureDisposition::AuthoritativeRemoteSuccess,
                metadata: None,
            },
        }
    );
    assert_eq!(*ports.asked.borrow(), vec!["submit", "settle", "complete"]);
    assert!(!outcome.publishes_a_result());
}

#[test]
fn artifact_capacity_pause_is_not_replaced_with_an_automatic_transfer_retry() {
    let recovery = slingshot_domain::operation::RecoveryFact {
        category: RecoveryCategory::PersistentCapacityUnavailable,
        evidence: RecoveryExecutionEvidence::AuthoritativeRemoteSuccess,
        attempt_count: 6,
        detail: "persistent capacity unavailable".to_owned(),
        manual_resume_eligible: true,
        retry_delay_milliseconds: 0,
        retry_observed_at_unix_milliseconds: OBSERVED_AT,
    };
    let mut ports = ScriptedPorts::answering(
        HandoffDisposition::ReconcileRetained,
        AgentSettlement::Succeeded { inline_result: Some(INLINE_RESULT.to_owned()) },
    );
    ports.artifacts = ArtifactCompletion::Recovery { recovery: recovery.clone() };
    let (outcome, _) = executed(&ports);
    assert_eq!(outcome, OperationExecutorOutcome::RecoveryRequired { recovery });
    assert!(!outcome.publishes_a_result());
    assert_eq!(*ports.asked.borrow(), vec!["submit", "settle", "complete"]);
}

#[test]
fn the_executor_is_installed_only_after_the_audit_that_precedes_readiness() {
    let database = OperationDatabase::open_in_memory(settings()).expect("a database");
    let ports = ScriptedPorts::answering(
        HandoffDisposition::Accepted,
        AgentSettlement::Succeeded { inline_result: None },
    );
    install_executor(&database, &served(), &ports).expect("nothing foreign is outstanding");
    assert!(
        ports.asked.borrow().is_empty(),
        "installing reaches the author for nothing, so a refusal costs no request"
    );
    assert_eq!(AuthorAgentOperationExecutor::NAME, "author-agent");
    let refusal = install_executor(&database, &served(), &ports);
    assert!(
        !matches!(refusal, Err(StartupRefusal::InvariantUnavailable { .. })),
        "an audit that can run and passes installs the executor every time"
    );
}
