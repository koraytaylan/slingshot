//! Which target each leaf needs, and what it costs to ask for the wrong one.
//!
//! Three leaves need three different things, and conflating them costs
//! something real each time. Help and version need no target, and asking for
//! one would make them fail on a machine with no configuration - which is
//! exactly the machine somebody runs them on. The daemon lifecycle probes need
//! the two names and no profile content, because a caller must be able to stop
//! a daemon whose configuration broke after it started. Everything else needs
//! the whole selection, because it will act against an author under an identity
//! that has to come from what is actually configured.
//!
//! So the tests are mostly about the middle case. A namespace built from names
//! alone is what makes an unfixable configuration fixable, and the way to prove
//! it is to resolve one while the profiles are unreadable.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use slingshot_command_line::invocation::{METADATA_ONLY_LEAVES, Selection, parse};
use slingshot_command_line::target_selection::{
    NAMESPACE_ONLY_LEAVES, NamespacePair, SelectionRefusal, TargetRequirement, namespace_of,
    namespace_of_selection, requested, requirement_of, select,
};
use slingshot_configuration::profile_loader::{LoadedProfiles, load_profiles};
use slingshot_configuration::testing::credential_filesystem::ScriptedFilesystem;
use slingshot_domain::command::catalog::CommandCatalog;

/// Directory holding the committed profile directories.
const DIRECTORY_FIXTURES: &str = "../slingshot-test-support/fixtures/profile-directories";

/// Profile the committed fixture declares.
const PROFILE: &str = "alpha-site";

/// Environment that profile declares.
const ENVIRONMENT: &str = "production";

/// A profile no fixture declares.
const ABSENT_PROFILE: &str = "nobody-site";

/// A name the configuration grammar does not admit.
const UNUSABLE_NAME: &str = "Alpha Site";

/// Returns the files one committed profile directory holds.
fn fixture_files() -> BTreeMap<String, Vec<u8>> {
    let directory =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(DIRECTORY_FIXTURES).join("ordered");
    let mut files = BTreeMap::new();
    collect(&directory, &directory, &mut files);
    files
}

/// Collects every file below `directory`, keyed by its root-relative spelling.
fn collect(root: &Path, directory: &Path, files: &mut BTreeMap<String, Vec<u8>>) {
    for entry in std::fs::read_dir(directory).expect("the fixture directory reads") {
        let path = entry.expect("the entry reads").path();
        if path.is_dir() {
            collect(root, &path, files);
            continue;
        }
        let relative = path.strip_prefix(root).expect("the file is inside the fixture");
        let spelling = relative.to_str().expect("the path is text").replace('\\', "/");
        files.insert(spelling, std::fs::read(&path).expect("the file reads"));
    }
}

/// Returns the lowercase hexadecimal digest of `bytes`.
fn digest(bytes: &[u8]) -> String {
    use sha2::{Digest as _, Sha256};
    Sha256::digest(bytes).iter().map(|octet| format!("{octet:02x}")).collect()
}

/// Returns the profiles the committed fixture holds.
fn loaded() -> LoadedProfiles {
    let mut authority = ScriptedFilesystem::new();
    let files = fixture_files();
    let mut inventory = String::from("format_version = 1\n");
    for (reference, bytes) in &files {
        if reference == "configuration-snapshot.toml" {
            continue;
        }
        authority = authority.with_source(reference, bytes);
        inventory.push_str(&format!(
            "\n[[sources]]\nreference = \"{reference}\"\nsha256 = \"{}\"\n",
            digest(bytes)
        ));
    }
    let authority = authority
        .with_source("configuration-snapshot.toml", inventory.as_bytes())
        .with_directory("profiles");
    load_profiles(authority).expect("the committed root loads")
}

/// Returns the selection two names make.
fn named(profile: &str, environment: &str) -> Selection {
    Selection { environment: Some(environment.to_owned()), profile: Some(profile.to_owned()) }
}

#[test]
fn only_help_and_version_need_no_target_at_all() {
    for leaf in METADATA_ONLY_LEAVES {
        assert_eq!(
            requirement_of(leaf),
            TargetRequirement::None,
            "{leaf} answers on a machine with no configuration, which is where it is run"
        );
    }
    for leaf in NAMESPACE_ONLY_LEAVES {
        assert_eq!(requirement_of(leaf), TargetRequirement::NamespaceOnly, "{leaf}");
    }
    for descriptor in CommandCatalog::published().descriptors() {
        assert_eq!(
            requirement_of(&descriptor.wire_name),
            TargetRequirement::Complete,
            "{}: it acts against an author, so it needs the identity to act under",
            descriptor.wire_name
        );
    }
    assert_eq!(requirement_of("operation-list"), TargetRequirement::Complete);
}

#[test]
fn a_namespace_is_two_names_and_never_profile_content() {
    let pair = namespace_of(&named(PROFILE, ENVIRONMENT)).expect("two names make a namespace");
    assert_eq!(
        pair,
        NamespacePair { environment: ENVIRONMENT.to_owned(), profile: PROFILE.to_owned() }
    );
    assert_eq!(pair.key(), format!("{PROFILE}/{ENVIRONMENT}"));
    assert_eq!(
        namespace_of(&named(ABSENT_PROFILE, ENVIRONMENT)).expect("names are not looked up").key(),
        format!("{ABSENT_PROFILE}/{ENVIRONMENT}"),
        "a stop must reach a daemon whose profile has since become unreadable"
    );
}

#[test]
fn one_name_on_its_own_is_refused_rather_than_completed() {
    let half = Selection { environment: None, profile: Some(PROFILE.to_owned()) };
    assert_eq!(namespace_of(&half), Err(SelectionRefusal::SelectionIncomplete));
    let other = Selection { environment: Some(ENVIRONMENT.to_owned()), profile: None };
    assert_eq!(
        namespace_of(&other),
        Err(SelectionRefusal::SelectionIncomplete),
        "completing one name from somewhere else is how a command is aimed at a server \
         nobody named"
    );
}

#[test]
fn a_name_the_grammar_does_not_admit_is_refused_before_anything_is_looked_up() {
    assert_eq!(
        namespace_of(&named(UNUSABLE_NAME, ENVIRONMENT)),
        Err(SelectionRefusal::NameUnusable { named: UNUSABLE_NAME.to_owned() })
    );
    assert!(matches!(
        requested(&named(PROFILE, UNUSABLE_NAME)),
        Err(SelectionRefusal::NameUnusable { .. })
    ));
    assert!(requested(&named(PROFILE, ENVIRONMENT)).is_ok());
}

#[test]
fn a_complete_selection_resolves_to_the_namespace_it_will_be_a_daemon_under() {
    let loaded = loaded();
    let selection = select(&loaded, &named(PROFILE, ENVIRONMENT)).expect("the fixture declares it");
    let namespace = namespace_of_selection(&selection);
    assert_eq!(namespace.key(), selection.namespace_key());
    assert_eq!(
        namespace,
        namespace_of(&named(PROFILE, ENVIRONMENT)).expect("the same two names"),
        "the namespace a complete selection produces is the one the names alone produce"
    );
}

#[test]
fn a_selection_the_configuration_refuses_carries_its_own_diagnostics_unchanged() {
    let loaded = loaded();
    let refusal = select(&loaded, &named(ABSENT_PROFILE, ENVIRONMENT))
        .expect_err("no profile declares that name");
    let SelectionRefusal::Configuration { count, diagnostics } = refusal else {
        panic!("the configuration's own vocabulary comes through")
    };
    assert_eq!(count, diagnostics.len());
    assert!(count > 0, "and there is at least one of them");
    let rendered = format!("{diagnostics:?}");
    assert!(
        !rendered.contains(ABSENT_PROFILE),
        "a refusal that named what was asked for would enumerate the root for whoever asked"
    );
}

#[test]
fn an_invocation_carries_the_names_a_target_is_resolved_from() {
    let parsed = parse(&[
        "query_paths".to_owned(),
        "--profile".to_owned(),
        PROFILE.to_owned(),
        "--environment".to_owned(),
        ENVIRONMENT.to_owned(),
    ])
    .expect("it parses");
    assert_eq!(
        namespace_of(&parsed.selection).expect("two names").key(),
        format!("{PROFILE}/{ENVIRONMENT}")
    );
    let bare = parse(&["version".to_owned()]).expect("it parses");
    assert_eq!(
        requirement_of(&bare.verb),
        TargetRequirement::None,
        "and a leaf that needs no target is never asked for one"
    );
}
