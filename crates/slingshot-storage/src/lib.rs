//! Operation ledger and artifact persistence.
//!
//! The workspace dependency contract lets this crate depend only on
//! `slingshot-domain`, so it persists domain agent-job values without an edge
//! to the wire protocol crate. This commit declares the crate's
//! module families as documentation-only roots.

pub mod operation;
