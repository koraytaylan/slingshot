//! One invocation's concrete protocol over the selected transport and durable owner.

use crate::{
    author_agent_operation_executor::{AgentSettlement, ArtifactCompletion, AuthorAgentProtocol},
    operation::{
        durable_author_lookup::{
            automatic_recovery_paused, lookup_retained_operation_with_authentication, retained_command,
        },
        durable_author_submission::submit_initial_with_authentication,
        author_authentication::AuthorAuthentication,
        remote_submission::{HandoffDisposition, disposition_of},
        subscription_reset::ResetTransport,
    },
};
use slingshot_agent_connection::{
    authentication::environment_provider::RequestAuthentication, command_submission::Submission,
    selected_author_transport::SelectedAuthorTransport,
};
use slingshot_domain::{
    command::catalog::Command,
    operation::{
        OperationExecutionCertainty, OperationLifecycleState, RecoveryCategory,
        RecoveryExecutionEvidence, RecoveryFact,
    },
    operation_executor::{ExecutionFuture, ExecutionIdentity, ProducedArtifact},
};
use slingshot_storage::{
    agent_job_repository::AgentJobRepository,
    artifact_store::{ArtifactAssociations, ArtifactIdentifier, ArtifactStore},
    operation_repository::{OperationRepository, OperationSummary},
    persistent_capacity::PersistentCapacityAccount,
};

/// The invocation owns no alternate client and cannot change endpoint, identity,
/// command or authentication policy between submission and recovery. Provider
/// credentials may refresh without changing the selected principal or revision.
pub struct RetainedAuthorProtocol<'runtime> {
    operations: &'runtime OperationRepository,
    remote: &'runtime AgentJobRepository,
    store: &'runtime ArtifactStore,
    capacity: &'runtime PersistentCapacityAccount<'runtime>,
    authentication: AuthorAuthentication<'runtime>,
    identity: ExecutionIdentity,
    submission: Submission,
    command: Command,
    started: std::time::Instant,
    now: u64,
}

impl core::fmt::Debug for RetainedAuthorProtocol<'_> {
    fn fmt(&self, formatter: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        formatter.write_str("RetainedAuthorProtocol([redacted])")
    }
}

/// Retained local input could not establish an invocation context.
#[derive(Debug, Clone, Copy, thiserror::Error)]
#[error("the retained author invocation could not be bound")]
pub struct RetainedAuthorProtocolRefusal;

impl<'runtime> RetainedAuthorProtocol<'runtime> {
    /// Typed command derived from the independently admitted retained bytes.
    pub(crate) fn command(&self) -> &Command { &self.command }

    /// Binds a validated submission to the independently admitted command.
    /// Uses negotiated transport, with preflight before every network phase.
    pub fn new(
        operations: &'runtime OperationRepository,
        remote: &'runtime AgentJobRepository,
        store: &'runtime ArtifactStore,
        capacity: &'runtime PersistentCapacityAccount<'runtime>,
        authentication: &'runtime RequestAuthentication,
        identity: ExecutionIdentity,
        submission: Submission,
        now: u64,
    ) -> Result<Self, RetainedAuthorProtocolRefusal> {
        Self::new_over(
            operations, remote, store, capacity, authentication, identity,
            submission, now, ResetTransport::Automatic,
        )
    }

    /// Binds one immutable protocol mode across initial submission, lookup and
    /// artifact acquisition. No phase can fall back or change that choice.
    pub fn new_over(
        operations: &'runtime OperationRepository,
        remote: &'runtime AgentJobRepository,
        store: &'runtime ArtifactStore,
        capacity: &'runtime PersistentCapacityAccount<'runtime>,
        authentication: &'runtime RequestAuthentication,
        identity: ExecutionIdentity,
        submission: Submission,
        now: u64,
        protocol: ResetTransport,
    ) -> Result<Self, RetainedAuthorProtocolRefusal> {
        Self::new_with_authentication(operations, remote, store, capacity,
            AuthorAuthentication::Fixed { authentication, protocol }, identity, submission, now)
    }

    /// Binds one runtime authentication policy through admission, lookup and
    /// artifact completion. The provider branch holds no invocation-long token
    /// and reuses the same durable ownership checks as fixed-credential callers.
    pub fn new_with_authentication(
        operations: &'runtime OperationRepository,
        remote: &'runtime AgentJobRepository,
        store: &'runtime ArtifactStore,
        capacity: &'runtime PersistentCapacityAccount<'runtime>,
        authentication: AuthorAuthentication<'runtime>,
        identity: ExecutionIdentity,
        submission: Submission,
        now: u64,
    ) -> Result<Self, RetainedAuthorProtocolRefusal> {
        authentication.require_execution(&identity).map_err(|_| RetainedAuthorProtocolRefusal)?;
        if !operations.database().shares_database_with(remote.database())
            || !capacity.belongs_to(operations.database())
        {
            return Err(RetainedAuthorProtocolRefusal);
        }
        slingshot_agent_connection::selected_author_submission::require_submission_derivation(
            &identity,
            &submission,
        )
        .map_err(|_| RetainedAuthorProtocolRefusal)?;
        let local = retained_command(operations, &identity, &submission)
            .map_err(|_| RetainedAuthorProtocolRefusal)?;
        if local.selected_environment_revision != identity.selected_environment_revision {
            return Err(RetainedAuthorProtocolRefusal);
        }
        let mut arguments: serde_json::Value =
            serde_json::from_str(&submission.canonical_arguments)
                .map_err(|_| RetainedAuthorProtocolRefusal)?;
        let object = arguments.as_object_mut().ok_or(RetainedAuthorProtocolRefusal)?;
        if object
            .insert(
                "command".to_owned(),
                submission.provenance.command_contract.command_wire_name.clone().into(),
            )
            .is_some()
        {
            return Err(RetainedAuthorProtocolRefusal);
        }
        let command =
            serde_json::from_value(arguments).map_err(|_| RetainedAuthorProtocolRefusal)?;
        Ok(Self {
            operations,
            remote,
            store,
            capacity,
            authentication,
            identity,
            submission,
            command,
            started: std::time::Instant::now(),
            now,
        })
    }

    fn now(&self) -> u64 {
        self.now.saturating_add(
            u64::try_from(self.started.elapsed().as_nanos().div_ceil(1_000_000))
                .unwrap_or(u64::MAX),
        )
    }

    fn local(
        &self,
        identity: &ExecutionIdentity,
    ) -> Result<OperationSummary, RetainedAuthorProtocolRefusal> {
        if identity != &self.identity
            || !self.operations.database().shares_database_with(self.remote.database())
            || !self.capacity.belongs_to(self.operations.database())
        {
            return Err(RetainedAuthorProtocolRefusal);
        }
        retained_command(self.operations, identity, &self.submission)
            .map_err(|_| RetainedAuthorProtocolRefusal)
    }

    fn waiting(&self, local: Option<&OperationSummary>) -> RecoveryFact {
        local.and_then(|local| local.record.outstanding_recovery.clone()).unwrap_or(RecoveryFact {
            category: if local.is_some_and(|local| {
                local.record.lifecycle_state == OperationLifecycleState::Succeeded
            }) {
                RecoveryCategory::ResultAcquisition
            } else {
                RecoveryCategory::OperationLookup
            },
            evidence: if local.is_some_and(|local| {
                local.record.lifecycle_state == OperationLifecycleState::Succeeded
            }) {
                RecoveryExecutionEvidence::AuthoritativeRemoteSuccess
            } else {
                RecoveryExecutionEvidence::ExecutionCertainty {
                    certainty: OperationExecutionCertainty::RemoteOutcomeUnknown,
                }
            },
            attempt_count: self.identity.attempt,
            detail: "same-operation author lookup remains pending".to_owned(),
            manual_resume_eligible: false,
            retry_delay_milliseconds: 0,
            retry_observed_at_unix_milliseconds: self.now(),
        })
    }

    fn deferred(&self, local: &OperationSummary) -> bool {
        local.record.outstanding_recovery.as_ref().is_some_and(|fact| {
            automatic_recovery_paused(fact)
                || self.now()
                    < fact
                        .retry_observed_at_unix_milliseconds
                        .saturating_add(fact.retry_delay_milliseconds)
        })
    }

    fn settlement(&self, local: &OperationSummary) -> AgentSettlement {
        if let Some(failure) = &local.record.terminal_failure {
            AgentSettlement::Terminal { failure: failure.clone() }
        } else if local.record.lifecycle_state == OperationLifecycleState::Succeeded {
            AgentSettlement::Succeeded { inline_result: local.result_inline_bytes.clone() }
        } else {
            AgentSettlement::Outstanding { recovery: self.waiting(Some(local)) }
        }
    }
}

impl AuthorAgentProtocol for RetainedAuthorProtocol<'_> {
    fn submit<'a>(
        &'a self,
        transport: &'a SelectedAuthorTransport,
        identity: &'a ExecutionIdentity,
        command: &'a Command,
    ) -> ExecutionFuture<'a, HandoffDisposition> {
        Box::pin(async move {
            if command != &self.command
                || transport.require_submission(identity, &self.submission).is_err()
            {
                return HandoffDisposition::Conflict;
            }
            let Ok(local) = self.local(identity) else {
                return HandoffDisposition::Unknown;
            };
            if local.record.lifecycle_state.is_terminal() || self.deferred(&local) {
                return HandoffDisposition::ReconcileRetained;
            }
            match submit_initial_with_authentication(
                self.remote,
                self.operations,
                local.record.revision,
                transport,
                identity,
                &self.submission,
                self.authentication,
                self.now(),
            )
            .await
            {
                Ok(outcome) => disposition_of(&outcome),
                Err(_) => HandoffDisposition::Unknown,
            }
        })
    }

    fn settle<'a>(
        &'a self,
        transport: &'a SelectedAuthorTransport,
        identity: &'a ExecutionIdentity,
    ) -> ExecutionFuture<'a, AgentSettlement> {
        Box::pin(async move {
            let Ok(local) = self.local(identity) else {
                return AgentSettlement::Outstanding { recovery: self.waiting(None) };
            };
            if transport.require_submission(identity, &self.submission).is_err()
                || local.record.lifecycle_state.is_terminal()
                || self.deferred(&local)
            {
                return self.settlement(&local);
            }
            // Re-read even on refusal: the coordinator can durably establish
            // remote success before a later artifact verification fails.
            let _ = lookup_retained_operation_with_authentication(
                self.remote,
                self.operations,
                local.record.revision,
                transport,
                identity,
                &self.submission,
                self.authentication,
                self.now(),
                Some((self.store, self.capacity)),
            )
            .await;
            match self.local(identity) {
                Ok(current) => self.settlement(&current),
                Err(_) => AgentSettlement::Outstanding { recovery: self.waiting(Some(&local)) },
            }
        })
    }

    fn complete_artifacts<'a>(
        &'a self,
        transport: &'a SelectedAuthorTransport,
        identity: &'a ExecutionIdentity,
    ) -> ExecutionFuture<'a, ArtifactCompletion> {
        Box::pin(async move {
            let local = self.local(identity).ok();
            let recovery =
                || ArtifactCompletion::Recovery { recovery: self.waiting(local.as_ref()) };
            if transport.require_submission(identity, &self.submission).is_err() {
                return recovery();
            }
            let Some(local) = &local else {
                return recovery();
            };
            if local.record.terminal_failure.as_ref().is_some_and(|failure|
                failure.kind == slingshot_domain::operation::TerminalFailureKind::ResultUnavailable
                && failure.disposition == slingshot_domain::operation::TerminalFailureDisposition::AuthoritativeRemoteSuccess) {
                return ArtifactCompletion::Unavailable;
            }
            if local.record.lifecycle_state != OperationLifecycleState::Succeeded {
                return recovery();
            }
            let associations = ArtifactAssociations::new(self.operations.database());
            let mut artifacts = Vec::new();
            for slot in ["content_package", "loaded_content_json", "structured_result"] {
                let metadata = match associations.read(
                    &identity.author_target_identity_digest,
                    &identity.operation_identifier,
                    slot,
                ) {
                    Ok(Some(metadata)) => metadata,
                    Ok(None) => continue,
                    Err(_) => return recovery(),
                };
                if metadata.artifact_identifier
                    != ArtifactIdentifier::derive(
                        &local.installation_identifier,
                        &identity.author_target_identity_digest,
                        &identity.operation_identifier,
                        slot,
                    )
                {
                    return recovery();
                }
                // Reopening a completed invocation must not expose a corrupt
                // or replaced content object merely because its row survives.
                if self.store.open_verified(&metadata).is_err() {
                    return recovery();
                }
                artifacts.push(ProducedArtifact {
                    artifact_identifier: metadata.artifact_identifier.as_text().to_owned(),
                    artifact_slot: metadata.artifact_slot,
                    byte_length: metadata.byte_length,
                    content_digest: metadata.content_digest,
                    media_type: metadata.media_type,
                });
            }
            // A surviving operation row is insufficient if maintenance or
            // corruption removed one of the associations the result requires.
            let canonical = if let Some(inline) = &local.result_inline_bytes {
                if artifacts.iter().any(|artifact| artifact.artifact_slot == "structured_result") {
                    return recovery();
                }
                inline.clone()
            } else {
                use std::io::Read;
                let Ok(Some(metadata)) = associations.read(
                    &identity.author_target_identity_digest,
                    &identity.operation_identifier,
                    "structured_result",
                ) else {
                    return recovery();
                };
                let Ok(mut reader) = self.store.open_verified(&metadata) else {
                    return recovery();
                };
                let mut bytes = Vec::new();
                let limit = slingshot_agent_connection::structured_job_result::maximum_agent_inline_result_bytes();
                if reader.by_ref().take(limit.saturating_add(1)).read_to_end(&mut bytes).is_err()
                    || bytes.len() as u64 > limit
                    || reader.finish().is_err()
                {
                    return recovery();
                }
                let Ok(text) = String::from_utf8(bytes) else {
                    return recovery();
                };
                text
            };
            let Ok(mut value) = slingshot_domain::command::canonical_json::require_canonical_bytes(
                canonical.as_bytes(),
            ) else {
                return recovery();
            };
            let Some(object) = value.as_object_mut() else {
                return recovery();
            };
            if object.insert("command".to_owned(), self.command.wire_name().into()).is_some() {
                return recovery();
            }
            use slingshot_domain::command::catalog::{CommandResult, validate_result_for_command};
            let Ok(result) = serde_json::from_value::<CommandResult>(value) else {
                return recovery();
            };
            if validate_result_for_command(&self.command, &result).is_err() {
                return recovery();
            }
            use slingshot_domain::command::load_content_as_javascript_object_notation::LoadContentAsJavaScriptObjectNotationResult;
            let descriptor = match &result {
                CommandResult::DownloadContentPackage(result) => Some(&result.artifact),
                CommandResult::LoadContentAsJson(
                    LoadContentAsJavaScriptObjectNotationResult::Artifact { artifact, .. },
                ) => Some(artifact),
                _ => None,
            };
            let remote: Vec<_> = artifacts
                .iter()
                .filter(|artifact| artifact.artifact_slot != "structured_result")
                .collect();
            if remote.len() != usize::from(descriptor.is_some()) {
                return recovery();
            }
            if let Some(descriptor) = descriptor {
                let metadata = remote[0];
                if metadata.artifact_identifier != descriptor.identifier.as_text()
                    || metadata.artifact_slot != descriptor.slot.as_text()
                    || metadata.content_digest != descriptor.digest.as_text()
                    || metadata.media_type != descriptor.media_type.as_text()
                    || metadata.byte_length != descriptor.byte_length
                {
                    return recovery();
                }
            }
            ArtifactCompletion::Published { artifacts }
        })
    }
}
