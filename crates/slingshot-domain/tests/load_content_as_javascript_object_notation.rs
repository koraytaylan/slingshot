//! Loading one subtree, proved typed and proved bounded.
//!
//! The disposition boundary gets the most attention here because it is the one
//! place where a number decides which of two shapes a result takes. The test
//! builds a document whose canonical bytes land exactly on the boundary, then
//! one byte over, then varies the echoed path and the outer envelope without
//! touching the document - which must not move the answer.
//!
//! What is not proved here is traversal. This crate does not talk to a
//! repository, so the execution contract is validated as an inventory of
//! boundaries and call traces an agent must honor, and the test says so rather
//! than implying it ran one.

use std::collections::BTreeMap;

use serde_json::Value;
use slingshot_domain::command::artifact::{
    ArtifactDescriptor, ArtifactDigest, ArtifactIdentifier, ArtifactMediaType, ArtifactSlot,
    LOADED_CONTENT_FILE_NAME, LOADED_CONTENT_MEDIA_TYPE, LOADED_CONTENT_SLOT, SuggestedFileName,
};
use slingshot_domain::command::load_content_as_javascript_object_notation::{
    ARTIFACT_DISPOSITION, DECLARED_PROPERTY_TYPES, INLINE_DISPOSITION, LoadBudget,
    LoadContentAsJavaScriptObjectNotationCommand, LoadContentAsJavaScriptObjectNotationResult,
    LoadDepth, LoadFailure, LoadRefusal, LoadedScalar,
    RepositoryJavaScriptObjectNotationPropertyValue, RepositoryJavaScriptObjectNotationResource,
    UnsupportedValueRole, default_load_depth, maximum_agent_inline_loaded_document_bytes,
    maximum_load_depth, maximum_load_document_bytes,
};
use slingshot_domain::command::repository_path::RepositoryPath;

/// Requests this test reads.
const REQUESTS: &str = include_str!("fixtures/commands/load_content_as_json/requests.jsonl");

/// Property values this test reads.
const PROPERTY_VALUES: &str =
    include_str!("fixtures/commands/load_content_as_json/property-values.jsonl");

/// Failures this test reads.
const FAILURES: &str = include_str!("fixtures/commands/load_content_as_json/failures.jsonl");

/// Execution scenarios this test reads.
const EXECUTION: &str = include_str!("fixtures/commands/load_content_as_json/execution.jsonl");

/// Every refusal the fixtures can name, beside the variant that produces it.
const DECLARED_REFUSALS: &[(&str, LoadFailure)] = &[
    ("DepthAboveMaximum", LoadFailure::DepthAboveMaximum),
    ("UnknownPropertyType", LoadFailure::UnknownPropertyType),
    ("UnknownCardinality", LoadFailure::UnknownCardinality),
    ("TypeMismatch", LoadFailure::TypeMismatch),
    ("NotExactlyOneValue", LoadFailure::NotExactlyOneValue),
    ("NotHomogeneous", LoadFailure::NotHomogeneous),
    ("BinaryLengthOutOfRange", LoadFailure::BinaryLengthOutOfRange),
    ("DoubleNotBitString", LoadFailure::DoubleNotBitString),
    ("ReferenceOutOfBounds", LoadFailure::ReferenceOutOfBounds),
    ("UniformResourceIdentifierNotCanonical", LoadFailure::UniformResourceIdentifierNotCanonical),
    ("StringTooLong", LoadFailure::StringTooLong),
    ("UnknownDisposition", LoadFailure::UnknownDisposition),
    ("DispositionDoesNotMatchDocument", LoadFailure::DispositionDoesNotMatchDocument),
    ("DocumentTooLong", LoadFailure::DocumentTooLong),
    ("ArtifactDoesNotMatchSlot", LoadFailure::ArtifactDoesNotMatchSlot),
    ("NotThisRequest", LoadFailure::NotThisRequest),
];

/// Name the fixtures give to the refusals the closed object makes on its own.
const CLOSED_OBJECT: &str = "ClosedObject";

/// A digest of bytes this test does not need to have.
const SAMPLE_DIGEST: &str = "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855";

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

/// Returns the rendering the named refusal produces.
fn refusal_rendering(reason: &str) -> Option<String> {
    if reason == CLOSED_OBJECT {
        return None;
    }
    DECLARED_REFUSALS
        .iter()
        .find(|(name, _)| *name == reason)
        .map(|(_, failure)| failure.to_string())
        .or_else(|| panic!("the fixture names a refusal this test does not know: {reason}"))
}

/// Returns every sentence a load refusal can render as.
fn every_refusal_rendering() -> Vec<String> {
    DECLARED_REFUSALS.iter().map(|(_, failure)| failure.to_string()).collect()
}

/// Checks one accept-or-refuse vector against the type it names.
fn check<Parsed: serde::de::DeserializeOwned + std::fmt::Debug>(row: &Value) {
    let document = text(row, "document");
    let note = text(row, "note");
    match (row["accepted"].as_bool(), serde_json::from_str::<Parsed>(document)) {
        (Some(true), Ok(_)) => (),
        (Some(false), Err(failure)) => {
            let rendered = failure.to_string();
            match refusal_rendering(text(row, "reason")) {
                Some(expected) => assert!(
                    rendered.contains(&expected),
                    "{note}: refused as {rendered}, not as {expected}"
                ),
                None => assert!(
                    !every_refusal_rendering().contains(&rendered),
                    "{note}: the closed object itself refuses this: {rendered}"
                ),
            }
        }
        (Some(true), Err(failure)) => panic!("{note}: refused as {failure}"),
        (Some(false), Ok(value)) => panic!("{note}: accepted as {value:?}"),
        (None, _) => panic!("{note}: the fixture states whether it is accepted"),
    }
}

#[test]
fn every_request_vector_lands_where_the_fixture_says_it_does() {
    for row in &rows(REQUESTS) {
        check::<LoadContentAsJavaScriptObjectNotationCommand>(row);
        if row["accepted"] != Value::Bool(true) {
            continue;
        }
        let command: LoadContentAsJavaScriptObjectNotationCommand =
            serde_json::from_str(text(row, "document")).expect("the fixture says this is accepted");
        assert_eq!(
            command.resolved_depth().edges(),
            row["resolved_depth"].as_u64().expect("every accepted request states its depth"),
            "{}",
            text(row, "note")
        );
    }
}

#[test]
fn an_omitted_depth_resolves_and_a_stated_one_survives() {
    let omitted: LoadContentAsJavaScriptObjectNotationCommand =
        serde_json::from_str(r#"{"path":"/content"}"#).expect("a legal request");
    assert_eq!(omitted.depth, None, "the request did not state one");
    assert_eq!(omitted.resolved_depth().edges(), default_load_depth());
    assert_eq!(
        serde_json::to_string(&omitted).expect("a request serializes"),
        r#"{"path":"/content"}"#,
        "and it is not written back as though it had"
    );
    assert_eq!(
        LoadDepth::new(maximum_load_depth()).map(LoadDepth::edges),
        Ok(maximum_load_depth())
    );
    assert_eq!(LoadDepth::new(maximum_load_depth() + 1), Err(LoadFailure::DepthAboveMaximum));
    assert_eq!(LoadDepth::new(0).map(LoadDepth::edges), Ok(0), "zero is the resource alone");
}

#[test]
fn every_property_value_vector_lands_where_the_fixture_says_it_does() {
    let vectors = rows(PROPERTY_VALUES);
    assert!(vectors.len() >= 70, "every JCR type is proved at both edges");
    for row in &vectors {
        check::<RepositoryJavaScriptObjectNotationPropertyValue>(row);
    }
}

#[test]
fn every_accepted_property_value_writes_itself_back_byte_for_byte() {
    for row in rows(PROPERTY_VALUES).iter().filter(|row| row["accepted"] == Value::Bool(true)) {
        let document = text(row, "document");
        let value: RepositoryJavaScriptObjectNotationPropertyValue =
            serde_json::from_str(document).expect("the fixture says this is accepted");
        assert_eq!(
            serde_json::to_string(&value).expect("a valid value serializes"),
            document,
            "{}: rewritten differently",
            text(row, "note")
        );
    }
}

#[test]
fn every_declared_jcr_type_has_an_accepted_vector() {
    let accepted: Vec<RepositoryJavaScriptObjectNotationPropertyValue> = rows(PROPERTY_VALUES)
        .iter()
        .filter(|row| row["accepted"] == Value::Bool(true))
        .map(|row| {
            serde_json::from_str(text(row, "document")).expect("the fixture says this is accepted")
        })
        .collect();
    for property_type in DECLARED_PROPERTY_TYPES {
        assert!(
            accepted.iter().any(|value| value.property_type() == *property_type),
            "{property_type} has no accepted vector"
        );
    }
    assert_eq!(DECLARED_PROPERTY_TYPES.len(), 12, "twelve types, and no thirteenth");
}

#[test]
fn a_double_keeps_bits_that_a_json_number_would_lose() {
    let bits = |spelling: &str| {
        let document = format!(
            "{{\"cardinality\":\"single\",\"property_type\":\"double\",\"value\":\"{spelling}\"}}"
        );
        let value: RepositoryJavaScriptObjectNotationPropertyValue =
            serde_json::from_str(&document).expect("a legal double");
        match value.values() {
            [LoadedScalar::Double(double)] => double.bits(),
            other => panic!("a double parsed as {other:?}"),
        }
    };
    assert_ne!(
        bits("0000000000000000"),
        bits("8000000000000000"),
        "positive and negative zero stay two values"
    );
    assert_ne!(
        bits("7ff8000000000001"),
        bits("7ff8000000000002"),
        "two NaN payloads stay two values"
    );
    assert_eq!(bits("7ff0000000000000"), f64::INFINITY.to_bits());
    assert_eq!(bits("fff0000000000000"), f64::NEG_INFINITY.to_bits());
    assert_eq!(f64::from_bits(bits("3ff8000000000000")), 1.5);
}

#[test]
fn a_binary_reports_its_length_and_carries_nothing() {
    let document =
        r#"{"cardinality":"single","property_type":"binary","value":{"byte_length":"1048576"}}"#;
    let value: RepositoryJavaScriptObjectNotationPropertyValue =
        serde_json::from_str(document).expect("a legal binary");
    let written = serde_json::to_string(&value).expect("a binary serializes");
    assert_eq!(written, document, "the length comes back exactly");
    assert!(!written.contains("bytes"), "and no bytes come back at all");
}

/// Returns one resource with `properties` and `children`.
fn resource(
    path: &str,
    properties: Vec<(&str, RepositoryJavaScriptObjectNotationPropertyValue)>,
    children: Vec<RepositoryJavaScriptObjectNotationResource>,
    truncated: bool,
) -> RepositoryJavaScriptObjectNotationResource {
    RepositoryJavaScriptObjectNotationResource {
        children,
        children_truncated: truncated,
        path: RepositoryPath::parse(path).expect("a legal path"),
        properties: properties
            .into_iter()
            .map(|(name, value)| (name.to_owned(), value))
            .collect::<BTreeMap<_, _>>(),
    }
}

/// Returns one single-valued string property.
fn string_property(value: &str) -> RepositoryJavaScriptObjectNotationPropertyValue {
    RepositoryJavaScriptObjectNotationPropertyValue::new(
        "string",
        false,
        vec![LoadedScalar::Text(value.to_owned())],
    )
    .expect("a legal string property")
}

#[test]
fn properties_are_written_in_ascending_name_bytes_whatever_order_they_arrived_in() {
    let document = resource(
        "/content/example",
        vec![
            ("zebra", string_property("last")),
            ("alpha", string_property("first")),
            ("Alpha", string_property("before first, because bytes")),
        ],
        Vec::new(),
        false,
    );
    let written = document.canonical_bytes().expect("a small document");
    let alpha = written.find("\"Alpha\"").expect("the capital name is written");
    let lower = written.find("\"alpha\"").expect("the lowercase name is written");
    let zebra = written.find("\"zebra\"").expect("the last name is written");
    assert!(alpha < lower && lower < zebra, "sorted by bytes, not by folded spelling");
    assert!(written.starts_with(r#"{"children":[],"children_truncated":false,"path":"#));
}

#[test]
fn every_resource_carries_its_own_path_so_a_sibling_can_be_represented() {
    let document = resource(
        "/content/example/par",
        vec![],
        vec![
            resource("/content/example/par/text", vec![], Vec::new(), false),
            resource("/content/example/par/text[2]", vec![], Vec::new(), false),
        ],
        true,
    );
    let written = document.canonical_bytes().expect("a small document");
    assert!(written.contains("/content/example/par/text[2]"), "the sibling has its own address");
    assert!(written.contains(r#""children_truncated":true"#), "and the flag says more remain");
    let read: RepositoryJavaScriptObjectNotationResource =
        serde_json::from_str(&written).expect("its own bytes parse");
    assert_eq!(read, document);
}

/// Returns one document whose canonical bytes are exactly `wanted`.
///
/// The padding goes into one string property, so growing the document by one
/// byte grows exactly the thing the boundary charges.
fn document_of_exactly(wanted: u64) -> RepositoryJavaScriptObjectNotationResource {
    let empty = resource("/a", vec![("p", string_property(""))], Vec::new(), false);
    let overhead = u64::try_from(empty.canonical_bytes().expect("a small document").len())
        .expect("an addressable length");
    let padding = usize::try_from(wanted - overhead).expect("the wanted size is larger");
    let document =
        resource("/a", vec![("p", string_property(&"x".repeat(padding)))], Vec::new(), false);
    assert_eq!(
        u64::try_from(document.canonical_bytes().expect("a bounded document").len())
            .expect("an addressable length"),
        wanted,
        "the padding landed exactly"
    );
    document
}

#[test]
fn the_disposition_boundary_is_decided_by_the_document_and_nothing_else() {
    let boundary = maximum_agent_inline_loaded_document_bytes();
    let at = document_of_exactly(boundary);
    let over = document_of_exactly(boundary + 1);
    assert_eq!(at.required_disposition(), Ok(INLINE_DISPOSITION), "exactly at the boundary");
    assert_eq!(over.required_disposition(), Ok(ARTIFACT_DISPOSITION), "one byte over it");

    for path in ["/a", "/content/example/en/products/a-very-long-page-name-indeed"] {
        let echoed = RepositoryPath::parse(path).expect("a legal path");
        let inline = LoadContentAsJavaScriptObjectNotationResult::Inline {
            document: at.clone(),
            path: echoed.clone(),
        };
        assert_eq!(
            inline.require_consistent(),
            Ok(()),
            "a longer echoed path cannot push a document over the boundary"
        );
        let outer = serde_json::to_string(&inline).expect("a result serializes");
        assert!(
            u64::try_from(outer.len()).expect("addressable") > boundary,
            "even though the whole result is larger than the boundary"
        );
    }

    let wrong = LoadContentAsJavaScriptObjectNotationResult::Inline {
        document: over,
        path: RepositoryPath::parse("/a").expect("a legal path"),
    };
    assert_eq!(
        wrong.require_consistent(),
        Err(LoadFailure::DispositionDoesNotMatchDocument),
        "a document past the boundary cannot be carried inline"
    );
}

/// Returns one descriptor for a document of `byte_length`.
fn descriptor(byte_length: u64, name: &str) -> ArtifactDescriptor {
    ArtifactDescriptor {
        identifier: ArtifactIdentifier::new("loaded-content-1").expect("a legal identifier"),
        slot: ArtifactSlot::new(LOADED_CONTENT_SLOT).expect("a legal slot"),
        media_type: ArtifactMediaType::new(LOADED_CONTENT_MEDIA_TYPE).expect("a legal type"),
        byte_length,
        digest: ArtifactDigest::new(SAMPLE_DIGEST).expect("a legal digest"),
        suggested_file_name: SuggestedFileName::new(name).expect("a legal name"),
    }
}

#[test]
fn an_artifact_result_fills_exactly_the_slot_this_command_declares() {
    let over = maximum_agent_inline_loaded_document_bytes() + 1;
    let path = RepositoryPath::parse("/a").expect("a legal path");
    let good = LoadContentAsJavaScriptObjectNotationResult::Artifact {
        artifact: descriptor(over, LOADED_CONTENT_FILE_NAME),
        path: path.clone(),
    };
    assert_eq!(good.require_consistent(), Ok(()));

    let renamed = LoadContentAsJavaScriptObjectNotationResult::Artifact {
        artifact: descriptor(over, "something-else.json"),
        path: path.clone(),
    };
    assert_eq!(
        renamed.require_consistent(),
        Err(LoadFailure::ArtifactDoesNotMatchSlot),
        "the suggested file name this command declares is exact"
    );

    let small = LoadContentAsJavaScriptObjectNotationResult::Artifact {
        artifact: descriptor(
            maximum_agent_inline_loaded_document_bytes(),
            LOADED_CONTENT_FILE_NAME,
        ),
        path: path.clone(),
    };
    assert_eq!(
        small.require_consistent(),
        Err(LoadFailure::DispositionDoesNotMatchDocument),
        "a document that fits inline is not offered as a file"
    );

    let huge = LoadContentAsJavaScriptObjectNotationResult::Artifact {
        artifact: descriptor(maximum_load_document_bytes() + 1, LOADED_CONTENT_FILE_NAME),
        path,
    };
    assert_eq!(
        huge.require_consistent(),
        Err(LoadFailure::ArtifactDoesNotMatchSlot),
        "and one larger than the slot admits is refused"
    );
}

#[test]
fn a_result_from_another_request_is_refused_before_it_can_be_kept() {
    let asked = LoadContentAsJavaScriptObjectNotationCommand {
        depth: None,
        path: RepositoryPath::parse("/content/example").expect("a legal path"),
    };
    let answered = LoadContentAsJavaScriptObjectNotationResult::Inline {
        document: resource("/content/other", vec![], Vec::new(), false),
        path: RepositoryPath::parse("/content/other").expect("a legal path"),
    };
    assert_eq!(answered.require_answers(&asked), Err(LoadFailure::NotThisRequest));

    let own = LoadContentAsJavaScriptObjectNotationResult::Inline {
        document: resource("/content/example", vec![], Vec::new(), false),
        path: asked.path.clone(),
    };
    assert_eq!(own.require_answers(&asked), Ok(()));

    let artifact = LoadContentAsJavaScriptObjectNotationResult::Artifact {
        artifact: descriptor(
            maximum_agent_inline_loaded_document_bytes() + 1,
            LOADED_CONTENT_FILE_NAME,
        ),
        path: RepositoryPath::parse("/content/other").expect("a legal path"),
    };
    assert_eq!(
        artifact.require_answers(&asked),
        Err(LoadFailure::NotThisRequest),
        "an artifact from another request is refused before it is accepted"
    );
}

#[test]
fn a_result_carries_one_disposition_and_never_both_or_neither() {
    /// Bytes of a document small enough that its size decides nothing here.
    const SMALL_DOCUMENT_BYTES: u64 = 1000;

    let small = document_of_exactly(SMALL_DOCUMENT_BYTES);
    let written = serde_json::to_string(&small).expect("a document serializes");
    let both = format!(
        "{{\"artifact\":{},\"disposition\":\"inline\",\"document\":{written},\"path\":\"/a\"}}",
        serde_json::to_string(&descriptor(1, LOADED_CONTENT_FILE_NAME))
            .expect("a descriptor serializes")
    );
    assert!(
        serde_json::from_str::<LoadContentAsJavaScriptObjectNotationResult>(&both).is_err(),
        "a result carrying both is refused"
    );
    let neither = r#"{"disposition":"inline","path":"/a"}"#;
    assert!(
        serde_json::from_str::<LoadContentAsJavaScriptObjectNotationResult>(neither).is_err(),
        "and so is one carrying neither"
    );
    let unknown =
        format!("{{\"disposition\":\"streamed\",\"document\":{written},\"path\":\"/a\"}}");
    assert!(
        serde_json::from_str::<LoadContentAsJavaScriptObjectNotationResult>(&unknown).is_err(),
        "and so is a third disposition"
    );
}

#[test]
fn every_failure_is_the_closed_shape_the_contract_declares() {
    let vectors = rows(FAILURES);
    assert_eq!(vectors.len(), 10, "two anchors, three roles, and five budgets");
    for row in &vectors {
        let document = text(row, "document");
        let note = text(row, "note");
        let refusal: LoadRefusal =
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
        let mut declared: Vec<String> = row["members"]
            .as_array()
            .expect("every vector states its members")
            .iter()
            .map(|member| member.as_str().expect("a member name").to_owned())
            .collect();
        declared.sort();
        assert_eq!(members, declared, "{note}: carries other members");
        if let LoadRefusal::LoadBudgetExceeded { .. } = refusal {
            assert_eq!(
                members,
                vec!["budget".to_owned(), "failure".to_owned()],
                "{note}: a budget failure names no path and carries no partial document"
            );
        }
    }
}

#[test]
fn the_declared_budgets_and_roles_are_exactly_these() {
    let budgets = [
        LoadBudget::ResourceNodes,
        LoadBudget::PropertyValues,
        LoadBudget::PropertyBytes,
        LoadBudget::SerializedDocumentBytes,
        LoadBudget::TraversalDuration,
    ];
    let written: Vec<String> = budgets
        .iter()
        .map(|budget| serde_json::to_string(budget).expect("a budget serializes"))
        .collect();
    assert_eq!(
        written,
        vec![
            "\"resource_nodes\"",
            "\"property_values\"",
            "\"property_bytes\"",
            "\"serialized_document_bytes\"",
            "\"traversal_duration\"",
        ]
    );
    let roles = [
        UnsupportedValueRole::ResourceName,
        UnsupportedValueRole::PropertyName,
        UnsupportedValueRole::PropertyValue,
    ];
    let written: Vec<String> =
        roles.iter().map(|role| serde_json::to_string(role).expect("a role serializes")).collect();
    assert_eq!(written, vec!["\"resource_name\"", "\"property_name\"", "\"property_value\""]);
}

#[test]
fn the_execution_contract_names_the_same_three_checks_at_every_boundary() {
    let vectors = rows(EXECUTION);
    let boundaries: Vec<&Value> =
        vectors.iter().filter(|row| text(row, "kind") == "boundary").collect();
    assert!(boundaries.len() >= 10, "before and after every kind of call");
    for row in &boundaries {
        let checks: Vec<&str> = row["checks"]
            .as_array()
            .expect("every boundary states its checks")
            .iter()
            .map(|check| check.as_str().expect("a check name"))
            .collect();
        assert_eq!(
            checks,
            vec!["cancellation", "traversal_duration", "next_charge"],
            "{}",
            text(row, "note")
        );
    }
    for position in ["root_call", "child_call", "property_call", "binary_metadata_call"] {
        for side in ["before", "after"] {
            let named = format!("{side}_{position}");
            assert!(
                boundaries.iter().any(|row| text(row, "position") == named),
                "{named} has no boundary"
            );
        }
    }
}

#[test]
fn no_trace_that_stopped_early_publishes_anything() {
    let traces: Vec<Value> =
        rows(EXECUTION).into_iter().filter(|row| text(row, "kind") == "trace").collect();
    assert!(traces.len() >= 10, "every budget and both cancellation points are traced");
    for row in &traces {
        let outcome = text(row, "outcome");
        let publishes = row["publishes"].as_bool().expect("every trace states what it published");
        let note = text(row, "note");
        assert_eq!(
            publishes,
            outcome == "completed",
            "{note}: a stopped traversal publishes nothing"
        );
        if let Some(budget) = outcome.strip_prefix("load_budget_exceeded:") {
            let quoted = format!("\"{budget}\"");
            assert!(
                serde_json::from_str::<LoadBudget>(&quoted).is_ok(),
                "{note}: {budget} is not one of the five"
            );
        } else {
            assert!(
                outcome == "completed" || outcome == "cancelled",
                "{note}: {outcome} is not an outcome this contract has"
            );
        }
    }
    assert!(
        traces.iter().any(|row| text(row, "note").contains("after the deadline")),
        "a late return is traced, though nothing here claims to preempt a call"
    );
}
