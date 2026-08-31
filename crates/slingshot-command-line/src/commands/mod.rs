//! Command-line command family root.
//!
//! The module map assigns this family the argument surface of every catalog
//! operation the executable exposes. The leaves are declared in the order the
//! command reference presents them, so a reader meeting them here meets them
//! in the same order twice.

pub mod content;
pub mod package;
pub mod replication;
pub mod configuration;
pub mod page_query;
pub mod path_query;
pub mod asset_query;
pub mod page_mutation;
