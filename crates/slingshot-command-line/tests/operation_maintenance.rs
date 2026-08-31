//! Showing what would be removed, and removing only that.
//!
//! An apply quotes a digest and supplies no criteria, so it cannot select
//! anything. That is the whole safety property: a target that moved between the
//! preview and the apply is a refusal rather than a different removal carried
//! out under an approval given for something else. The suite proves it by
//! moving the target and watching the apply refuse.
//!
//! A complete manifest is referenced rather than truncated when it will not fit
//! the shared budget. Truncating would produce something that looked like a
//! manifest and was not one, which is worse for a reviewer than being told
//! where to get the whole thing.
//!
//! Reading a referenced result asks for metadata first and checks the caller's
//! digest against it. Trusting the caller's digest alone would fetch whatever
//! the identifier now points at; trusting the metadata alone would fetch
//! whatever the daemon now holds. Requiring both makes a result that changed
//! underneath a refusal rather than a surprise on disk.

use slingshot_command_line::machine_outcome_envelope::MAXIMUM_MACHINE_OUTCOME_ENVELOPE_BYTES;
use slingshot_command_line::operation_maintenance::{
    ApplyRequest, Delivery, ListedOperation, ListingRequest, MAXIMUM_PAGE_SIZE,
    MAXIMUM_PREVIEW_LIMIT, MaintenanceRefusal, PreviewRequest, ResultMetadata, SelectedPartition,
    delivery_of, require_appliable, require_listable, require_previewable, require_readable,
};

/// Where the vectors this suite is driven from live.
const FIXTURES: &str = "tests/fixtures/operation-maintenance";

/// The partition this client serves.
const TARGET: &str = "target-identity-digest-one";

/// One it served before.
const HISTORICAL_TARGET: &str = "target-identity-digest-two";

/// One maintenance result.
const RESULT_IDENTIFIER: &str = "maintenance-result-one";

/// What it digests to.
const DIGEST: &str = "content-digest";

/// What a caller who is out of date expects.
const STALE_DIGEST: &str = "another-digest";

/// The instant a preview selects before.
const CUTOFF: u64 = 1_700_000_000_000;

/// How many bytes one referenced result holds.
const RESULT_BYTES: u64 = 524_288;

/// Returns every vector one fixture holds.
fn vectors(name: &str) -> Vec<serde_json::Value> {
    let path = format!("{FIXTURES}/{name}");
    let text = std::fs::read_to_string(&path).unwrap_or_else(|_| panic!("{path} is readable"));
    text.lines().map(|line| serde_json::from_str(line).expect("each line is one vector")).collect()
}

/// Returns the partition one vector names.
fn partition(historical: bool) -> SelectedPartition {
    if historical {
        SelectedPartition::Historical {
            author_target_identity_digest: HISTORICAL_TARGET.to_owned(),
        }
    } else {
        SelectedPartition::Current { author_target_identity_digest: TARGET.to_owned() }
    }
}

#[test]
fn every_listing_is_answered_the_way_its_vector_says() {
    for vector in vectors("listings.jsonl") {
        let name = vector["name"].as_str().expect("a name");
        let request = ListingRequest {
            continuation_token: None,
            limit: vector["limit"].as_u64().expect("a limit"),
            partition: partition(vector["historical"].as_bool().expect("an expectation")),
        };
        let produced =
            require_listable(&request, vector["includes_unsettled"].as_bool().expect("one"));
        let spelling = match produced {
            Ok(()) => "permitted",
            Err(MaintenanceRefusal::PageTooLarge { .. }) => "page-too-large",
            Err(MaintenanceRefusal::HistoryNotSettled) => "history-not-settled",
            Err(other) => panic!("{name}: {other}"),
        };
        assert_eq!(spelling, vector["outcome"].as_str().expect("an outcome"), "{name}");
    }
}

#[test]
fn every_row_names_the_partition_it_came_from() {
    let row = ListedOperation {
        author_target_identity_digest: TARGET.to_owned(),
        operation_identifier: "operation-one".to_owned(),
        settled: true,
    };
    assert_eq!(
        row.author_target_identity_digest, TARGET,
        "a page that omitted it would be ambiguous the moment a caller kept two of them"
    );
    assert!(row.settled);
}

#[test]
fn a_preview_selects_no_more_than_one_run_may_remove() {
    let within = PreviewRequest {
        before_unix_milliseconds: CUTOFF,
        limit: MAXIMUM_PREVIEW_LIMIT,
        partition: partition(false),
    };
    require_previewable(&within).expect("exactly at the bound");
    let beyond = PreviewRequest { limit: MAXIMUM_PREVIEW_LIMIT + 1, ..within };
    assert_eq!(
        require_previewable(&beyond),
        Err(MaintenanceRefusal::PreviewTooWide {
            actual: MAXIMUM_PREVIEW_LIMIT + 1,
            allowed: MAXIMUM_PREVIEW_LIMIT
        })
    );
    assert_eq!(
        MAXIMUM_PREVIEW_LIMIT.min(MAXIMUM_PAGE_SIZE),
        MAXIMUM_PREVIEW_LIMIT,
        "removing is narrower than looking"
    );
    assert_ne!(MAXIMUM_PREVIEW_LIMIT, MAXIMUM_PAGE_SIZE);
}

#[test]
fn an_apply_quotes_a_digest_and_cannot_select_anything_itself() {
    let request = ApplyRequest { partition: partition(false), reviewed_digest: DIGEST.to_owned() };
    require_appliable(&request, DIGEST).expect("the target has not moved");
    assert_eq!(
        require_appliable(&request, STALE_DIGEST),
        Err(MaintenanceRefusal::DigestUnknown),
        "a target that moved between the preview and the apply is a refusal rather than a \
         different removal under an old approval"
    );
    let source = std::fs::read_to_string("src/operation_maintenance.rs").expect("it is readable");
    let apply = source
        .split("pub struct ApplyRequest")
        .nth(1)
        .and_then(|rest| rest.split('}').next())
        .expect("the request is declared");
    for criterion in ["before_unix_milliseconds", "limit"] {
        assert!(
            !apply.contains(criterion),
            "an apply carrying {criterion} could remove something the reviewer never saw"
        );
    }
}

#[test]
fn a_manifest_is_referenced_rather_than_truncated_when_it_will_not_fit() {
    for vector in vectors("deliveries.jsonl") {
        let name = vector["name"].as_str().expect("a name");
        let delivery = delivery_of(vector["bytes"].as_u64().expect("a size"));
        let spelling = match delivery {
            Delivery::Inline => "inline",
            Delivery::Referenced => "referenced",
        };
        assert_eq!(spelling, vector["delivery"].as_str().expect("a delivery"), "{name}");
    }
    assert_eq!(delivery_of(MAXIMUM_MACHINE_OUTCOME_ENVELOPE_BYTES), Delivery::Inline);
    assert_eq!(
        delivery_of(MAXIMUM_MACHINE_OUTCOME_ENVELOPE_BYTES + 1),
        Delivery::Referenced,
        "something that looked like a manifest and was not one is worse than a reference"
    );
    assert_eq!(delivery_of(RESULT_BYTES), Delivery::Referenced);
}

#[test]
fn a_read_checks_the_metadata_and_the_callers_digest_and_needs_both() {
    for vector in vectors("reads.jsonl") {
        let name = vector["name"].as_str().expect("a name");
        let metadata = ResultMetadata {
            author_target_identity_digest: if vector["target_matches"]
                .as_bool()
                .expect("an expectation")
            {
                TARGET.to_owned()
            } else {
                HISTORICAL_TARGET.to_owned()
            },
            byte_length: RESULT_BYTES,
            content_digest: if vector["digest_matches"].as_bool().expect("an expectation") {
                DIGEST.to_owned()
            } else {
                STALE_DIGEST.to_owned()
            },
            maintenance_result_identifier: if vector["identifier_matches"]
                .as_bool()
                .expect("an expectation")
            {
                RESULT_IDENTIFIER.to_owned()
            } else {
                "another-result".to_owned()
            },
            readable: vector["readable"].as_bool().expect("an expectation"),
        };
        let produced = require_readable(&metadata, &partition(false), RESULT_IDENTIFIER, DIGEST);
        let spelling = match produced {
            Ok(()) => "permitted",
            Err(MaintenanceRefusal::NoLongerReadable) => "no-longer-readable",
            Err(MaintenanceRefusal::DigestMismatched) => "digest-mismatched",
            Err(other) => panic!("{name}: {other}"),
        };
        assert_eq!(spelling, vector["outcome"].as_str().expect("an outcome"), "{name}");
    }
}

#[test]
fn a_historical_partition_holds_settled_work_and_says_so() {
    assert!(partition(true).holds_only_settled_work());
    assert!(!partition(false).holds_only_settled_work());
    assert_eq!(partition(true).digest(), HISTORICAL_TARGET);
    assert_eq!(partition(false).digest(), TARGET);
}
