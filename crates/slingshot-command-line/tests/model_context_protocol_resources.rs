//! What this server publishes as a resource, and what it refuses to.
//!
//! The two claims that matter most are about identity. A maintenance result is
//! addressed by target and identifier alone, so nothing here may accept an
//! operation, a slot, or a separator inside that identifier; and a read is
//! checked against the lookup that preceded it, so a document that changed
//! between the two calls is refused rather than served as though it had not.
//!
//! Exactly one change is admitted between lookup and read: a current preview
//! becoming an application receipt at the next revision. That is what an apply
//! committing between the calls looks like from here, and refusing it would
//! make a correct sequence fail for having been correct.

use serde_json::json;

use slingshot_command_line::model_context_protocol::resource_catalog::{
    ListingPage, MAINTENANCE_IDENTIFIER_CHARACTERS, MAINTENANCE_MEDIA_TYPE, MAINTENANCE_TEMPLATE,
    MAXIMUM_LISTED_OPERATIONS, MaintenanceFacts, Namespace, PREVIEW_OWNER, RECEIPT_OWNER,
    ResourceAddress, ResourceRefusal, UNDISCLOSABLE_MEMBERS, maintenance_address, parse,
    require_disclosable, require_maintenance_identifier, require_same_document,
};

/// A maintenance-result identifier of the shape Plan 0004 produces.
const IDENTIFIER: &str = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";

/// A partition digest of the shape Plan 0002 produces.
const TARGET: &str = "fedcba9876543210fedcba9876543210fedcba9876543210fedcba9876543210";

/// Returns the namespace every case addresses.
fn namespace() -> Namespace {
    Namespace {
        author_target_identity_digest: TARGET.to_owned(),
        environment: "author".to_owned(),
        profile: "local".to_owned(),
    }
}

/// How many bytes the document every case describes holds.
const DOCUMENT_BYTES: u64 = 4_096;

/// The revision a lookup finds the association at.
const FOUND_REVISION: u64 = 3;

/// The revision an apply committing between two calls moves it to.
const TRANSFERRED_REVISION: u64 = 4;

/// A revision one further on than any transfer reaches.
const FURTHER_REVISION: u64 = 5;

/// How many operations a short listing holds.
const FEW_OPERATIONS: usize = 3;

/// Returns the facts a lookup answers with.
fn facts(owner: &str, revision: u64) -> MaintenanceFacts {
    MaintenanceFacts {
        association_revision: revision,
        author_target_identity_digest: TARGET.to_owned(),
        byte_length: DOCUMENT_BYTES,
        content_digest: IDENTIFIER.to_owned(),
        kind: "preview".to_owned(),
        maintenance_result_identifier: IDENTIFIER.to_owned(),
        media_type: MAINTENANCE_MEDIA_TYPE.to_owned(),
        retention_owner: owner.to_owned(),
        reviewed_source_digest: TARGET.to_owned(),
    }
}

#[test]
fn a_maintenance_address_names_a_target_and_an_identifier_and_nothing_else() {
    let address = maintenance_address(&namespace(), IDENTIFIER);
    let parsed = parse(&address).expect("this server published that address");
    assert_eq!(
        parsed,
        ResourceAddress::MaintenanceResult {
            namespace: namespace(),
            maintenance_result_identifier: IDENTIFIER.to_owned(),
        }
    );
    assert!(!address.contains("/operations/"), "a maintenance result belongs to no operation");
    assert!(MAINTENANCE_TEMPLATE.contains("{maintenance_result_identifier}"));
    assert!(!MAINTENANCE_TEMPLATE.contains("{operation_identifier}"));
}

#[test]
fn an_identifier_that_is_not_sixty_four_hexadecimal_characters_is_refused() {
    for held in [
        "",
        "ABCDEF0123456789abcdef0123456789abcdef0123456789abcdef0123456789",
        "0123456789abcdef",
        &format!("{IDENTIFIER}0"),
        "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcde/",
    ] {
        assert_eq!(
            require_maintenance_identifier(held),
            Err(ResourceRefusal::IdentifierUnusable),
            "{held:?} is not an identifier"
        );
    }
    require_maintenance_identifier(IDENTIFIER).expect("this one is");
    assert_eq!(IDENTIFIER.len(), MAINTENANCE_IDENTIFIER_CHARACTERS);
}

#[test]
fn an_address_carrying_an_operation_where_a_maintenance_result_goes_is_refused() {
    let smuggled = format!(
        "slingshot://profiles/local/environments/author/targets/{TARGET}\
         /maintenance/results/operations"
    );
    assert_eq!(parse(&smuggled), Err(ResourceRefusal::IdentifierUnusable));
    let deeper = format!(
        "slingshot://profiles/local/environments/author/targets/{TARGET}\
         /maintenance/results/{IDENTIFIER}/slots/one"
    );
    assert_eq!(parse(&deeper), Err(ResourceRefusal::UnknownShape));
}

#[test]
fn an_operation_address_and_an_artifact_address_name_what_they_say() {
    let operation =
        format!("slingshot://profiles/local/environments/author/targets/{TARGET}/operations/one");
    assert_eq!(
        parse(&operation),
        Ok(ResourceAddress::Operation {
            namespace: namespace(),
            operation_identifier: "one".to_owned(),
        })
    );
    let artifact = format!("{operation}/artifacts/structured-result");
    assert_eq!(
        parse(&artifact),
        Ok(ResourceAddress::Artifact {
            namespace: namespace(),
            operation_identifier: "one".to_owned(),
            artifact_identifier: "structured-result".to_owned(),
        })
    );
}

#[test]
fn an_address_this_server_does_not_publish_is_refused_rather_than_guessed_at() {
    assert_eq!(parse("https://example.invalid/one"), Err(ResourceRefusal::ForeignScheme));
    assert_eq!(parse("slingshot://profiles/local"), Err(ResourceRefusal::UnknownShape));
    assert_eq!(
        parse(&format!("slingshot://profiles/local/environments/author/targets/{TARGET}/jobs/one")),
        Err(ResourceRefusal::UnknownShape)
    );
}

#[test]
fn an_escaped_segment_is_read_back_as_what_it_escaped() {
    let awkward = Namespace {
        author_target_identity_digest: TARGET.to_owned(),
        environment: "author one".to_owned(),
        profile: "local/other".to_owned(),
    };
    let address = maintenance_address(&awkward, IDENTIFIER);
    let ResourceAddress::MaintenanceResult { namespace: read, .. } =
        parse(&address).expect("an escaped address is still an address")
    else {
        panic!("it names a maintenance result")
    };
    assert_eq!(read, awkward, "what was escaped is what comes back");
}

#[test]
fn a_read_that_describes_the_document_the_lookup_described_is_accepted() {
    let lookup = facts(PREVIEW_OWNER, FOUND_REVISION);
    require_same_document(&lookup, &lookup).expect("nothing changed between the calls");
}

#[test]
fn the_one_change_admitted_between_a_lookup_and_a_read_is_an_apply_committing() {
    let lookup = facts(PREVIEW_OWNER, FOUND_REVISION);
    let mut transferred = facts(RECEIPT_OWNER, TRANSFERRED_REVISION);
    require_same_document(&lookup, &transferred).expect("an apply committed between the calls");

    transferred.association_revision = FURTHER_REVISION;
    assert_eq!(
        require_same_document(&lookup, &transferred),
        Err(ResourceRefusal::ReadDiverged("the owner".to_owned())),
        "a receipt two revisions on is not the transition that was admitted"
    );

    let backwards = facts(PREVIEW_OWNER, TRANSFERRED_REVISION);
    assert_eq!(
        require_same_document(&facts(RECEIPT_OWNER, FOUND_REVISION), &backwards),
        Err(ResourceRefusal::ReadDiverged("the owner".to_owned())),
        "ownership transfers one way"
    );
}

#[test]
fn every_other_difference_between_a_lookup_and_a_read_refuses_the_read() {
    let lookup = facts(PREVIEW_OWNER, FOUND_REVISION);
    let mut wrong_length = lookup.clone();
    wrong_length.byte_length += 1;
    assert_eq!(
        require_same_document(&lookup, &wrong_length),
        Err(ResourceRefusal::ReadDiverged("the length".to_owned()))
    );
    let mut wrong_digest = lookup.clone();
    wrong_digest.content_digest = TARGET.to_owned();
    assert_eq!(
        require_same_document(&lookup, &wrong_digest),
        Err(ResourceRefusal::ReadDiverged("the content digest".to_owned()))
    );
    let mut wrong_target = lookup.clone();
    wrong_target.author_target_identity_digest = IDENTIFIER.to_owned();
    assert_eq!(
        require_same_document(&lookup, &wrong_target),
        Err(ResourceRefusal::ReadDiverged("the target".to_owned()))
    );
    let mut wrong_revision = lookup.clone();
    wrong_revision.association_revision += 1;
    assert_eq!(
        require_same_document(&lookup, &wrong_revision),
        Err(ResourceRefusal::ReadDiverged("the association revision".to_owned()))
    );
}

#[test]
fn no_resource_says_anything_a_resource_may_not_say() {
    let clean = json!({
        "uri": maintenance_address(&namespace(), IDENTIFIER),
        "mediaType": MAINTENANCE_MEDIA_TYPE,
        "byteLength": DOCUMENT_BYTES,
    });
    require_disclosable(&clean).expect("an address, a media type, and a length say nothing");
    for member in UNDISCLOSABLE_MEMBERS {
        let leaking = json!({ "contents": [{ *member: "whatever it holds" }] });
        assert_eq!(
            require_disclosable(&leaking),
            Err(ResourceRefusal::Undisclosable((*member).to_owned())),
            "{member} reached a resource"
        );
    }
}

#[test]
fn a_listing_page_carries_what_it_can_and_says_what_follows() {
    let few: Vec<String> = (0..FEW_OPERATIONS).map(|index| format!("operation-{index}")).collect();
    let page = ListingPage::of(&few, None);
    assert_eq!(page.operations, few);
    assert_eq!(page.continuation_token, None, "a page holding everything continues nowhere");

    let many: Vec<String> =
        (0..MAXIMUM_LISTED_OPERATIONS + 1).map(|index| format!("operation-{index}")).collect();
    let bounded = ListingPage::of(&many, None);
    assert_eq!(bounded.operations.len(), MAXIMUM_LISTED_OPERATIONS);
    assert_eq!(bounded.continuation_token, bounded.operations.last().cloned());
}
