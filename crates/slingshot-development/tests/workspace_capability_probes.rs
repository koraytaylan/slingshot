//! Assertions for the capability probes that freeze the dependency selection.
//!
//! Every inventory capability has exactly one probe, in a crate the inventory
//! names as an owner, at a dependency kind the inventory declares, using the
//! public interface the coverage fixture records, and gated to exactly the
//! target rows the inventory conditions the capability on. A probe with no
//! inventory row and an inventory row with no probe both fail.
//!
//! The probes themselves run wherever they apply: a target-independent probe
//! runs everywhere, and a target-conditioned probe compiles and runs only on
//! the row that matches the current environment. A report may therefore carry
//! at most one current-environment observation, and never an aggregate claim.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use serde::Deserialize;
use slingshot_development::supported_platform_matrix::{
    SUPPORTED_TARGET_TRIPLES, UNTRUSTED_OBSERVATION_LABEL, current_target_triple,
};

/// Repository path of the capability policy.
const POLICY_PATH: &str = "policy/workspace-capabilities.toml";

/// Directory holding the fixtures this test evaluates.
const FIXTURE_DIRECTORY: &str =
    "crates/slingshot-development/tests/fixtures/workspace-capability-probes";

/// Fixture that maps every capability to its probe.
const COVERAGE_FIXTURE: &str = "probe-coverage.toml";

/// Fixture that records accepted and refused report shapes.
const REPORT_FIXTURE: &str = "native-observation-reports.toml";

/// Format identifier the coverage fixture must declare.
const COVERAGE_FORMAT: &str = "slingshot.capability-probes/1";

/// Format identifier the report fixture must declare.
const REPORT_FORMAT: &str = "slingshot.capability-probe-reports/1";

/// Directory each crate keeps its probes in, relative to the crate root.
const PROBE_DIRECTORY: &str = "tests/workspace-capabilities";

/// Entry point that declares every probe module of one crate.
const PROBE_ENTRY_POINT: &str = "main.rs";

/// Module that supplies shared probe material rather than covering a row.
const SHARED_MATERIAL_MODULE: &str = "material";

/// Observation kind that names the row matching the current environment.
const CURRENT_OBSERVATION: &str = "current";

/// The probe coverage fixture.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "kebab-case", deny_unknown_fields)]
struct ProbeCoverage {
    /// Format identifier of the fixture.
    format: String,
    /// One row per covered capability.
    probe: Vec<ProbeRow>,
}

/// One capability and the probe that exercises it.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "kebab-case", deny_unknown_fields)]
struct ProbeRow {
    /// Capability the probe covers.
    capability: String,
    /// Crate that owns the probe.
    package: String,
    /// Probe module inside that crate's probe directory.
    module: String,
    /// Dependency kind the probe exercises the capability at.
    kind: String,
    /// Public interface the probe must literally use.
    interface: Vec<String>,
    /// Target rows the probe applies to; empty means every row.
    targets: Vec<String>,
}

/// The report-shape fixture.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "kebab-case", deny_unknown_fields)]
struct ReportShapes {
    /// Format identifier of the fixture.
    format: String,
    /// One entry per accepted or refused report shape.
    report: Vec<ReportShape>,
}

/// One accepted or refused report shape.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "kebab-case", deny_unknown_fields)]
struct ReportShape {
    /// Name the fixture gives the shape.
    name: String,
    /// Observation kinds the report carries.
    observations: Vec<String>,
    /// Whether the shape must be accepted.
    accepted: bool,
}

/// The parts of the capability policy this test compares against.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "kebab-case")]
struct CapabilityPolicy {
    /// One row per capability.
    capability: Vec<PolicyRow>,
}

/// The capability facts a probe row must agree with.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "kebab-case")]
struct PolicyRow {
    /// Capability name.
    name: String,
    /// Packages whose manifests declare the capability.
    owners: Vec<String>,
    /// Dependency kinds the capability is declared at.
    kinds: Vec<String>,
    /// Target rows the capability is conditioned on.
    targets: Vec<String>,
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

/// Reads and parses the probe coverage fixture.
fn coverage() -> ProbeCoverage {
    let text = read_repository_file(&format!("{FIXTURE_DIRECTORY}/{COVERAGE_FIXTURE}"));
    toml::from_str(&text).expect("the coverage fixture is a valid document")
}

/// Reads and parses the report-shape fixture.
fn report_shapes() -> ReportShapes {
    let text = read_repository_file(&format!("{FIXTURE_DIRECTORY}/{REPORT_FIXTURE}"));
    toml::from_str(&text).expect("the report fixture is a valid document")
}

/// Reads and parses the capability policy.
fn policy() -> CapabilityPolicy {
    toml::from_str(&read_repository_file(POLICY_PATH)).expect("the capability policy parses")
}

/// Returns the repository-relative path of one probe module.
fn probe_path(row: &ProbeRow) -> String {
    format!("crates/{}/{PROBE_DIRECTORY}/{}.rs", row.package, row.module)
}

/// Returns the target predicate a probe module declaration must carry.
fn target_predicate(targets: &[String]) -> Option<String> {
    let systems: BTreeSet<&str> = targets
        .iter()
        .map(|triple| match triple.as_str() {
            value if value == SUPPORTED_TARGET_TRIPLES[0] => "linux",
            value if value == SUPPORTED_TARGET_TRIPLES[1] => "macos",
            _ => "windows",
        })
        .collect();
    match systems.len() {
        0 => None,
        1 => {
            let single = systems.iter().next().copied().unwrap_or_default();
            Some(format!("#[cfg(target_os = \"{single}\")]"))
        }
        _ if systems.contains("windows") => None,
        _ => Some("#[cfg(unix)]".to_owned()),
    }
}

/// Returns the module declarations of one crate's probe entry point.
fn declared_modules(package: &str) -> BTreeMap<String, Option<String>> {
    let text =
        read_repository_file(&format!("crates/{package}/{PROBE_DIRECTORY}/{PROBE_ENTRY_POINT}"));
    let mut declared = BTreeMap::new();
    let mut pending: Option<String> = None;
    for line in text.lines().map(str::trim) {
        if line.starts_with("#[cfg(") {
            pending = Some(line.to_owned());
            continue;
        }
        let Some(rest) = line.strip_prefix("mod ").or_else(|| line.strip_prefix("pub mod ")) else {
            continue;
        };
        if let Some(name) = rest.strip_suffix(';') {
            declared.insert(name.to_owned(), pending.take());
        }
    }
    declared
}

/// Reports whether a report shape is accepted for the current environment.
fn evaluate_report(shape: &ReportShape) -> bool {
    let current = current_target_triple();
    shape.observations.len() <= 1
        && shape.observations.iter().all(|kind| kind == CURRENT_OBSERVATION)
        && (shape.observations.is_empty() || current.is_some())
}

#[test]
fn every_capability_has_exactly_one_probe_in_an_owning_crate() {
    let coverage = coverage();
    assert_eq!(coverage.format, COVERAGE_FORMAT);
    let policy = policy();
    let capabilities: BTreeMap<&str, &PolicyRow> =
        policy.capability.iter().map(|row| (row.name.as_str(), row)).collect();
    let covered: BTreeSet<&str> =
        coverage.probe.iter().map(|row| row.capability.as_str()).collect();
    assert_eq!(covered.len(), coverage.probe.len(), "a capability is probed twice");
    let declared: BTreeSet<&str> = capabilities.keys().copied().collect();
    assert_eq!(covered, declared, "every inventory row has exactly one probe");

    for row in &coverage.probe {
        let capability = capabilities[row.capability.as_str()];
        assert!(
            capability.owners.contains(&row.package),
            "{} is probed by a crate that does not own it",
            row.capability
        );
        assert!(
            capability.kinds.contains(&row.kind),
            "{} is probed at an undeclared kind",
            row.capability
        );
        assert_eq!(
            row.targets, capability.targets,
            "{} is probed on the wrong rows",
            row.capability
        );
    }
}

#[test]
fn every_probe_module_exists_is_declared_and_uses_its_required_interface() {
    let coverage = coverage();
    let mut declarations: BTreeMap<String, BTreeMap<String, Option<String>>> = BTreeMap::new();
    for row in &coverage.probe {
        let source = read_repository_file(&probe_path(row));
        for required in &row.interface {
            assert!(
                source.contains(required.as_str()),
                "{} does not use {required}",
                probe_path(row)
            );
        }
        let declared = declarations
            .entry(row.package.clone())
            .or_insert_with(|| declared_modules(&row.package));
        let predicate = declared
            .get(&row.module)
            .unwrap_or_else(|| panic!("{} declares no module {}", row.package, row.module));
        assert_eq!(
            predicate.as_deref(),
            target_predicate(&row.targets).as_deref(),
            "{} carries the wrong target predicate",
            probe_path(row)
        );
    }
}

#[test]
fn no_probe_module_exists_without_a_coverage_row() {
    let coverage = coverage();
    let expected: BTreeSet<String> =
        coverage.probe.iter().map(|row| format!("{}::{}", row.package, row.module)).collect();
    let packages: BTreeSet<&str> = coverage.probe.iter().map(|row| row.package.as_str()).collect();
    let mut found = BTreeSet::new();
    for package in packages {
        let directory = workspace_root().join(format!("crates/{package}/{PROBE_DIRECTORY}"));
        for entry in std::fs::read_dir(&directory).expect("the probe directory is readable") {
            let path = entry.expect("the directory entry is readable").path();
            if path.is_dir() || path.extension().is_none_or(|extension| extension != "rs") {
                continue;
            }
            let stem = path.file_stem().and_then(|value| value.to_str()).unwrap_or_default();
            if stem == "main" || stem == SHARED_MATERIAL_MODULE {
                continue;
            }
            found.insert(format!("{package}::{stem}"));
        }
        for (module, _) in declared_modules(package) {
            if module != SHARED_MATERIAL_MODULE {
                assert!(
                    expected.contains(&format!("{package}::{module}")),
                    "{package} declares {module} without a coverage row"
                );
            }
        }
    }
    assert_eq!(found, expected);
}

#[test]
fn a_report_carries_at_most_one_current_environment_observation() {
    let shapes = report_shapes();
    assert_eq!(shapes.format, REPORT_FORMAT);
    for shape in &shapes.report {
        assert_eq!(evaluate_report(shape), shape.accepted, "{}", shape.name);
    }
    let current = current_target_triple();
    assert!(
        current.is_none_or(|triple| SUPPORTED_TARGET_TRIPLES.contains(&triple)),
        "the current environment matches at most one supported row"
    );
    assert_eq!(UNTRUSTED_OBSERVATION_LABEL, "untrusted_current_native_observation");
}

#[test]
fn every_target_conditioned_probe_names_a_supported_row() {
    let supported: BTreeSet<&str> = SUPPORTED_TARGET_TRIPLES.iter().copied().collect();
    let mut conditioned = BTreeSet::new();
    for row in coverage().probe {
        for triple in &row.targets {
            assert!(supported.contains(triple.as_str()), "{} names {triple}", row.capability);
            conditioned.insert(triple.clone());
        }
    }
    let named: BTreeSet<&str> = conditioned.iter().map(String::as_str).collect();
    assert_eq!(named, supported, "every supported row carries at least one conditioned probe");
}
