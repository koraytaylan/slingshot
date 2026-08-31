//! What a release may build from, and what it refuses to build from.
//!
//! A release that resolved anything at build time would be a release nobody can
//! reproduce: the same commit built twice would draw different bytes on the two
//! days. So the cache is prepared once, deliberately, over the network, and
//! verified offline before a single crate is compiled from it.
//!
//! What verification establishes is narrow, and these tests hold it to exactly
//! that: the cache is the one prepared for this lockfile, unchanged, and inside
//! its bounds. Whether the bytes were trustworthy when they were fetched is a
//! different question, and answering it here would be the dangerous kind of
//! wrong. So the tests below prove the manifest is not taken at its word - the
//! cache is walked and digested - and they prove nothing about provenance.

use std::path::{Path, PathBuf};

use serde_json::{Map, Value};
use slingshot_development::finite_state_machine_compatibility::{
    FiniteStateMachineCompatibilityPin, MANIFEST_PATH, SeedLimits,
};
use slingshot_development::release_input_cache::{
    CacheDeclaration, CacheRefusal, DECLARATION_PATH, MANIFEST_FORMAT, RESOLUTION, SCHEMA_PATH,
    lockfile_digest, manifest_path, parse_declaration, prepare, survey, verified,
};

/// Where the fixtures live.
const FIXTURES: &str = "tests/fixtures/release-input-cache";

/// How many directories separate this crate from the workspace root.
const CRATE_DEPTH: usize = 2;

/// Entries every cache in these tests is built from.
const ENTRIES: &[(&str, &str)] = &[
    ("registry/cache/example/first.crate", "the first crate's bytes"),
    ("registry/cache/example/second.crate", "the second crate's bytes"),
    ("registry/index/example/.cache/first", "what the index says about the first"),
];

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

/// Returns one fixture's text.
fn fixture(name: &str) -> String {
    std::fs::read_to_string(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(FIXTURES).join(name))
        .unwrap_or_else(|failure| panic!("{name} could not be read: {failure}"))
}

/// Returns what this repository declares a release may build from.
fn declaration() -> CacheDeclaration {
    parse_declaration(&read_repository_file(DECLARATION_PATH)).expect("the declaration parses")
}

/// Returns the one authority for what a Cargo home may be.
fn limits() -> SeedLimits {
    FiniteStateMachineCompatibilityPin::parse(&read_repository_file(MANIFEST_PATH))
        .expect("the compatibility manifest parses")
        .cargo_home_seed
}

/// Returns the digest of the lockfile this workspace builds from.
fn lock_digest() -> String {
    lockfile_digest(&workspace_root()).expect("the workspace has a lockfile")
}

/// Builds a cache holding [`ENTRIES`], with no manifest yet.
fn cache_built(named: &str) -> PathBuf {
    let root = std::env::temp_dir().join(format!("release-cache-{named}-{}", std::process::id()));
    std::fs::remove_dir_all(&root).ok();
    for (relative, content) in ENTRIES {
        let path = root.join(relative);
        std::fs::create_dir_all(path.parent().expect("an entry has a parent"))
            .expect("the entry's directory is created");
        std::fs::write(&path, content).expect("the entry is written");
    }
    root
}

/// Reads one cache's manifest as loose members.
fn manifest_members(cache: &Path) -> Map<String, Value> {
    let text = std::fs::read_to_string(manifest_path(cache)).expect("the manifest is there");
    serde_json::from_str(&text).expect("the manifest reads")
}

/// Writes loose members back as one cache's manifest.
fn write_manifest(cache: &Path, members: &Map<String, Value>) {
    let rendered = serde_json::to_string(members).expect("the manifest writes");
    std::fs::write(manifest_path(cache), rendered).expect("the manifest is replaced");
}

#[test]
fn the_declaration_says_a_release_resolves_nothing_at_build_time() {
    let held = declaration();
    assert_eq!(held.resolution, RESOLUTION);
    assert!(held.requires.checksums, "an entry nobody checksummed is bytes nobody authenticated");
    assert!(held.requires.locked_resolution);
    assert!(held.requires.registry_only, "a dependency from anywhere is from anywhere");
    assert_eq!(held.format, "slingshot.release-input-cache/1");
}

#[test]
fn a_declaration_that_would_resolve_online_is_refused() {
    let held = read_repository_file(DECLARATION_PATH).replace(RESOLUTION, "online");
    assert_eq!(
        parse_declaration(&held),
        Err(CacheRefusal::ResolutionUnacceptable("online".to_owned())),
        "a release that resolves at build time is a release nobody can reproduce"
    );
}

#[test]
fn a_cache_prepared_for_this_lockfile_is_accepted() {
    let cache = cache_built("accepted");
    let written =
        prepare(&cache, &declaration(), &limits(), &lock_digest()).expect("this cache prepares");
    assert_eq!(written.format, MANIFEST_FORMAT);
    assert_eq!(written.resolution, RESOLUTION);
    assert_eq!(written.entries, ENTRIES.len() as u64);
    let read_back =
        verified(&cache, &declaration(), &limits(), &lock_digest()).expect("and verifies");
    assert_eq!(read_back, written, "verifying reads back exactly what preparing wrote");
    std::fs::remove_dir_all(&cache).ok();
}

#[test]
fn the_manifest_does_not_count_itself_among_what_the_cache_holds() {
    let cache = cache_built("not-itself");
    let before = survey(&cache).expect("the cache surveys");
    prepare(&cache, &declaration(), &limits(), &lock_digest()).expect("it prepares");
    let after = survey(&cache).expect("and surveys again");
    assert_eq!(before, after, "preparing a cache would otherwise change what it measured");
    std::fs::remove_dir_all(&cache).ok();
}

/// One declared change to a prepared manifest.
#[derive(Debug, serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct Refusal {
    /// What it is called.
    name: String,
    /// What it changes about the manifest.
    r#override: Map<String, Value>,
    /// Which refusal it earns.
    refusal: String,
}

/// Returns which refusal variant one failure is.
fn refusal_name(failure: &CacheRefusal) -> &'static str {
    match failure {
        CacheRefusal::Unreadable(_) => "Unreadable",
        CacheRefusal::ForeignFormat(_) => "ForeignFormat",
        CacheRefusal::ResolutionUnacceptable(_) => "ResolutionUnacceptable",
        CacheRefusal::AnotherLockfile => "AnotherLockfile",
        CacheRefusal::OutsideItsBounds(_) => "OutsideItsBounds",
        CacheRefusal::Changed(_) => "Changed",
    }
}

#[test]
fn every_declared_change_to_a_prepared_manifest_is_refused() {
    let declared: Vec<Refusal> = fixture("refusals.jsonl")
        .lines()
        .filter(|line| !line.trim().is_empty() && !line.starts_with('#'))
        .map(|line| serde_json::from_str(line).expect("every declared refusal reads"))
        .collect();
    assert!(!declared.is_empty());
    for refusal in declared {
        let cache = cache_built(&refusal.name);
        prepare(&cache, &declaration(), &limits(), &lock_digest())
            .expect("the cache prepares first");
        let mut members = manifest_members(&cache);
        for (named, held) in refusal.r#override {
            members.insert(named, held);
        }
        write_manifest(&cache, &members);
        let failure = verified(&cache, &declaration(), &limits(), &lock_digest())
            .expect_err(&format!("{} was accepted", refusal.name));
        assert_eq!(refusal_name(&failure), refusal.refusal, "{} earned {failure}", refusal.name);
        std::fs::remove_dir_all(&cache).ok();
    }
}

#[test]
fn a_cache_prepared_for_another_lockfile_is_not_this_release_s_cache() {
    let cache = cache_built("another-lock");
    prepare(&cache, &declaration(), &limits(), &lock_digest()).expect("it prepares");
    let held = verified(&cache, &declaration(), &limits(), &"0".repeat(lock_digest().len()));
    assert_eq!(held.unwrap_err(), CacheRefusal::AnotherLockfile);
    std::fs::remove_dir_all(&cache).ok();
}

#[test]
fn changing_one_entry_after_preparation_is_noticed() {
    let cache = cache_built("changed-entry");
    prepare(&cache, &declaration(), &limits(), &lock_digest()).expect("it prepares");
    let (relative, content) = ENTRIES[0];
    std::fs::write(cache.join(relative), format!("{content} and one more word"))
        .expect("the entry is rewritten");
    let failure =
        verified(&cache, &declaration(), &limits(), &lock_digest()).expect_err("it is noticed");
    assert_eq!(refusal_name(&failure), "Changed", "the manifest is not taken at its word");
    std::fs::remove_dir_all(&cache).ok();
}

#[test]
fn slipping_an_extra_entry_in_after_preparation_is_noticed() {
    let cache = cache_built("extra-entry");
    prepare(&cache, &declaration(), &limits(), &lock_digest()).expect("it prepares");
    std::fs::write(cache.join("registry/cache/example/third.crate"), "bytes nobody asked for")
        .expect("the extra entry is written");
    let failure =
        verified(&cache, &declaration(), &limits(), &lock_digest()).expect_err("it is noticed");
    assert_eq!(refusal_name(&failure), "Changed");
    std::fs::remove_dir_all(&cache).ok();
}

#[test]
fn a_cache_outside_what_a_cargo_home_may_be_is_refused_where_it_is_made() {
    let cache = cache_built("over-bounds");
    let mut tightened = limits();
    tightened.maximum_files = ENTRIES.len() as u64 - 1;
    let failure = prepare(&cache, &declaration(), &tightened, &lock_digest())
        .expect_err("more files than a Cargo home may hold");
    assert_eq!(refusal_name(&failure), "OutsideItsBounds");
    assert!(
        !manifest_path(&cache).exists(),
        "a cache nobody may build from is not left stamped as one that may be"
    );

    let mut narrowed = limits();
    narrowed.maximum_aggregate_file_bytes = 1;
    let failure = prepare(&cache, &declaration(), &narrowed, &lock_digest())
        .expect_err("more bytes than a Cargo home may hold");
    assert_eq!(refusal_name(&failure), "OutsideItsBounds");

    let mut shortened = limits();
    shortened.maximum_relative_path_utf8_bytes = 1;
    let failure = prepare(&cache, &declaration(), &shortened, &lock_digest())
        .expect_err("a longer path than a Cargo home may hold");
    assert_eq!(refusal_name(&failure), "OutsideItsBounds");
    std::fs::remove_dir_all(&cache).ok();
}

#[test]
fn a_cache_holding_anything_but_ordinary_files_is_refused_by_the_one_verifier() {
    let cache = cache_built("not-ordinary");
    prepare(&cache, &declaration(), &limits(), &lock_digest()).expect("it prepares");
    std::os::unix::fs::symlink(cache.join(ENTRIES[0].0), cache.join("registry/link"))
        .expect("the link is made");
    let failure = verified(&cache, &declaration(), &limits(), &lock_digest())
        .expect_err("a link is not an ordinary file");
    assert_eq!(
        refusal_name(&failure),
        "OutsideItsBounds",
        "what a Cargo home may hold is decided in one place, and this cache is one"
    );
    std::fs::remove_dir_all(&cache).ok();
}

#[test]
fn the_declaration_does_not_restate_what_a_cargo_home_may_be() {
    let held = read_repository_file(DECLARATION_PATH);
    assert!(!held.contains("[bounds]"), "two documents naming one limit can disagree");
    let limits = limits();
    for restated in [
        limits.maximum_files,
        limits.maximum_directories,
        limits.maximum_depth,
        limits.maximum_file_bytes,
        limits.maximum_aggregate_file_bytes,
        limits.maximum_component_utf8_bytes,
        limits.maximum_relative_path_utf8_bytes,
    ] {
        assert!(
            !held.contains(&restated.to_string()),
            "{restated} is the compatibility manifest's to declare, and this copies it"
        );
    }
}

#[test]
fn the_schema_and_the_manifest_that_is_written_agree() {
    let cache = cache_built("schema");
    prepare(&cache, &declaration(), &limits(), &lock_digest()).expect("it prepares");
    let written = manifest_members(&cache);
    let schema: Value =
        serde_json::from_str(&read_repository_file(SCHEMA_PATH)).expect("the schema reads");
    for member in schema["required"].as_array().expect("the schema names what is required") {
        let named = member.as_str().expect("a member is named");
        assert!(written.contains_key(named), "the written manifest omits {named}");
    }
    let properties = schema["properties"].as_object().expect("the schema names its members");
    for named in written.keys() {
        assert!(properties.contains_key(named), "the schema does not describe {named}");
    }
    assert_eq!(schema["properties"]["resolution"]["const"].as_str(), Some(RESOLUTION));
    assert_eq!(schema["properties"]["format"]["const"].as_str(), Some(MANIFEST_FORMAT));
    std::fs::remove_dir_all(&cache).ok();
}

#[test]
fn a_cache_with_no_manifest_at_all_is_refused_rather_than_assumed() {
    let cache = cache_built("no-manifest");
    let failure =
        verified(&cache, &declaration(), &limits(), &lock_digest()).expect_err("there is none");
    assert_eq!(refusal_name(&failure), "Unreadable");
    std::fs::remove_dir_all(&cache).ok();
}

#[test]
fn the_preparation_command_names_every_input_it_is_given() {
    let held = read_repository_file("scripts/prepare_locked_source_cache");
    for option in [
        "--finite-state-machine-source",
        "--rustsec-advisory-database",
        "--rustsec-owner-review-record",
        "--coverage-fuzzing-tool-bundle",
        "--output-directory",
    ] {
        assert!(held.contains(option), "{option} is an input this command is given explicitly");
    }
    assert!(
        held.contains("support/github-automation-authority.toml"),
        "a review record is evidence only against the authority that says whose approval counts"
    );
    assert!(held.contains("[ ! -e \"$OUTPUT_DIRECTORY\" ]"), "it prepares only into a new place");
}

#[test]
fn the_verifier_neither_fetches_nor_repairs_what_it_is_checking() {
    let held = read_repository_file("scripts/verify_locked_source_cache");
    assert!(held.contains("--cache-set"), "it is given the cache explicitly");
    for reaching in ["cargo fetch", "cargo update", "cargo install"] {
        assert!(
            !held.contains(reaching),
            "a verifier that ran {reaching} would report on something else"
        );
    }
}

#[test]
fn the_two_scripts_say_which_one_reaches_the_network() {
    let prepare_script = read_repository_file("scripts/prepare_locked_source_cache");
    assert!(
        prepare_script.contains("this command reaches the network"),
        "the one that does says so, out loud, when it runs"
    );
    let verify_script = read_repository_file("scripts/verify_locked_source_cache");
    assert!(!verify_script.contains("cargo fetch"), "and the one that does not, does not");
    let flowed =
        verify_script.replace('#', " ").split_whitespace().collect::<Vec<&str>>().join(" ");
    assert!(
        flowed.contains("says nothing about whether the bytes inside it were trustworthy"),
        "and says plainly what it leaves unanswered"
    );
}
