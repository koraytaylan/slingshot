//! Four descriptions of one protocol-module inventory, checked against each other.
//!
//! The fixture is written independently of the declarations; both are compared
//! against what is on disk and against the footprint of the task that created
//! them. A leaf that is declared and absent, present and undeclared, owned
//! twice, or ordered differently is a finding rather than something a checker
//! forgives, because each of those is how a module ends up reachable from two
//! places or from none.
//!
//! The order is checked as well as the membership. These leaves are met in the
//! reference, in the source, and in the plan, and an order that drifts between
//! them is a small thing nobody notices until two of them disagree about which
//! leaf is which.
//!
//! Every leaf is structure and documentation. A leaf carrying an item, a body,
//! a protocol constant, or a planning marker has stopped being a scaffold and
//! belongs to whichever task was supposed to implement it.

use std::collections::BTreeSet;
use std::path::PathBuf;

/// Where the independent inventory lives.
const FIXTURE: &str = "tests/fixtures/model-context-protocol/module-scaffold/leaves.txt";

/// Where the family lives.
const FAMILY_SOURCE: &str = "crates/slingshot-command-line/src/model_context_protocol";

/// The task document whose footprint has to name every leaf.
const TASK_DOCUMENT: &str = "docs/plans/0007-model-context-protocol-server/tasks/2904-model-context-protocol-module-scaffold.md";

/// The parent every leaf is declared by.
const FAMILY: &str = "model_context_protocol";

/// The file a family root is written in.
const ROOT_FILE: &str = "mod.rs";

/// Markers a scaffold leaf may not carry.
const PLANNING_MARKERS: &[&str] =
    &["TODO", "FIXME", "will be", "for now", "not yet", "coming soon"];

/// Beginnings of a line that declares something rather than describing it.
const ITEM_BEGINNINGS: &[&str] = &[
    "pub ", "fn ", "const ", "static ", "struct ", "enum ", "trait ", "impl ", "use ", "type ",
    "#[",
];

/// How many directories separate this crate from the workspace root.
const CRATE_DEPTH: usize = 2;

/// Returns the workspace root.
fn workspace_root() -> PathBuf {
    let mut root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    for _ in 0..CRATE_DEPTH {
        root = root.parent().expect("the crate is inside the workspace").to_path_buf();
    }
    root
}

/// Reads one file from the workspace.
fn read_repository_file(relative: &str) -> String {
    let path = workspace_root().join(relative);
    std::fs::read_to_string(&path)
        .unwrap_or_else(|failure| panic!("{} could not be read: {failure}", path.display()))
}

/// One row of the independent inventory.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Leaf {
    /// Where the leaf lives.
    path: String,
    /// Which parent declares it.
    parent: String,
    /// Where it sits in its parent's order.
    position: usize,
}

/// Returns the inventory the fixture declares.
fn inventory() -> Vec<Leaf> {
    let text = read_repository_file(&format!("crates/slingshot-command-line/{FIXTURE}"));
    text.lines()
        .filter(|line| !line.trim().is_empty() && !line.starts_with('#'))
        .map(|line| {
            let mut fields = line.split('|');
            let path = fields.next().expect("a row names a path").to_owned();
            let parent = fields.next().expect("a row names a parent").to_owned();
            let position =
                fields.next().expect("a row names a position").parse().expect("a position counts");
            assert!(fields.next().is_none(), "a row carries three fields");
            Leaf { path, parent, position }
        })
        .collect()
}

/// Returns the children one family root declares, in the order it declares them.
fn declared_children(root: &str) -> Vec<String> {
    root.lines()
        .filter_map(|line| line.strip_prefix("pub mod ").and_then(|rest| rest.strip_suffix(';')))
        .map(str::to_owned)
        .collect()
}

/// Returns every leaf file on disk, by name.
fn leaves_on_disk() -> BTreeSet<String> {
    let directory = workspace_root().join(FAMILY_SOURCE);
    std::fs::read_dir(&directory)
        .expect("the family directory is readable")
        .filter_map(Result::ok)
        .map(|entry| entry.file_name().to_string_lossy().into_owned())
        .filter(|name| name.ends_with(".rs") && name != ROOT_FILE)
        .map(|name| name.trim_end_matches(".rs").to_owned())
        .collect()
}

/// Returns the source footprint the scaffold task recorded.
fn footprint() -> BTreeSet<String> {
    let document = read_repository_file(TASK_DOCUMENT);
    let frontmatter = document.split("---").nth(1).expect("the task has frontmatter");
    frontmatter
        .lines()
        .filter_map(|line| line.strip_prefix("  - "))
        .map(|entry| entry.trim().trim_matches('"').to_owned())
        .filter(|entry| entry.contains("/src/"))
        .collect()
}

#[test]
fn the_fixture_the_declarations_and_the_source_tree_describe_one_inventory() {
    let inventory = inventory();
    let declared =
        declared_children(&read_repository_file(&format!("{FAMILY_SOURCE}/{ROOT_FILE}")));
    let named: Vec<String> = inventory
        .iter()
        .map(|leaf| {
            leaf.path
                .rsplit('/')
                .next()
                .expect("a path names a file")
                .trim_end_matches(".rs")
                .to_owned()
        })
        .collect();
    assert_eq!(declared, named, "the root declares something the fixture does not, or otherwise");
    assert_eq!(leaves_on_disk(), named.iter().cloned().collect::<BTreeSet<String>>());
    for leaf in &inventory {
        assert_eq!(leaf.parent, FAMILY, "{} is parented elsewhere", leaf.path);
        assert!(workspace_root().join(&leaf.path).is_file(), "{} is not on disk", leaf.path);
    }
}

#[test]
fn the_order_the_fixture_fixes_is_the_order_the_root_declares() {
    let inventory = inventory();
    let mut ordered = inventory.clone();
    ordered.sort_by_key(|leaf| leaf.position);
    assert_eq!(ordered, inventory, "the fixture is written out of its own order");
    let positions: Vec<usize> = inventory.iter().map(|leaf| leaf.position).collect();
    let expected: Vec<usize> = (0..inventory.len()).collect();
    assert_eq!(positions, expected, "a position is repeated or skipped");
}

#[test]
fn every_leaf_is_in_the_footprint_of_the_task_that_created_it() {
    let claimed = footprint();
    for leaf in inventory() {
        assert!(claimed.contains(&leaf.path), "{} is outside the scaffold's footprint", leaf.path);
    }
    let root = format!("{FAMILY_SOURCE}/{ROOT_FILE}");
    assert!(claimed.contains(&root), "the family root is outside the scaffold's footprint");
    assert_eq!(
        claimed.len(),
        inventory().len() + 1,
        "the footprint claims a source file nothing declares"
    );
}

#[test]
fn the_family_root_declares_its_children_and_holds_nothing_else() {
    let root = read_repository_file(&format!("{FAMILY_SOURCE}/{ROOT_FILE}"));
    for line in root.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with("//!") || line.starts_with("pub mod ") {
            continue;
        }
        panic!("the family root holds {line:?}");
    }
    assert!(!root.contains("#[cfg"), "a child is declared unconditionally or not at all");
    assert!(!root.contains("pub use "), "a family root re-exports nothing");
}

#[test]
fn every_leaf_opens_with_documentation_and_declares_nothing_yet() {
    for leaf in inventory() {
        let source = read_repository_file(&leaf.path);
        assert!(
            source.starts_with("//!"),
            "{} opens with something other than its own docs",
            leaf.path
        );
        for marker in PLANNING_MARKERS {
            assert!(!source.contains(marker), "{} carries planning language: {marker}", leaf.path);
        }
        for line in source.lines() {
            let line = line.trim();
            if line.is_empty() || line.starts_with("//!") {
                continue;
            }
            assert!(
                !ITEM_BEGINNINGS.iter().any(|beginning| line.starts_with(beginning)),
                "{} declares {line:?} before the task that owns it",
                leaf.path
            );
        }
    }
}
