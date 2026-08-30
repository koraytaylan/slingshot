//! Creating a page, proved resolvable after an interruption.
//!
//! The reconciliation table is the part worth the vectors. A matching receipt
//! is proof the save committed, and it stays proof after somebody else has
//! deleted the target - the commit happened, and what the repository looks like
//! now does not unmake it. Neither receipt nor target after an attempt proves
//! the save did not commit, which is the only case that permits a retry.
//! Everything else is unknown, and unknown never authorizes a retry and never
//! claims no effect.

use serde_json::Value;
use slingshot_domain::command::create_page::{
    CreatePageCommand, CreatePageRefusal, CreatePageResult, MutationCheckpoint, MutationFailure,
    PAGE_CONTENT_CHILD, PAGE_PRIMARY_NODE_TYPE, PAGE_TITLE_PROPERTY, ReconciledOutcome,
    ReconciliationEvidence, maximum_mutation_properties, maximum_mutation_success_result_bytes,
};
use slingshot_domain::command::repository_path::RepositoryPath;

/// Commands this test reads.
const COMMANDS: &str = include_str!("fixtures/commands/create_page/commands.jsonl");

/// Failures this test reads.
const FAILURES: &str = include_str!("fixtures/commands/create_page/failures.jsonl");

/// Reconciliation vectors this test reads.
const RECONCILIATION: &str = include_str!("fixtures/commands/create_page/reconciliation.jsonl");

/// Reads one row's string member.
fn text<'row>(row: &'row Value, member: &str) -> &'row str {
    row[member].as_str().unwrap_or_else(|| panic!("{member} is a string in {row}"))
}

/// Returns every row of one fixture.
fn rows(fixture: &str) -> Vec<Value> {
    fixture
        .lines()
        .map(|line| serde_json::from_str::<Value>(line).expect("every fixture line is one object"))
        .collect()
}

#[test]
fn every_command_vector_lands_where_the_fixture_says_it_does() {
    let vectors = rows(COMMANDS);
    assert!(vectors.len() >= 15, "every name shape and both property bounds");
    for row in &vectors {
        let document = text(row, "document");
        let note = text(row, "note");
        match (row["accepted"].as_bool(), serde_json::from_str::<CreatePageCommand>(document)) {
            (Some(true), Ok(command)) => {
                assert_eq!(
                    serde_json::to_string(&command).expect("a command serializes"),
                    document,
                    "{note}: rewritten differently"
                );
                assert_eq!(
                    command.target_path().map(|path| path.as_text().to_owned()),
                    Ok(text(row, "target_path").to_owned()),
                    "{note}: computed the wrong target"
                );
            }
            (Some(false), Err(_)) => (),
            (_, parsed) => panic!("{note}: the command answered {parsed:?}"),
        }
    }
}

#[test]
fn the_target_is_computed_from_the_request_rather_than_asserted_by_it() {
    let command: CreatePageCommand = serde_json::from_str(
        r#"{"page_name":"annual-report","parent_path":"/content/example/en","template_path":"/apps/example/templates/page","title":"Annual Report"}"#,
    )
    .expect("a legal command");
    let target = command.target_path().expect("a creatable child");
    assert_eq!(target.as_text(), "/content/example/en/annual-report");
    assert!(
        !serde_json::to_string(&command).expect("serializes").contains("target_path"),
        "the request does not carry a target for the result to echo back"
    );
    assert_eq!(PAGE_PRIMARY_NODE_TYPE, "cq:Page");
    assert_eq!(PAGE_CONTENT_CHILD, "jcr:content", "content goes to the content resource");
    assert_eq!(PAGE_TITLE_PROPERTY, "jcr:title");
}

#[test]
fn an_initial_property_map_cannot_argue_with_the_title_field() {
    let redefining: CreatePageCommand = serde_json::from_str(
        r#"{"initial_properties":{"jcr:title":{"cardinality":"single","value":{"type":"string","value":"Other"}}},"page_name":"report","parent_path":"/content","template_path":"/apps/t","title":"Annual Report"}"#,
    )
    .expect("the shape is legal");
    assert_eq!(
        redefining.require_title_not_redefined(),
        Err(MutationFailure::PropertyReserved),
        "two parts of one request would otherwise disagree with nothing to say which wins"
    );
    let ordinary: CreatePageCommand = serde_json::from_str(
        r#"{"initial_properties":{"dc:description":{"cardinality":"single","value":{"type":"string","value":"A report"}}},"page_name":"report","parent_path":"/content","template_path":"/apps/t","title":"Annual Report"}"#,
    )
    .expect("a legal command");
    assert_eq!(ordinary.require_title_not_redefined(), Ok(()));
}

#[test]
fn every_failure_names_the_computed_target_and_says_what_it_proves() {
    let vectors = rows(FAILURES);
    assert_eq!(vectors.len(), 8, "the eight registered categories");
    for row in &vectors {
        let document = text(row, "document");
        let note = text(row, "note");
        let refusal: CreatePageRefusal =
            serde_json::from_str(document).expect("every failure vector is a legal failure");
        assert_eq!(
            serde_json::to_string(&refusal).expect("a failure serializes"),
            document,
            "{note}: rewritten differently"
        );
        let members: Vec<String> = serde_json::from_str::<Value>(document)
            .expect("one object")
            .as_object()
            .expect("an object")
            .keys()
            .cloned()
            .collect();
        assert_eq!(members, vec!["failure".to_owned(), "target_path".to_owned()], "{note}");
        assert_eq!(
            refusal.proves_no_effect(),
            row["proves_no_effect"].as_bool().expect("every vector says what it proves"),
            "{note}"
        );
    }
}

#[test]
fn every_reconciliation_vector_resolves_the_way_the_fixture_says() {
    let vectors = rows(RECONCILIATION);
    assert!(vectors.len() >= 9, "every combination that can arise");
    for row in &vectors {
        let checkpoint: MutationCheckpoint =
            serde_json::from_value(Value::from(text(row, "checkpoint"))).expect("a legal state");
        let evidence = ReconciliationEvidence {
            target_present: row["target_present"].as_bool().expect("a Boolean"),
            matching_receipt: row["matching_receipt"].as_bool().expect("a Boolean"),
            conflicting_receipt: row["conflicting_receipt"].as_bool().expect("a Boolean"),
        };
        let expected = match text(row, "outcome") {
            "committed" => ReconciledOutcome::Committed,
            "not_committed" => ReconciledOutcome::NotCommitted,
            "unknown" => ReconciledOutcome::Unknown,
            other => {
                assert_eq!(other, "target_already_exists", "an outcome this contract has");
                ReconciledOutcome::TargetAlreadyExists
            }
        };
        assert_eq!(evidence.resolve(checkpoint), expected, "{}", text(row, "note"));
    }
}

#[test]
fn a_matching_receipt_outlives_the_target_it_created() {
    let deleted = ReconciliationEvidence {
        target_present: false,
        matching_receipt: true,
        conflicting_receipt: false,
    };
    assert_eq!(
        deleted.resolve(MutationCheckpoint::InFlight),
        ReconciledOutcome::Committed,
        "somebody deleting the page afterwards does not unmake the commit"
    );
    let orphaned = ReconciliationEvidence {
        target_present: true,
        matching_receipt: false,
        conflicting_receipt: false,
    };
    assert_eq!(
        orphaned.resolve(MutationCheckpoint::InFlight),
        ReconciledOutcome::Unknown,
        "and a target with no receipt is never read as success"
    );
    assert_eq!(
        orphaned.resolve(MutationCheckpoint::NotStarted),
        ReconciledOutcome::TargetAlreadyExists,
        "though before any attempt the same target is simply already there"
    );
}

#[test]
fn a_result_or_a_failure_from_another_request_is_rejected() {
    let asked: CreatePageCommand = serde_json::from_str(
        r#"{"page_name":"annual-report","parent_path":"/content/example/en","template_path":"/apps/t","title":"Annual Report"}"#,
    )
    .expect("a legal command");
    let own = CreatePageResult { target_path: asked.target_path().expect("a creatable child") };
    assert_eq!(own.require_answers(&asked), Ok(()));
    let elsewhere = CreatePageResult {
        target_path: RepositoryPath::parse("/content/other/report").expect("a legal path"),
    };
    assert_eq!(elsewhere.require_answers(&asked), Err(MutationFailure::NotThisRequest));

    let refusal: CreatePageRefusal = serde_json::from_str(
        r#"{"failure":"parent_not_found","target_path":"/content/other/report"}"#,
    )
    .expect("a legal refusal");
    assert_eq!(refusal.require_answers(&asked), Err(MutationFailure::NotThisRequest));
}

#[test]
fn every_named_bound_comes_from_the_manifest_rather_than_from_here() {
    let contract = slingshot_domain::command::command_identity::CommandContract::embedded();
    assert_eq!(maximum_mutation_properties(), contract.limit("maximum_mutation_properties"));
    assert_eq!(
        maximum_mutation_success_result_bytes(),
        contract.limit("maximum_mutation_success_result_bytes")
    );
}
