//! Bounded failure-envelope decoding. This is deliberately not a terminal
//! settlement decision: the selected command's refusal type must still validate
//! category, exact fields, request correlation and effect disposition.

use crate::structured_job_result::{
    ResultExpectation, maximum_agent_inline_result_bytes, maximum_document_bytes,
};
use slingshot_agent_protocol::terminal_failure::TerminalFailureDocument;

/// An opaque refusal that cannot expose remote failure details.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[error("the terminal failure document is not acceptable")]
pub struct TerminalFailureDecodeRefusal;

/// A closed, request-correlated listing, inspection or resolution refusal.
#[derive(Clone, PartialEq, Eq)]
pub struct ValidatedReadFailure { category: String }

impl core::fmt::Debug for ValidatedReadFailure {
    fn fmt(&self, formatter: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        formatter.write_str("ValidatedReadFailure([redacted])")
    }
}

impl ValidatedReadFailure {
    /// The category admitted by both the typed refusal and selected registry row.
    #[must_use]
    pub fn category(&self) -> &str { &self.category }
}

/// Validates listing and targeted inspection/resolution refusals without exposing private
/// identifiers, addresses, partial results or remote-selected dispositions.
pub fn decode_read_failure(
    body: &[u8], expectation: &ResultExpectation,
    command: &slingshot_domain::command::catalog::Command,
) -> Result<ValidatedReadFailure, TerminalFailureDecodeRefusal> {
    use slingshot_domain::command::catalog::{Command, CommandCatalog};
    require_installed_command(expectation, command)?;
    let document = decode_terminal_failure(body, expectation)?;
    let category = failure_category(&document)?;
    if !CommandCatalog::published().find(command.wire_name()).ok_or(TerminalFailureDecodeRefusal)?.failure_categories.contains(&category) {
        return Err(TerminalFailureDecodeRefusal);
    }
    let window = match command {
        Command::FindOpenServiceGatewayInitiativeConfigurations(command) => Some(&command.result_window),
        Command::FindSlingJobs(command) => Some(&command.result_window),
        Command::FindWorkflowInstances(command) => Some(&command.result_window),
        Command::ListOpenServiceGatewayInitiativeBundles(command) => Some(&command.result_window),
        Command::ListOpenServiceGatewayInitiativeComponents(command) => Some(&command.result_window),
        Command::ListReplicationAgents(command) => Some(&command.result_window),
        Command::ListResourceMappings(command) => Some(&command.result_window),
        Command::ListSlingJobQueues(command) => Some(&command.result_window),
        Command::ListWorkflowModels(command) => Some(&command.result_window),
        Command::ListChildPages(command) => Some(&command.result_window),
        Command::ListGroupMembers(command) => Some(&command.result_window),
        Command::ListAssetRenditions(command) => Some(&command.result_window),
        Command::InspectReplicationQueue(command) => Some(&command.result_window),
        _ => None,
    };
    if let Some(window) = window {
        let value: serde_json::Value = serde_json::from_str(&document.canonical_failure).map_err(|_| TerminalFailureDecodeRefusal)?;
        let members = value.as_object().ok_or(TerminalFailureDecodeRefusal)?;
        if category == "discovery_budget_exceeded" {
            if members.len() != 2 { return Err(TerminalFailureDecodeRefusal); }
            let _: slingshot_domain::command::discovery_budget::DiscoveryBudget = serde_json::from_value(value["budget"].clone()).map_err(|_| TerminalFailureDecodeRefusal)?;
            return Ok(ValidatedReadFailure { category });
        }
        if slingshot_domain::command::result_window::CONTINUATION_FAILURE_PRECEDENCE.contains(&category.as_str()) {
            if members.len() != 1 || !matches!(window, Some(slingshot_domain::command::result_window::ResultWindow::Continuation { .. })) { return Err(TerminalFailureDecodeRefusal); }
            return Ok(ValidatedReadFailure { category });
        }
    }
    match command {
        Command::ReadContentFragment(command) => {
            let refusal: slingshot_domain::command::read_content_fragment::ReadContentFragmentRefusal = serde_json::from_str(&document.canonical_failure).map_err(|_| TerminalFailureDecodeRefusal)?;
            refusal.require_answers(command).map_err(|_| TerminalFailureDecodeRefusal)?;
        }
        Command::FindOpenServiceGatewayInitiativeConfigurations(_) | Command::FindSlingJobs(_) | Command::FindWorkflowInstances(_) | Command::ListOpenServiceGatewayInitiativeBundles(_) | Command::ListOpenServiceGatewayInitiativeComponents(_) | Command::ListReplicationAgents(_) | Command::ListResourceMappings(_) | Command::ListSlingJobQueues(_) | Command::ListWorkflowModels(_) => {
            let refusal: slingshot_domain::command::operational_listing::InventoryRefusal = serde_json::from_str(&document.canonical_failure).map_err(|_| TerminalFailureDecodeRefusal)?;
            // Internally tagged unit variants require an explicit shape check:
            // serde can otherwise ignore extra members despite deny_unknown_fields.
            let received: serde_json::Value = serde_json::from_str(&document.canonical_failure).map_err(|_| TerminalFailureDecodeRefusal)?;
            if serde_json::to_value(refusal).map_err(|_| TerminalFailureDecodeRefusal)? != received {
                return Err(TerminalFailureDecodeRefusal);
            }
        }
        Command::ListChildPages(command) => {
            let refusal: slingshot_domain::command::query_paths::AnchorRefusal = serde_json::from_str(&document.canonical_failure).map_err(|_| TerminalFailureDecodeRefusal)?;
            if refusal.root_path() != &command.root_path { return Err(TerminalFailureDecodeRefusal); }
        }
        Command::ListGroupMembers(command) => {
            let refusal: slingshot_domain::command::list_group_members::ListGroupMembersRefusal = serde_json::from_str(&document.canonical_failure).map_err(|_| TerminalFailureDecodeRefusal)?;
            refusal.require_answers(command).map_err(|_| TerminalFailureDecodeRefusal)?;
        }
        Command::ListAssetRenditions(command) => {
            let refusal: slingshot_domain::command::list_asset_renditions::ListAssetRenditionsRefusal = serde_json::from_str(&document.canonical_failure).map_err(|_| TerminalFailureDecodeRefusal)?;
            refusal.require_answers(command).map_err(|_| TerminalFailureDecodeRefusal)?;
        }
        Command::InspectReplicationQueue(command) => {
            let refusal: slingshot_domain::command::inspect_replication_queue::InspectReplicationQueueRefusal = serde_json::from_str(&document.canonical_failure).map_err(|_| TerminalFailureDecodeRefusal)?;
            refusal.require_answers(command).map_err(|_| TerminalFailureDecodeRefusal)?;
        }
        Command::InspectSlingJob(command) => {
            let refusal: slingshot_domain::command::inspect_sling_job::InspectSlingJobRefusal = serde_json::from_str(&document.canonical_failure).map_err(|_| TerminalFailureDecodeRefusal)?;
            refusal.require_answers(command).map_err(|_| TerminalFailureDecodeRefusal)?;
        }
        Command::InspectWorkflowInstance(command) => {
            let refusal: slingshot_domain::command::inspect_workflow_instance::InspectWorkflowInstanceRefusal = serde_json::from_str(&document.canonical_failure).map_err(|_| TerminalFailureDecodeRefusal)?;
            refusal.require_answers(command).map_err(|_| TerminalFailureDecodeRefusal)?;
        }
        Command::InspectReplicationAgent(command) => {
            let refusal: slingshot_domain::command::replication_agent::ReplicationAgentRefusal = serde_json::from_str(&document.canonical_failure).map_err(|_| TerminalFailureDecodeRefusal)?;
            if refusal.agent_identifier != command.agent_identifier { return Err(TerminalFailureDecodeRefusal); }
        }
        Command::ResolveResourcePath(command) => {
            let refusal: slingshot_domain::command::resource_resolution::ResourceResolutionRefusal = serde_json::from_str(&document.canonical_failure).map_err(|_| TerminalFailureDecodeRefusal)?;
            if refusal.subject != command.request_address.as_text() { return Err(TerminalFailureDecodeRefusal); }
        }
        Command::MapResourcePath(command) => {
            let refusal: slingshot_domain::command::resource_resolution::ResourceResolutionRefusal = serde_json::from_str(&document.canonical_failure).map_err(|_| TerminalFailureDecodeRefusal)?;
            if refusal.subject != command.repository_path.as_text() { return Err(TerminalFailureDecodeRefusal); }
        }
        _ => return Err(TerminalFailureDecodeRefusal),
    }
    Ok(ValidatedReadFailure { category })
}

/// A request-correlated mutation or platform-control refusal. Repository locations
/// and arbitrary remote details are never exposed by this value.
#[derive(Clone, PartialEq, Eq)]
pub struct ValidatedMutationFailure {
    category: String,
    proves_no_effect: bool,
}

impl core::fmt::Debug for ValidatedMutationFailure {
    fn fmt(&self, formatter: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        formatter.write_str("ValidatedMutationFailure([redacted])")
    }
}

impl ValidatedMutationFailure {
    /// The category admitted by the installed command-specific refusal type.
    #[must_use]
    pub fn category(&self) -> &str { &self.category }
    /// False preserves mutation uncertainty and does not authorize replay.
    #[must_use]
    pub fn proves_no_effect(&self) -> bool { self.proves_no_effect }
}

/// Validates authoring, identity and operational mutations against their typed
/// request, including reference-policy, ordering and queue-expectation constraints.
pub fn decode_mutation_failure(
    body: &[u8], expectation: &ResultExpectation,
    command: &slingshot_domain::command::catalog::Command,
) -> Result<ValidatedMutationFailure, TerminalFailureDecodeRefusal> {
    use slingshot_domain::command::catalog::Command;
    require_installed_command(expectation, command)?;
    let document = decode_terminal_failure(body, expectation)?;
    macro_rules! decode {
        ($group:expr, $member:expr, $refusal:ty) => {{
            let refusal: $refusal = serde_json::from_str(&document.canonical_failure).map_err(|_| TerminalFailureDecodeRefusal)?;
            refusal.require_answers($group, $member).map_err(|_| TerminalFailureDecodeRefusal)?;
            refusal.proves_no_effect()
        }};
        ($command:expr, $refusal:ty) => {{
            let refusal: $refusal = serde_json::from_str(&document.canonical_failure).map_err(|_| TerminalFailureDecodeRefusal)?;
            refusal.require_answers($command).map_err(|_| TerminalFailureDecodeRefusal)?;
            refusal.proves_no_effect()
        }};
    }
    let proves_no_effect = match command {
        Command::UpdateOpenServiceGatewayInitiativeConfiguration(command) => decode!(command, slingshot_domain::command::update_open_service_gateway_initiative_configuration::UpdateOpenServiceGatewayInitiativeConfigurationRefusal),
        Command::DeleteOpenServiceGatewayInitiativeConfiguration(command) => decode!(command, slingshot_domain::command::delete_open_service_gateway_initiative_configuration::DeleteOpenServiceGatewayInitiativeConfigurationRefusal),
        Command::SetOpenServiceGatewayInitiativeBundleState(command) => decode!(command, slingshot_domain::command::set_open_service_gateway_initiative_bundle_state::SetOpenServiceGatewayInitiativeBundleStateRefusal),
        Command::CancelSlingJob(command) => decode!(command, slingshot_domain::command::cancel_sling_job::CancelSlingJobRefusal),
        Command::StartWorkflow(command) => decode!(command, slingshot_domain::command::start_workflow::StartWorkflowRefusal),
        Command::TerminateWorkflowInstance(command) => decode!(command, slingshot_domain::command::terminate_workflow_instance::TerminateWorkflowInstanceRefusal),
        Command::SetWorkflowInstanceSuspension(command) => decode!(command, slingshot_domain::command::set_workflow_instance_suspension::SetWorkflowInstanceSuspensionRefusal),
        Command::FlushReplicationQueue(command) => decode!(command, slingshot_domain::command::flush_replication_queue::FlushReplicationQueueRefusal),
        Command::RetryReplicationQueueEntry(command) => decode!(command, slingshot_domain::command::retry_replication_queue_entry::RetryReplicationQueueEntryRefusal),
        Command::CreateUser(command) => decode!(&command.authorizable_identifier, slingshot_domain::command::create_authorizable::CreateAuthorizableRefusal),
        Command::CreateGroup(command) => decode!(&command.authorizable_identifier, slingshot_domain::command::create_authorizable::CreateAuthorizableRefusal),
        Command::DeleteAuthorizable(command) => decode!(command, slingshot_domain::command::delete_authorizable::DeleteAuthorizableRefusal),
        Command::UpdateUserProfile(command) => decode!(command, slingshot_domain::command::update_user_profile::UpdateUserProfileRefusal),
        Command::SetUserDisabled(command) => decode!(command, slingshot_domain::command::set_user_disabled::SetUserDisabledRefusal),
        Command::AddGroupMember(command) => decode!(&command.group_identifier, &command.member_identifier, slingshot_domain::command::group_membership::GroupMembershipRefusal),
        Command::RemoveGroupMember(command) => decode!(&command.group_identifier, &command.member_identifier, slingshot_domain::command::group_membership::GroupMembershipRefusal),
        Command::CreateContentFragment(command) => decode!(command, slingshot_domain::command::create_content_fragment::CreateContentFragmentRefusal),
        Command::UpdateContentFragment(command) => decode!(command, slingshot_domain::command::update_content_fragment::UpdateContentFragmentRefusal),
        Command::DeleteContentFragment(command) => decode!(command, slingshot_domain::command::delete_content_fragment::DeleteContentFragmentRefusal),
        Command::CreateExperienceFragment(command) => decode!(command, slingshot_domain::command::create_experience_fragment::CreateExperienceFragmentRefusal),
        Command::UpdateExperienceFragment(command) => decode!(command, slingshot_domain::command::update_experience_fragment::UpdateExperienceFragmentRefusal),
        Command::DeleteExperienceFragment(command) => decode!(command, slingshot_domain::command::delete_experience_fragment::DeleteExperienceFragmentRefusal),
        Command::CreateAsset(command) => decode!(command, slingshot_domain::command::create_asset::CreateAssetRefusal),
        Command::CreateAssetFolder(command) => decode!(command, slingshot_domain::command::create_asset_folder::CreateAssetFolderRefusal),
        Command::MoveAsset(command) => decode!(command, slingshot_domain::command::move_asset::MoveAssetRefusal),
        Command::DeleteAsset(command) => decode!(command, slingshot_domain::command::delete_asset::DeleteAssetRefusal),
        Command::UpdateAssetMetadata(command) => decode!(command, slingshot_domain::command::update_asset_metadata::UpdateAssetMetadataRefusal),
        Command::UpdatePage(command) => decode!(command, slingshot_domain::command::update_page::UpdatePageRefusal),
        Command::MovePage(command) => decode!(command, slingshot_domain::command::move_page::MovePageRefusal),
        Command::DeletePage(command) => decode!(command, slingshot_domain::command::delete_page::DeletePageRefusal),
        Command::UpdateComponent(command) => decode!(command, slingshot_domain::command::update_component::UpdateComponentRefusal),
        Command::DeleteComponent(command) => decode!(command, slingshot_domain::command::delete_component::DeleteComponentRefusal),
        Command::ReorderComponent(command) => decode!(command, slingshot_domain::command::reorder_component::ReorderComponentRefusal),
        _ => return Err(TerminalFailureDecodeRefusal),
    };
    Ok(ValidatedMutationFailure { category: failure_category(&document)?, proves_no_effect })
}

/// Locally derived replication effect branch, never accepted from the wire.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReplicationFailureEffect {
    /// No item was admitted.
    NoAdmission,
    /// A positive count was admitted before a confirmed stopping point.
    PartialAdmission,
    /// The current item's admission outcome cannot be established.
    Unknown,
}

/// A request-correlated replication failure with no exposed repository paths.
#[derive(Clone, PartialEq, Eq)]
pub struct ValidatedReplicationFailure {
    category: String,
    effect: ReplicationFailureEffect,
}

impl core::fmt::Debug for ValidatedReplicationFailure {
    fn fmt(&self, formatter: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        formatter.write_str("ValidatedReplicationFailure([redacted])")
    }
}

impl ValidatedReplicationFailure {
    /// The category admitted by the installed replication refusal types.
    #[must_use]
    pub fn category(&self) -> &str { &self.category }
    /// The command-derived admission evidence, not publisher delivery evidence.
    #[must_use]
    pub fn effect(&self) -> ReplicationFailureEffect { self.effect }
}

/// Validates replication preflight or bounded admission failure evidence.
pub fn decode_replication_failure(
    body: &[u8], expectation: &ResultExpectation,
    command: &slingshot_domain::command::catalog::Command,
) -> Result<ValidatedReplicationFailure, TerminalFailureDecodeRefusal> {
    use slingshot_domain::command::{catalog::Command, replicate_content::{PreflightRefusal, AdmissionRefusal, AdmissionOutcome}};
    require_installed_command(expectation, command)?;
    let Command::ReplicateContent(command) = command else { return Err(TerminalFailureDecodeRefusal); };
    let document = decode_terminal_failure(body, expectation)?;
    let category = failure_category(&document)?;
    let effect = match category.as_str() {
        "source_not_found" | "source_access_denied" | "candidate_limit_exceeded" | "traversal_budget_exceeded" => {
            let refusal: PreflightRefusal = serde_json::from_str(&document.canonical_failure).map_err(|_| TerminalFailureDecodeRefusal)?;
            refusal.require_answers(command).map_err(|_| TerminalFailureDecodeRefusal)?;
            ReplicationFailureEffect::NoAdmission
        }
        _ => {
            let refusal: AdmissionRefusal = serde_json::from_str(&document.canonical_failure).map_err(|_| TerminalFailureDecodeRefusal)?;
            refusal.require_answers(command).map_err(|_| TerminalFailureDecodeRefusal)?;
            if refusal.failure == AdmissionOutcome::AdmissionOutcomeUnknown { ReplicationFailureEffect::Unknown }
            else if refusal.accepted_item_count == 0 { ReplicationFailureEffect::NoAdmission }
            else { ReplicationFailureEffect::PartialAdmission }
        }
    };
    Ok(ValidatedReplicationFailure { category, effect })
}

/// A closed refusal correlated with one of the six repository discovery commands.
#[derive(Clone, PartialEq, Eq)]
pub struct ValidatedDiscoveryFailure {
    category: String,
}

impl core::fmt::Debug for ValidatedDiscoveryFailure {
    fn fmt(&self, formatter: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        formatter.write_str("ValidatedDiscoveryFailure([redacted])")
    }
}

impl ValidatedDiscoveryFailure {
    /// The category admitted by this discovery contract.
    #[must_use]
    pub fn category(&self) -> &str { &self.category }
}

/// Validates discovery anchors, bounded budget names and continuation failures.
/// Continuation failures require a request carrying a continuation token; no
/// failure may carry partial matches, counts or a replacement token.
pub fn decode_discovery_failure(
    body: &[u8],
    expectation: &ResultExpectation,
    command: &slingshot_domain::command::catalog::Command,
) -> Result<ValidatedDiscoveryFailure, TerminalFailureDecodeRefusal> {
    use slingshot_domain::command::{catalog::Command, result_window::ResultWindow};
    require_installed_command(expectation, command)?;
    let (root, window) = match command {
        Command::QueryPaths(command) => (Some(&command.root_path), &command.result_window),
        Command::FindPagesByTemplate(command) => (Some(&command.root_path), &command.result_window),
        Command::FindPagesContainingPhrase(command) => (Some(&command.root_path), &command.result_window),
        Command::FindPagesUsingComponents(command) => (Some(&command.root_path), &command.result_window),
        Command::FindAssetsByMetadata(command) => (Some(&command.root_path), &command.result_window),
        Command::FindAssetsReferencedByPage(command) => (None, &command.result_window),
        _ => return Err(TerminalFailureDecodeRefusal),
    };
    let document = decode_terminal_failure(body, expectation)?;
    let value: serde_json::Value = serde_json::from_str(&document.canonical_failure).map_err(|_| TerminalFailureDecodeRefusal)?;
    let members = value.as_object().ok_or(TerminalFailureDecodeRefusal)?;
    let category = failure_category(&document)?;
    match category.as_str() {
        "root_not_found" | "root_access_denied" => {
            let refusal: slingshot_domain::command::query_paths::AnchorRefusal = serde_json::from_value(value.clone()).map_err(|_| TerminalFailureDecodeRefusal)?;
            if root != Some(refusal.root_path()) { return Err(TerminalFailureDecodeRefusal); }
        }
        "page_not_found" | "page_access_denied" | "page_invalid" => {
            let Command::FindAssetsReferencedByPage(command) = command else { return Err(TerminalFailureDecodeRefusal); };
            let refusal: slingshot_domain::command::find_assets_referenced_by_page::PageAnchorRefusal = serde_json::from_value(value.clone()).map_err(|_| TerminalFailureDecodeRefusal)?;
            refusal.require_answers(command).map_err(|_| TerminalFailureDecodeRefusal)?;
        }
        "discovery_budget_exceeded" => {
            if members.len() != 2 { return Err(TerminalFailureDecodeRefusal); }
            let _: slingshot_domain::command::discovery_budget::DiscoveryBudget = serde_json::from_value(value["budget"].clone()).map_err(|_| TerminalFailureDecodeRefusal)?;
        }
        category if slingshot_domain::command::result_window::CONTINUATION_FAILURE_PRECEDENCE.contains(&category) => {
            if members.len() != 1 || !matches!(window, Some(ResultWindow::Continuation { .. })) {
                return Err(TerminalFailureDecodeRefusal);
            }
        }
        _ => return Err(TerminalFailureDecodeRefusal),
    }
    Ok(ValidatedDiscoveryFailure { category })
}

/// A closed configuration-inspection refusal with no private identifiers,
/// property names, values, filters or partial maps.
#[derive(Clone, PartialEq, Eq)]
pub struct ValidatedConfigurationFailure {
    category: String,
}

impl core::fmt::Debug for ValidatedConfigurationFailure {
    fn fmt(&self, formatter: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        formatter.write_str("ValidatedConfigurationFailure([redacted])")
    }
}

impl ValidatedConfigurationFailure {
    /// The category admitted by the installed inspection refusal enum.
    #[must_use]
    pub fn category(&self) -> &str {
        &self.category
    }
}

/// Validates configuration-inspection failures under the retained command's
/// installed contract. No failure field is allowed to carry request data.
pub fn decode_configuration_failure(
    body: &[u8],
    expectation: &ResultExpectation,
    command: &slingshot_domain::command::catalog::Command,
) -> Result<ValidatedConfigurationFailure, TerminalFailureDecodeRefusal> {
    require_installed_command(expectation, command)?;
    if !matches!(command, slingshot_domain::command::catalog::Command::InspectOpenServiceGatewayInitiativeConfiguration(_)) {
        return Err(TerminalFailureDecodeRefusal);
    }
    let document = decode_terminal_failure(body, expectation)?;
    let refusal: slingshot_domain::command::inspect_open_service_gateway_initiative_configuration::ConfigurationRefusal =
        serde_json::from_str(&document.canonical_failure).map_err(|_| TerminalFailureDecodeRefusal)?;
    // Internally tagged unit variants need an explicit exact-shape check.
    let received: serde_json::Value = serde_json::from_str(&document.canonical_failure)
        .map_err(|_| TerminalFailureDecodeRefusal)?;
    if serde_json::to_value(refusal).map_err(|_| TerminalFailureDecodeRefusal)? != received {
        return Err(TerminalFailureDecodeRefusal);
    }
    Ok(ValidatedConfigurationFailure { category: failure_category(&document)? })
}

/// A request-correlated content-read refusal. This carries no mutation or
/// publication evidence and does not authorize replay.
#[derive(Clone, PartialEq, Eq)]
pub struct ValidatedLoadFailure {
    category: String,
}

impl core::fmt::Debug for ValidatedLoadFailure {
    fn fmt(&self, formatter: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        formatter.write_str("ValidatedLoadFailure([redacted])")
    }
}

impl ValidatedLoadFailure {
    /// The category admitted by the load command's closed refusal enum.
    #[must_use]
    pub fn category(&self) -> &str {
        &self.category
    }
}

/// Package publication evidence is deliberately distinct from mutation evidence.
#[derive(Clone, PartialEq, Eq)]
pub struct ValidatedPackageFailure {
    category: String,
    proves_no_publication: bool,
}

impl core::fmt::Debug for ValidatedPackageFailure {
    fn fmt(&self, formatter: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        formatter.write_str("ValidatedPackageFailure([redacted])")
    }
}

impl ValidatedPackageFailure {
    /// The category admitted by the package command's closed refusal enum.
    #[must_use]
    pub fn category(&self) -> &str {
        &self.category
    }

    /// False preserves publication uncertainty; true is not proof of no other
    /// effects and does not authorize resubmission.
    #[must_use]
    pub fn proves_no_publication(&self) -> bool {
        self.proves_no_publication
    }
}

fn require_installed_command(
    expectation: &ResultExpectation,
    command: &slingshot_domain::command::catalog::Command,
) -> Result<(), TerminalFailureDecodeRefusal> {
    use slingshot_domain::selected_command_contract_identity::SelectedCommandContractIdentity;
    if expectation.wire_name != command.wire_name()
        || expectation.expected_provenance.command_contract != SelectedCommandContractIdentity::installed(command.wire_name()).map_err(|_| TerminalFailureDecodeRefusal)?
        || expectation.expected_provenance.canonical_json_contract_digest != slingshot_domain::command::schema::canonical_contract_digest()
        || expectation.expected_provenance.transport_contract_digest != slingshot_domain::author_agent_transport_contract::AuthorAgentTransportContract::embedded_digest() {
        return Err(TerminalFailureDecodeRefusal);
    }
    Ok(())
}

fn failure_category(document: &TerminalFailureDocument) -> Result<String, TerminalFailureDecodeRefusal> {
    let value: serde_json::Value = serde_json::from_str(&document.canonical_failure)
        .map_err(|_| TerminalFailureDecodeRefusal)?;
    Ok(value["failure"].as_str().ok_or(TerminalFailureDecodeRefusal)?.to_owned())
}

/// Validates the closed load refusal and its requested subtree/depth.
pub fn decode_load_failure(
    body: &[u8],
    expectation: &ResultExpectation,
    command: &slingshot_domain::command::catalog::Command,
) -> Result<ValidatedLoadFailure, TerminalFailureDecodeRefusal> {
    require_installed_command(expectation, command)?;
    let slingshot_domain::command::catalog::Command::LoadContentAsJson(command) = command else {
        return Err(TerminalFailureDecodeRefusal);
    };
    let document = decode_terminal_failure(body, expectation)?;
    let refusal: slingshot_domain::command::load_content_as_javascript_object_notation::LoadRefusal =
        serde_json::from_str(&document.canonical_failure).map_err(|_| TerminalFailureDecodeRefusal)?;
    refusal.require_answers(command).map_err(|_| TerminalFailureDecodeRefusal)?;
    Ok(ValidatedLoadFailure { category: failure_category(&document)? })
}

/// Validates exact requested package roots/filter references and preserves
/// publication-outcome uncertainty separately from repository mutation state.
pub fn decode_package_failure(
    body: &[u8],
    expectation: &ResultExpectation,
    command: &slingshot_domain::command::catalog::Command,
) -> Result<ValidatedPackageFailure, TerminalFailureDecodeRefusal> {
    require_installed_command(expectation, command)?;
    let slingshot_domain::command::catalog::Command::DownloadContentPackage(command) = command else {
        return Err(TerminalFailureDecodeRefusal);
    };
    let document = decode_terminal_failure(body, expectation)?;
    let refusal: slingshot_domain::command::download_content_package::DownloadContentPackageRefusal =
        serde_json::from_str(&document.canonical_failure).map_err(|_| TerminalFailureDecodeRefusal)?;
    // Internally tagged serde unit variants can ignore extra fields even with
    // deny_unknown_fields. Require the exact typed shape without repairing bytes.
    let received: serde_json::Value = serde_json::from_str(&document.canonical_failure)
        .map_err(|_| TerminalFailureDecodeRefusal)?;
    if serde_json::to_value(&refusal).map_err(|_| TerminalFailureDecodeRefusal)? != received {
        return Err(TerminalFailureDecodeRefusal);
    }
    refusal.require_answers(command).map_err(|_| TerminalFailureDecodeRefusal)?;
    Ok(ValidatedPackageFailure {
        category: failure_category(&document)?,
        proves_no_publication: refusal.proves_no_publication(),
    })
}

/// A request-correlated create-page/add-component refusal. No remote target
/// path or arbitrary failure payload is exposed through this value.
#[derive(Clone, PartialEq, Eq)]
pub struct ValidatedCreationFailure {
    category: String,
    proves_no_effect: bool,
}

impl core::fmt::Debug for ValidatedCreationFailure {
    fn fmt(&self, formatter: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        formatter.write_str("ValidatedCreationFailure([redacted])")
    }
}

impl ValidatedCreationFailure {
    /// The category admitted by the selected command's closed refusal enum.
    #[must_use]
    pub fn category(&self) -> &str {
        &self.category
    }

    /// Whether the command-specific refusal proves that no mutation occurred.
    /// False means outcome unknown, not proof of remote failure or permission
    /// to resend the command.
    #[must_use]
    pub fn proves_no_effect(&self) -> bool {
        self.proves_no_effect
    }
}

/// Validates creation failures using the existing typed refusal and exact
/// computed-target checks. Other command families are not admitted here.
pub fn decode_creation_failure(
    body: &[u8],
    expectation: &ResultExpectation,
    command: &slingshot_domain::command::catalog::Command,
) -> Result<ValidatedCreationFailure, TerminalFailureDecodeRefusal> {
    use slingshot_domain::command::catalog::Command;
    require_installed_command(expectation, command)?;
    if !matches!(command, Command::CreatePage(_) | Command::AddComponent(_)) {
        return Err(TerminalFailureDecodeRefusal);
    }
    let document = decode_terminal_failure(body, expectation)?;
    let proves_no_effect = match command {
        Command::CreatePage(command) => {
            let refusal: slingshot_domain::command::create_page::CreatePageRefusal =
                serde_json::from_str(&document.canonical_failure)
                    .map_err(|_| TerminalFailureDecodeRefusal)?;
            refusal.require_answers(command).map_err(|_| TerminalFailureDecodeRefusal)?;
            refusal.proves_no_effect()
        }
        Command::AddComponent(command) => {
            let refusal: slingshot_domain::command::add_component::AddComponentRefusal =
                serde_json::from_str(&document.canonical_failure)
                    .map_err(|_| TerminalFailureDecodeRefusal)?;
            refusal.require_answers(command).map_err(|_| TerminalFailureDecodeRefusal)?;
            refusal.proves_no_effect()
        }
        _ => return Err(TerminalFailureDecodeRefusal),
    };
    let category = failure_category(&document)?;
    Ok(ValidatedCreationFailure { category, proves_no_effect })
}

/// Validates framing-independent envelope identity and exact canonical payload.
/// The returned document remains untrusted as a command failure.
pub fn decode_terminal_failure(
    body: &[u8],
    expectation: &ResultExpectation,
) -> Result<TerminalFailureDocument, TerminalFailureDecodeRefusal> {
    if body.len() as u64 > maximum_document_bytes() {
        return Err(TerminalFailureDecodeRefusal);
    }
    let document: TerminalFailureDocument =
        serde_json::from_slice(body).map_err(|_| TerminalFailureDecodeRefusal)?;
    expectation
        .expected_provenance
        .require_matching(&document.provenance)
        .map_err(|_| TerminalFailureDecodeRefusal)?;
    if document.operation != expectation.operation
        || document.daemon_subscription_identifier != expectation.daemon_subscription_identifier
        || document.submitted_command_digest != expectation.submitted_command_digest
        || document.provenance.command_contract.command_wire_name != expectation.wire_name
        || document.canonical_failure.len() as u64 > maximum_agent_inline_result_bytes()
    {
        return Err(TerminalFailureDecodeRefusal);
    }
    slingshot_domain::command::canonical_json::require_canonical_bytes(
        document.canonical_failure.as_bytes(),
    )
    .map_err(|_| TerminalFailureDecodeRefusal)?;
    static VALIDATOR: std::sync::OnceLock<Result<jsonschema::Validator, ()>> =
        std::sync::OnceLock::new();
    let validator = VALIDATOR
        .get_or_init(|| {
            let mut options = jsonschema::options().with_draft(jsonschema::Draft::Draft202012);
            for source in [
                include_str!("../../../schemas/agent-protocol/identity/operation.json"),
                include_str!("../../../schemas/agent-protocol/identity/command-contract.json"),
                include_str!("../../../schemas/agent-protocol/common/provenance.json"),
            ] {
                let schema: serde_json::Value = serde_json::from_str(source).map_err(|_| ())?;
                let uri = schema["$id"].as_str().ok_or(())?.to_owned();
                options = options.with_resource(
                    uri,
                    jsonschema::Resource::from_contents(schema).map_err(|_| ())?,
                );
            }
            let schema = serde_json::from_str(slingshot_agent_protocol::terminal_failure::SCHEMA)
                .map_err(|_| ())?;
            options.build(&schema).map_err(|_| ())
        })
        .as_ref()
        .map_err(|_| TerminalFailureDecodeRefusal)?;
    if !validator
        .is_valid(&serde_json::to_value(&document).map_err(|_| TerminalFailureDecodeRefusal)?)
    {
        return Err(TerminalFailureDecodeRefusal);
    }
    Ok(document)
}
