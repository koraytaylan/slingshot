//! Command validation and durable admission, before any acceptance is rendered.

use slingshot_domain::command::canonical_json::write_canonical;
use slingshot_domain::command::catalog::{Command, CommandCatalog};
use slingshot_domain::command_fingerprint::{CommandFingerprint, FingerprintInput};
use slingshot_domain::daemon_runtime_contract::DaemonRuntimeContract;
use slingshot_domain::installation::InstallationIdentifier;
use slingshot_domain::selected_command_contract_identity::SelectedCommandContractIdentity;
use slingshot_local_protocol::message::{OperationRequest, OperationResponse};
use slingshot_storage::operation_repository::{
    AdmissionOutcome, AdmissionRequest, OperationRepository, PendingAdmissionCapacity,
    RepositoryFailure,
};

use super::{BoundRequest, malformed};

/// Exact command and daemon-derived fingerprint prepared for one transaction.
#[derive(Debug)]
pub struct PreparedAdmission {
    request: AdmissionRequest,
}

impl BoundRequest {
    /// Prepares an execute request using this build's exact installed catalog.
    /// No repository, credential provider, or remote endpoint is accessed here.
    ///
    /// # Errors
    ///
    /// Returns a typed unavailable, validation, or installed-contract refusal.
    /// `Ok(None)` means this request belongs to a different handler.
    pub fn prepare_admission(
        &self,
        installation: &InstallationIdentifier,
    ) -> Result<Option<PreparedAdmission>, OperationResponse> {
        let OperationRequest::Execute {
            command,
            operation_identifier,
            workflow_correlation_identifier,
        } = self.request()
        else {
            return Ok(None);
        };
        if !self.execution_available {
            return Err(OperationResponse::ExecutorUnavailable);
        }
        if operation_identifier.is_empty()
            || operation_identifier.contains('\0')
            || !super::listing::identifier_is_representable(
                &self.envelope.author_target_identity_digest,
                operation_identifier,
            )
            || workflow_correlation_identifier.as_ref().is_some_and(|identifier| {
                identifier.is_empty()
                    || identifier.contains('\0')
                    || identifier.len() as u64
                        > DaemonRuntimeContract::embedded()
                            .limit("maximum_workflow_correlation_identifier_bytes")
            })
        {
            return Err(malformed());
        }
        let typed: Command = serde_json::from_value(command.clone()).map_err(|_| malformed())?;
        let wire_name = typed.wire_name();
        let installed = SelectedCommandContractIdentity::installed(wire_name)
            .map_err(|_| contract_failure())?;
        let catalog = CommandCatalog::published();
        let descriptor = catalog.find(wire_name).ok_or_else(contract_failure)?;
        if installed.command_wire_name != descriptor.wire_name
            || installed.command_semantic_contract_version
                != descriptor.command_semantic_contract_version
            || installed.command_contract_limits_digest != descriptor.command_contract_limits_sha256
            || installed.argument_schema_digest != descriptor.arguments_schema_sha256
            || installed.result_schema_digest != descriptor.result_schema_sha256
        {
            return Err(contract_failure());
        }
        // Persist the normalized typed arguments, not the untrusted wire map.
        // The command tag is represented separately in the retained input.
        let mut arguments = serde_json::to_value(&typed).map_err(|_| contract_failure())?;
        arguments.as_object_mut().ok_or_else(contract_failure)?.remove("command");
        let canonical_command = write_canonical(&arguments).map_err(|_| malformed())?;
        let command_fingerprint = CommandFingerprint::derive(&FingerprintInput {
            author_target_identity_digest: self.envelope.author_target_identity_digest.clone(),
            canonical_command: canonical_command.clone(),
            command_wire_name: wire_name.to_owned(),
            command_semantic_contract_version: installed.command_semantic_contract_version,
            selected_environment_revision: self.envelope.selected_environment_revision.clone(),
        })
        .map_err(|_| malformed())?;
        Ok(Some(PreparedAdmission {
            request: AdmissionRequest {
                author_target_identity: self.envelope.author_target_identity_digest.clone(),
                author_target_identity_digest: self.envelope.author_target_identity_digest.clone(),
                caller_identity: None,
                canonical_command,
                command_fingerprint,
                command_wire_name: wire_name.to_owned(),
                daemon_runtime_contract_digest: self
                    .envelope
                    .daemon_runtime_contract_digest
                    .clone(),
                installation_identifier: installation.clone(),
                operation_identifier: operation_identifier.clone(),
                selected_environment_revision: self.envelope.selected_environment_revision.clone(),
                workflow_correlation_identifier: workflow_correlation_identifier.clone(),
            },
        }))
    }
}

impl PreparedAdmission {
    /// Admits under the embedded pending-work limits and maps a full queue to
    /// a typed refusal. The owner must hold its scheduling lock throughout this
    /// call; `active_operations` is its live slot set, never client input.
    ///
    /// # Errors
    ///
    /// Returns persistent/storage failures without acknowledging new work.
    pub fn persist_scheduled(
        &self,
        repository: &OperationRepository,
        active_operations: &std::collections::BTreeSet<String>,
        now_unix_milliseconds: u64,
    ) -> Result<OperationResponse, RepositoryFailure> {
        let limits = DaemonRuntimeContract::embedded();
        if active_operations.len() as u64 > limits.limit("maximum_global_in_flight_operations") {
            return Ok(OperationResponse::InternalFailure {
                detail: "the retained execution slot set exceeds its bound".to_owned(),
            });
        }
        match repository.admit_with_pending_capacity(&self.request, now_unix_milliseconds, PendingAdmissionCapacity {
            active_operations,
            global_pending: limits.limit("maximum_global_pending_operations"),
            pending_per_caller: limits.limit("maximum_pending_operations_per_caller"),
        }) {
            Ok(outcome) => Ok(admission_response(outcome)),
            Err(RepositoryFailure::PendingCapacity { .. }) => Ok(OperationResponse::SchedulerCapacityExhausted {
                guidance: "wait for pending work to advance, then retry with the same operation identifier".to_owned(),
            }),
            Err(failure) => Err(failure),
        }
    }

    /// Commits the retained operation before constructing an acceptance.
    /// Scheduler admission must be checked by the owning runtime before a new
    /// row reaches this method. Repository capacity and repeat decisions remain
    /// inside the durable transaction.
    ///
    /// # Errors
    ///
    /// Returns the repository refusal without converting it into an acceptance.
    #[cfg(test)]
    pub(crate) fn persist(
        &self,
        repository: &OperationRepository,
        now_unix_milliseconds: u64,
    ) -> Result<OperationResponse, RepositoryFailure> {
        Ok(admission_response(repository.admit(&self.request, now_unix_milliseconds)?))
    }
}

fn admission_response(outcome: AdmissionOutcome) -> OperationResponse {
    match outcome {
        AdmissionOutcome::Admitted(row) => {
            OperationResponse::Accepted { operation_identifier: row.operation_identifier }
        }
        AdmissionOutcome::Replayed(row) => {
            OperationResponse::Replayed { operation_identifier: row.operation_identifier }
        }
        AdmissionOutcome::Conflict(row) => {
            OperationResponse::IdentifierConflict { operation_identifier: row.operation_identifier }
        }
    }
}

fn contract_failure() -> OperationResponse {
    OperationResponse::InternalFailure {
        detail: "the installed command binding could not be established".to_owned(),
    }
}
