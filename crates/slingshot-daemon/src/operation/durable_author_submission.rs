//! Durable first-send ownership for the product author connection.
//!
//! Inserting the exact remote child is the first-send claim. A restart or a
//! second worker that finds a child must reconcile it, even if the first
//! process died before reaching the socket. Neither time nor a higher local
//! attempt number proves that the POST was never sent.

use super::author_authentication::AuthorAuthentication;
use super::subscription_reset::ResetTransport;
use slingshot_agent_connection::authentication::environment_provider::RequestAuthentication;
use slingshot_agent_connection::command_submission::{Submission, SubmissionOutcome, UnknownCause};
use slingshot_agent_connection::selected_author_submission::{
    SubmissionSendRefusal, require_submission_derivation,
};
use slingshot_agent_connection::selected_author_transport::SelectedAuthorTransport;
use slingshot_domain::operation_executor::ExecutionIdentity;
use slingshot_domain::remote_job::{JobEventSequence, RemoteJobObservation};
use slingshot_storage::agent_job_repository::{
    AgentJobRepository, AgentSubmission, SubmissionContracts, SubmissionIdentity,
    SubmissionOutcome as AdmissionOutcome,
};

/// Local failure before a new author request can be issued.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum DurableSubmissionRefusal {
    /// The local record cannot authorize this execution or these bytes.
    #[error("the retained submission conflicts with this execution")]
    Conflict,
    /// Storage could not retain or read the remote child.
    #[error("the remote submission could not be persisted")]
    Storage,
    /// The selected connection or request preflight refused.
    #[error("the selected-author submission preflight refused")]
    Preflight,
}

/// A first-send claim. Its private fields prevent callers from fabricating a
/// permission; consuming it prevents one caller from invoking it twice.
pub struct InitialSubmissionPermit<'repository> {
    repository: &'repository AgentJobRepository,
    submission: Submission,
    identity: ExecutionIdentity,
    retained: AgentSubmission,
}

impl core::fmt::Debug for InitialSubmissionPermit<'_> {
    fn fmt(&self, formatter: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        formatter.write_str("InitialSubmissionPermit([redacted])")
    }
}

impl InitialSubmissionPermit<'_> {
    /// Consumes this newly admitted child's one initial network attempt.
    /// Accepted/duplicate outcomes are returned only after durable association;
    /// they are still not terminal operation results.
    pub async fn send(
        self,
        operations: &slingshot_storage::operation_repository::OperationRepository,
        expected_operation_revision: u64,
        transport: &SelectedAuthorTransport,
        authentication: &RequestAuthentication,
        now_unix_milliseconds: u64,
    ) -> Result<SubmissionOutcome, SubmissionSendRefusal> {
        self.send_over(
            operations,
            expected_operation_revision,
            transport,
            authentication,
            now_unix_milliseconds,
            ResetTransport::Http1,
        )
        .await
    }

    /// Consumes the first-send permit using one mode for discovery, token and
    /// POST. Mode selection does not authorize retry or protocol fallback.
    pub async fn send_over(
        self,
        operations: &slingshot_storage::operation_repository::OperationRepository,
        expected_operation_revision: u64,
        transport: &SelectedAuthorTransport,
        authentication: &RequestAuthentication,
        now_unix_milliseconds: u64,
        protocol: ResetTransport,
    ) -> Result<SubmissionOutcome, SubmissionSendRefusal> {
        self.send_with_authentication(
            operations,
            expected_operation_revision,
            transport,
            AuthorAuthentication::Fixed { authentication, protocol },
            now_unix_milliseconds,
        )
        .await
    }

    /// Consumes the durable first-send claim using request-scoped authentication.
    /// Provider refresh does not grant another permit or bypass the final guard.
    pub async fn send_with_authentication(
        self,
        operations: &slingshot_storage::operation_repository::OperationRepository,
        expected_operation_revision: u64,
        transport: &SelectedAuthorTransport,
        authentication: AuthorAuthentication<'_>,
        now_unix_milliseconds: u64,
    ) -> Result<SubmissionOutcome, SubmissionSendRefusal> {
        let started = std::time::Instant::now();
        authentication
            .require_execution(&self.identity)
            .map_err(|_| SubmissionSendRefusal::Identity)?;
        let require_local = || {
            let local = super::durable_author_lookup::retained_command(
                operations,
                &self.identity,
                &self.submission,
            )
            .map_err(|_| SubmissionSendRefusal::Identity)?;
            if local.record.revision != expected_operation_revision
                || local.record.lifecycle_state.is_terminal()
                || local.selected_environment_revision
                    != self.identity.selected_environment_revision
            {
                return Err(SubmissionSendRefusal::Identity);
            }
            Ok(())
        };
        require_local()?;
        transport.require_submission(&self.identity, &self.submission)?;
        authentication
            .discover(transport, &self.identity, &self.submission)
            .await
            .map_err(|_| SubmissionSendRefusal::Request)?;
        let mut outcome = authentication
            .submit(
                transport,
                &self.identity,
                &self.submission,
                now_unix_milliseconds,
                require_local,
            )
            .await?;
        if let SubmissionOutcome::Accepted {
            physical_sling_job_identifiers,
            remaining_retention_milliseconds,
        }
        | SubmissionOutcome::Duplicate {
            physical_sling_job_identifiers,
            remaining_retention_milliseconds,
        } = &mut outcome
        {
            let elapsed = u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX);
            match self.repository.acknowledge(
                &self.retained,
                physical_sling_job_identifiers,
                *remaining_retention_milliseconds,
                now_unix_milliseconds.saturating_add(elapsed),
            ) {
                Ok(0) => {
                    return Ok(SubmissionOutcome::SubmissionUnknown {
                        cause: UnknownCause::Retention,
                    });
                }
                Ok(retained) => *remaining_retention_milliseconds = retained,
                Err(_) => {
                    return Ok(SubmissionOutcome::SubmissionUnknown {
                        cause: UnknownCause::DurableAssociation,
                    });
                }
            }
        }
        Ok(outcome)
    }
}

/// Persists a new submission before credentials or network work. `None` means
/// an identical child already exists and must go to lookup, never initial POST.
/// Mutable observation/timing fields do not turn an exact replay into drift.
pub fn prepare_initial_submission<'repository>(
    repository: &'repository AgentJobRepository,
    identity: &ExecutionIdentity,
    submission: &Submission,
    now_unix_milliseconds: u64,
) -> Result<Option<InitialSubmissionPermit<'repository>>, DurableSubmissionRefusal> {
    require_submission_derivation(identity, submission)
        .map_err(|_| DurableSubmissionRefusal::Preflight)?;
    let wire =
        String::from_utf8(submission.wire_body().map_err(|_| DurableSubmissionRefusal::Preflight)?)
            .map_err(|_| DurableSubmissionRefusal::Preflight)?;
    let contract = &submission.provenance.command_contract;
    let retained = AgentSubmission {
        canonical_submission: wire,
        contracts: SubmissionContracts {
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
        },
        identity: SubmissionIdentity {
            agent_event_store_generation: submission.operation.agent_event_store_generation,
            agent_operation_identifier: submission.operation.agent_operation_identifier.clone(),
            author_target_identity_digest: identity.author_target_identity_digest.clone(),
            daemon_subscription_identifier: submission.daemon_subscription_identifier.clone(),
            operation_identifier: identity.operation_identifier.clone(),
            selected_environment_revision: identity.selected_environment_revision.clone(),
        },
        // Queued is bookkeeping here, not evidence of remote acceptance.
        // No physical association or retained remote lifetime exists yet.
        observation: RemoteJobObservation::accepted(),
        recorded_at_unix_milliseconds: now_unix_milliseconds,
        remaining_retention_milliseconds: 0,
        request_start_unix_milliseconds: now_unix_milliseconds,
        snapshot_watermark: JobEventSequence::of(0),
        terminal_disposition: None,
    };
    let read = || {
        repository
            .read(
                &identity.author_target_identity_digest,
                &submission.operation.agent_operation_identifier,
            )
            .map_err(|_| DurableSubmissionRefusal::Storage)
    };
    let same = |stored: &AgentSubmission| {
        stored.identity == retained.identity
            && stored.contracts == retained.contracts
            && stored.canonical_submission == retained.canonical_submission
    };
    if let Some(stored) = read()? {
        return if same(&stored) { Ok(None) } else { Err(DurableSubmissionRefusal::Conflict) };
    }
    match repository.submit(&retained) {
        Ok(AdmissionOutcome::Admitted) => Ok(Some(InitialSubmissionPermit {
            repository,
            identity: identity.clone(),
            submission: submission.clone(),
            retained,
        })),
        Ok(AdmissionOutcome::ExactReplay) => Ok(None),
        Err(_) => match read()? {
            Some(stored) if same(&stored) => Ok(None),
            Some(_) => Err(DurableSubmissionRefusal::Conflict),
            None => Err(DurableSubmissionRefusal::Storage),
        },
    }
}

/// Runs the initial durable handoff through the selected transport. Existing
/// children return uncertainty so the protocol advances to lookup instead.
/// Capability disagreement precedes new child admission; a pending child can
/// still survive a crash between admission and its separate send-time check.
pub async fn submit_initial(
    repository: &AgentJobRepository,
    operations: &slingshot_storage::operation_repository::OperationRepository,
    expected_operation_revision: u64,
    transport: &SelectedAuthorTransport,
    identity: &ExecutionIdentity,
    submission: &Submission,
    authentication: &RequestAuthentication,
    now_unix_milliseconds: u64,
) -> Result<SubmissionOutcome, DurableSubmissionRefusal> {
    submit_initial_over(
        repository,
        operations,
        expected_operation_revision,
        transport,
        identity,
        submission,
        authentication,
        now_unix_milliseconds,
        ResetTransport::Http1,
    )
    .await
}

/// Runs durable initial admission and its consumed permit over one HTTP mode.
/// Existing children remain lookup-only, irrespective of the chosen mode.
pub async fn submit_initial_over(
    repository: &AgentJobRepository,
    operations: &slingshot_storage::operation_repository::OperationRepository,
    expected_operation_revision: u64,
    transport: &SelectedAuthorTransport,
    identity: &ExecutionIdentity,
    submission: &Submission,
    authentication: &RequestAuthentication,
    now_unix_milliseconds: u64,
    protocol: ResetTransport,
) -> Result<SubmissionOutcome, DurableSubmissionRefusal> {
    submit_initial_with_authentication(
        repository,
        operations,
        expected_operation_revision,
        transport,
        identity,
        submission,
        AuthorAuthentication::Fixed { authentication, protocol },
        now_unix_milliseconds,
    )
    .await
}

/// Admits and sends using one authentication policy, retaining every durable
/// preflight and restart fence. Finding an existing child performs no token
/// exchange or author request and returns lookup-required uncertainty.
pub async fn submit_initial_with_authentication(
    repository: &AgentJobRepository,
    operations: &slingshot_storage::operation_repository::OperationRepository,
    expected_operation_revision: u64,
    transport: &SelectedAuthorTransport,
    identity: &ExecutionIdentity,
    submission: &Submission,
    authentication: AuthorAuthentication<'_>,
    now_unix_milliseconds: u64,
) -> Result<SubmissionOutcome, DurableSubmissionRefusal> {
    authentication.require_execution(identity).map_err(|_| DurableSubmissionRefusal::Preflight)?;
    transport
        .require_submission(identity, submission)
        .map_err(|_| DurableSubmissionRefusal::Preflight)?;
    let require_local = || {
        let local =
            super::durable_author_lookup::retained_command(operations, identity, submission)
                .map_err(|_| DurableSubmissionRefusal::Preflight)?;
        if local.record.revision != expected_operation_revision
            || local.record.lifecycle_state.is_terminal()
            || local.selected_environment_revision != identity.selected_environment_revision
        {
            return Err(DurableSubmissionRefusal::Preflight);
        }
        Ok(())
    };
    require_local()?;
    let started = std::time::Instant::now();
    let existing = repository
        .read(
            &identity.author_target_identity_digest,
            &submission.operation.agent_operation_identifier,
        )
        .map_err(|_| DurableSubmissionRefusal::Storage)?;
    if existing.is_none() {
        authentication
            .discover(transport, identity, submission)
            .await
            .map_err(|_| DurableSubmissionRefusal::Preflight)?;
    }
    let elapsed = u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX);
    require_local()?;
    let now_unix_milliseconds =
        now_unix_milliseconds.checked_add(elapsed).ok_or(DurableSubmissionRefusal::Preflight)?;
    match prepare_initial_submission(repository, identity, submission, now_unix_milliseconds)? {
        Some(permit) => permit
            .send_with_authentication(
                operations,
                expected_operation_revision,
                transport,
                authentication,
                now_unix_milliseconds,
            )
            .await
            .map_err(|_| DurableSubmissionRefusal::Preflight),
        None => Ok(SubmissionOutcome::SubmissionUnknown { cause: UnknownCause::LookupRequired }),
    }
}
