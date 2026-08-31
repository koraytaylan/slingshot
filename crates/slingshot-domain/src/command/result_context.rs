//! Whether one result answers the request that produced it.
//!
//! One implementation per command and nothing else in this file. Sixty-four of
//! them beside the registry would bury the registry; sixty-four of them in each
//! command's own leaf would make "what does this contract actually check" a
//! question with sixty-four answers to go and find.
//!
//! Most compare a path or an identifier the result echoes. A few have nothing to
//! compare, and say so rather than inventing a comparison: a listing of every
//! replication agent, of every queue, or of the whole resource mapping is
//! determined by nothing in its own request, so there is no fact in it that could
//! belong to another one.

/// Whether one result answers the request that produced it.
///
/// Implemented once per command pair so the registry's dispatch stays one rule
/// rather than twelve. What "answers" means is each command's own business:
/// most compare a path or an identifier the result echoes, and one has nothing
/// to compare.
pub trait AnswersCommand {
    /// The command this result answers.
    type Asked;

    /// Returns whether this result answers `asked`.
    fn answers(&self, asked: &Self::Asked) -> bool;
}

impl AnswersCommand for crate::command::add_component::AddComponentResult {
    type Asked = crate::command::add_component::AddComponentCommand;

    fn answers(&self, asked: &Self::Asked) -> bool {
        self.require_answers(asked).is_ok()
    }
}

impl AnswersCommand for crate::command::create_page::CreatePageResult {
    type Asked = crate::command::create_page::CreatePageCommand;

    fn answers(&self, asked: &Self::Asked) -> bool {
        self.require_answers(asked).is_ok()
    }
}

impl AnswersCommand for crate::command::download_content_package::DownloadContentPackageResult {
    type Asked = crate::command::download_content_package::DownloadContentPackageCommand;

    fn answers(&self, asked: &Self::Asked) -> bool {
        self.require_answers(asked).is_ok()
    }
}

impl AnswersCommand for crate::command::find_assets_by_metadata::FindAssetsByMetadataResult {
    type Asked = crate::command::find_assets_by_metadata::FindAssetsByMetadataCommand;

    fn answers(&self, asked: &Self::Asked) -> bool {
        self.require_answers(asked).is_ok()
    }
}

impl AnswersCommand
    for crate::command::find_assets_referenced_by_page::FindAssetsReferencedByPageResult
{
    type Asked = crate::command::find_assets_referenced_by_page::FindAssetsReferencedByPageCommand;

    fn answers(&self, asked: &Self::Asked) -> bool {
        self.require_answers(asked).is_ok()
    }
}

impl AnswersCommand for crate::command::find_pages_by_template::FindPagesByTemplateResult {
    type Asked = crate::command::find_pages_by_template::FindPagesByTemplateCommand;

    fn answers(&self, asked: &Self::Asked) -> bool {
        self.require_answers(asked).is_ok()
    }
}

impl AnswersCommand
    for crate::command::find_pages_containing_phrase::FindPagesContainingPhraseResult
{
    type Asked = crate::command::find_pages_containing_phrase::FindPagesContainingPhraseCommand;

    fn answers(&self, asked: &Self::Asked) -> bool {
        self.require_answers(asked).is_ok()
    }
}

impl AnswersCommand
    for crate::command::find_pages_using_components::FindPagesUsingComponentsResult
{
    type Asked = crate::command::find_pages_using_components::FindPagesUsingComponentsCommand;

    fn answers(&self, asked: &Self::Asked) -> bool {
        self.require_answers(asked).is_ok()
    }
}

impl AnswersCommand for crate::command::inspect_open_service_gateway_initiative_configuration::InspectOpenServiceGatewayInitiativeConfigurationResult {
    type Asked = crate::command::inspect_open_service_gateway_initiative_configuration::InspectOpenServiceGatewayInitiativeConfigurationCommand;

    fn answers(&self, asked: &Self::Asked) -> bool {
        self.require_answers(asked).is_ok()
    }
}

impl AnswersCommand
    for crate::command::load_content_as_javascript_object_notation::LoadContentAsJavaScriptObjectNotationResult
{
    type Asked =
        crate::command::load_content_as_javascript_object_notation::LoadContentAsJavaScriptObjectNotationCommand;

    fn answers(&self, asked: &Self::Asked) -> bool {
        self.require_answers(asked).is_ok()
    }
}

impl AnswersCommand for crate::command::query_paths::QueryPathsResult {
    type Asked = crate::command::query_paths::QueryPathsCommand;

    fn answers(&self, asked: &Self::Asked) -> bool {
        self.require_answers(asked).is_ok()
    }
}

impl AnswersCommand for crate::command::replicate_content::ReplicateContentResult {
    type Asked = crate::command::replicate_content::ReplicateContentCommand;

    fn answers(&self, asked: &Self::Asked) -> bool {
        let _ = asked;
        true
    }
}

impl AnswersCommand for crate::command::group_membership::AddGroupMemberResult {
    type Asked = crate::command::group_membership::AddGroupMemberCommand;

    fn answers(&self, asked: &Self::Asked) -> bool {
        self.require_answers(asked).is_ok()
    }
}

impl AnswersCommand for crate::command::cancel_sling_job::CancelSlingJobResult {
    type Asked = crate::command::cancel_sling_job::CancelSlingJobCommand;

    fn answers(&self, asked: &Self::Asked) -> bool {
        self.require_answers(asked).is_ok()
    }
}

impl AnswersCommand for crate::command::create_asset::CreateAssetResult {
    type Asked = crate::command::create_asset::CreateAssetCommand;

    fn answers(&self, asked: &Self::Asked) -> bool {
        self.require_answers(asked).is_ok()
    }
}

impl AnswersCommand for crate::command::create_asset_folder::CreateAssetFolderResult {
    type Asked = crate::command::create_asset_folder::CreateAssetFolderCommand;

    fn answers(&self, asked: &Self::Asked) -> bool {
        self.require_answers(asked).is_ok()
    }
}

impl AnswersCommand for crate::command::create_content_fragment::CreateContentFragmentResult {
    type Asked = crate::command::create_content_fragment::CreateContentFragmentCommand;

    fn answers(&self, asked: &Self::Asked) -> bool {
        self.require_answers(asked).is_ok()
    }
}

impl AnswersCommand for crate::command::create_experience_fragment::CreateExperienceFragmentResult {
    type Asked = crate::command::create_experience_fragment::CreateExperienceFragmentCommand;

    fn answers(&self, asked: &Self::Asked) -> bool {
        self.require_answers(asked).is_ok()
    }
}

impl AnswersCommand for crate::command::create_authorizable::CreateGroupResult {
    type Asked = crate::command::create_authorizable::CreateGroupCommand;

    fn answers(&self, asked: &Self::Asked) -> bool {
        self.require_answers(asked).is_ok()
    }
}

impl AnswersCommand for crate::command::create_authorizable::CreateUserResult {
    type Asked = crate::command::create_authorizable::CreateUserCommand;

    fn answers(&self, asked: &Self::Asked) -> bool {
        self.require_answers(asked).is_ok()
    }
}

impl AnswersCommand for crate::command::delete_asset::DeleteAssetResult {
    type Asked = crate::command::delete_asset::DeleteAssetCommand;

    fn answers(&self, asked: &Self::Asked) -> bool {
        self.require_answers(asked).is_ok()
    }
}

impl AnswersCommand for crate::command::delete_authorizable::DeleteAuthorizableResult {
    type Asked = crate::command::delete_authorizable::DeleteAuthorizableCommand;

    fn answers(&self, asked: &Self::Asked) -> bool {
        self.require_answers(asked).is_ok()
    }
}

impl AnswersCommand for crate::command::delete_component::DeleteComponentResult {
    type Asked = crate::command::delete_component::DeleteComponentCommand;

    fn answers(&self, asked: &Self::Asked) -> bool {
        self.require_answers(asked).is_ok()
    }
}

impl AnswersCommand for crate::command::delete_content_fragment::DeleteContentFragmentResult {
    type Asked = crate::command::delete_content_fragment::DeleteContentFragmentCommand;

    fn answers(&self, asked: &Self::Asked) -> bool {
        self.require_answers(asked).is_ok()
    }
}

impl AnswersCommand for crate::command::delete_experience_fragment::DeleteExperienceFragmentResult {
    type Asked = crate::command::delete_experience_fragment::DeleteExperienceFragmentCommand;

    fn answers(&self, asked: &Self::Asked) -> bool {
        self.require_answers(asked).is_ok()
    }
}

impl AnswersCommand for crate::command::delete_open_service_gateway_initiative_configuration::DeleteOpenServiceGatewayInitiativeConfigurationResult {
    type Asked = crate::command::delete_open_service_gateway_initiative_configuration::DeleteOpenServiceGatewayInitiativeConfigurationCommand;

    fn answers(&self, asked: &Self::Asked) -> bool {
        self.require_answers(asked).is_ok()
    }
}

impl AnswersCommand for crate::command::delete_page::DeletePageResult {
    type Asked = crate::command::delete_page::DeletePageCommand;

    fn answers(&self, asked: &Self::Asked) -> bool {
        self.require_answers(asked).is_ok()
    }
}

impl AnswersCommand for crate::command::find_open_service_gateway_initiative_configurations::FindOpenServiceGatewayInitiativeConfigurationsResult {
    type Asked = crate::command::find_open_service_gateway_initiative_configurations::FindOpenServiceGatewayInitiativeConfigurationsCommand;

    fn answers(&self, asked: &Self::Asked) -> bool {
        self.require_answers(asked).is_ok()
    }
}

impl AnswersCommand for crate::command::find_sling_jobs::FindSlingJobsResult {
    type Asked = crate::command::find_sling_jobs::FindSlingJobsCommand;

    fn answers(&self, asked: &Self::Asked) -> bool {
        self.require_answers(asked).is_ok()
    }
}

impl AnswersCommand for crate::command::find_workflow_instances::FindWorkflowInstancesResult {
    type Asked = crate::command::find_workflow_instances::FindWorkflowInstancesCommand;

    fn answers(&self, asked: &Self::Asked) -> bool {
        self.require_answers(asked).is_ok()
    }
}

impl AnswersCommand for crate::command::flush_replication_queue::FlushReplicationQueueResult {
    type Asked = crate::command::flush_replication_queue::FlushReplicationQueueCommand;

    fn answers(&self, asked: &Self::Asked) -> bool {
        self.require_answers(asked).is_ok()
    }
}

impl AnswersCommand for crate::command::replication_agent::InspectReplicationAgentResult {
    type Asked = crate::command::replication_agent::InspectReplicationAgentCommand;

    fn answers(&self, asked: &Self::Asked) -> bool {
        self.require_answers(asked).is_ok()
    }
}

impl AnswersCommand for crate::command::inspect_replication_queue::InspectReplicationQueueResult {
    type Asked = crate::command::inspect_replication_queue::InspectReplicationQueueCommand;

    fn answers(&self, asked: &Self::Asked) -> bool {
        let _ = asked;
        true
    }
}

impl AnswersCommand for crate::command::inspect_sling_job::InspectSlingJobResult {
    type Asked = crate::command::inspect_sling_job::InspectSlingJobCommand;

    fn answers(&self, asked: &Self::Asked) -> bool {
        self.require_answers(asked).is_ok()
    }
}

impl AnswersCommand for crate::command::inspect_workflow_instance::InspectWorkflowInstanceResult {
    type Asked = crate::command::inspect_workflow_instance::InspectWorkflowInstanceCommand;

    fn answers(&self, asked: &Self::Asked) -> bool {
        self.require_answers(asked).is_ok()
    }
}

impl AnswersCommand for crate::command::list_asset_renditions::ListAssetRenditionsResult {
    type Asked = crate::command::list_asset_renditions::ListAssetRenditionsCommand;

    fn answers(&self, asked: &Self::Asked) -> bool {
        self.require_answers(asked).is_ok()
    }
}

impl AnswersCommand for crate::command::list_child_pages::ListChildPagesResult {
    type Asked = crate::command::list_child_pages::ListChildPagesCommand;

    fn answers(&self, asked: &Self::Asked) -> bool {
        self.require_answers(asked).is_ok()
    }
}

impl AnswersCommand for crate::command::list_group_members::ListGroupMembersResult {
    type Asked = crate::command::list_group_members::ListGroupMembersCommand;

    fn answers(&self, asked: &Self::Asked) -> bool {
        self.require_answers(asked).is_ok()
    }
}

impl AnswersCommand for crate::command::list_open_service_gateway_initiative_bundles::ListOpenServiceGatewayInitiativeBundlesResult {
    type Asked = crate::command::list_open_service_gateway_initiative_bundles::ListOpenServiceGatewayInitiativeBundlesCommand;

    fn answers(&self, asked: &Self::Asked) -> bool {
        self.require_answers(asked).is_ok()
    }
}

impl AnswersCommand for crate::command::list_open_service_gateway_initiative_components::ListOpenServiceGatewayInitiativeComponentsResult {
    type Asked = crate::command::list_open_service_gateway_initiative_components::ListOpenServiceGatewayInitiativeComponentsCommand;

    fn answers(&self, asked: &Self::Asked) -> bool {
        self.require_answers(asked).is_ok()
    }
}

impl AnswersCommand for crate::command::replication_agent::ListReplicationAgentsResult {
    type Asked = crate::command::replication_agent::ListReplicationAgentsCommand;

    fn answers(&self, asked: &Self::Asked) -> bool {
        let _ = asked;
        true
    }
}

impl AnswersCommand for crate::command::list_resource_mappings::ListResourceMappingsResult {
    type Asked = crate::command::list_resource_mappings::ListResourceMappingsCommand;

    fn answers(&self, asked: &Self::Asked) -> bool {
        let _ = asked;
        true
    }
}

impl AnswersCommand for crate::command::list_sling_job_queues::ListSlingJobQueuesResult {
    type Asked = crate::command::list_sling_job_queues::ListSlingJobQueuesCommand;

    fn answers(&self, asked: &Self::Asked) -> bool {
        let _ = asked;
        true
    }
}

impl AnswersCommand for crate::command::list_workflow_models::ListWorkflowModelsResult {
    type Asked = crate::command::list_workflow_models::ListWorkflowModelsCommand;

    fn answers(&self, asked: &Self::Asked) -> bool {
        self.require_answers(asked).is_ok()
    }
}

impl AnswersCommand for crate::command::resource_resolution::MapResourcePathResult {
    type Asked = crate::command::resource_resolution::MapResourcePathCommand;

    fn answers(&self, asked: &Self::Asked) -> bool {
        self.require_answers(asked).is_ok()
    }
}

impl AnswersCommand for crate::command::move_asset::MoveAssetResult {
    type Asked = crate::command::move_asset::MoveAssetCommand;

    fn answers(&self, asked: &Self::Asked) -> bool {
        self.require_answers(asked).is_ok()
    }
}

impl AnswersCommand for crate::command::move_page::MovePageResult {
    type Asked = crate::command::move_page::MovePageCommand;

    fn answers(&self, asked: &Self::Asked) -> bool {
        self.require_answers(asked).is_ok()
    }
}

impl AnswersCommand for crate::command::read_content_fragment::ReadContentFragmentResult {
    type Asked = crate::command::read_content_fragment::ReadContentFragmentCommand;

    fn answers(&self, asked: &Self::Asked) -> bool {
        self.require_answers(asked).is_ok()
    }
}

impl AnswersCommand for crate::command::group_membership::RemoveGroupMemberResult {
    type Asked = crate::command::group_membership::RemoveGroupMemberCommand;

    fn answers(&self, asked: &Self::Asked) -> bool {
        self.require_answers(asked).is_ok()
    }
}

impl AnswersCommand for crate::command::reorder_component::ReorderComponentResult {
    type Asked = crate::command::reorder_component::ReorderComponentCommand;

    fn answers(&self, asked: &Self::Asked) -> bool {
        self.require_answers(asked).is_ok()
    }
}

impl AnswersCommand for crate::command::resource_resolution::ResolveResourcePathResult {
    type Asked = crate::command::resource_resolution::ResolveResourcePathCommand;

    fn answers(&self, asked: &Self::Asked) -> bool {
        self.require_answers(asked).is_ok()
    }
}

impl AnswersCommand
    for crate::command::retry_replication_queue_entry::RetryReplicationQueueEntryResult
{
    type Asked = crate::command::retry_replication_queue_entry::RetryReplicationQueueEntryCommand;

    fn answers(&self, asked: &Self::Asked) -> bool {
        self.require_answers(asked).is_ok()
    }
}

impl AnswersCommand for crate::command::set_open_service_gateway_initiative_bundle_state::SetOpenServiceGatewayInitiativeBundleStateResult {
    type Asked = crate::command::set_open_service_gateway_initiative_bundle_state::SetOpenServiceGatewayInitiativeBundleStateCommand;

    fn answers(&self, asked: &Self::Asked) -> bool {
        self.require_answers(asked).is_ok()
    }
}

impl AnswersCommand for crate::command::set_user_disabled::SetUserDisabledResult {
    type Asked = crate::command::set_user_disabled::SetUserDisabledCommand;

    fn answers(&self, asked: &Self::Asked) -> bool {
        self.require_answers(asked).is_ok()
    }
}

impl AnswersCommand
    for crate::command::set_workflow_instance_suspension::SetWorkflowInstanceSuspensionResult
{
    type Asked =
        crate::command::set_workflow_instance_suspension::SetWorkflowInstanceSuspensionCommand;

    fn answers(&self, asked: &Self::Asked) -> bool {
        self.require_answers(asked).is_ok()
    }
}

impl AnswersCommand for crate::command::start_workflow::StartWorkflowResult {
    type Asked = crate::command::start_workflow::StartWorkflowCommand;

    fn answers(&self, asked: &Self::Asked) -> bool {
        self.require_answers(asked).is_ok()
    }
}

impl AnswersCommand
    for crate::command::terminate_workflow_instance::TerminateWorkflowInstanceResult
{
    type Asked = crate::command::terminate_workflow_instance::TerminateWorkflowInstanceCommand;

    fn answers(&self, asked: &Self::Asked) -> bool {
        self.require_answers(asked).is_ok()
    }
}

impl AnswersCommand for crate::command::update_asset_metadata::UpdateAssetMetadataResult {
    type Asked = crate::command::update_asset_metadata::UpdateAssetMetadataCommand;

    fn answers(&self, asked: &Self::Asked) -> bool {
        self.require_answers(asked).is_ok()
    }
}

impl AnswersCommand for crate::command::update_component::UpdateComponentResult {
    type Asked = crate::command::update_component::UpdateComponentCommand;

    fn answers(&self, asked: &Self::Asked) -> bool {
        self.require_answers(asked).is_ok()
    }
}

impl AnswersCommand for crate::command::update_content_fragment::UpdateContentFragmentResult {
    type Asked = crate::command::update_content_fragment::UpdateContentFragmentCommand;

    fn answers(&self, asked: &Self::Asked) -> bool {
        self.require_answers(asked).is_ok()
    }
}

impl AnswersCommand for crate::command::update_experience_fragment::UpdateExperienceFragmentResult {
    type Asked = crate::command::update_experience_fragment::UpdateExperienceFragmentCommand;

    fn answers(&self, asked: &Self::Asked) -> bool {
        self.require_answers(asked).is_ok()
    }
}

impl AnswersCommand for crate::command::update_open_service_gateway_initiative_configuration::UpdateOpenServiceGatewayInitiativeConfigurationResult {
    type Asked = crate::command::update_open_service_gateway_initiative_configuration::UpdateOpenServiceGatewayInitiativeConfigurationCommand;

    fn answers(&self, asked: &Self::Asked) -> bool {
        self.require_answers(asked).is_ok()
    }
}

impl AnswersCommand for crate::command::update_page::UpdatePageResult {
    type Asked = crate::command::update_page::UpdatePageCommand;

    fn answers(&self, asked: &Self::Asked) -> bool {
        self.require_answers(asked).is_ok()
    }
}

impl AnswersCommand for crate::command::update_user_profile::UpdateUserProfileResult {
    type Asked = crate::command::update_user_profile::UpdateUserProfileCommand;

    fn answers(&self, asked: &Self::Asked) -> bool {
        self.require_answers(asked).is_ok()
    }
}
