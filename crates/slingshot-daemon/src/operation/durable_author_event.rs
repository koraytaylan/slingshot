//! Selected-author event observations committed with their subscription cursor.

use slingshot_agent_connection::{
    job_event_reducer::{AssociationBinding, JobDisposition, RetainedJob, reduce_decoded},
    selected_author_transport::SelectedAuthorTransport,
    server_sent_event_decoder::DecodedEvent,
};
use slingshot_agent_protocol::wire_contract::ExpectedProvenance;
use slingshot_domain::operation_executor::ExecutionIdentity;
use slingshot_storage::{
    agent_subscription_ledger::{AgentSubscriptionLedger, EventFact, LedgerOutcome},
    operation_repository::OperationRepository,
};

/// One event's durable effect or the authority needed before streaming resumes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DurableEventOutcome {
    /// Cursor and any believed active job observation committed together.
    Recorded(LedgerOutcome),
    /// The job sequence has a gap; no cursor or job fact changed.
    NeedsSnapshot,
    /// Terminal correlation is not a complete typed result/failure document.
    NeedsTerminalLookup,
    /// Conflicting accounts require reconciliation; no cursor advanced.
    NeedsIntegrityRecovery,
}

/// Redacted refusal of event identity, selected binding or durable guards.
#[derive(Debug, thiserror::Error)]
#[error("selected author event could not be committed")]
pub struct DurableEventRefusal;

/// Folds a decoded event from this selected subscription into durable storage.
///
/// The caller binds the decoder to `subscription` and the selected transport.
/// Unknown operations move only the cursor. Associated operations restore their
/// exact retained submission and independently validate their local command.
/// Terminal results, gaps and conflicts never advance past unprocessed evidence.
/// This synchronous callback holds no database transaction across network I/O.
///
/// # Errors
/// Refuses missing cursors, another selection/generation, changed local state or
/// invalid retained contracts. No refused event advances the cursor.
pub fn fold_selected_event(
    ledger: &AgentSubscriptionLedger,
    operations: &OperationRepository,
    transport: &SelectedAuthorTransport,
    selection: &ExecutionIdentity,
    subscription: &str,
    event: &DecodedEvent,
    now: u64,
) -> Result<DurableEventOutcome, DurableEventRefusal> {
    if event.daemon_subscription_identifier != subscription
        || !ledger.database().shares_database_with(operations.database())
    {
        return Err(DurableEventRefusal);
    }
    transport.require_execution(selection).map_err(|_| DurableEventRefusal)?;
    let view = ledger
        .read_recovery_view(&selection.author_target_identity_digest, subscription)
        .map_err(|_| DurableEventRefusal)?;
    if view.ledger().agent_event_store_generation != event.event.agent_event_store_generation {
        return Err(DurableEventRefusal);
    }
    if view.ledger().unresolved_incident.is_some() {
        return Ok(DurableEventOutcome::NeedsIntegrityRecovery);
    }
    let cursor = event.cursor.as_ref().ok_or(DurableEventRefusal)?;
    let mut fact = EventFact {
        agent_event_store_generation: event.event.agent_event_store_generation,
        agent_operation_identifier: None,
        canonical_digest: event.canonical_digest.clone(),
        cursor: cursor.as_text().to_owned(),
        event_bytes: event.canonical_bytes,
        job_sequence: None,
    };
    // A subscription-position conflict takes precedence over job sequencing:
    // a gap or stale job event cannot hide two different accounts of one cursor.
    if view.ledger().cursor.as_deref() == Some(cursor.as_text())
        && view
            .ledger()
            .canonical_digest
            .as_ref()
            .is_some_and(|held| held != &event.canonical_digest)
    {
        ledger.record_event_conflict(&view, cursor.as_text()).map_err(|_| DurableEventRefusal)?;
        return Ok(DurableEventOutcome::NeedsIntegrityRecovery);
    }
    let active = view.members().iter().find(|member| {
        member.identity.agent_operation_identifier == event.event.agent_operation_identifier
    });
    let completed = if active.is_none() {
        ledger
            .read_completed_event(
                &selection.author_target_identity_digest,
                subscription,
                &event.event.agent_operation_identifier,
            )
            .map_err(|_| DurableEventRefusal)?
    } else {
        None
    };
    let outcome = if let Some(member) = active.or_else(|| completed.as_ref().map(|held| held.member())) {
        let submission = super::subscription_reset::restore(member).map_err(|_| DurableEventRefusal)?;
        let identity = ExecutionIdentity {
            attempt: selection.attempt,
            operation_identifier: member.identity.operation_identifier.clone(),
            author_target_identity_digest: member.identity.author_target_identity_digest.clone(),
            selected_environment_revision: member.identity.selected_environment_revision.clone(),
        };
        transport.require_submission(&identity, &submission).map_err(|_| DurableEventRefusal)?;
        super::durable_author_lookup::retained_command(operations, &identity, &submission)
            .map_err(|_| DurableEventRefusal)?;
        let binding = AssociationBinding {
            expected_provenance: ExpectedProvenance {
                command_contract: slingshot_domain::selected_command_contract_identity::SelectedCommandContractIdentity::installed(&member.contracts.command_wire_name)
                    .map_err(|_| DurableEventRefusal)?,
                canonical_json_contract_digest: slingshot_domain::command::schema::canonical_contract_digest(),
                transport_contract_digest: slingshot_domain::author_agent_transport_contract::AuthorAgentTransportContract::embedded_digest(),
            },
            selected_environment_revision: identity.selected_environment_revision,
            submitted_command_digest: submission.submitted_command_digest,
        };
        let (disposition, observation) = reduce_decoded(&RetainedJob {
            observation: member.observation, snapshot_watermark: member.snapshot_watermark,
        }, &binding, member.identity.agent_event_store_generation,
            &member.identity.agent_operation_identifier, event).map_err(|_| DurableEventRefusal)?;
        fact.agent_operation_identifier = Some(member.identity.agent_operation_identifier.clone());
        fact.job_sequence = Some(event.event.sequence);
        if completed.is_some() && matches!(disposition, JobDisposition::Applied | JobDisposition::NeedsSnapshot) {
            return Err(DurableEventRefusal);
        }
        match disposition {
            JobDisposition::NeedsSnapshot => return Ok(DurableEventOutcome::NeedsSnapshot),
            JobDisposition::IntegrityConflictNeedsReconciliation => {
                ledger.record_event_conflict(&view, cursor.as_text()).map_err(|_| DurableEventRefusal)?;
                return Ok(DurableEventOutcome::NeedsIntegrityRecovery);
            }
            JobDisposition::Applied if event.event.kind.is_terminal() => return Ok(DurableEventOutcome::NeedsTerminalLookup),
            JobDisposition::Applied => ledger.record_active_event(&view, &fact,
                observation.ok_or(DurableEventRefusal)?, &event.sling_job_identifier, now),
            JobDisposition::ExactReplay | JobDisposition::StaleCursorOnly => {
                if let Some(completed) = &completed {
                    ledger.record_completed_event_cursor(completed, &fact, &event.sling_job_identifier, now)
                } else { ledger.record_cursor_event(&view, &fact, now) }
            }
        }
    } else {
        ledger.record_cursor_event(&view, &fact, now)
    }.map_err(|_| DurableEventRefusal)?;
    Ok(if outcome == LedgerOutcome::IntegrityConflict {
        DurableEventOutcome::NeedsIntegrityRecovery
    } else {
        DurableEventOutcome::Recorded(outcome)
    })
}
