//! Where an artifact's name comes from, and what it never comes from.
//!
//! The identifier is a digest over five fields. These prove the mapping is
//! deterministic in both directions: the same fields always name the same
//! artifact, and every field that differs names another one - including the
//! partition, which is the field two otherwise identical requests differ by
//! when the same deployment is reached through two opaque principals.

use std::collections::BTreeSet;

use slingshot_domain::installation::InstallationIdentifier;
use slingshot_storage::artifact_store::{
    ARTIFACT_IDENTIFIER_VERSION, ArtifactFailure, ArtifactIdentifier, DIGEST_CHARACTERS,
    MAXIMUM_ARTIFACT_ACCESS_BYTES, MAXIMUM_ARTIFACT_SLOT_BYTES, MAXIMUM_BYTE_LENGTH_CHARACTERS,
    MAXIMUM_DESCRIPTOR_BYTES, MAXIMUM_MEDIA_TYPE_BYTES, STRUCTURED_RESULT_SLOT,
};

use crate::fixtures::*;

#[test]
fn every_identifier_vector_derives_the_digest_the_fixture_records() {
    let vectors = rows(IDENTIFIERS);
    assert!(vectors.len() >= 6, "every field, varied one at a time");
    let mut derived = BTreeSet::new();
    for row in &vectors {
        let installation = InstallationIdentifier::parse(text(row, "installation_identifier"))
            .expect("a legal identifier");
        let identifier = ArtifactIdentifier::derive(
            &installation,
            text(row, "author_target_identity_digest"),
            text(row, "operation_identifier"),
            text(row, "artifact_slot"),
        );
        assert_eq!(identifier.as_text(), text(row, "artifact_identifier"), "{}", text(row, "note"));
        derived.insert(identifier.as_text().to_owned());
    }
    assert_eq!(
        derived.len(),
        vectors.len() - 1,
        "one vector repeats another's fields on purpose, and every other pair differs"
    );
}

#[test]
fn an_identifier_is_a_digest_and_reads_back_as_one() {
    let identifier = ArtifactIdentifier::derive(
        &installation(),
        &partition(FIRST_PRINCIPAL),
        "operation-1",
        STRUCTURED_RESULT_SLOT,
    );
    assert_eq!(identifier.as_text().len(), DIGEST_CHARACTERS);
    assert_eq!(
        ArtifactIdentifier::parse(identifier.as_text()).expect("a legal identifier"),
        identifier,
        "an identifier reads back as the one it is"
    );
    for wrong in
        ["", "not-a-digest", &"a".repeat(DIGEST_CHARACTERS - 1), &"A".repeat(DIGEST_CHARACTERS)]
    {
        assert!(
            matches!(ArtifactIdentifier::parse(wrong), Err(ArtifactFailure::DigestNotCanonical)),
            "{wrong} is not a digest"
        );
    }
    assert_eq!(
        ARTIFACT_IDENTIFIER_VERSION, "slingshot.artifact-identifier/1",
        "the version marker is part of the preimage, so it is part of the contract"
    );
}

#[test]
fn a_file_name_never_becomes_an_identity() {
    let (_directory, store) = store();
    let first = store
        .install(
            &request(&partition(FIRST_PRINCIPAL), "operation-1", "content_package"),
            &mut content("one-octet").as_slice(),
        )
        .expect("an installation");
    let renamed = store
        .install(
            &request(&partition(FIRST_PRINCIPAL), "operation-1", "content_package"),
            &mut content("duplicate-of-one-octet").as_slice(),
        )
        .expect("an installation of the same bytes from another fixture file");
    assert_eq!(
        first.artifact_identifier, renamed.artifact_identifier,
        "the identifier is derived from the operation and slot, not from where the bytes came from"
    );
    assert_eq!(
        first.content_digest, renamed.content_digest,
        "and the bytes address the same content"
    );
}

#[test]
fn the_whole_access_record_fits_the_machine_envelope() {
    let widest = MAXIMUM_ARTIFACT_SLOT_BYTES
        + MAXIMUM_MEDIA_TYPE_BYTES
        + MAXIMUM_DESCRIPTOR_BYTES
        + MAXIMUM_BYTE_LENGTH_CHARACTERS
        + DIGEST_CHARACTERS
        + DIGEST_CHARACTERS;
    assert!(
        widest < MAXIMUM_ARTIFACT_ACCESS_BYTES,
        "every field at its own bound still leaves the record inside the envelope: \
         {widest} against {MAXIMUM_ARTIFACT_ACCESS_BYTES}"
    );

    let (_directory, store) = store();
    let mut asked = request(&partition(FIRST_PRINCIPAL), "operation-1", "content_package");
    asked.artifact_slot = "s".repeat(MAXIMUM_ARTIFACT_SLOT_BYTES);
    asked.media_type = "m".repeat(MAXIMUM_MEDIA_TYPE_BYTES);
    asked.descriptor = Some("d".repeat(MAXIMUM_DESCRIPTOR_BYTES));
    let widest_record = store
        .install(&asked, &mut content("one-octet").as_slice())
        .expect("every field at its bound is legal");
    assert!(
        widest_record.access_bytes() < MAXIMUM_ARTIFACT_ACCESS_BYTES,
        "and the record it produces is inside the envelope too"
    );

    for (field, over) in [
        ("artifact slot", "s".repeat(MAXIMUM_ARTIFACT_SLOT_BYTES + 1)),
        ("media type", "m".repeat(MAXIMUM_MEDIA_TYPE_BYTES + 1)),
        ("descriptor", "d".repeat(MAXIMUM_DESCRIPTOR_BYTES + 1)),
    ] {
        let mut asked = request(&partition(SECOND_PRINCIPAL), "operation-1", "content_package");
        match field {
            "artifact slot" => asked.artifact_slot = over,
            "media type" => asked.media_type = over,
            _ => asked.descriptor = Some(over),
        }
        let refused = store.install(&asked, &mut content("one-octet").as_slice());
        assert!(
            matches!(refused, Err(ArtifactFailure::TooLong { field: named, .. }) if named == field),
            "one byte past the {field} bound is refused: {refused:?}"
        );
    }
}
