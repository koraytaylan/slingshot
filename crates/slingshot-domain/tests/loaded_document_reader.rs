//! Incremental loaded-document validation against the existing typed vectors.
use slingshot_domain::command::{
    load_content_as_javascript_object_notation::LoadContentAsJavaScriptObjectNotationCommand,
    loaded_document_reader::require_loaded_document_reader,
};

fn command() -> LoadContentAsJavaScriptObjectNotationCommand {
    serde_json::from_value(serde_json::json!({"path":"/content/example","depth":1})).unwrap()
}

#[test]
fn every_committed_property_vector_passes_through_the_incremental_document_gate() {
    let fixture = include_str!("fixtures/commands/load_content_as_json/property-values.jsonl");
    for line in fixture.lines() {
        let row: serde_json::Value = serde_json::from_str(line).unwrap();
        let property = row["document"].as_str().unwrap();
        let document = format!(
            r#"{{"children":[],"children_truncated":false,"path":"/content/example","properties":{{"p":{property}}}}}"#
        );
        let canonical =
            slingshot_domain::command::canonical_json::require_canonical_bytes(document.as_bytes())
                .is_ok();
        assert_eq!(
            require_loaded_document_reader(document.as_bytes(), &command()).is_ok(),
            row["accepted"].as_bool().unwrap() && canonical,
            "{}",
            row["note"]
        );
    }
}

#[test]
fn structure_and_scalar_refusals_are_closed_and_request_bound() {
    let leaf = |path: &str| serde_json::json!({"children":[],"children_truncated":false,"path":path,"properties":{}});
    for mutation in 0..9 {
        let mut document = leaf("/content/example");
        document["children"] = serde_json::json!([
            leaf("/content/example/a"),
            leaf("/content/example/a[2]"),
            leaf("/content/example/a[10]")
        ]);
        match mutation {
            0 => {}
            1 => document["path"] = "/content/other".into(),
            2 => document["children"][0]["path"] = "/content/other/a".into(),
            3 => document["children"].as_array_mut().unwrap().swap(1, 2),
            4 => {
                document["children"][0]["children"] =
                    serde_json::json!([leaf("/content/example/a/deep")])
            }
            5 => document["children_truncated"] = true.into(),
            6 => document["extra"] = true.into(),
            7 => {
                document.as_object_mut().unwrap().remove("properties");
            }
            8 => {
                document["properties"]["invalid/name"] = serde_json::json!({"cardinality":"multiple","property_type":"string","values":[]})
            }
            _ => unreachable!(),
        }
        let bytes = slingshot_domain::command::canonical_json::write_canonical(&document).unwrap();
        assert_eq!(
            require_loaded_document_reader(bytes.as_bytes(), &command()).is_ok(),
            mutation == 0,
            "mutation {mutation}"
        );
    }
}
