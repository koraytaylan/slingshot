//! What a database costs on disk, and when there is not enough of it.
//!
//! A daemon that runs out of space partway through a write leaves a database
//! somebody has to recover, so the arithmetic that decides whether a write may
//! begin belongs somewhere it can be checked rather than inside the code that
//! performs it. Every figure here is derived from the runtime contract: the
//! page size, the log's headers, and the reserve a filesystem is left with.
//!
//! # A reserve is not spare space
//!
//! The reserve exists so that the one operation nobody can defer - recovering
//! after a crash - always has room. Treating it as capacity would mean
//! discovering it was needed at exactly the moment it was gone.

use slingshot_domain::daemon_runtime_contract::DaemonRuntimeContract;

/// What one database occupies while it is being written to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PhysicalFootprint {
    /// Bytes the database file holds.
    pub database_bytes: u64,
    /// Bytes the write-ahead log holds.
    pub write_ahead_log_bytes: u64,
    /// Bytes the shared-memory index holds.
    pub shared_memory_bytes: u64,
}

impl PhysicalFootprint {
    /// Returns everything one database occupies together.
    #[must_use]
    pub fn total(self) -> u64 {
        self.database_bytes
            .saturating_add(self.write_ahead_log_bytes)
            .saturating_add(self.shared_memory_bytes)
    }

    /// Returns the largest footprint the contract admits.
    #[must_use]
    pub fn largest_admitted() -> Self {
        Self {
            database_bytes: formula("maximum_sqlite_database_bytes"),
            write_ahead_log_bytes: formula("maximum_sqlite_write_ahead_log_bytes"),
            shared_memory_bytes: limit("maximum_sqlite_shared_memory_bytes"),
        }
    }
}

/// Returns one limit the runtime contract names.
fn limit(named: &str) -> u64 {
    DaemonRuntimeContract::embedded().limit(named)
}

/// Returns one formula the runtime contract derives.
fn formula(named: &str) -> u64 {
    DaemonRuntimeContract::embedded().formula(named)
}

/// What a filesystem has, and what a write would need.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SpaceQuestion {
    /// Bytes the filesystem reports free.
    pub available_bytes: u64,
    /// Bytes the write would add.
    pub wanted_bytes: u64,
}

/// What the arithmetic says about one write.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SpaceAnswer {
    /// There is room, with the reserve still intact.
    Permitted,
    /// There is room only by taking the reserve, so the write waits.
    Backpressured,
    /// There is not room even with the reserve.
    Refused,
}

/// Returns what the arithmetic says about one write.
///
/// The reserve is subtracted before the question is asked, because a write that
/// fitted only by consuming it would leave recovery with nothing - and recovery
/// is the one thing that cannot be deferred until space appears.
#[must_use]
pub fn answered(question: SpaceQuestion) -> SpaceAnswer {
    let reserve = formula("persistent_filesystem_safety_reserve_bytes");
    let usable = question.available_bytes.saturating_sub(reserve);
    if question.wanted_bytes <= usable {
        return SpaceAnswer::Permitted;
    }
    if question.wanted_bytes <= question.available_bytes {
        return SpaceAnswer::Backpressured;
    }
    SpaceAnswer::Refused
}

/// Returns the bytes a log holding `frames` pages occupies.
///
/// Header, then one frame header and one page per frame. Written out rather
/// than approximated, because an approximation that rounded down would let a
/// log grow past the bound the contract sets for it.
#[must_use]
pub fn write_ahead_log_bytes(frames: u64) -> u64 {
    let header = limit("sqlite_write_ahead_log_header_bytes");
    let per_frame = limit("sqlite_write_ahead_log_frame_header_bytes") + limit("sqlite_page_bytes");
    header.saturating_add(frames.saturating_mul(per_frame))
}

/// Returns whether a log of `frames` is past the point one is checkpointed.
#[must_use]
pub fn wants_checkpoint(frames: u64) -> bool {
    frames >= limit("maximum_sqlite_write_ahead_log_frames")
}
