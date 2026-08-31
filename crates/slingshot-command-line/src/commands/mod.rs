//! Command-line command family root.
//!
//! The module map assigns this family the argument surface of every catalog
//! operation the executable exposes. The leaves are declared in the order the
//! command reference presents them, so a reader meeting them here meets them
//! in the same order twice.

pub mod content;
pub mod operational_values;
pub mod package;
pub mod replication;
pub mod configuration;
pub mod page_query;
pub mod path_query;
pub mod asset_query;
pub mod page_mutation;
pub mod page_lifecycle;
pub mod asset_lifecycle;
pub mod content_fragment;
pub mod experience_fragment;
pub mod platform_configuration;
pub mod resource_mapping;
pub mod workflow;
pub mod sling_job;
pub mod authorizable;
pub mod replication_queue;
