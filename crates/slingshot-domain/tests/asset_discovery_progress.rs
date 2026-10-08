//! Independent version-two asset result invariants, retaining every metadata field constraint.

use serde_json::{Value, json};
use slingshot_domain::command::command_identity::CommandContract;
use slingshot_domain::command::find_assets_by_metadata::FindAssetsByMetadataResult;

/// Whether the typed result accepts the document, independently of generated schemas.
fn accepts(document: Value) -> bool {
    serde_json::from_value::<FindAssetsByMetadataResult>(document).is_ok()
}

#[test]
fn complete_empty_pages_and_empty_partial_pages_preserve_explicit_progress() {
    assert!(accepts(json!({"matches":[], "complete":true, "examined_nodes":0})));
    assert!(accepts(json!({"matches":[], "complete":false, "examined_nodes":1,
        "next_continuation_token":"synthetic-asset-continuation"})));
}

#[test]
fn asset_rows_retain_provider_order_and_canonical_metadata() {
    assert!(accepts(json!({"matches":[
        {"repository_path":"/content/dam/synthetic-assets/z", "byte_length":9,
            "media_format":"application/octet-stream", "tags":["synthetic-first", "synthetic-second"]},
        {"repository_path":"/content/dam/synthetic-assets/a"}
    ], "complete":true, "examined_nodes":2})));
}

#[test]
fn legacy_results_cannot_impersonate_explicit_progress() {
    assert!(!accepts(json!({"matches":[]})));
    assert!(!accepts(json!({"matches":[], "complete":true})));
    assert!(!accepts(json!({"matches":[], "examined_nodes":0})));
}

#[test]
fn completion_and_successor_must_agree() {
    assert!(!accepts(json!({"matches":[], "complete":true, "examined_nodes":0,
        "next_continuation_token":"synthetic-asset-continuation"})));
    assert!(!accepts(json!({"matches":[], "complete":false, "examined_nodes":1})));
}

#[test]
fn repeated_paths_and_noncanonical_tags_remain_refused() {
    assert!(!accepts(json!({"matches":[
        {"repository_path":"/content/dam/synthetic-assets/one"},
        {"repository_path":"/content/dam/synthetic-assets/one"}
    ], "complete":true, "examined_nodes":2})));
    for tags in [
        json!(["synthetic-second", "synthetic-first"]),
        json!(["synthetic-first", "synthetic-first"]),
    ] {
        assert!(!accepts(json!({"matches":[
            {"repository_path":"/content/dam/synthetic-assets/one", "tags":tags}
        ], "complete":true, "examined_nodes":1})));
    }
}

#[test]
fn examined_work_stays_at_the_original_limit_and_refuses_one_more() {
    let limit = CommandContract::embedded().limit("maximum_discovery_candidate_nodes");
    assert!(accepts(json!({"matches":[], "complete":false, "examined_nodes":limit,
        "next_continuation_token":"synthetic-asset-continuation"})));
    assert!(!accepts(json!({"matches":[], "complete":false, "examined_nodes":limit+1,
        "next_continuation_token":"synthetic-asset-continuation"})));
}

#[test]
fn a_result_cannot_claim_more_matches_than_examined_nodes() {
    assert!(!accepts(json!({"matches":[{"repository_path":"/content/dam/synthetic-assets/one"}],
        "complete":true, "examined_nodes":0})));
}
