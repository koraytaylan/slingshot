//! Three descriptions of one module inventory, checked against each other.
//!
//! The fixture is written independently of the declarations, and both are
//! compared against what is on disk and against the footprint the task that
//! created them recorded. A leaf that is declared and absent, present and
//! undeclared, owned twice, or ordered differently under the command family is
//! a finding rather than something a checker forgives, because each of those is
//! how a module ends up reachable from two places or from none.
//!
//! The order of the command family is checked as well as its membership. A
//! reader meeting these commands in the reference and then in the source should
//! meet them in the same order, and an order that drifts is a small thing that
//! nobody notices until the two disagree about which command is which.
//!
//! The leaves are documentation only for now, and the checker says so directly:
//! a leaf carrying an item, a body, or a planning marker has stopped being a
//! scaffold and belongs to whichever task was supposed to implement it.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

/// Where the independent inventory lives.
const FIXTURE: &str = "tests/fixtures/command-line-module-scaffold/leaves.txt";

/// The crate every leaf belongs to.
const CRATE_SOURCE: &str = "crates/slingshot-command-line/src";

/// The parent a top-level leaf is declared by.
const CRATE_ROOT: &str = "crate-root";

/// The parent a command leaf is declared by.
const COMMAND_FAMILY: &str = "commands";

/// The position column a top-level leaf carries.
const UNORDERED: &str = "top-level";

/// Modules that existed before Plan 0006 and are not its scaffold.
const INHERITED: &[&str] = &[
    "command_line",
    "commands",
    "daemon_connection",
    "daemon_entry",
    "daemon_process",
    "explicit_daemon_start",
    "lib",
    "main",
    "mod",
    "model_context_protocol",
    "platform_runtime",
];

/// Markers a scaffold leaf may not carry.
const PLANNING_MARKERS: &[&str] =
    &["TODO", "FIXME", "will be", "for now", "not yet", "coming soon"];

/// How many directories separate this crate from the workspace root.
const CRATE_DEPTH: usize = 2;

/// Returns the workspace root.
fn workspace_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(CRATE_DEPTH)
        .expect("the crate sits two directories below the workspace")
        .to_path_buf()
}

/// Returns one repository file's text.
fn read_repository_file(relative: &str) -> String {
    let path = workspace_root().join(relative);
    std::fs::read_to_string(&path).unwrap_or_else(|_| panic!("{relative} is readable"))
}

/// One row of the independent inventory.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
struct Leaf {
    /// Where in its parent's declaration order it sits, when that is fixed.
    position: String,
    /// Which parent declares it.
    parent: String,
    /// Where the file is.
    path: String,
}

/// Returns every row the fixture states.
fn inventory() -> Vec<Leaf> {
    let text = read_repository_file(&format!("crates/slingshot-command-line/{FIXTURE}"));
    text.lines()
        .map(str::trim)
        .filter(|line| !line.is_empty() && !line.starts_with('#'))
        .map(|line| {
            let mut parts = line.split('|');
            Leaf {
                path: parts.next().expect("a path").to_owned(),
                parent: parts.next().expect("a parent").to_owned(),
                position: parts.next().expect("a position").to_owned(),
            }
        })
        .collect()
}

/// Returns the child modules `text` declares, in order.
fn declared_children(text: &str) -> Vec<String> {
    text.lines()
        .filter_map(|line| line.trim().strip_prefix("pub mod "))
        .filter_map(|line| line.strip_suffix(';'))
        .map(str::to_owned)
        .collect()
}

/// Returns every Rust source file the crate holds, as repository paths.
fn source_files() -> BTreeSet<String> {
    let mut held = BTreeSet::new();
    for directory in [CRATE_SOURCE.to_owned(), format!("{CRATE_SOURCE}/commands")] {
        let entries = std::fs::read_dir(workspace_root().join(&directory))
            .unwrap_or_else(|_| panic!("{directory} is readable"));
        for entry in entries {
            let entry = entry.expect("one directory entry");
            if entry.path().extension().is_some_and(|extension| extension == "rs") {
                let name = entry.file_name().to_string_lossy().to_string();
                held.insert(format!("{directory}/{name}"));
            }
        }
    }
    held
}

#[test]
fn the_fixture_the_declarations_and_the_source_tree_describe_one_inventory() {
    let inventory = inventory();
    let declared_top = declared_children(&read_repository_file(&format!("{CRATE_SOURCE}/lib.rs")));
    let declared_commands =
        declared_children(&read_repository_file(&format!("{CRATE_SOURCE}/commands/mod.rs")));
    let present = source_files();

    for leaf in &inventory {
        assert!(present.contains(&leaf.path), "{} is in the fixture and not on disk", leaf.path);
        let name = leaf
            .path
            .rsplit('/')
            .next()
            .and_then(|file| file.strip_suffix(".rs"))
            .expect("a module name");
        let declared =
            if leaf.parent == COMMAND_FAMILY { &declared_commands } else { &declared_top };
        assert!(
            declared.contains(&name.to_owned()),
            "{} is in the fixture and its parent does not declare it",
            leaf.path
        );
        assert_eq!(
            declared.iter().filter(|held| *held == name).count(),
            1,
            "{} is declared more than once",
            leaf.path
        );
    }

    let inventoried: BTreeSet<String> = inventory.iter().map(|leaf| leaf.path.clone()).collect();
    for path in &present {
        let name = path
            .rsplit('/')
            .next()
            .and_then(|file| file.strip_suffix(".rs"))
            .expect("a module name");
        if INHERITED.contains(&name) {
            continue;
        }
        assert!(inventoried.contains(path), "{path} is on disk and in no fixture row");
    }
}

#[test]
fn the_command_family_is_declared_in_the_order_the_fixture_fixes() {
    let expected: Vec<String> = {
        let mut ordered: Vec<&Leaf> = Vec::new();
        let inventory = inventory();
        let held: Vec<&Leaf> =
            inventory.iter().filter(|leaf| leaf.parent == COMMAND_FAMILY).collect();
        for position in 0..held.len() {
            let at = held
                .iter()
                .find(|leaf| leaf.position == position.to_string())
                .unwrap_or_else(|| panic!("the fixture fixes position {position}"));
            ordered.push(at);
        }
        ordered
            .into_iter()
            .map(|leaf| {
                leaf.path
                    .rsplit('/')
                    .next()
                    .and_then(|file| file.strip_suffix(".rs"))
                    .expect("a module name")
                    .to_owned()
            })
            .collect()
    };
    let declared =
        declared_children(&read_repository_file(&format!("{CRATE_SOURCE}/commands/mod.rs")));
    assert_eq!(
        declared, expected,
        "a reader meeting these commands twice should meet them in the same order"
    );
}

#[test]
fn every_top_level_leaf_is_owned_by_the_crate_root_and_nothing_else() {
    for leaf in inventory() {
        if leaf.parent == COMMAND_FAMILY {
            assert_ne!(leaf.position, UNORDERED, "{}: a command leaf has a position", leaf.path);
            continue;
        }
        assert_eq!(leaf.parent, CRATE_ROOT, "{}: a leaf has one owning parent", leaf.path);
        assert_eq!(leaf.position, UNORDERED, "{}: only the family fixes an order", leaf.path);
        assert!(
            !leaf.path.contains("/commands/"),
            "{}: a crate-root leaf does not live under the family",
            leaf.path
        );
    }
}

#[test]
fn every_scaffold_leaf_carries_documentation_and_no_behavior_at_all() {
    for leaf in inventory() {
        let text = read_repository_file(&leaf.path);
        let named = &leaf.path;
        assert!(text.starts_with("//!"), "{named} opens with its module documentation");
        for marker in PLANNING_MARKERS {
            assert!(!text.contains(marker), "{named} carries planning language: {marker:?}");
        }
        for line in text.lines() {
            assert!(
                line.trim().is_empty() || line.trim_start().starts_with("//!"),
                "{named} carries something other than documentation: {line}"
            );
        }
        assert!(
            text.lines().filter(|line| line.starts_with("//!")).count() > 1,
            "{named} says what it owns rather than only naming itself"
        );
    }
}

#[test]
fn the_family_root_declares_its_children_and_holds_nothing_else() {
    let text = read_repository_file(&format!("{CRATE_SOURCE}/commands/mod.rs"));
    for line in text.lines() {
        let line = line.trim();
        assert!(
            line.is_empty() || line.starts_with("//!") || line.starts_with("pub mod "),
            "a family root declares its children and nothing else: {line}"
        );
    }
    assert!(!text.contains('*'), "and declares each of them by name rather than by wildcard");
}
