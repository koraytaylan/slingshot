//! Assertions for the retained control surface and its canonical limit source.
//!
//! Every accepted fixture is hand-authored and must decode and re-encode
//! byte-for-byte, every refused fixture must produce its exact structured
//! error, and every declared bound must accept the value at the limit and
//! refuse the adjacent one. A separate inventory assertion proves that no other
//! repository file carries a second copy of a contract value.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde::Deserialize;
use slingshot_local_protocol::envelope::{
    self, ControlRequest, ControlResponse, LIMIT_EXCEEDED_CODE, MALFORMED_REQUEST_CODE,
    STALE_DAEMON_INSTANCE_CODE, UNSUPPORTED_CONTROL_VERSION_CODE,
};
use slingshot_local_protocol::foundation_contract::{ContractFailure, FoundationContract};
use slingshot_local_protocol::framing::{self, FrameProgress, FramingFailure};
use slingshot_local_protocol::ping::{
    self, PING_METHOD, PingResult, STOP_METHOD, StopArguments, StopResult,
};

/// Directory holding the hand-authored fixtures.
const FIXTURE_DIRECTORY: &str = "tests/fixtures/minimal-protocol";

/// Repository path of the committed contract manifest.
const MANIFEST_PATH: &str = "support/foundation-contract.toml";

/// Fixture holding the byte-level framing cases.
const FRAMING_FIXTURE: &str = "framing-cases.toml";

/// Format identifier the framing fixture must declare.
const FRAMING_FORMAT: &str = "slingshot.minimal-protocol-frames/1";

/// Repository directories whose Rust must not repeat a contract value.
const CONTRACT_CONSUMER_DIRECTORIES: &[&str] = &[
    "crates/slingshot-local-protocol",
    "crates/slingshot-daemon",
    "crates/slingshot-command-line",
    "crates/slingshot-test-support",
];

/// Files that hold or assert the contract values and are therefore excluded.
const CONTRACT_OWNING_FILES: &[&str] = &[
    "crates/slingshot-local-protocol/src/foundation_contract.rs",
    "crates/slingshot-local-protocol/tests/minimal_protocol.rs",
];

/// Smallest contract value a repeated literal is reported for.
const REPEATED_VALUE_FLOOR: u64 = 2;

/// The byte-level framing cases.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "kebab-case", deny_unknown_fields)]
struct FramingCases {
    /// Format identifier of the fixture.
    format: String,
    /// One entry per buffer a reader may receive.
    case: Vec<FramingCase>,
}

/// One buffer a reader may receive and the progress it must report.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "kebab-case", deny_unknown_fields)]
struct FramingCase {
    /// Name the fixture gives the case.
    name: String,
    /// The literal buffer, in lowercase hexadecimal.
    bytes: String,
    /// Progress the reader must report.
    expected: String,
    /// Bytes already received, where the progress reports one.
    #[serde(default)]
    received: Option<usize>,
    /// Bytes the prefix declared, where the progress reports one.
    #[serde(default)]
    declared: Option<usize>,
}

/// Returns the workspace root directory.
fn workspace_root() -> PathBuf {
    let crate_root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    crate_root
        .ancestors()
        .find(|directory| directory.join(MANIFEST_PATH).is_file())
        .expect("the crate lives inside the workspace")
        .to_path_buf()
}

/// Reads one fixture as bytes.
fn fixture_bytes(name: &str) -> Vec<u8> {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(FIXTURE_DIRECTORY).join(name);
    std::fs::read(&path)
        .unwrap_or_else(|failure| panic!("{} could not be read: {failure}", path.display()))
}

/// Reads one fixture as text.
fn fixture_text(name: &str) -> String {
    String::from_utf8(fixture_bytes(name)).expect("the fixture is text")
}

/// Reads the byte-level framing cases.
fn framing_cases() -> FramingCases {
    toml::from_str(&fixture_text(FRAMING_FIXTURE)).expect("the framing fixture is a valid document")
}

/// Reads a lowercase hexadecimal buffer out of a framing case.
fn case_bytes(rendered: &str) -> Vec<u8> {
    assert_eq!(rendered.len() % 2, 0, "a buffer is written as whole bytes");
    rendered
        .as_bytes()
        .chunks(2)
        .map(|pair| {
            let text = std::str::from_utf8(pair).expect("the buffer is text");
            u8::from_str_radix(text, 16).expect("the buffer is hexadecimal")
        })
        .collect()
}

/// Decodes one request fixture, expecting it to be admitted.
fn admitted(contract: &FoundationContract, name: &str) -> ControlRequest {
    let framed =
        framing::render(&contract.framing, &fixture_bytes(name)).expect("the fixture frames");
    let payload = &framed[contract.framing.length_prefix_bytes as usize..];
    envelope::decode_request(contract, payload)
        .unwrap_or_else(|refused| panic!("{name} must be admitted, but was refused: {refused:?}"))
}

/// Decodes one request fixture, expecting the named refusal.
fn refused_with(contract: &FoundationContract, name: &str, code: &str) {
    let payload = fixture_bytes(name);
    let refused =
        envelope::decode_request(contract, &payload).expect_err(&format!("{name} must be refused"));
    assert_eq!(refused.error.code, code, "{name} reported {refused:?}");
}

#[test]
fn the_committed_manifest_and_the_embedded_bytes_are_the_same() {
    let committed = std::fs::read_to_string(workspace_root().join(MANIFEST_PATH))
        .expect("the committed manifest reads");
    assert_eq!(committed, FoundationContract::embedded_manifest(), "the embedded bytes drifted");
    let parsed = FoundationContract::parse(&committed).expect("the committed manifest is valid");
    assert_eq!(parsed, FoundationContract::embedded());
}

#[test]
fn the_manifest_schema_refuses_every_recorded_defect() {
    let committed = FoundationContract::embedded_manifest();
    let missing = committed.replace("connection-capacity = 64\n", "");
    assert!(matches!(FoundationContract::parse(&missing), Err(ContractFailure::Unreadable(_))));
    let additional = format!("{committed}\n[surprise]\nvalue = 1\n");
    assert!(matches!(FoundationContract::parse(&additional), Err(ContractFailure::Unreadable(_))));
    let duplicated = committed.replace("[server]", "[server]\nconnection-capacity = 64");
    assert!(matches!(FoundationContract::parse(&duplicated), Err(ContractFailure::Unreadable(_))));
    let negative = committed.replace("connection-capacity = 64", "connection-capacity = -64");
    assert!(matches!(FoundationContract::parse(&negative), Err(ContractFailure::Unreadable(_))));
    let overflowed =
        committed.replace("connection-capacity = 64", "connection-capacity = 4_294_967_296");
    assert!(matches!(FoundationContract::parse(&overflowed), Err(ContractFailure::Unreadable(_))));
    let text_encoded =
        committed.replace("connection-capacity = 64", "connection-capacity = \"64\"");
    assert!(matches!(
        FoundationContract::parse(&text_encoded),
        Err(ContractFailure::Unreadable(_))
    ));
    let zeroed = committed.replace("connection-capacity = 64", "connection-capacity = 0");
    assert_eq!(
        FoundationContract::parse(&zeroed),
        Err(ContractFailure::NotPositive("server.connection-capacity"))
    );
    let mismatched = committed.replace("digest-rendered-bytes = 64", "digest-rendered-bytes = 63");
    assert!(matches!(
        FoundationContract::parse(&mismatched),
        Err(ContractFailure::RenderedLengthMismatch { .. })
    ));
    let other_format =
        committed.replace("slingshot.foundation-contract/1", "slingshot.foundation-contract/2");
    assert!(matches!(
        FoundationContract::parse(&other_format),
        Err(ContractFailure::UnsupportedFormat(_))
    ));
}

#[test]
fn every_canonical_fixture_decodes_and_re_encodes_byte_for_byte() {
    let contract = FoundationContract::embedded();
    let request = admitted(&contract, "ping-request.json");
    assert_eq!(request.method, PING_METHOD);
    assert_eq!(
        serde_json::to_vec(&request).expect("the request renders"),
        fixture_bytes("ping-request.json")
    );

    let stop = admitted(&contract, "stop-request.json");
    assert_eq!(stop.method, STOP_METHOD);
    let arguments: StopArguments =
        serde_json::from_value(stop.arguments.clone()).expect("the stop arguments read");
    assert!(ping::nonce_is_well_formed(&contract, &arguments.readiness_nonce));
    assert_eq!(
        serde_json::to_vec(&stop).expect("the request renders"),
        fixture_bytes("stop-request.json")
    );

    for name in [
        "ping-success-response.json",
        "stop-success-response.json",
        "stale-stop-failure-response.json",
    ] {
        let text = fixture_text(name);
        let response: ControlResponse = serde_json::from_str(&text).expect("the response reads");
        assert_eq!(serde_json::to_string(&response).expect("the response renders"), text, "{name}");
        assert_eq!(response.control_version, contract.control.version);
    }
}

#[test]
fn a_ping_reports_the_live_owner_and_a_stop_accepts_only_the_live_nonce() {
    let contract = FoundationContract::embedded();
    let response: ControlResponse =
        serde_json::from_str(&fixture_text("ping-success-response.json"))
            .expect("the response reads");
    let result: PingResult =
        serde_json::from_value(response.result.clone().expect("the response carries a result"))
            .expect("the ping result reads");
    assert!(ping::nonce_is_well_formed(&contract, &result.readiness_nonce));
    assert!(
        result.supported_operation_protocol_versions.is_empty(),
        "no operation protocol is served yet"
    );

    let live = result.readiness_nonce.clone();
    let stale = format!("b{}", &live[1..]);
    assert!(ping::stop_is_authorized(&live, &live));
    assert!(!ping::stop_is_authorized(&live, &stale), "a stale nonce is refused");
    assert!(!ping::stop_is_authorized(&live, ""), "an empty nonce is refused");

    let refusal = ping::stale_instance_refusal();
    assert_eq!(refusal.code, STALE_DAEMON_INSTANCE_CODE);
    let refused = ControlResponse::refused(&contract, "two", refusal);
    assert_eq!(
        serde_json::to_string(&refused).expect("the refusal renders"),
        fixture_text("stale-stop-failure-response.json")
    );

    let acknowledged =
        ControlResponse::served(&contract, "two", &StopResult { acknowledged: true })
            .expect("the acknowledgement renders");
    assert_eq!(
        serde_json::to_string(&acknowledged).expect("the acknowledgement renders"),
        fixture_text("stop-success-response.json")
    );
}

#[test]
fn an_unknown_control_version_is_refused_before_any_method_is_read() {
    let contract = FoundationContract::embedded();
    let payload = fixture_bytes("version-mismatch-request.json");
    let refused =
        envelope::decode_request(&contract, &payload).expect_err("the version is refused");
    assert_eq!(refused.error.code, UNSUPPORTED_CONTROL_VERSION_CODE);
    assert_eq!(refused.request_identifier.as_deref(), Some("three"));
    assert!(!refused.error.message.contains(PING_METHOD), "no method was dispatched");

    let unknown = admitted(&contract, "unknown-method-request.json");
    assert_ne!(unknown.method, PING_METHOD, "an unknown method reaches dispatch, not decoding");
    assert_ne!(unknown.method, STOP_METHOD);
}

#[test]
fn every_malformed_or_oversized_payload_reports_its_own_bounded_error() {
    let contract = FoundationContract::embedded();
    for name in [
        "malformed-request.json",
        "duplicate-field-request.json",
        "trailing-data-request.json",
        "unknown-field-request.json",
        "invalid-text-request.bin",
    ] {
        refused_with(&contract, name, MALFORMED_REQUEST_CODE);
    }
    for name in [
        "beyond-limit-request-identifier.json",
        "beyond-limit-method.json",
        "beyond-limit-nesting-request.json",
        "beyond-limit-collection-request.json",
    ] {
        refused_with(&contract, name, LIMIT_EXCEEDED_CODE);
    }
}

#[test]
fn a_payload_at_each_declared_limit_is_admitted() {
    let contract = FoundationContract::embedded();
    let identifier = admitted(&contract, "at-limit-request-identifier.json");
    assert_eq!(
        identifier.request_identifier.len(),
        contract.names.request_identifier_bytes as usize
    );
    admitted(&contract, "at-limit-nesting-request.json");
    let collection = admitted(&contract, "at-limit-collection-request.json");
    assert_eq!(
        collection.arguments.as_array().expect("the arguments are a list").len(),
        contract.framing.maximum_collection_items as usize
    );

    let limit = contract.framing.maximum_payload_bytes as usize;
    let at_limit = vec![b' '; limit];
    assert!(framing::render(&contract.framing, &at_limit).is_ok(), "a payload at the limit frames");
    let beyond = vec![b' '; limit + 1];
    assert!(matches!(
        framing::render(&contract.framing, &beyond),
        Err(FramingFailure::PayloadTooLarge { .. })
    ));
}

#[test]
fn every_framing_case_reports_its_recorded_progress() {
    let contract = FoundationContract::embedded();
    let cases = framing_cases();
    assert_eq!(cases.format, FRAMING_FORMAT);
    for case in &cases.case {
        let buffer = case_bytes(&case.bytes);
        let observed = framing::progress(&contract.framing, &buffer);
        match case.expected.as_str() {
            "empty" => assert_eq!(observed, Ok(FrameProgress::Empty), "{}", case.name),
            "partial-prefix" => assert_eq!(
                observed,
                Ok(FrameProgress::PartialPrefix { received: case.received.expect("received") }),
                "{}",
                case.name
            ),
            "partial-payload" => assert_eq!(
                observed,
                Ok(FrameProgress::PartialPayload {
                    received: case.received.expect("received"),
                    declared: case.declared.expect("declared"),
                }),
                "{}",
                case.name
            ),
            "complete" => assert_eq!(
                observed,
                Ok(FrameProgress::Complete { declared: case.declared.expect("declared") }),
                "{}",
                case.name
            ),
            "payload-too-large" => assert!(
                matches!(observed, Err(FramingFailure::PayloadTooLarge { .. })),
                "{}: {observed:?}",
                case.name
            ),
            other => panic!("{} names the unknown progress {other}", case.name),
        }
    }
}

#[test]
fn arbitrary_bytes_never_panic_and_never_allocate_beyond_the_frame_limit() {
    let contract = FoundationContract::embedded();
    let limit = contract.framing.maximum_payload_bytes as usize;
    proptest::proptest!(|(buffer: Vec<u8>)| {
        let observed = framing::progress(&contract.framing, &buffer);
        if let Ok(FrameProgress::Complete { declared }) = observed {
            proptest::prop_assert!(declared <= limit);
            let start = contract.framing.length_prefix_bytes as usize;
            let payload = &buffer[start..start + declared];
            let _outcome = framing::read_payload(&contract.framing, payload);
            let _decoded = envelope::decode_request(&contract, payload);
        }
    });
}

#[test]
fn no_other_repository_source_repeats_a_contract_value() {
    let contract = FoundationContract::embedded();
    let mut values: BTreeMap<u64, &str> = BTreeMap::new();
    let manifest = FoundationContract::embedded_manifest();
    for line in manifest.lines() {
        let Some((name, value)) = line.split_once(" = ") else { continue };
        let Ok(number) = value.replace('_', "").parse::<u64>() else { continue };
        if number >= REPEATED_VALUE_FLOOR {
            values
                .entry(number)
                .or_insert_with(|| Box::leak(name.trim().to_owned().into_boxed_str()));
        }
    }
    assert!(values.contains_key(&u64::from(contract.server.connection_capacity)));
    let root = workspace_root();
    let mut repeated = Vec::new();
    for directory in CONTRACT_CONSUMER_DIRECTORIES {
        for path in rust_sources(&root.join(directory)) {
            let relative = path
                .strip_prefix(&root)
                .expect("the source is inside the workspace")
                .to_str()
                .expect("the path is text")
                .to_owned();
            if CONTRACT_OWNING_FILES.contains(&relative.as_str()) {
                continue;
            }
            let source = std::fs::read_to_string(&path).expect("the source reads");
            repeated.extend(repeated_values(&relative, &source, &values));
        }
    }
    assert_eq!(repeated, Vec::<String>::new());
}

/// Returns every Rust source file under one directory.
fn rust_sources(directory: &Path) -> Vec<PathBuf> {
    let mut found = Vec::new();
    let mut pending = vec![directory.to_path_buf()];
    while let Some(current) = pending.pop() {
        let Ok(entries) = std::fs::read_dir(&current) else { continue };
        for entry in entries {
            let path = entry.expect("the directory entry is readable").path();
            if path.is_dir() {
                pending.push(path);
            } else if path.extension().is_some_and(|extension| extension == "rs") {
                found.push(path);
            }
        }
    }
    found.sort();
    found
}

/// Reports whether a character may sit beside a numeric literal.
fn is_identifier_byte(character: char) -> bool {
    character.is_ascii_alphabetic() || character == '_'
}

/// Returns every standalone numeric literal one line declares.
///
/// A digit run touching a letter belongs to a type name such as `u32` or to an
/// identifier, not to a value, so it is not a repeated limit.
fn numeric_literals(line: &str) -> Vec<u64> {
    let code = line.split_once("//").map_or(line, |(before, _)| before);
    let characters: Vec<char> = code.chars().collect();
    let mut found = Vec::new();
    let mut start = 0_usize;
    while start < characters.len() {
        if !characters[start].is_ascii_digit() {
            start += 1;
            continue;
        }
        let mut end = start;
        while end < characters.len() && (characters[end].is_ascii_digit() || characters[end] == '_')
        {
            end += 1;
        }
        let before_is_identifier = start > 0 && is_identifier_byte(characters[start - 1]);
        let after_is_identifier = end < characters.len() && is_identifier_byte(characters[end]);
        if !before_is_identifier && !after_is_identifier {
            let literal: String =
                characters[start..end].iter().filter(|value| **value != '_').collect();
            if let Ok(number) = literal.parse::<u64>() {
                found.push(number);
            }
        }
        start = end;
    }
    found
}

/// Reports every contract value one source file repeats as a literal.
fn repeated_values(relative: &str, source: &str, values: &BTreeMap<u64, &str>) -> Vec<String> {
    let mut reported = Vec::new();
    for (line_number, line) in source.lines().enumerate() {
        for number in numeric_literals(line) {
            if let Some(field) = values.get(&number) {
                reported.push(format!(
                    "{relative}:{} repeats the contract value {number} of {field}",
                    line_number + 1
                ));
            }
        }
    }
    reported
}
