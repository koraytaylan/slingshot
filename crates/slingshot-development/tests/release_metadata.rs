//! What a release ships under, and what it refuses to ship under.
//!
//! The declaration is a statement about what somebody else may do with this
//! software, so the suite treats every part of it as a fact to be checked
//! rather than a value to be trusted: the material is digested and compared,
//! Cargo's own declaration is read back and compared, every member manifest is
//! required to inherit rather than declare, and the address is required to be
//! the one the automation authority already validated.
//!
//! The refusal cases are the point. A placeholder that parsed, a path that left
//! the repository, a license Cargo declared differently, or a package that
//! could be published would each ship something the owner did not choose.

use std::path::{Path, PathBuf};

use serde_json::Value;
use slingshot_development::github_automation_authority::{AUTHORITY_PATH, parse_authority};
use slingshot_development::release_metadata::{
    METADATA_FORMAT, METADATA_PATH, MetadataRefusal, PLACEHOLDERS, ReleaseMetadata,
    WORKSPACE_MANIFEST, parse_metadata, require_authoritative_address, require_material,
    require_member_inherits, require_workspace_agreement,
};

/// Where the fixtures live.
const FIXTURES: &str = "tests/fixtures/release-metadata";

/// The expression the owner selected.
const SELECTED_EXPRESSION: &str = "MIT OR Apache-2.0";

/// Returns the workspace root.
fn workspace_root() -> PathBuf {
    slingshot_development::locate_workspace_root(Path::new(env!("CARGO_MANIFEST_DIR")))
        .expect("the development crate lives inside the workspace")
}

/// Returns one repository file's text.
fn read_repository_file(relative: &str) -> String {
    let path = workspace_root().join(relative);
    std::fs::read_to_string(&path)
        .unwrap_or_else(|failure| panic!("{} could not be read: {failure}", path.display()))
}

/// Returns the rows one fixture states.
fn fixture_rows(name: &str) -> Vec<Value> {
    std::fs::read_to_string(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(FIXTURES).join(name))
        .unwrap_or_else(|failure| panic!("{name} could not be read: {failure}"))
        .lines()
        .filter(|line| !line.trim().is_empty() && !line.starts_with('#'))
        .map(|line| serde_json::from_str(line).expect("every row reads"))
        .collect()
}

/// Returns the committed declaration.
fn committed() -> ReleaseMetadata {
    parse_metadata(&read_repository_file(METADATA_PATH)).expect("the committed metadata parses")
}

/// Returns which refusal one failure is.
fn refusal_name(failure: &MetadataRefusal) -> &'static str {
    match failure {
        MetadataRefusal::Unreadable(_) => "Unreadable",
        MetadataRefusal::ForeignFormat(_) => "ForeignFormat",
        MetadataRefusal::Placeholder { .. } => "Placeholder",
        MetadataRefusal::Absent(_) => "Absent",
        MetadataRefusal::MaterialPathUnsafe(_) => "MaterialPathUnsafe",
        MetadataRefusal::MaterialDrift { .. } => "MaterialDrift",
        MetadataRefusal::CargoDrift { .. } => "CargoDrift",
        MetadataRefusal::CargoDeclaredTwice => "CargoDeclaredTwice",
        MetadataRefusal::PackagePublishable(_) => "PackagePublishable",
        MetadataRefusal::AddressDrift { .. } => "AddressDrift",
    }
}

#[test]
fn the_committed_declaration_is_the_one_the_owner_selected() {
    let held = committed();
    assert_eq!(held.format, METADATA_FORMAT);
    assert_eq!(held.license.expression, SELECTED_EXPRESSION);
    assert_eq!(held.license.material, "LICENSE");
    assert!(!held.packages.publish, "every member stays unpublished");
}

#[test]
fn the_committed_material_is_exactly_the_bytes_the_declaration_names() {
    let held = committed();
    require_material(&held, &workspace_root()).expect("the material is what it says it is");
    let license = read_repository_file(&held.license.material);
    assert_eq!(license.len() as u64, held.license.material_bytes);
    assert!(license.contains(SELECTED_EXPRESSION), "the material states the choice it offers");
    assert!(license.contains("The MIT License"), "and carries the first text in full");
    assert!(
        license.contains("Apache License"),
        "and the second, because an archive has one member"
    );
    assert!(license.contains("Koray Taylan Davgana"), "and names who holds the copyright");
}

#[test]
fn one_changed_byte_of_material_is_no_longer_the_declared_material() {
    let held = committed();
    let root = std::env::temp_dir().join(format!("release-metadata-{}", std::process::id()));
    std::fs::remove_dir_all(&root).ok();
    std::fs::create_dir_all(&root).expect("a temporary root is created");
    let altered = format!("{} ", read_repository_file(&held.license.material));
    std::fs::write(root.join(&held.license.material), altered).expect("the material is written");
    let failure = require_material(&held, &root).expect_err("one byte is a different document");
    assert_eq!(refusal_name(&failure), "MaterialDrift");
    std::fs::remove_dir_all(&root).ok();
}

#[test]
fn material_that_is_not_there_is_refused_rather_than_treated_as_empty() {
    let held = committed();
    let root = std::env::temp_dir().join(format!("release-metadata-absent-{}", std::process::id()));
    std::fs::remove_dir_all(&root).ok();
    std::fs::create_dir_all(&root).expect("a temporary root is created");
    let failure = require_material(&held, &root).expect_err("there is no material");
    assert_eq!(refusal_name(&failure), "Unreadable");
    std::fs::remove_dir_all(&root).ok();
}

#[test]
fn every_declared_change_to_the_declaration_is_refused_for_its_own_reason() {
    let committed_text = read_repository_file(METADATA_PATH);
    let declared = fixture_rows("refused-declarations.jsonl");
    assert!(!declared.is_empty());
    for row in declared {
        let name = row["name"].as_str().expect("a name");
        let find = row["find"].as_str().expect("a find");
        assert!(committed_text.contains(find), "{name}: the declaration has no {find:?}");
        let altered =
            committed_text.replacen(find, row["replace"].as_str().expect("a replacement"), 1);
        let failure = parse_metadata(&altered).expect_err(&format!("{name} was accepted"));
        assert_eq!(
            refusal_name(&failure),
            row["refusal"].as_str().expect("a refusal"),
            "{name}: {failure}"
        );
    }
}

#[test]
fn every_placeholder_the_policy_names_is_refused_wherever_it_appears() {
    let committed_text = read_repository_file(METADATA_PATH);
    for placeholder in PLACEHOLDERS {
        let altered = committed_text.replacen(
            &format!("expression = \"{SELECTED_EXPRESSION}\""),
            &format!("expression = \"{placeholder}\""),
            1,
        );
        let failure = parse_metadata(&altered).expect_err(&format!("{placeholder} was accepted"));
        assert_eq!(refusal_name(&failure), "Placeholder", "{placeholder}");
    }
}

#[test]
fn the_workspace_manifest_declares_exactly_what_the_declaration_does() {
    let held = committed();
    require_workspace_agreement(&held, &read_repository_file(WORKSPACE_MANIFEST))
        .expect("Cargo and the declaration agree");
}

#[test]
fn every_refused_workspace_manifest_earns_its_own_reason() {
    let held = committed();
    for row in fixture_rows("refused-manifests.jsonl") {
        let name = row["name"].as_str().expect("a name");
        let manifest = format!("[workspace]\n{}", row["manifest"].as_str().expect("a manifest"));
        let failure = require_workspace_agreement(&held, &manifest)
            .expect_err(&format!("{name} was accepted"));
        assert_eq!(
            refusal_name(&failure),
            row["refusal"].as_str().expect("a refusal"),
            "{name}: {failure}"
        );
    }
}

#[test]
fn every_member_inherits_the_declaration_rather_than_making_one() {
    let mut members = 0_usize;
    let crates = workspace_root().join("crates");
    for entry in std::fs::read_dir(&crates).expect("the crates directory reads") {
        let entry = entry.expect("one directory entry");
        let manifest = entry.path().join("Cargo.toml");
        if !manifest.is_file() {
            continue;
        }
        let name = entry.file_name().to_string_lossy().to_string();
        let text = std::fs::read_to_string(&manifest).expect("the manifest reads");
        require_member_inherits(&name, &text).unwrap_or_else(|failure| panic!("{name}: {failure}"));
        members += 1;
    }
    assert!(members > 0, "the workspace has members");
}

#[test]
fn a_member_that_declares_its_own_license_is_refused() {
    let held = "[package]\nname = \"probe\"\nlicense = \"MIT\"\npublish.workspace = true\n";
    let failure = require_member_inherits("probe", held).expect_err("it declares its own");
    assert_eq!(refusal_name(&failure), "CargoDrift");

    let publishable = "[package]\nname = \"probe\"\nlicense.workspace = true\npublish = true\n";
    let failure = require_member_inherits("probe", publishable).expect_err("it could be published");
    assert_eq!(refusal_name(&failure), "PackagePublishable");
}

#[test]
fn the_address_is_the_one_the_automation_authority_validated() {
    let held = committed();
    let authority = parse_authority(&read_repository_file(AUTHORITY_PATH)).expect("it parses");
    require_authoritative_address(
        &held,
        &authority.repository.canonical_address,
        authority.repository.owner_identifier,
    )
    .expect("one address, validated once");

    let failure = require_authoritative_address(
        &held,
        "https://github.com/somebody/else",
        authority.repository.owner_identifier,
    )
    .expect_err("another address");
    assert_eq!(refusal_name(&failure), "AddressDrift");
    let failure = require_authoritative_address(
        &held,
        &authority.repository.canonical_address,
        authority.repository.owner_identifier + 1,
    )
    .expect_err("another account");
    assert_eq!(refusal_name(&failure), "AddressDrift");
}
