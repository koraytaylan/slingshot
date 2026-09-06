//! Authenticated capture and staged snapshots; no partial reset or resend.
use serde::Deserialize;
use slingshot_agent_connection::{
    authentication::environment_provider::RequestAuthentication,
    command_submission::{ExpectedArtifactManifest, ManifestKind, Submission},
    event_stream_reset::ValidatedEventReset,
    selected_author_lookup::OperationLookupReceipt,
    selected_author_transport::SelectedAuthorTransport,
    server_sent_event_decoder::EventStreamCursor,
    subscription_high_water::HighWaterOutcome,
};
use slingshot_domain::operation_executor::ExecutionIdentity;
use slingshot_storage::{
    agent_job_repository::{AgentSubmission, SubmissionContracts},
    agent_subscription_ledger::{ActiveResetSnapshot, AgentSubscriptionLedger},
    operation_repository::OperationRepository,
};

/// Immutable connection policy for durable author work. Automatic mode selects
/// a codec on each original socket; no mode permits fallback or a replayed POST.
#[derive(Debug, Clone, Copy)]
pub enum ResetTransport {
    /// Negotiate HTTP/1.1 or HTTP/2 on each original selected-author socket.
    Automatic,
    /// Selected HTTP/1.1 transport.
    Http1,
    /// Strict negotiated/prior-knowledge HTTP/2 transport.
    Http2,
}

/// Progress after one bounded capture-and-snapshot attempt.
pub enum SubscriptionResetOutcome<'runtime> {
    /// The complete staged active membership and boundary committed together.
    Installed {
        /// Committed generation.
        generation: u64,
        /// Committed position to use on a subsequent event attachment.
        cursor: EventStreamCursor,
    },
    /// Generation-loss recovery must examine retained physical-job identities.
    GenerationChanged(ValidatedEventReset),
    /// Validated terminal or absence receipt retained for the guarded operation
    /// recovery path. The subscription incident and cursor are still unchanged.
    CapturedOperationRecovery(CapturedResetRecovery<'runtime>),
    /// A terminal/absent snapshot needs the existing per-operation recovery or
    /// settlement path. No member or cursor from this reset attempt was written.
    NeedsOperationRecovery {
        /// Persisted local operation that needs recovery, not a new identity.
        operation_identifier: String,
    },
}
impl core::fmt::Debug for SubscriptionResetOutcome<'_> {
    fn fmt(&self, formatter: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        formatter.write_str("SubscriptionResetOutcome([redacted])")
    }
}

/// Single-use handoff of independently validated lookup evidence. Private fields
/// prevent manufactured receipts and retain the original local database owner.
pub struct CapturedResetRecovery<'runtime> {
    authentication: super::author_authentication::AuthorAuthentication<'runtime>,
    operations: &'runtime OperationRepository,
    identity: ExecutionIdentity,
    submission: Submission,
    expected_revision: u64,
    receipt: OperationLookupReceipt,
    received: std::time::Instant,
    reset_started: std::time::Instant,
    reset_started_at_unix_milliseconds: u64,
}
impl core::fmt::Debug for CapturedResetRecovery<'_> {
    fn fmt(&self, formatter: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        formatter.write_str("CapturedResetRecovery([redacted])")
    }
}
impl CapturedResetRecovery<'_> {
    /// Local operation the captured receipt belongs to.
    pub fn operation_identifier(&self) -> &str {
        &self.identity.operation_identifier
    }

    /// Consumes this evidence through the existing identity, revision, command,
    /// result/failure and retention checks without another logical lookup or POST.
    /// Artifact acquisition may still use the selected transport. This never
    /// clears a subscription incident or installs its captured cursor.
    pub async fn reconcile(
        self,
        repository: &slingshot_storage::agent_job_repository::AgentJobRepository,
        transport: &SelectedAuthorTransport,
        authentication: &RequestAuthentication,
        completion: Option<(
            &slingshot_storage::artifact_store::ArtifactStore,
            &slingshot_storage::persistent_capacity::PersistentCapacityAccount<'_>,
        )>,
    ) -> Result<OperationLookupReceipt, SubscriptionResetRefusal> {
        use super::author_authentication::AuthorAuthentication;
        let policy = match self.authentication {
            AuthorAuthentication::Fixed { protocol, .. } => {
                AuthorAuthentication::Fixed { authentication, protocol }
            }
            policy @ (AuthorAuthentication::Provider { .. }
            | AuthorAuthentication::AsyncProvider { .. }) => policy,
        };
        self.reconcile_using(repository, transport, policy, completion).await
    }

    /// Completes using the captured provider policy without lending a fixed
    /// credential. No new lookup, cursor installation or POST is authorized.
    pub async fn reconcile_saved(
        self,
        repository: &slingshot_storage::agent_job_repository::AgentJobRepository,
        transport: &SelectedAuthorTransport,
        completion: Option<(
            &slingshot_storage::artifact_store::ArtifactStore,
            &slingshot_storage::persistent_capacity::PersistentCapacityAccount<'_>,
        )>,
    ) -> Result<OperationLookupReceipt, SubscriptionResetRefusal> {
        let authentication = self.authentication;
        self.reconcile_using(repository, transport, authentication, completion).await
    }

    async fn reconcile_using(
        mut self,
        repository: &slingshot_storage::agent_job_repository::AgentJobRepository,
        transport: &SelectedAuthorTransport,
        authentication: super::author_authentication::AuthorAuthentication<'_>,
        completion: Option<(
            &slingshot_storage::artifact_store::ArtifactStore,
            &slingshot_storage::persistent_capacity::PersistentCapacityAccount<'_>,
        )>,
    ) -> Result<OperationLookupReceipt, SubscriptionResetRefusal> {
        if let OperationLookupReceipt::Found(found) = &mut self.receipt {
            found.remaining_retention_milliseconds =
                found.remaining_retention_milliseconds.saturating_sub(elapsed(self.received)?);
        }
        let now = self
            .reset_started_at_unix_milliseconds
            .checked_add(elapsed(self.reset_started)?)
            .ok_or(SubscriptionResetRefusal)?;
        super::durable_author_lookup::reconcile_captured_lookup_with_authentication(
            repository,
            self.operations,
            self.expected_revision,
            transport,
            &self.identity,
            &self.submission,
            authentication,
            now,
            completion,
            self.receipt,
        )
        .await
        .map_err(|_| SubscriptionResetRefusal)
    }
}

/// A refused attempt leaves cursor/incident and staged job facts unchanged.
#[derive(Debug, Clone, Copy, thiserror::Error)]
#[error("the subscription reset could not establish complete reconciled truth")]
pub struct SubscriptionResetRefusal;

/// Installs a validated new-generation boundary only after every retained local
/// operation has finished its independent recovery/settlement. Missing local
/// owners and pending result acquisition still prevent this transition. The
/// empty-membership check and boundary installation share one write transaction.
/// No retained operation identifier or generation is rewritten.
pub fn finish_generation_reset(
    ledger: &AgentSubscriptionLedger,
    operations: &OperationRepository,
    transport: &SelectedAuthorTransport,
    selection: &ExecutionIdentity,
    reset: &ValidatedEventReset,
) -> Result<EventStreamCursor, SubscriptionResetRefusal> {
    if !ledger.database().shares_database_with(operations.database()) {
        return Err(SubscriptionResetRefusal);
    }
    transport.require_execution(selection).map_err(|_| SubscriptionResetRefusal)?;
    let view = ledger
        .read_recovery_view(&selection.author_target_identity_digest, reset.subscription())
        .map_err(|_| SubscriptionResetRefusal)?;
    if !view.members().is_empty()
        || view.ledger().agent_event_store_generation != reset.requested_generation()
        || reset.generation() == reset.requested_generation()
        || reset
            .requested_cursor()
            .is_some_and(|cursor| view.ledger().cursor.as_deref() != Some(cursor))
    {
        return Err(SubscriptionResetRefusal);
    }
    ledger
        .install_empty_recovery(&view, reset.generation(), reset.captured_cursor().as_text())
        .map_err(|_| SubscriptionResetRefusal)?;
    Ok(reset.captured_cursor().clone())
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RetainedWire {
    canonical_arguments: String,
    daemon_subscription_identifier: String,
    artifact_manifest: RetainedManifest,
    operation: slingshot_agent_protocol::identity::WireOperationIdentity,
    provenance: slingshot_agent_protocol::identity::DocumentProvenance,
    submitted_command_digest: String,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RetainedManifest {
    artifact_bytes: u64,
    artifact_rows: u64,
    kind: String,
}

pub(super) fn restore(member: &AgentSubmission) -> Result<Submission, SubscriptionResetRefusal> {
    let bound =
        slingshot_domain::author_agent_transport_contract::AuthorAgentTransportContract::embedded()
            .limit("maximum_agent_protocol_document_bytes");
    if member.canonical_submission.len() as u64 > bound {
        return Err(SubscriptionResetRefusal);
    }
    let wire: RetainedWire =
        serde_json::from_str(&member.canonical_submission).map_err(|_| SubscriptionResetRefusal)?;
    let manifest = ExpectedArtifactManifest {
        artifact_bytes: wire.artifact_manifest.artifact_bytes,
        artifact_rows: wire.artifact_manifest.artifact_rows,
        kind: match wire.artifact_manifest.kind.as_str() {
            "empty" => ManifestKind::Empty,
            "load" => ManifestKind::Load,
            "package" => ManifestKind::Package,
            _ => return Err(SubscriptionResetRefusal),
        },
    };
    let submission = Submission {
        canonical_arguments: wire.canonical_arguments,
        daemon_subscription_identifier: wire.daemon_subscription_identifier,
        manifest,
        operation: wire.operation,
        provenance: wire.provenance,
        submitted_command_digest: wire.submitted_command_digest,
    };
    let contract = &submission.provenance.command_contract;
    let contracts = SubmissionContracts {
        argument_schema_digest: contract.argument_schema_digest.clone(),
        author_agent_transport_contract_digest: submission
            .provenance
            .transport_contract_digest
            .clone(),
        command_canonical_json_contract_digest: submission
            .provenance
            .canonical_json_contract_digest
            .clone(),
        command_contract_limits_digest: contract.command_contract_limits_digest.clone(),
        command_semantic_contract_version: contract.command_semantic_contract_version.clone(),
        command_wire_name: contract.command_wire_name.clone(),
        result_schema_digest: contract.result_schema_digest.clone(),
        submitted_command_digest: submission.submitted_command_digest.clone(),
    };
    if contracts != member.contracts
        || submission.wire_body().map_err(|_| SubscriptionResetRefusal)?
            != member.canonical_submission.as_bytes()
        || submission.operation.agent_operation_identifier
            != member.identity.agent_operation_identifier
        || submission.operation.agent_event_store_generation
            != member.identity.agent_event_store_generation
        || submission.operation.author_target_identity_digest
            != member.identity.author_target_identity_digest
        || submission.operation.selected_environment_revision
            != member.identity.selected_environment_revision
        || submission.daemon_subscription_identifier
            != member.identity.daemon_subscription_identifier
    {
        return Err(SubscriptionResetRefusal);
    }
    Ok(submission)
}

fn elapsed(start: std::time::Instant) -> Result<u64, SubscriptionResetRefusal> {
    u64::try_from(start.elapsed().as_nanos().div_ceil(1_000_000))
        .map_err(|_| SubscriptionResetRefusal)
}

/// Captures the exact durable subscription/generation, snapshots every active
/// member and commits only a complete covering batch. Cancellation drops staged
/// facts; no transaction spans a network await and no POST is ever issued.
pub async fn reset_active_subscription<'runtime>(
    ledger: &AgentSubscriptionLedger,
    operations: &'runtime OperationRepository,
    transport: &SelectedAuthorTransport,
    selection: &ExecutionIdentity,
    subscription: &str,
    authentication: &'runtime RequestAuthentication,
    protocol: ResetTransport,
    now_unix_milliseconds: u64,
) -> Result<SubscriptionResetOutcome<'runtime>, SubscriptionResetRefusal> {
    reset_active_subscription_with_authentication(
        ledger,
        operations,
        transport,
        selection,
        subscription,
        super::author_authentication::AuthorAuthentication::Fixed { authentication, protocol },
        now_unix_milliseconds,
    )
    .await
}

/// Captures and reconciles the complete subscription with one authentication
/// policy. Provider refresh never changes the durable membership/reset fences.
pub async fn reset_active_subscription_with_authentication<'runtime>(
    ledger: &AgentSubscriptionLedger,
    operations: &'runtime OperationRepository,
    transport: &SelectedAuthorTransport,
    selection: &ExecutionIdentity,
    subscription: &str,
    authentication: super::author_authentication::AuthorAuthentication<'runtime>,
    now_unix_milliseconds: u64,
) -> Result<SubscriptionResetOutcome<'runtime>, SubscriptionResetRefusal> {
    authentication.require_execution(selection).map_err(|_| SubscriptionResetRefusal)?;
    let started = std::time::Instant::now();
    if !ledger.database().shares_database_with(operations.database()) {
        return Err(SubscriptionResetRefusal);
    }
    transport.require_execution(selection).map_err(|_| SubscriptionResetRefusal)?;
    let view = ledger
        .read_recovery_view(&selection.author_target_identity_digest, subscription)
        .map_err(|_| SubscriptionResetRefusal)?;
    let generation = view.ledger().agent_event_store_generation;
    // All independent persisted input is checked before the first socket opens.
    let mut members = Vec::new();
    for member in view.members() {
        let submission = restore(member)?;
        let identity = ExecutionIdentity {
            attempt: selection.attempt,
            author_target_identity_digest: member.identity.author_target_identity_digest.clone(),
            selected_environment_revision: member.identity.selected_environment_revision.clone(),
            operation_identifier: member.identity.operation_identifier.clone(),
        };
        transport
            .require_submission(&identity, &submission)
            .map_err(|_| SubscriptionResetRefusal)?;
        let local =
            super::durable_author_lookup::retained_command(operations, &identity, &submission)
                .map_err(|_| SubscriptionResetRefusal)?;
        if local.record.lifecycle_state.is_terminal()
            || member.identity.agent_event_store_generation != generation
            || local
                .record
                .outstanding_recovery
                .as_ref()
                .is_some_and(super::durable_author_lookup::automatic_recovery_paused)
        {
            return Ok(SubscriptionResetOutcome::NeedsOperationRecovery {
                operation_identifier: identity.operation_identifier,
            });
        }
        members.push((identity, submission, local.record.revision));
    }
    let capture = authentication
        .high_water(transport, selection, subscription, generation)
        .await
        .map_err(|_| SubscriptionResetRefusal)?;
    let capture = match capture {
        HighWaterOutcome::Captured(capture) => capture,
        HighWaterOutcome::Reset(reset) => {
            if members.is_empty() {
                let cursor =
                    finish_generation_reset(ledger, operations, transport, selection, &reset)?;
                return Ok(SubscriptionResetOutcome::Installed {
                    generation: reset.generation(),
                    cursor,
                });
            }
            return Ok(SubscriptionResetOutcome::GenerationChanged(reset));
        }
        HighWaterOutcome::Response(_) => return Err(SubscriptionResetRefusal),
    };
    let mut staged = Vec::new();
    for (identity, submission, expected_revision) in &members {
        let local =
            super::durable_author_lookup::retained_command(operations, identity, submission)
                .map_err(|_| SubscriptionResetRefusal)?;
        if local.record.revision != *expected_revision
            || local.record.lifecycle_state.is_terminal()
            || local
                .record
                .outstanding_recovery
                .as_ref()
                .is_some_and(super::durable_author_lookup::automatic_recovery_paused)
        {
            return Ok(SubscriptionResetOutcome::NeedsOperationRecovery {
                operation_identifier: identity.operation_identifier.clone(),
            });
        }
        let lookup = authentication
            .lookup(transport, identity, submission)
            .await
            .map_err(|_| SubscriptionResetRefusal)?;
        let received = std::time::Instant::now();
        let handoff = |receipt| {
            SubscriptionResetOutcome::CapturedOperationRecovery(CapturedResetRecovery {
                authentication,
                operations,
                identity: identity.clone(),
                submission: submission.clone(),
                expected_revision: *expected_revision,
                receipt,
                received,
                reset_started: started,
                reset_started_at_unix_milliseconds: now_unix_milliseconds,
            })
        };
        let OperationLookupReceipt::Found(receipt) = lookup else {
            return Ok(handoff(lookup));
        };
        capture
            .require_snapshot_coverage(&receipt.snapshot)
            .map_err(|_| SubscriptionResetRefusal)?;
        if receipt.snapshot.described_state().is_terminal() {
            return Ok(handoff(OperationLookupReceipt::Found(receipt)));
        }
        staged.push((
            received,
            ActiveResetSnapshot {
                agent_operation_identifier: submission.operation.agent_operation_identifier.clone(),
                observation: slingshot_domain::remote_job::RemoteJobObservation {
                    state: receipt.snapshot.described_state(),
                    applied_sequence: receipt.snapshot.sequence,
                    attempt: receipt.snapshot.attempt,
                    progress: receipt.snapshot.progress,
                },
                physical_sling_job_identifiers: receipt.snapshot.physical_sling_job_identifiers,
                remaining_retention_milliseconds: receipt.remaining_retention_milliseconds,
                subscription_watermark: receipt
                    .snapshot
                    .subscription_watermark
                    .as_text()
                    .to_owned(),
            },
        ));
    }
    let now =
        now_unix_milliseconds.checked_add(elapsed(started)?).ok_or(SubscriptionResetRefusal)?;
    let snapshots = staged
        .into_iter()
        .map(|(received, mut snapshot)| {
            snapshot.remaining_retention_milliseconds = snapshot
                .remaining_retention_milliseconds
                .checked_sub(elapsed(received)?)
                .filter(|remaining| *remaining > 0)
                .ok_or(SubscriptionResetRefusal)?;
            Ok(snapshot)
        })
        .collect::<Result<Vec<_>, SubscriptionResetRefusal>>()?;
    if snapshots.is_empty() {
        ledger
            .install_empty_recovery(&view, capture.generation(), capture.cursor().as_text())
            .map_err(|_| SubscriptionResetRefusal)?;
    } else {
        ledger
            .install_active_snapshot_reset(
                &view,
                capture.generation(),
                capture.cursor().as_text(),
                &snapshots,
                now,
            )
            .map_err(|_| SubscriptionResetRefusal)?;
    }
    Ok(SubscriptionResetOutcome::Installed {
        generation: capture.generation(),
        cursor: capture.cursor().clone(),
    })
}
