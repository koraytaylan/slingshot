//! Attach local observers only after reading the bound persisted operation.

use slingshot_domain::daemon_runtime_contract::DaemonRuntimeContract;
use slingshot_local_protocol::message::{OperationRequest, OperationResponse};
use slingshot_storage::operation_repository::OperationRepository;

use super::{BoundRequest, internal_failure, malformed};
use crate::operation_queries::{OperationResult, result_of};
use crate::operation_wait::WaitUpdate;
use crate::operation_wait::runtime::{AttachRefusal, RuntimeWait, RuntimeWaiters};

impl BoundRequest {
    /// Reads and attaches under the owning service's runtime lock. That same
    /// lock must protect commit/publication to close the status/subscribe race.
    ///
    /// # Errors
    /// Returns a public-safe refusal without changing durable work or retaining
    /// a reader slot. `Ok(None)` is a request owned by another handler.
    pub fn wait(
        &self,
        repository: &OperationRepository,
        waiters: &RuntimeWaiters,
    ) -> Result<Option<RuntimeWait>, OperationResponse> {
        let OperationRequest::Wait { observed_revision, operation_identifier } = self.request()
        else {
            return Ok(None);
        };
        if operation_identifier.is_empty() || operation_identifier.contains('\0') {
            return Err(malformed());
        }
        let summary = repository
            .read(&self.envelope.author_target_identity_digest, operation_identifier)
            .map_err(|_| internal_failure())?
            .ok_or_else(|| OperationResponse::MissingOperation {
                operation_identifier: operation_identifier.clone(),
            })?;
        let revision = summary.record.revision;
        let update = match result_of(&summary).map_err(|_| internal_failure())? {
            OperationResult::Succeeded { .. } | OperationResult::Failed { .. } => {
                if !summary.record.lifecycle_state.is_terminal() {
                    return Err(internal_failure());
                }
                WaitUpdate::Terminal { revision }
            }
            OperationResult::RecoveryRequired { recovery } => {
                if summary.record.lifecycle_state.is_terminal()
                    || !recovery.category.admits(recovery.evidence)
                {
                    return Err(internal_failure());
                }
                WaitUpdate::RecoveryRequired { revision }
            }
            OperationResult::Pending { lifecycle_state } => {
                if lifecycle_state.is_terminal() {
                    return Err(internal_failure());
                }
                let detail = match summary.record.latest_progress {
                    Some(detail) => detail,
                    None => serde_json::to_value(lifecycle_state)
                        .ok()
                        .and_then(|value| value.as_str().map(str::to_owned))
                        .ok_or_else(internal_failure)?,
                };
                if detail.len() as u64
                    > DaemonRuntimeContract::embedded().limit("maximum_progress_detail_bytes")
                {
                    return Err(internal_failure());
                }
                WaitUpdate::Progress { detail, revision }
            }
        };
        waiters.attach(operation_identifier, observed_revision.unwrap_or_default(), update)
            .map(Some).map_err(|failure| match failure {
                AttachRefusal::FutureRevision => malformed(),
                AttachRefusal::Capacity | AttachRefusal::OperationCapacity(_) => OperationResponse::WaiterCapacityExhausted {
                    guidance: "retry observation after a local reader disconnects; the operation continues unchanged".to_owned(),
                },
                AttachRefusal::Stopping => OperationResponse::InternalFailure {
                    detail: "the daemon is stopping; reconnect to observe retained work".to_owned(),
                },
            })
    }
}
