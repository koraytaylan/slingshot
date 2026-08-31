//! The manifest's formulas, against numbers computed somewhere else entirely.

use slingshot_domain::daemon_runtime_contract::DaemonRuntimeContract;
use slingshot_domain::persistent_capacity::{
    NAMESPACE_FORMULA_NAMES, NAMESPACE_LIMIT_NAMES, PersistentCapacityPolicy, PolicyFailure,
    REMOTE_ARTIFACT_SLOT_LIMITS,
};

use crate::fixtures::*;

#[test]
fn every_formula_equals_the_value_its_operands_produce() {
    let contract = DaemonRuntimeContract::embedded();
    let vectors = rows(FORMULAS);
    assert_eq!(vectors.len(), 11, "every formula the manifest carries has a vector");
    for row in &vectors {
        let name = text(row, "name");
        let recomputed = row["value"].as_u64().expect("a vector states its result");
        assert_eq!(contract.formula(name), recomputed, "{name}: {}", text(row, "note"));
        let operands = row["operands"].as_object().expect("a vector states its operands");
        assert!(!operands.is_empty(), "{name} names what it was computed from");
    }
    contract.require_consistent_formulas().expect("and the manifest agrees with itself");
}

#[test]
fn the_write_ahead_log_maximum_is_not_a_page_count_times_a_page() {
    let vectors = rows(FORMULAS);
    let row = vectors
        .iter()
        .find(|row| text(row, "name") == "maximum_sqlite_write_ahead_log_bytes")
        .expect("the log vector");
    let operands = &row["operands"];
    let frames = operands["frames"].as_u64().expect("a frame count");
    let page = operands["page_bytes"].as_u64().expect("a page length");
    let naive = frames * page;
    let actual = row["value"].as_u64().expect("a result");
    assert!(
        actual > naive,
        "a framed page carries its own header, so the log is larger than {naive} bytes of pages"
    );
    assert_eq!(
        actual - naive,
        operands["header_bytes"].as_u64().expect("a file header")
            + frames * operands["frame_header_bytes"].as_u64().expect("a frame header"),
        "and the difference is exactly one file header plus one header per frame"
    );
}

#[test]
fn the_backpressure_threshold_leaves_room_for_one_whole_transaction() {
    let contract = DaemonRuntimeContract::embedded();
    let log = contract.formula("maximum_sqlite_write_ahead_log_bytes");
    let transaction = contract.formula("maximum_sqlite_write_transaction_write_ahead_log_bytes");
    let threshold = contract.formula("sqlite_write_backpressure_bytes");
    assert_eq!(
        threshold + transaction,
        log,
        "refusing at the threshold means the largest transaction still fits below the cap"
    );
}

#[test]
fn the_artifact_bound_is_exactly_the_largest_thing_that_has_to_fit_in_it() {
    let policy = PersistentCapacityPolicy::embedded();
    let contract = slingshot_domain::command::command_identity::CommandContract::embedded();
    let declared: Vec<(&str, u64)> =
        REMOTE_ARTIFACT_SLOT_LIMITS.iter().map(|name| (*name, contract.limit(name))).collect();
    assert_eq!(declared.len(), 2, "the two commands that produce artifacts rather than results");
    let canonical =
        DaemonRuntimeContract::embedded().limit("maximum_canonical_structured_result_bytes");
    policy
        .require_artifact_bound_covers(&declared, canonical)
        .expect("every declared slot fits, and the bound is the largest of them");

    let unstorable = [("a_command_nobody_checked", policy.individual_artifact_bytes + 1)];
    assert!(
        matches!(
            policy.require_artifact_bound_covers(&unstorable, canonical),
            Err(PolicyFailure::SlotUnrepresentable { .. })
        ),
        "a slot larger than the store can hold is a command that cannot succeed"
    );
    let smaller = [("a_smaller_command", canonical)];
    assert!(
        matches!(
            policy.require_artifact_bound_covers(&smaller, canonical),
            Err(PolicyFailure::ArtifactBoundNotTheLargest { .. })
        ),
        "and a bound larger than anything needs it is a promise nothing asked for"
    );
}

#[test]
fn every_bound_a_namespace_is_held_to_comes_from_the_manifest() {
    let contract = DaemonRuntimeContract::embedded();
    let policy = PersistentCapacityPolicy::embedded();
    for name in NAMESPACE_LIMIT_NAMES {
        assert!(contract.limit(name) > 0, "{name} is named by the manifest");
    }
    for name in NAMESPACE_FORMULA_NAMES {
        assert!(contract.formula(name) > 0, "{name} is named by the manifest");
    }
    assert_eq!(policy.retained_operation_rows, contract.limit("maximum_retained_operation_rows"));
    assert_eq!(
        policy.recovery_resume_receipts_per_operation,
        contract.limit("maximum_recovery_resume_receipts_per_operation")
    );
    assert_eq!(
        policy.maintenance_application_receipts_per_target,
        contract.limit("maximum_terminal_maintenance_application_receipts_per_target")
    );
    assert_eq!(
        policy.maintenance_result_associations_per_target,
        contract.formula("maximum_terminal_maintenance_result_associations_per_target")
    );
    assert_eq!(
        policy.committed_plus_reserved_artifact_bytes,
        contract.limit("maximum_committed_plus_reserved_artifact_bytes")
    );
    assert_eq!(
        policy.individual_artifact_bytes,
        contract.formula("maximum_individual_artifact_bytes")
    );
}
