//! Version-two page search results preserve explicit progress and nearest-page scope.

use serde_json::{Value, json};
use slingshot_domain::command::find_pages_containing_phrase::FindPagesContainingPhraseResult;
use slingshot_domain::command::find_pages_using_components::{
    FindPagesUsingComponentsCommand, FindPagesUsingComponentsResult,
};

/// Both page searches share the incremental result invariants.
const COMMANDS: &[&str] = &["find_pages_containing_phrase", "find_pages_using_components"];

/// Whether the typed result accepts this document, independently of its generated schema.
fn accepts(command: &str, document: &Value) -> bool {
    match command {
        "find_pages_containing_phrase" => {
            serde_json::from_value::<FindPagesContainingPhraseResult>(document.clone()).is_ok()
        }
        "find_pages_using_components" => {
            serde_json::from_value::<FindPagesUsingComponentsResult>(document.clone()).is_ok()
        }
        _ => panic!("the fixture selects a declared page search"),
    }
}

/// A complete empty response proves exhaustion even when the search has no matches.
fn complete() -> Value {
    json!({"matches": [], "complete": true, "examined_nodes": 0})
}

#[test]
fn both_searches_accept_complete_empty_and_empty_continuation_pages() {
    let partial = json!({"matches": [], "complete": false, "examined_nodes": 1,
        "next_continuation_token": "synthetic-opaque-token"});
    COMMANDS.iter().for_each(|command| {
        assert!(accepts(command, &complete()), "{command} must prove empty completion");
        assert!(accepts(command, &partial), "{command} must retain empty partial progress");
    });
}

#[test]
fn both_searches_accept_unique_repository_provider_order_without_sorting() {
    let document = json!({"matches": [
        {"repository_path": "/content/synthetic-search/z"},
        {"repository_path": "/content/synthetic-search/a"}
    ], "complete": true, "examined_nodes": 2});
    COMMANDS.iter().for_each(|command| {
        assert!(accepts(command, &document), "{command} must retain provider order");
    });
}

#[test]
fn both_searches_refuse_legacy_results_without_explicit_progress() {
    COMMANDS.iter().for_each(|command| {
        assert!(!accepts(command, &json!({"matches": []})), "{command} accepted a legacy result");
        assert!(!accepts(command, &json!({"matches": [], "complete": true})));
        assert!(!accepts(command, &json!({"matches": [], "examined_nodes": 0})));
    });
}

#[test]
fn both_searches_refuse_inconsistent_completion_and_repeated_paths() {
    let mut with_successor = complete();
    with_successor["next_continuation_token"] = json!("synthetic-opaque-token");
    let partial_without_successor = json!({"matches": [], "complete": false, "examined_nodes": 1});
    let repeated = json!({"matches": [
        {"repository_path": "/content/synthetic-search/page"},
        {"repository_path": "/content/synthetic-search/page"}
    ], "complete": true, "examined_nodes": 2});
    COMMANDS.iter().for_each(|command| {
        assert!(!accepts(command, &with_successor));
        assert!(!accepts(command, &partial_without_successor));
        assert!(!accepts(command, &repeated));
    });
}

#[test]
fn both_searches_refuse_counts_that_cannot_cover_the_returned_matches() {
    let document = json!({"matches": [{"repository_path": "/content/synthetic-search/page"}],
        "complete": true, "examined_nodes": 0});
    COMMANDS.iter().for_each(|command| assert!(!accepts(command, &document)));
}

#[test]
fn component_results_admit_the_containing_page_above_an_inside_page_anchor() {
    let command: FindPagesUsingComponentsCommand = serde_json::from_value(json!({
        "root_path": "/content/synthetic-search/page/jcr:content/text",
        "resource_types": ["synthetic/components/text"], "match_mode": "any"
    }))
    .expect("a canonical inside-page anchor");
    let result: FindPagesUsingComponentsResult = serde_json::from_value(json!({
        "matches": [{"repository_path": "/content/synthetic-search/page"}],
        "complete": true, "examined_nodes": 1
    }))
    .expect("a complete provider-ordered component page");
    assert!(result.require_answers(&command).is_ok());
}

#[test]
fn component_results_still_refuse_an_unrelated_page_outside_the_anchor() {
    let command: FindPagesUsingComponentsCommand = serde_json::from_value(json!({
        "root_path": "/content/synthetic-search/page/jcr:content/text",
        "resource_types": ["synthetic/components/text"], "match_mode": "any"
    }))
    .expect("a canonical inside-page anchor");
    let result: FindPagesUsingComponentsResult = serde_json::from_value(json!({
        "matches": [{"repository_path": "/content/synthetic-other/page"}],
        "complete": true, "examined_nodes": 1
    }))
    .expect("a structurally valid result before request correlation");
    assert!(result.require_answers(&command).is_err());
}
