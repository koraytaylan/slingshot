//! Operation ledger and artifact persistence.
//!
//! The workspace dependency contract lets this crate depend only on
//! `slingshot-domain`, so it persists domain agent-job values without an edge
//! to the wire protocol crate. This commit declares the crate's
//! module families as documentation-only roots.

pub mod agent_job_repository;
pub mod artifact_store;
pub mod database;
pub mod installation_state;
pub mod maintenance;
pub mod operation;
pub mod operation_repository;
pub mod persistent_capacity;
pub mod sqlite_statement_inventory;
pub mod sqlite_vfs;
