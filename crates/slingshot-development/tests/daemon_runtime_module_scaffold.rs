//! One declaration per module, and one owner per declaration.
//!
//! A scaffold is worth testing because the failure it prevents is quiet: a leaf
//! that exists but is declared nowhere compiles fine and is simply never
//! reached, and a leaf declared twice compiles fine until two tasks edit it
//! from opposite directions. So the fixture, the declarations, the files on
//! disk, and the footprints of every task in this plan are compared in both
//! directions, and a disagreement anywhere is a failure here rather than a
//! surprise later.
//!
//! The fixture also records which modules this plan *adopts* rather than
//! creates. Plan 0001 owns the crate roots and five behavioral leaves, and a
//! scaffold that recreated one of them would silently take ownership of code
//! somebody else is responsible for.

use std::collections::{BTreeMap, BTreeSet};

use serde_json::Value;

/// The independently ordered ownership fixture.
const OWNERSHIP: &str =
    include_str!("fixtures/daemon-runtime-module-scaffold/module-ownership.jsonl");

/// The task this scaffold is.
const SCAFFOLD: &str = "1406-daemon-runtime-module-scaffold";

/// Binaries the workspace inherited and keeps.
const INHERITED_BINARIES: &[&str] = &["slingshot", "slingshot-development"];

/// Returns the repository root.
fn repository() -> std::path::PathBuf {
    /// Directories between this crate and the repository root.
    const CRATE_DEPTH: usize = 2;

    std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(CRATE_DEPTH)
        .expect("the crate sits two levels below the repository root")
        .to_path_buf()
}

/// Reads one row's string member.
fn text<'row>(row: &'row Value, member: &str) -> &'row str {
    row[member].as_str().unwrap_or_else(|| panic!("{member} is a string in {row}"))
}

/// Returns every fixture row.
fn rows() -> Vec<Value> {
    OWNERSHIP
        .lines()
        .map(|line| serde_json::from_str(line).expect("every fixture line is one object"))
        .collect()
}

/// Returns every path the fixture classifies as one kind.
fn paths_of(kind: &str) -> BTreeSet<String> {
    rows()
        .iter()
        .filter(|row| text(row, "kind") == kind)
        .map(|row| text(row, "path").to_owned())
        .collect()
}

/// Returns the source paths every task in this plan records.
fn footprints() -> BTreeMap<String, BTreeSet<String>> {
    let directory = repository().join("docs/plans/0004-daemon-runtime-and-local-protocol/tasks");
    let mut recorded = BTreeMap::new();
    for entry in std::fs::read_dir(&directory).expect("the task directory reads") {
        let path = entry.expect("a directory entry").path();
        let name = path.file_stem().expect("a task name").to_string_lossy().into_owned();
        let document = std::fs::read_to_string(&path).expect("a task document reads");
        let block = document
            .split_once("touches:")
            .expect("every task records a footprint")
            .1
            .split_once("\nstatus:")
            .expect("every footprint ends at the status")
            .0;
        let sources: BTreeSet<String> = block
            .lines()
            .map(|line| line.trim().trim_start_matches("- ").trim().trim_matches('"').to_owned())
            .filter(|line| {
                line.starts_with("crates/") && line.ends_with(".rs") && line.contains("/src/")
            })
            .collect();
        recorded.insert(name, sources);
    }
    recorded
}

#[test]
fn every_new_leaf_exists_is_declared_once_and_matches_its_fixture_entry() {
    let root = repository();
    for row in rows() {
        let path = text(&row, "path");
        let file = root.join(path);
        assert!(file.is_file(), "{path} has no source file");
        let crate_name = text(&row, "crate");
        assert!(
            path.starts_with(&format!("crates/{crate_name}/src/")),
            "{path} is not in the crate the fixture names"
        );
        let parent = root.join(text(&row, "parent"));
        let declarations = std::fs::read_to_string(&parent).expect("a crate root reads");
        if matches!(text(&row, "kind"), "adopted_root" | "adopted_entry") {
            continue;
        }
        let leaf = module_name(path);
        let declared =
            declarations.lines().filter(|line| line.trim() == format!("pub mod {leaf};")).count();
        assert_eq!(declared, 1, "{path} is declared {declared} times in its crate root");
    }
}

/// Returns the name one leaf is declared under in its parent.
///
/// A file names itself, and a `mod.rs` is named by the directory holding it: a
/// family that grew children is declared exactly as it was when it had none, so
/// growing one is not a change its parent has to notice.
fn module_name(path: &str) -> String {
    let held = std::path::Path::new(path);
    let stem = held.file_stem().expect("a leaf name").to_string_lossy().into_owned();
    if stem != "mod" {
        return stem;
    }
    held.parent()
        .and_then(std::path::Path::file_name)
        .expect("a family directory")
        .to_string_lossy()
        .into_owned()
}

#[test]
fn the_fixture_the_declarations_and_the_footprints_describe_one_set() {
    let scaffold = paths_of("scaffold_leaf");
    assert_eq!(scaffold.len(), 36, "this plan creates thirty-six library leaves");

    let recorded = footprints();
    let owned: BTreeSet<String> = recorded
        .get(SCAFFOLD)
        .expect("the scaffold records a footprint")
        .iter()
        .filter(|path| !path.ends_with("/lib.rs"))
        .cloned()
        .collect();
    assert_eq!(owned, scaffold, "the scaffold footprint and the fixture disagree");

    let descendants: BTreeSet<String> = recorded
        .iter()
        .filter(|(name, _)| name.as_str() != SCAFFOLD)
        .flat_map(|(_, sources)| sources.iter().cloned())
        .collect();
    let adopted: BTreeSet<String> = paths_of("adopted_leaf")
        .union(&paths_of("adopted_root"))
        .chain(paths_of("adopted_entry").iter())
        .cloned()
        .collect();
    assert_eq!(
        descendants.difference(&adopted).cloned().collect::<BTreeSet<String>>(),
        scaffold,
        "every descendant source is either adopted or created by this scaffold"
    );
}

#[test]
fn every_scaffold_leaf_has_at_least_one_descendant_owner_and_no_second_root() {
    for row in rows() {
        let path = text(&row, "path");
        let descendants: Vec<&str> = row["descendants"]
            .as_array()
            .expect("every row lists its descendants")
            .iter()
            .map(|owner| owner.as_str().expect("a task name"))
            .collect();
        if text(&row, "kind") == "adopted_root" {
            continue;
        }
        assert!(!descendants.is_empty(), "{path} is created for nobody");
        assert!(!descendants.contains(&SCAFFOLD), "{path}: the scaffold is not its own descendant");
    }
    let recorded = footprints();
    let dependencies = std::fs::read_to_string(
        repository()
            .join("docs/plans/0004-daemon-runtime-and-local-protocol/tasks")
            .join("1405-daemon-runtime-contract.md"),
    )
    .expect("the contract task reads");
    assert!(
        dependencies.contains("daemon-runtime-module-scaffold"),
        "the contract task depends on this scaffold, so every later task reaches it"
    );
    assert!(recorded.len() >= 34, "every task in the plan records a footprint");
}

#[test]
fn every_scaffold_leaf_names_exactly_the_tasks_that_record_it() {
    let recorded = footprints();
    for row in rows() {
        if text(&row, "kind") != "scaffold_leaf" {
            continue;
        }
        let path = text(&row, "path");
        let declared: BTreeSet<String> = row["descendants"]
            .as_array()
            .expect("a descendant list")
            .iter()
            .map(|owner| owner.as_str().expect("a task name").to_owned())
            .collect();
        let recording: BTreeSet<String> = recorded
            .iter()
            .filter(|(task, sources)| task.as_str() != SCAFFOLD && sources.contains(path))
            .map(|(task, _)| task.clone())
            .collect();
        assert_eq!(
            declared, recording,
            "{path}: the fixture names one set of owners and the footprints another"
        );
    }
}

#[test]
fn no_two_feature_tasks_own_one_source_without_an_ancestor_between_them() {
    let recorded = footprints();
    let mut owners: BTreeMap<&String, Vec<&String>> = BTreeMap::new();
    for (task, sources) in &recorded {
        for source in sources {
            owners.entry(source).or_default().push(task);
        }
    }
    for (source, tasks) in owners {
        if source.ends_with("/lib.rs") {
            continue;
        }
        let features: Vec<&&String> =
            tasks.iter().filter(|task| task.as_str() != SCAFFOLD).collect();
        assert!(
            features.len() <= 1 || is_ordered(source),
            "{source} is owned by {features:?} with no ancestor between them"
        );
    }
}

/// Returns whether a shared source is one the plan deliberately sequences.
///
/// Two tasks may touch one file when the later extends the earlier, and the
/// plan says which. Adopted behavioral leaves are the case that arises: they
/// existed before this plan and more than one task builds on them.
fn is_ordered(source: &str) -> bool {
    paths_of("adopted_leaf").contains(source)
        || rows().iter().any(|row| {
            text(row, "path") == source
                && row["descendants"].as_array().is_some_and(|owners| owners.len() > 1)
        })
}

#[test]
fn every_scaffold_leaf_is_documentation_alone_until_its_owner_lands() {
    let root = repository();
    let landed = landed_tasks();
    for row in rows() {
        if text(&row, "kind") != "scaffold_leaf" {
            continue;
        }
        let path = text(&row, "path");
        let source = std::fs::read_to_string(root.join(path)).expect("a leaf reads");
        let owners: Vec<String> = row["descendants"]
            .as_array()
            .expect("a descendant list")
            .iter()
            .map(|owner| owner.as_str().expect("a task name").to_owned())
            .collect();
        let unlanded = owners.iter().all(|owner| !landed.contains(owner));
        let structural = source
            .lines()
            .all(|line| line.trim().is_empty() || line.trim_start().starts_with("//!"));
        assert_eq!(
            structural, unlanded,
            "{path} holds documentation alone exactly while its owning task is unlanded"
        );
        for marker in PLANNING_LANGUAGE {
            assert!(!states_a_plan(&source, marker), "{path} carries planning language: {marker}");
        }
    }
}

/// Phrases that say a source file is describing work rather than doing it.
const PLANNING_LANGUAGE: &[&str] = &["TODO", "FIXME", "will be", "for now", "placeholder"];

/// Returns whether `source` uses `marker` as its own phrase.
///
/// The comparison is bounded at both ends by something other than a letter,
/// because a bare substring search reads a plan into ordinary prose: "accounts
/// for nowhere" contains "for now", and a statement inventory's bind markers
/// are placeholders in the only sense SQL has for the word.
fn states_a_plan(source: &str, marker: &str) -> bool {
    let letter =
        |text: &str, index: usize| text[index..].chars().next().is_some_and(char::is_alphabetic);
    source.match_indices(marker).any(|(start, found)| {
        let before = source[..start].chars().next_back();
        let after = start + found.len();
        !before.is_some_and(char::is_alphabetic) && !(after < source.len() && letter(source, after))
    })
}

/// Returns every task in this plan that has landed.
fn landed_tasks() -> BTreeSet<String> {
    let directory = repository().join("docs/plans/0004-daemon-runtime-and-local-protocol/tasks");
    let mut landed = BTreeSet::new();
    for entry in std::fs::read_dir(&directory).expect("the task directory reads") {
        let path = entry.expect("a directory entry").path();
        let document = std::fs::read_to_string(&path).expect("a task document reads");
        if document.contains("\nstatus: done\n") {
            landed.insert(path.file_stem().expect("a name").to_string_lossy().into_owned());
        }
    }
    landed
}

#[test]
fn the_workspace_keeps_exactly_the_two_binaries_it_inherited() {
    let root = repository();
    let mut targets = BTreeSet::new();
    for entry in std::fs::read_dir(root.join("crates")).expect("the crate directory reads") {
        let path = entry.expect("a directory entry").path();
        let manifest = path.join("Cargo.toml");
        if !manifest.is_file() {
            continue;
        }
        let manifest_text = std::fs::read_to_string(&manifest).expect("a manifest reads");
        assert!(!path.join("src/bin").exists(), "{} declares a binary directory", path.display());
        if !manifest_text.contains("[[bin]]") && !path.join("src/main.rs").is_file() {
            continue;
        }
        let named = manifest_text
            .lines()
            .skip_while(|line| !line.starts_with("[[bin]]"))
            .find_map(|line| line.strip_prefix("name = "))
            .map(|name| name.trim_matches('"').to_owned());
        targets.insert(
            named.unwrap_or_else(|| {
                path.file_name().expect("a name").to_string_lossy().into_owned()
            }),
        );
    }
    let expected: BTreeSet<String> =
        INHERITED_BINARIES.iter().map(|name| (*name).to_owned()).collect();
    assert_eq!(targets, expected, "the workspace gained or lost a binary");
}

#[test]
fn the_adopted_roots_and_behavioral_leaves_are_classified_and_not_recreated() {
    let adopted = paths_of("adopted_leaf");
    let expected: BTreeSet<String> = [
        "crates/slingshot-command-line/src/daemon_connection.rs",
        "crates/slingshot-daemon/src/local_server.rs",
        "crates/slingshot-daemon/src/ownership.rs",
        "crates/slingshot-daemon/src/platform_runtime/readiness.rs",
        "crates/slingshot-daemon/src/runtime_namespace.rs",
        "crates/slingshot-local-protocol/src/framing.rs",
    ]
    .iter()
    .map(|path| (*path).to_owned())
    .collect();
    assert_eq!(adopted, expected, "the behavioral leaves this plan adopts");
    assert_eq!(
        paths_of("adopted_entry"),
        ["crates/slingshot-development/src/main.rs".to_owned()].into_iter().collect(),
        "one process entry is adopted, and it is not a library leaf"
    );
    assert!(
        adopted.is_disjoint(&paths_of("scaffold_leaf")),
        "an adopted leaf is never also created here"
    );
    let roots = paths_of("adopted_root");
    assert_eq!(roots.len(), 7, "seven crate roots are adopted rather than recreated");
    for root in &roots {
        assert!(root.ends_with("/src/lib.rs"), "{root} is a crate root");
    }
}
