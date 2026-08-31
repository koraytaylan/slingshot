//! Every channel a run can be observed through, searched for every secret.
//!
//! A secret escapes through whichever channel nobody was watching, so what
//! matters is the list of channels rather than another careful look at one of
//! them. Six are searched, and the list itself is committed so a channel that
//! is added and not searched is a failing test.
//!
//! Six encodings are searched too, because a value can leave in a form nobody
//! typed. A credential that leaks base64-encoded has leaked, and a scanner that
//! only looked for what was typed would report a clean run.

use std::path::PathBuf;

use slingshot_test_support::observable_capture::{ObservableCapture, every_encoding};

/// Where the fixtures live.
const FIXTURES: &str = "tests/fixtures/credential-threats";

/// One declared secret.
#[derive(Debug, serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct Sentinel {
    /// What class it stands for.
    name: String,
    /// The distinct value searched for.
    value: String,
    /// What it stands for.
    why: String,
}

/// Returns every declared secret.
fn sentinels() -> Vec<Sentinel> {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(FIXTURES).join("sentinels.jsonl");
    let text = std::fs::read_to_string(&path)
        .unwrap_or_else(|failure| panic!("{} could not be read: {failure}", path.display()));
    text.lines()
        .filter(|line| !line.trim().is_empty() && !line.starts_with('#'))
        .map(|line| serde_json::from_str(line).expect("every sentinel reads"))
        .collect()
}

/// Returns every channel a run is observed through.
fn channels() -> Vec<String> {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(FIXTURES).join("channels.txt");
    std::fs::read_to_string(&path)
        .expect("the channels are committed")
        .lines()
        .filter(|line| !line.trim().is_empty() && !line.starts_with('#'))
        .map(str::to_owned)
        .collect()
}

/// Returns a capture whose every channel holds `contents`.
fn capture_holding(contents: &str) -> ObservableCapture {
    channels()
        .into_iter()
        .fold(ObservableCapture::new(), |held, channel| held.holding(&channel, contents))
}

#[test]
fn every_channel_a_run_has_is_one_this_suite_searches() {
    let mut declared = channels();
    assert!(!declared.is_empty());
    declared.sort();
    let capture = capture_holding("nothing interesting");
    let searched: Vec<String> = capture.channels().into_iter().map(str::to_owned).collect();
    assert_eq!(searched, declared, "a channel exists that nothing here searches");
}

#[test]
fn every_secret_class_is_distinct_and_says_what_it_stands_for() {
    let declared = sentinels();
    assert!(!declared.is_empty());
    let mut values: Vec<&str> = declared.iter().map(|held| held.value.as_str()).collect();
    let held = values.len();
    values.sort_unstable();
    values.dedup();
    assert_eq!(values.len(), held, "two classes share one value, so a find names neither");
    for sentinel in &declared {
        assert!(!sentinel.why.is_empty(), "{} says what it stands for", sentinel.name);
    }
}

#[test]
fn the_scanner_finds_every_secret_in_every_encoding_and_every_channel() {
    for sentinel in sentinels() {
        for (encoding, written) in every_encoding(&sentinel.value) {
            let capture = capture_holding(&format!("a run wrote {written} here"));
            let found = capture.exposures(&sentinel.value);
            for channel in channels() {
                assert!(
                    found.iter().any(
                        |exposure| exposure.channel == channel && exposure.encoding == encoding
                    ),
                    "{} written as {encoding} was missed in {channel}",
                    sentinel.name
                );
            }
        }
    }
}

#[test]
fn a_capture_holding_nothing_secret_reports_nothing() {
    let capture =
        capture_holding("daemon-ping: absent\nslingshot: a target is a profile and an environment");
    for sentinel in sentinels() {
        assert_eq!(
            capture.exposures(&sentinel.value),
            Vec::new(),
            "{} was found where it is not",
            sentinel.name
        );
    }
}

#[test]
fn one_secret_in_one_channel_names_that_channel_and_no_other() {
    let sentinel = sentinels().into_iter().next().expect("a class is declared");
    let capture = channels().into_iter().fold(ObservableCapture::new(), |held, channel| {
        let contents =
            if channel == "diagnostics" { sentinel.value.clone() } else { "clean".to_owned() };
        held.holding(&channel, contents)
    });
    let found = capture.exposures(&sentinel.value);
    let named: std::collections::BTreeSet<&str> =
        found.iter().map(|exposure| exposure.channel.as_str()).collect();
    assert_eq!(
        named,
        std::collections::BTreeSet::from(["diagnostics"]),
        "one channel held it and these were reported"
    );
    assert!(
        found.iter().any(|exposure| exposure.encoding == "raw"),
        "a value written plainly is found plainly, whatever else it also matches"
    );
}
