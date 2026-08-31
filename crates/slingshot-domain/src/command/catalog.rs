//! The one place a command's safety, effect, and identity are written down.
//!
//! Everything a presentation needs to decide how to treat a command comes from
//! here, and from nowhere else. The three classifications are data in a closed
//! sixty-four-row table - written in `classification`, beside the families the
//! rows describe - not something inferred from a command's name, its result
//! size, or whether it publishes an artifact. A name that reads like a read is
//! not evidence, and a command that writes a file is not thereby a write.
//!
//! The distinctions are narrower than they look:
//!
//! - `Read` means no state the author retains after the command returns is
//!   different because the command ran. Operation bookkeeping, artifact
//!   publication, and evaluation cost do not make it a `Write`, which is why
//!   loading and packaging stay reads. Retained state is wider than content: a
//!   bundle's lifecycle state, a workflow instance's state, a job's
//!   disposition, an authorizable's existence, and a replication queue's
//!   contents are all retained, so changing any of them is a `Write`.
//! - `Destructive` means a success can replace or end something that was
//!   already visible or already in effect. Refusing an existing target is not
//!   destructive; replacing what a publisher is serving, stopping an active
//!   bundle, terminating a running instance, or emptying a queue is.
//! - `IntrinsicallyIdempotent` means running it twice is running it once. It is
//!   the only source of both the idempotency hint and the operation-key
//!   requirement, so the two can never disagree.
//!
//! The two closed enums are here too, beside the table that describes them.
//! They are parallel on purpose: one variant of [`Command`] answers with one
//! variant of [`CommandResult`], and [`validate_result_for_command`] refuses a
//! pair that does not match before anything is persisted or forwarded.
//!
//! What that validation can and cannot do is worth being exact about. It
//! compares the variant, and every fact a result echoes back or derives from
//! the request - the loaded path, the configuration identifier, the anchor a
//! failure names, the counts a replication reports, the file name a package
//! suggests, the target a mutation computed. It does not decode a continuation
//! token, re-execute repository semantics, or tell apart two results of the
//! same request shape that expose no differing fact. That last case is real,
//! and it is Plan 0005's authenticated submitted-command digest to close rather
//! than something this crate can pretend to.
//!
//! Loading and packaging are the interesting rows: `Read`, `NonDestructive`,
//! and not idempotent, because each may publish a retained artifact and a
//! second run without the caller's operation key would create a duplicate
//! rather than reuse it.

use serde::{Deserialize, Serialize};

use crate::command::artifact::ArtifactSlotDeclaration;
use crate::command::canonical_json::{canonical_digest, write_canonical};
use crate::command::classification::{CLASSIFICATIONS, ClassificationRow, DISCOVERY_FAILURES};
use crate::command::command_identity::{CommandContract, INITIAL_COMMAND_VERSION};
use crate::command::result_context::AnswersCommand;
use crate::command::schema::{COMMAND_WIRE_NAMES, SchemaRole, command_schema};

/// Whether a command changes content.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum AccessClassification {
    /// It changes no repository or replicated content.
    Read,
    /// It can change authored or replicated content.
    Write,
}

/// Whether a command can replace what was already visible.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum DestructiveClassification {
    /// A success can replace an already visible content state.
    Destructive,
    /// It observes, packages, or refuses an existing target instead.
    NonDestructive,
}

/// Whether running a command twice is running it once.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum IntrinsicIdempotencyClassification {
    /// Running it twice is running it once.
    IntrinsicallyIdempotent,
    /// It is not, so the caller supplies an operation key.
    NotIntrinsicallyIdempotent,
}

impl IntrinsicIdempotencyClassification {
    /// Returns the Model Context Protocol idempotency hint.
    ///
    /// Exactly this column and nothing else. Deriving the hint from anything
    /// observable - a name, a result size, an artifact - is how two callers end
    /// up disagreeing about whether a retry is safe.
    #[must_use]
    pub fn idempotent_hint(self) -> bool {
        matches!(self, Self::IntrinsicallyIdempotent)
    }

    /// Returns whether the caller must supply an operation key.
    ///
    /// The complement of the hint, from the same column, so the two cannot
    /// disagree.
    #[must_use]
    pub fn requires_operation_key(self) -> bool {
        !self.idempotent_hint()
    }
}

impl AccessClassification {
    /// Returns the Model Context Protocol read-only hint.
    #[must_use]
    pub fn read_only_hint(self) -> bool {
        matches!(self, Self::Read)
    }
}

impl DestructiveClassification {
    /// Returns the Model Context Protocol destructive hint.
    #[must_use]
    pub fn destructive_hint(self) -> bool {
        matches!(self, Self::Destructive)
    }
}

/// Everything the registry knows about one command.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct CommandDescriptor {
    /// Whether it changes content.
    pub access: AccessClassification,
    /// Digest of the canonical argument schema.
    pub arguments_schema_sha256: String,
    /// Digest of the byte contract both schemas bind.
    pub canonical_json_contract_sha256: String,
    /// Digest of the canonical limits manifest.
    pub command_contract_limits_sha256: String,
    /// Version of the meaning this command implements.
    pub command_semantic_contract_version: String,
    /// Present-state description.
    pub description: String,
    /// Whether a success can replace visible content.
    pub destructive: DestructiveClassification,
    /// Failure categories this version allows, in order.
    pub failure_categories: Vec<String>,
    /// Whether running it twice is running it once.
    pub intrinsic_idempotency: IntrinsicIdempotencyClassification,
    /// Largest canonical success result, when one is named.
    pub maximum_result_bytes: u64,
    /// Digest of the canonical result schema.
    pub result_schema_sha256: String,
    /// Artifact slots this command declares, in order.
    pub remote_artifact_slots: Vec<ArtifactSlotDeclaration>,
    /// Human title.
    pub title: String,
    /// Stable name, which is also the sole capability name.
    pub wire_name: String,
}

impl CommandDescriptor {
    /// Returns whether this command and `other` are compatible.
    ///
    /// Every one of the five has to match. A match on one schema role, or on
    /// bytes produced under a different limits or version contract, is not
    /// compatibility - it is two systems agreeing about half of something.
    #[must_use]
    pub fn compatible_with(&self, other: &Self) -> bool {
        self.wire_name == other.wire_name
            && self.command_semantic_contract_version == other.command_semantic_contract_version
            && self.command_contract_limits_sha256 == other.command_contract_limits_sha256
            && self.arguments_schema_sha256 == other.arguments_schema_sha256
            && self.result_schema_sha256 == other.result_schema_sha256
    }
}

/// Every command this plan defines, with everything the registry knows.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(transparent)]
pub struct CommandCatalog {
    /// The descriptors, in ascending wire-name order.
    descriptors: Vec<CommandDescriptor>,
}

impl CommandCatalog {
    /// Returns the catalog.
    ///
    /// # Panics
    ///
    /// Panics when the table and the schema inventory disagree about which
    /// commands exist, which is a defect in this repository.
    #[must_use]
    pub fn published() -> Self {
        /// The catalog, built once.
        ///
        /// Building it writes one hundred and twenty-eight canonical schema
        /// documents and digests every one of them. Every boundary above this
        /// crate asks for the catalog while it parses a single invocation, so
        /// rebuilding it per question turned an executable's startup into
        /// arithmetic - noticeably, on the path where a signal has to reach an
        /// installed handler. It is built once and handed out as a copy.
        static PUBLISHED: std::sync::OnceLock<CommandCatalog> = std::sync::OnceLock::new();

        PUBLISHED.get_or_init(Self::build).clone()
    }

    /// Returns the catalog, built from the table and the schema inventory.
    ///
    /// # Panics
    ///
    /// Panics when the table and the schema inventory disagree about which
    /// commands exist, which is a defect in this repository.
    fn build() -> Self {
        let limits_digest = canonical_digest(
            &write_canonical(
                &serde_json::from_str(CommandContract::embedded_manifest())
                    .expect("the limits manifest is one value"),
            )
            .expect("the limits manifest is canonical"),
        );
        let contract_digest = crate::command::schema::canonical_contract_digest();
        let descriptors = CLASSIFICATIONS
            .iter()
            .map(|row| describe(row, &limits_digest, &contract_digest))
            .collect();
        Self { descriptors }
    }

    /// Returns the descriptors, in ascending wire-name order.
    #[must_use]
    pub fn descriptors(&self) -> &[CommandDescriptor] {
        &self.descriptors
    }

    /// Returns the descriptor for `wire_name`, when there is one.
    #[must_use]
    pub fn find(&self, wire_name: &str) -> Option<&CommandDescriptor> {
        self.descriptors.iter().find(|descriptor| descriptor.wire_name == wire_name)
    }
}

/// Returns the descriptor one table row describes.
fn describe(
    row: &ClassificationRow,
    limits_digest: &str,
    contract_digest: &str,
) -> CommandDescriptor {
    let limits = CommandContract::embedded();
    let digest = |role| {
        canonical_digest(
            &write_canonical(&command_schema(row.wire_name, role)).expect("a schema is canonical"),
        )
    };
    let shared = if row.discovery { DISCOVERY_FAILURES } else { &[] };
    let failure_categories: Vec<String> = shared
        .iter()
        .chain(row.failure_categories)
        .map(|category| (*category).to_owned())
        .collect();
    CommandDescriptor {
        access: row.access,
        arguments_schema_sha256: digest(SchemaRole::Arguments),
        canonical_json_contract_sha256: contract_digest.to_owned(),
        command_contract_limits_sha256: limits_digest.to_owned(),
        command_semantic_contract_version: INITIAL_COMMAND_VERSION.to_owned(),
        description: row.description.to_owned(),
        destructive: row.destructive,
        failure_categories,
        intrinsic_idempotency: row.intrinsic_idempotency,
        maximum_result_bytes: limits.limit(row.result_bytes_limit),
        remote_artifact_slots: declared_slots(row.wire_name),
        result_schema_sha256: digest(SchemaRole::Result),
        title: row.title.to_owned(),
        wire_name: row.wire_name.to_owned(),
    }
}

/// Returns the artifact slots one command declares.
///
/// Two commands declare one each and the other sixty-two declare none. A command
/// that declares no slot forbids one, so an empty list is a statement rather
/// than an omission.
fn declared_slots(wire_name: &str) -> Vec<ArtifactSlotDeclaration> {
    match wire_name {
        "load_content_as_json" => vec![ArtifactSlotDeclaration::loaded_content()],
        "download_content_package" => vec![ArtifactSlotDeclaration::content_package()],
        _ => Vec::new(),
    }
}

/// Returns whether the catalog and the schema inventory name the same commands.
#[must_use]
pub fn catalog_matches_schema_inventory() -> bool {
    let catalog = CommandCatalog::published();
    let names: Vec<&str> =
        catalog.descriptors().iter().map(|descriptor| descriptor.wire_name.as_str()).collect();
    names == COMMAND_WIRE_NAMES
}

/// Builds the two parallel enums and the one rule that pairs them.
///
/// Written as a macro because the alternative is three sixty-four-armed matches
/// that have to be kept in step by hand, and a sixty-fifth command would need
/// remembering in all three.
macro_rules! command_family {
    ($($(#[$attribute:meta])* $variant:ident, $wire:literal, $command:path, $result:path;)+) => {
        /// One request, in whichever shape the caller asked for.
        #[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
        #[serde(tag = "command", rename_all = "snake_case", deny_unknown_fields)]
        pub enum Command {
            $($(#[$attribute])* $variant($command),)+
        }

        /// One answer, in whichever shape its command produces.
        #[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
        #[serde(tag = "command", rename_all = "snake_case", deny_unknown_fields)]
        pub enum CommandResult {
            $($(#[$attribute])* $variant($result),)+
        }

        impl Command {
            /// Returns the stable wire name of this command.
            ///
            /// Also its sole capability name. There is no second alias to drift.
            #[must_use]
            pub fn wire_name(&self) -> &'static str {
                match self {
                    $(Self::$variant(_) => $wire,)+
                }
            }
        }

        impl CommandResult {
            /// Returns the stable wire name of the command this answers.
            #[must_use]
            pub fn wire_name(&self) -> &'static str {
                match self {
                    $(Self::$variant(_) => $wire,)+
                }
            }
        }

        /// Requires `result` to answer `command`.
        ///
        /// Variant first, then every fact the result echoes or derives. A
        /// same-variant substitution that exposes no differing fact passes
        /// here and is Plan 0005's authenticated digest to catch; this function
        /// does not pretend otherwise.
        ///
        /// # Errors
        ///
        /// Returns [`ResultContextFailure::VariantMismatch`] when the two are
        /// different commands and [`ResultContextFailure::RequestMismatch`]
        /// when an echoed fact belongs to another request.
        pub fn validate_result_for_command(
            command: &Command,
            result: &CommandResult,
        ) -> Result<(), ResultContextFailure> {
            match (command, result) {
                $((Command::$variant(asked), CommandResult::$variant(answered)) => {
                    if answered.answers(asked) {
                        Ok(())
                    } else {
                        Err(ResultContextFailure::RequestMismatch)
                    }
                })+
                _ => Err(ResultContextFailure::VariantMismatch),
            }
        }
    };
}

/// Why a result does not answer the command it was offered for.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum ResultContextFailure {
    /// The result is of another command entirely.
    #[error("a result answers the command it was produced for")]
    VariantMismatch,
    /// The result is the right shape and answers another request.
    #[error("a result echoes the facts its own request determined")]
    RequestMismatch,
}

command_family! {
    /// Add one component to a page.
    /// What adding a component produced.
    AddComponent, "add_component", crate::command::add_component::AddComponentCommand, crate::command::add_component::AddComponentResult;
    /// Ask the author to add group member.
    /// What add group member answered.
    AddGroupMember, "add_group_member", crate::command::group_membership::AddGroupMemberCommand, crate::command::group_membership::AddGroupMemberResult;
    /// Ask the author to cancel sling job.
    /// What cancel sling job answered.
    CancelSlingJob, "cancel_sling_job", crate::command::cancel_sling_job::CancelSlingJobCommand, crate::command::cancel_sling_job::CancelSlingJobResult;
    /// Ask the author to create asset.
    /// What create asset answered.
    CreateAsset, "create_asset", crate::command::create_asset::CreateAssetCommand, crate::command::create_asset::CreateAssetResult;
    /// Ask the author to create asset folder.
    /// What create asset folder answered.
    CreateAssetFolder, "create_asset_folder", crate::command::create_asset_folder::CreateAssetFolderCommand, crate::command::create_asset_folder::CreateAssetFolderResult;
    /// Ask the author to create content fragment.
    /// What create content fragment answered.
    CreateContentFragment, "create_content_fragment", crate::command::create_content_fragment::CreateContentFragmentCommand, crate::command::create_content_fragment::CreateContentFragmentResult;
    /// Ask the author to create experience fragment.
    /// What create experience fragment answered.
    CreateExperienceFragment, "create_experience_fragment", crate::command::create_experience_fragment::CreateExperienceFragmentCommand, crate::command::create_experience_fragment::CreateExperienceFragmentResult;
    /// Ask the author to create group.
    /// What create group answered.
    CreateGroup, "create_group", crate::command::create_authorizable::CreateGroupCommand, crate::command::create_authorizable::CreateGroupResult;
    /// Create one page from a template.
    /// What creating a page produced.
    CreatePage, "create_page", crate::command::create_page::CreatePageCommand, crate::command::create_page::CreatePageResult;
    /// Ask the author to create user.
    /// What create user answered.
    CreateUser, "create_user", crate::command::create_authorizable::CreateUserCommand, crate::command::create_authorizable::CreateUserResult;
    /// Ask the author to delete asset.
    /// What delete asset answered.
    DeleteAsset, "delete_asset", crate::command::delete_asset::DeleteAssetCommand, crate::command::delete_asset::DeleteAssetResult;
    /// Ask the author to delete authorizable.
    /// What delete authorizable answered.
    DeleteAuthorizable, "delete_authorizable", crate::command::delete_authorizable::DeleteAuthorizableCommand, crate::command::delete_authorizable::DeleteAuthorizableResult;
    /// Ask the author to delete component.
    /// What delete component answered.
    DeleteComponent, "delete_component", crate::command::delete_component::DeleteComponentCommand, crate::command::delete_component::DeleteComponentResult;
    /// Ask the author to delete content fragment.
    /// What delete content fragment answered.
    DeleteContentFragment, "delete_content_fragment", crate::command::delete_content_fragment::DeleteContentFragmentCommand, crate::command::delete_content_fragment::DeleteContentFragmentResult;
    /// Ask the author to delete experience fragment.
    /// What delete experience fragment answered.
    DeleteExperienceFragment, "delete_experience_fragment", crate::command::delete_experience_fragment::DeleteExperienceFragmentCommand, crate::command::delete_experience_fragment::DeleteExperienceFragmentResult;
    /// Ask the author to delete open service gateway initiative configuration.
    /// What delete open service gateway initiative configuration answered.
    DeleteOpenServiceGatewayInitiativeConfiguration, "delete_open_service_gateway_initiative_configuration", crate::command::delete_open_service_gateway_initiative_configuration::DeleteOpenServiceGatewayInitiativeConfigurationCommand, crate::command::delete_open_service_gateway_initiative_configuration::DeleteOpenServiceGatewayInitiativeConfigurationResult;
    /// Ask the author to delete page.
    /// What delete page answered.
    DeletePage, "delete_page", crate::command::delete_page::DeletePageCommand, crate::command::delete_page::DeletePageResult;
    /// Build one content package.
    /// What building a package produced.
    DownloadContentPackage, "download_content_package", crate::command::download_content_package::DownloadContentPackageCommand, crate::command::download_content_package::DownloadContentPackageResult;
    /// Find assets by their metadata.
    /// What the asset search found.
    FindAssetsByMetadata, "find_assets_by_metadata", crate::command::find_assets_by_metadata::FindAssetsByMetadataCommand, crate::command::find_assets_by_metadata::FindAssetsByMetadataResult;
    /// Find the assets one page refers to.
    /// What the reference search found.
    FindAssetsReferencedByPage, "find_assets_referenced_by_page", crate::command::find_assets_referenced_by_page::FindAssetsReferencedByPageCommand, crate::command::find_assets_referenced_by_page::FindAssetsReferencedByPageResult;
    /// Ask the author to find open service gateway initiative configurations.
    /// What find open service gateway initiative configurations answered.
    FindOpenServiceGatewayInitiativeConfigurations, "find_open_service_gateway_initiative_configurations", crate::command::find_open_service_gateway_initiative_configurations::FindOpenServiceGatewayInitiativeConfigurationsCommand, crate::command::find_open_service_gateway_initiative_configurations::FindOpenServiceGatewayInitiativeConfigurationsResult;
    /// Find pages built from one template.
    /// What the template search found.
    FindPagesByTemplate, "find_pages_by_template", crate::command::find_pages_by_template::FindPagesByTemplateCommand, crate::command::find_pages_by_template::FindPagesByTemplateResult;
    /// Find pages containing one phrase.
    /// What the phrase search found.
    FindPagesContainingPhrase, "find_pages_containing_phrase", crate::command::find_pages_containing_phrase::FindPagesContainingPhraseCommand, crate::command::find_pages_containing_phrase::FindPagesContainingPhraseResult;
    /// Find pages using particular components.
    /// What the component search found.
    FindPagesUsingComponents, "find_pages_using_components", crate::command::find_pages_using_components::FindPagesUsingComponentsCommand, crate::command::find_pages_using_components::FindPagesUsingComponentsResult;
    /// Ask the author to find sling jobs.
    /// What find sling jobs answered.
    FindSlingJobs, "find_sling_jobs", crate::command::find_sling_jobs::FindSlingJobsCommand, crate::command::find_sling_jobs::FindSlingJobsResult;
    /// Ask the author to find workflow instances.
    /// What find workflow instances answered.
    FindWorkflowInstances, "find_workflow_instances", crate::command::find_workflow_instances::FindWorkflowInstancesCommand, crate::command::find_workflow_instances::FindWorkflowInstancesResult;
    /// Ask the author to flush replication queue.
    /// What flush replication queue answered.
    FlushReplicationQueue, "flush_replication_queue", crate::command::flush_replication_queue::FlushReplicationQueueCommand, crate::command::flush_replication_queue::FlushReplicationQueueResult;
    /// Inspect one effective configuration.
    /// What the configuration inspection found.
    InspectOpenServiceGatewayInitiativeConfiguration, "inspect_open_service_gateway_initiative_configuration", crate::command::inspect_open_service_gateway_initiative_configuration::InspectOpenServiceGatewayInitiativeConfigurationCommand, crate::command::inspect_open_service_gateway_initiative_configuration::InspectOpenServiceGatewayInitiativeConfigurationResult;
    /// Ask the author to inspect replication agent.
    /// What inspect replication agent answered.
    InspectReplicationAgent, "inspect_replication_agent", crate::command::replication_agent::InspectReplicationAgentCommand, crate::command::replication_agent::InspectReplicationAgentResult;
    /// Ask the author to inspect replication queue.
    /// What inspect replication queue answered.
    InspectReplicationQueue, "inspect_replication_queue", crate::command::inspect_replication_queue::InspectReplicationQueueCommand, crate::command::inspect_replication_queue::InspectReplicationQueueResult;
    /// Ask the author to inspect sling job.
    /// What inspect sling job answered.
    InspectSlingJob, "inspect_sling_job", crate::command::inspect_sling_job::InspectSlingJobCommand, crate::command::inspect_sling_job::InspectSlingJobResult;
    /// Ask the author to inspect workflow instance.
    /// What inspect workflow instance answered.
    InspectWorkflowInstance, "inspect_workflow_instance", crate::command::inspect_workflow_instance::InspectWorkflowInstanceCommand, crate::command::inspect_workflow_instance::InspectWorkflowInstanceResult;
    /// Ask the author to list asset renditions.
    /// What list asset renditions answered.
    ListAssetRenditions, "list_asset_renditions", crate::command::list_asset_renditions::ListAssetRenditionsCommand, crate::command::list_asset_renditions::ListAssetRenditionsResult;
    /// Ask the author to list child pages.
    /// What list child pages answered.
    ListChildPages, "list_child_pages", crate::command::list_child_pages::ListChildPagesCommand, crate::command::list_child_pages::ListChildPagesResult;
    /// Ask the author to list group members.
    /// What list group members answered.
    ListGroupMembers, "list_group_members", crate::command::list_group_members::ListGroupMembersCommand, crate::command::list_group_members::ListGroupMembersResult;
    /// Ask the author to list open service gateway initiative bundles.
    /// What list open service gateway initiative bundles answered.
    ListOpenServiceGatewayInitiativeBundles, "list_open_service_gateway_initiative_bundles", crate::command::list_open_service_gateway_initiative_bundles::ListOpenServiceGatewayInitiativeBundlesCommand, crate::command::list_open_service_gateway_initiative_bundles::ListOpenServiceGatewayInitiativeBundlesResult;
    /// Ask the author to list open service gateway initiative components.
    /// What list open service gateway initiative components answered.
    ListOpenServiceGatewayInitiativeComponents, "list_open_service_gateway_initiative_components", crate::command::list_open_service_gateway_initiative_components::ListOpenServiceGatewayInitiativeComponentsCommand, crate::command::list_open_service_gateway_initiative_components::ListOpenServiceGatewayInitiativeComponentsResult;
    /// Ask the author to list replication agents.
    /// What list replication agents answered.
    ListReplicationAgents, "list_replication_agents", crate::command::replication_agent::ListReplicationAgentsCommand, crate::command::replication_agent::ListReplicationAgentsResult;
    /// Ask the author to list resource mappings.
    /// What list resource mappings answered.
    ListResourceMappings, "list_resource_mappings", crate::command::list_resource_mappings::ListResourceMappingsCommand, crate::command::list_resource_mappings::ListResourceMappingsResult;
    /// Ask the author to list sling job queues.
    /// What list sling job queues answered.
    ListSlingJobQueues, "list_sling_job_queues", crate::command::list_sling_job_queues::ListSlingJobQueuesCommand, crate::command::list_sling_job_queues::ListSlingJobQueuesResult;
    /// Ask the author to list workflow models.
    /// What list workflow models answered.
    ListWorkflowModels, "list_workflow_models", crate::command::list_workflow_models::ListWorkflowModelsCommand, crate::command::list_workflow_models::ListWorkflowModelsResult;
    /// Load one repository subtree.
    /// What the load produced.
    LoadContentAsJson, "load_content_as_json", crate::command::load_content_as_javascript_object_notation::LoadContentAsJavaScriptObjectNotationCommand, crate::command::load_content_as_javascript_object_notation::LoadContentAsJavaScriptObjectNotationResult;
    /// Ask the author to map resource path.
    /// What map resource path answered.
    MapResourcePath, "map_resource_path", crate::command::resource_resolution::MapResourcePathCommand, crate::command::resource_resolution::MapResourcePathResult;
    /// Ask the author to move asset.
    /// What move asset answered.
    MoveAsset, "move_asset", crate::command::move_asset::MoveAssetCommand, crate::command::move_asset::MoveAssetResult;
    /// Ask the author to move page.
    /// What move page answered.
    MovePage, "move_page", crate::command::move_page::MovePageCommand, crate::command::move_page::MovePageResult;
    /// Find nodes answering a structured question.
    /// What the query found.
    QueryPaths, "query_paths", crate::command::query_paths::QueryPathsCommand, crate::command::query_paths::QueryPathsResult;
    /// Ask the author to read content fragment.
    /// What read content fragment answered.
    ReadContentFragment, "read_content_fragment", crate::command::read_content_fragment::ReadContentFragmentCommand, crate::command::read_content_fragment::ReadContentFragmentResult;
    /// Ask the author to remove group member.
    /// What remove group member answered.
    RemoveGroupMember, "remove_group_member", crate::command::group_membership::RemoveGroupMemberCommand, crate::command::group_membership::RemoveGroupMemberResult;
    /// Ask the author to reorder component.
    /// What reorder component answered.
    ReorderComponent, "reorder_component", crate::command::reorder_component::ReorderComponentCommand, crate::command::reorder_component::ReorderComponentResult;
    /// Offer content to the replication service.
    /// What the replication admitted.
    ReplicateContent, "replicate_content", crate::command::replicate_content::ReplicateContentCommand, crate::command::replicate_content::ReplicateContentResult;
    /// Ask the author to resolve resource path.
    /// What resolve resource path answered.
    ResolveResourcePath, "resolve_resource_path", crate::command::resource_resolution::ResolveResourcePathCommand, crate::command::resource_resolution::ResolveResourcePathResult;
    /// Ask the author to retry replication queue entry.
    /// What retry replication queue entry answered.
    RetryReplicationQueueEntry, "retry_replication_queue_entry", crate::command::retry_replication_queue_entry::RetryReplicationQueueEntryCommand, crate::command::retry_replication_queue_entry::RetryReplicationQueueEntryResult;
    /// Ask the author to set open service gateway initiative bundle state.
    /// What set open service gateway initiative bundle state answered.
    SetOpenServiceGatewayInitiativeBundleState, "set_open_service_gateway_initiative_bundle_state", crate::command::set_open_service_gateway_initiative_bundle_state::SetOpenServiceGatewayInitiativeBundleStateCommand, crate::command::set_open_service_gateway_initiative_bundle_state::SetOpenServiceGatewayInitiativeBundleStateResult;
    /// Ask the author to set user disabled.
    /// What set user disabled answered.
    SetUserDisabled, "set_user_disabled", crate::command::set_user_disabled::SetUserDisabledCommand, crate::command::set_user_disabled::SetUserDisabledResult;
    /// Ask the author to set workflow instance suspension.
    /// What set workflow instance suspension answered.
    SetWorkflowInstanceSuspension, "set_workflow_instance_suspension", crate::command::set_workflow_instance_suspension::SetWorkflowInstanceSuspensionCommand, crate::command::set_workflow_instance_suspension::SetWorkflowInstanceSuspensionResult;
    /// Ask the author to start workflow.
    /// What start workflow answered.
    StartWorkflow, "start_workflow", crate::command::start_workflow::StartWorkflowCommand, crate::command::start_workflow::StartWorkflowResult;
    /// Ask the author to terminate workflow instance.
    /// What terminate workflow instance answered.
    TerminateWorkflowInstance, "terminate_workflow_instance", crate::command::terminate_workflow_instance::TerminateWorkflowInstanceCommand, crate::command::terminate_workflow_instance::TerminateWorkflowInstanceResult;
    /// Ask the author to update asset metadata.
    /// What update asset metadata answered.
    UpdateAssetMetadata, "update_asset_metadata", crate::command::update_asset_metadata::UpdateAssetMetadataCommand, crate::command::update_asset_metadata::UpdateAssetMetadataResult;
    /// Ask the author to update component.
    /// What update component answered.
    UpdateComponent, "update_component", crate::command::update_component::UpdateComponentCommand, crate::command::update_component::UpdateComponentResult;
    /// Ask the author to update content fragment.
    /// What update content fragment answered.
    UpdateContentFragment, "update_content_fragment", crate::command::update_content_fragment::UpdateContentFragmentCommand, crate::command::update_content_fragment::UpdateContentFragmentResult;
    /// Ask the author to update experience fragment.
    /// What update experience fragment answered.
    UpdateExperienceFragment, "update_experience_fragment", crate::command::update_experience_fragment::UpdateExperienceFragmentCommand, crate::command::update_experience_fragment::UpdateExperienceFragmentResult;
    /// Ask the author to update open service gateway initiative configuration.
    /// What update open service gateway initiative configuration answered.
    UpdateOpenServiceGatewayInitiativeConfiguration, "update_open_service_gateway_initiative_configuration", crate::command::update_open_service_gateway_initiative_configuration::UpdateOpenServiceGatewayInitiativeConfigurationCommand, crate::command::update_open_service_gateway_initiative_configuration::UpdateOpenServiceGatewayInitiativeConfigurationResult;
    /// Ask the author to update page.
    /// What update page answered.
    UpdatePage, "update_page", crate::command::update_page::UpdatePageCommand, crate::command::update_page::UpdatePageResult;
    /// Ask the author to update user profile.
    /// What update user profile answered.
    UpdateUserProfile, "update_user_profile", crate::command::update_user_profile::UpdateUserProfileCommand, crate::command::update_user_profile::UpdateUserProfileResult;
}
