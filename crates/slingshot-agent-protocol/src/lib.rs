//! Language-neutral author-agent messages, schemas, and wire conversions.
//!
//! The workspace dependency contract lets this crate depend only on
//! `slingshot-domain`, whose durable agent-job values it converts to and from
//! their wire representations. This commit declares the crate's module families as
//! documentation-only roots.

pub mod artifact_unavailable;
pub mod capabilities;
pub mod event_stream_reset;
pub mod subscription_high_water;
pub mod continuation_key_authority;
pub mod continuation_token;
pub mod identity;
pub mod job_contract;
pub mod job_event_document;
pub mod lookup_absence;
pub mod physical_job_missing;
pub mod remote_job;
pub mod terminal_result;
pub mod terminal_failure;
pub mod wire_contract;
