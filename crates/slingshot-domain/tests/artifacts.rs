//! What a result says about a file, proved to say nothing about where it is.
//!
//! The claim worth proving hardest is a negative one: a descriptor carries no
//! location. There is no remote address, no local path, no inline bytes, and
//! nothing credential-bearing, and the vectors offer each of those explicitly
//! rather than trusting that nobody will add one later.
//!
//! The second claim is that three similar-looking fields are not
//! interchangeable. Identity, purpose, and presentation are three types with
//! three alphabets, and renaming the download cannot change what the artifact
//! is.

use serde_json::Value;
use slingshot_domain::command::artifact::{
    ArtifactDescriptor, ArtifactDigest, ArtifactFailure, ArtifactIdentifier, ArtifactMediaType,
    ArtifactRequirement, ArtifactSlot, ArtifactSlotDeclaration, CONTENT_PACKAGE_FILE_NAME_SUFFIX,
    CONTENT_PACKAGE_MEDIA_TYPE, CONTENT_PACKAGE_SLOT, LOADED_CONTENT_FILE_NAME,
    LOADED_CONTENT_MEDIA_TYPE, LOADED_CONTENT_SLOT, SuggestedFileName,
    maximum_artifact_identifier_bytes, maximum_artifact_media_type_bytes,
    maximum_artifact_slot_bytes, maximum_artifact_suggested_file_name_bytes,
    maximum_loaded_content_artifact_bytes, maximum_package_output_bytes,
};

/// Vectors this test reads.
const FIXTURE: &str = include_str!("fixtures/commands/artifacts.jsonl");

/// Every refusal the fixture can name, beside the variant that produces it.
const DECLARED_REFUSALS: &[(&str, ArtifactFailure)] = &[
    ("IdentifierOutOfBounds", ArtifactFailure::IdentifierOutOfBounds),
    ("IdentifierNotPrintable", ArtifactFailure::IdentifierNotPrintable),
    ("SlotOutOfBounds", ArtifactFailure::SlotOutOfBounds),
    ("SlotNotCanonical", ArtifactFailure::SlotNotCanonical),
    ("MediaTypeOutOfBounds", ArtifactFailure::MediaTypeOutOfBounds),
    ("MediaTypeNotCanonical", ArtifactFailure::MediaTypeNotCanonical),
    ("FileNameOutOfBounds", ArtifactFailure::FileNameOutOfBounds),
    ("FileNameNotPresentational", ArtifactFailure::FileNameNotPresentational),
    ("DigestNotCanonical", ArtifactFailure::DigestNotCanonical),
    ("UnknownRequirement", ArtifactFailure::UnknownRequirement),
    ("LongerThanSlotAllows", ArtifactFailure::LongerThanSlotAllows),
    ("SlotNotDeclared", ArtifactFailure::SlotNotDeclared),
];

/// Name the fixture gives to the refusals the closed object makes on its own.
const CLOSED_OBJECT: &str = "ClosedObject";

/// A digest of bytes this test does not need to have.
const SAMPLE_DIGEST: &str = "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855";

/// Reads one row's string member.
fn text<'row>(row: &'row Value, member: &str) -> &'row str {
    row[member].as_str().unwrap_or_else(|| panic!("{member} is a string in {row}"))
}

/// Returns every fixture row of one kind.
fn rows(kind: &str) -> Vec<Value> {
    FIXTURE
        .lines()
        .map(|line| serde_json::from_str::<Value>(line).expect("every fixture line is one object"))
        .filter(|row| text(row, "kind") == kind)
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

/// Returns every sentence an artifact refusal can render as.
fn every_refusal_rendering() -> Vec<String> {
    DECLARED_REFUSALS.iter().map(|(_, failure)| failure.to_string()).collect()
}

#[test]
fn every_descriptor_vector_lands_where_the_fixture_says_it_does() {
    let vectors = rows("descriptor");
    assert!(vectors.len() >= 35, "every field is proved at both edges of its bound");
    for row in &vectors {
        let document = text(row, "document");
        let note = text(row, "note");
        let outcome = serde_json::from_str::<ArtifactDescriptor>(document);
        match (row["accepted"].as_bool(), outcome) {
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
            (Some(false), Ok(descriptor)) => panic!("{note}: accepted as {descriptor:?}"),
            (None, _) => panic!("{note}: the fixture states whether it is accepted"),
        }
    }
}

#[test]
fn every_accepted_descriptor_writes_itself_back_byte_for_byte() {
    for row in rows("descriptor").iter().filter(|row| row["accepted"] == Value::Bool(true)) {
        let document = text(row, "document");
        let descriptor: ArtifactDescriptor =
            serde_json::from_str(document).expect("the fixture says this is accepted");
        assert_eq!(
            serde_json::to_string(&descriptor).expect("a valid descriptor serializes"),
            document,
            "{}: rewritten differently",
            text(row, "note")
        );
    }
}

#[test]
fn a_descriptor_says_nothing_about_where_the_bytes_are() {
    for member in ["location", "url", "inline_bytes", "local_path", "access_token", "href"] {
        let document = format!(
            "{{\"identifier\":\"a\",\"slot\":\"loaded_content_json\",\
             \"media_type\":\"application/json\",\"byte_length\":1,\
             \"digest\":\"{SAMPLE_DIGEST}\",\"suggested_file_name\":\"a.json\",\
             \"{member}\":\"x\"}}"
        );
        assert!(
            serde_json::from_str::<ArtifactDescriptor>(&document).is_err(),
            "{member} is not a field a descriptor has"
        );
    }
    let descriptor = sample_descriptor();
    let written = serde_json::to_string(&descriptor).expect("a descriptor serializes");
    for absent in ["http", "://", "/var/", "token"] {
        assert!(!written.contains(absent), "a written descriptor carries no {absent}");
    }
}

/// Returns one valid descriptor.
fn sample_descriptor() -> ArtifactDescriptor {
    ArtifactDescriptor {
        identifier: ArtifactIdentifier::new("loaded-content-1").expect("a legal identifier"),
        slot: ArtifactSlot::new(LOADED_CONTENT_SLOT).expect("a legal slot"),
        media_type: ArtifactMediaType::new(LOADED_CONTENT_MEDIA_TYPE).expect("a legal type"),
        byte_length: 1,
        digest: ArtifactDigest::new(SAMPLE_DIGEST).expect("a legal digest"),
        suggested_file_name: SuggestedFileName::new(LOADED_CONTENT_FILE_NAME)
            .expect("a legal name"),
    }
}

#[test]
fn identity_purpose_and_presentation_are_three_different_things() {
    let descriptor = sample_descriptor();
    let renamed = ArtifactDescriptor {
        suggested_file_name: SuggestedFileName::new("something-else.json").expect("a legal name"),
        ..descriptor.clone()
    };
    assert_eq!(renamed.identifier, descriptor.identifier, "renaming changes no identity");
    assert_eq!(renamed.slot, descriptor.slot, "and no purpose");
    assert_eq!(renamed.digest, descriptor.digest, "and no content");
    assert_ne!(renamed, descriptor, "though it does change the descriptor");

    let slot_spelling = descriptor.slot.as_text().to_owned();
    let identifier_as_slot = ArtifactIdentifier::new(slot_spelling).expect("a legal identifier");
    assert_ne!(
        identifier_as_slot.as_text(),
        descriptor.identifier.as_text(),
        "a slot spelling read as an identifier is a different identifier, not this one"
    );
}

#[test]
fn each_declared_slot_is_declared_exactly_once_and_the_rest_are_forbidden() {
    let declared = ArtifactSlotDeclaration::declared();
    assert_eq!(declared.len(), 2, "two slots, and no third");
    for row in &rows("declaration") {
        let slot = text(row, "slot");
        let found = declared.iter().find(|declaration| declaration.slot.as_text() == slot);
        let note = text(row, "note");
        match (row["declared"].as_bool(), found) {
            (Some(true), Some(declaration)) => {
                assert_eq!(declaration.media_type.as_text(), text(row, "media_type"), "{note}");
                assert_eq!(declaration.requirement.as_text(), text(row, "requirement"), "{note}");
                assert_eq!(
                    declaration.maximum_byte_length,
                    row["maximum_byte_length"].as_u64().expect("a maximum"),
                    "{note}"
                );
            }
            (Some(false), None) => (),
            (_, found) => panic!("{note}: the declaration list says {found:?}"),
        }
    }
}

#[test]
fn the_two_declared_slots_say_exactly_what_this_plan_says_they_say() {
    let loaded = ArtifactSlotDeclaration::loaded_content();
    assert_eq!(loaded.slot.as_text(), LOADED_CONTENT_SLOT);
    assert_eq!(loaded.media_type.as_text(), LOADED_CONTENT_MEDIA_TYPE);
    assert_eq!(loaded.requirement, ArtifactRequirement::OptionalAlternative);
    assert_eq!(loaded.maximum_byte_length, maximum_loaded_content_artifact_bytes());
    assert!(
        SuggestedFileName::new(LOADED_CONTENT_FILE_NAME).is_ok(),
        "the exact name it suggests is itself a legal name"
    );

    let package = ArtifactSlotDeclaration::content_package();
    assert_eq!(package.slot.as_text(), CONTENT_PACKAGE_SLOT);
    assert_eq!(package.media_type.as_text(), CONTENT_PACKAGE_MEDIA_TYPE);
    assert_eq!(package.requirement, ArtifactRequirement::Required);
    assert_eq!(package.maximum_byte_length, maximum_package_output_bytes());
    let suggested = format!("example-pages{CONTENT_PACKAGE_FILE_NAME_SUFFIX}");
    assert!(SuggestedFileName::new(suggested).is_ok(), "and so is the archive's");
    assert_ne!(loaded.slot, package.slot, "the two slots are distinct");
}

#[test]
fn a_slot_admits_exactly_what_it_declares_it_admits() {
    for row in &rows("admission") {
        let slot = text(row, "slot");
        let note = text(row, "note");
        let declaration = if slot == CONTENT_PACKAGE_SLOT {
            ArtifactSlotDeclaration::content_package()
        } else {
            ArtifactSlotDeclaration::loaded_content()
        };
        let descriptor = ArtifactDescriptor {
            slot: ArtifactSlot::new(slot).expect("the fixture names a legal slot"),
            byte_length: row["byte_length"].as_u64().expect("a byte length"),
            ..sample_descriptor()
        };
        let outcome = declaration.admit(&descriptor);
        match (row["admitted"].as_bool(), outcome) {
            (Some(true), Ok(())) => (),
            (Some(false), Err(failure)) => assert_eq!(
                Some(failure.to_string()),
                refusal_rendering(text(row, "reason")),
                "{note}"
            ),
            (_, outcome) => panic!("{note}: admission answered {outcome:?}"),
        }
    }
}

#[test]
fn every_named_bound_comes_from_the_manifest_rather_than_from_here() {
    let contract = slingshot_domain::command::command_identity::CommandContract::embedded();
    assert_eq!(
        maximum_artifact_identifier_bytes(),
        contract.limit("maximum_artifact_identifier_bytes")
    );
    assert_eq!(maximum_artifact_slot_bytes(), contract.limit("maximum_artifact_slot_bytes"));
    assert_eq!(
        maximum_artifact_media_type_bytes(),
        contract.limit("maximum_artifact_media_type_bytes")
    );
    assert_eq!(
        maximum_artifact_suggested_file_name_bytes(),
        contract.limit("maximum_artifact_suggested_file_name_bytes")
    );
    assert_eq!(
        maximum_loaded_content_artifact_bytes(),
        contract.limit("maximum_loaded_content_artifact_bytes")
    );
    assert_eq!(maximum_package_output_bytes(), contract.limit("maximum_package_output_bytes"));
}
