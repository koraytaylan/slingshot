//! Assertions for what a write answers with and what one write carries inward.
//!
//! The payload assertions are the ones worth reading. The encoded bound is
//! checked before decoding, which is not an optimization: a request whose
//! encoded form is over the bound must be refused without allocating the decoded
//! form, and the fixture that proves it is one whose decoded form would have
//! been comfortably under the decoded bound.

use slingshot_domain::command::command_identity::CommandContract;
use slingshot_domain::command::create_page::MutationProperties;
use slingshot_domain::command::property_value::PropertyValue;
use slingshot_domain::command::repository_path::PropertyName;
use slingshot_domain::command::repository_path::RepositoryPath;
use slingshot_domain::command::resource_mutation::{
    DeletedResourceResult, InlineBinaryPayload, InlinePayloadFailure, MovedResourceResult,
    MutationResultFailure, PropertyMutationFailure, ReferencePolicy, RemovedPropertyNames,
    ResourceMutationResult, require_property_mutation,
};

/// Returns one limit by name.
fn limit(name: &str) -> u64 {
    CommandContract::embedded().limit(name)
}

/// Returns one legal path.
fn path(text: &str) -> RepositoryPath {
    RepositoryPath::parse(text).expect("a legal path")
}

#[test]
fn a_mutation_result_answers_only_the_request_that_determined_its_address() {
    let answered = ResourceMutationResult { repository_path: path("/content/example/jcr:content") };
    assert_eq!(answered.require_answers(&path("/content/example/jcr:content")), Ok(()));
    assert_eq!(
        answered.require_answers(&path("/content/other/jcr:content")),
        Err(MutationResultFailure::NotThisRequest)
    );
}

#[test]
fn a_mutation_result_round_trips_and_refuses_an_unknown_member() {
    let written = "{\"repository_path\":\"/content/example\"}";
    let read: ResourceMutationResult = serde_json::from_str(written).expect("a result parses");
    assert_eq!(serde_json::to_string(&read).expect("a result serializes"), written);
    assert!(
        serde_json::from_str::<ResourceMutationResult>(
            "{\"repository_path\":\"/content/example\",\"extra\":1}"
        )
        .is_err()
    );
}

#[test]
fn a_deletion_count_is_accepted_at_its_bound_and_refused_one_past_it() {
    let bound = limit("maximum_deleted_nodes");
    assert!(DeletedResourceResult::new(path("/content/example"), bound).is_ok());
    assert_eq!(
        DeletedResourceResult::new(path("/content/example"), bound + 1),
        Err(MutationResultFailure::CountTooLarge)
    );
}

#[test]
fn a_move_count_is_accepted_at_its_bound_and_refused_one_past_it() {
    let bound = limit("maximum_adjusted_references");
    assert!(MovedResourceResult::new(path("/content/a"), path("/content/b"), bound).is_ok());
    assert_eq!(
        MovedResourceResult::new(path("/content/a"), path("/content/b"), bound + 1),
        Err(MutationResultFailure::CountTooLarge)
    );
}

#[test]
fn a_move_refuses_a_destination_at_or_inside_its_own_source() {
    for destination in ["/content/a", "/content/a/child", "/content/a/child/deeper"] {
        assert_eq!(
            MovedResourceResult::new(path("/content/a"), path(destination), 0),
            Err(MutationResultFailure::DestinationInsideSource),
            "{destination} was accepted as a destination"
        );
    }
    // The boundary the other way: a sibling whose name merely begins the same.
    assert!(MovedResourceResult::new(path("/content/a"), path("/content/ab"), 0).is_ok());
}

#[test]
fn a_move_answers_only_the_request_that_determined_both_addresses() {
    let moved =
        MovedResourceResult::new(path("/content/a"), path("/content/b"), 0).expect("a legal move");
    assert_eq!(moved.require_answers(&path("/content/a"), &path("/content/b")), Ok(()));
    assert_eq!(
        moved.require_answers(&path("/content/a"), &path("/content/c")),
        Err(MutationResultFailure::NotThisRequest)
    );
}

#[test]
fn a_reference_policy_has_two_spellings_and_no_default() {
    for (policy, spelling) in [
        (ReferencePolicy::IgnoreReferences, "\"ignore_references\""),
        (ReferencePolicy::RefuseWhenReferenced, "\"refuse_when_referenced\""),
    ] {
        assert_eq!(serde_json::to_string(&policy).expect("a policy serializes"), spelling);
        assert_eq!(
            serde_json::from_str::<ReferencePolicy>(spelling).expect("a policy parses"),
            policy
        );
    }
    assert!(serde_json::from_str::<ReferencePolicy>("\"maybe\"").is_err());
}

#[test]
fn a_payload_decodes_to_exactly_the_bytes_it_was_given() {
    let payload = InlineBinaryPayload::new("image/png", "aGVsbG8=").expect("a legal payload");
    assert_eq!(payload.media_type(), "image/png");
    assert_eq!(payload.encoded_content(), "aGVsbG8=");
    assert_eq!(payload.decoded_content(), b"hello");
    assert_eq!(payload.decoded_byte_length(), u64::try_from(b"hello".len()).expect("it fits"));
}

#[test]
fn a_payload_refuses_every_spelling_that_is_not_canonical_base64() {
    for refused in ["aGVsbG8", "aGVsbG8==", "aGVs bG8=", "aGVs\nbG8=", "aGVsbG8*", "===="] {
        assert_eq!(
            InlineBinaryPayload::new("image/png", refused),
            Err(InlinePayloadFailure::EncodingMalformed),
            "{refused:?} was accepted as canonical Base64"
        );
    }
}

#[test]
fn a_payload_names_a_media_type_within_its_bound() {
    let bound = usize::try_from(limit("maximum_inline_binary_media_type_bytes")).expect("it fits");
    assert!(InlineBinaryPayload::new(&"a".repeat(bound), "").is_ok());
    assert_eq!(
        InlineBinaryPayload::new(&"a".repeat(bound + 1), ""),
        Err(InlinePayloadFailure::MediaTypeRejected)
    );
    assert_eq!(InlineBinaryPayload::new("", ""), Err(InlinePayloadFailure::MediaTypeRejected));
}

#[test]
fn both_payload_bounds_are_accepted_exactly_and_refused_one_step_beyond() {
    let encoded_bound =
        usize::try_from(limit("maximum_inline_binary_encoded_bytes")).expect("the bound fits");
    let decoded_bound =
        usize::try_from(limit("maximum_inline_binary_decoded_bytes")).expect("the bound fits");

    // The canonical encoding of exactly the decoded bound: one padding
    // character, because the bound is not a multiple of a Base64 group's input.
    let exact = format!("{}=", "A".repeat(encoded_bound - "=".len()));
    let accepted = InlineBinaryPayload::new("application/octet-stream", &exact)
        .expect("the encoded bound itself was refused");
    assert_eq!(accepted.encoded_content().len(), encoded_bound);
    assert_eq!(
        accepted.decoded_byte_length(),
        u64::try_from(decoded_bound).expect("the bound fits"),
        "the exact fixture is not the decoded bound itself"
    );

    // The same length with the padding spent on content instead: within the
    // encoded bound and one byte past the decoded bound.
    assert_eq!(
        InlineBinaryPayload::new("application/octet-stream", &"A".repeat(encoded_bound)),
        Err(InlinePayloadFailure::DecodedTooLarge)
    );

    // One whole group past the encoded bound.
    assert_eq!(
        InlineBinaryPayload::new(
            "application/octet-stream",
            &"A".repeat(encoded_bound + "AAAA".len())
        ),
        Err(InlinePayloadFailure::EncodedTooLarge)
    );
}

#[test]
fn the_encoded_bound_is_checked_before_the_bytes_are_read_at_all() {
    let encoded_bound =
        usize::try_from(limit("maximum_inline_binary_encoded_bytes")).expect("the bound fits");
    // Over the encoded bound and also not Base64. Which failure comes back says
    // which check ran first, and the length check has to be the one that did.
    let oversized_and_malformed = "*".repeat(encoded_bound + "AAAA".len());
    assert_eq!(
        InlineBinaryPayload::new("application/octet-stream", &oversized_and_malformed),
        Err(InlinePayloadFailure::EncodedTooLarge),
        "an oversized payload was decoded before its length was checked"
    );
}

#[test]
fn a_payload_round_trips_and_refuses_an_unknown_member() {
    let written = "{\"encoded_content\":\"aGVsbG8=\",\"media_type\":\"text/plain\"}";
    let read: InlineBinaryPayload = serde_json::from_str(written).expect("a payload parses");
    assert_eq!(read.decoded_content(), b"hello");
    assert!(
        serde_json::from_str::<InlineBinaryPayload>(
            "{\"encoded_content\":\"aGVsbG8=\",\"media_type\":\"text/plain\",\"extra\":1}"
        )
        .is_err()
    );
    assert!(
        serde_json::from_str::<InlineBinaryPayload>(
            "{\"encoded_content\":\"not base64\",\"media_type\":\"text/plain\"}"
        )
        .is_err()
    );
}

/// Returns one removal list from the names it should carry.
fn removals(names: &[&str]) -> Vec<PropertyName> {
    names.iter().map(|name| PropertyName::parse(name).expect("a legal property name")).collect()
}

/// Returns one assignment document naming `name`.
fn assignment(name: &str) -> MutationProperties {
    let value: PropertyValue =
        serde_json::from_str(r#"{"cardinality":"single","value":{"type":"string","value":"x"}}"#)
            .expect("a legal property value");
    let mut values = std::collections::BTreeMap::new();
    values.insert(name.to_owned(), value);
    MutationProperties::new(values, &[]).expect("a legal property document")
}

#[test]
fn a_removal_list_is_nonempty_ascending_and_distinct() {
    assert!(RemovedPropertyNames::new(removals(&["dc:description", "jcr:description"])).is_ok());
    assert_eq!(
        RemovedPropertyNames::new(Vec::new()),
        Err(PropertyMutationFailure::RemovalsNotAscendingDistinct)
    );
    assert_eq!(
        RemovedPropertyNames::new(removals(&["dc:description", "dc:description"])),
        Err(PropertyMutationFailure::RemovalsNotAscendingDistinct)
    );
    assert_eq!(
        RemovedPropertyNames::new(removals(&["jcr:description", "dc:description"])),
        Err(PropertyMutationFailure::RemovalsNotAscendingDistinct)
    );
}

#[test]
fn a_removal_list_is_accepted_at_its_bound_and_refused_one_name_past_it() {
    let bound = usize::try_from(limit("maximum_removed_property_names")).expect("the bound fits");
    let names: Vec<PropertyName> = (0..=bound)
        .map(|index| PropertyName::parse(&format!("p{index:06}")).expect("a legal name"))
        .collect();
    assert!(RemovedPropertyNames::new(names[..bound].to_vec()).is_ok());
    assert_eq!(RemovedPropertyNames::new(names), Err(PropertyMutationFailure::TooManyRemovals));
}

#[test]
fn a_property_is_assigned_or_removed_and_never_both_by_one_request() {
    let assigned = assignment("dc:description");
    let removed = RemovedPropertyNames::new(removals(&["dc:description"])).expect("a legal list");
    assert_eq!(
        require_property_mutation(Some(&assigned), Some(&removed), false),
        Err(PropertyMutationFailure::BothAssignedAndRemoved)
    );
    let elsewhere =
        RemovedPropertyNames::new(removals(&["jcr:description"])).expect("a legal list");
    assert_eq!(require_property_mutation(Some(&assigned), Some(&elsewhere), false), Ok(()));
}

#[test]
fn a_mutation_that_changes_nothing_is_refused_and_a_title_alone_is_not() {
    assert_eq!(
        require_property_mutation(None, None, false),
        Err(PropertyMutationFailure::ChangesNothing)
    );
    assert_eq!(require_property_mutation(None, None, true), Ok(()));
    let empty = MutationProperties::new(std::collections::BTreeMap::new(), &[])
        .expect("an empty document is legal on its own");
    assert_eq!(
        require_property_mutation(Some(&empty), None, false),
        Err(PropertyMutationFailure::ChangesNothing),
        "an empty assignment document was mistaken for a change"
    );
}

#[test]
fn a_removal_list_round_trips_and_refuses_an_unordered_document() {
    let written = "[\"dc:description\",\"jcr:description\"]";
    let read: RemovedPropertyNames = serde_json::from_str(written).expect("a list parses");
    assert_eq!(serde_json::to_string(&read).expect("a list serializes"), written);
    assert!(
        serde_json::from_str::<RemovedPropertyNames>("[\"jcr:description\",\"dc:description\"]")
            .is_err()
    );
    assert!(serde_json::from_str::<RemovedPropertyNames>("[]").is_err());
}
