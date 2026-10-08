//! Incremental discovery pages distinguish partial progress from completed enumeration.

use serde::de::DeserializeOwned;
use serde_json::{Value, json};
use slingshot_domain::command::list_component_definitions::ListComponentDefinitionsResult;
use slingshot_domain::command::list_components::ListComponentsResult;
use slingshot_domain::command::list_content_fragments::ListContentFragmentsResult;
use slingshot_domain::command::list_experience_fragments::ListExperienceFragmentsResult;

/// Checks the same protocol invariants for each independently decoded result type.
fn check_pages<ResultType: DeserializeOwned>(row: Value) {
    let complete = json!({"complete": true, "examined_nodes": 1, "matches": [row.clone()]});
    assert!(serde_json::from_value::<ResultType>(complete.clone()).is_ok());
    let partial = json!({"complete": false, "examined_nodes": 0, "matches": [],
        "next_continuation_token": "opaque-cursor"});
    assert!(serde_json::from_value::<ResultType>(partial.clone()).is_ok());
    let mut missing_token = partial;
    missing_token.as_object_mut().expect("an object").remove("next_continuation_token");
    assert!(serde_json::from_value::<ResultType>(missing_token).is_err());
    let mut extra_token = complete.clone();
    extra_token["next_continuation_token"] = json!("opaque-cursor");
    assert!(serde_json::from_value::<ResultType>(extra_token).is_err());
    let mut old_shape = complete.clone();
    old_shape.as_object_mut().expect("an object").remove("complete");
    assert!(serde_json::from_value::<ResultType>(old_shape).is_err());
    let mut no_work = complete.clone();
    no_work["examined_nodes"] = json!(0);
    assert!(serde_json::from_value::<ResultType>(no_work).is_err());
    let mut repeated = complete.clone();
    repeated["matches"] = json!([row.clone(), row.clone()]);
    repeated["examined_nodes"] = json!(2);
    assert!(serde_json::from_value::<ResultType>(repeated).is_err());
    let mut earlier = row;
    earlier["repository_path"] = json!("/content/a");
    let mut provider_order = complete;
    provider_order["matches"].as_array_mut().expect("an array").push(earlier);
    provider_order["examined_nodes"] = json!(2);
    assert!(serde_json::from_value::<ResultType>(provider_order).is_ok());
}

#[test]
fn component_pages_preserve_provider_order_and_validate_progress() {
    let row = json!({"repository_path": "/content/z", "resource_type": "site/component"});
    check_pages::<ListComponentsResult>(row.clone());
    check_pages::<ListComponentDefinitionsResult>(row);
}

#[test]
fn fragment_pages_preserve_provider_order_and_validate_progress() {
    let row = json!({"repository_path": "/content/z"});
    check_pages::<ListContentFragmentsResult>(row.clone());
    check_pages::<ListExperienceFragmentsResult>(row);
}
