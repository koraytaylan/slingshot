//! Language-neutral author-agent messages, schemas, and wire conversions.
//!
//! The workspace dependency contract lets this crate depend only on
//! `slingshot-domain`, whose durable agent-job values it converts to and from
//! their wire representations. This commit declares the crate's module families as
//! documentation-only roots.

pub mod continuation_key_authority;
pub mod continuation_token;
pub mod identity;
pub mod remote_job;
pub mod wire_contract;
