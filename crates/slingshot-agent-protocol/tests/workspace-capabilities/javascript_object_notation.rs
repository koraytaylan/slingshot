//! Probe for the JavaScript Object Notation capability.
//!
//! Requires an ordered document model, byte-exact rendering, refusal of
//! trailing input, and a reported line and column for a malformed document.

use serde_json::{Value, json};

#[test]
fn a_document_renders_canonically_and_reports_where_it_is_malformed() {
    let document = json!({"method": "daemon.ping", "identifier": "one", "arguments": []});
    let rendered = serde_json::to_string(&document).expect("the document renders");
    assert_eq!(rendered, r#"{"arguments":[],"identifier":"one","method":"daemon.ping"}"#);
    let restored: Value = serde_json::from_str(&rendered).expect("the document reads back");
    assert_eq!(restored, document);
    let trailing = serde_json::from_str::<Value>(&format!("{rendered} {rendered}"));
    assert!(trailing.is_err(), "trailing input must be refused");
    let malformed = serde_json::from_str::<Value>("{\n  \"method\": ,\n}")
        .expect_err("a malformed document is refused");
    assert_eq!(malformed.line(), 2);
    assert!(malformed.column() > 0, "the failure reports a column");
}
