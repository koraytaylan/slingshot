//! Structural assertions for the workspace module-ownership map.
//!
//! Three independent sources describe the same set of source files: the
//! checked ownership fixture, the mutation footprint recorded in this task's
//! plan document, and the compiled source tree. Every assertion compares them
//! bidirectionally, so a wildcard claim, a later plan's feature leaf, a missing
//! declaration, and a misowned reusable value all fail.
//!
//! The assertions describe the module tree as it stays: a crate root declares
//! exactly the children the map assigns it, a family root carries only
//! documentation and its children, and every module is documented and free of a
//! placeholder body. A leaf gains behavior when its owning task lands, so no
//! assertion here requires a leaf to stay empty.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

/// Directory holding the fixtures this test compares the source tree against.
const FIXTURE_DIRECTORY: &str = "crates/slingshot-development/tests/fixtures/workspace-module-map";

/// Fixture that maps every owned source path to its crate, layer, and kind.
const OWNERSHIP_FIXTURE: &str = "module-ownership.txt";

/// Fixture that maps every workspace crate to its layer and graph.
const LAYER_FIXTURE: &str = "crate-layers.txt";

/// Fixture that maps named architectural vocabulary to its owning module.
const VOCABULARY_FIXTURE: &str = "vocabulary-ownership.txt";

/// Plan document whose frontmatter records this task's mutation footprint.
const TASK_DOCUMENT: &str =
    "docs/plans/0001-foundations-and-walking-skeleton/tasks/0104-workspace-module-map.md";

/// Directory every workspace member lives in.
const CRATE_DIRECTORY: &str = "crates";

/// Source file name of a crate root.
const CRATE_ROOT_FILE_NAME: &str = "lib.rs";

/// Source file name of a module-family root.
const FAMILY_ROOT_FILE_NAME: &str = "mod.rs";

/// Source file name of a process entry point, which stays with its owner.
const PROCESS_ENTRY_FILE_NAME: &str = "main.rs";

/// Kind recorded for a crate root.
const CRATE_ROOT_KIND: &str = "crate-root";

/// Kind recorded for a module-family root.
const FAMILY_ROOT_KIND: &str = "family-root";

/// Kind recorded for a single-file module.
const LEAF_KIND: &str = "leaf";

/// Graph recorded for the crates that exist for tests and repository policy.
const SUPPORT_GRAPH: &str = "support";

/// Number of columns in an ownership fixture row.
const OWNERSHIP_COLUMN_COUNT: usize = 5;

/// Number of columns in a crate-layer fixture row.
const LAYER_COLUMN_COUNT: usize = 3;

/// Number of columns in a vocabulary fixture row.
const VOCABULARY_COLUMN_COUNT: usize = 2;

/// Placeholder bodies no declared module may contain.
///
/// The scan is deliberately narrow. Marker tokens, planning headings, and the
/// word for an unchecked block need syntax classification rather than substring
/// matching, so the source-policy checker owns them and this assertion does not
/// pretend to.
const PLACEHOLDER_BODIES: &[&str] = &["todo!(", "unimplemented!("];

/// Vocabulary the architecture places in an exact crate.
const VOCABULARY_PLACEMENT: &[(&str, &str)] = &[
    ("AgentJobIdentifier", "slingshot-domain"),
    ("AgentJobState", "slingshot-domain"),
    ("JobEventSequence", "slingshot-domain"),
    ("EventStreamCursor", "slingshot-domain"),
    ("RemoteJobWireConversion", "slingshot-agent-protocol"),
    ("ProcessCheckpointObserver", "slingshot-daemon"),
    ("FiniteStateMachineExecutable", "slingshot-test-support"),
    ("ProcessHarness", "slingshot-test-support"),
    ("SupervisedChild", "slingshot-test-support"),
];

/// One declared source file and the ownership the fixture assigns it.
#[derive(Debug, Clone, PartialEq, Eq)]
struct OwnershipRow {
    /// Repository-relative source path.
    path: String,
    /// Package that owns the source file.
    package: String,
    /// Architectural layer of the owning package.
    layer: String,
    /// Whether the file is a crate root, a family root, or a leaf.
    kind: String,
    /// Rust module path the file declares.
    module: String,
}

/// Layer and graph of one workspace crate.
#[derive(Debug, Clone, PartialEq, Eq)]
struct CrateLayer {
    /// Architectural layer of the crate.
    layer: String,
    /// Graph the crate belongs to: the product graph or the support graph.
    graph: String,
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

/// Reads one fixture owned by this test.
fn read_fixture(name: &str) -> String {
    read_repository_file(&format!("{FIXTURE_DIRECTORY}/{name}"))
}

/// Returns every fixture line that carries data rather than commentary.
fn data_lines(text: &str) -> Vec<&str> {
    text.lines()
        .map(str::trim_end)
        .filter(|line| !line.is_empty() && !line.starts_with('#'))
        .collect()
}

/// Derives the kind and module path a source path must declare.
fn derive_shape(path: &str) -> Result<(String, String, String), String> {
    let segments: Vec<&str> = path.split('/').collect();
    let (Some(&CRATE_DIRECTORY), Some(package), Some(&"src")) =
        (segments.first(), segments.get(1), segments.get(2))
    else {
        return Err(format!("{path} is not a crate source path"));
    };
    let root = package.replace('-', "_");
    let tail = &segments[3..];
    let (Some(file_name), parents) = (tail.last(), &tail[..tail.len().saturating_sub(1)]) else {
        return Err(format!("{path} names no source file"));
    };
    let mut module = vec![root];
    module.extend(parents.iter().map(|&segment| segment.to_owned()));
    match *file_name {
        CRATE_ROOT_FILE_NAME if parents.is_empty() => {
            Ok(((*package).to_owned(), CRATE_ROOT_KIND.to_owned(), module.join("::")))
        }
        FAMILY_ROOT_FILE_NAME if !parents.is_empty() => {
            Ok(((*package).to_owned(), FAMILY_ROOT_KIND.to_owned(), module.join("::")))
        }
        other if other.ends_with(".rs") => {
            module.push(other.trim_end_matches(".rs").to_owned());
            Ok(((*package).to_owned(), LEAF_KIND.to_owned(), module.join("::")))
        }
        other => Err(format!("{other} is not a Rust source file")),
    }
}

/// Parses the ownership fixture and reports every structural violation.
fn parse_ownership(text: &str) -> (Vec<OwnershipRow>, Vec<String>) {
    let mut rows = Vec::new();
    let mut violations = Vec::new();
    let mut seen = BTreeSet::new();
    for line in data_lines(text) {
        let columns: Vec<&str> = line.split('|').collect();
        if columns.len() != OWNERSHIP_COLUMN_COUNT {
            violations.push(format!("{line:?} does not have {OWNERSHIP_COLUMN_COUNT} columns"));
            continue;
        }
        let row = OwnershipRow {
            path: columns[0].to_owned(),
            package: columns[1].to_owned(),
            layer: columns[2].to_owned(),
            kind: columns[3].to_owned(),
            module: columns[4].to_owned(),
        };
        if row.path.contains('*') || row.path.contains('?') {
            violations.push(format!("{} claims a wildcard footprint", row.path));
            continue;
        }
        if !seen.insert(row.path.clone()) {
            violations.push(format!("{} is declared more than once", row.path));
            continue;
        }
        match derive_shape(&row.path) {
            Err(reason) => violations.push(reason),
            Ok((package, kind, module)) => {
                if package != row.package {
                    violations
                        .push(format!("{} is owned by {package}, not {}", row.path, row.package));
                }
                if kind != row.kind {
                    violations.push(format!("{} is a {kind}, not a {}", row.path, row.kind));
                }
                if module != row.module {
                    violations.push(format!("{} declares {module}, not {}", row.path, row.module));
                }
            }
        }
        rows.push(row);
    }
    (rows, violations)
}

/// Parses the crate-layer fixture.
fn parse_layers(text: &str) -> BTreeMap<String, CrateLayer> {
    let mut layers = BTreeMap::new();
    for line in data_lines(text) {
        let columns: Vec<&str> = line.split('|').collect();
        assert_eq!(columns.len(), LAYER_COLUMN_COUNT, "{line:?}");
        let layer = CrateLayer { layer: columns[1].to_owned(), graph: columns[2].to_owned() };
        assert!(
            layers.insert(columns[0].to_owned(), layer).is_none(),
            "{line:?} repeats a package"
        );
    }
    layers
}

/// Parses the vocabulary fixture.
fn parse_vocabulary(text: &str) -> BTreeMap<String, String> {
    let mut placement = BTreeMap::new();
    for line in data_lines(text) {
        let columns: Vec<&str> = line.split('|').collect();
        assert_eq!(columns.len(), VOCABULARY_COLUMN_COUNT, "{line:?}");
        assert!(
            placement.insert(columns[0].to_owned(), columns[1].to_owned()).is_none(),
            "{line:?} repeats an item"
        );
    }
    placement
}

/// Reports every row whose layer disagrees with its crate's declared layer.
fn evaluate_layers(rows: &[OwnershipRow], layers: &BTreeMap<String, CrateLayer>) -> Vec<String> {
    let mut violations = Vec::new();
    let product_layers: BTreeSet<&str> = layers
        .values()
        .filter(|entry| entry.graph != SUPPORT_GRAPH)
        .map(|entry| entry.layer.as_str())
        .collect();
    for row in rows {
        let Some(declared) = layers.get(&row.package) else {
            violations.push(format!("{} belongs to the unknown package {}", row.path, row.package));
            continue;
        };
        if declared.layer != row.layer {
            violations
                .push(format!("{} claims layer {}, not {}", row.path, row.layer, declared.layer));
        }
        if declared.graph == SUPPORT_GRAPH && product_layers.contains(row.layer.as_str()) {
            violations.push(format!(
                "{} gives a support crate the product layer {}",
                row.path, row.layer
            ));
        }
    }
    violations
}

/// Returns the repository-relative source paths this task's footprint claims.
fn footprint_paths() -> BTreeSet<String> {
    let document = read_repository_file(TASK_DOCUMENT);
    let frontmatter = document.split("---").nth(1).expect("the task document has frontmatter");
    let mut claimed = BTreeSet::new();
    let mut inside = false;
    for line in frontmatter.lines() {
        if line.starts_with("touches:") {
            inside = true;
            continue;
        }
        if !inside {
            continue;
        }
        let Some(entry) = line.strip_prefix("  - ") else {
            break;
        };
        let entry = entry.trim().trim_matches('"');
        if entry.starts_with(CRATE_DIRECTORY) && entry.contains("/src/") && entry.ends_with(".rs") {
            claimed.insert(entry.to_owned());
        }
    }
    claimed
}

/// Returns every source path in the tree except an auto-discovered entry point.
fn source_paths_on_disk() -> BTreeSet<String> {
    let root = workspace_root();
    let mut found = BTreeSet::new();
    let mut pending = vec![root.join(CRATE_DIRECTORY)];
    while let Some(directory) = pending.pop() {
        let entries = std::fs::read_dir(&directory)
            .unwrap_or_else(|failure| panic!("{} is unreadable: {failure}", directory.display()));
        for entry in entries {
            let path = entry.expect("the directory entry is readable").path();
            if path.is_dir() {
                pending.push(path);
                continue;
            }
            if path.extension().is_none_or(|extension| extension != "rs") {
                continue;
            }
            let relative = path.strip_prefix(&root).expect("the path is inside the workspace");
            let relative = relative.to_str().expect("the path is text").to_owned();
            if relative.contains("/src/") && !relative.ends_with(PROCESS_ENTRY_FILE_NAME) {
                found.insert(relative);
            }
        }
    }
    found
}

/// Reports every difference between two named path sets.
fn compare_paths(
    left_name: &str,
    left: &BTreeSet<String>,
    right_name: &str,
    right: &BTreeSet<String>,
) -> Vec<String> {
    let mut differences: Vec<String> = left
        .difference(right)
        .map(|path| format!("{path} is in the {left_name} but not the {right_name}"))
        .collect();
    differences.extend(
        right
            .difference(left)
            .map(|path| format!("{path} is in the {right_name} but not the {left_name}")),
    );
    differences.sort();
    differences
}

/// Returns the child module names a source file declares.
fn declared_children(text: &str) -> BTreeSet<String> {
    text.lines()
        .map(str::trim)
        .filter_map(|line| line.strip_prefix("pub mod ").or_else(|| line.strip_prefix("mod ")))
        .filter_map(|rest| rest.strip_suffix(';'))
        .map(str::to_owned)
        .collect()
}

/// Reports every way a module fails the present-state documentation rule.
fn evaluate_documentation(path: &str, text: &str) -> Vec<String> {
    let mut violations = Vec::new();
    if !text.starts_with("//!") {
        violations.push(format!("{path} does not open with module documentation"));
    }
    let documented = text
        .lines()
        .any(|line| line.strip_prefix("//!").is_some_and(|rest| !rest.trim().is_empty()));
    if !documented {
        violations.push(format!("{path} has empty module documentation"));
    }
    violations.extend(
        PLACEHOLDER_BODIES
            .iter()
            .filter(|body| text.contains(**body))
            .map(|body| format!("{path} carries the placeholder body {body}")),
    );
    violations
}

/// Reports every line a family root carries beyond documentation and children.
fn evaluate_family_root(path: &str, text: &str) -> Vec<String> {
    text.lines()
        .map(str::trim)
        .filter(|line| {
            !line.is_empty() && !line.starts_with("//!") && declared_children(line).is_empty()
        })
        .map(|line| format!("{path} carries the non-structural line {line:?}"))
        .collect()
}

/// Loads the accepted ownership map, asserting it parses without violation.
fn accepted_rows() -> Vec<OwnershipRow> {
    let (rows, violations) = parse_ownership(&read_fixture(OWNERSHIP_FIXTURE));
    assert_eq!(violations, Vec::<String>::new());
    rows
}

#[test]
fn the_ownership_map_the_footprint_and_the_source_tree_describe_one_set() {
    let rows = accepted_rows();
    let declared: BTreeSet<String> = rows.iter().map(|row| row.path.clone()).collect();
    assert_eq!(declared.len(), rows.len(), "the map declares a path twice");
    let footprint = footprint_paths();
    let disk = source_paths_on_disk();
    assert_eq!(compare_paths("map", &declared, "footprint", &footprint), Vec::<String>::new());
    assert_eq!(compare_paths("map", &declared, "source tree", &disk), Vec::<String>::new());
}

#[test]
fn every_declared_layer_matches_its_crate_and_graph() {
    let rows = accepted_rows();
    let layers = parse_layers(&read_fixture(LAYER_FIXTURE));
    assert_eq!(evaluate_layers(&rows, &layers), Vec::<String>::new());
    let mapped: BTreeSet<String> = rows.iter().map(|row| row.package.clone()).collect();
    let declared: BTreeSet<String> = layers.keys().cloned().collect();
    assert_eq!(mapped, declared, "every crate has a layer and every layer has a crate");
}

#[test]
fn every_crate_root_declares_exactly_the_modules_it_owns() {
    let rows = accepted_rows();
    let mut expected: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    for row in &rows {
        expected.entry(row.module.clone()).or_default();
    }
    for row in &rows {
        if let Some((parent, child)) = row.module.rsplit_once("::") {
            expected.entry(parent.to_owned()).or_default().insert(child.to_owned());
        }
    }
    for row in &rows {
        let text = read_repository_file(&row.path);
        let owned = expected.get(&row.module).expect("every module is in the map");
        assert_eq!(&declared_children(&text), owned, "{} declares the wrong children", row.path);
    }
}

#[test]
fn every_declared_module_is_documented_without_a_placeholder_body() {
    for row in accepted_rows() {
        let text = read_repository_file(&row.path);
        assert_eq!(evaluate_documentation(&row.path, &text), Vec::<String>::new());
    }
}

#[test]
fn every_family_root_carries_only_documentation_and_its_children() {
    for row in accepted_rows().into_iter().filter(|row| row.kind == FAMILY_ROOT_KIND) {
        let text = read_repository_file(&row.path);
        assert_eq!(evaluate_family_root(&row.path, &text), Vec::<String>::new());
    }
}

#[test]
fn named_vocabulary_stays_in_the_crate_the_architecture_assigns_it() {
    let rows = accepted_rows();
    let modules: BTreeMap<String, String> =
        rows.iter().map(|row| (row.module.clone(), row.package.clone())).collect();
    let vocabulary = parse_vocabulary(&read_fixture(VOCABULARY_FIXTURE));
    let expected: BTreeMap<String, String> = VOCABULARY_PLACEMENT
        .iter()
        .map(|(item, package)| ((*item).to_owned(), (*package).to_owned()))
        .collect();
    let observed: BTreeMap<String, String> = vocabulary
        .iter()
        .map(|(item, module)| {
            let package = modules
                .get(module)
                .unwrap_or_else(|| panic!("{item} names the undeclared module {module}"));
            (item.clone(), package.clone())
        })
        .collect();
    assert_eq!(observed, expected);
}

#[test]
fn the_ownership_map_rejects_every_recorded_mutation() {
    let disk = source_paths_on_disk();
    let footprint = footprint_paths();
    let layers = parse_layers(&read_fixture(LAYER_FIXTURE));
    for name in ["rejected-wildcard-path.txt", "rejected-duplicate-path.txt"] {
        let (_, violations) = parse_ownership(&read_fixture(name));
        assert!(!violations.is_empty(), "{name} must be rejected");
    }
    for name in ["rejected-later-plan-leaf.txt", "rejected-missing-leaf.txt"] {
        let (rows, violations) = parse_ownership(&read_fixture(name));
        let declared: BTreeSet<String> = rows.iter().map(|row| row.path.clone()).collect();
        let differences = compare_paths("map", &declared, "source tree", &disk);
        assert!(!violations.is_empty() || !differences.is_empty(), "{name} must be rejected");
        assert!(
            !compare_paths("map", &declared, "footprint", &footprint).is_empty()
                || name == "rejected-missing-leaf.txt",
            "{name} must leave the footprint"
        );
    }
    for name in
        ["rejected-misowned-process-harness.txt", "rejected-support-crate-product-layer.txt"]
    {
        let (rows, violations) = parse_ownership(&read_fixture(name));
        let mut rejected = violations;
        rejected.extend(evaluate_layers(&rows, &layers));
        let declared: BTreeSet<String> = rows.iter().map(|row| row.path.clone()).collect();
        rejected.extend(compare_paths("map", &declared, "source tree", &disk));
        assert!(!rejected.is_empty(), "{name} must be rejected");
    }
}
