//! Assertions for the normative command limits and versions.
//!
//! Every bound a command enforces, every keyword a schema emits, and every
//! vector an external agent runs comes from one file. That only holds if the
//! file is exactly what a reader regenerates from it, and if nothing else in the
//! repository writes one of its values down a second time - so both are checked
//! here rather than assumed.
//!
//! The version grammar is checked against its own asymmetry, which is the part
//! that is easy to get wrong: a core or numeric prerelease identifier has one
//! minimal spelling, while build metadata keeps whatever spelling it was given.
//! `1.0.0+01` is therefore legal and `1.0.0-01` is not.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use slingshot_domain::command::command_identity::{
    CONTRACT_CANONICALIZATION, CONTRACT_DURATION_UNIT, CONTRACT_FORMAT, CommandContract,
    CommandSemanticContractVersion, INITIAL_COMMAND_VERSION, VersionFailure,
};

/// Path of the committed manifest, relative to this crate.
const MANIFEST_PATH: &str = "../../schemas/command-contract-limits-1.json";

/// Directory the command family lives in, relative to this crate.
const FAMILY_DIRECTORY: &str = "src/command";

/// The one leaf that reads the manifest rather than consuming it.
const FOUNDATION_LEAF: &str = "command_identity.rs";

/// Smallest value a repeated limit is worth searching for.
///
/// Below this, a number in ordinary code is far more often an index, a width,
/// or a small count than a copy of a bound, and treating it as a copy would
/// make the assertion noise rather than evidence.
const SMALLEST_SEARCHABLE_LIMIT: u64 = 32;

/// Core identifiers every version carries.
const CORE_IDENTIFIERS: usize = 3;

/// Prerelease identifiers the split-charge assertion uses.
const SPLIT: usize = 2;

/// Digits one underscore-grouped number puts between separators.
const DIGIT_GROUP: usize = 3;

/// Commands this plan publishes.
const COMMANDS: &[&str] = &[
    "load_content_as_json",
    "inspect_open_service_gateway_initiative_configuration",
    "query_paths",
    "find_pages_containing_phrase",
    "find_pages_by_template",
    "find_pages_using_components",
    "find_assets_by_metadata",
    "find_assets_referenced_by_page",
    "replicate_content",
    "download_content_package",
    "create_page",
    "add_component",
];

/// Returns the directory this crate's manifest lives in.
fn crate_directory() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

#[test]
fn rendering_the_parsed_contract_reproduces_the_committed_manifest() {
    let manifest =
        std::fs::read_to_string(crate_directory().join(MANIFEST_PATH)).expect("the manifest reads");
    assert_eq!(manifest, CommandContract::embedded_manifest(), "the embedded bytes drifted");
    let contract = CommandContract::embedded();
    assert_eq!(contract.render().expect("it renders"), manifest, "the reader is not a mirror");
    assert_eq!(contract.format, CONTRACT_FORMAT);
    assert_eq!(contract.canonicalization, CONTRACT_CANONICALIZATION);
    assert_eq!(contract.duration_unit, CONTRACT_DURATION_UNIT);
    assert!(manifest.ends_with('\n'), "the manifest does not end with one line feed");
    assert!(!manifest.contains("  "), "the manifest carries insignificant whitespace");
}

#[test]
fn every_command_this_plan_publishes_is_at_the_one_initial_version() {
    let declared: Vec<&str> = CommandContract::embedded()
        .command_semantic_contract_versions
        .keys()
        .map(String::as_str)
        .collect();
    let mut expected: Vec<&str> = COMMANDS.to_vec();
    expected.sort_unstable();
    assert_eq!(declared, expected, "the command inventory changed");
    for version in CommandContract::embedded().command_semantic_contract_versions.values() {
        assert_eq!(version, INITIAL_COMMAND_VERSION, "a command chose its own version");
    }
    assert!(
        CommandSemanticContractVersion::parse(INITIAL_COMMAND_VERSION).is_ok(),
        "the initial version is not a version"
    );
}

#[test]
fn the_derived_limit_is_exactly_what_its_equation_says() {
    let contract = CommandContract::embedded();
    let tokens = contract.limit("maximum_package_selection_expression_tokens");
    let segments = contract.limit("maximum_repository_path_segments");
    assert_eq!(
        contract.limit("maximum_package_matcher_cells"),
        (tokens + 1) * (segments + 1),
        "the matcher bound is not its own equation"
    );
    assert_eq!(
        contract.limit("maximum_asset_byte_length"),
        u64::try_from(i64::MAX).expect("the range fits"),
        "the asset length is not the signed sixty-four-bit maximum"
    );
    assert!(
        contract.limit("maximum_agent_inline_loaded_document_bytes")
            < contract.limit("maximum_loaded_content_artifact_bytes"),
        "an inline document could not be smaller than the artifact it becomes"
    );
}

#[test]
fn asking_for_a_limit_nobody_declared_fails_rather_than_answering() {
    let asked = std::panic::catch_unwind(|| CommandContract::embedded().limit("invented_limit"));
    assert!(asked.is_err(), "an invented limit was answered");
}

#[test]
fn every_accepted_version_spelling_round_trips_and_every_other_one_fails() {
    for accepted in [
        "0.0.0",
        "1.0.0",
        "1.2.3",
        "1.0.0-alpha",
        "1.0.0-alpha.1",
        "1.0.0-0.3.7",
        "1.0.0-x-y-z",
        "1.0.0+01",
        "1.0.0+build.001",
        "1.0.0-alpha+001",
        "1234567890.0.0",
    ] {
        let version = CommandSemanticContractVersion::parse(accepted)
            .unwrap_or_else(|failure| panic!("{accepted} was refused: {failure}"));
        assert_eq!(version.as_text(), accepted, "the spelling changed");
        assert_eq!(version.to_string(), accepted);
    }

    for (refused, expected) in [
        ("1.0.0-01", VersionFailure::NonMinimalNumber),
        ("01.0.0", VersionFailure::NonMinimalNumber),
        ("1.00.0", VersionFailure::NonMinimalNumber),
        ("12345678901.0.0", VersionFailure::NumericTooLong),
        ("1.0.0-12345678901", VersionFailure::NumericTooLong),
        ("1.0", VersionFailure::MalformedCore),
        ("1.0.0.0", VersionFailure::MalformedCore),
        ("1.0.0-", VersionFailure::MalformedIdentifier),
        ("1.0.0+", VersionFailure::MalformedIdentifier),
        ("1.0.0-alpha..1", VersionFailure::MalformedIdentifier),
        ("1.0.0-alpha/1", VersionFailure::MalformedIdentifier),
        ("1.0.0-alpha:1", VersionFailure::MalformedIdentifier),
        ("1.0.0-alpha?1", VersionFailure::MalformedIdentifier),
        ("1.0.0-alpha#1", VersionFailure::MalformedIdentifier),
        ("1.0.0-alpha 1", VersionFailure::MalformedIdentifier),
        ("1.0.0-alphá", VersionFailure::MalformedIdentifier),
        ("", VersionFailure::MalformedCore),
    ] {
        assert_eq!(
            CommandSemanticContractVersion::parse(refused),
            Err(expected),
            "{refused} was accepted or refused for another reason"
        );
    }
}

#[test]
fn a_version_is_bounded_in_bytes_and_in_identifiers() {
    let contract = CommandContract::embedded();
    let maximum_bytes =
        usize::try_from(contract.limit("maximum_command_semantic_contract_version_bytes"))
            .expect("the bound fits");
    let maximum_identifiers =
        usize::try_from(contract.limit("maximum_command_semantic_contract_version_identifiers"))
            .expect("the bound fits");

    let core = "1.0.0+";
    let filler = "a".repeat(maximum_bytes - core.len());
    let exact = format!("{core}{filler}");
    assert_eq!(exact.len(), maximum_bytes);
    assert!(CommandSemanticContractVersion::parse(&exact).is_ok(), "the bound itself was refused");
    assert_eq!(
        CommandSemanticContractVersion::parse(&format!("{exact}a")),
        Err(VersionFailure::TooLong)
    );

    let additional = maximum_identifiers - CORE_IDENTIFIERS;
    let identifiers: Vec<&str> = (0..additional).map(|_| "a").collect();
    let exact = format!("1.0.0-{}", identifiers.join("."));
    assert!(
        CommandSemanticContractVersion::parse(&exact).is_ok(),
        "the identifier bound itself was refused"
    );
    assert_eq!(
        CommandSemanticContractVersion::parse(&format!("{exact}.a")),
        Err(VersionFailure::TooManyIdentifiers)
    );
    let split =
        format!("1.0.0-{}+{}", identifiers[..SPLIT].join("."), identifiers[SPLIT..].join("."));
    assert!(
        CommandSemanticContractVersion::parse(&split).is_ok(),
        "the charge is not the sum across both parts"
    );
    assert_eq!(
        CommandSemanticContractVersion::parse(&format!("{split}.a")),
        Err(VersionFailure::TooManyIdentifiers)
    );
}

#[test]
fn no_command_module_writes_a_contract_value_down_again() {
    let contract = CommandContract::embedded();
    let values: BTreeSet<String> = contract
        .limits
        .values()
        .filter(|value| **value >= SMALLEST_SEARCHABLE_LIMIT)
        .map(|value| value.to_string())
        .collect();
    let mut repeated = Vec::new();
    for path in command_modules() {
        let text = std::fs::read_to_string(&path).expect("the source reads");
        for (number, line) in text.lines().enumerate() {
            let code = line.trim();
            if code.starts_with("//") {
                continue;
            }
            for value in &values {
                if carries_value(code, value) {
                    repeated.push(format!("{}:{} repeats {value}", path.display(), number + 1));
                }
            }
        }
    }
    assert_eq!(repeated, Vec::<String>::new(), "a contract value exists twice");
}

/// Returns every command module that consumes the contract.
///
/// The scan is scoped to the command family, because that is where a bound
/// would be redeclared: a number elsewhere in the workspace that happens to
/// equal one of these is a different value with the same magnitude.
fn command_modules() -> Vec<PathBuf> {
    let directory = crate_directory().join(FAMILY_DIRECTORY);
    let mut modules = Vec::new();
    for entry in std::fs::read_dir(&directory).expect("the family directory reads") {
        let path = entry.expect("the entry reads").path();
        if path.file_name().is_some_and(|name| name == FOUNDATION_LEAF) {
            continue;
        }
        modules.push(path);
    }
    modules
}

/// Reports whether one line writes `value` as a number of its own.
///
/// The comparison requires the value to stand alone, so a longer number that
/// happens to contain it, and an identifier that ends in digits, are not
/// mistaken for a second copy of a bound.
fn carries_value(line: &str, value: &str) -> bool {
    let spellings = [value.to_owned(), grouped(value)];
    spellings.iter().any(|spelling| {
        line.match_indices(spelling.as_str()).any(|(start, _)| {
            let before = line[..start].chars().next_back();
            let after = line[start + spelling.len()..].chars().next();
            let bounded = |character: Option<char>| {
                character
                    .is_none_or(|character| !character.is_ascii_alphanumeric() && character != '_')
            };
            bounded(before) && bounded(after)
        })
    })
}

/// Returns one number in the underscore-grouped spelling Rust also accepts.
fn grouped(value: &str) -> String {
    let digits: Vec<char> = value.chars().rev().collect();
    let mut grouped: Vec<char> = Vec::new();
    for (position, digit) in digits.iter().enumerate() {
        if position > 0 && position % DIGIT_GROUP == 0 {
            grouped.push('_');
        }
        grouped.push(*digit);
    }
    grouped.iter().rev().collect()
}

#[test]
fn the_manifest_is_where_the_repository_keeps_it() {
    let path = Path::new(MANIFEST_PATH);
    assert_eq!(path.extension().and_then(|extension| extension.to_str()), Some("json"));
    assert!(crate_directory().join(path).is_file(), "the manifest is not committed");
}
