//! Assertions for accepting one complete committed configuration generation.
//!
//! Reading each file stably is not enough: a writer can replace a source
//! between two perfectly stable reads. What makes a set one generation is the
//! commit inventory, read before the sources and again afterwards, matching the
//! digest of every source it lists and listing exactly the sources that were
//! discovered and referenced.
//!
//! Every refusal in this file reports the same code. That is deliberate: which
//! source disagreed, and what its digest was, are facts about bytes that may be
//! secret, so a caller learns only that the generation was not whole.
//!
//! The inspector here derives nothing. It answers with an inventory the test
//! wrote down, which is what proves the coordinator holds no parser: it cannot
//! tell a profile from any other bytes.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use slingshot_configuration::configuration_generation::{
    ConfigurationGenerationCoordinator, ConfigurationGenerationFailure,
    ConfigurationSourceInventoryInspector, InspectedDocuments, InspectionOutcome,
    RoleTaggedReference, SourceRole,
};
use slingshot_configuration::testing::credential_filesystem::{
    Instability, ScriptedEntry, ScriptedFilesystem,
};
use slingshot_domain::configuration_snapshot::ConfigurationReference;
use slingshot_domain::profile_authentication_contract::ConfigurationFailureCode;

/// Directory holding the committed generations.
const GENERATION_DIRECTORY: &str = "../slingshot-test-support/fixtures/configuration-generations";

/// Reference of the credential the fixture profiles name.
const CREDENTIAL_REFERENCE: &str = "credentials/production.json";

/// Reference of the profile every fixture holds.
const PROFILE_REFERENCE: &str = "profiles/cloud-site.toml";

/// Reference of the selection every fixture holds.
const SELECTION_REFERENCE: &str = "selection.toml";

/// Reads one refused attempt takes before the next one starts.
///
/// A refused attempt reads the commit inventory and then the first source it
/// lists, and stops there, so a writer that finishes after those two reads is a
/// writer the second attempt sees complete.
const READS_BEFORE_THE_SECOND_ATTEMPT: u64 = 2;

/// An inspector that answers with an inventory the test wrote down.
///
/// It never reads a document, so a coordinator that needed one would fail here
/// rather than quietly grow a parser.
struct ScriptedInspector {
    /// References the inspection reports, with their roles.
    references: Vec<RoleTaggedReference>,
}

impl ConfigurationSourceInventoryInspector<usize> for ScriptedInspector {
    fn inspect(
        &self,
        documents: &InspectedDocuments<'_>,
    ) -> Result<InspectionOutcome<usize>, ConfigurationGenerationFailure> {
        Ok(InspectionOutcome {
            inspection: documents.profiles.len(),
            references: self.references.clone(),
        })
    }
}

/// Returns the reference `spelling` names.
fn reference(spelling: &str) -> ConfigurationReference {
    ConfigurationReference::parse(spelling).expect("the reference is valid")
}

/// Returns the inspector every whole fixture generation needs.
fn credential_inspector() -> ScriptedInspector {
    ScriptedInspector {
        references: vec![RoleTaggedReference {
            reference: reference(CREDENTIAL_REFERENCE),
            role: SourceRole::ServiceCredentials,
        }],
    }
}

/// Returns the files one committed generation holds.
fn generation_files(case: &str) -> BTreeMap<String, Vec<u8>> {
    let directory = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(GENERATION_DIRECTORY).join(case);
    let mut files = BTreeMap::new();
    collect(&directory, &directory, &mut files);
    assert!(!files.is_empty(), "{case} holds no file");
    files
}

/// Collects every file below `directory`, keyed by its root-relative spelling.
fn collect(root: &Path, directory: &Path, files: &mut BTreeMap<String, Vec<u8>>) {
    for entry in std::fs::read_dir(directory).expect("the generation directory reads") {
        let path = entry.expect("the entry reads").path();
        if path.is_dir() {
            collect(root, &path, files);
            continue;
        }
        let relative = path.strip_prefix(root).expect("the file is inside the generation");
        let spelling = relative.to_str().expect("the path is text").replace('\\', "/");
        files.insert(spelling, std::fs::read(&path).expect("the file reads"));
    }
}

/// Returns a scripted root holding one committed generation.
fn scripted_generation(case: &str) -> ScriptedFilesystem {
    let mut authority = ScriptedFilesystem::new();
    for (reference, bytes) in generation_files(case) {
        authority = authority.with_source(&reference, &bytes);
    }
    authority.with_directory("profiles")
}

#[test]
fn one_complete_generation_is_accepted_with_its_sources_role_tagged() {
    let authority = scripted_generation("complete");
    let coordinator = ConfigurationGenerationCoordinator::new(authority);
    let generation = coordinator
        .read_generation(&credential_inspector())
        .expect("the complete generation is accepted");
    assert_eq!(generation.inspection, 1, "the inspector saw another profile count");

    let profiles = generation.sources_in_role(SourceRole::Profile);
    assert_eq!(profiles.len(), 1);
    assert_eq!(profiles[0].reference.as_text(), PROFILE_REFERENCE);
    let selections = generation.sources_in_role(SourceRole::Selection);
    assert_eq!(selections.len(), 1);
    assert_eq!(selections[0].reference.as_text(), SELECTION_REFERENCE);
    let credentials = generation.sources_in_role(SourceRole::ServiceCredentials);
    assert_eq!(credentials.len(), 1);
    assert_eq!(credentials[0].reference.as_text(), CREDENTIAL_REFERENCE);
    let retained = profiles.len() + selections.len() + credentials.len();
    assert_eq!(generation.sources.len(), retained, "the generation retained another source");
}

#[test]
fn every_incomplete_generation_reports_only_that_it_was_not_whole() {
    for case in ["missing-source", "surplus-profile", "digest-mismatch"] {
        let coordinator = ConfigurationGenerationCoordinator::new(scripted_generation(case));
        let failure =
            coordinator.read_generation(&credential_inspector()).expect_err("{case} was accepted");
        assert!(
            matches!(
                failure.code,
                ConfigurationFailureCode::ConfigurationSnapshotInconsistent
                    | ConfigurationFailureCode::ConfigurationFileUnsafe
            ),
            "{case} reported {}",
            failure.code
        );
        let rendered = format!("{failure}");
        assert!(!rendered.contains("credentials"), "{rendered} names a source");
        assert!(!rendered.contains(char::is_numeric), "{rendered} carries a digest");
    }
}

#[test]
fn a_source_replaced_without_its_inventory_is_refused_by_every_attempt() {
    let files = generation_files("complete");
    let mut authority = ScriptedFilesystem::new();
    for (reference, bytes) in &files {
        authority = authority.with_source(reference, bytes);
    }
    let replacement = BTreeMap::from([(
        CREDENTIAL_REFERENCE.to_owned(),
        b"{\"ok\":true,\"statusCode\":200,\"replaced\":true}\n".to_vec(),
    )]);
    let authority = authority.with_directory("profiles").publishing_after(1, replacement);

    let coordinator = ConfigurationGenerationCoordinator::new(authority);
    let failure = coordinator
        .read_generation(&credential_inspector())
        .expect_err("a source replaced without its inventory is refused");
    assert_eq!(failure.code, ConfigurationFailureCode::ConfigurationSnapshotInconsistent);
}

#[test]
fn a_generation_that_becomes_whole_is_accepted_by_the_second_attempt() {
    let complete = generation_files("complete");
    let mut authority = ScriptedFilesystem::new();
    for (reference, bytes) in &complete {
        let entry = if reference == "configuration-snapshot.toml" {
            ScriptedEntry::safe(&generation_files("digest-mismatch")["configuration-snapshot.toml"])
        } else {
            ScriptedEntry::safe(bytes)
        };
        authority = authority.with_entry(reference, entry);
    }
    let publication = BTreeMap::from([(
        "configuration-snapshot.toml".to_owned(),
        complete["configuration-snapshot.toml"].clone(),
    )]);
    let authority = authority
        .with_directory("profiles")
        .publishing_after(READS_BEFORE_THE_SECOND_ATTEMPT, publication);

    let coordinator = ConfigurationGenerationCoordinator::new(authority);
    coordinator
        .read_generation(&credential_inspector())
        .expect("the writer finished before the second attempt");
}

#[test]
fn a_source_that_never_settles_stops_the_generation_with_its_own_code() {
    let files = generation_files("complete");
    let mut authority = ScriptedFilesystem::new();
    for (reference, bytes) in &files {
        let entry = ScriptedEntry::safe(bytes);
        let entry = if reference == CREDENTIAL_REFERENCE {
            entry.with_instability(Instability::NeverSettles)
        } else {
            entry
        };
        authority = authority.with_entry(reference, entry);
    }
    let coordinator = ConfigurationGenerationCoordinator::new(authority.with_directory("profiles"));
    let failure = coordinator
        .read_generation(&credential_inspector())
        .expect_err("an unstable source stops the generation");
    assert_eq!(failure.code, ConfigurationFailureCode::ConfigurationFileChangedDuringRead);
}

#[test]
fn an_inspector_that_claims_another_role_for_a_discovered_profile_is_refused() {
    let crossed = ScriptedInspector {
        references: vec![
            RoleTaggedReference {
                reference: reference(CREDENTIAL_REFERENCE),
                role: SourceRole::ServiceCredentials,
            },
            RoleTaggedReference {
                reference: reference(PROFILE_REFERENCE),
                role: SourceRole::ServiceCredentials,
            },
        ],
    };
    let coordinator = ConfigurationGenerationCoordinator::new(scripted_generation("complete"));
    let failure = coordinator.read_generation(&crossed).expect_err("a crossed role is refused");
    assert_eq!(failure.code, ConfigurationFailureCode::ConfigurationSnapshotInconsistent);
}

#[test]
fn every_role_names_its_own_manifest_literal_and_document_bound() {
    let roles = [
        SourceRole::Profile,
        SourceRole::Selection,
        SourceRole::ServiceCredentials,
        SourceRole::AdditionalCertificateAuthority,
    ];
    let spellings: Vec<&str> = roles.iter().map(|role| role.as_text()).collect();
    assert_eq!(
        spellings,
        vec!["profile", "selection", "service_credentials", "additional_certificate_authority"]
    );
    assert!(
        SourceRole::Selection.document_bound() < SourceRole::Profile.document_bound(),
        "a selection is bounded more tightly than a profile"
    );
}
