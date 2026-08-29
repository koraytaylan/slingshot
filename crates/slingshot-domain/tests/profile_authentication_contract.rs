//! Assertions for the normative profile-authentication contract.
//!
//! The manifest is the only place a Plan 0002 value is allowed to exist, so
//! these assertions are about completeness rather than spot checks. Rendering
//! the parsed contract reproduces the committed bytes, which can only happen if
//! the typed reader carries every value the manifest declares, in the manifest's
//! own order, and declares nothing the manifest does not. A recorded mutation of
//! the manifest must then fail, and the values that are sums of other values
//! must still add up.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use serde::Deserialize;
use slingshot_domain::profile_authentication_contract::{
    CONTRACT_FORMAT, ConfigurationFailureCode, ProfileAuthenticationContract,
};

/// Directory holding the fixtures these assertions read.
const FIXTURE_DIRECTORY: &str =
    "../slingshot-test-support/fixtures/profile-authentication-contract";

/// Path of the committed manifest, relative to the workspace root.
const MANIFEST_PATH: &str = "../../policy/profile-authentication-contract-1.json";

/// Directory holding the recorded manifest mutations.
const REJECTED_DIRECTORY: &str = "rejected-manifests";

/// Directory holding the depth documents.
const DEPTH_DIRECTORY: &str = "json-depth";

/// Depth a scalar and an empty container both reach.
const BASE_DEPTH: u64 = 1;

/// The derivations fixture.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct Derivations {
    /// Format identifier of the fixture.
    format: String,
    /// One entry per derived maximum.
    derivation: Vec<Derivation>,
}

/// One maximum that is the sum of other maxima.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct Derivation {
    /// Name the fixture gives the derivation.
    name: String,
    /// Limit the terms must add up to.
    total: String,
    /// Terms the worst case charges.
    terms: Vec<DerivationTerm>,
}

/// One term of a derived maximum.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct DerivationTerm {
    /// Limit the term charges.
    limit: String,
    /// How many times the worst case charges it.
    charges: u64,
}

/// The truncation fixture.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct TruncationVectors {
    /// Format identifier of the fixture.
    format: String,
    /// One entry per candidate set size.
    vector: Vec<TruncationVector>,
}

/// One candidate set of distinct diagnostics.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct TruncationVector {
    /// Which size the candidate set has, named against the contract.
    distinct: String,
    /// Whether the result carries a marker.
    truncated: bool,
    /// Occurrences the marker reports, when there is one.
    marker_occurrences: Option<u64>,
}

/// The depth fixture index.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct DepthIndex {
    /// Format identifier of the fixture.
    format: String,
    /// One entry per depth document.
    document: Vec<DepthDocument>,
}

/// One document and where its depth sits.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct DepthDocument {
    /// File name of the document.
    name: String,
    /// Depth relative to the contract ceiling.
    offset: Option<i64>,
    /// Depth as an absolute value, for the recurrence's base cases.
    absolute: Option<u64>,
}

/// Returns the directory this crate's manifest lives in.
fn crate_directory() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

/// Reads one fixture owned by these assertions.
fn read_fixture(name: &str) -> String {
    let path = crate_directory().join(FIXTURE_DIRECTORY).join(name);
    std::fs::read_to_string(&path)
        .unwrap_or_else(|failure| panic!("{} could not be read: {failure}", path.display()))
}

/// Parses one fixture document.
fn parse_fixture<Shape: serde::de::DeserializeOwned>(name: &str) -> Shape {
    toml::from_str(&read_fixture(name))
        .unwrap_or_else(|failure| panic!("{name} could not be read: {failure}"))
}

/// Returns every file in one fixture directory, in name order.
fn fixture_files(directory: &str) -> Vec<PathBuf> {
    let path = crate_directory().join(FIXTURE_DIRECTORY).join(directory);
    let mut files: Vec<PathBuf> = std::fs::read_dir(&path)
        .unwrap_or_else(|failure| panic!("{} is unreadable: {failure}", path.display()))
        .map(|entry| entry.expect("the directory entry is readable").path())
        .collect();
    files.sort();
    files
}

/// Returns the limit one fixture names.
fn named_limit(contract: &ProfileAuthenticationContract, name: &str) -> u64 {
    let rendered = serde_json::to_value(&contract.limits).expect("the limits render");
    rendered
        .get(name)
        .and_then(serde_json::Value::as_u64)
        .unwrap_or_else(|| panic!("the contract declares no limit named {name}"))
}

/// Returns the root-inclusive depth of one document, under the contract's
/// recurrence: a scalar and an empty container are one, and a nonempty
/// container is one more than its deepest direct child.
fn measure_depth(value: &serde_json::Value) -> u64 {
    let children: Vec<&serde_json::Value> = match value {
        serde_json::Value::Object(members) => members.values().collect(),
        serde_json::Value::Array(items) => items.iter().collect(),
        _ => Vec::new(),
    };
    BASE_DEPTH + children.iter().copied().map(measure_depth).max().unwrap_or(0)
}

#[test]
fn rendering_the_parsed_contract_reproduces_the_committed_manifest() {
    let manifest = std::fs::read_to_string(crate_directory().join(MANIFEST_PATH))
        .expect("the committed manifest is readable");
    assert_eq!(
        manifest,
        ProfileAuthenticationContract::embedded_manifest(),
        "the embedded bytes are not the committed bytes"
    );
    let rendered =
        ProfileAuthenticationContract::embedded().render().expect("the contract renders");
    assert_eq!(rendered, manifest, "the typed reader is not a complete mirror of the manifest");
    assert_eq!(ProfileAuthenticationContract::embedded().format, CONTRACT_FORMAT);
}

#[test]
fn the_rust_registry_matches_the_manifest_registry_in_spelling_and_order() {
    let declared: Vec<&str> =
        ConfigurationFailureCode::REGISTRY.iter().map(|code| code.code()).collect();
    let recorded: Vec<&str> = ProfileAuthenticationContract::embedded()
        .failure_codes
        .iter()
        .map(String::as_str)
        .collect();
    assert_eq!(declared, recorded);
    let distinct: BTreeSet<&str> = declared.iter().copied().collect();
    assert_eq!(distinct.len(), declared.len(), "the registry repeats a code");
    for code in ConfigurationFailureCode::REGISTRY {
        assert_eq!(code.to_string(), code.code(), "a code renders as something else");
    }
}

#[test]
fn no_recorded_manifest_mutation_reaches_a_reader_as_this_contract() {
    let manifest = ProfileAuthenticationContract::embedded_manifest();
    for path in fixture_files(REJECTED_DIRECTORY) {
        let mutated = std::fs::read_to_string(&path).expect("the mutation is readable");
        assert_ne!(mutated, manifest, "{} is not a mutation", path.display());
        let Ok(parsed) = ProfileAuthenticationContract::parse(&mutated) else {
            continue;
        };
        assert_ne!(
            parsed.render().expect("a parsed contract renders"),
            manifest,
            "{} was accepted as this contract",
            path.display()
        );
        assert_ne!(parsed, *ProfileAuthenticationContract::embedded(), "{}", path.display());
    }
}

#[test]
fn a_manifest_that_is_not_well_formed_is_refused_outright() {
    for name in [
        "additional-member.json",
        "missing-member.json",
        "renamed-member.json",
        "repeated-registry-entry.json",
        "unsupported-format.json",
        "truncation-excludes-marker.json",
        "unregistered-marker-code.json",
        "insignificant-whitespace.json",
        "unsorted-keys.json",
        "missing-final-line-feed.json",
    ] {
        let mutated = read_fixture(&format!("{REJECTED_DIRECTORY}/{name}"));
        assert!(ProfileAuthenticationContract::parse(&mutated).is_err(), "{name} was accepted");
    }
}

#[test]
fn every_derived_maximum_equals_the_sum_of_its_terms() {
    let contract = ProfileAuthenticationContract::embedded();
    let fixture: Derivations = parse_fixture("derivations.toml");
    assert_eq!(fixture.format, "slingshot.contract-derivations/1");
    assert!(!fixture.derivation.is_empty(), "the fixture derives nothing");
    for derivation in fixture.derivation {
        let total = derivation
            .terms
            .iter()
            .try_fold(0_u64, |sum, term| {
                named_limit(contract, &term.limit).checked_mul(term.charges)?.checked_add(sum)
            })
            .unwrap_or_else(|| panic!("{} overflows", derivation.name));
        assert_eq!(total, named_limit(contract, &derivation.total), "{}", derivation.name);
    }
}

#[test]
fn the_diagnostic_truncation_rule_includes_its_marker() {
    let limits = &ProfileAuthenticationContract::embedded().limits;
    let fixture: TruncationVectors = parse_fixture("diagnostic-truncation-vectors.toml");
    assert_eq!(fixture.format, "slingshot.diagnostic-truncation-vectors/1");
    for vector in fixture.vector {
        let distinct = match vector.distinct.as_str() {
            "retained" => limits.retained_configuration_diagnostics,
            "maximum" => limits.maximum_configuration_diagnostics,
            "maximum_plus_one" => limits.maximum_configuration_diagnostics + 1,
            other => panic!("the fixture names the unknown size {other}"),
        };
        let truncated = distinct > limits.maximum_configuration_diagnostics;
        assert_eq!(truncated, vector.truncated, "{} truncates differently", vector.distinct);
        let returned =
            if truncated { limits.retained_configuration_diagnostics + 1 } else { distinct };
        assert!(
            returned <= limits.maximum_configuration_diagnostics,
            "the result exceeds the limit"
        );
        if let Some(occurrences) = vector.marker_occurrences {
            assert!(truncated, "an untruncated result carries no marker");
            assert_eq!(occurrences, distinct - limits.retained_configuration_diagnostics);
        }
    }
}

#[test]
fn every_depth_document_sits_where_the_index_says_relative_to_the_ceiling() {
    let ceiling =
        ProfileAuthenticationContract::embedded().limits.maximum_service_credential_json_depth;
    let index: DepthIndex = parse_fixture(&format!("{DEPTH_DIRECTORY}/index.toml"));
    assert_eq!(index.format, "slingshot.json-depth-vectors/1");
    let listed: BTreeSet<&str> = index.document.iter().map(|entry| entry.name.as_str()).collect();
    let present: BTreeSet<String> = fixture_files(DEPTH_DIRECTORY)
        .iter()
        .filter(|path| path.extension().is_some_and(|extension| extension == "json"))
        .map(|path| path.file_name().expect("the file has a name").to_string_lossy().into_owned())
        .collect();
    let present: BTreeSet<&str> = present.iter().map(String::as_str).collect();
    assert_eq!(listed, present, "the index and the directory disagree");
    for entry in index.document {
        let text = read_fixture(&format!("{DEPTH_DIRECTORY}/{}", entry.name));
        let value: serde_json::Value = serde_json::from_str(&text)
            .unwrap_or_else(|failure| panic!("{}: {failure}", entry.name));
        let expected = match (entry.offset, entry.absolute) {
            (Some(offset), None) => {
                u64::try_from(i64::try_from(ceiling).expect("the ceiling fits") + offset)
                    .expect("the depth is positive")
            }
            (None, Some(absolute)) => absolute,
            _ => panic!("{} declares no single depth", entry.name),
        };
        assert_eq!(measure_depth(&value), expected, "{}", entry.name);
    }
    assert!(
        index_has_each_side_of(ceiling),
        "the fixture must prove the ceiling from below, at, and above"
    );
}

/// Reports whether the index carries a document below, at, and above `ceiling`.
fn index_has_each_side_of(ceiling: u64) -> bool {
    let index: DepthIndex = parse_fixture(&format!("{DEPTH_DIRECTORY}/index.toml"));
    let offsets: BTreeSet<i64> = index.document.iter().filter_map(|entry| entry.offset).collect();
    let _ = ceiling;
    offsets.contains(&-1) && offsets.contains(&0) && offsets.contains(&1)
}

#[test]
fn every_closed_inventory_the_manifest_declares_is_internally_consistent() {
    let contract = ProfileAuthenticationContract::embedded();
    let literals = &contract.literals;
    let marker = &literals.diagnostic_truncation_marker;
    assert!(literals.diagnostic_source_classes.contains(&marker.source_class));
    assert!(literals.diagnostic_stages.contains(&marker.stage));
    assert!(contract.failure_codes.contains(&marker.code));
    assert_eq!(literals.deployments.len(), literals.authentication_methods.len());
    assert_eq!(
        literals.identity_management_authorities.len(),
        1,
        "the contract admits exactly one identity-management authority"
    );
    let authority = &literals.identity_management_authorities[0];
    for prefix in [&literals.assertion_audience_prefix, &literals.assertion_metascope_claim_prefix]
    {
        assert!(prefix.contains(authority.as_str()), "{prefix} names another authority");
        assert!(
            prefix.starts_with(&format!("{}://", literals.identity_management_scheme)),
            "{prefix} uses another scheme"
        );
    }
    for inventory in [
        &contract.precedence.assertion_failure,
        &contract.precedence.configuration_failure,
        &contract.precedence.exchange_failure,
    ] {
        let distinct: BTreeSet<&String> = inventory.iter().collect();
        assert_eq!(distinct.len(), inventory.len(), "a precedence order repeats a checkpoint");
        assert!(!inventory.is_empty(), "a precedence order is empty");
    }
    for inventory in [
        &literals.profile_members,
        &literals.environment_members,
        &literals.selection_members,
        &literals.configuration_snapshot_members,
        &literals.service_credential_integration_members,
    ] {
        let distinct: BTreeSet<&String> = inventory.iter().map(|member| &member.name).collect();
        assert_eq!(distinct.len(), inventory.len(), "a member inventory repeats a name");
    }
}

#[test]
fn the_reader_refuses_a_document_that_is_not_the_contract() {
    let manifest = ProfileAuthenticationContract::embedded_manifest();
    assert!(ProfileAuthenticationContract::parse("").is_err());
    assert!(ProfileAuthenticationContract::parse("{}").is_err());
    assert!(ProfileAuthenticationContract::parse(&manifest.replace('\n', "")).is_err());
    assert!(ProfileAuthenticationContract::parse(&format!("{manifest}\n")).is_err());
    let path = Path::new(MANIFEST_PATH);
    assert!(path.extension().is_some_and(|extension| extension == "json"));
}
