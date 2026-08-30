//! Adding a component, proved to compute its own target and to refuse guessing
//! about order.
//!
//! Two refusals carry this command. A descendant address that begins with the
//! content segment would produce `jcr:content/jcr:content/...` - a mistake that
//! reads as reasonable and creates content nobody will find - so it is refused
//! rather than accepted and appended to. And a parent whose type does not report
//! orderable children is refused outright, because "appended last" would be a
//! claim the repository never made.

use serde_json::Value;
use slingshot_domain::command::add_component::{
    AddComponentCommand, AddComponentRefusal, AddComponentResult, COMPONENT_RESOURCE_TYPE_PROPERTY,
    PageContentParent,
};
use slingshot_domain::command::create_page::MutationFailure;
use slingshot_domain::command::repository_path::{RepositoryPath, RepositoryRelativePath};

/// Commands this test reads.
const COMMANDS: &str = include_str!("fixtures/commands/add_component/commands.jsonl");

/// Duplicate-segment vectors this test reads.
const DUPLICATES: &str = include_str!("fixtures/commands/add_component/duplicates.jsonl");

/// Failures this test reads.
const FAILURES: &str = include_str!("fixtures/commands/add_component/failures.jsonl");

/// Orderability vectors this test reads.
const ORDERABLE: &str = include_str!("fixtures/commands/add_component/orderable.jsonl");

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
    assert!(vectors.len() >= 13, "every address shape and every refusal");
    for row in &vectors {
        let document = text(row, "document");
        let note = text(row, "note");
        match (row["accepted"].as_bool(), serde_json::from_str::<AddComponentCommand>(document)) {
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
fn the_target_is_the_page_its_content_resource_and_the_address_below_it() {
    let command: AddComponentCommand = serde_json::from_str(
        r#"{"component_name":"text","content_parent":"par/columns[2]","page_path":"/content/example/en","resource_type":"example/components/text"}"#,
    )
    .expect("a legal command");
    assert_eq!(
        command.target_path().expect("a creatable child").as_text(),
        "/content/example/en/jcr:content/par/columns[2]/text",
        "the sibling index in the address survives into the target"
    );
    assert_eq!(AddComponentCommand::required_page_primary_node_type(), "cq:Page");
}

#[test]
fn a_descendant_address_cannot_repeat_the_content_segment() {
    for row in &rows(DUPLICATES) {
        let parent = PageContentParent::Descendant(
            RepositoryRelativePath::parse(text(row, "parent")).expect("a legal address"),
        );
        assert_eq!(
            parent.repeats_content_segment(),
            row["repeats"].as_bool().expect("every vector states its verdict"),
            "{}",
            text(row, "note")
        );
    }
    let root: AddComponentCommand = serde_json::from_str(
        r#"{"component_name":"text","content_parent":"content_root","page_path":"/content/a","resource_type":"example/components/text"}"#,
    )
    .expect("a legal command");
    assert_eq!(root.content_parent.descendant(), None, "the content resource itself");
    assert!(!root.content_parent.repeats_content_segment());
}

#[test]
fn a_property_map_cannot_override_the_resource_type_field() {
    let overriding: AddComponentCommand = serde_json::from_str(
        r#"{"component_name":"text","content_parent":"content_root","page_path":"/content/a","properties":{"sling:resourceType":{"cardinality":"single","value":{"type":"string","value":"example/components/other"}}},"resource_type":"example/components/text"}"#,
    )
    .expect("the shape is legal");
    assert_eq!(
        overriding.require_resource_type_not_overridden(),
        Err(MutationFailure::PropertyReserved),
        "two parts of one request would otherwise disagree"
    );
    let ordinary: AddComponentCommand = serde_json::from_str(
        r#"{"component_name":"text","content_parent":"content_root","page_path":"/content/a","properties":{"text":{"cardinality":"single","value":{"type":"string","value":"Hello"}}},"resource_type":"example/components/text"}"#,
    )
    .expect("a legal command");
    assert_eq!(ordinary.require_resource_type_not_overridden(), Ok(()));
    assert_eq!(COMPONENT_RESOURCE_TYPE_PROPERTY, "sling:resourceType");
}

#[test]
fn a_non_orderable_parent_is_refused_rather_than_appended_to() {
    for row in &rows(ORDERABLE) {
        assert_eq!(
            row["orderable"].as_bool().expect("a Boolean"),
            row["accepted"].as_bool().expect("a Boolean"),
            "{}",
            text(row, "note")
        );
    }
    let refusal: AddComponentRefusal = serde_json::from_str(
        r#"{"failure":"parent_not_orderable","target_path":"/content/a/jcr:content/text"}"#,
    )
    .expect("a legal refusal");
    assert!(
        refusal.proves_no_effect(),
        "the refusal happens before any mutation, so it changed neither content nor order"
    );
}

#[test]
fn every_failure_names_the_computed_target_and_says_what_it_proves() {
    let vectors = rows(FAILURES);
    assert_eq!(vectors.len(), 9, "the nine registered categories");
    for row in &vectors {
        let document = text(row, "document");
        let note = text(row, "note");
        let refusal: AddComponentRefusal =
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
fn a_result_or_a_failure_from_another_request_is_rejected() {
    let asked: AddComponentCommand = serde_json::from_str(
        r#"{"component_name":"text","content_parent":"par","page_path":"/content/example/en","resource_type":"example/components/text"}"#,
    )
    .expect("a legal command");
    let own = AddComponentResult { target_path: asked.target_path().expect("a creatable child") };
    assert_eq!(own.require_answers(&asked), Ok(()));
    let elsewhere = AddComponentResult {
        target_path: RepositoryPath::parse("/content/other/jcr:content/text")
            .expect("a legal path"),
    };
    assert_eq!(elsewhere.require_answers(&asked), Err(MutationFailure::NotThisRequest));

    let refusal: AddComponentRefusal = serde_json::from_str(
        r#"{"failure":"page_not_found","target_path":"/content/other/jcr:content/text"}"#,
    )
    .expect("a legal refusal");
    assert_eq!(refusal.require_answers(&asked), Err(MutationFailure::NotThisRequest));
}
