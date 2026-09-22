//! Scheduling holds the local runtime mutex only for synchronous database work.
//! Network polling owns separate live connections and never owns that mutex.
//!
//! # One execution can never exclude the others
//!
//! A claim is a claim on one operation, never on the scheduler. Each execution
//! runs as its own task over its own live connections and under the contract's
//! own in-flight bound, so a single slow, stuck, or dead author connection
//! consumes one slot and no more. The loop that claims work therefore keeps
//! claiming whatever else is eligible while an execution is still in the air.
//!
//! # Time cannot be a reason to forget work
//!
//! An execution that outlives the contract's total attempt budget is detached,
//! not settled by the passage of time: its claim is released so the row is
//! claimable again, and its outcome stays whatever the durable evidence already
//! says. Bounding the wait is how the daemon stays answerable; it is never how
//! an operation is decided. A detached attempt never resends, because the
//! durable child it may have left behind sends the next attempt to lookup.

use super::{DurableRuntime, unix_milliseconds};
use slingshot_agent_connection::command_submission::Submission;
use slingshot_domain::operation_executor::ExecutionIdentity;
use tokio_util::sync::CancellationToken;

struct ScheduledInvocation {
    identity: ExecutionIdentity,
    submission: Submission,
}

/// How often an idle scheduler looks for newly eligible work.
const SCHEDULER_POLL_MILLISECONDS: u64 = 50;

static NEXT_FENCE: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);

/// How many executions may be in the air at once.
///
/// Read from the runtime contract rather than chosen here: a scheduler that
/// invented its own width would admit different amounts of work depending on
/// which build was running.
fn maximum_in_flight() -> usize {
    usize::try_from(
        slingshot_domain::daemon_runtime_contract::DaemonRuntimeContract::embedded()
            .limit("maximum_global_in_flight_operations"),
    )
    .unwrap_or(usize::MAX)
}

/// Returns the longest one whole attempt may take before it is detached.
///
/// Every phase one attempt can legitimately spend time in is read from the
/// transport contract and added up, so the budget can never fire ahead of a
/// phase the contract itself still allows. Once it does fire, the attempt is
/// detached rather than settled: the row is released for a later attempt, which
/// reconciles what it finds by lookup and never resends. Nothing remote is
/// cancelled and no outcome is invented, so a detached attempt costs a lookup
/// and can never turn a running job into a result this daemon guessed.
fn attempt_budget_milliseconds() -> u64 {
    let contract =
        slingshot_domain::author_agent_transport_contract::AuthorAgentTransportContract::embedded();
    [
        "author_connect_timeout_milliseconds",
        "author_tls_timeout_milliseconds",
        "author_request_body_timeout_milliseconds",
        "author_response_header_timeout_milliseconds",
        "finite_response_total_timeout_milliseconds",
        "artifact_transfer_total_timeout_milliseconds",
        "worker_execution_lease_milliseconds",
    ]
    .iter()
    .fold(0_u64, |total, name| total.saturating_add(contract.limit(name)))
}

/// One execution that is currently in the air.
struct InFlight {
    handle: tokio::task::JoinHandle<()>,
}

pub(super) async fn scheduler_loop(
    runtime: std::sync::Arc<std::sync::Mutex<DurableRuntime>>,
    shutdown: CancellationToken,
) {
    let interval = std::time::Duration::from_millis(SCHEDULER_POLL_MILLISECONDS);
    let mut in_flight: Vec<InFlight> = Vec::new();
    loop {
        reap_finished(&mut in_flight);
        if shutdown.is_cancelled() {
            // Every task owns its own claim release on cancellation, so awaiting
            // them here is what makes a stop leave no claim behind.
            for held in in_flight {
                let _ = held.handle.await;
            }
            return;
        }
        {
            let guard = runtime.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
            heal_paused_queue(&guard, unix_milliseconds());
        }
        while in_flight.len() < maximum_in_flight() {
            let fence = NEXT_FENCE.fetch_add(1, std::sync::atomic::Ordering::Relaxed).max(1);
            let now = unix_milliseconds();
            let invocation = {
                let guard = runtime.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
                prepare(&guard, fence, now)
            };
            let Some(invocation) = invocation else {
                break;
            };
            // Each execution owns its own live connections, so two in the air
            // can never share one SQLite transaction. Opening happens only once
            // work is actually held, so an idle daemon opens nothing.
            let execution = {
                let guard = runtime.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
                match guard.execution() {
                    Ok(execution) => execution,
                    Err(_) => {
                        release_claim_unless_paused(&guard, &invocation.identity, fence);
                        let _ = guard
                            .diagnostics()
                            .record("scheduler execution connections unavailable");
                        shutdown.cancel();
                        return;
                    }
                }
            };
            let runtime = std::sync::Arc::clone(&runtime);
            let shutdown = shutdown.clone();
            let handle = tokio::task::spawn_local(async move {
                run_one(runtime, execution, invocation, fence, now, shutdown).await;
            });
            in_flight.push(InFlight { handle });
        }
        tokio::select! {
            biased;
            () = shutdown.cancelled() => {},
            () = tokio::time::sleep(interval) => {},
        }
    }
}

/// Runs one claimed execution under a total budget and settles it, or detaches.
///
/// Nothing here decides an operation. It either publishes the executor's own
/// outcome through the same durable fence it was claimed under, or it leaves
/// the row exactly as it was and releases the claim so the work is reachable
/// again. Cancellation and expiry are the second case; neither is an ending.
async fn run_one(
    runtime: std::sync::Arc<std::sync::Mutex<DurableRuntime>>,
    execution: crate::runtime_builder::execution::RuntimeExecution,
    invocation: ScheduledInvocation,
    fence: u64,
    now: u64,
    shutdown: CancellationToken,
) {
    struct NoopProgress;
    impl slingshot_domain::operation_executor::ProgressPort for NoopProgress {
        fn report(&self, _detail: &str) {}
    }
    let budget = std::time::Duration::from_millis(attempt_budget_milliseconds());
    let outcome = tokio::select! {
        biased;
        () = shutdown.cancelled() => None,
        outcome = tokio::time::timeout(
            budget,
            execution.execute_retained_with_claim(
                &invocation.identity,
                invocation.submission,
                &NoopProgress,
                fence,
                now,
            ),
        ) => outcome.ok(),
    };
    let guard = runtime.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
    let Some(outcome) = outcome else {
        // The execution produced no outcome this daemon may publish. Its
        // attempt is charged durably first, so a permanently stuck connection
        // reaches the automatic budget and pauses for a person instead of being
        // reclaimed and detached forever; then its claim is released so a later
        // attempt can reconcile the row from durable evidence. The row keeps
        // whatever evidence it already held: a local timeout is never an ending
        // and never a reason to believe the remote did not run.
        charge_detached_attempt(&guard, &invocation.identity);
        release_claim_unless_paused(&guard, &invocation.identity, fence);
        let _ = guard.diagnostics().record("scheduler execution detached");
        return;
    };
    let Ok(outcome) = outcome else {
        // A refused execution settled nothing, so its claim must not outlive it:
        // leaving it checkpointed would make the row permanently unclaimable,
        // which is exactly what a refusal must never cause.
        release_claim_unless_paused(&guard, &invocation.identity, fence);
        let _ = guard.diagnostics().record("scheduler execution refused");
        return;
    };
    let identity = &invocation.identity;
    let settled = guard
        .operations()
        .read(&identity.author_target_identity_digest, &identity.operation_identifier)
        .ok()
        .flatten()
        .map_or(
            Err(slingshot_storage::operation_repository::RepositoryFailure::NoSuchOperation {
                identifier: identity.operation_identifier.clone(),
            }),
            |current| {
                guard.settle_execution_with_scheduler_fence(
                    &current,
                    &outcome,
                    unix_milliseconds(),
                    fence,
                )
            },
        );
    if settled.is_err() {
        // The fenced write did not settle the operation, so the claim it held
        // is released here rather than left to expire. A pause a person has to
        // release is the one claim that must survive, and it is left alone.
        release_claim_unless_paused(&guard, identity, fence);
    }
    // The row may already have been settled by an unfenced write (a lookup
    // reconciliation ahead of this settle call), in which case the fenced write
    // above refused and the transition still happened. Publish whatever the row
    // now holds, so a blocked observer learns the outcome whichever path wrote it.
    if let Ok(Some(current)) = guard
        .operations()
        .read(&identity.author_target_identity_digest, &identity.operation_identifier)
    {
        publish_settlement(&guard, &current);
    }
}

/// Forgets the executions that have already finished, whether or not they
/// published an outcome. Their fences were released by `run_one`.
fn reap_finished(in_flight: &mut Vec<InFlight>) {
    in_flight.retain(|held| !held.handle.is_finished());
}

/// Charges one detached attempt against the operation's automatic budget.
///
/// Best-effort by design: the claim release that follows is what makes the work
/// reachable again, and it must happen whether or not this charge could be
/// written. A refused charge leaves the row exactly as it was, which is the
/// same outcome the attempt would have had if it had never been claimed.
fn charge_detached_attempt(runtime: &DurableRuntime, identity: &ExecutionIdentity) {
    let revision = runtime
        .operations()
        .read(&identity.author_target_identity_digest, &identity.operation_identifier)
        .ok()
        .flatten()
        .map(|summary| summary.record.revision);
    let Some(revision) = revision else {
        return;
    };
    let _ = crate::operation::durable_author_lookup::record_detached_attempt(
        runtime.operations(),
        identity,
        revision,
        unix_milliseconds(),
    );
}

/// Releases one attempt's claim unless a person is being waited on.
///
/// A manually resumable recovery is the one hold a claim encodes rather than
/// merely occupies: releasing it would make the scheduler retry work ahead of
/// the person who was asked to decide, so it is left exactly as it is.
fn release_claim_unless_paused(runtime: &DurableRuntime, identity: &ExecutionIdentity, fence: u64) {
    let paused = runtime
        .operations()
        .read(&identity.author_target_identity_digest, &identity.operation_identifier)
        .ok()
        .flatten()
        .and_then(|summary| summary.record.outstanding_recovery)
        .is_some_and(|fact| fact.manual_resume_eligible);
    if paused {
        return;
    }
    let _ = slingshot_storage::operation::scheduler_claim::release(
        runtime.database(),
        &identity.author_target_identity_digest,
        &identity.operation_identifier,
        fence,
    );
}

/// Tells every local waiter where the operation it is watching has got to.
///
/// A wait is a subscription to persisted revisions, and the scheduler is the
/// only writer on the execution path, so this is where a blocked observer is
/// released. The durable write has already committed, so a waiter told the
/// truth here is a waiter whose read can never disagree with the row.
fn publish_settlement(
    runtime: &DurableRuntime,
    current: &slingshot_storage::operation_repository::OperationSummary,
) {
    use crate::operation_wait::WaitUpdate;
    let update = if current.record.lifecycle_state.is_terminal() {
        WaitUpdate::Terminal { revision: current.record.revision }
    } else if current.record.outstanding_recovery.is_some() {
        WaitUpdate::RecoveryRequired { revision: current.record.revision }
    } else {
        let detail = current.record.latest_progress.clone().unwrap_or_else(|| {
            serde_json::to_value(current.record.lifecycle_state)
                .ok()
                .and_then(|value| value.as_str().map(str::to_owned))
                .unwrap_or_default()
        });
        WaitUpdate::Progress { detail, revision: current.record.revision }
    };
    runtime.waiters().publish(&current.operation_identifier, &update);
}

/// Settles queued work that was paused for a person.
///
/// A paused row never becomes eligible again on its own, and its claim keeps
/// the scheduler from choosing it. Ending it is what lets the queue heal.
fn heal_paused_queue(runtime: &DurableRuntime, now: u64) {
    let target = runtime.context().target().author_target_identity_digest.clone();
    let Ok(paused) =
        slingshot_storage::operation::scheduler_claim::paused_queued(runtime.database(), &target)
    else {
        return;
    };
    for row in paused {
        let failure = if row.evidence_kind == "authoritative_remote_success" {
            slingshot_domain::operation::TerminalFailure {
                kind: slingshot_domain::operation::TerminalFailureKind::ResultUnavailable,
                disposition:
                    slingshot_domain::operation::TerminalFailureDisposition::AuthoritativeRemoteSuccess,
                metadata: Some("the author answered and the result was not obtained".to_owned()),
            }
        } else {
            slingshot_domain::operation::TerminalFailure {
                kind: slingshot_domain::operation::TerminalFailureKind::RetryPolicyExhausted,
                disposition: slingshot_domain::operation::TerminalFailureDisposition::FailClosedIndeterminate {
                    certainty:
                        slingshot_domain::operation::OperationExecutionCertainty::RemoteOutcomeUnknown,
                },
                metadata: Some("the author did not answer before the retry budget ended".to_owned()),
            }
        };
        let Ok(summary) = runtime.operations().apply(
            &target,
            &row.operation_identifier,
            row.revision,
            &slingshot_domain::operation::OperationFact::Terminal { failure },
            now,
        ) else {
            continue;
        };
        if let Some(fence) = row.fence {
            let _ = slingshot_storage::operation::scheduler_claim::release(
                runtime.database(),
                &target,
                &row.operation_identifier,
                fence,
            );
        }
        publish_settlement(runtime, &summary);
    }
}

/// Runs one admitted command to a terminal result inside the request that submitted it.
///
/// The author finishes a shipped command in that same call. Leaving the row
/// queued would make the caller poll a handle, so a result that is not terminal
/// is ended here instead of being handed to a worker.
pub(crate) fn complete_submitted(
    runtime: &std::sync::Arc<std::sync::Mutex<DurableRuntime>>,
    operation_identifier: &str,
) {
    let fence = NEXT_FENCE.fetch_add(1, std::sync::atomic::Ordering::Relaxed).max(1);
    let now = unix_milliseconds();
    let prepared = {
        let guard = runtime.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        let target = guard.context().target().author_target_identity_digest.clone();
        let Some(summary) = guard.operations().read(&target, operation_identifier).ok().flatten()
        else {
            return;
        };
        if summary.record.lifecycle_state.is_terminal() {
            return;
        }
        let lease = now.saturating_add(
            slingshot_domain::author_agent_transport_contract::AuthorAgentTransportContract::embedded()
                .limit("worker_execution_lease_milliseconds"),
        );
        let claimed = slingshot_storage::operation::scheduler_claim::claim(
            guard.database(),
            &target,
            operation_identifier,
            "queued",
            summary.record.revision,
            fence,
            lease,
            now,
        );
        if !matches!(
            claimed,
            Ok(slingshot_storage::operation::scheduler_claim::ClaimOutcome::Claimed)
        ) {
            return;
        }
        let Some(input) =
            guard.operations().read_execution_input(&target, operation_identifier).ok().flatten()
        else {
            return;
        };
        let invocation = schedule_invocation(&guard, &input, now, fence);
        let execution = guard.execution().ok();
        invocation.zip(execution)
    };
    let Some((invocation, execution)) = prepared else {
        return;
    };
    struct SilentProgress;
    impl slingshot_domain::operation_executor::ProgressPort for SilentProgress {
        fn report(&self, _detail: &str) {}
    }
    let outcome = tokio::runtime::Handle::try_current().ok().and_then(|handle| {
        tokio::task::block_in_place(|| {
            handle.block_on(execution.execute_retained_with_claim(
                &invocation.identity,
                invocation.submission.clone(),
                &SilentProgress,
                fence,
                now,
            ))
        })
        .ok()
    });
    let outcome = match outcome {
        Some(
            slingshot_domain::operation_executor::OperationExecutorOutcome::RecoveryRequired {
                recovery,
            },
        ) => unfinished_submission(recovery.detail),
        None => unfinished_submission(String::new()),
        Some(outcome) => outcome,
    };
    let guard = runtime.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
    let identity = &invocation.identity;
    let settled = guard
        .operations()
        .read(&identity.author_target_identity_digest, &identity.operation_identifier)
        .ok()
        .flatten()
        .map(|current| {
            guard.settle_execution_with_scheduler_fence(
                &current,
                &outcome,
                unix_milliseconds(),
                fence,
            )
        });
    if let Some(Ok(current)) = settled {
        publish_settlement(&guard, &current);
    }
}

/// Ends an inline submit that produced no terminal result.
///
/// There is no queue to leave it on. The detail is the executor's own account
/// of why, so a response-head deadline stays a deadline.
fn unfinished_submission(
    detail: String,
) -> slingshot_domain::operation_executor::OperationExecutorOutcome {
    let metadata = if detail.is_empty() {
        "the author did not finish inside this request".to_owned()
    } else {
        detail
    };
    slingshot_domain::operation_executor::OperationExecutorOutcome::TerminalFailure {
        failure: slingshot_domain::operation::TerminalFailure {
            kind: slingshot_domain::operation::TerminalFailureKind::RetryPolicyExhausted,
            disposition:
                slingshot_domain::operation::TerminalFailureDisposition::FailClosedIndeterminate {
                    certainty:
                        slingshot_domain::operation::OperationExecutionCertainty::RemoteOutcomeUnknown,
                },
            metadata: Some(metadata),
        },
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
    let target = runtime.context().target().author_target_identity_digest.clone();
    let input =
        match runtime.operations().read_execution_input(&target, &claim.operation_identifier) {
            Ok(Some(input)) => input,
            _ => {
                let _ = runtime.diagnostics().record("scheduler input read failed");
                let _ = slingshot_storage::operation::scheduler_claim::release(
                    runtime.database(),
                    &target,
                    &claim.operation_identifier,
                    fence,
                );
                return None;
            }
        };
    schedule_invocation(runtime, &input, now, fence)
}

/// Builds the submission for one already claimed operation.
fn schedule_invocation(
    runtime: &DurableRuntime,
    input: &slingshot_storage::operation_repository::RetainedExecutionInput,
    now: u64,
    fence: u64,
) -> Option<ScheduledInvocation> {
    let release_unstarted_claim = || {
        let _ = slingshot_storage::operation::scheduler_claim::release(
            runtime.database(),
            &input.summary.author_target_identity_digest,
            &input.summary.operation_identifier,
            fence,
        );
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
                        release_unstarted_claim();
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
        release_unstarted_claim();
        return None;
    };

    Some(ScheduledInvocation { identity, submission })
}
