//! Scheduling holds the local runtime mutex only for synchronous database work.
//! Network polling owns separate live connections and never owns that mutex.

use super::{DurableRuntime, unix_milliseconds};
use slingshot_agent_connection::command_submission::Submission;
use slingshot_domain::operation_executor::ExecutionIdentity;
use tokio_util::sync::CancellationToken;

struct ScheduledInvocation {
    identity: ExecutionIdentity,
    submission: Submission,
}

const SCHEDULER_POLL_MILLISECONDS: u64 = 50;

pub(super) async fn scheduler_loop(
    runtime: std::sync::Arc<std::sync::Mutex<DurableRuntime>>,
    shutdown: CancellationToken,
) {
    static NEXT_FENCE: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);
    let execution = {
        let guard = runtime.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        match guard.execution() {
            Ok(execution) => execution,
            Err(_) => {
                let _ = guard.diagnostics().record("scheduler execution connections unavailable");
                shutdown.cancel();
                return;
            }
        }
    };
    let interval = std::time::Duration::from_millis(SCHEDULER_POLL_MILLISECONDS);
    loop {
        if shutdown.is_cancelled() {
            return;
        }
        let fence = NEXT_FENCE.fetch_add(1, std::sync::atomic::Ordering::Relaxed).max(1);
        let now = unix_milliseconds();
        let invocation = {
            let guard = runtime.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
            prepare(&guard, fence, now)
        };
        let Some(invocation) = invocation else {
            tokio::select! {
                biased;
                () = shutdown.cancelled() => return,
                () = tokio::time::sleep(interval) => {},
            }
            continue;
        };
        struct NoopProgress;
        impl slingshot_domain::operation_executor::ProgressPort for NoopProgress {
            fn report(&self, _detail: &str) {}
        }
        let outcome = tokio::select! {
            biased;
            () = shutdown.cancelled() => return,
            outcome = execution.execute_retained_with_claim(
                &invocation.identity, invocation.submission, &NoopProgress, fence, now,
            ) => outcome,
        };
        let guard = runtime.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        if let Ok(outcome) = outcome {
            let identity = &invocation.identity;
            let _settled = guard.operations()
                .read(&identity.author_target_identity_digest, &identity.operation_identifier)
                .ok().flatten()
                .map_or(Err(slingshot_storage::operation_repository::RepositoryFailure::NoSuchOperation {
                    identifier: identity.operation_identifier.clone(),
                }), |current| {
                    guard.settle_execution_with_scheduler_fence(&current, &outcome, unix_milliseconds(), fence)
                });
        } else {
            let _ = guard.diagnostics().record("scheduler execution refused");
        }
    }
}

fn prepare(runtime: &DurableRuntime, fence: u64, now: u64) -> Option<ScheduledInvocation> {
    let lease = now.saturating_add(
        slingshot_domain::author_agent_transport_contract::AuthorAgentTransportContract::embedded()
            .limit("worker_execution_lease_milliseconds"),
    );
    let claim = match runtime.claim_next_scheduled_operation(fence, lease, now) {
        Ok(claim) => claim,
        Err(_) => {
            let _ = runtime.diagnostics().record("scheduler claim failed");
            return None;
        }
    };
    let claim = claim?;
    let target = runtime.context().target();
    let input = match runtime
        .operations()
        .read_execution_input(&target.author_target_identity_digest, &claim.operation_identifier)
    {
        Ok(Some(input)) => input,
        _ => {
            let _ = runtime.diagnostics().record("scheduler input read failed");
            return None;
        }
    };
    let attempt = input
        .summary
        .record
        .outstanding_recovery
        .as_ref()
        .map_or(1, |recovery| recovery.attempt_count.saturating_add(1));
    let identity = slingshot_domain::operation_executor::ExecutionIdentity {
        attempt,
        author_target_identity_digest: input.summary.author_target_identity_digest.clone(),
        selected_environment_revision: input.summary.selected_environment_revision.clone(),
        operation_identifier: input.summary.operation_identifier.clone(),
    };
    let generation = slingshot_domain::agent_identity::AgentEventStoreGeneration::first();
    let subscription = slingshot_domain::agent_identity::DaemonSubscriptionIdentifier::derive(
        runtime.installation().as_text(),
        &identity.author_target_identity_digest,
        &identity.selected_environment_revision,
        generation,
    );
    let _ = runtime.subscriptions().open_subscription(
        &identity.author_target_identity_digest,
        subscription.as_text(),
        generation.value(),
        now,
    );
    let expected = slingshot_agent_protocol::wire_contract::ExpectedProvenance {
            canonical_json_contract_digest:
                slingshot_domain::command::schema::canonical_contract_digest(),
            command_contract:
                match slingshot_domain::selected_command_contract_identity::SelectedCommandContractIdentity::installed(
                    &input.summary.command_wire_name,
                ) {
                    Ok(contract) => contract,
                    Err(_) => {
                        return None;
                    }
                },
            transport_contract_digest:
                slingshot_domain::author_agent_transport_contract::AuthorAgentTransportContract::embedded_digest(),
        };
    let operation = slingshot_agent_protocol::identity::WireOperationIdentity::of(
        &identity.author_target_identity_digest,
        &identity.selected_environment_revision,
        &identity.operation_identifier,
        generation,
    );
    let Ok(submission) = slingshot_agent_connection::command_submission::Submission::build(
        &expected,
        operation,
        subscription.as_text(),
        &input.canonical_command,
        slingshot_agent_connection::command_submission::ExpectedArtifactManifest::empty(),
    ) else {
        return None;
    };

    Some(ScheduledInvocation { identity, submission })
}
