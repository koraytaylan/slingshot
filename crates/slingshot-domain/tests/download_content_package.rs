//! Building a content package, proved to return metadata and not bytes.
//!
//! The selection rules are where the surprises live, so they get the vectors.
//! Naming a root and nothing else packages that subtree. An inclusion anchor
//! brings its subtree. An exclusion removes its anchor *and* everything under
//! it, whatever order the expressions were written in - which is what stops an
//! excluded subtree from leaving unmatched descendants behind.
//!
//! Structural ancestors are the nodes on the way to the selection, carried as
//! directories alone. An ancestor is usually somebody else's content, and
//! packaging its properties because it happened to be on the path would export
//! things nobody asked for.

use serde_json::Value;
use slingshot_domain::command::artifact::ArtifactDescriptor;
use slingshot_domain::command::download_content_package::{
    DownloadContentPackageCommand, DownloadContentPackageRefusal, DownloadContentPackageResult,
    FILEVAULT_ACCESS_CONTROL_HANDLING, FILEVAULT_IMPORT_MODE, FILEVAULT_PROFILE,
    MINIMUM_FILEVAULT_VERSION, PACKAGE_FILE_NAME_SUFFIX, PackageFailure, PackageName,
    PackageSelection, REQUIRED_FILEVAULT_CAPABILITIES, maximum_package_name_bytes,
    maximum_package_output_bytes, maximum_package_roots,
};
use slingshot_domain::command::repository_path::RepositoryPath;

/// Commands this test reads.
const COMMANDS: &str = include_str!("fixtures/commands/download_content_package/commands.jsonl");

/// Names this test reads.
const NAMES: &str = include_str!("fixtures/commands/download_content_package/names.jsonl");

/// Selection vectors this test reads.
const SELECTION: &str = include_str!("fixtures/commands/download_content_package/selection.jsonl");

/// Failures this test reads.
const FAILURES: &str = include_str!("fixtures/commands/download_content_package/failures.jsonl");

/// The profile vector this test reads.
const PROFILE: &str = include_str!("fixtures/commands/download_content_package/profile.jsonl");

/// Results this test reads.
const RESULTS: &str = include_str!("fixtures/commands/download_content_package/results.jsonl");

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

/// Returns the paths one fixture member lists.
fn paths(row: &Value, member: &str) -> Vec<RepositoryPath> {
    row[member]
        .as_array()
        .unwrap_or(&Vec::new())
        .iter()
        .map(|spelling| {
            RepositoryPath::parse(spelling.as_str().expect("a spelling")).expect("a legal path")
        })
        .collect()
}

/// Checks one accept-or-refuse vector against the type it names.
fn check<Parsed: serde::de::DeserializeOwned + std::fmt::Debug>(row: &Value) {
    let document = text(row, "document");
    let note = text(row, "note");
    match (row["accepted"].as_bool(), serde_json::from_str::<Parsed>(document)) {
        (Some(true), Ok(_)) => (),
        (Some(false), Err(_)) => (),
        (Some(true), Err(failure)) => panic!("{note}: refused as {failure}"),
        (Some(false), Ok(value)) => panic!("{note}: accepted as {value:?}"),
        (None, _) => panic!("{note}: the fixture states whether it is accepted"),
    }
}

#[test]
fn every_command_vector_lands_where_the_fixture_says_it_does() {
    let vectors = rows(COMMANDS);
    assert!(vectors.len() >= 17, "every name shape and both root bounds");
    for row in &vectors {
        check::<DownloadContentPackageCommand>(row);
    }
    for row in vectors.iter().filter(|row| row["accepted"] == Value::Bool(true)) {
        let document = text(row, "document");
        let command: DownloadContentPackageCommand =
            serde_json::from_str(document).expect("the fixture says this is accepted");
        assert_eq!(
            serde_json::to_string(&command).expect("a command serializes"),
            document,
            "{}: rewritten differently",
            text(row, "note")
        );
        assert_eq!(command.require_bounded_filters(), Ok(()), "{}", text(row, "note"));
    }
}

#[test]
fn a_package_name_is_a_stem_and_the_extension_is_added_once() {
    for row in &rows(NAMES) {
        let stem = text(row, "stem");
        let note = text(row, "note");
        match (row["valid"].as_bool(), PackageName::new(stem)) {
            (Some(true), Ok(name)) => {
                assert_eq!(name.as_text(), stem, "{note}: the stem was rewritten");
                assert_eq!(name.suggested_file_name(), text(row, "suggested_file_name"), "{note}");
            }
            (Some(false), Err(_)) => (),
            (_, built) => panic!("{note}: the name answered {built:?}"),
        }
    }
    let name = PackageName::new("example-pages").expect("a legal name");
    assert!(name.suggested_file_name().ends_with(PACKAGE_FILE_NAME_SUFFIX));
    assert!(
        !name.as_text().contains(PACKAGE_FILE_NAME_SUFFIX),
        "the stem does not carry the extension the file name adds"
    );
    assert_eq!(
        PackageName::new("example.zip"),
        Err(PackageFailure::NameNotCanonical),
        "so a caller cannot supply it twice"
    );
}

#[test]
fn every_selection_vector_selects_exactly_what_the_fixture_says() {
    let vectors = rows(SELECTION);
    assert!(vectors.len() >= 8, "both filter kinds, the empty case, and the wildcard");
    for row in &vectors {
        let note = text(row, "note");
        let mut document = serde_json::Map::new();
        document.insert("package_name".to_owned(), Value::from("example"));
        document.insert("roots".to_owned(), row["roots"].clone());
        for member in ["inclusion_filters", "exclusion_filters"] {
            if row[member].is_array() {
                document.insert(member.to_owned(), row[member].clone());
            }
        }
        let command: DownloadContentPackageCommand =
            serde_json::from_value(Value::Object(document))
                .unwrap_or_else(|failure| panic!("{note}: {failure}"));
        let selection = PackageSelection::compute(&command, &paths(row, "candidates"))
            .unwrap_or_else(|failure| panic!("{note}: {failure}"));
        assert_eq!(
            selection.selected_content,
            paths(row, "selected_content"),
            "{note}: selected the wrong content"
        );
        assert_eq!(
            selection.structural_ancestors,
            paths(row, "structural_ancestors"),
            "{note}: carried the wrong ancestors"
        );
        assert!(
            selection
                .structural_ancestors
                .iter()
                .all(|ancestor| !selection.selected_content.contains(ancestor)),
            "{note}: a selected path is never also structural"
        );
    }
}

#[test]
fn an_exclusion_removes_its_whole_subtree_whatever_order_it_was_written_in() {
    let tree: Vec<RepositoryPath> =
        ["/content", "/content/bar", "/content/bar/baz", "/content/bar/baz/child"]
            .iter()
            .map(|spelling| RepositoryPath::parse(spelling).expect("a legal path"))
            .collect();
    let command: DownloadContentPackageCommand = serde_json::from_str(
        r#"{"exclusion_filters":["/content/bar/baz"],"inclusion_filters":["/content/bar"],"package_name":"example","roots":["/content"]}"#,
    )
    .expect("a legal command");
    let selection = PackageSelection::compute(&command, &tree).expect("a bounded selection");
    assert_eq!(
        selection.selected_content,
        vec![RepositoryPath::parse("/content/bar").expect("a legal path")],
        "the excluded anchor and its child are both gone"
    );

    let reversed: DownloadContentPackageCommand = serde_json::from_str(
        r#"{"exclusion_filters":["/content/bar"],"inclusion_filters":["/content/bar/(.*)"],"package_name":"example","roots":["/content"]}"#,
    )
    .expect("a legal command");
    assert!(
        PackageSelection::compute(&reversed, &tree).expect("a bounded selection").is_empty(),
        "an exclusion above every inclusion anchor removes everything"
    );
}

#[test]
fn every_failure_is_the_closed_shape_the_contract_declares() {
    let vectors = rows(FAILURES);
    assert_eq!(vectors.len(), 19, "seven fieldless, two pattern, two anchor, eight budget");
    for row in &vectors {
        let document = text(row, "document");
        let note = text(row, "note");
        let refusal: DownloadContentPackageRefusal =
            serde_json::from_str(document).expect("every failure vector is a legal failure");
        assert_eq!(
            serde_json::to_string(&refusal).expect("a failure serializes"),
            document,
            "{note}: rewritten differently"
        );
        let mut members: Vec<String> = row["members"]
            .as_array()
            .expect("every vector states its members")
            .iter()
            .map(|member| member.as_str().expect("a member name").to_owned())
            .collect();
        members.sort();
        let written: Vec<String> = serde_json::from_str::<Value>(document)
            .expect("one object")
            .as_object()
            .expect("an object")
            .keys()
            .cloned()
            .collect();
        assert_eq!(written, members, "{note}: carries other members");
        assert_eq!(
            refusal.proves_no_publication(),
            row["proves_no_publication"].as_bool().expect("every vector says what it proves"),
            "{note}"
        );
        assert!(
            !document.contains("artifact\":{"),
            "{note}: a failure carries no artifact descriptor"
        );
    }
}

#[test]
fn a_refused_expression_is_named_by_position_and_never_echoed() {
    let refusal: DownloadContentPackageRefusal = serde_json::from_str(
        r#"{"failure":"pattern_rejected","collection":"inclusion","expression_index":2}"#,
    )
    .expect("a legal failure");
    let written = serde_json::to_string(&refusal).expect("a failure serializes");
    assert!(
        !written.contains('/'),
        "an expression a caller wrote is not put anywhere it was not sent: {written}"
    );
    assert!(
        !DownloadContentPackageRefusal::ArtifactPublicationOutcomeUnknown.proves_no_publication(),
        "outcome unknown is the one category that does not prove nothing was published"
    );
}

#[test]
fn the_profile_is_the_one_this_contract_names_and_has_no_legacy_fallback() {
    let vectors = rows(PROFILE);
    assert_eq!(vectors.len(), 1, "one profile");
    let row = &vectors[0];
    assert_eq!(text(row, "profile"), FILEVAULT_PROFILE);
    assert_eq!(text(row, "import_mode"), FILEVAULT_IMPORT_MODE);
    assert_eq!(text(row, "access_control_handling"), FILEVAULT_ACCESS_CONTROL_HANDLING);
    assert_eq!(text(row, "minimum_version"), MINIMUM_FILEVAULT_VERSION);
    let capabilities: Vec<&str> = row["capabilities"]
        .as_array()
        .expect("a capability list")
        .iter()
        .map(|capability| capability.as_str().expect("a capability name"))
        .collect();
    assert_eq!(capabilities, REQUIRED_FILEVAULT_CAPABILITIES);
    assert_ne!(
        FILEVAULT_IMPORT_MODE, "merge",
        "there is no legacy merge fallback, because it is not equivalent"
    );
}

#[test]
fn a_result_carries_a_descriptor_and_never_the_package() {
    for row in &rows(RESULTS) {
        check::<DownloadContentPackageResult>(row);
    }
    let accepted = rows(RESULTS)
        .into_iter()
        .find(|row| row["accepted"] == Value::Bool(true))
        .expect("one accepted result");
    let result: DownloadContentPackageResult =
        serde_json::from_str(text(&accepted, "document")).expect("a legal result");
    let command: DownloadContentPackageCommand =
        serde_json::from_str(r#"{"package_name":"example-pages","roots":["/content/example"]}"#)
            .expect("a legal command");
    assert_eq!(result.require_answers(&command), Ok(()));

    let renamed: DownloadContentPackageCommand =
        serde_json::from_str(r#"{"package_name":"something-else","roots":["/content"]}"#)
            .expect("a legal command");
    assert_eq!(
        result.require_answers(&renamed),
        Err(PackageFailure::NotThisRequest),
        "a package built for another request does not answer this one"
    );
    let oversized = DownloadContentPackageResult {
        artifact: ArtifactDescriptor {
            byte_length: maximum_package_output_bytes() + 1,
            ..result.artifact.clone()
        },
    };
    assert_eq!(
        oversized.require_answers(&command),
        Err(PackageFailure::ArtifactDoesNotMatchSlot),
        "and one larger than the slot admits is refused"
    );
}

#[test]
fn every_named_bound_comes_from_the_manifest_rather_than_from_here() {
    let contract = slingshot_domain::command::command_identity::CommandContract::embedded();
    assert_eq!(maximum_package_name_bytes(), contract.limit("maximum_package_name_bytes"));
    assert_eq!(maximum_package_roots(), contract.limit("maximum_package_roots"));
    assert_eq!(maximum_package_output_bytes(), contract.limit("maximum_package_output_bytes"));
}
