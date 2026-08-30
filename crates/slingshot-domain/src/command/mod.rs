//! Command family root.
//!
//! The module map assigns this family the typed, transport-independent command
//! vocabulary: what a command is called, which arguments it accepts, and what
//! result it returns. Every boundary above it - the command line, the local
//! protocol, the author transport, the workflow server - reads those answers
//! from here rather than defining its own, which is the only way they cannot
//! drift apart.
//!
//! The two closed enums every boundary above this crate speaks in live in the
//! `catalog` leaf beside the registry that describes them, because a family
//! root declares its children and nothing else.

pub mod add_component;
pub mod artifact;
pub mod canonical_json;
pub mod catalog;
pub mod command_identity;
pub mod component_resource_type;
pub mod create_page;
pub mod discovery_budget;
pub mod download_content_package;
pub mod find_assets_by_metadata;
pub mod find_assets_referenced_by_page;
pub mod find_pages_by_template;
pub mod find_pages_containing_phrase;
pub mod find_pages_using_components;
pub mod inspect_open_service_gateway_initiative_configuration;
pub mod load_content_as_javascript_object_notation;
pub mod package_selection;
pub mod property_value;
pub mod query_paths;
pub mod replicate_content;
pub mod repository_path;
pub mod result_window;
pub mod schema;
pub mod search_predicate;
