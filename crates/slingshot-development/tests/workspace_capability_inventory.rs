//! Assertions for the candidate external capability inventory.
//!
//! Two independently authored sources describe the same dependency contract:
//! `policy/workspace-capabilities.toml` names the exact registry package,
//! version, default-feature choice, feature set, and target triples of every
//! capability, while the consumer fixture records which structural module
//! family or named planned consumer requires each capability at which
//! dependency kind. Resolved Cargo metadata is the third source, so a manifest
//! edge that neither source justifies, and a declared row no manifest carries,
//! both fail.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::process::Command;

use serde::Deserialize;

/// Repository path of the capability policy.
const POLICY_PATH: &str = "policy/workspace-capabilities.toml";

/// Directory holding the fixtures this test compares the policy against.
const FIXTURE_DIRECTORY: &str =
    "crates/slingshot-development/tests/fixtures/workspace-capability-inventory";

/// Fixture that records the capability requirements of every consumer.
const CONSUMER_FIXTURE: &str = "consumer-capabilities.toml";

/// Format identifier the capability policy must declare.
const POLICY_FORMAT: &str = "slingshot.workspace-capabilities/1";

/// Format identifier the consumer fixture must declare.
const CONSUMER_FORMAT: &str = "slingshot.consumer-capabilities/1";

/// Reserved consumer entry meaning the kind needs no external capability.
const STANDARD_LIBRARY: &str = "standard-library";

/// Dependency kind of a library dependency.
const NORMAL_KIND: &str = "normal";

/// Dependency kind of a build-script dependency.
const BUILD_KIND: &str = "build";

/// Dependency kind of a test-only dependency.
const DEVELOPMENT_KIND: &str = "development";

/// Spelling Cargo metadata uses for a test-only dependency.
const METADATA_DEVELOPMENT_KIND: &str = "dev";

/// Target condition recorded for a capability that applies everywhere.
const EVERY_TARGET: &str = "";

/// The capability policy as it is committed.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "kebab-case", deny_unknown_fields)]
struct CapabilityPolicy {
    /// Format identifier of the policy document.
    format: String,
    /// Lowest Rust version every selected package must support.
    minimum_rust_version: String,
    /// Every target triple a capability may be conditioned on.
    supported_targets: Vec<String>,
    /// One row per non-standard capability.
    capability: Vec<CapabilityRow>,
}

/// One candidate registry package selected for a capability.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "kebab-case", deny_unknown_fields)]
struct CapabilityRow {
    /// Capability name a consumer refers to.
    name: String,
    /// Present-state description of what the capability supplies.
    purpose: String,
    /// Registry package selected for the capability.
    package: String,
    /// Exact selected version.
    version: String,
    /// Whether the package's default features stay enabled.
    default_features: bool,
    /// Features the workspace enables explicitly.
    features: Vec<String>,
    /// Target triples the capability is conditioned on; empty means all.
    targets: Vec<String>,
    /// Dependency kinds the capability is declared at.
    kinds: Vec<String>,
    /// Packages whose manifests declare the capability.
    owners: Vec<String>,
}

/// The consumer fixture as it is committed.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "kebab-case", deny_unknown_fields)]
struct ConsumerFixture {
    /// Format identifier of the fixture.
    format: String,
    /// One row per structural family or named planned consumer.
    consumer: Vec<ConsumerRow>,
}

/// The capabilities one consumer requires at each dependency kind.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "kebab-case", deny_unknown_fields)]
struct ConsumerRow {
    /// Structural family path or named planned consumer.
    name: String,
    /// Package whose manifest carries the consumer's dependencies.
    package: String,
    /// Capabilities required as library dependencies.
    normal: Vec<String>,
    /// Capabilities required by a build script.
    build: Vec<String>,
    /// Capabilities required only by tests.
    development: Vec<String>,
}

/// One dependency edge a member manifest declares.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
struct ManifestEdge {
    /// Package that declares the edge.
    package: String,
    /// Registry package the edge reaches.
    dependency: String,
    /// Dependency kind of the edge.
    kind: String,
    /// Target triple the edge is conditioned on, or the empty string.
    target: String,
}

/// The exact selection one dependency edge carries.
#[derive(Debug, Clone, PartialEq, Eq)]
struct EdgeSelection {
    /// Version requirement Cargo recorded for the edge.
    requirement: String,
    /// Whether the edge keeps the package's default features.
    default_features: bool,
    /// Features the edge enables explicitly.
    features: Vec<String>,
}

/// Returns the workspace root directory.
fn workspace_root() -> PathBuf {
    slingshot_development::locate_workspace_root(Path::new(env!("CARGO_MANIFEST_DIR")))
        .expect("the development crate lives inside the workspace")
}

/// Reads one repository file relative to the workspace root.
fn read_repository_file(relative: &str) -> String {
    let path = workspace_root().join(relative);
    std::fs::read_to_string(&path)
        .unwrap_or_else(|failure| panic!("{} could not be read: {failure}", path.display()))
}

/// Reads and parses the committed capability policy.
fn committed_policy() -> CapabilityPolicy {
    parse_policy(&read_repository_file(POLICY_PATH))
}

/// Parses a capability policy document.
fn parse_policy(text: &str) -> CapabilityPolicy {
    toml::from_str(text).expect("the capability policy is a valid document")
}

/// Reads and parses the committed consumer fixture.
fn committed_consumers() -> ConsumerFixture {
    let text = read_repository_file(&format!("{FIXTURE_DIRECTORY}/{CONSUMER_FIXTURE}"));
    toml::from_str(&text).expect("the consumer fixture is a valid document")
}

/// Reads and parses one rejection fixture.
fn rejection_fixture(name: &str) -> CapabilityPolicy {
    parse_policy(&read_repository_file(&format!("{FIXTURE_DIRECTORY}/{name}")))
}

/// Returns the resolved Cargo metadata for the workspace, including dependencies.
fn resolved_metadata() -> serde_json::Value {
    let produced = Command::new(slingshot_development::cargo_executable())
        .current_dir(workspace_root())
        .args(["metadata", "--locked", "--format-version", "1"])
        .output()
        .expect("cargo metadata starts");
    assert!(produced.status.success(), "{}", String::from_utf8_lossy(&produced.stderr));
    serde_json::from_slice(&produced.stdout).expect("cargo metadata is well-formed")
}

/// Returns the package names Cargo reports as workspace members.
fn workspace_member_names(metadata: &serde_json::Value) -> BTreeSet<String> {
    let members: BTreeSet<&str> = metadata["workspace_members"]
        .as_array()
        .expect("cargo metadata lists workspace members")
        .iter()
        .filter_map(serde_json::Value::as_str)
        .collect();
    metadata["packages"]
        .as_array()
        .expect("cargo metadata lists packages")
        .iter()
        .filter(|package| members.contains(package["id"].as_str().unwrap_or_default()))
        .filter_map(|package| package["name"].as_str().map(str::to_owned))
        .collect()
}

/// Translates the dependency kind Cargo reports into the recorded spelling.
fn recorded_kind(value: &serde_json::Value) -> String {
    match value.as_str() {
        None => NORMAL_KIND.to_owned(),
        Some(METADATA_DEVELOPMENT_KIND) => DEVELOPMENT_KIND.to_owned(),
        Some(other) => other.to_owned(),
    }
}

/// Collects every dependency edge the workspace members declare.
fn manifest_edges(metadata: &serde_json::Value) -> BTreeMap<ManifestEdge, EdgeSelection> {
    let members = workspace_member_names(metadata);
    let mut edges = BTreeMap::new();
    for package in metadata["packages"].as_array().expect("packages") {
        let Some(name) = package["name"].as_str() else { continue };
        if !members.contains(name) {
            continue;
        }
        for dependency in package["dependencies"].as_array().expect("dependencies") {
            let edge = ManifestEdge {
                package: name.to_owned(),
                dependency: dependency["name"].as_str().unwrap_or_default().to_owned(),
                kind: recorded_kind(&dependency["kind"]),
                target: dependency["target"].as_str().unwrap_or(EVERY_TARGET).to_owned(),
            };
            let selection = EdgeSelection {
                requirement: dependency["req"].as_str().unwrap_or_default().to_owned(),
                default_features: dependency["uses_default_features"].as_bool().unwrap_or(true),
                features: dependency["features"]
                    .as_array()
                    .map(|values| {
                        values
                            .iter()
                            .filter_map(serde_json::Value::as_str)
                            .map(str::to_owned)
                            .collect()
                    })
                    .unwrap_or_default(),
            };
            edges.insert(edge, selection);
        }
    }
    edges
}

/// Returns the lowest Rust version each resolved package declares.
fn resolved_rust_versions(metadata: &serde_json::Value) -> BTreeMap<String, String> {
    metadata["packages"]
        .as_array()
        .expect("packages")
        .iter()
        .filter_map(|package| {
            let name = package["name"].as_str()?;
            let version = package["rust_version"].as_str()?;
            Some((name.to_owned(), version.to_owned()))
        })
        .collect()
}

/// Reads a Rust version into comparable release numbers.
fn release_numbers(version: &str) -> Vec<u64> {
    version.split('.').map(|part| part.parse::<u64>().unwrap_or_default()).collect()
}

/// Derives the dependency edges the policy and the consumer fixture require.
fn expected_edges(
    policy: &CapabilityPolicy,
    consumers: &ConsumerFixture,
) -> BTreeMap<ManifestEdge, EdgeSelection> {
    let rows: BTreeMap<&str, &CapabilityRow> =
        policy.capability.iter().map(|row| (row.name.as_str(), row)).collect();
    let mut expected = BTreeMap::new();
    for consumer in &consumers.consumer {
        for (kind, wanted) in kind_requirements(consumer) {
            for name in wanted {
                let Some(row) = rows.get(name.as_str()) else { continue };
                let selection = EdgeSelection {
                    requirement: format!("^{}", row.version),
                    default_features: row.default_features,
                    features: row.features.clone(),
                };
                for target in target_conditions(row) {
                    let edge = ManifestEdge {
                        package: consumer.package.clone(),
                        dependency: row.package.clone(),
                        kind: kind.to_owned(),
                        target,
                    };
                    expected.insert(edge, selection.clone());
                }
            }
        }
    }
    expected
}

/// Returns the capability lists of one consumer, paired with their kinds.
fn kind_requirements(consumer: &ConsumerRow) -> Vec<(&'static str, &Vec<String>)> {
    vec![
        (NORMAL_KIND, &consumer.normal),
        (BUILD_KIND, &consumer.build),
        (DEVELOPMENT_KIND, &consumer.development),
    ]
}

/// Returns the target conditions one capability row applies to.
fn target_conditions(row: &CapabilityRow) -> Vec<String> {
    if row.targets.is_empty() { vec![EVERY_TARGET.to_owned()] } else { row.targets.clone() }
}

/// Reports every structural violation inside the capability policy.
fn evaluate_policy_shape(policy: &CapabilityPolicy, members: &BTreeSet<String>) -> Vec<String> {
    let mut violations = Vec::new();
    if policy.format != POLICY_FORMAT {
        violations.push(format!("the policy declares the format {}", policy.format));
    }
    let mut names = BTreeSet::new();
    let mut selections: BTreeMap<&str, (&str, bool, &Vec<String>)> = BTreeMap::new();
    for row in &policy.capability {
        if !names.insert(row.name.as_str()) {
            violations.push(format!("{} is declared more than once", row.name));
        }
        if members.contains(&row.package) {
            violations.push(format!("{} selects the workspace member {}", row.name, row.package));
        }
        for target in &row.targets {
            if !policy.supported_targets.contains(target) {
                violations.push(format!("{} names the unsupported target {target}", row.name));
            }
        }
        let selection = (row.version.as_str(), row.default_features, &row.features);
        match selections.get(row.package.as_str()) {
            Some(existing) if *existing != selection => {
                violations.push(format!("{} is selected twice with different policy", row.package));
            }
            _ => {
                selections.insert(row.package.as_str(), selection);
            }
        }
    }
    violations
}

/// Reports every structural violation inside the consumer fixture.
fn evaluate_consumer_shape(consumers: &ConsumerFixture, policy: &CapabilityPolicy) -> Vec<String> {
    let mut violations = Vec::new();
    if consumers.format != CONSUMER_FORMAT {
        violations.push(format!("the fixture declares the format {}", consumers.format));
    }
    let names: BTreeSet<&str> = policy.capability.iter().map(|row| row.name.as_str()).collect();
    let mut seen = BTreeSet::new();
    for consumer in &consumers.consumer {
        if !seen.insert(consumer.name.as_str()) {
            violations.push(format!("{} is declared more than once", consumer.name));
        }
        for (kind, wanted) in kind_requirements(consumer) {
            for capability in wanted {
                if capability != STANDARD_LIBRARY && !names.contains(capability.as_str()) {
                    violations.push(format!(
                        "{} requires the unknown {capability} at {kind}",
                        consumer.name
                    ));
                }
            }
        }
    }
    violations
}

/// Reports every disagreement between declared and derived owners and kinds.
fn evaluate_ownership(policy: &CapabilityPolicy, consumers: &ConsumerFixture) -> Vec<String> {
    let mut owners: BTreeMap<&str, BTreeSet<&str>> = BTreeMap::new();
    let mut kinds: BTreeMap<&str, BTreeSet<&str>> = BTreeMap::new();
    for consumer in &consumers.consumer {
        for (kind, wanted) in kind_requirements(consumer) {
            for capability in wanted {
                if capability == STANDARD_LIBRARY {
                    continue;
                }
                owners.entry(capability).or_default().insert(consumer.package.as_str());
                kinds.entry(capability).or_default().insert(kind);
            }
        }
    }
    let mut violations = Vec::new();
    for row in &policy.capability {
        let derived_owners = owners.remove(row.name.as_str()).unwrap_or_default();
        let derived_kinds = kinds.remove(row.name.as_str()).unwrap_or_default();
        if derived_owners.is_empty() {
            violations.push(format!("{} has no consumer", row.name));
            continue;
        }
        let declared_owners: BTreeSet<&str> = row.owners.iter().map(String::as_str).collect();
        let declared_kinds: BTreeSet<&str> = row.kinds.iter().map(String::as_str).collect();
        if declared_owners != derived_owners {
            violations.push(format!("{} declares owners {declared_owners:?}", row.name));
        }
        if declared_kinds != derived_kinds {
            violations.push(format!("{} declares kinds {declared_kinds:?}", row.name));
        }
    }
    violations
}

/// Reports every difference between the required and the declared edges.
fn evaluate_manifest_edges(
    expected: &BTreeMap<ManifestEdge, EdgeSelection>,
    observed: &BTreeMap<ManifestEdge, EdgeSelection>,
) -> Vec<String> {
    let mut violations = Vec::new();
    for (edge, selection) in expected {
        match observed.get(edge) {
            None => violations.push(format!("{edge:?} is required but no manifest declares it")),
            Some(found) if found != selection => {
                violations.push(format!("{edge:?} declares {found:?}, not {selection:?}"));
            }
            Some(_) => {}
        }
    }
    for edge in observed.keys() {
        if !expected.contains_key(edge) {
            violations.push(format!("{edge:?} is declared but no capability requires it"));
        }
    }
    violations
}

/// Reports every selection the workspace dependency table does not centralize.
fn evaluate_workspace_dependencies(policy: &CapabilityPolicy, root: &str) -> Vec<String> {
    let document: toml::Value =
        toml::from_str(root).expect("the root manifest is a valid document");
    let table = document
        .get("workspace")
        .and_then(|workspace| workspace.get("dependencies"))
        .and_then(toml::Value::as_table)
        .expect("the root manifest centralizes workspace dependencies");
    let mut violations = Vec::new();
    let mut required = BTreeSet::new();
    for row in &policy.capability {
        required.insert(row.package.as_str());
        let Some(entry) = table.get(&row.package) else {
            violations
                .push(format!("{} is not centralized under workspace dependencies", row.package));
            continue;
        };
        let version = entry.get("version").and_then(toml::Value::as_str).unwrap_or_default();
        let defaults = entry.get("default-features").and_then(toml::Value::as_bool).unwrap_or(true);
        let features: Vec<String> = entry
            .get("features")
            .and_then(toml::Value::as_array)
            .map(|values| {
                values.iter().filter_map(toml::Value::as_str).map(str::to_owned).collect()
            })
            .unwrap_or_default();
        if version != row.version || defaults != row.default_features || features != row.features {
            violations.push(format!("{} is centralized with a different selection", row.package));
        }
    }
    for package in table.keys() {
        if !required.contains(package.as_str()) {
            violations.push(format!("{package} is centralized but no capability selects it"));
        }
    }
    violations
}

/// Reports every resolved package that needs a newer compiler than the policy.
fn evaluate_rust_versions(
    policy: &CapabilityPolicy,
    resolved: &BTreeMap<String, String>,
) -> Vec<String> {
    let ceiling = release_numbers(&policy.minimum_rust_version);
    resolved
        .iter()
        .filter(|(_, version)| release_numbers(version) > ceiling)
        .map(|(package, version)| format!("{package} requires Rust {version}"))
        .collect()
}

#[test]
fn the_committed_inventory_satisfies_every_structural_rule() {
    let policy = committed_policy();
    let consumers = committed_consumers();
    let metadata = resolved_metadata();
    let members = workspace_member_names(&metadata);
    assert_eq!(evaluate_policy_shape(&policy, &members), Vec::<String>::new());
    assert_eq!(evaluate_consumer_shape(&consumers, &policy), Vec::<String>::new());
    assert_eq!(evaluate_ownership(&policy, &consumers), Vec::<String>::new());
}

#[test]
fn every_manifest_edge_matches_one_capability_and_one_consumer() {
    let policy = committed_policy();
    let consumers = committed_consumers();
    let metadata = resolved_metadata();
    let observed = manifest_edges(&metadata);
    let expected = expected_edges(&policy, &consumers);
    assert_eq!(evaluate_manifest_edges(&expected, &observed), Vec::<String>::new());
}

#[test]
fn every_selection_is_centralized_under_workspace_dependencies() {
    let policy = committed_policy();
    let root = read_repository_file("Cargo.toml");
    assert_eq!(evaluate_workspace_dependencies(&policy, &root), Vec::<String>::new());
}

#[test]
fn the_target_conditioned_rows_name_exactly_the_supported_triples() {
    let policy = committed_policy();
    let declared: BTreeSet<&str> =
        policy.capability.iter().flat_map(|row| row.targets.iter().map(String::as_str)).collect();
    let supported: BTreeSet<&str> = policy.supported_targets.iter().map(String::as_str).collect();
    assert_eq!(declared, supported);
}

#[test]
fn no_resolved_package_requires_a_newer_compiler_than_the_pinned_one() {
    let policy = committed_policy();
    let metadata = resolved_metadata();
    let resolved = resolved_rust_versions(&metadata);
    for member in workspace_member_names(&metadata) {
        let declared = resolved.get(&member).map(String::as_str);
        assert_eq!(declared, Some(policy.minimum_rust_version.as_str()), "{member}");
    }
    assert_eq!(evaluate_rust_versions(&policy, &resolved), Vec::<String>::new());
}

#[test]
fn the_recorded_rejection_fixtures_are_refused() {
    let consumers = committed_consumers();
    let metadata = resolved_metadata();
    let members = workspace_member_names(&metadata);
    let observed = manifest_edges(&metadata);
    let root = read_repository_file("Cargo.toml");
    for name in ["rejected-duplicated-policy.toml", "rejected-workspace-member-selection.toml"] {
        let policy = rejection_fixture(name);
        assert!(!evaluate_policy_shape(&policy, &members).is_empty(), "{name}");
    }
    for name in ["rejected-missing-dependency.toml", "rejected-additional-dependency.toml"] {
        let policy = rejection_fixture(name);
        let expected = expected_edges(&policy, &consumers);
        assert!(!evaluate_manifest_edges(&expected, &observed).is_empty(), "{name}");
    }
    for name in ["rejected-misplaced-kind.toml", "rejected-feature-drift.toml"] {
        let policy = rejection_fixture(name);
        let expected = expected_edges(&policy, &consumers);
        let mut refused = evaluate_manifest_edges(&expected, &observed);
        refused.extend(evaluate_ownership(&policy, &consumers));
        refused.extend(evaluate_workspace_dependencies(&policy, &root));
        assert!(!refused.is_empty(), "{name}");
    }
    let policy = rejection_fixture("rejected-unsupported-rust-version.toml");
    let resolved = BTreeMap::from([("slingshot-domain".to_owned(), "1.98.0".to_owned())]);
    assert!(!evaluate_rust_versions(&policy, &resolved).is_empty());
}
