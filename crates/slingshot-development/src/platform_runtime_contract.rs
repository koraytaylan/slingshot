//! Platform runtime contract command.
//!
//! The module owns the deterministic policy evaluation every supported row is
//! checked through, and the shape of the single explicitly untrusted report a
//! machine may emit about itself. The evaluation is pure: it decides ownership
//! and readiness from observed facts alone, so all three rows are checked from
//! one machine and the decisions cannot differ between them.
//!
//! Nothing here treats a numeric process identifier as authority, and nothing
//! here aggregates reports: a release claim needs owner-mapped environments
//! whose evidence a provider has attested.

use std::collections::BTreeSet;

use serde::{Deserialize, Serialize};

use crate::supported_platform_matrix::{
    SUPPORTED_TARGET_TRIPLES, SupportedTarget, UNTRUSTED_OBSERVATION_LABEL,
};

/// Schema every report is shaped by, embedded at compile time.
const EVIDENCE_SCHEMA: &str =
    include_str!("../../../support/platform-runtime-evidence.schema.json");

/// Endpoint a Unix row listens on.
pub const UNIX_ENDPOINT_KIND: &str = "unix-domain-socket";

/// Endpoint the Windows row listens on.
pub const WINDOWS_ENDPOINT_KIND: &str = "windows-named-pipe";

/// Outcome of a check that held.
pub const OUTCOME_PASSED: &str = "passed";

/// Outcome of a check that did not hold.
pub const OUTCOME_FAILED: &str = "failed";

/// Outcome of a real check this environment could not run.
pub const OUTCOME_NOT_RUN: &str = "not_run_untrusted";

/// Behaviors every row's real job is reported against.
pub const REPORTED_BEHAVIORS: &[&str] = &[
    "current-user-endpoint-isolation",
    "atomic-readiness",
    "one-daemon-owner-under-contention",
    "one-elected-starter-under-contention",
    "election-release-after-abrupt-exit",
    "connect-before-takeover",
    "detached-child-survives-starter-exit",
    "stale-record-recovery",
    "bounded-supervised-cleanup",
    "windows-remote-client-refusal",
];

/// What one process observed about a runtime namespace.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case", deny_unknown_fields)]
pub struct RuntimeObservation {
    /// Name the fixture gives the observation.
    pub name: String,
    /// Target row the observation claims.
    pub triple: String,
    /// Endpoint the namespace listens on.
    pub endpoint_kind: String,
    /// Whether some process holds the daemon-lifetime owner lock.
    pub owner_lock_held: bool,
    /// Whether some client holds the startup-election lock.
    pub election_lock_held: bool,
    /// Whether a readiness record is present.
    pub readiness_present: bool,
    /// Whether an endpoint object is present.
    pub endpoint_present: bool,
    /// Whether the runtime directory is reachable only by the current user.
    pub current_user_only: bool,
    /// Whether readiness is published by replacing it in one operation.
    pub atomic_readiness: bool,
    /// Whether a child is created detached from its starter.
    pub detached_child_creation: bool,
    /// Whether a supervisor retains the exact child until one disposition.
    pub supervision_retained: bool,
    /// Whether a remote client is refused, where the row requires a decision.
    pub remote_clients_rejected: Option<bool>,
    /// Whether any decision here was taken from a numeric process identifier.
    pub decided_by_process_identifier: bool,
}

/// What a process may conclude about a runtime namespace.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OwnershipDecision {
    /// A live daemon owns the namespace.
    Owned,
    /// No daemon owns the namespace, but records from a prior owner remain.
    RecoverStaleRecords,
    /// No daemon owns the namespace and nothing is left behind.
    Absent,
}

/// One behavior a report records.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ReportedOutcome {
    /// Behavior the row requires.
    pub behavior: String,
    /// Outcome of the deterministic policy evaluation.
    pub policy: String,
    /// Outcome of the real behavior on this environment.
    pub real: String,
    /// Why a real behavior was not run.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
}

/// One machine's own report about the row that matches it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct UntrustedRuntimeReport {
    /// Fixed label that keeps the report from being read as authority.
    pub label: String,
    /// Revision of the source the observation was taken from.
    pub source_revision: String,
    /// Digest of the abstract supported-target manifest.
    pub matrix_digest: String,
    /// Digest of the foundation contract.
    pub contract_digest: String,
    /// Exact target triple of the single row this report describes.
    pub triple: String,
    /// Operating system the observation ran on.
    pub operating_system: String,
    /// Architecture the observation ran on.
    pub architecture: String,
    /// One entry per behavior the row requires.
    pub outcomes: Vec<ReportedOutcome>,
}

/// Returns the schema every report is shaped by.
#[must_use]
pub fn evidence_schema() -> &'static str {
    EVIDENCE_SCHEMA
}

/// Decides who owns a runtime namespace from the observed facts alone.
///
/// Only the operating-system owner lock confers ownership. A readiness record
/// or an endpoint object without that lock is what a prior owner left behind,
/// and the startup-election lock never confers anything.
#[must_use]
pub fn decide_ownership(observation: &RuntimeObservation) -> OwnershipDecision {
    if observation.owner_lock_held {
        OwnershipDecision::Owned
    } else if observation.readiness_present || observation.endpoint_present {
        OwnershipDecision::RecoverStaleRecords
    } else {
        OwnershipDecision::Absent
    }
}

/// Returns the endpoint kind one row requires.
#[must_use]
pub fn required_endpoint_kind(triple: &str) -> &'static str {
    if triple == SUPPORTED_TARGET_TRIPLES[2] { WINDOWS_ENDPOINT_KIND } else { UNIX_ENDPOINT_KIND }
}

/// Reports every requirement one observation fails for its row.
#[must_use]
pub fn evaluate_runtime_policy(
    row: &SupportedTarget,
    observation: &RuntimeObservation,
) -> Vec<String> {
    let mut violations = Vec::new();
    if observation.triple != row.triple {
        violations.push(format!(
            "{} claims {}, not {}",
            observation.name, observation.triple, row.triple
        ));
    }
    let required_endpoint = required_endpoint_kind(&row.triple);
    if observation.endpoint_kind != required_endpoint {
        violations.push(format!(
            "{} listens on {}, not {required_endpoint}",
            observation.name, observation.endpoint_kind
        ));
    }
    let requirements = [
        (
            "the runtime directory is reachable only by the current user",
            observation.current_user_only,
        ),
        ("readiness is published in one replacement", observation.atomic_readiness),
        ("a child is created detached from its starter", observation.detached_child_creation),
        ("a supervisor retains the exact child", observation.supervision_retained),
    ];
    for (requirement, held) in requirements {
        if !held {
            violations.push(format!("{} does not prove that {requirement}", observation.name));
        }
    }
    if observation.decided_by_process_identifier {
        violations.push(format!(
            "{} decided something from a numeric process identifier",
            observation.name
        ));
    }
    violations.extend(evaluate_remote_client_policy(row, observation));
    violations
}

/// Reports whether a row's remote-client decision is present and correct.
fn evaluate_remote_client_policy(
    row: &SupportedTarget,
    observation: &RuntimeObservation,
) -> Vec<String> {
    let windows = row.triple == SUPPORTED_TARGET_TRIPLES[2];
    match (windows, observation.remote_clients_rejected) {
        (true, Some(true)) | (false, None) => Vec::new(),
        (true, Some(false)) => {
            vec![format!("{} admits a remote client on a named pipe", observation.name)]
        }
        (true, None) => vec![format!("{} records no remote-client decision", observation.name)],
        (false, Some(_)) => {
            vec![format!(
                "{} records a remote-client decision on a row that has no pipe",
                observation.name
            )]
        }
    }
}

/// Reports every rule one report breaks.
///
/// A report describes exactly one row, carries the untrusted label, records
/// every required behavior once, and never claims a real outcome for a row this
/// environment is not.
#[must_use]
pub fn evaluate_report(
    report: &UntrustedRuntimeReport,
    current_triple: Option<&str>,
) -> Vec<String> {
    let mut violations = Vec::new();
    if report.label != UNTRUSTED_OBSERVATION_LABEL {
        violations.push(format!("the report is labelled {}", report.label));
    }
    if !SUPPORTED_TARGET_TRIPLES.contains(&report.triple.as_str()) {
        violations.push(format!("the report describes the unsupported row {}", report.triple));
    }
    if current_triple != Some(report.triple.as_str()) {
        violations
            .push(format!("the report describes {}, which this environment is not", report.triple));
    }
    let recorded: BTreeSet<&str> =
        report.outcomes.iter().map(|outcome| outcome.behavior.as_str()).collect();
    if recorded.len() != report.outcomes.len() {
        violations.push("the report records a behavior twice".to_owned());
    }
    let required: BTreeSet<&str> = REPORTED_BEHAVIORS.iter().copied().collect();
    if recorded != required {
        violations.push(format!("the report records the behaviors {recorded:?}"));
    }
    for outcome in &report.outcomes {
        violations.extend(evaluate_outcome(outcome));
    }
    violations
}

/// Reports every rule one recorded behavior breaks.
fn evaluate_outcome(outcome: &ReportedOutcome) -> Vec<String> {
    let mut violations = Vec::new();
    if ![OUTCOME_PASSED, OUTCOME_FAILED].contains(&outcome.policy.as_str()) {
        violations
            .push(format!("{} records the policy outcome {}", outcome.behavior, outcome.policy));
    }
    if ![OUTCOME_PASSED, OUTCOME_FAILED, OUTCOME_NOT_RUN].contains(&outcome.real.as_str()) {
        violations.push(format!("{} records the real outcome {}", outcome.behavior, outcome.real));
    }
    if outcome.real == OUTCOME_NOT_RUN && outcome.reason.is_none() {
        violations.push(format!("{} was not run and gives no reason", outcome.behavior));
    }
    if outcome.real != OUTCOME_NOT_RUN && outcome.reason.is_some() {
        violations.push(format!("{} was run and still gives a reason", outcome.behavior));
    }
    violations
}
