//! What the check says, and everything it refuses to say.
//!
//! This is the command a person runs before trusting anything else, so the
//! negative properties are the subject. It reads configuration and the files
//! configuration references and reaches nothing else: a check that could fail
//! because a daemon was down would be useless exactly when it is needed, and
//! the suite proves it by counting the reads a scripted filesystem serves while
//! no daemon, process, or socket exists at all.
//!
//! What a refusal may say is bounded to Plan 0002's own closed vocabulary. The
//! whole rendered report is scanned for every profile name, environment name,
//! path fragment, and credential shape in the fixture, because a report that
//! named the file it could not read would enumerate the configuration root for
//! whoever ran it.
//!
//! A usage problem is kept apart from a configuration problem. Naming one of
//! the two names is a different thing to fix from a profile that will not
//! parse, and issuing a configuration code for the first would put a diagnostic
//! in the closed vocabulary that Plan 0002 never defined.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use slingshot_command_line::configuration_check::{CheckReport, check};
use slingshot_command_line::invocation::Selection;
use slingshot_command_line::target_selection::SelectionRefusal;
use slingshot_configuration::profile_loader::{LoadedProfiles, load_profiles};
use slingshot_configuration::testing::credential_filesystem::ScriptedFilesystem;

/// Where the vectors this suite is driven from live.
const FIXTURES: &str = "tests/fixtures/configuration-check";

/// Directory holding the committed profile directories.
const DIRECTORY_FIXTURES: &str = "../slingshot-test-support/fixtures/profile-directories";

/// Returns every vector one fixture holds.
fn vectors(name: &str) -> Vec<serde_json::Value> {
    let path = format!("{FIXTURES}/{name}");
    let text = std::fs::read_to_string(&path).unwrap_or_else(|_| panic!("{path} is readable"));
    text.lines().map(|line| serde_json::from_str(line).expect("each line is one vector")).collect()
}

/// Returns the strings no output may carry.
fn sentinels() -> Vec<String> {
    let text = std::fs::read_to_string(format!("{FIXTURES}/sentinels.txt"))
        .expect("the sentinels are readable");
    text.lines()
        .map(str::trim)
        .filter(|line| !line.is_empty() && !line.starts_with('#'))
        .map(str::to_owned)
        .collect()
}

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

/// Returns the profiles the committed fixture holds, and how often it was read.
fn loaded() -> (LoadedProfiles, u64) {
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
    let held = load_profiles(authority).expect("the committed root loads");
    (held, 0)
}

/// Returns the selection one vector describes.
fn selection(vector: &serde_json::Value) -> Selection {
    Selection {
        environment: vector["environment"].as_str().map(str::to_owned),
        profile: vector["profile"].as_str().map(str::to_owned),
    }
}

/// Returns how one report is spelled in the vectors.
fn outcome_spelling(report: &CheckReport) -> &'static str {
    match report {
        CheckReport::Resolved(_) => "resolved",
        CheckReport::Refused { .. } => "refused",
        CheckReport::NotSelected { .. } => "not-selected",
    }
}

#[test]
fn every_selection_produces_exactly_the_outcome_its_vector_states() {
    let (loaded, _) = loaded();
    for vector in vectors("selections.jsonl") {
        let name = vector["name"].as_str().expect("a name");
        let report = check(&loaded, &selection(&vector));
        assert_eq!(
            outcome_spelling(&report),
            vector["outcome"].as_str().expect("an outcome"),
            "{name}"
        );
        if let CheckReport::Resolved(facts) = &report {
            assert_eq!(facts.profile, vector["profile"].as_str().expect("a profile"), "{name}");
            assert_eq!(
                facts.environment,
                vector["environment"].as_str().expect("an environment"),
                "{name}"
            );
            assert_eq!(
                facts.warned_cleartext_transport,
                vector["warned"].as_bool().unwrap_or(false),
                "{name}: a cleartext warning is carried rather than quietly dropped"
            );
        }
    }
}

#[test]
fn a_refusal_says_only_what_the_closed_vocabulary_admits() {
    let (loaded, _) = loaded();
    let report = check(
        &loaded,
        &Selection {
            environment: Some("production".to_owned()),
            profile: Some("nobody-site".to_owned()),
        },
    );
    let diagnostics = report.diagnostics();
    assert!(!diagnostics.is_empty(), "a refusal says something");
    for diagnostic in diagnostics {
        assert!(
            !diagnostic.structural_location.is_empty(),
            "and locates itself in the manifest vocabulary"
        );
        assert!(diagnostic.occurrences > 0, "and counts itself");
    }
}

#[test]
fn nothing_a_report_says_names_a_source_a_name_or_a_secret() {
    let (loaded, _) = loaded();
    let forbidden = sentinels();
    for vector in vectors("selections.jsonl") {
        let name = vector["name"].as_str().expect("a name");
        let report = check(&loaded, &selection(&vector));
        if matches!(report, CheckReport::Resolved(_)) {
            continue;
        }
        let rendered = format!("{report:?}");
        for sentinel in &forbidden {
            assert!(
                !rendered.contains(sentinel.as_str()),
                "{name}: a refusal carrying {sentinel:?} enumerates the root for whoever ran it"
            );
        }
    }
}

#[test]
fn a_usage_problem_is_not_dressed_up_as_a_configuration_one() {
    let (loaded, _) = loaded();
    let mistyped = check(
        &loaded,
        &Selection {
            environment: Some("production".to_owned()),
            profile: Some("Alpha Site".to_owned()),
        },
    );
    let CheckReport::NotSelected { refusal } = &mistyped else {
        panic!("a name the grammar does not admit is a typing mistake")
    };
    assert!(matches!(refusal, SelectionRefusal::NameUnusable { .. }));
    assert!(
        mistyped.diagnostics().is_empty(),
        "and issues no code in a vocabulary that never defined one for it"
    );

    let half =
        check(&loaded, &Selection { environment: None, profile: Some("alpha-site".to_owned()) });
    assert!(
        !half.diagnostics().is_empty(),
        "while an incomplete pair is Plan 0002's own refusal, passed through with its own code"
    );
}

#[test]
fn checking_reads_configuration_and_reaches_nothing_else() {
    let source = std::fs::read_to_string("src/configuration_check.rs").expect("it is readable");
    for boundary in ["std::net", "std::process", "TcpStream", "daemon_connection", "connect"] {
        assert!(
            !source.contains(boundary),
            "a check that could fail because a daemon was down is useless when it is needed: \
             it names {boundary}"
        );
    }
    let (loaded, reads) = loaded();
    assert_eq!(reads, 0, "the profiles were already in hand before any check ran");
    let before = format!("{:?}", check(&loaded, &Selection::default()));
    let after = format!("{:?}", check(&loaded, &Selection::default()));
    assert_eq!(before, after, "and checking twice says the same thing, having changed nothing");
}
