//! Which executor each binary composes, and what every outcome can be.
//!
//! Two claims, and the first is about the shape of the workspace rather than
//! about any code path. Startup installs the author-backed executor, no setting
//! selects one, and no product crate has an edge to test support or to this
//! crate - so the fake is unreachable from anything a user runs, and the
//! executor that refuses everything is reachable only where it is asked for by
//! name. The second is that every outcome the boundary permits is reachable
//! from a script, including the ones a real system produces rarely and at the
//! worst moment.

use slingshot_daemon::author_agent_operation_executor::AuthorAgentOperationExecutor;
use slingshot_daemon::unavailable_operation_executor::{
    UNAVAILABLE_DETAIL, UnavailableOperationExecutor,
};
use slingshot_development::slingshot_test_daemon::{
    DroppedProgress, RecordedProgress, TEST_DAEMON_COMMAND, TestDaemonComposition,
};
use slingshot_domain::operation::{
    OperationExecutionCertainty, RecoveryCategory, RecoveryExecutionEvidence, RecoveryFact,
    TerminalFailure, TerminalFailureDisposition, TerminalFailureKind, terminal_pairing_is_legal,
};
use slingshot_domain::operation_executor::{
    ExecutionIdentity, OperationExecutorOutcome, ProducedArtifact,
};
use slingshot_test_support::fake_operation_executor::{Script, ScriptedStep};

/// Two-character pairs in a sixty-four-character hexadecimal value.
const DIGEST_PAIRS: usize = 32;

/// Binaries this workspace has, and the only ones a release accounts for.
const INHERITED_BINARIES: &[&str] = &["slingshot", "slingshot-development"];

/// Crates a product build is made of.
const PRODUCT_CRATES: &[&str] = &[
    "slingshot-domain",
    "slingshot-configuration",
    "slingshot-agent-protocol",
    "slingshot-local-protocol",
    "slingshot-agent-connection",
    "slingshot-storage",
    "slingshot-daemon",
    "slingshot-command-line",
];

/// Crates no product build may reach.
const TEST_ONLY_CRATES: &[&str] = &["slingshot-test-support", "slingshot-development"];

/// Returns one execution identity.
fn identity(target: &str, operation: &str, attempt: u32) -> ExecutionIdentity {
    ExecutionIdentity {
        attempt,
        author_target_identity_digest: target.repeat(DIGEST_PAIRS),
        operation_identifier: operation.to_owned(),
    }
}

/// Returns a composition whose unscripted executions refuse.
fn composed() -> TestDaemonComposition {
    TestDaemonComposition::new(UnavailableOperationExecutor::outcome())
}

#[test]
fn the_executor_that_refuses_everything_says_so_truthfully_where_it_is_asked_for() {
    let outcome = UnavailableOperationExecutor::outcome();
    let OperationExecutorOutcome::TerminalFailure { failure } = &outcome else {
        panic!("the product executor ends every operation: {outcome:?}");
    };
    assert_eq!(failure.kind, TerminalFailureKind::Rejected);
    assert_eq!(
        failure.disposition,
        TerminalFailureDisposition::AuthoritativeNonExecution {
            certainty: OperationExecutionCertainty::ConfirmedNotExecuted
        },
        "nothing was submitted anywhere, so this is not uncertainty about whether it ran"
    );
    assert!(
        terminal_pairing_is_legal(failure.kind, failure.disposition),
        "and the pairing is one the domain validates rather than one this crate invented"
    );
    assert_eq!(failure.metadata.as_deref(), Some(UNAVAILABLE_DETAIL));
    assert!(outcome.is_terminal(), "it ends the operation");
    assert!(!outcome.publishes_a_result(), "and publishes nothing");
    assert_eq!(
        AuthorAgentOperationExecutor::NAME,
        "author-agent",
        "while the executor startup installs is the one that reaches the author"
    );
}

#[test]
fn no_product_crate_can_reach_the_fake() {
    let metadata = std::process::Command::new(env!("CARGO"))
        .args(["metadata", "--format-version", "1", "--no-deps", "--offline"])
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .output()
        .expect("cargo metadata runs");
    let document: serde_json::Value =
        serde_json::from_slice(&metadata.stdout).expect("metadata is one document");
    let packages = document["packages"].as_array().expect("a package list");

    let mut targets: Vec<String> = Vec::new();
    for package in packages {
        let name = package["name"].as_str().expect("a package name");
        for target in package["targets"].as_array().expect("a target list") {
            let kinds = target["kind"].as_array().expect("a kind list");
            if kinds.iter().any(|kind| kind == "bin") {
                targets.push(target["name"].as_str().expect("a target name").to_owned());
            }
        }
        if !PRODUCT_CRATES.contains(&name) {
            continue;
        }
        for dependency in package["dependencies"].as_array().expect("a dependency list") {
            let depended = dependency["name"].as_str().expect("a dependency name");
            let kind = dependency["kind"].as_str().unwrap_or("normal");
            assert!(
                !(TEST_ONLY_CRATES.contains(&depended) && kind != "dev"),
                "{name} has a {kind} edge to {depended}, so a product build could reach the fake"
            );
        }
    }
    targets.sort();
    assert_eq!(
        targets,
        INHERITED_BINARIES.iter().map(|name| (*name).to_owned()).collect::<Vec<String>>(),
        "the workspace keeps exactly the binaries a release accounts for, and the test daemon \
         is a subcommand of one of them rather than a third"
    );
}

#[test]
fn the_development_dispatcher_routes_the_test_daemon_and_rejects_what_it_does_not_know() {
    let executable = env!("CARGO_BIN_EXE_slingshot-development");
    let composed = std::process::Command::new(executable)
        .arg(TEST_DAEMON_COMMAND)
        .output()
        .expect("the development binary runs");
    assert!(composed.status.success(), "the test daemon composes");
    assert!(
        String::from_utf8_lossy(&composed.stdout).contains(TEST_DAEMON_COMMAND),
        "and says which composition it built"
    );

    for retained in ["dependency-direction", "source-policy"] {
        let known = std::process::Command::new(executable)
            .arg(retained)
            .output()
            .expect("the development binary runs");
        assert!(
            !String::from_utf8_lossy(&known.stderr).contains("unknown"),
            "{retained} is still a command this dispatcher knows"
        );
    }
    let unknown = std::process::Command::new(executable)
        .arg("a-command-nobody-added")
        .output()
        .expect("the development binary runs");
    assert!(!unknown.status.success(), "and an unknown command is refused rather than ignored");
}

#[test]
fn every_outcome_the_boundary_permits_is_reachable_from_a_script() {
    let indeterminate = TerminalFailureDisposition::FailClosedIndeterminate {
        certainty: OperationExecutionCertainty::SubmissionUnknown,
    };
    let not_executed = TerminalFailureDisposition::AuthoritativeNonExecution {
        certainty: OperationExecutionCertainty::ConfirmedNotExecuted,
    };
    let terminal_pairings = [
        (TerminalFailureKind::Rejected, not_executed),
        (TerminalFailureKind::RemoteFailed, TerminalFailureDisposition::AuthoritativeRemoteFailure),
        (
            TerminalFailureKind::ResultUnavailable,
            TerminalFailureDisposition::AuthoritativeRemoteSuccess,
        ),
        (TerminalFailureKind::RecoveryWindowExpired, indeterminate),
        (TerminalFailureKind::RemoteStateLost, indeterminate),
        (TerminalFailureKind::IntegrityFailure, indeterminate),
        (TerminalFailureKind::RetryPolicyExhausted, not_executed),
    ];

    let composition = composed();
    let progress = RecordedProgress::default();
    for (index, (kind, disposition)) in terminal_pairings.iter().enumerate() {
        let operation = format!("operation-{index}");
        composition.executor().script(
            &identity("1d", &operation, 1).author_target_identity_digest,
            &operation,
            Script {
                outcome: OperationExecutorOutcome::TerminalFailure {
                    failure: TerminalFailure {
                        disposition: *disposition,
                        kind: *kind,
                        metadata: None,
                    },
                },
                steps: Vec::new(),
            },
        );
        let produced = composition.execute(&identity("1d", &operation, 1), &progress);
        let OperationExecutorOutcome::TerminalFailure { failure } = produced else {
            panic!("{kind:?} is a terminal failure");
        };
        assert!(
            terminal_pairing_is_legal(failure.kind, failure.disposition),
            "{kind:?} reaches its own legal pairing and no other"
        );
    }
}

#[test]
fn every_recovery_category_reaches_the_evidence_it_admits() {
    let unresolved = RecoveryExecutionEvidence::ExecutionCertainty {
        certainty: OperationExecutionCertainty::RemoteOutcomeUnknown,
    };
    let proven = RecoveryExecutionEvidence::AuthoritativeRemoteSuccess;
    let pairs = [
        (RecoveryCategory::AmbiguousSubmission, unresolved),
        (RecoveryCategory::EventReconnection, unresolved),
        (RecoveryCategory::OperationLookup, unresolved),
        (RecoveryCategory::JobSnapshotRecovery, unresolved),
        (RecoveryCategory::ResultAcquisition, proven),
        (RecoveryCategory::ArtifactTransfer, proven),
        (RecoveryCategory::PersistentCapacityUnavailable, proven),
    ];

    let composition = composed();
    let progress = DroppedProgress;
    for (index, (category, evidence)) in pairs.iter().enumerate() {
        let operation = format!("recovering-{index}");
        composition.executor().script(
            &identity("1d", &operation, 1).author_target_identity_digest,
            &operation,
            Script {
                outcome: OperationExecutorOutcome::RecoveryRequired {
                    recovery: RecoveryFact {
                        attempt_count: 1,
                        category: *category,
                        detail: "outstanding".to_owned(),
                        evidence: *evidence,
                        manual_resume_eligible: true,
                        retry_delay_milliseconds: 0,
                        retry_observed_at_unix_milliseconds: 0,
                    },
                },
                steps: Vec::new(),
            },
        );
        let produced = composition.execute(&identity("1d", &operation, 1), &progress);
        let OperationExecutorOutcome::RecoveryRequired { recovery } = produced else {
            panic!("{category:?} is not an ending");
        };
        assert!(
            recovery.category.admits(recovery.evidence),
            "{category:?} carries only the evidence its kind allows"
        );
        assert!(
            !OperationExecutorOutcome::RecoveryRequired { recovery }.is_terminal(),
            "and an operation waiting on something has not ended"
        );
    }
}

#[test]
fn progress_reaches_a_listener_and_a_consumer_that_left_stalls_nothing() {
    let composition = composed();
    let target = identity("1d", "operation-1", 1).author_target_identity_digest;
    let steps = vec![
        ScriptedStep::Progress { detail: "submitting".to_owned() },
        ScriptedStep::Progress { detail: "accepted".to_owned() },
    ];
    composition.executor().script(
        &target,
        "operation-1",
        Script {
            outcome: OperationExecutorOutcome::Succeeded {
                artifacts: vec![ProducedArtifact {
                    artifact_identifier: "a".repeat(DIGEST_PAIRS * 2),
                    artifact_slot: "structured_result".to_owned(),
                    byte_length: 2,
                    content_digest: "b".repeat(DIGEST_PAIRS * 2),
                    media_type: "application/json".to_owned(),
                }],
                inline_result: None,
            },
            steps: steps.clone(),
        },
    );

    let listening = RecordedProgress::default();
    let produced = composition.execute(&identity("1d", "operation-1", 1), &listening);
    assert!(produced.publishes_a_result(), "the work succeeded");
    assert_eq!(
        listening.reported(),
        vec!["submitting".to_owned(), "accepted".to_owned()],
        "and everything it reported reached the listener, in order"
    );

    composition.executor().script(
        &target,
        "operation-2",
        Script {
            outcome: OperationExecutorOutcome::Succeeded {
                artifacts: Vec::new(),
                inline_result: Some("{}".to_owned()),
            },
            steps,
        },
    );
    let produced = composition.execute(&identity("1d", "operation-2", 1), &DroppedProgress);
    assert!(
        produced.publishes_a_result(),
        "a consumer that stopped listening cannot stall or fail an execution"
    );
}

#[test]
fn invocations_are_counted_per_target_and_tell_a_replay_from_an_execution() {
    let composition = composed();
    let first = identity("1d", "operation-1", 1);
    let elsewhere = identity("2d", "operation-1", 1);
    let script = || Script {
        outcome: OperationExecutorOutcome::Succeeded {
            artifacts: Vec::new(),
            inline_result: Some("{}".to_owned()),
        },
        steps: Vec::new(),
    };
    composition.executor().script(&first.author_target_identity_digest, "operation-1", script());
    composition.executor().script(
        &elsewhere.author_target_identity_digest,
        "operation-1",
        script(),
    );

    composition.execute(&first, &DroppedProgress);
    composition.execute(&first, &DroppedProgress);
    composition.execute(&elsewhere, &DroppedProgress);

    let held = composition.invocations(&first.author_target_identity_digest, "operation-1");
    assert_eq!(held.executed, 1, "one script ran");
    assert_eq!(held.replayed, 1, "and the second call found nothing left to run");
    let neighbour =
        composition.invocations(&elsewhere.author_target_identity_digest, "operation-1");
    assert_eq!(neighbour.executed, 1, "the same identifier at another target is another operation");
    assert_eq!(neighbour.replayed, 0, "with its own count");
}
