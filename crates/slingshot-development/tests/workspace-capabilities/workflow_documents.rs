//! Probe for the workflow-documents capability.
//!
//! Requires reading a workflow document into a navigable model that keeps
//! nested mapping keys and sequence order, and refusing a malformed document,
//! so workflow policy can reach permissions and step text.

use serde_yaml_ng::Value;

#[test]
fn a_workflow_document_keeps_its_nested_keys_and_step_order() {
    let workflow = "name: quality\n\
                    permissions:\n  contents: read\n\
                    jobs:\n  gate:\n    steps:\n      - uses: actions/checkout@0000000\n      - run: scripts/quality\n";
    let document: Value = serde_yaml_ng::from_str(workflow).expect("the workflow parses");
    assert_eq!(document["permissions"]["contents"], Value::String("read".to_owned()));
    let steps = document["jobs"]["gate"]["steps"].as_sequence().expect("the job lists steps");
    assert_eq!(steps.len(), 2);
    assert_eq!(steps[0]["uses"], Value::String("actions/checkout@0000000".to_owned()));
    assert_eq!(steps[1]["run"], Value::String("scripts/quality".to_owned()));

    let malformed = serde_yaml_ng::from_str::<Value>("jobs:\n  gate:\n   - bad\n  worse\n");
    assert!(malformed.is_err(), "a malformed document must be refused");
}
