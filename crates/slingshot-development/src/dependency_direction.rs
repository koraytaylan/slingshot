//! Dependency direction check.
//!
//! The crate diagram is an executable boundary. This module reads resolved
//! Cargo metadata, keeps only the edges between local packages, and compares
//! each one with the workspace dependency contract: a product crate depends
//! inward on the contracts it is allowed to name, reaches a support crate only
//! through a development edge into test support, and never reaches the
//! outermost tooling crate at all. Registry dependencies are outside this table
//! and are covered by dependency policy instead.

use std::collections::{BTreeMap, BTreeSet};

use serde::Deserialize;

/// Dependency kind of a library dependency.
pub const NORMAL_KIND: &str = "normal";

/// Dependency kind of a build-script dependency.
pub const BUILD_KIND: &str = "build";

/// Dependency kind of a test-only dependency.
pub const DEVELOPMENT_KIND: &str = "development";

/// Spelling Cargo metadata uses for a test-only dependency.
const METADATA_DEVELOPMENT_KIND: &str = "dev";

/// Reusable fakes, harnesses, and path-only values for tests.
pub const TEST_SUPPORT_PACKAGE: &str = "slingshot-test-support";

/// Outermost tooling crate, which no local package may depend on.
pub const DEVELOPMENT_PACKAGE: &str = "slingshot-development";

/// Product crates, in dependency order from the innermost contract outward.
pub const PRODUCT_PACKAGES: &[&str] = &[
    "slingshot-domain",
    "slingshot-configuration",
    "slingshot-agent-protocol",
    "slingshot-local-protocol",
    "slingshot-agent-connection",
    "slingshot-storage",
    "slingshot-daemon",
    "slingshot-command-line",
];

/// Local packages each product crate may depend on, at any dependency kind.
const PERMITTED_PRODUCT_DEPENDENCIES: &[(&str, &[&str])] = &[
    ("slingshot-domain", &[]),
    ("slingshot-configuration", &["slingshot-domain"]),
    ("slingshot-agent-protocol", &["slingshot-domain"]),
    ("slingshot-local-protocol", &["slingshot-domain"]),
    (
        "slingshot-agent-connection",
        &["slingshot-configuration", "slingshot-agent-protocol", "slingshot-domain"],
    ),
    ("slingshot-storage", &["slingshot-domain"]),
    (
        "slingshot-daemon",
        &[
            "slingshot-domain",
            "slingshot-configuration",
            "slingshot-agent-protocol",
            "slingshot-local-protocol",
            "slingshot-agent-connection",
            "slingshot-storage",
        ],
    ),
    (
        // The command line turns arguments into domain commands, so it names the
        // domain like every other product crate. Without that edge it could
        // offer the catalog's operations only by keeping a second copy of the
        // catalog, and two lists of the same commands eventually disagree about
        // which of them changes something.
        "slingshot-command-line",
        &[
            "slingshot-domain",
            "slingshot-local-protocol",
            "slingshot-configuration",
            "slingshot-daemon",
        ],
    ),
];

/// Local packages the test-support crate may depend on, at any kind.
const PERMITTED_TEST_SUPPORT_DEPENDENCIES: &[&str] = &[
    "slingshot-domain",
    "slingshot-agent-protocol",
    "slingshot-local-protocol",
    "slingshot-storage",
];

/// Reason the dependency-direction check could not read its input.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum DirectionFailure {
    /// The metadata document could not be read.
    #[error("the metadata document could not be read: {0}")]
    Unreadable(String),
    /// The metadata document has no package list.
    #[error("the metadata document lists no packages")]
    NoPackages,
}

/// One dependency edge between two local packages.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct LocalEdge {
    /// Package that declares the edge.
    pub dependent: String,
    /// Local package the edge reaches.
    pub dependency: String,
    /// Dependency kind of the edge.
    pub kind: String,
}

/// The local-package edges of one resolved workspace.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct LocalPackageGraph {
    /// Every local package name, in sorted order.
    pub packages: BTreeSet<String>,
    /// Every edge between two local packages, in sorted order.
    pub edges: BTreeSet<LocalEdge>,
}

/// The parts of a Cargo metadata document this check reads.
#[derive(Debug, Deserialize)]
struct MetadataDocument {
    /// Every package Cargo resolved.
    packages: Vec<MetadataPackage>,
}

/// One package entry of a Cargo metadata document.
#[derive(Debug, Deserialize)]
struct MetadataPackage {
    /// Package name.
    name: String,
    /// Registry the package came from, or none for a local package.
    #[serde(default)]
    source: Option<String>,
    /// Direct dependencies the package declares.
    #[serde(default)]
    dependencies: Vec<MetadataDependency>,
}

/// One dependency entry of a Cargo metadata package.
#[derive(Debug, Deserialize)]
struct MetadataDependency {
    /// Dependency package name.
    name: String,
    /// Dependency kind, or none for a library dependency.
    #[serde(default)]
    kind: Option<String>,
}

/// Translates the dependency kind Cargo reports into the recorded spelling.
fn recorded_kind(kind: Option<&str>) -> String {
    match kind {
        None => NORMAL_KIND.to_owned(),
        Some(METADATA_DEVELOPMENT_KIND) => DEVELOPMENT_KIND.to_owned(),
        Some(other) => other.to_owned(),
    }
}

/// Reads the local-package graph out of a Cargo metadata document.
///
/// A package is local when Cargo records no registry source for it. Every
/// dependency that names another local package becomes one edge; registry
/// dependencies are dropped.
///
/// # Errors
///
/// Returns [`DirectionFailure::Unreadable`] when the document is not valid
/// metadata and [`DirectionFailure::NoPackages`] when it lists no package.
pub fn read_graph(metadata: &str) -> Result<LocalPackageGraph, DirectionFailure> {
    let document: MetadataDocument = serde_json::from_str(metadata)
        .map_err(|failure| DirectionFailure::Unreadable(failure.to_string()))?;
    if document.packages.is_empty() {
        return Err(DirectionFailure::NoPackages);
    }
    let packages: BTreeSet<String> = document
        .packages
        .iter()
        .filter(|package| package.source.is_none())
        .map(|package| package.name.clone())
        .collect();
    let mut edges = BTreeSet::new();
    for package in &document.packages {
        if !packages.contains(&package.name) {
            continue;
        }
        for dependency in &package.dependencies {
            if !packages.contains(&dependency.name) {
                continue;
            }
            edges.insert(LocalEdge {
                dependent: package.name.clone(),
                dependency: dependency.name.clone(),
                kind: recorded_kind(dependency.kind.as_deref()),
            });
        }
    }
    Ok(LocalPackageGraph { packages, edges })
}

/// Returns the local packages one product crate may depend on.
fn permitted_product_dependencies(package: &str) -> Option<&'static [&'static str]> {
    PERMITTED_PRODUCT_DEPENDENCIES
        .iter()
        .find(|(name, _)| *name == package)
        .map(|(_, permitted)| *permitted)
}

/// Renders the permitted direction of one dependent package.
fn permitted_direction(dependent: &str) -> String {
    if dependent == TEST_SUPPORT_PACKAGE {
        return format!(
            "{TEST_SUPPORT_PACKAGE} may depend only on {PERMITTED_TEST_SUPPORT_DEPENDENCIES:?}"
        );
    }
    if dependent == DEVELOPMENT_PACKAGE {
        return format!(
            "{DEVELOPMENT_PACKAGE} may depend inward on any product crate and on {TEST_SUPPORT_PACKAGE}"
        );
    }
    match permitted_product_dependencies(dependent) {
        Some(permitted) => format!(
            "{dependent} may depend on {permitted:?}, and on {TEST_SUPPORT_PACKAGE} only as a {DEVELOPMENT_KIND} dependency"
        ),
        None => format!("{dependent} is not a package of this workspace"),
    }
}

/// Reports whether one edge from a product crate is permitted.
fn product_edge_is_permitted(edge: &LocalEdge, permitted: &[&str]) -> bool {
    if edge.dependency == DEVELOPMENT_PACKAGE {
        return false;
    }
    if edge.dependency == TEST_SUPPORT_PACKAGE {
        return edge.kind == DEVELOPMENT_KIND;
    }
    permitted.contains(&edge.dependency.as_str())
}

/// Reports whether one edge is permitted by the dependency contract.
fn edge_is_permitted(edge: &LocalEdge) -> bool {
    if edge.dependency == DEVELOPMENT_PACKAGE || edge.dependent == edge.dependency {
        return false;
    }
    if edge.dependent == DEVELOPMENT_PACKAGE {
        return PRODUCT_PACKAGES.contains(&edge.dependency.as_str())
            || edge.dependency == TEST_SUPPORT_PACKAGE;
    }
    if edge.dependent == TEST_SUPPORT_PACKAGE {
        return PERMITTED_TEST_SUPPORT_DEPENDENCIES.contains(&edge.dependency.as_str());
    }
    match permitted_product_dependencies(&edge.dependent) {
        Some(permitted) => product_edge_is_permitted(edge, permitted),
        None => false,
    }
}

/// Returns the shortest cycle among the compiled local edges, if one exists.
fn find_cycle(graph: &LocalPackageGraph) -> Option<Vec<String>> {
    let mut adjacency: BTreeMap<&str, BTreeSet<&str>> = BTreeMap::new();
    for edge in &graph.edges {
        if edge.kind == NORMAL_KIND || edge.kind == BUILD_KIND {
            adjacency.entry(&edge.dependent).or_default().insert(&edge.dependency);
        }
    }
    for start in graph.packages.iter() {
        if let Some(cycle) = walk_for_cycle(&adjacency, start) {
            return Some(cycle);
        }
    }
    None
}

/// Walks one package's compiled edges looking for a path back to it.
fn walk_for_cycle(adjacency: &BTreeMap<&str, BTreeSet<&str>>, start: &str) -> Option<Vec<String>> {
    let mut pending = vec![vec![start.to_owned()]];
    while let Some(path) = pending.pop() {
        let last = path.last().cloned().unwrap_or_default();
        for next in adjacency.get(last.as_str()).into_iter().flatten() {
            if *next == start {
                let mut cycle = path.clone();
                cycle.push(start.to_owned());
                return Some(cycle);
            }
            if !path.iter().any(|visited| visited == next) {
                let mut extended = path.clone();
                extended.push((*next).to_owned());
                pending.push(extended);
            }
        }
    }
    None
}

/// Reports every forbidden edge and every cycle, in deterministic order.
///
/// Each diagnostic names the dependent crate, the dependency crate, the
/// dependency kind, and the direction the contract permits instead.
#[must_use]
pub fn evaluate(graph: &LocalPackageGraph) -> Vec<String> {
    let mut violations: Vec<String> = graph
        .edges
        .iter()
        .filter(|edge| !edge_is_permitted(edge))
        .map(|edge| {
            format!(
                "{} depends on {} as a {} dependency; {}",
                edge.dependent,
                edge.dependency,
                edge.kind,
                permitted_direction(&edge.dependent)
            )
        })
        .collect();
    if let Some(cycle) = find_cycle(graph) {
        violations.push(format!("the local packages form the cycle {}", cycle.join(" -> ")));
    }
    violations
}
