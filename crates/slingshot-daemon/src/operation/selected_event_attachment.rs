//! One owned selected-author attachment, from stored cursor to durable delivery.

use slingshot_agent_connection::{
    authentication::environment_provider::RequestAuthentication,
    selected_author_events::EventHttpOutcome,
    selected_author_http::FiniteHttpFailure,
    selected_author_transport::SelectedAuthorTransport,
    server_sent_event_decoder::{
        DecoderBounds, EventStreamCursor, OperationStreamExpectation, StreamItem, StreamRefusal,
    },
};
use slingshot_domain::operation_executor::ExecutionIdentity;
use slingshot_storage::{
    agent_subscription_ledger::AgentSubscriptionLedger, operation_repository::OperationRepository,
};

use super::{
    durable_author_event::{DurableEventOutcome, DurableEventRefusal, fold_selected_event},
    subscription_reset::ResetTransport,
};

/// The one attachment ended or stopped for independent recovery authority.
pub enum SelectedEventAttachmentOutcome<'runtime> {
    /// Single-use terminal event bound to its retained owner and sequence.
    TerminalRecovery(super::terminal_event_recovery::CapturedTerminalEvent<'runtime>),
    /// Complete transport outcome, including authenticated reset evidence.
    Transport(EventHttpOutcome),
    /// Streaming stopped before the named event could be committed.
    Recovery {
        /// Remote operation key, absent for an already-held subscription incident.
        agent_operation_identifier: Option<String>,
        /// Authority needed; never permission to settle or resubmit an operation.
        reason: DurableEventOutcome,
    },
}
impl core::fmt::Debug for SelectedEventAttachmentOutcome<'_> {
    fn fmt(&self, formatter: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        formatter.write_str("SelectedEventAttachmentOutcome([redacted])")
    }
}

/// Attaches once using the ledger's generation and committed cursor.
///
/// This function owns both the terminal resolver and durable event callback.
/// Preflight validates all retained members before any socket opens. Terminal
/// contracts are resolved afresh from storage, never supplied by the caller or
/// inferred from the event. Dropping the future detaches without remote cancel.
/// Reconnection/backoff and recovery remain the supervisor's responsibility.
///
/// # Errors
/// Refuses invalid retained bindings, missing owners, transport failures or a
/// durable write refusal. Earlier committed events remain committed.
pub async fn attach_selected_events<'runtime>(
    ledger: &'runtime AgentSubscriptionLedger,
    operations: &'runtime OperationRepository,
    transport: &SelectedAuthorTransport,
    selection: &ExecutionIdentity,
    subscription: &str,
    authentication: &'runtime RequestAuthentication,
    protocol: ResetTransport,
    now: u64,
) -> Result<SelectedEventAttachmentOutcome<'runtime>, DurableEventRefusal> {
    attach_selected_events_with_authentication(
        ledger,
        operations,
        transport,
        selection,
        subscription,
        super::author_authentication::AuthorAuthentication::Fixed { authentication, protocol },
        now,
    )
    .await
}

/// Attaches using the invocation provider policy and retains it in terminal
/// recovery handoffs. Durable cursor and ownership checks remain unchanged.
pub async fn attach_selected_events_with_authentication<'runtime>(
    ledger: &'runtime AgentSubscriptionLedger,
    operations: &'runtime OperationRepository,
    transport: &SelectedAuthorTransport,
    selection: &ExecutionIdentity,
    subscription: &str,
    authentication: super::author_authentication::AuthorAuthentication<'runtime>,
    now: u64,
) -> Result<SelectedEventAttachmentOutcome<'runtime>, DurableEventRefusal> {
    let started = tokio::time::Instant::now();
    if now > i64::MAX as u64 || !ledger.database().shares_database_with(operations.database()) {
        return Err(DurableEventRefusal);
    }
    transport.require_execution(selection).map_err(|_| DurableEventRefusal)?;
    authentication.require_execution(selection).map_err(|_| DurableEventRefusal)?;
    let view = ledger
        .read_recovery_view(&selection.author_target_identity_digest, subscription)
        .map_err(|_| DurableEventRefusal)?;
    let generation = view.ledger().agent_event_store_generation;
    if view.ledger().unresolved_incident.is_some() {
        return Ok(SelectedEventAttachmentOutcome::Recovery {
            agent_operation_identifier: None,
            reason: DurableEventOutcome::NeedsIntegrityRecovery,
        });
    }
    let resolve_member = |member: &slingshot_storage::agent_job_repository::AgentSubmission,
                          completed: bool|
     -> Result<OperationStreamExpectation, DurableEventRefusal> {
        let submission =
            super::subscription_reset::restore(member).map_err(|_| DurableEventRefusal)?;
        let identity = ExecutionIdentity {
            attempt: selection.attempt,
            operation_identifier: member.identity.operation_identifier.clone(),
            author_target_identity_digest: member.identity.author_target_identity_digest.clone(),
            selected_environment_revision: member.identity.selected_environment_revision.clone(),
        };
        transport.require_submission(&identity, &submission).map_err(|_| DurableEventRefusal)?;
        let local =
            super::durable_author_lookup::retained_command(operations, &identity, &submission)
                .map_err(|_| DurableEventRefusal)?;
        if (local.record.lifecycle_state.is_terminal() && !completed)
            || member.identity.agent_event_store_generation != generation
        {
            return Err(DurableEventRefusal);
        }
        Ok(OperationStreamExpectation {
            daemon_subscription_identifier: submission.daemon_subscription_identifier,
            agent_event_store_generation: member.identity.agent_event_store_generation,
            agent_operation_identifier: member.identity.agent_operation_identifier.clone(),
            expected_provenance: slingshot_agent_protocol::wire_contract::ExpectedProvenance {
                command_contract: (&submission.provenance.command_contract).into(),
                canonical_json_contract_digest: submission
                    .provenance
                    .canonical_json_contract_digest,
                transport_contract_digest: submission.provenance.transport_contract_digest,
            },
            submitted_command_digest: submission.submitted_command_digest,
        })
    };
    for member in view.members() {
        resolve_member(member, false)?;
    }
    let cursor = view
        .ledger()
        .cursor
        .as_deref()
        .map(|cursor| EventStreamCursor::new(cursor, DecoderBounds::embedded().identifier_bytes))
        .transpose()
        .map_err(|_| DurableEventRefusal)?;
    let resolver = |operation: &str| {
        let refusal = || StreamRefusal::Malformed { field: "retained operation" };
        let current = ledger
            .read_recovery_view(&selection.author_target_identity_digest, subscription)
            .map_err(|_| refusal())?;
        if current.ledger().agent_event_store_generation != generation
            || current.ledger().unresolved_incident.is_some()
        {
            return Err(refusal());
        }
        let member = current
            .members()
            .iter()
            .find(|member| member.identity.agent_operation_identifier == operation);
        if let Some(member) = member {
            return resolve_member(member, false).map_err(|_| refusal());
        }
        let completed = ledger
            .read_completed_event(&selection.author_target_identity_digest, subscription, operation)
            .map_err(|_| refusal())?
            .ok_or_else(refusal)?;
        resolve_member(completed.member(), true).map_err(|_| refusal())
    };
    let mut recovery = None;
    let consume = |item| {
        let StreamItem::Event(event) = item else { return Ok(()) };
        let elapsed = started.elapsed();
        let milliseconds = u64::try_from(elapsed.as_millis())
            .ok()
            .and_then(|value| value.checked_add(u64::from(elapsed.subsec_nanos() % 1_000_000 != 0)))
            .and_then(|value| now.checked_add(value))
            .ok_or(FiniteHttpFailure::Body)?;
        let outcome = fold_selected_event(
            ledger,
            operations,
            transport,
            selection,
            subscription,
            &event,
            milliseconds,
        )
        .map_err(|_| FiniteHttpFailure::Body)?;
        if matches!(outcome, DurableEventOutcome::Recorded(_)) {
            return Ok(());
        }
        recovery = Some(if outcome == DurableEventOutcome::NeedsTerminalLookup {
            SelectedEventAttachmentOutcome::TerminalRecovery(
                super::terminal_event_recovery::CapturedTerminalEvent::capture(
                    ledger,
                    operations,
                    transport,
                    selection,
                    *event,
                    authentication,
                    milliseconds,
                )
                .map_err(|_| FiniteHttpFailure::Body)?,
            )
        } else {
            SelectedEventAttachmentOutcome::Recovery {
                agent_operation_identifier: Some(event.event.agent_operation_identifier.clone()),
                reason: outcome,
            }
        });
        Err(FiniteHttpFailure::Body)
    };
    let result = authentication
        .events(transport, selection, subscription, generation, cursor.as_ref(), resolver, consume)
        .await;
    if let Some(recovery) = recovery {
        return Ok(recovery);
    }
    result.map(SelectedEventAttachmentOutcome::Transport).map_err(|_| DurableEventRefusal)
}
