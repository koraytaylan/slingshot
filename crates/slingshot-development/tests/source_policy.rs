//! Assertions for the repository source policy.
//!
//! Every rule is proved against a hand-authored sample rather than against the
//! repository alone, so an accepted sample and the sample one step beyond it
//! both have to behave. The repository is then checked with the same checker.

use std::path::{Path, PathBuf};
use std::process::Command;

use slingshot_development::source_policy::{self, LoadedPolicy, SourceKind, Violation};

/// Directory holding the hand-authored samples.
const FIXTURE_DIRECTORY: &str = "crates/slingshot-development/tests/fixtures/source-policy";

/// Samples that must produce no violation, paired with the name they are
/// checked under so the checker classifies them the way they are meant.
const ACCEPTED: &[(&str, &str)] = &[
    ("accepted-line-ceiling.rs", "probe.rs"),
    ("accepted-external-interface.rs", "probe.rs"),
    ("accepted-complexity-ceiling.rs", "probe.rs"),
    ("accepted-unchecked-word-in-prose.rs", "probe.rs"),
    ("accepted-named-numeric-value.rs", "probe.rs"),
    ("accepted-workflow.yml", ".github/workflows/probe.yml"),
    ("accepted-attestation-workflow.yml", ".github/workflows/probe.yml"),
    ("accepted-script", "scripts/probe"),
    ("accepted-migration.sql", "probe.sql"),
    ("accepted-product-prose.md", "probe.md"),
];

/// Samples that must produce the named rule, with the name they are checked
/// under.
const REJECTED: &[(&str, &str, &str)] = &[
    ("rejected-one-line-beyond-the-ceiling.rs", "probe.rs", "file-is-longer-than-the-ceiling"),
    ("rejected-aliased-external-interface.rs", "probe.rs", "declared-name-is-not-spelled-in-full"),
    ("rejected-inherent-lookalike.rs", "probe.rs", "declared-name-is-not-spelled-in-full"),
    (
        "rejected-project-owned-trait-lookalike.rs",
        "probe.rs",
        "declared-name-is-not-spelled-in-full",
    ),
    ("rejected-abbreviated-body-local.rs", "probe.rs", "declared-name-is-not-spelled-in-full"),
    ("rejected-abbreviated-declaration.rs", "probe.rs", "declared-name-is-not-spelled-in-full"),
    ("rejected-complexity-beyond-the-ceiling.rs", "probe.rs", "function-branches-too-many-ways"),
    ("rejected-unchecked-block.rs", "probe.rs", "unchecked-block"),
    ("rejected-unchecked-function.rs", "probe.rs", "unchecked-function"),
    ("rejected-unchecked-contract.rs", "probe.rs", "unchecked-contract"),
    ("rejected-unchecked-implementation.rs", "probe.rs", "unchecked-implementation"),
    ("rejected-foreign-declaration-block.rs", "probe.rs", "foreign-declaration-block"),
    ("rejected-undocumented-export.rs", "probe.rs", "exported-item-is-not-documented"),
    (
        "rejected-fallible-without-a-failure-section.rs",
        "probe.rs",
        "fallible-interface-omits-its-failure-section",
    ),
    ("rejected-placeholder-body.rs", "probe.rs", "placeholder-stands-in-for-behavior"),
    ("rejected-unnamed-numeric-value.rs", "probe.rs", "numeric-value-carries-no-name"),
    (
        "rejected-tag-pinned-action.yml",
        ".github/workflows/probe.yml",
        "action-is-not-pinned-to-a-full-commit",
    ),
    (
        "rejected-persisted-credential.yml",
        ".github/workflows/probe.yml",
        "checkout-persists-its-credential",
    ),
    (
        "rejected-expression-reaching-a-shell.yml",
        ".github/workflows/probe.yml",
        "workflow-expression-reaches-a-shell",
    ),
    (
        "rejected-untrusted-shell-value.yml",
        ".github/workflows/probe.yml",
        "untrusted-expression-reaches-a-shell-value",
    ),
    (
        "rejected-write-permission-outside-attestation.yml",
        ".github/workflows/probe.yml",
        "job-holds-a-permission-beyond-least-privilege",
    ),
    (
        "rejected-attestation-outside-its-job.yml",
        ".github/workflows/probe.yml",
        "job-holds-a-permission-beyond-least-privilege",
    ),
    (
        "rejected-workflow-without-permissions.yml",
        ".github/workflows/probe.yml",
        "job-declares-no-explicit-permissions",
    ),
    (
        "rejected-abbreviated-script-function",
        "scripts/probe",
        "declared-name-is-not-spelled-in-full",
    ),
    ("rejected-complex-script", "scripts/probe", "function-branches-too-many-ways"),
    (
        "rejected-abbreviated-migration-column.sql",
        "probe.sql",
        "declared-name-is-not-spelled-in-full",
    ),
    ("rejected-marker-in-prose.md", "probe.md", "unfinished-work-marker"),
    ("rejected-planning-heading-in-prose.md", "probe.md", "planning-heading-in-product-prose"),
];

/// Returns the workspace root directory.
fn workspace_root() -> PathBuf {
    slingshot_development::locate_workspace_root(Path::new(env!("CARGO_MANIFEST_DIR")))
        .expect("the development crate lives inside the workspace")
}

/// Loads the committed policy documents.
fn policy() -> LoadedPolicy {
    LoadedPolicy::load(&workspace_root()).expect("the policy documents are readable")
}

/// Checks one sample under the name that classifies it.
fn check_sample(policy: &LoadedPolicy, sample: &str, checked_as: &str) -> Vec<Violation> {
    let root = tempfile::tempdir().expect("a temporary root is created");
    let target = root.path().join(checked_as);
    std::fs::create_dir_all(target.parent().expect("the sample has a directory"))
        .expect("the sample directory is created");
    let source = workspace_root().join(FIXTURE_DIRECTORY).join(sample);
    std::fs::copy(&source, &target).unwrap_or_else(|failure| {
        panic!("{} could not be copied: {failure}", source.display());
    });
    source_policy::check_file(policy, root.path(), checked_as).expect("the sample is readable")
}

#[test]
fn every_accepted_sample_produces_no_violation() {
    let policy = policy();
    for (sample, checked_as) in ACCEPTED {
        let violations = check_sample(&policy, sample, checked_as);
        assert_eq!(violations, Vec::new(), "{sample}");
    }
}

#[test]
fn every_refused_sample_produces_its_named_rule() {
    let policy = policy();
    for (sample, checked_as, rule) in REJECTED {
        let violations = check_sample(&policy, sample, checked_as);
        assert!(
            violations.iter().any(|violation| violation.rule == *rule),
            "{sample} reported {violations:?}"
        );
    }
}

#[test]
fn the_closed_interface_table_exempts_only_a_literal_qualified_path() {
    let policy = policy();
    let exempt = check_sample(&policy, "accepted-external-interface.rs", "probe.rs");
    assert_eq!(exempt, Vec::new(), "a literal qualified path is exempt");
    for sample in [
        "rejected-aliased-external-interface.rs",
        "rejected-inherent-lookalike.rs",
        "rejected-project-owned-trait-lookalike.rs",
        "rejected-abbreviated-body-local.rs",
    ] {
        let violations = check_sample(&policy, sample, "probe.rs");
        assert!(
            violations
                .iter()
                .any(|violation| violation.symbol == "fmt" || violation.symbol == "cfg"),
            "{sample} reported {violations:?}"
        );
    }
    assert!(
        policy.interfaces.interface.iter().all(|interface| interface.path.starts_with("::")),
        "every exempt path is fully qualified"
    );
}

#[test]
fn the_semantic_documentation_questions_stay_a_review_checklist() {
    let policy = policy();
    assert!(!policy.documentation.review_checklist.is_empty(), "the checklist has entries");
    for entry in &policy.documentation.review_checklist {
        assert!(!entry.trim().is_empty(), "every checklist entry says something");
    }
    let narrating = "//! This module declares a struct.\n\n/// Returns the value.\npub fn read() -> usize {\n    0\n}\n";
    let root = tempfile::tempdir().expect("a temporary root is created");
    std::fs::write(root.path().join("probe.rs"), narrating).expect("the sample is written");
    let violations = source_policy::check_file(&policy, root.path(), "probe.rs")
        .expect("the sample is readable");
    assert_eq!(violations, Vec::new(), "narrating prose is a review question, not a rule");
}

#[test]
fn the_checker_examines_only_the_kinds_the_policy_names() {
    assert_eq!(source_policy::classify("crates/probe/src/lib.rs"), Some(SourceKind::Rust));
    assert_eq!(
        source_policy::classify(".github/workflows/quality.yml"),
        Some(SourceKind::Workflow)
    );
    assert_eq!(source_policy::classify("scripts/quality"), Some(SourceKind::Script));
    assert_eq!(source_policy::classify("migrations/0001.sql"), Some(SourceKind::Migration));
    assert_eq!(source_policy::classify("Cargo.toml"), Some(SourceKind::Manifest));
    assert_eq!(source_policy::classify("README.md"), Some(SourceKind::Prose));
    assert_eq!(source_policy::classify("policy/source-policy.toml"), None);
}

#[test]
fn the_repository_follows_every_rule_through_its_own_command() {
    let violations =
        source_policy::check_repository(&workspace_root()).expect("the repository reads");
    assert_eq!(violations, Vec::new());
    let produced = Command::new(slingshot_development::cargo_executable())
        .current_dir(workspace_root())
        .args([
            "run",
            "--locked",
            "--quiet",
            "--package",
            "slingshot-development",
            "--",
            "source-policy",
        ])
        .output()
        .expect("the repository command runs");
    assert!(produced.status.success(), "{}", String::from_utf8_lossy(&produced.stdout));
}
