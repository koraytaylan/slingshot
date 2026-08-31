//! Model Context Protocol family root.
//!
//! The module map assigns this family the standard-stream server that offers
//! the catalog operations as protocol tools and resources. A family root
//! declares its children and holds nothing else, so what each child owns is
//! stated where that child lives.

pub mod active_request_registry;
pub mod application;
pub mod current_stateless_revision;
pub mod legacy_initialized_revision;
pub mod operation_execution;
pub mod progress_and_cancellation;
pub mod protocol_diagnostics;
pub mod resource_catalog;
pub mod result_projection;
pub mod schema_projection;
pub mod size_budget;
pub mod standard_stream_transport;
pub mod tool_catalog;
