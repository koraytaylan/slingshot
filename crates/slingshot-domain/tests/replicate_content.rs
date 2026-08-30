//! Replication, proved honest about what it admitted and what it does not know.
//!
//! Two distinctions carry this command and both are proved here rather than
//! described. `accepted_item_count` counts author-side admission, never
//! publisher delivery, so a complete success says nothing about arrival. And a
//! rejection or a budget failure proves the current path was not accepted,
//! while an unknown outcome proves nothing about it - which is why a retry may
//! resume a path that was never offered and may never re-offer one that was.

use serde_json::Value;
use slingshot_domain::command::replicate_content::{
    AdmissionCheckpoint, AdmissionOutcome, AdmissionRefusal, PreflightRefusal,
    ReplicateContentCommand, ReplicateContentResult, ReplicationFailure, ReplicationManifest,
    maximum_replication_admission_duration_milliseconds, maximum_replication_candidate_paths,
    maximum_replication_traversal_duration_milliseconds,
};
use slingshot_domain::command::repository_path::RepositoryPath;

/// Commands this test reads.
const COMMANDS: &str = include_str!("fixtures/commands/replicate_content/commands.jsonl");

/// Manifests this test reads.
const MANIFESTS: &str = include_str!("fixtures/commands/replicate_content/manifests.jsonl");

/// Preflight failures this test reads.
const PREFLIGHT: &str = include_str!("fixtures/commands/replicate_content/preflight.jsonl");

/// Admission failures this test reads.
const ADMISSIONS: &str = include_str!("fixtures/commands/replicate_content/admissions.jsonl");

/// Checkpoints this test reads.
const CHECKPOINTS: &str = include_str!("fixtures/commands/replicate_content/checkpoints.jsonl");

/// Every refusal the fixtures can name, beside the sentence that produces it.
const DECLARED_REFUSALS: &[(&str, ReplicationFailure)] = &[
    ("ManifestNotCanonical", ReplicationFailure::ManifestNotCanonical),
    ("CountsDoNotSum", ReplicationFailure::CountsDoNotSum),
    ("CurrentPathNotInManifest", ReplicationFailure::CurrentPathNotInManifest),
    ("NotThisRequest", ReplicationFailure::NotThisRequest),
];

/// Name the fixtures give to the refusals the closed object makes on its own.
const CLOSED_OBJECT: &str = "ClosedObject";

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

/// Checks one accept-or-refuse vector against the type it names.
fn check<Parsed: serde::de::DeserializeOwned + std::fmt::Debug>(row: &Value) {
    let document = text(row, "document");
    let note = text(row, "note");
    match (row["accepted"].as_bool(), serde_json::from_str::<Parsed>(document)) {
        (Some(true), Ok(_)) => (),
        (Some(false), Err(failure)) => {
            let rendered = failure.to_string();
            let known: Vec<String> =
                DECLARED_REFUSALS.iter().map(|(_, failure)| failure.to_string()).collect();
            match refusal_rendering(text(row, "reason")) {
                Some(expected) => assert!(rendered.contains(&expected), "{note}: {rendered}"),
                None => assert!(!known.contains(&rendered), "{note}: {rendered}"),
            }
        }
        (Some(true), Err(failure)) => panic!("{note}: refused as {failure}"),
        (Some(false), Ok(value)) => panic!("{note}: accepted as {value:?}"),
        (None, _) => panic!("{note}: the fixture states whether it is accepted"),
    }
}

#[test]
fn every_command_vector_lands_where_the_fixture_says_it_does() {
    for row in &rows(COMMANDS) {
        check::<ReplicateContentCommand>(row);
    }
    for row in rows(COMMANDS).iter().filter(|row| row["accepted"] == Value::Bool(true)) {
        let document = text(row, "document");
        let command: ReplicateContentCommand =
            serde_json::from_str(document).expect("the fixture says this is accepted");
        assert_eq!(
            serde_json::to_string(&command).expect("a command serializes"),
            document,
            "{}: rewritten differently",
            text(row, "note")
        );
    }
}

#[test]
fn a_manifest_is_fixed_finite_and_offered_once_per_path() {
    let vectors = rows(MANIFESTS);
    assert!(vectors.len() >= 7, "both bounds and both ordering mistakes");
    for row in &vectors {
        check::<ReplicationManifest>(row);
    }
    let bound = usize::try_from(maximum_replication_candidate_paths()).expect("addressable");
    let full: Vec<RepositoryPath> = (0..bound)
        .map(|index| RepositoryPath::parse(&format!("/content/p{index:05}")).expect("a legal path"))
        .collect();
    let manifest = ReplicationManifest::new(full).expect("the largest manifest");
    assert_eq!(manifest.size(), maximum_replication_candidate_paths());
}

#[test]
fn a_preflight_failure_admitted_nothing_and_says_so_by_carrying_no_count() {
    let vectors = rows(PREFLIGHT);
    for row in vectors.iter().filter(|row| row["refused"] != Value::Bool(true)) {
        let document = text(row, "document");
        let note = text(row, "note");
        let refusal: PreflightRefusal =
            serde_json::from_str(document).expect("every preflight vector is a legal failure");
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
        assert_eq!(
            members,
            vec!["failure".to_owned(), "source_path".to_owned()],
            "{note}: a preflight failure carries no count and no partial manifest"
        );
        assert_eq!(row["admitted"].as_u64(), Some(0), "{note}");
    }
    for row in vectors.iter().filter(|row| row["refused"] == Value::Bool(true)) {
        assert!(
            serde_json::from_str::<PreflightRefusal>(text(row, "document")).is_err(),
            "{}: accepted",
            text(row, "note")
        );
    }
}

#[test]
fn the_counts_add_up_and_name_the_path_the_offering_stopped_at() {
    let manifest = ReplicationManifest::new(
        ["/content/a", "/content/b", "/content/c"]
            .iter()
            .map(|spelling| RepositoryPath::parse(spelling).expect("a legal path"))
            .collect(),
    )
    .expect("a legal manifest");
    for row in &rows(ADMISSIONS) {
        let note = text(row, "note");
        let mut document = row.as_object().expect("an object").clone();
        for member in ["note", "consistent", "proves_current_path_not_accepted"] {
            document.remove(member);
        }
        let refusal: AdmissionRefusal = serde_json::from_value(Value::Object(document))
            .unwrap_or_else(|failure| panic!("{note}: {failure}"));
        assert_eq!(
            refusal.require_consistent(&manifest).is_ok(),
            row["consistent"].as_bool().expect("every vector states its verdict"),
            "{note}"
        );
        assert_eq!(
            refusal.failure.proves_current_path_not_accepted(),
            row["proves_current_path_not_accepted"]
                .as_bool()
                .expect("every vector states what it proves"),
            "{note}"
        );
    }
}

#[test]
fn an_unknown_outcome_claims_less_than_a_rejection_does() {
    assert!(AdmissionOutcome::AdmissionRejected.proves_current_path_not_accepted());
    assert!(AdmissionOutcome::AdmissionBudgetExceeded.proves_current_path_not_accepted());
    assert!(
        !AdmissionOutcome::AdmissionOutcomeUnknown.proves_current_path_not_accepted(),
        "an offered path with no durable answer may or may not have been admitted"
    );
    assert_eq!(
        serde_json::to_string(&AdmissionOutcome::AdmissionOutcomeUnknown)
            .expect("an outcome serializes"),
        "\"admission_outcome_unknown\""
    );
}

#[test]
fn a_retry_resumes_only_what_was_never_offered() {
    for row in &rows(CHECKPOINTS) {
        let state: AdmissionCheckpoint =
            serde_json::from_value(Value::from(text(row, "state"))).expect("a legal state");
        assert_eq!(
            state.may_be_offered(),
            row["may_be_offered"].as_bool().expect("every vector states its verdict"),
            "{}",
            text(row, "note")
        );
    }
    assert!(AdmissionCheckpoint::NotStarted.may_be_offered());
    assert!(
        !AdmissionCheckpoint::InFlight.may_be_offered(),
        "an interrupted offer resolves to unknown rather than being repeated"
    );
    assert!(!AdmissionCheckpoint::Accepted.may_be_offered());
}

#[test]
fn a_success_admitted_every_path_and_says_nothing_about_delivery() {
    let manifest = ReplicationManifest::new(
        ["/content/a", "/content/b"]
            .iter()
            .map(|spelling| RepositoryPath::parse(spelling).expect("a legal path"))
            .collect(),
    )
    .expect("a legal manifest");
    let complete = ReplicateContentResult::complete(&manifest);
    assert_eq!(complete.accepted_item_count, manifest.size());
    assert_eq!(complete.require_complete(&manifest), Ok(()));
    assert_eq!(
        ReplicateContentResult { accepted_item_count: 1 }.require_complete(&manifest),
        Err(ReplicationFailure::CountsDoNotSum),
        "a success that admitted only some of its paths is not a success"
    );
    let written = serde_json::to_string(&complete).expect("a result serializes");
    assert_eq!(written, r#"{"accepted_item_count":2}"#);
    for absent in ["publish", "delivered", "replicated_to"] {
        assert!(!written.contains(absent), "the result claims admission alone, not {absent}");
    }
}

#[test]
fn a_preflight_failure_from_another_request_is_rejected() {
    let asked: ReplicateContentCommand =
        serde_json::from_str(r#"{"path":"/content/example","recursive":true}"#)
            .expect("a legal command");
    let elsewhere: PreflightRefusal =
        serde_json::from_str(r#"{"failure":"source_not_found","source_path":"/content/other"}"#)
            .expect("a legal refusal");
    assert_eq!(elsewhere.require_answers(&asked), Err(ReplicationFailure::NotThisRequest));
    let own: PreflightRefusal =
        serde_json::from_str(r#"{"failure":"source_not_found","source_path":"/content/example"}"#)
            .expect("a legal refusal");
    assert_eq!(own.require_answers(&asked), Ok(()));
}

#[test]
fn every_named_bound_comes_from_the_manifest_rather_than_from_here() {
    let contract = slingshot_domain::command::command_identity::CommandContract::embedded();
    assert_eq!(
        maximum_replication_candidate_paths(),
        contract.limit("maximum_replication_candidate_paths")
    );
    assert_eq!(
        maximum_replication_traversal_duration_milliseconds(),
        contract.limit("maximum_replication_traversal_duration_milliseconds")
    );
    assert_eq!(
        maximum_replication_admission_duration_milliseconds(),
        contract.limit("maximum_replication_admission_duration_milliseconds")
    );
}
