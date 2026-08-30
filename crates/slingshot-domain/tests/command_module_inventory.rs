//! Structural assertions for the command leaf inventory.
//!
//! One plan adds one command leaf per command over twenty tasks. The list of
//! those leaves therefore has to live somewhere that every task can add to
//! without touching what another task owns, and it has to be exactly one place:
//! two lists of the same thing disagree the moment one of them is edited.
//!
//! `command-module-inventory.txt` is that place. It, the family root's
//! declarations, and the files on disk are compared in both directions, and the
//! workspace module map delegates this family to it rather than repeating it.
//!
//! A leaf holds documentation alone until its owning task lands, and holds an
//! implementation once that task records itself as done. Both directions are
//! checked, so a status stamped without an implementation fails as loudly as an
//! implementation without a status.

use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;

/// Fixture that names every command leaf.
const INVENTORY_FIXTURE: &str = "tests/fixtures/command-module-inventory.txt";

/// Directory the command family lives in.
const FAMILY_DIRECTORY: &str = "src/command";

/// Source file name of the family root.
const FAMILY_ROOT_FILE_NAME: &str = "mod.rs";

/// Directory holding every plan bundle, relative to this crate.
const PLAN_DIRECTORY: &str = "../../docs/plans";

/// Fixture Plan 0001 maps the workspace module tree with.
const WORKSPACE_MAP: &str =
    "../slingshot-development/tests/fixtures/workspace-module-map/module-ownership.txt";

/// Status a task document records before it lands.
const UNLANDED_STATUS: &str = "status: planned";

/// The one leaf this task implements itself.
const FOUNDATION_LEAF: &str = "command_identity";

/// Line openings an unlanded leaf may not carry.
const UNLANDED_LINE_OPENINGS: &[&str] =
    &["use ", "pub ", "const ", "static ", "fn ", "struct ", "enum ", "impl ", "#["];

/// Returns the directory this crate's manifest lives in.
fn crate_directory() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

/// Reads one file relative to this crate.
fn read_crate_file(relative: &str) -> String {
    let path = crate_directory().join(relative);
    std::fs::read_to_string(&path)
        .unwrap_or_else(|failure| panic!("{} could not be read: {failure}", path.display()))
}

/// Returns every leaf the inventory names.
fn inventoried() -> Vec<String> {
    read_crate_file(INVENTORY_FIXTURE)
        .lines()
        .map(str::trim_end)
        .filter(|line| !line.is_empty() && !line.starts_with('#'))
        .map(str::to_owned)
        .collect()
}

/// Returns every leaf file the family directory holds.
fn present() -> BTreeSet<String> {
    let directory = crate_directory().join(FAMILY_DIRECTORY);
    let mut found = BTreeSet::new();
    for entry in std::fs::read_dir(&directory).expect("the family directory reads") {
        let path = entry.expect("the entry reads").path();
        let name = path.file_name().expect("the file has a name").to_string_lossy().into_owned();
        if name == FAMILY_ROOT_FILE_NAME {
            continue;
        }
        found.insert(
            name.strip_suffix(".rs")
                .unwrap_or_else(|| panic!("{name} is not a source file"))
                .to_owned(),
        );
    }
    found
}

/// Returns every child the family root declares.
fn declared() -> BTreeSet<String> {
    read_crate_file(&format!("{FAMILY_DIRECTORY}/{FAMILY_ROOT_FILE_NAME}"))
        .lines()
        .map(str::trim)
        .filter_map(|line| line.strip_prefix("pub mod "))
        .filter_map(|rest| rest.strip_suffix(';'))
        .map(str::to_owned)
        .collect()
}

/// Returns which plan task owns each leaf, and whether it has landed.
fn owning_tasks() -> BTreeMap<String, bool> {
    let root = crate_directory().join(PLAN_DIRECTORY);
    let mut owners = BTreeMap::new();
    for plan in std::fs::read_dir(&root).expect("the plan directory reads") {
        let tasks = plan.expect("the plan entry reads").path().join("tasks");
        if !tasks.is_dir() {
            continue;
        }
        for task in std::fs::read_dir(&tasks).expect("the task directory reads") {
            let path = task.expect("the task entry reads").path();
            if path.is_dir() {
                continue;
            }
            let document = std::fs::read_to_string(&path).expect("the task document reads");
            let landed = !document.contains(UNLANDED_STATUS);
            for line in document.lines() {
                let Some(claimed) = line.trim().strip_prefix("- ") else {
                    continue;
                };
                let claimed = claimed.trim_matches('"');
                let Some(leaf) = claimed
                    .strip_prefix("crates/slingshot-domain/src/command/")
                    .and_then(|leaf| leaf.strip_suffix(".rs"))
                else {
                    continue;
                };
                owners.entry(leaf.to_owned()).or_insert(false);
                if landed {
                    owners.insert(leaf.to_owned(), true);
                }
            }
        }
    }
    owners
}

/// Returns the lines of one leaf that are neither blank nor documentation.
fn implementation_lines(text: &str) -> Vec<&str> {
    text.lines()
        .map(str::trim)
        .filter(|line| !line.is_empty() && !line.starts_with("//!"))
        .collect()
}

#[test]
fn the_inventory_the_declarations_and_the_files_describe_one_set() {
    let inventoried = inventoried();
    let listed: BTreeSet<String> = inventoried.iter().cloned().collect();
    assert_eq!(listed.len(), inventoried.len(), "the inventory names a leaf twice");
    let mut ordered = inventoried.clone();
    ordered.sort();
    assert_eq!(inventoried, ordered, "the inventory is not in byte order");
    assert_eq!(listed, present(), "the inventory and the family directory disagree");
    assert_eq!(listed, declared(), "the inventory and the family root disagree");
}

#[test]
fn the_workspace_map_keeps_the_family_root_and_enumerates_no_command_leaf() {
    let map = read_crate_file(WORKSPACE_MAP);
    // The family is a directory, so the separator is part of the prefix: without
    // it a later sibling named `command_fingerprint.rs` reads as a command leaf.
    let rooted: Vec<&str> = map
        .lines()
        .filter(|line| line.starts_with("crates/slingshot-domain/src/command/"))
        .collect();
    assert_eq!(
        rooted,
        vec![
            "crates/slingshot-domain/src/command/mod.rs|slingshot-domain|domain|family-root|slingshot_domain::command|workspace-module-map"
        ],
        "the workspace map holds a second command-leaf inventory"
    );
}

#[test]
fn every_leaf_has_an_owning_task_and_holds_what_that_task_has_landed() {
    let owners = owning_tasks();
    let mut violations = Vec::new();
    for leaf in inventoried() {
        let text = read_crate_file(&format!("{FAMILY_DIRECTORY}/{leaf}.rs"));
        if leaf == FOUNDATION_LEAF {
            assert!(!implementation_lines(&text).is_empty(), "{leaf} holds no implementation");
            continue;
        }
        let Some(landed) = owners.get(&leaf) else {
            violations.push(format!("{leaf} has no owning task"));
            continue;
        };
        if *landed {
            if implementation_lines(&text).is_empty() {
                violations.push(format!("{leaf} records a landed owner but holds no code"));
            }
            continue;
        }
        if !text.starts_with("//!") {
            violations.push(format!("{leaf} does not open with module documentation"));
        }
        for line in implementation_lines(&text) {
            let opening = UNLANDED_LINE_OPENINGS.iter().find(|opening| line.starts_with(**opening));
            violations.push(opening.map_or_else(
                || format!("{leaf} carries the non-documentation line {line:?}"),
                |opening| format!("{leaf} declares {opening:?} before its owning task lands"),
            ));
        }
    }
    assert_eq!(violations, Vec::<String>::new());
}

#[test]
fn the_inventory_rejects_a_leaf_that_only_one_side_knows_about() {
    let listed: BTreeSet<String> = inventoried().into_iter().collect();
    let mut without = listed.clone();
    let dropped = without.pop_first().expect("the inventory names a leaf");
    assert_ne!(without, present(), "dropping {dropped} left the sets equal");

    let mut surplus = listed.clone();
    surplus.insert("a_leaf_no_file_backs".to_owned());
    assert_ne!(surplus, present(), "an invented leaf left the sets equal");
    assert_ne!(surplus, declared(), "an invented leaf left the declarations equal");
}
