//! Assertions for reading a fragment as what it is.
//!
//! The variation echo rule is the interesting one. A request that named a
//! variation is answered about that variation and no other; a request that named
//! none accepts whichever variation the author read, because which one is the
//! master is the author's answer and not this contract's.

use serde_json::Value;
use slingshot_domain::command::content_fragment_element::{
    ContentFragmentFailure, ContentFragmentVariationName,
};
use slingshot_domain::command::read_content_fragment::{
    ReadContentFragmentCommand, ReadContentFragmentFailure, ReadContentFragmentRefusal,
    ReadContentFragmentResult,
};
use slingshot_domain::command::repository_path::RepositoryPath;

/// Commands this test reads.
const COMMANDS: &str = include_str!("fixtures/commands/read_content_fragment/commands.jsonl");

/// Failures this test reads.
const FAILURES: &str = include_str!("fixtures/commands/read_content_fragment/failures.jsonl");

/// Fragment every vector addresses.
const FRAGMENT: &str = "/content/dam/example/fragments/offer";

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

/// Returns one request, over the variation it names.
fn command(variation: Option<&str>) -> ReadContentFragmentCommand {
    ReadContentFragmentCommand {
        fragment_path: path(FRAGMENT),
        variation_name: variation
            .map(|name| ContentFragmentVariationName::parse(name).expect("a legal name")),
    }
}

/// Returns one result naming `variation`.
fn result(variation: &str) -> ReadContentFragmentResult {
    serde_json::from_str(&format!(
        "{{\"elements\":{{\"title\":\"Spring offer\"}},\"model_path\":\"/conf/example/settings/dam/cfm/models/offer\",\"repository_path\":\"{FRAGMENT}\",\"variation_name\":\"{variation}\"}}"
    ))
    .expect("a legal result")
}

#[test]
fn every_command_vector_parses_exactly_as_the_fixture_says() {
    let vectors = rows(COMMANDS);
    assert!(vectors.len() >= 5, "every document shape and every refusal");
    for row in &vectors {
        let document = text(row, "document");
        let note = text(row, "note");
        match (
            row["accepted"].as_bool(),
            serde_json::from_str::<ReadContentFragmentCommand>(document),
        ) {
            (Some(true), Ok(parsed)) => {
                assert_eq!(
                    serde_json::to_string(&parsed).expect("a command serializes"),
                    document,
                    "{note}: rewritten differently"
                );
            }
            (Some(false), Err(_)) => (),
            (_, parsed) => panic!("{note}: the command answered {parsed:?}"),
        }
    }
}

#[test]
fn a_request_that_named_a_variation_is_answered_about_that_one() {
    assert_eq!(result("mobile").require_answers(&command(Some("mobile"))), Ok(()));
    assert_eq!(
        result("web").require_answers(&command(Some("mobile"))),
        Err(ContentFragmentFailure::NotThisRequest)
    );
}

#[test]
fn a_request_that_named_none_accepts_whichever_variation_the_author_read() {
    for variation in ["master", "web", "mobile"] {
        assert_eq!(result(variation).require_answers(&command(None)), Ok(()));
    }
}

#[test]
fn a_result_naming_another_fragment_is_refused() {
    let elsewhere: ReadContentFragmentResult = serde_json::from_str(
        r#"{"elements":{"title":"Spring offer"},"model_path":"/conf/example/settings/dam/cfm/models/offer","repository_path":"/content/dam/example/fragments/other","variation_name":"master"}"#,
    )
    .expect("a legal result");
    assert_eq!(
        elsewhere.require_answers(&command(None)),
        Err(ContentFragmentFailure::NotThisRequest)
    );
}

#[test]
fn a_result_round_trips_and_omits_an_absent_title_rather_than_nulling_it() {
    let answered = result("master");
    let written = serde_json::to_string(&answered).expect("a result serializes");
    assert!(!written.contains("title\":null"), "an absent title was serialized as null");
    let read: ReadContentFragmentResult = serde_json::from_str(&written).expect("a result parses");
    assert_eq!(read, answered);
}

#[test]
fn every_failure_document_carries_its_two_members_and_names_this_request() {
    let vectors = rows(FAILURES);
    assert_eq!(vectors.len(), 5, "one for each category this command allows");
    for row in &vectors {
        let note = text(row, "note");
        let document = text(row, "document");
        let refusal: ReadContentFragmentRefusal =
            serde_json::from_str(document).unwrap_or_else(|failure| panic!("{note}: {failure}"));
        assert_eq!(
            serde_json::to_string(&refusal).expect("a refusal serializes"),
            document,
            "{note}: rewritten differently"
        );
        assert_eq!(refusal.require_answers(&command(Some("mobile"))), Ok(()), "{note}");
        // A missing variation belongs to the request that named one; every
        // other category answers a request about the master just as well, and
        // that half has to be asserted or a later tightening would lose it.
        let master = refusal.require_answers(&command(None));
        let sought = document.contains("variation_not_found");
        assert_eq!(master.is_err(), sought, "{note}: the master request was answered wrongly");
    }
    let elsewhere = ReadContentFragmentRefusal {
        failure: ReadContentFragmentFailure::FragmentNotFound,
        fragment_path: path("/content/dam/example/fragments/other"),
    };
    assert_eq!(
        elsewhere.require_answers(&command(None)),
        Err(ContentFragmentFailure::NotThisRequest)
    );
    let missing = ReadContentFragmentRefusal {
        failure: ReadContentFragmentFailure::VariationNotFound,
        fragment_path: path(FRAGMENT),
    };
    assert_eq!(
        missing.require_answers(&command(None)),
        Err(ContentFragmentFailure::NotThisRequest),
        "a request about the master was answered with a missing variation"
    );
}
