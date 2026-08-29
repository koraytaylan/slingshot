//! Structural assertions for the Plan 0002 source-leaf scaffold.
//!
//! Four independent sources describe the same set of source files: the checked
//! ownership fixture, the mutation footprints recorded in the plan's task
//! documents, the `pub mod` declarations reachable from each crate root, and
//! the compiled source tree. Every assertion compares them in both directions,
//! so an undeclared source file, a declaration without a file, a duplicate row,
//! a leaf placed in the wrong crate, and a leaf reachable from a second parent
//! all fail.
//!
//! A leaf gains behavior when its owning descendant task lands. The
//! documentation-only assertion therefore reads the status each descendant task
//! records and applies only while every task that touches the leaf is still
//! unlanded, so this test never has to be relaxed to let the plan proceed.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

/// Directory holding the fixtures this test compares the source tree against.
const FIXTURE_DIRECTORY: &str =
    "crates/slingshot-development/tests/fixtures/profile-authentication-module-scaffold";

/// Fixture that maps every Plan 0002 source leaf to its owner and parent.
const OWNERSHIP_FIXTURE: &str = "leaf-ownership.txt";

/// Directory holding this plan's task documents.
const PLAN_TASK_DIRECTORY: &str = "docs/plans/0002-profiles-and-authentication/tasks";

/// Identity of the task that creates the whole leaf inventory.
const SCAFFOLD_TASK: &str = "profile-authentication-module-scaffold";

/// Number of columns in an ownership fixture row.
const OWNERSHIP_COLUMN_COUNT: usize = 5;

/// Directory every workspace member lives in.
const CRATE_DIRECTORY: &str = "crates";

/// Directory inside a workspace member that holds its library sources.
const SOURCE_DIRECTORY: &str = "src";

/// Source file name of a crate root.
const CRATE_ROOT_FILE_NAME: &str = "lib.rs";

/// Source file name of a module-family root.
const FAMILY_ROOT_FILE_NAME: &str = "mod.rs";

/// Status a task document records before it lands.
const UNLANDED_STATUS: &str = "status: planned";

/// Package that owns each leaf family.
///
/// The family is recorded independently of the source path, so a row that
/// moves a leaf into another crate fails this table even when its path, parent,
/// and module path remain consistent with one another.
const FAMILY_PACKAGE: &[(&str, &str)] = &[
    ("authentication-fake", "slingshot-test-support"),
    ("authentication-review", "slingshot-development"),
    ("authentication-transport", "slingshot-agent-connection"),
    ("configuration-boundary", "slingshot-configuration"),
    ("profile-contract", "slingshot-domain"),
];

/// Plan 0002 vocabulary that belongs to a leaf and never to a shared parent.
///
/// A parent that names one of these has taken on behavior a descendant task
/// owns, which is the drift the scaffold exists to prevent.
const LEAF_VOCABULARY: &[&str] = &[
    "AccessToken",
    "AdditionalCertificateAuthority",
    "AdobeExperienceManagerDeployment",
    "AdobeIdentityManagement",
    "AuthenticationPrincipalIdentity",
    "AuthorTargetIdentity",
    "ConfigurationDiagnostic",
    "ConfigurationGeneration",
    "ConfigurationRoot",
    "ConfigurationSnapshot",
    "EnvironmentAuthentication",
    "PlatformTrust",
    "ProfileDocument",
    "SecretValue",
    "SelectedEnvironmentRevision",
    "SensitiveConfigurationDocument",
    "ServiceCredential",
    "TransportPolicy",
];

/// Tokens that mark work an author has not finished.
const FORBIDDEN_MARKERS: &[&str] = &["TODO", "FIXME", "XXX", "HACK", "TBD", "WIP"];

/// Line openings an unlanded leaf may not carry.
///
/// A documentation-only module declares nothing, so any item opening, any
/// import, and any attribute is a feature the leaf's owning task has not landed
/// yet.
const UNLANDED_LINE_OPENINGS: &[&str] =
    &["use ", "pub ", "const ", "static ", "fn ", "struct ", "enum ", "impl ", "#["];

/// Position of the package name inside a crate source path.
const PACKAGE_SEGMENT: usize = 1;

/// Position of the source directory inside a crate source path.
const SOURCE_SEGMENT: usize = 2;

/// Number of leading segments before a crate source path's module segments.
const MODULE_SEGMENT_START: usize = 3;

/// One source leaf and the ownership the fixture assigns it.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
struct LeafRow {
    /// Repository-relative source path.
    path: String,
    /// Package that owns the source file.
    package: String,
    /// Boundary family the leaf belongs to.
    family: String,
    /// Module path of the crate root or family root that declares the leaf.
    parent: String,
    /// Rust module path the file declares.
    module: String,
}

impl LeafRow {
    /// Renders the row in the fixture's canonical single-line form.
    fn render(&self) -> String {
        let LeafRow { path, package, family, parent, module } = self;
        format!("{path}|{package}|{family}|{parent}|{module}")
    }
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

/// Derives the package, parent module, and module path a source path implies.
fn derive_shape(path: &str) -> Result<(String, String, String), String> {
    let segments: Vec<&str> = path.split('/').collect();
    let (Some(&CRATE_DIRECTORY), Some(package), Some(&SOURCE_DIRECTORY)) =
        (segments.first(), segments.get(PACKAGE_SEGMENT), segments.get(SOURCE_SEGMENT))
    else {
        return Err(format!("{path} is not a crate library source path"));
    };
    let tail = &segments[MODULE_SEGMENT_START..];
    let Some(file_name) = tail.last() else {
        return Err(format!("{path} names no source file"));
    };
    let Some(leaf) = file_name.strip_suffix(".rs") else {
        return Err(format!("{file_name} is not a Rust source file"));
    };
    if [CRATE_ROOT_FILE_NAME, FAMILY_ROOT_FILE_NAME].contains(file_name) {
        return Err(format!("{path} is a structural parent, not a leaf"));
    }
    let mut parent = vec![package.replace('-', "_")];
    parent.extend(tail[..tail.len() - 1].iter().map(|&segment| segment.to_owned()));
    let parent = parent.join("::");
    Ok(((*package).to_owned(), format!("{parent}::{leaf}"), parent))
}

/// Parses the ownership fixture and reports every structural violation.
fn parse_ownership(text: &str) -> (Vec<LeafRow>, Vec<String>) {
    let mut rows = Vec::new();
    let mut violations = Vec::new();
    let families: BTreeMap<&str, &str> = FAMILY_PACKAGE.iter().copied().collect();
    let mut seen = BTreeSet::new();
    for line in data_lines(text) {
        let columns: Vec<&str> = line.split('|').collect();
        if columns.len() != OWNERSHIP_COLUMN_COUNT {
            violations.push(format!("{line:?} does not have {OWNERSHIP_COLUMN_COUNT} columns"));
            continue;
        }
        let row = LeafRow {
            path: columns[0].to_owned(),
            package: columns[1].to_owned(),
            family: columns[2].to_owned(),
            parent: columns[3].to_owned(),
            module: columns[4].to_owned(),
        };
        if !seen.insert(row.path.clone()) {
            violations.push(format!("{} is declared more than once", row.path));
            continue;
        }
        violations.extend(evaluate_row(&row, &families));
        rows.push(row);
    }
    (rows, violations)
}

/// Reports every way one fixture row disagrees with itself or with the family
/// table.
fn evaluate_row(row: &LeafRow, families: &BTreeMap<&str, &str>) -> Vec<String> {
    let mut violations = Vec::new();
    match families.get(row.family.as_str()) {
        None => violations.push(format!("{} claims the unknown family {}", row.path, row.family)),
        Some(&owner) if owner != row.package => violations
            .push(format!("{} puts the {} family in {}", row.path, row.family, row.package)),
        Some(_) => {}
    }
    match derive_shape(&row.path) {
        Err(reason) => violations.push(reason),
        Ok((package, module, parent)) => {
            if package != row.package {
                violations.push(format!("{} is owned by {package}, not {}", row.path, row.package));
            }
            if module != row.module {
                violations.push(format!("{} declares {module}, not {}", row.path, row.module));
            }
            if parent != row.parent {
                violations.push(format!("{} sits under {parent}, not {}", row.path, row.parent));
            }
        }
    }
    violations
}

/// Loads the accepted ownership rows, asserting the fixture parses cleanly.
fn accepted_rows() -> Vec<LeafRow> {
    let (rows, violations) = parse_ownership(&read_fixture(OWNERSHIP_FIXTURE));
    assert_eq!(violations, Vec::<String>::new());
    rows
}

/// The source files one task's recorded footprint claims.
#[derive(Debug, Default)]
struct Footprint {
    /// Library source paths that are structural parents.
    parents: BTreeSet<String>,
    /// Library source paths that are leaves.
    leaves: BTreeSet<String>,
}

/// Returns the library source files each Plan 0002 task's footprint claims.
fn plan_footprints() -> BTreeMap<String, Footprint> {
    let directory = workspace_root().join(PLAN_TASK_DIRECTORY);
    let mut footprints = BTreeMap::new();
    let entries = std::fs::read_dir(&directory).expect("the plan task directory is readable");
    for entry in entries {
        let path = entry.expect("the task entry is readable").path();
        let document = std::fs::read_to_string(&path).expect("the task document is readable");
        let frontmatter = document.split("---").nth(1).expect("the task has frontmatter");
        let identity = frontmatter
            .lines()
            .find_map(|line| line.strip_prefix("id: "))
            .expect("the task declares an identity")
            .trim()
            .to_owned();
        let mut footprint = Footprint::default();
        for claimed in footprint_entries(frontmatter) {
            if claimed.ends_with(CRATE_ROOT_FILE_NAME) || claimed.ends_with(FAMILY_ROOT_FILE_NAME) {
                footprint.parents.insert(claimed);
            } else {
                footprint.leaves.insert(claimed);
            }
        }
        assert!(
            footprints.insert(identity.clone(), footprint).is_none(),
            "{identity} is declared by two task documents"
        );
    }
    footprints
}

/// Returns the identity of every Plan 0002 task that has already landed.
fn landed_tasks() -> BTreeSet<String> {
    let directory = workspace_root().join(PLAN_TASK_DIRECTORY);
    let mut landed = BTreeSet::new();
    for entry in std::fs::read_dir(&directory).expect("the plan task directory is readable") {
        let path = entry.expect("the task entry is readable").path();
        let document = std::fs::read_to_string(&path).expect("the task document is readable");
        let identity = document
            .lines()
            .find_map(|line| line.strip_prefix("id: "))
            .expect("the task declares an identity")
            .trim()
            .to_owned();
        if !document.contains(UNLANDED_STATUS) {
            landed.insert(identity);
        }
    }
    landed
}

/// Returns the library source paths one task's frontmatter names.
fn footprint_entries(frontmatter: &str) -> Vec<String> {
    let mut claimed = Vec::new();
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
        if entry.starts_with(CRATE_DIRECTORY)
            && entry.contains("/src/")
            && entry.ends_with(".rs")
            && !entry.contains('*')
        {
            claimed.push(entry.to_owned());
        }
    }
    claimed
}

/// The parent that declares one module and the file the module lives in.
#[derive(Debug, Clone)]
struct Declaration {
    /// Module path of the declaring crate root or family root.
    parent: String,
    /// Repository-relative path of the declared module's source file.
    path: String,
}

/// Walks every crate root and returns each declared module path exactly once
/// per declaring parent.
fn declaration_graph() -> BTreeMap<String, Vec<Declaration>> {
    let root = workspace_root();
    let mut graph: BTreeMap<String, Vec<Declaration>> = BTreeMap::new();
    let mut pending: Vec<(String, String)> = Vec::new();
    let members = std::fs::read_dir(root.join(CRATE_DIRECTORY)).expect("the crate directory reads");
    for member in members {
        let directory = member.expect("the crate entry is readable").path();
        let Some(package) = directory.file_name().and_then(|name| name.to_str()) else {
            continue;
        };
        let relative = format!("{CRATE_DIRECTORY}/{package}/{SOURCE_DIRECTORY}");
        if root.join(&relative).join(CRATE_ROOT_FILE_NAME).is_file() {
            pending.push((package.replace('-', "_"), relative));
        }
    }
    while let Some((module, directory)) = pending.pop() {
        let file = if module.contains("::") {
            format!("{directory}/{FAMILY_ROOT_FILE_NAME}")
        } else {
            format!("{directory}/{CRATE_ROOT_FILE_NAME}")
        };
        for child in declared_children(&read_repository_file(&file)) {
            let child_module = format!("{module}::{child}");
            let leaf_path = format!("{directory}/{child}.rs");
            let family_directory = format!("{directory}/{child}");
            if root.join(&family_directory).join(FAMILY_ROOT_FILE_NAME).is_file() {
                pending.push((child_module.clone(), family_directory.clone()));
                let path = format!("{family_directory}/{FAMILY_ROOT_FILE_NAME}");
                graph
                    .entry(child_module)
                    .or_default()
                    .push(Declaration { parent: module.clone(), path });
            } else {
                graph
                    .entry(child_module)
                    .or_default()
                    .push(Declaration { parent: module.clone(), path: leaf_path });
            }
        }
    }
    graph
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

/// Reports every difference between two named path sets.
fn compare(
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

/// Reports every way one unlanded leaf carries more than module documentation.
fn evaluate_unlanded_leaf(path: &str, text: &str) -> Vec<String> {
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
    for line in text.lines().map(str::trim).filter(|line| !line.is_empty()) {
        if line.starts_with("//!") {
            continue;
        }
        let opening = UNLANDED_LINE_OPENINGS.iter().find(|opening| line.starts_with(**opening));
        violations.push(opening.map_or_else(
            || format!("{path} carries the non-documentation line {line:?}"),
            |opening| format!("{path} declares {opening:?} before its owning task lands"),
        ));
    }
    violations.extend(
        FORBIDDEN_MARKERS
            .iter()
            .filter(|marker| text.contains(**marker))
            .map(|marker| format!("{path} carries the unfinished-work marker {marker}")),
    );
    violations
}

/// Returns the source paths this plan's task footprints claim as leaves.
fn footprint_leaves(footprints: &BTreeMap<String, Footprint>) -> BTreeSet<String> {
    footprints[SCAFFOLD_TASK].leaves.clone()
}

#[test]
fn the_fixture_the_task_footprints_and_the_source_tree_describe_one_leaf_set() {
    let rows = accepted_rows();
    let declared: BTreeSet<String> = rows.iter().map(|row| row.path.clone()).collect();
    assert_eq!(declared.len(), rows.len(), "the fixture declares a leaf twice");
    let footprints = plan_footprints();
    assert_eq!(
        compare("fixture", &declared, "scaffold footprint", &footprint_leaves(&footprints)),
        Vec::<String>::new()
    );
    let missing: Vec<&String> =
        declared.iter().filter(|path| !workspace_root().join(path).is_file()).collect();
    assert_eq!(missing, Vec::<&String>::new(), "a declared leaf has no source file");
    for (task, footprint) in &footprints {
        let outside: Vec<&String> = footprint.leaves.difference(&declared).collect();
        assert_eq!(outside, Vec::<&String>::new(), "{task} touches a leaf the scaffold omits");
        let unadopted: Vec<&String> =
            footprint.parents.difference(&footprints[SCAFFOLD_TASK].parents).collect();
        assert_eq!(unadopted, Vec::<&String>::new(), "{task} touches an unadopted parent");
    }
}

#[test]
fn the_ownership_fixture_is_byte_identical_to_its_canonical_rendering() {
    let text = read_fixture(OWNERSHIP_FIXTURE);
    let mut rows = accepted_rows();
    rows.sort();
    let rendered: Vec<String> = rows.iter().map(LeafRow::render).collect();
    let recorded: Vec<String> = data_lines(&text).iter().map(|line| (*line).to_owned()).collect();
    assert_eq!(recorded, rendered, "the fixture is not in canonical order and form");
    assert!(text.ends_with('\n'), "the fixture does not end with one line feed");
}

#[test]
fn every_declared_leaf_is_reachable_from_exactly_one_owning_parent() {
    let graph = declaration_graph();
    for row in accepted_rows() {
        let declarations = graph
            .get(&row.module)
            .unwrap_or_else(|| panic!("{} is declared by no parent", row.module));
        let parents: Vec<&str> =
            declarations.iter().map(|declaration| declaration.parent.as_str()).collect();
        assert_eq!(parents, vec![row.parent.as_str()], "{} has the wrong parents", row.module);
        assert_eq!(declarations[0].path, row.path, "{} resolves to another file", row.module);
    }
}

#[test]
fn no_structural_parent_carries_plan_0002_leaf_vocabulary() {
    let rows = accepted_rows();
    let parents: BTreeSet<String> = plan_footprints()[SCAFFOLD_TASK].parents.clone();
    assert_eq!(parents.len(), rows.iter().map(|row| &row.parent).collect::<BTreeSet<_>>().len());
    let mut violations = Vec::new();
    for parent in &parents {
        let text = read_repository_file(parent);
        violations.extend(
            LEAF_VOCABULARY
                .iter()
                .filter(|word| text.contains(**word))
                .map(|word| format!("{parent} names the leaf vocabulary {word}")),
        );
        if !parent.ends_with(FAMILY_ROOT_FILE_NAME) {
            continue;
        }
        violations.extend(
            text.lines()
                .map(str::trim)
                .filter(|line| {
                    !line.is_empty()
                        && !line.starts_with("//!")
                        && declared_children(line).is_empty()
                })
                .map(|line| format!("{parent} carries the non-structural line {line:?}")),
        );
    }
    assert_eq!(violations, Vec::<String>::new());
}

#[test]
fn an_unlanded_leaf_carries_only_present_state_module_documentation() {
    let footprints = plan_footprints();
    let landed = landed_tasks();
    let mut violations = Vec::new();
    for row in accepted_rows() {
        let implemented = footprints.iter().any(|(task, footprint)| {
            task != SCAFFOLD_TASK && footprint.leaves.contains(&row.path) && landed.contains(task)
        });
        if implemented {
            continue;
        }
        violations.extend(evaluate_unlanded_leaf(&row.path, &read_repository_file(&row.path)));
    }
    assert_eq!(violations, Vec::<String>::new());
}

#[test]
fn the_ownership_fixture_rejects_every_recorded_structural_mutation() {
    let graph = declaration_graph();
    let accepted: BTreeSet<String> = accepted_rows().into_iter().map(|row| row.path).collect();
    for name in [
        "rejected-undeclared-source-file.txt",
        "rejected-declaration-without-file.txt",
        "rejected-duplicate-leaf.txt",
        "rejected-misowned-leaf.txt",
        "rejected-second-parent.txt",
    ] {
        let (rows, mut rejected) = parse_ownership(&read_fixture(name));
        let declared: BTreeSet<String> = rows.iter().map(|row| row.path.clone()).collect();
        rejected.extend(compare("map", &declared, "accepted inventory", &accepted));
        for row in &rows {
            if !workspace_root().join(&row.path).is_file() {
                rejected.push(format!("{} has no source file", row.path));
            }
            let parents: Vec<&str> = graph
                .get(&row.module)
                .map(|declarations| {
                    declarations.iter().map(|entry| entry.parent.as_str()).collect()
                })
                .unwrap_or_default();
            if parents != vec![row.parent.as_str()] {
                rejected.push(format!("{} is not declared by {}", row.module, row.parent));
            }
        }
        assert!(!rejected.is_empty(), "{name} must be rejected");
    }
}
