//! Assertions for moving an asset.
//!
//! The containment rule is the shared one, so what is proved here is that it is
//! in force rather than that it works - it is proved once, where it lives. What
//! is this command's own is the reference decision: a move told not to adjust
//! references cannot answer with references adjusted.

use serde_json::Value;
use slingshot_domain::command::move_asset::{
    MoveAssetCommand, MoveAssetFailure, MoveAssetRefusal, MoveAssetResult,
};
use slingshot_domain::command::repository_path::RepositoryPath;
use slingshot_domain::command::resource_mutation::{MovedResourceResult, MutationResultFailure};

/// Commands this test reads.
const COMMANDS: &str = include_str!("fixtures/commands/move_asset/commands.jsonl");

/// Failures this test reads.
const FAILURES: &str = include_str!("fixtures/commands/move_asset/failures.jsonl");

/// Asset every vector moves.
const SOURCE: &str = "/content/dam/example/logo.png";

/// Where it goes.
const DESTINATION: &str = "/content/dam/archive/logo.png";

/// References one adjusting move rewrote, which is more than none.
const ADJUSTED: u64 = 2;

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

/// Returns one legal path.
fn path(value: &str) -> RepositoryPath {
    RepositoryPath::parse(value).expect("a legal path")
}

/// Returns one legal request over `adjust_references`.
fn command(adjust_references: bool) -> MoveAssetCommand {
    MoveAssetCommand {
        adjust_references,
        destination_path: path(DESTINATION),
        source_path: path(SOURCE),
    }
}

#[test]
fn every_command_vector_parses_exactly_as_the_fixture_says() {
    let vectors = rows(COMMANDS);
    assert!(vectors.len() >= 4, "every document shape and every refusal");
    for row in &vectors {
        let document = text(row, "document");
        let note = text(row, "note");
        match (row["accepted"].as_bool(), serde_json::from_str::<MoveAssetCommand>(document)) {
            (Some(true), Ok(parsed)) => {
                assert_eq!(
                    serde_json::to_string(&parsed).expect("a command serializes"),
                    document,
                    "{note}: rewritten differently"
                );
                assert_eq!(parsed.require_usable(), Ok(()), "{note}: refused as unusable");
            }
            (Some(false), Err(_)) => (),
            (_, parsed) => panic!("{note}: the command answered {parsed:?}"),
        }
    }
}

#[test]
fn the_shared_containment_rule_is_in_force_here() {
    let inside = MoveAssetCommand {
        adjust_references: false,
        destination_path: path("/content/dam/example/logo.png/rendition"),
        source_path: path(SOURCE),
    };
    assert_eq!(inside.require_usable(), Err(MutationResultFailure::DestinationInsideSource));
    assert_eq!(command(false).require_usable(), Ok(()));
}

#[test]
fn a_move_that_adjusted_nothing_cannot_report_that_it_adjusted_something() {
    let asked = command(false);
    let answered = MoveAssetResult {
        moved: MovedResourceResult::new(path(SOURCE), path(DESTINATION), ADJUSTED)
            .expect("a legal move"),
    };
    assert_eq!(answered.require_answers(&asked), Err(MutationResultFailure::NotThisRequest));
    let none = MoveAssetResult {
        moved: MovedResourceResult::new(path(SOURCE), path(DESTINATION), 0).expect("a legal move"),
    };
    assert_eq!(none.require_answers(&asked), Ok(()));
    assert_eq!(answered.require_answers(&command(true)), Ok(()));
}

#[test]
fn a_reference_budget_refusal_belongs_only_to_a_request_that_asked_to_adjust() {
    let refusal = MoveAssetRefusal {
        destination_path: path(DESTINATION),
        failure: MoveAssetFailure::ReferenceAdjustmentBudgetExceeded,
        source_path: path(SOURCE),
    };
    assert_eq!(refusal.require_answers(&command(true)), Ok(()));
    assert_eq!(
        refusal.require_answers(&command(false)),
        Err(MutationResultFailure::NotThisRequest)
    );
}

#[test]
fn every_failure_document_carries_its_members_and_says_what_it_proves() {
    let vectors = rows(FAILURES);
    assert_eq!(vectors.len(), 8, "one for each category this command allows");
    for row in &vectors {
        let note = text(row, "note");
        let document = text(row, "document");
        let refusal: MoveAssetRefusal =
            serde_json::from_str(document).unwrap_or_else(|failure| panic!("{note}: {failure}"));
        assert_eq!(
            serde_json::to_string(&refusal).expect("a refusal serializes"),
            document,
            "{note}: rewritten differently"
        );
        assert_eq!(
            refusal.proves_no_effect(),
            row["proves_no_effect"].as_bool().expect("a verdict"),
            "{note}"
        );
    }
    assert!(
        serde_json::from_str::<MoveAssetRefusal>(
            r#"{"destination_path":"/a","failure":"source_not_found","source_path":"/b","extra":1}"#
        )
        .is_err(),
        "a surplus member was accepted"
    );
}
