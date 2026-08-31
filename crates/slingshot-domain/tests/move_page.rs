//! Assertions for moving a page.
//!
//! The containment table is the part worth the vectors. A destination equal to
//! the source, inside it, or immediately under it are three spellings of one
//! impossible request, and a sibling whose name merely begins the same is not
//! one of them - that boundary is where a prefix comparison would get it wrong.

use serde_json::Value;
use slingshot_domain::command::command_identity::CommandContract;
use slingshot_domain::command::move_page::{
    MovePageCommand, MovePageFailure, MovePageRefusal, MovePageResult,
};
use slingshot_domain::command::repository_path::RepositoryPath;
use slingshot_domain::command::resource_mutation::{MovedResourceResult, MutationResultFailure};

/// Commands this test reads.
const COMMANDS: &str = include_str!("fixtures/commands/move_page/commands.jsonl");

/// Containment vectors this test reads.
const CONTAINMENT: &str = include_str!("fixtures/commands/move_page/containment.jsonl");

/// Failures this test reads.
const FAILURES: &str = include_str!("fixtures/commands/move_page/failures.jsonl");

/// Source every containment vector moves from.
const SOURCE: &str = "/content/example/en/report";

/// References one adjusting move rewrote, which is more than none.
const ADJUSTED: u64 = 3;

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

/// Returns one legal move from the shared source to `destination`.
fn command(destination: &str, adjust_references: bool) -> MovePageCommand {
    MovePageCommand {
        adjust_references,
        destination_path: path(destination),
        source_path: path(SOURCE),
    }
}

#[test]
fn every_command_vector_parses_exactly_as_the_fixture_says() {
    let vectors = rows(COMMANDS);
    assert!(vectors.len() >= 6, "both reference decisions and every refusal");
    for row in &vectors {
        let document = text(row, "document");
        let note = text(row, "note");
        match (row["accepted"].as_bool(), serde_json::from_str::<MovePageCommand>(document)) {
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
fn a_destination_at_or_inside_the_source_is_refused_and_a_lookalike_sibling_is_not() {
    let vectors = rows(CONTAINMENT);
    assert!(vectors.len() >= 5, "the three containment shapes and both boundaries");
    for row in &vectors {
        let note = text(row, "note");
        let asked = command(text(row, "destination_path"), false);
        let usable = row["usable"].as_bool().expect("a verdict");
        let answered = asked.require_usable();
        if usable {
            assert_eq!(answered, Ok(()), "{note}: a usable move was refused");
        } else {
            assert_eq!(
                answered,
                Err(MutationResultFailure::DestinationInsideSource),
                "{note}: an impossible move was accepted"
            );
        }
    }
}

#[test]
fn an_adjusted_reference_count_is_accepted_at_its_bound_and_refused_one_past_it() {
    let bound = CommandContract::embedded().limit("maximum_adjusted_references");
    assert!(MovedResourceResult::new(path(SOURCE), path("/content/archive/report"), bound).is_ok());
    assert_eq!(
        MovedResourceResult::new(path(SOURCE), path("/content/archive/report"), bound + 1),
        Err(MutationResultFailure::CountTooLarge)
    );
}

#[test]
fn a_result_answers_only_the_request_that_determined_both_addresses() {
    let asked = command("/content/archive/report", true);
    let answered = MovePageResult {
        moved: MovedResourceResult::new(path(SOURCE), path("/content/archive/report"), ADJUSTED)
            .expect("a legal move"),
    };
    assert_eq!(answered.require_answers(&asked), Ok(()));
    let elsewhere = MovePageResult {
        moved: MovedResourceResult::new(path(SOURCE), path("/content/elsewhere/report"), 0)
            .expect("a legal move"),
    };
    assert_eq!(elsewhere.require_answers(&asked), Err(MutationResultFailure::NotThisRequest));
}

#[test]
fn a_move_that_adjusted_nothing_cannot_report_that_it_adjusted_something() {
    let asked = command("/content/archive/report", false);
    let answered = MovePageResult {
        moved: MovedResourceResult::new(path(SOURCE), path("/content/archive/report"), 1)
            .expect("a legal move"),
    };
    assert_eq!(
        answered.require_answers(&asked),
        Err(MutationResultFailure::NotThisRequest),
        "a move that was told not to adjust references reported adjusting one"
    );
    let none = MovePageResult {
        moved: MovedResourceResult::new(path(SOURCE), path("/content/archive/report"), 0)
            .expect("a legal move"),
    };
    assert_eq!(none.require_answers(&asked), Ok(()));
}

#[test]
fn a_reference_budget_refusal_belongs_only_to_a_request_that_asked_to_adjust() {
    let refusal = MovePageRefusal {
        destination_path: path("/content/archive/report"),
        failure: MovePageFailure::ReferenceAdjustmentBudgetExceeded,
        source_path: path(SOURCE),
    };
    assert_eq!(refusal.require_answers(&command("/content/archive/report", true)), Ok(()));
    assert_eq!(
        refusal.require_answers(&command("/content/archive/report", false)),
        Err(MutationResultFailure::NotThisRequest)
    );
}

#[test]
fn every_failure_document_carries_its_three_members_and_says_what_it_proves() {
    let vectors = rows(FAILURES);
    assert_eq!(vectors.len(), 8, "one for each category this command allows");
    for row in &vectors {
        let note = text(row, "note");
        let document = text(row, "document");
        let refusal: MovePageRefusal =
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
        serde_json::from_str::<MovePageRefusal>(
            r#"{"destination_path":"/a","failure":"source_not_found","source_path":"/b","extra":1}"#
        )
        .is_err()
    );
}
