//! Language-neutral author-agent messages, schemas, and wire conversions.
//!
//! The workspace dependency contract lets this crate depend only on
//! `slingshot-domain`, whose durable agent-job values it converts to and from
//! their wire representations. This commit declares the crate's module families as
//! documentation-only roots.

pub mod remote_job;
