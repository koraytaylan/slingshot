//! Shapes, bounds and lookup helpers for the closed SQL inventory.

use super::STATEMENTS;

/// One statement this crate may run.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct InventoriedStatement {
    /// What it is for.
    pub purpose: &'static str,
    /// The exact text, with bind markers and no interpolation.
    pub text: &'static str,
    /// How many parameters it binds.
    pub parameters: usize,
    /// How many rows it can return.
    pub maximum_rows: u64,
}

/// Constructs this crate may never use.
///
/// Each of them either reaches a file outside the whitelist, writes one the
/// accounting does not know about, or takes its text from somewhere other than
/// this inventory.
pub const FORBIDDEN_CONSTRUCTS: &[&str] = &[
    "ATTACH",
    "DETACH",
    "VACUUM",
    "CREATE TEMP",
    "CREATE TEMPORARY",
    "PRAGMA TEMP_STORE_DIRECTORY",
];

/// One row the listing statement can return.
pub(super) const LISTING_ROWS: u64 = 256;

/// One row a keyed lookup can return.
pub(super) const SINGLE_ROW: u64 = 1;

/// Physical Sling jobs one logical submission may be carried by.
pub(super) const PHYSICAL_JOB_ROWS: u64 = 32;

/// Returns the text of the statement with `purpose`.
///
/// The inventory is the single place a statement exists, so every runner looks
/// its text up here rather than holding a copy. A statement that is not in the
/// list is therefore not reachable at all.
///
/// # Panics
///
/// Panics when no statement carries `purpose`, which is a programming mistake
/// rather than a runtime condition: the purposes are constants in this file.
#[must_use]
pub fn statement_text(purpose: &str) -> &'static str {
    STATEMENTS
        .iter()
        .find(|inventoried| inventoried.purpose == purpose)
        .map(|inventoried| inventoried.text)
        .unwrap_or_else(|| panic!("the inventory holds a statement for {purpose}"))
}

/// Returns whether `text` is a statement this crate may run.
#[must_use]
pub fn is_inventoried(text: &str) -> bool {
    STATEMENTS.iter().any(|statement| statement.text == text)
}

/// A mutation returns no result rows; its parameter count remains explicit.
pub(super) const fn mutation(
    purpose: &'static str,
    text: &'static str,
    parameters: usize,
) -> InventoriedStatement {
    InventoriedStatement { purpose, text, parameters, maximum_rows: 0 }
}
