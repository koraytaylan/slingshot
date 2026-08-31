//! Whether a write may begin, worked out from the contract rather than the code.
//!
//! The vectors are computed from the runtime contract's own numbers and
//! committed, so the arithmetic is compared against something written
//! independently of it. An implementation checked against itself proves that it
//! is consistent, which is not the same as proving it is right.
//!
//! The reserve is the point of the whole exercise. It exists so recovery after
//! a crash always has room, and a write that fitted only by consuming it would
//! leave the one operation nobody can defer with nothing.

use std::path::PathBuf;

use slingshot_domain::daemon_runtime_contract::DaemonRuntimeContract;
use slingshot_test_support::storage_faults::{
    PhysicalFootprint, SpaceAnswer, SpaceQuestion, answered, wants_checkpoint,
    write_ahead_log_bytes,
};

/// Where the space vectors live.
const FIXTURE: &str = "tests/fixtures/storage-faults/space.jsonl";

/// One declared space question.
#[derive(Debug, serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct Vector {
    /// What it is called.
    name: String,
    /// What the filesystem reports free.
    available_bytes: u64,
    /// What the write would add.
    wanted_bytes: u64,
    /// What the arithmetic says.
    answer: String,
}

/// Returns every declared question.
fn vectors() -> Vec<Vector> {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(FIXTURE);
    let text = std::fs::read_to_string(&path)
        .unwrap_or_else(|failure| panic!("{} could not be read: {failure}", path.display()));
    text.lines()
        .filter(|line| !line.trim().is_empty() && !line.starts_with('#'))
        .map(|line| serde_json::from_str(line).expect("every vector reads"))
        .collect()
}

#[test]
fn every_committed_vector_is_answered_the_way_it_was_computed() {
    for vector in vectors() {
        let expected = match vector.answer.as_str() {
            "permitted" => SpaceAnswer::Permitted,
            "backpressured" => SpaceAnswer::Backpressured,
            _ => SpaceAnswer::Refused,
        };
        let held = answered(SpaceQuestion {
            available_bytes: vector.available_bytes,
            wanted_bytes: vector.wanted_bytes,
        });
        assert_eq!(held, expected, "{}", vector.name);
    }
}

#[test]
fn the_reserve_the_vectors_were_computed_against_is_the_one_the_contract_names() {
    let reserve =
        DaemonRuntimeContract::embedded().formula("persistent_filesystem_safety_reserve_bytes");
    let exactly = vectors()
        .into_iter()
        .find(|vector| vector.name == "exactly-the-reserve-left")
        .expect("the vector is committed");
    assert_eq!(
        exactly.available_bytes, reserve,
        "the vectors were computed against another reserve than the contract names"
    );
}

#[test]
fn a_write_that_would_eat_the_reserve_waits_rather_than_proceeding() {
    let reserve =
        DaemonRuntimeContract::embedded().formula("persistent_filesystem_safety_reserve_bytes");
    let held = answered(SpaceQuestion { available_bytes: reserve, wanted_bytes: 1 });
    assert_eq!(
        held,
        SpaceAnswer::Backpressured,
        "recovery is the one operation that cannot be deferred until space appears"
    );
    let with_room = answered(SpaceQuestion { available_bytes: reserve + 1, wanted_bytes: 1 });
    assert_eq!(with_room, SpaceAnswer::Permitted, "one usable byte is one byte of room");
}

#[test]
fn a_log_of_one_frame_costs_a_header_a_frame_header_and_a_page() {
    let contract = DaemonRuntimeContract::embedded();
    let header = contract.limit("sqlite_write_ahead_log_header_bytes");
    let frame_header = contract.limit("sqlite_write_ahead_log_frame_header_bytes");
    let page = contract.limit("sqlite_page_bytes");
    assert_eq!(write_ahead_log_bytes(0), header, "an empty log is its header");
    assert_eq!(write_ahead_log_bytes(1), header + frame_header + page);
    assert_eq!(
        write_ahead_log_bytes(FRAMES),
        header + FRAMES * (frame_header + page),
        "the cost is linear, and an approximation would let a log grow past its bound"
    );
}

/// How many frames the linearity case uses.
const FRAMES: u64 = 1_024;

#[test]
fn the_largest_log_the_contract_admits_is_the_one_its_own_formula_derives() {
    let contract = DaemonRuntimeContract::embedded();
    let frames = contract.limit("maximum_sqlite_write_ahead_log_frames");
    let derived = write_ahead_log_bytes(frames);
    assert_eq!(
        derived,
        contract.formula("maximum_sqlite_write_ahead_log_bytes"),
        "the arithmetic here and the contract's own formula disagree"
    );
}

#[test]
fn a_log_at_its_frame_bound_wants_a_checkpoint_and_one_below_it_does_not() {
    let frames = DaemonRuntimeContract::embedded().limit("maximum_sqlite_write_ahead_log_frames");
    assert!(!wants_checkpoint(frames - 1));
    assert!(wants_checkpoint(frames), "the bound itself is the point at which one is taken");
    assert!(wants_checkpoint(frames + 1));
}

#[test]
fn the_largest_admitted_footprint_is_what_the_contract_says_it_is() {
    let contract = DaemonRuntimeContract::embedded();
    let held = PhysicalFootprint::largest_admitted();
    assert_eq!(held.database_bytes, contract.formula("maximum_sqlite_database_bytes"));
    assert_eq!(
        held.write_ahead_log_bytes,
        contract.formula("maximum_sqlite_write_ahead_log_bytes")
    );
    assert_eq!(held.shared_memory_bytes, contract.limit("maximum_sqlite_shared_memory_bytes"));
    assert_eq!(
        held.total(),
        contract.formula("maximum_sqlite_physical_bytes"),
        "the parts and the whole the contract names disagree"
    );
}
