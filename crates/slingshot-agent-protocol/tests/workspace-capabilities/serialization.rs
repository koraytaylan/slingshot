//! Probe for the serialization capability.
//!
//! Requires the derive feature, stable renamed field spellings, and a refusal
//! of an unknown field so wire shapes cannot silently widen.

use serde::{Deserialize, Serialize};

#[derive(Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct OperationRecord {
    operation_name: String,
    attempt_count: u32,
}

#[test]
fn a_derived_shape_renames_its_fields_and_refuses_an_unknown_one() {
    let record = OperationRecord { operation_name: "ping".to_owned(), attempt_count: 2 };
    let rendered = serde_json::to_string(&record).expect("the record renders");
    assert_eq!(rendered, r#"{"operationName":"ping","attemptCount":2}"#);
    let restored: OperationRecord = serde_json::from_str(&rendered).expect("the record reads back");
    assert_eq!(restored, record);
    let widened = r#"{"operationName":"ping","attemptCount":2,"extra":true}"#;
    let refused = serde_json::from_str::<OperationRecord>(widened);
    assert!(refused.is_err(), "an unknown field must be refused");
}
