//! The one authority, and the arithmetic anyone can check.
//!
//! Every formula in the manifest is recomputed here from its primitive
//! operands, by vectors authored outside the code under test. That matters more
//! than it sounds: the write-ahead-log maximum is *not* frames times page size -
//! it is one file header plus frames times a frame, and a reader who assumed
//! otherwise would be wrong by four hundred kilobytes and would never notice,
//! because the wrong answer is also a plausible number.
//!
//! The maintenance-result identifier gets the same treatment. Its ninety-seven
//! octets are counted and read back rather than trusted, and the test proves no
//! operation identifier, command name, or artifact slot is among them - which is
//! the whole reason the type exists.

use serde_json::Value;
use slingshot_domain::daemon_runtime_contract::{
    DAEMON_OPERATION_PROTOCOL_VERSION, DAEMON_RUNTIME_CONTRACT_FORMAT, DIGEST_CHARACTERS,
    DIGEST_OCTETS, DaemonRuntimeContract, DaemonRuntimeContractFailure, MAINTENANCE_RESULT_DOMAIN,
    MAINTENANCE_RESULT_PREIMAGE_OCTETS, MaintenanceResultIdentifier,
    MaintenanceResultIdentifierFailure, MaintenanceResultKind,
};

/// Equation vectors this test reads.
const EQUATIONS: &str =
    include_str!("../../slingshot-test-support/fixtures/daemon-runtime-contract/equations.jsonl");

/// Identifier vectors this test reads.
const IDENTIFIERS: &str =
    include_str!("../../slingshot-test-support/fixtures/daemon-runtime-contract/identifiers.jsonl");

/// Rendering vectors this test reads.
const RENDERINGS: &str =
    include_str!("../../slingshot-test-support/fixtures/daemon-runtime-contract/renderings.jsonl");

/// Digest vectors this test reads.
const DIGESTS: &str =
    include_str!("../../slingshot-test-support/fixtures/daemon-runtime-contract/digests.jsonl");

/// Radix a digest is rendered in.
const HEXADECIMAL_RADIX: u32 = 16;

/// Octets one vector fills its target with.
const VECTOR_TARGET_OCTET: u8 = 7;

/// Octets one vector fills its reviewed manifest with.
const VECTOR_MANIFEST_OCTET: u8 = 8;

/// Octets one vector fills its content digest with.
const VECTOR_CONTENT_OCTET: u8 = 9;

/// Reads one row's string member.
fn text<'row>(row: &'row Value, member: &str) -> &'row str {
    row[member].as_str().unwrap_or_else(|| panic!("{member} is a string in {row}"))
}

/// Returns every row of one fixture.
fn rows(fixture: &str) -> Vec<Value> {
    fixture
        .lines()
        .map(|line| serde_json::from_str(line).expect("every fixture line is one object"))
        .collect()
}

/// Returns the thirty-two octets one hexadecimal member spells.
fn octets(row: &Value, member: &str) -> [u8; DIGEST_OCTETS] {
    let spelling = text(row, member);
    let mut held = [0_u8; DIGEST_OCTETS];
    for (position, octet) in held.iter_mut().enumerate() {
        let pair = &spelling[position * 2..position * 2 + 2];
        *octet = u8::from_str_radix(pair, HEXADECIMAL_RADIX).expect("a hexadecimal pair");
    }
    held
}

#[test]
fn the_committed_bytes_the_embedded_bytes_and_the_sidecar_agree() {
    let repository = std::fs::read_to_string(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../policy/daemon-runtime-contract-1.json"
    ))
    .expect("the committed manifest reads");
    assert_eq!(repository, DaemonRuntimeContract::embedded_manifest());
    let contract = DaemonRuntimeContract::embedded();
    assert_eq!(contract.render().expect("it renders"), repository, "it is not canonical");
    assert_eq!(contract.format, DAEMON_RUNTIME_CONTRACT_FORMAT);
    assert_eq!(contract.operation_protocol_version, DAEMON_OPERATION_PROTOCOL_VERSION);

    let digest = DaemonRuntimeContract::embedded_digest();
    let recorded = rows(DIGESTS);
    assert_eq!(digest.as_text(), text(&recorded[0], "sha256"), "the sidecar and the vector agree");
    assert_eq!(
        recorded[0]["bytes"].as_u64(),
        Some(repository.len() as u64),
        "the vector counted the bytes it hashed"
    );
    assert_ne!(
        text(&recorded[0], "sha256"),
        text(&recorded[1], "sha256"),
        "one changed value is a different digest"
    );
}

#[test]
fn every_formula_is_recomputed_from_its_operands_rather_than_believed() {
    let contract = DaemonRuntimeContract::embedded();
    let vectors = rows(EQUATIONS);
    assert!(vectors.len() >= 10, "every formula the manifest records");
    for row in &vectors {
        let name = text(row, "formula");
        assert_eq!(
            contract.formula(name),
            row["value"].as_u64().expect("a vector value"),
            "{name}: {}",
            text(row, "note")
        );
        assert!(
            row["operands"].as_array().is_some_and(|operands| operands.len() >= 2),
            "{name}: a formula vector names the operands it multiplied"
        );
    }
    assert_eq!(contract.require_consistent_formulas(), Ok(()));
}

#[test]
fn the_log_maximum_is_not_pages_times_page_size() {
    let contract = DaemonRuntimeContract::embedded();
    let page = contract.limit("sqlite_page_bytes");
    let frames = contract.limit("maximum_sqlite_write_ahead_log_frames");
    let naive = frames * page;
    let actual = contract.formula("maximum_sqlite_write_ahead_log_bytes");
    assert_ne!(actual, naive, "the plausible wrong answer and the right one are different numbers");
    assert_eq!(
        actual,
        contract.limit("sqlite_write_ahead_log_header_bytes")
            + frames * (contract.limit("sqlite_write_ahead_log_frame_header_bytes") + page),
        "one file header plus frames times one frame"
    );
    assert!(actual > naive, "and the wrong one is smaller, which is the dangerous direction");
}

#[test]
fn a_formula_that_disagrees_with_its_operands_is_refused() {
    let manifest = DaemonRuntimeContract::embedded_manifest();
    let mutated = manifest.replace(
        "\"maximum_sqlite_physical_bytes\":1141506080",
        "\"maximum_sqlite_physical_bytes\":1141506081",
    );
    assert_ne!(mutated, manifest, "the vector changed something");
    assert_eq!(
        DaemonRuntimeContract::parse(&mutated),
        Err(DaemonRuntimeContractFailure::FormulaInconsistent {
            name: "maximum_sqlite_physical_bytes".to_owned()
        })
    );
}

#[test]
fn a_noncanonical_or_wrongly_declared_manifest_is_refused() {
    let manifest = DaemonRuntimeContract::embedded_manifest();
    let refusals: &[(&str, String, DaemonRuntimeContractFailure)] = &[
        (
            "an unknown member",
            manifest.replace("{\"format\"", "{\"deployment\":true,\"format\""),
            DaemonRuntimeContractFailure::Unreadable(String::new()),
        ),
        (
            "another format",
            manifest.replace(DAEMON_RUNTIME_CONTRACT_FORMAT, "slingshot.daemon-runtime-contract/2"),
            DaemonRuntimeContractFailure::UnsupportedFormat(String::new()),
        ),
        (
            "another operation protocol version",
            manifest
                .replace("\"operation_protocol_version\":1", "\"operation_protocol_version\":2"),
            DaemonRuntimeContractFailure::UnsupportedVersion(0),
        ),
        (
            "insignificant whitespace",
            manifest.replace("{\"format\"", "{ \"format\""),
            DaemonRuntimeContractFailure::NotCanonical,
        ),
    ];
    for (note, mutated, expected) in refusals {
        assert_ne!(mutated.as_str(), manifest, "{note}: the vector changed nothing");
        let outcome = DaemonRuntimeContract::parse(mutated);
        assert_eq!(
            outcome.as_ref().err().map(kind_of),
            Some(kind_of(expected)),
            "{note}: refused as {outcome:?}"
        );
    }
}

/// Returns which refusal one failure is, ignoring what it carries.
///
/// The messages quote the offending value, which is right for a reader and
/// wrong for a comparison, so the vectors name the kind and this reduces a
/// failure to it.
fn kind_of(failure: &DaemonRuntimeContractFailure) -> &'static str {
    match failure {
        DaemonRuntimeContractFailure::Unreadable(_) => "unreadable",
        DaemonRuntimeContractFailure::UnsupportedFormat(_) => "unsupported format",
        DaemonRuntimeContractFailure::UnsupportedVersion(_) => "unsupported version",
        DaemonRuntimeContractFailure::NotCanonical => "not canonical",
        DaemonRuntimeContractFailure::FormulaInconsistent { .. } => "formula inconsistent",
        DaemonRuntimeContractFailure::DigestMismatch => "digest mismatch",
    }
}

#[test]
fn every_identifier_vector_derives_its_own_recorded_value() {
    let vectors = rows(IDENTIFIERS);
    assert!(vectors.len() >= 7, "both kinds, three one-bit changes, and a field swap");
    let mut derived = std::collections::BTreeSet::new();
    for row in &vectors {
        let kind = match text(row, "kind") {
            "preview" => MaintenanceResultKind::Preview,
            other => {
                assert_eq!(other, "application", "a kind this contract has");
                MaintenanceResultKind::Application
            }
        };
        let identifier = MaintenanceResultIdentifier::derive(
            &octets(row, "target"),
            kind,
            &octets(row, "reviewed_manifest"),
            &octets(row, "content"),
        );
        assert_eq!(identifier.as_text(), text(row, "identifier"), "{}", text(row, "note"));
        derived.insert(identifier.as_text().to_owned());
        assert_eq!(
            row["preimage_octets"].as_u64(),
            Some(MAINTENANCE_RESULT_PREIMAGE_OCTETS as u64),
            "{}: the vector counted the octets it hashed",
            text(row, "note")
        );
    }
    assert_eq!(derived.len(), vectors.len(), "no two vectors share an identifier");
}

#[test]
fn the_two_kinds_of_one_result_are_two_identifiers() {
    let target = [VECTOR_TARGET_OCTET; DIGEST_OCTETS];
    let manifest = [VECTOR_MANIFEST_OCTET; DIGEST_OCTETS];
    let content = [VECTOR_CONTENT_OCTET; DIGEST_OCTETS];
    let preview = MaintenanceResultIdentifier::derive(
        &target,
        MaintenanceResultKind::Preview,
        &manifest,
        &content,
    );
    let applied = MaintenanceResultIdentifier::derive(
        &target,
        MaintenanceResultKind::Application,
        &manifest,
        &content,
    );
    assert_ne!(preview, applied, "one octet of difference, and it is not a collision");
    assert_eq!(MaintenanceResultKind::Preview.octet(), 0x00);
    assert_eq!(MaintenanceResultKind::Application.octet(), 0x01);
}

#[test]
fn the_preimage_carries_nothing_that_names_an_operation() {
    let target = [VECTOR_TARGET_OCTET; DIGEST_OCTETS];
    let manifest = [VECTOR_MANIFEST_OCTET; DIGEST_OCTETS];
    let content = [VECTOR_CONTENT_OCTET; DIGEST_OCTETS];
    let preimage = MaintenanceResultIdentifier::preimage(
        &target,
        MaintenanceResultKind::Application,
        &manifest,
        &content,
    );
    assert_eq!(preimage.len(), MAINTENANCE_RESULT_PREIMAGE_OCTETS, "ninety-seven octets");
    assert_eq!(&preimage[..DIGEST_OCTETS], &target, "the target, first and fixed width");
    assert_eq!(preimage[DIGEST_OCTETS], 0x01, "then the kind, one octet");
    assert_eq!(&preimage[DIGEST_OCTETS + 1..DIGEST_OCTETS * 2 + 1], &manifest);
    assert_eq!(&preimage[DIGEST_OCTETS * 2 + 1..], &content);
    assert_eq!(
        DIGEST_OCTETS * 3 + 1,
        MAINTENANCE_RESULT_PREIMAGE_OCTETS,
        "three digests and one octet, with nothing left over for an operation identifier, \
         a command name, or an artifact slot"
    );
    assert_eq!(MAINTENANCE_RESULT_DOMAIN, "slingshot.maintenance-result/1");
}

#[test]
fn every_rendering_vector_is_read_the_way_the_fixture_says() {
    for row in &rows(RENDERINGS) {
        let spelling = text(row, "spelling");
        let accepted = row["accepted"].as_bool().expect("a verdict");
        let outcome = MaintenanceResultIdentifier::parse(spelling);
        assert_eq!(outcome.is_ok(), accepted, "{}", text(row, "note"));
        if !accepted {
            assert_eq!(outcome, Err(MaintenanceResultIdentifierFailure::NotCanonical));
        }
    }
    assert_eq!(DIGEST_CHARACTERS, 64);
}
