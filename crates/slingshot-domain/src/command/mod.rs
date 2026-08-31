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
pub mod authorizable_identity;
pub mod cancel_sling_job;
pub mod canonical_json;
pub mod catalog;
pub mod command_identity;
pub mod component_resource_type;
pub mod content_fragment_element;
pub mod create_asset;
pub mod create_asset_folder;
pub mod create_authorizable;
pub mod create_content_fragment;
pub mod create_experience_fragment;
pub mod create_page;
pub mod delete_asset;
pub mod delete_authorizable;
pub mod delete_component;
pub mod delete_content_fragment;
pub mod delete_experience_fragment;
pub mod delete_open_service_gateway_initiative_configuration;
pub mod delete_page;
pub mod discovery_budget;
pub mod download_content_package;
pub mod find_assets_by_metadata;
pub mod find_assets_referenced_by_page;
pub mod find_open_service_gateway_initiative_configurations;
pub mod find_pages_by_template;
pub mod find_pages_containing_phrase;
pub mod find_pages_using_components;
pub mod find_sling_jobs;
pub mod find_workflow_instances;
pub mod flush_replication_queue;
pub mod group_membership;
pub mod inspect_open_service_gateway_initiative_configuration;
pub mod inspect_replication_queue;
pub mod inspect_sling_job;
pub mod inspect_workflow_instance;
pub mod list_asset_renditions;
pub mod list_child_pages;
pub mod list_group_members;
pub mod list_open_service_gateway_initiative_bundles;
pub mod list_open_service_gateway_initiative_components;
pub mod list_resource_mappings;
pub mod list_sling_job_queues;
pub mod list_workflow_models;
pub mod load_content_as_javascript_object_notation;
pub mod move_asset;
pub mod move_page;
pub mod operational_listing;
pub mod package_selection;
pub mod platform_service_identity;
pub mod process_identity;
pub mod property_value;
pub mod query_paths;
pub mod read_content_fragment;
pub mod reorder_component;
pub mod replicate_content;
pub mod replication_agent;
pub mod repository_path;
pub mod resource_mapping_entry;
pub mod resource_mutation;
pub mod resource_resolution;
pub mod result_window;
pub mod retry_replication_queue_entry;
pub mod schema;
pub mod search_predicate;
pub mod set_open_service_gateway_initiative_bundle_state;
pub mod set_user_disabled;
pub mod set_workflow_instance_suspension;
pub mod start_workflow;
pub mod terminate_workflow_instance;
pub mod update_asset_metadata;
pub mod update_component;
pub mod update_content_fragment;
pub mod update_experience_fragment;
pub mod update_open_service_gateway_initiative_configuration;
pub mod update_page;
pub mod update_user_profile;
