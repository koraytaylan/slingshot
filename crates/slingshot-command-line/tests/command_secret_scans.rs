//! What this executable writes, searched for what it must never write.
//!
//! A secret that reaches a stream has already escaped: streams are logged,
//! piped into other programs, pasted into issues, and captured by build
//! systems. So every observable channel of a real run is searched for a
//! distinct sentinel of every class, not only in the bytes as they are but in
//! the encodings a value passes through on its way somewhere - a value that
//! leaks base64-encoded has leaked.
//!
//! # Two kinds of sentinel, and one difference between them
//!
//! A secret may appear nowhere at all. A provenance value - which profile,
//! which environment, which file a credential came from, which digest, which
//! source was read first - may appear in an answer a caller asked for by name,
//! and in no diagnostic. That difference is the whole of Plan 0002's rule: a
//! diagnostic that named the file it could not read would enumerate the
//! configuration root for whoever ran the command, and one that preserved
//! discovery order would say which source was read first.
//!
//! # The scanner is proved before it is trusted
//!
//! A scanner that found nothing because it was looking wrongly would pass every
//! scenario here. So each encoding is proved against a helper that deliberately
//! emits it, and a scan that misses one fails.

use std::path::{Path, PathBuf};
use std::time::Duration;

use slingshot_test_support::process_harness::{
    CapturedProcess, ExecutablePath, ProcessHarness, ProcessRequest,
};
use slingshot_test_support::runtime_harness::TemporaryRuntimeRoot;

/// Where the sentinels live.
const FIXTURE_DIRECTORY: &str = "../slingshot-test-support/fixtures/command-secret-scans";

/// The file naming one sentinel per class.
const SENTINEL_SOURCE: &str = "sentinels.jsonl";

/// How long a scan waits for a run that should answer at once.
const PROMPT_DEADLINE: Duration = Duration::from_secs(30);

/// The kind of sentinel that may appear nowhere.
const SECRET: &str = "secret";

/// One sentinel, and where it is allowed to appear.
#[derive(Debug, Clone, serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct Sentinel {
    /// What class of value this stands for.
    name: String,
    /// The distinct value searched for.
    value: String,
    /// Whether it may appear in an answer at all.
    kind: String,
    /// What it stands for, for whoever reads the source rather than the scan.
    why: String,
}

impl Sentinel {
    /// Returns whether this value may appear in an answer a caller asked for.
    fn permitted_in_an_answer(&self) -> bool {
        self.kind != SECRET
    }
}

/// Returns every sentinel this scan searches for.
fn sentinels() -> Vec<Sentinel> {
    let path =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(FIXTURE_DIRECTORY).join(SENTINEL_SOURCE);
    let text = std::fs::read_to_string(&path)
        .unwrap_or_else(|failure| panic!("{} could not be read: {failure}", path.display()));
    text.lines()
        .filter(|line| !line.trim().is_empty() && !line.starts_with('#'))
        .map(|line| serde_json::from_str(line).expect("every sentinel row reads"))
        .collect()
}

// ------------------------------------------------------------- the encodings

/// One named way of writing a value.
type Encoding = (&'static str, fn(&str) -> String);

/// Every way one value may be written on its way somewhere.
const EVERY_ENCODING: &[Encoding] = &[
    ("raw", |value| value.to_owned()),
    ("uppercase", str::to_uppercase),
    ("base64", base64_of),
    ("base64-url", base64_url_of),
    ("hexadecimal", hexadecimal_of),
    ("percent", percent_of),
    ("json-escaped", json_escaped_of),
];

/// The alphabet standard base64 is written in.
const BASE64_ALPHABET: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

/// The alphabet the uniform-resource-locator variant is written in.
const BASE64_URL_ALPHABET: &[u8] =
    b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_";

/// How many input bytes one base64 group holds.
const BASE64_GROUP_BYTES: usize = 3;

/// How many characters one base64 group produces.
const BASE64_GROUP_CHARACTERS: usize = 4;

/// How many bits one base64 character carries.
const BASE64_CHARACTER_BITS: u32 = 6;

/// The mask that keeps one base64 character.
const BASE64_CHARACTER_MASK: u32 = 0b0011_1111;

/// How many bits one byte carries.
const BYTE_BITS: u32 = 8;

/// Returns one value in base64, without padding.
fn base64_of(value: &str) -> String {
    encoded_with(value.as_bytes(), BASE64_ALPHABET)
}

/// Returns one value in the uniform-resource-locator base64 variant.
fn base64_url_of(value: &str) -> String {
    encoded_with(value.as_bytes(), BASE64_URL_ALPHABET)
}

/// Returns one byte string in an alphabet, without padding.
fn encoded_with(bytes: &[u8], alphabet: &[u8]) -> String {
    let mut written = String::new();
    for group in bytes.chunks(BASE64_GROUP_BYTES) {
        let mut held = 0_u32;
        for (position, byte) in group.iter().enumerate() {
            held |= u32::from(*byte)
                << (BYTE_BITS * u32::try_from(BASE64_GROUP_BYTES - 1 - position).unwrap_or(0));
        }
        let characters = group.len() + 1;
        for position in 0..characters.min(BASE64_GROUP_CHARACTERS) {
            let shift = BASE64_CHARACTER_BITS
                * u32::try_from(BASE64_GROUP_CHARACTERS - 1 - position).unwrap_or(0);
            let index = ((held >> shift) & BASE64_CHARACTER_MASK) as usize;
            written.push(char::from(alphabet[index]));
        }
    }
    written
}

/// Returns one value in lowercase hexadecimal.
fn hexadecimal_of(value: &str) -> String {
    value.bytes().map(|byte| format!("{byte:02x}")).collect()
}

/// Returns one value with everything but the unreserved characters escaped.
fn percent_of(value: &str) -> String {
    value
        .bytes()
        .map(|byte| {
            let held = char::from(byte);
            if held.is_ascii_alphanumeric() || "-._~".contains(held) {
                held.to_string()
            } else {
                format!("%{byte:02X}")
            }
        })
        .collect()
}

/// Returns one value as it would be written inside a JavaScript Object Notation string.
fn json_escaped_of(value: &str) -> String {
    let written = serde_json::to_string(value).unwrap_or_default();
    written.trim_matches('"').to_owned()
}

// --------------------------------------------------------------- the scanner

/// One place a sentinel was found, in one encoding.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Exposure {
    /// Which channel it was found in.
    channel: String,
    /// Which sentinel.
    sentinel: String,
    /// Which encoding it was written in.
    encoding: String,
}

/// Returns every sentinel one channel exposes.
fn exposures(channel: &str, text: &str, searched: &[Sentinel]) -> Vec<Exposure> {
    let mut found = Vec::new();
    let uppercase = text.to_uppercase();
    for sentinel in searched {
        for (encoding, encode) in EVERY_ENCODING {
            let written = encode(&sentinel.value);
            let holds = if *encoding == "uppercase" {
                uppercase.contains(&written)
            } else {
                text.contains(&written)
            };
            if holds {
                found.push(Exposure {
                    channel: channel.to_owned(),
                    sentinel: sentinel.name.clone(),
                    encoding: (*encoding).to_owned(),
                });
            }
        }
    }
    found
}

/// Returns every channel one run produced, named.
fn channels<'run>(
    arguments: &'run [String],
    produced: &'run CapturedProcess,
    written: &'run str,
) -> Vec<(String, String)> {
    vec![
        ("arguments".to_owned(), arguments.join(" ")),
        ("standard output".to_owned(), produced.standard_output.clone()),
        ("standard error".to_owned(), produced.standard_error.clone()),
        ("files".to_owned(), written.to_owned()),
    ]
}

/// Returns everything one run left under a root, as one searchable text.
fn everything_written(root: &Path) -> String {
    let mut written = String::new();
    let mut pending = vec![root.to_path_buf()];
    while let Some(directory) = pending.pop() {
        let Ok(entries) = std::fs::read_dir(&directory) else {
            continue;
        };
        for entry in entries.filter_map(Result::ok) {
            let path = entry.path();
            written.push_str(&path.to_string_lossy());
            written.push('\n');
            if path.is_dir() {
                pending.push(path);
                continue;
            }
            if let Ok(held) = std::fs::read(&path) {
                written.push_str(&String::from_utf8_lossy(&held));
                written.push('\n');
            }
        }
    }
    written
}

/// Runs one argument vector and returns what every channel of it holds.
fn scanned(root: &Path, arguments: &[String]) -> Vec<Exposure> {
    let mut named = vec!["--runtime-root".to_owned(), root.to_string_lossy().into_owned()];
    named.extend(arguments.iter().cloned());
    let words: Vec<&str> = named.iter().map(String::as_str).collect();
    let harness = ProcessHarness::new();
    let produced = harness
        .run_within(&product_executable(), &ProcessRequest::new(&words), PROMPT_DEADLINE)
        .expect("the product executable runs");
    let written = everything_written(root);
    let searched = sentinels();
    let mut found = Vec::new();
    for (channel, text) in channels(&named, &produced, &written) {
        let permitted = channel == "arguments";
        for exposure in exposures(&channel, &text, &searched) {
            let sentinel = searched
                .iter()
                .find(|held| held.name == exposure.sentinel)
                .expect("the exposure names a sentinel");
            let answered = channel == "standard output" && sentinel.permitted_in_an_answer();
            if permitted || answered {
                continue;
            }
            found.push(exposure);
        }
    }
    found
}

/// Returns the product executable these scans drive.
fn product_executable() -> ExecutablePath {
    ExecutablePath::new(PathBuf::from(env!("CARGO_BIN_EXE_slingshot")))
        .expect("the product executable was built")
}

/// Returns the sentinel value of one class.
fn sentinel_value(name: &str) -> String {
    sentinels()
        .into_iter()
        .find(|held| held.name == name)
        .unwrap_or_else(|| panic!("{name} is a class the fixture declares"))
        .value
}

/// Returns one boundary's argument vector, with sentinels where values flow.
fn boundary(name: &str) -> Vec<String> {
    let profile = sentinel_value("profile-name");
    let environment = sentinel_value("environment-name");
    let addressed = vec!["--profile".to_owned(), profile, "--environment".to_owned(), environment];
    let with = |extra: Vec<&str>| {
        let mut named = addressed.clone();
        named.extend(extra.into_iter().map(str::to_owned));
        named
    };
    match name {
        "parsing" => with(vec!["daemon", "ping", "--surprise", &sentinel_value("password")]),
        "configuration-check" => with(vec!["check-configuration"]),
        "daemon-start" => with(vec!["daemon", "status"]),
        "authentication" => with(vec![
            "load_content_as_json",
            "--path",
            "/content/site",
            "--operation-key",
            &sentinel_value("client-secret"),
        ]),
        "submission" => with(vec![
            "replicate_content",
            "--path",
            "/content/site",
            "--operation-key",
            &sentinel_value("bearer-token"),
        ]),
        "observation" => {
            with(vec!["operation-status", "--operation", &sentinel_value("private-key")])
        }
        "result" => with(vec!["operation-result", "--operation", &sentinel_value("password")]),
        "artifact" => with(vec![
            "operation-artifact",
            "--operation",
            &sentinel_value("password"),
            "--artifact",
            &sentinel_value("bearer-token"),
            "--expected-digest",
            &sentinel_value("source-digest"),
        ]),
        "maintenance-result" => with(vec![
            "maintenance-result",
            "--result-identifier",
            &sentinel_value("readiness-nonce"),
        ]),
        _ => with(vec!["operation-list", "--limit", "1"]),
    }
}

/// Every boundary a failure may be produced at.
const EVERY_BOUNDARY: &[&str] = &[
    "parsing",
    "configuration-check",
    "daemon-start",
    "authentication",
    "submission",
    "observation",
    "result",
    "artifact",
    "maintenance-result",
    "interrupt",
];

#[test]
fn no_boundary_publishes_a_sentinel_in_any_encoding() {
    for boundary_name in EVERY_BOUNDARY {
        let root = TemporaryRuntimeRoot::create("n").expect("the temporary root is created");
        let found = scanned(root.path(), &boundary(boundary_name));
        assert_eq!(found, Vec::new(), "{boundary_name} published a sentinel");
    }
}

#[test]
fn every_declared_class_is_distinct_and_says_what_it_stands_for() {
    let declared = sentinels();
    assert!(!declared.is_empty(), "the fixture declares at least one class");
    let mut values: Vec<String> = declared.iter().map(|held| held.value.clone()).collect();
    let held = values.len();
    values.sort();
    values.dedup();
    assert_eq!(values.len(), held, "two classes share one value, so a find names neither");
    for sentinel in &declared {
        assert!(!sentinel.why.is_empty(), "{} says what it stands for", sentinel.name);
        assert!(sentinel.kind == SECRET || sentinel.permitted_in_an_answer());
    }
}

#[test]
fn the_scanner_finds_every_encoding_a_helper_deliberately_emits() {
    let searched = sentinels();
    for sentinel in &searched {
        for (encoding, encode) in EVERY_ENCODING {
            let emitted = format!("a helper wrote {} here", encode(&sentinel.value));
            let found = exposures("positive control", &emitted, &searched);
            assert!(
                found
                    .iter()
                    .any(|exposure| exposure.sentinel == sentinel.name
                        && exposure.encoding == *encoding),
                "the scanner missed {} written as {encoding}",
                sentinel.name
            );
        }
    }
}

#[test]
fn the_scanner_finds_nothing_in_text_that_holds_nothing() {
    let searched = sentinels();
    let innocent = "daemon-ping: absent\nslingshot: a target is a profile and an environment";
    assert_eq!(exposures("innocent", innocent, &searched), Vec::new());
}

#[test]
fn every_boundary_actually_carries_a_sentinel_into_the_process() {
    let searched = sentinels();
    for boundary_name in EVERY_BOUNDARY {
        let arguments = boundary(boundary_name).join(" ");
        let carried = searched.iter().filter(|held| arguments.contains(&held.value)).count();
        assert!(carried > 0, "{boundary_name} carries no sentinel, so its scan proves nothing");
        let root = TemporaryRuntimeRoot::create("l").expect("the temporary root is created");
        let mut named =
            vec!["--runtime-root".to_owned(), root.path().to_string_lossy().into_owned()];
        named.extend(boundary(boundary_name));
        let words: Vec<&str> = named.iter().map(String::as_str).collect();
        let harness = ProcessHarness::new();
        let produced = harness
            .run_within(&product_executable(), &ProcessRequest::new(&words), PROMPT_DEADLINE)
            .expect("the product executable runs");
        assert!(
            !produced.standard_output.is_empty() || !produced.standard_error.is_empty(),
            "{boundary_name} wrote nothing at all, so its scan searched nothing"
        );
    }
}

/// How many distinct diagnostics a public report keeps before it truncates.
const INCLUSIVE_DIAGNOSTIC_BOUND: u64 = 32;

#[test]
fn a_public_configuration_report_is_bounded_inclusively_and_says_so() {
    use slingshot_configuration::profile_loader::{
        ConfigurationDiagnostic, DiagnosticSourceClass, DiagnosticStage, summarize,
    };
    use slingshot_domain::profile_authentication_contract::{
        ConfigurationFailureCode, ProfileAuthenticationContract,
    };

    let limits = &ProfileAuthenticationContract::embedded().limits;
    assert_eq!(limits.maximum_configuration_diagnostics, INCLUSIVE_DIAGNOSTIC_BOUND);

    let ordinary: Vec<ConfigurationFailureCode> = ConfigurationFailureCode::REGISTRY
        .iter()
        .copied()
        .filter(|code| *code != ConfigurationFailureCode::ConfigurationDiagnosticsTruncated)
        .collect();
    let made = |index: u64| ConfigurationDiagnostic {
        source_class: DiagnosticSourceClass::Profile,
        stage: DiagnosticStage::DocumentShape,
        structural_location: EVERY_LOCATION[index as usize % EVERY_LOCATION.len()],
        code: ordinary[index as usize % ordinary.len()],
        occurrences: 1,
    };
    let exactly: Vec<ConfigurationDiagnostic> = (0..INCLUSIVE_DIAGNOSTIC_BOUND).map(made).collect();
    let held = summarize(exactly);
    assert_eq!(u64::try_from(held.len()).unwrap_or_default(), INCLUSIVE_DIAGNOSTIC_BOUND);
    let markers: Vec<&ConfigurationDiagnostic> = held
        .iter()
        .filter(|diagnostic| {
            diagnostic.code == ConfigurationFailureCode::ConfigurationDiagnosticsTruncated
        })
        .collect();
    assert_eq!(markers, Vec::<&ConfigurationDiagnostic>::new(), "the bound is inclusive");

    let one_more: Vec<ConfigurationDiagnostic> =
        (0..=INCLUSIVE_DIAGNOSTIC_BOUND).map(made).collect();
    let truncated = summarize(one_more);
    let marker = truncated.last().expect("a truncated report ends with its marker");
    assert_eq!(marker.code, ConfigurationFailureCode::ConfigurationDiagnosticsTruncated);
    assert_eq!(marker.structural_location, "diagnostics");
    for diagnostic in &truncated {
        assert!(!diagnostic.structural_location.contains('/'), "a location names no path");
    }
}

/// Structural locations a made-up diagnostic is placed at.
const EVERY_LOCATION: &[&str] =
    &["profile", "profile.environment", "selection", "credentials", "trust"];
