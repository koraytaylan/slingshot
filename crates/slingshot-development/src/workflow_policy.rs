//! What a hosted workflow may hold, and what it may reach.
//!
//! A workflow runs with credentials on a machine nobody in this repository
//! owns, so the questions here are different from the ones the rest of the
//! source policy asks. An action reference that is a tag is a reference
//! somebody else can move; a checkout that persists its credential leaves it
//! where every later step can read it; an expression interpolated into a shell
//! is somebody else's text becoming this repository's command. Each of those is
//! refused by shape rather than by review, because a review of a workflow
//! happens once and the workflow runs every day after that.
//!
//! Permissions are least-privilege by declaration rather than by default. A job
//! that declares none inherits whatever the repository grants, which is exactly
//! the thing nobody remembers to check, so silence is refused too.

use crate::source_policy::{FIRST_LINE, LoadedPolicy, Violation, check_line_count};

/// Length of a full commit identifier, in hexadecimal characters.
const FULL_COMMIT_LENGTH: usize = 40;

/// Permission value a job may hold without being the attestation job.
const READ_PERMISSION: &str = "read";

/// Permission value that grants nothing.
const NO_PERMISSION: &str = "none";

/// Reports whether one action reference is pinned to a full commit.
fn action_is_pinned(reference: &str) -> bool {
    if reference.starts_with("./") {
        return true;
    }
    reference.rsplit_once('@').is_some_and(|(_, revision)| {
        revision.len() == FULL_COMMIT_LENGTH
            && revision.bytes().all(|byte| byte.is_ascii_hexdigit())
    })
}

/// Refuses every rule one workflow step breaks.
fn check_workflow_step(
    policy: &LoadedPolicy,
    path: &str,
    step: &serde_yaml_ng::Value,
    violations: &mut Vec<Violation>,
) {
    if let Some(reference) = step.get("uses").and_then(serde_yaml_ng::Value::as_str) {
        if !action_is_pinned(reference) {
            let rule = "action-is-not-pinned-to-a-full-commit";
            violations.push(Violation::at(path, FIRST_LINE, rule, reference));
        }
        let persists = step.get("with").and_then(|with| with.get("persist-credentials"))
            != Some(&serde_yaml_ng::Value::Bool(false));
        if reference.contains("actions/checkout") && persists {
            let rule = "checkout-persists-its-credential";
            violations.push(Violation::at(path, FIRST_LINE, rule, reference));
        }
    }
    if let Some(script) = step.get("run").and_then(serde_yaml_ng::Value::as_str)
        && script.contains("${{")
    {
        let rule = "workflow-expression-reaches-a-shell";
        let first = script.lines().next().unwrap_or_default();
        violations.push(Violation::at(path, FIRST_LINE, rule, first));
    }
    let values = step.get("env").and_then(serde_yaml_ng::Value::as_mapping).into_iter().flatten();
    for (name, value) in values {
        let rendered = value.as_str().unwrap_or_default();
        let untrusted = &policy.source.untrusted_expression_prefixes;
        if untrusted.iter().any(|prefix| rendered.contains(prefix.as_str())) {
            let rule = "untrusted-expression-reaches-a-shell-value";
            violations.push(Violation::at(
                path,
                FIRST_LINE,
                rule,
                name.as_str().unwrap_or_default(),
            ));
        }
    }
}

/// Refuses every permission one job holds beyond least privilege.
fn check_workflow_permissions(
    policy: &LoadedPolicy,
    path: &str,
    name: &str,
    job: &serde_yaml_ng::Value,
    violations: &mut Vec<Violation>,
) {
    let Some(permissions) = job.get("permissions").and_then(serde_yaml_ng::Value::as_mapping)
    else {
        violations.push(Violation::at(
            path,
            FIRST_LINE,
            "job-declares-no-explicit-permissions",
            name,
        ));
        return;
    };
    let attesting = name == policy.source.release_attestation_job;
    for (permission, value) in permissions {
        let permission = permission.as_str().unwrap_or_default();
        let granted = value.as_str().unwrap_or_default();
        if granted == READ_PERMISSION || granted == NO_PERMISSION {
            continue;
        }
        let attestation = &policy.source.release_attestation_permissions;
        if !(attesting && attestation.iter().any(|allowed| allowed == permission)) {
            let rule = "job-holds-a-permission-beyond-least-privilege";
            violations.push(Violation::at(path, FIRST_LINE, rule, format!("{name}: {permission}")));
        }
    }
}

/// Refuses every rule one workflow document breaks.
///
/// The document is parsed rather than matched, so a rule cannot be evaded by
/// spelling one structure a different way.
#[must_use]
pub fn check(policy: &LoadedPolicy, path: &str, text: &str) -> Vec<Violation> {
    let mut violations = check_line_count(policy, path, text);
    let document: serde_yaml_ng::Value = match serde_yaml_ng::from_str(text) {
        Ok(document) => document,
        Err(failure) => {
            violations.push(Violation {
                path: path.to_owned(),
                line: failure.location().map_or(FIRST_LINE, |location| location.line()),
                rule: "source-is-not-parseable".to_owned(),
                symbol: failure.to_string(),
            });
            return violations;
        }
    };
    let Some(jobs) = document.get("jobs").and_then(serde_yaml_ng::Value::as_mapping) else {
        violations.push(Violation::at(path, FIRST_LINE, "workflow-declares-no-job", "jobs"));
        return violations;
    };
    for (name, job) in jobs {
        let name = name.as_str().unwrap_or_default();
        check_workflow_permissions(policy, path, name, job, &mut violations);
        for step in
            job.get("steps").and_then(serde_yaml_ng::Value::as_sequence).into_iter().flatten()
        {
            check_workflow_step(policy, path, step, &mut violations);
        }
    }
    violations.sort();
    violations
}
