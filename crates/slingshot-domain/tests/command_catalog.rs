//! The registry, and the one place safety and effect are decided.
//!
//! The classification table is proved row by row against what the architecture
//! says, because the whole value of a closed table is that nobody has to infer
//! anything from a name. Two rows are worth watching: loading and packaging are
//! reads that are not idempotent, so a caller supplies an operation key while
//! the read-only hint stays true. A rule that derived idempotency from access,
//! or from whether a command publishes an artifact, would get both wrong.

use serde_json::Value;
use slingshot_domain::command::canonical_json::write_canonical;
use slingshot_domain::command::catalog::{
    AccessClassification, Command, CommandCatalog, CommandResult, DestructiveClassification,
    ResultContextFailure, catalog_matches_schema_inventory, validate_result_for_command,
};
use slingshot_domain::command::command_identity::{CommandContract, INITIAL_COMMAND_VERSION};
use slingshot_domain::command::query_paths::{QueryPathsCommand, QueryPathsResult};
use slingshot_domain::command::replicate_content::{
    ReplicateContentCommand, ReplicateContentResult,
};
use slingshot_domain::command::repository_path::RepositoryPath;
use slingshot_domain::command::schema::COMMAND_WIRE_NAMES;

/// The committed catalog.
const COMMITTED: &str = include_str!("fixtures/commands/catalog.json");

/// Reads one row's string member.
fn text<'row>(row: &'row Value, member: &str) -> &'row str {
    row[member].as_str().unwrap_or_else(|| panic!("{member} is a string in {row}"))
}

#[test]
fn the_catalog_serializes_byte_for_byte_as_the_committed_fixture() {
    let written = write_canonical(
        &serde_json::to_value(CommandCatalog::published()).expect("the catalog serializes"),
    )
    .expect("the catalog is canonical");
    assert_eq!(COMMITTED, written, "the committed catalog differs from what the registry builds");
}

#[test]
fn every_command_appears_once_in_ascending_order() {
    let catalog = CommandCatalog::published();
    let names: Vec<&str> =
        catalog.descriptors().iter().map(|descriptor| descriptor.wire_name.as_str()).collect();
    assert_eq!(names.len(), 12, "twelve commands, and no thirteenth");
    let mut sorted = names.clone();
    sorted.sort_unstable();
    sorted.dedup();
    assert_eq!(names, sorted, "the names are unique and ascending");
    assert_eq!(names, COMMAND_WIRE_NAMES, "the catalog and the schema inventory agree");
    assert!(catalog_matches_schema_inventory());
}

#[test]
fn every_enum_variant_maps_to_exactly_one_descriptor_and_back() {
    let catalog = CommandCatalog::published();
    for wire_name in COMMAND_WIRE_NAMES {
        assert!(catalog.find(wire_name).is_some(), "{wire_name} has a descriptor");
    }
    let asked = Command::ReplicateContent(ReplicateContentCommand {
        path: RepositoryPath::parse("/content").expect("a legal path"),
        recursive: true,
    });
    assert_eq!(asked.wire_name(), "replicate_content");
    let answered =
        CommandResult::ReplicateContent(ReplicateContentResult { accepted_item_count: 0 });
    assert_eq!(answered.wire_name(), asked.wire_name());
    assert!(catalog.find(asked.wire_name()).is_some());
}

#[test]
fn the_twelve_classification_rows_are_exactly_what_the_architecture_says() {
    let catalog = CommandCatalog::published();
    let expected: &[(&str, AccessClassification, DestructiveClassification, bool)] = &[
        (
            "add_component",
            AccessClassification::Write,
            DestructiveClassification::NonDestructive,
            false,
        ),
        (
            "create_page",
            AccessClassification::Write,
            DestructiveClassification::NonDestructive,
            false,
        ),
        (
            "download_content_package",
            AccessClassification::Read,
            DestructiveClassification::NonDestructive,
            false,
        ),
        (
            "find_assets_by_metadata",
            AccessClassification::Read,
            DestructiveClassification::NonDestructive,
            true,
        ),
        (
            "find_assets_referenced_by_page",
            AccessClassification::Read,
            DestructiveClassification::NonDestructive,
            true,
        ),
        (
            "find_pages_by_template",
            AccessClassification::Read,
            DestructiveClassification::NonDestructive,
            true,
        ),
        (
            "find_pages_containing_phrase",
            AccessClassification::Read,
            DestructiveClassification::NonDestructive,
            true,
        ),
        (
            "find_pages_using_components",
            AccessClassification::Read,
            DestructiveClassification::NonDestructive,
            true,
        ),
        (
            "inspect_open_service_gateway_initiative_configuration",
            AccessClassification::Read,
            DestructiveClassification::NonDestructive,
            true,
        ),
        (
            "load_content_as_json",
            AccessClassification::Read,
            DestructiveClassification::NonDestructive,
            false,
        ),
        (
            "query_paths",
            AccessClassification::Read,
            DestructiveClassification::NonDestructive,
            true,
        ),
        (
            "replicate_content",
            AccessClassification::Write,
            DestructiveClassification::Destructive,
            false,
        ),
    ];
    assert_eq!(expected.len(), catalog.descriptors().len());
    for (wire_name, access, destructive, idempotent) in expected {
        let descriptor = catalog.find(wire_name).expect("a descriptor");
        assert_eq!(descriptor.access, *access, "{wire_name}");
        assert_eq!(descriptor.destructive, *destructive, "{wire_name}");
        assert_eq!(descriptor.intrinsic_idempotency.idempotent_hint(), *idempotent, "{wire_name}");
    }
    let destructive = catalog
        .descriptors()
        .iter()
        .filter(|descriptor| descriptor.destructive == DestructiveClassification::Destructive)
        .count();
    assert_eq!(destructive, 1, "replication is the only row that can replace visible content");
}

#[test]
fn every_hint_derives_from_its_own_column_and_no_other() {
    let catalog = CommandCatalog::published();
    let idempotent = catalog
        .descriptors()
        .iter()
        .filter(|descriptor| descriptor.intrinsic_idempotency.idempotent_hint())
        .count();
    assert_eq!(idempotent, 7, "seven idempotent commands");
    assert_eq!(catalog.descriptors().len() - idempotent, 5, "and five that are not");
    for descriptor in catalog.descriptors() {
        assert_eq!(
            descriptor.access.read_only_hint(),
            descriptor.access == AccessClassification::Read,
            "{}",
            descriptor.wire_name
        );
        assert_eq!(
            descriptor.destructive.destructive_hint(),
            descriptor.destructive == DestructiveClassification::Destructive,
            "{}",
            descriptor.wire_name
        );
        assert_eq!(
            descriptor.intrinsic_idempotency.requires_operation_key(),
            !descriptor.intrinsic_idempotency.idempotent_hint(),
            "{}: the key requirement and the hint come from one column",
            descriptor.wire_name
        );
    }
    for wire_name in ["load_content_as_json", "download_content_package"] {
        let descriptor = catalog.find(wire_name).expect("a descriptor");
        assert!(descriptor.access.read_only_hint(), "{wire_name} is a read");
        assert!(
            descriptor.intrinsic_idempotency.requires_operation_key(),
            "{wire_name} still needs a key, because a second run would publish a duplicate"
        );
        assert!(!descriptor.intrinsic_idempotency.idempotent_hint());
    }
}

#[test]
fn two_commands_declare_a_slot_and_the_other_ten_forbid_one() {
    let catalog = CommandCatalog::published();
    let declaring: Vec<&str> = catalog
        .descriptors()
        .iter()
        .filter(|descriptor| !descriptor.remote_artifact_slots.is_empty())
        .map(|descriptor| descriptor.wire_name.as_str())
        .collect();
    assert_eq!(declaring, vec!["download_content_package", "load_content_as_json"]);

    let load = catalog.find("load_content_as_json").expect("a descriptor");
    let slot = &load.remote_artifact_slots[0];
    assert_eq!(slot.slot.as_text(), "loaded_content_json");
    assert_eq!(slot.media_type.as_text(), "application/json");
    let package = catalog.find("download_content_package").expect("a descriptor");
    let slot = &package.remote_artifact_slots[0];
    assert_eq!(slot.slot.as_text(), "content_package");
    assert_eq!(slot.media_type.as_text(), "application/zip");
    assert_eq!(
        slot.maximum_byte_length,
        CommandContract::embedded().limit("maximum_package_output_bytes")
    );
    for descriptor in catalog.descriptors() {
        assert!(
            descriptor
                .remote_artifact_slots
                .iter()
                .all(|slot| slot.slot.as_text() != "structured_result"),
            "{}: the generic local result is never a remote slot",
            descriptor.wire_name
        );
    }
}

#[test]
fn every_descriptor_carries_one_version_one_limits_digest_and_two_distinct_roles() {
    let catalog = CommandCatalog::published();
    let limits = catalog.descriptors()[0].command_contract_limits_sha256.clone();
    let contract = catalog.descriptors()[0].canonical_json_contract_sha256.clone();
    for descriptor in catalog.descriptors() {
        assert!(!descriptor.wire_name.is_empty(), "a wire name is a capability name");
        assert!(!descriptor.title.is_empty());
        assert!(!descriptor.description.is_empty());
        assert!(
            !descriptor.description.contains("will "),
            "{}: the description says what the command does, not what it will do",
            descriptor.wire_name
        );
        assert_eq!(descriptor.command_semantic_contract_version, INITIAL_COMMAND_VERSION);
        assert_eq!(descriptor.command_contract_limits_sha256, limits, "one limits authority");
        assert_eq!(descriptor.canonical_json_contract_sha256, contract, "one byte contract");
        assert_ne!(
            descriptor.arguments_schema_sha256, descriptor.result_schema_sha256,
            "{}: a role swap would otherwise go unnoticed",
            descriptor.wire_name
        );
        assert!(descriptor.maximum_result_bytes > 0, "{}", descriptor.wire_name);
        assert!(!descriptor.failure_categories.is_empty(), "{}", descriptor.wire_name);
        let mut categories = descriptor.failure_categories.clone();
        categories.sort();
        categories.dedup();
        assert_eq!(
            categories.len(),
            descriptor.failure_categories.len(),
            "{}: a category is declared once",
            descriptor.wire_name
        );
    }
}

#[test]
fn compatibility_needs_all_five_and_not_four() {
    let catalog = CommandCatalog::published();
    let descriptor = catalog.find("query_paths").expect("a descriptor").clone();
    assert!(descriptor.compatible_with(&descriptor));
    for changed in [
        {
            let mut other = descriptor.clone();
            other.wire_name = "find_pages_by_template".to_owned();
            other
        },
        {
            let mut other = descriptor.clone();
            other.command_semantic_contract_version = "2.0.0".to_owned();
            other
        },
        {
            let mut other = descriptor.clone();
            other.command_contract_limits_sha256 = "0".repeat(64);
            other
        },
        {
            let mut other = descriptor.clone();
            other.arguments_schema_sha256 = "0".repeat(64);
            other
        },
        {
            let mut other = descriptor.clone();
            other.result_schema_sha256 = "0".repeat(64);
            other
        },
    ] {
        assert!(
            !descriptor.compatible_with(&changed),
            "one difference is enough to be incompatible"
        );
    }
}

#[test]
fn every_discovery_descriptor_allows_the_shared_categories_and_its_own_anchors() {
    let catalog = CommandCatalog::published();
    let shared = [
        "discovery_budget_exceeded",
        "continuation_token_malformed",
        "continuation_token_integrity_invalid",
        "continuation_token_wrong_target",
        "continuation_token_wrong_query",
        "continuation_token_expired",
    ];
    for wire_name in [
        "query_paths",
        "find_pages_containing_phrase",
        "find_pages_by_template",
        "find_pages_using_components",
        "find_assets_by_metadata",
    ] {
        let categories = &catalog.find(wire_name).expect("a descriptor").failure_categories;
        for category in shared {
            assert!(categories.iter().any(|held| held == category), "{wire_name}: {category}");
        }
        assert!(categories.iter().any(|held| held == "root_not_found"), "{wire_name}");
        assert!(categories.iter().any(|held| held == "root_access_denied"), "{wire_name}");
    }
    let referenced =
        &catalog.find("find_assets_referenced_by_page").expect("a descriptor").failure_categories;
    for category in ["page_not_found", "page_access_denied", "page_invalid"] {
        assert!(referenced.iter().any(|held| held == category), "{category}");
    }
    assert!(
        !referenced.iter().any(|held| held == "root_not_found"),
        "a page anchor is not a root anchor"
    );
    let package =
        &catalog.find("download_content_package").expect("a descriptor").failure_categories;
    assert!(package.iter().any(|held| held == "evaluation_budget_exceeded"));
    assert!(
        !package.iter().any(|held| held.contains("duration")),
        "the package budgets are deterministic counts and bytes, never a duration"
    );
}

#[test]
fn every_result_bound_is_the_one_the_architecture_names() {
    let catalog = CommandCatalog::published();
    let limits = CommandContract::embedded();
    for wire_name in [
        "query_paths",
        "find_pages_containing_phrase",
        "find_pages_by_template",
        "find_pages_using_components",
        "find_assets_by_metadata",
        "find_assets_referenced_by_page",
    ] {
        assert_eq!(
            catalog.find(wire_name).expect("a descriptor").maximum_result_bytes,
            limits.limit("maximum_discovery_result_bytes"),
            "{wire_name}"
        );
    }
    assert_eq!(
        catalog
            .find("inspect_open_service_gateway_initiative_configuration")
            .expect("a descriptor")
            .maximum_result_bytes,
        limits.limit("maximum_inspected_configuration_result_bytes")
    );
}

#[test]
fn a_result_of_another_command_or_another_request_is_refused() {
    let asked = Command::QueryPaths(
        serde_json::from_str::<QueryPathsCommand>(r#"{"root_path":"/content/example"}"#)
            .expect("a legal command"),
    );
    let own = CommandResult::QueryPaths(
        QueryPathsResult::new(Vec::new(), None).expect("an ordered page"),
    );
    assert_eq!(validate_result_for_command(&asked, &own), Ok(()));

    let other = CommandResult::ReplicateContent(ReplicateContentResult { accepted_item_count: 0 });
    assert_eq!(
        validate_result_for_command(&asked, &other),
        Err(ResultContextFailure::VariantMismatch),
        "a result of another command entirely"
    );

    let elsewhere = CommandResult::QueryPaths(
        QueryPathsResult::new(
            vec![slingshot_domain::command::query_paths::PathMatch {
                repository_path: RepositoryPath::parse("/content/other").expect("a legal path"),
            }],
            None,
        )
        .expect("an ordered page"),
    );
    assert_eq!(
        validate_result_for_command(&asked, &elsewhere),
        Err(ResultContextFailure::RequestMismatch),
        "the right shape, answering another request"
    );
}

#[test]
fn a_substitution_with_no_distinguishing_fact_is_deferred_rather_than_claimed() {
    let asked = Command::ReplicateContent(ReplicateContentCommand {
        path: RepositoryPath::parse("/content/example").expect("a legal path"),
        recursive: false,
    });
    let answered =
        CommandResult::ReplicateContent(ReplicateContentResult { accepted_item_count: 1 });
    assert_eq!(
        validate_result_for_command(&asked, &answered),
        Ok(()),
        "a replication success echoes no request-derived fact, so this layer cannot tell \
         two of them apart and does not pretend to; Plan 0005's authenticated submitted \
         command digest closes it"
    );
}

#[test]
fn the_committed_catalog_names_every_command_and_no_other() {
    let catalog: Value = serde_json::from_str(COMMITTED).expect("the fixture is one value");
    let rows = catalog.as_array().expect("a list of descriptors");
    assert_eq!(rows.len(), 12);
    let names: Vec<&str> = rows.iter().map(|row| text(row, "wire_name")).collect();
    assert_eq!(names, COMMAND_WIRE_NAMES);
}
