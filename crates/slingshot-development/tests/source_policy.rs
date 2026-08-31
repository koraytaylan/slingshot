//! Assertions for the repository source policy.
//!
//! Every rule is proved against a hand-authored sample rather than against the
//! repository alone, so an accepted sample and the sample one step beyond it
//! both have to behave. The repository is then checked with the same checker.

use std::collections::BTreeSet;
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
    ("accepted-workspace-shapes.rs", "probe.rs"),
    ("accepted-structural-numbers.rs", "probe.rs"),
    ("accepted-expectation-with-a-reason.rs", "probe.rs"),
    ("accepted-script-named-quantity", "scripts/probe"),
    ("accepted-script-line-ceiling", "scripts/probe"),
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
    (
        "rejected-single-character-type-parameter.rs",
        "probe.rs",
        "declared-name-is-not-spelled-in-full",
    ),
    ("rejected-single-character-lifetime.rs", "probe.rs", "declared-name-is-not-spelled-in-full"),
    ("rejected-suppression-marker.rs", "probe.rs", "suppression-marker-silences-a-rule"),
    ("rejected-unexplained-expectation.rs", "probe.rs", "suppression-marker-silences-a-rule"),
    ("rejected-suppression-in-prose.md", "probe.md", "suppression-marker-silences-a-rule"),
    ("rejected-redeclared-contract-limit.rs", "probe.rs", "contract-value-is-declared-again"),
    ("rejected-redeclared-contract-identifier.rs", "probe.rs", "contract-value-is-declared-again"),
    ("rejected-asynchronous-timing-number.rs", "probe.rs", "numeric-value-carries-no-name"),
    ("rejected-status-code-number.rs", "probe.rs", "numeric-value-carries-no-name"),
    ("rejected-retry-schedule-number.rs", "probe.rs", "numeric-value-carries-no-name"),
    ("rejected-collection-bound-number.rs", "probe.rs", "numeric-value-carries-no-name"),
    ("rejected-test-iteration-number.rs", "probe.rs", "numeric-value-carries-no-name"),
    ("rejected-empty-export-documentation.rs", "probe.rs", "exported-item-is-not-documented"),
    ("rejected-script-unnamed-quantity", "scripts/probe", "numeric-value-carries-no-name"),
    (
        "rejected-script-one-line-beyond-the-ceiling",
        "scripts/probe",
        "file-is-longer-than-the-ceiling",
    ),
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

/// Where the review the checklist asks for is recorded.
fn review_record(policy: &LoadedPolicy) -> String {
    let path = workspace_root().join(&policy.documentation.review_record);
    std::fs::read_to_string(&path)
        .unwrap_or_else(|failure| panic!("{} could not be read: {failure}", path.display()))
}

#[test]
fn the_checklist_inventory_is_closed_and_each_subject_is_named_once() {
    let policy = policy();
    let subjects = &policy.documentation.review_subjects;
    assert!(!subjects.is_empty(), "the inventory names something");
    for (subject, entry) in subjects {
        let named = policy
            .documentation
            .review_checklist
            .iter()
            .filter(|held| held.as_str() == entry.as_str())
            .count();
        assert_eq!(named, 1, "{subject} is covered by {named} entries rather than one");
    }
    let covered: BTreeSet<&String> = subjects.values().collect();
    assert_eq!(
        covered.len(),
        subjects.len(),
        "two subjects are covered by one entry, so one of them is nobody's"
    );
}

#[test]
fn the_review_record_is_present_and_answers_every_subject() {
    let policy = policy();
    let record = review_record(&policy);
    for (subject, entry) in &policy.documentation.review_subjects {
        assert!(record.contains(entry.as_str()), "the record does not quote {subject}");
    }
    assert!(
        record.contains("judgement"),
        "the record says what it is: answers a reader gave, not answers a checker inferred"
    );
}

#[test]
fn the_checker_never_claims_to_have_judged_what_the_checklist_asks() {
    let policy = policy();
    let source = std::fs::read_to_string(
        workspace_root().join("crates/slingshot-development/src/source_policy.rs"),
    )
    .expect("the checker is readable");
    for entry in &policy.documentation.review_checklist {
        assert!(
            !source.contains(entry.as_str()),
            "the checker restates a review question, which reads as a claim to answer it"
        );
    }
}

#[test]
fn the_abbreviation_table_is_a_sorted_table_of_lowercase_shortenings() {
    let text = std::fs::read_to_string(
        workspace_root().join(slingshot_development::source_policy::ABBREVIATED_IDENTIFIERS_PATH),
    )
    .expect("the table is readable");
    let entries: Vec<&str> = text
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty() && !line.starts_with('#'))
        .collect();
    assert!(!entries.is_empty());
    let mut sorted = entries.clone();
    sorted.sort_unstable();
    assert_eq!(entries, sorted, "the table is sorted, so a reader can find an entry");
    let unique: BTreeSet<&&str> = entries.iter().collect();
    assert_eq!(unique.len(), entries.len(), "an entry is listed twice");
    for entry in &entries {
        assert_eq!(*entry, entry.to_lowercase(), "{entry} is compared lowercased");
        assert!(
            entry.chars().all(|held| held.is_ascii_lowercase() || held.is_ascii_digit()),
            "{entry} is one word"
        );
    }
}

#[test]
fn one_authority_declares_every_wire_visible_command_value() {
    let policy = policy();
    let root = workspace_root();
    let owning = policy.source.command_contract_directory.as_str();
    let restating: Vec<String> = source_policy::examined_paths(&policy, &root)
        .expect("the repository reads")
        .into_iter()
        .filter(|path| !path.starts_with(owning))
        .filter(|path| {
            source_policy::check_file(&policy, &root, path)
                .expect("the file reads")
                .iter()
                .any(|violation| violation.rule == "contract-value-is-declared-again")
        })
        .collect();
    assert_eq!(restating, Vec::<String>::new(), "these sources declare a contract value again");
}

#[test]
fn every_repository_owned_code_file_is_inside_the_line_ceiling() {
    let policy = policy();
    let root = workspace_root();
    let ceiling = policy.source.maximum_code_file_lines;
    let beyond: Vec<(String, usize)> = source_policy::examined_paths(&policy, &root)
        .expect("the repository reads")
        .into_iter()
        .filter_map(|path| {
            let lines = std::fs::read_to_string(root.join(&path)).ok()?.lines().count();
            (lines > ceiling).then_some((path, lines))
        })
        .collect();
    assert_eq!(
        beyond,
        Vec::new(),
        "a file past the ceiling is split rather than the ceiling raised"
    );
}
