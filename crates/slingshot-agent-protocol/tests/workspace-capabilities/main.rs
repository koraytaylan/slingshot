//! Capability probes owned by the agent-protocol crate.
//!
//! Each module exercises the public behavior one inventory row promises, so a
//! candidate that compiles but does not expose the required interface fails
//! here rather than in the first feature task that needs it.

mod byte_buffers;
mod javascript_object_notation;
mod schema_documents;
mod serialization;
mod unique_identifiers;
