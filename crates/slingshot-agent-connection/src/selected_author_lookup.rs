//! Bounded, identity-checked snapshot lookup under the selected author.

use crate::authentication::environment_provider::RequestAuthentication;
use crate::command_submission::Submission;
use crate::job_snapshot_reconciliation::{
    JobSnapshot, LookupAnswer, SnapshotEcho, SnapshotExpectation, decode_snapshot,
};
use crate::selected_author_transport::SelectedAuthorTransport;
use slingshot_agent_protocol::wire_contract::ExpectedProvenance;
use slingshot_domain::operation_executor::ExecutionIdentity;

/// A lookup refusal never proves missing work or authorizes another POST.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[error("the author lookup did not provide a validated snapshot")]
pub struct SnapshotLookupRefusal;

/// A validated snapshot and its conservative request-start retention budget.
/// The wire grant is retained unchanged inside the snapshot.
pub struct SnapshotLookupReceipt {
    /// Fully echoed snapshot, still requiring durable reconciliation.
    pub snapshot: JobSnapshot,
    /// Budget after DNS, connection, request, and response time.
    pub remaining_retention_milliseconds: u64,
}

/// Validated lookup branches. Missing still requires durable grace and fence
/// checks; this response alone never grants permission to send.
pub enum OperationLookupReceipt {
    /// Active snapshot and its retention budget.
    Found(SnapshotLookupReceipt),
    /// Complete identity-checked tombstone or same-generation absence.
    Absent(LookupAnswer),
}

/// One physical query's complete, independently validated result.
#[derive(Debug)]
pub enum PhysicalLookupReceipt {
    /// Snapshot bound to the retained operation and exact queried physical job.
    Found(SnapshotLookupReceipt),
    /// Absence of this physical identifier in the independently observed current
    /// generation, not proof that every physical job for an operation is gone.
    Missing(crate::physical_job_missing::ValidatedPhysicalJobMissing),
}

enum DecodedLookup {
    Found(SnapshotLookupReceipt),
    LogicalAbsent(LookupAnswer),
    PhysicalMissing(crate::physical_job_missing::ValidatedPhysicalJobMissing),
}
impl DecodedLookup {
    fn logical(self) -> Result<OperationLookupReceipt, SnapshotLookupRefusal> {
        match self {
            Self::Found(receipt) => Ok(OperationLookupReceipt::Found(receipt)),
            Self::LogicalAbsent(answer) => Ok(OperationLookupReceipt::Absent(answer)),
            Self::PhysicalMissing(_) => Err(SnapshotLookupRefusal),
        }
    }
    fn physical(self) -> Result<PhysicalLookupReceipt, SnapshotLookupRefusal> {
        match self {
            Self::Found(receipt) => Ok(PhysicalLookupReceipt::Found(receipt)),
            Self::PhysicalMissing(proof) => Ok(PhysicalLookupReceipt::Missing(proof)),
            Self::LogicalAbsent(_) => Err(SnapshotLookupRefusal),
        }
    }
}

impl core::fmt::Debug for OperationLookupReceipt {
    fn fmt(&self, formatter: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        formatter.write_str("OperationLookupReceipt([redacted])")
    }
}

/// Validates a bounded absence document against the exact lookup context.
pub fn decode_lookup_absence(
    status: u16,
    body: &[u8],
    expected: &SnapshotExpectation,
) -> Result<LookupAnswer, SnapshotLookupRefusal> {
    use slingshot_agent_protocol::lookup_absence::LookupAbsence;
    use slingshot_domain::author_agent_transport_contract::AuthorAgentTransportContract;
    if body.len() as u64
        > AuthorAgentTransportContract::embedded().limit("maximum_agent_protocol_document_bytes")
    {
        return Err(SnapshotLookupRefusal);
    }
    let document: LookupAbsence =
        serde_json::from_slice(body).map_err(|_| SnapshotLookupRefusal)?;
    match document {
        LookupAbsence::Missing {
            format,
            transport_contract_digest,
            agent_event_store_generation,
            agent_operation_identifier,
            author_target_identity_digest,
        } if status == 404 => {
            if format != slingshot_agent_protocol::identity::AGENT_FORMAT
                || transport_contract_digest
                    != expected.expected_provenance.transport_contract_digest
                || agent_event_store_generation != expected.agent_event_store_generation
                || agent_operation_identifier != expected.agent_operation_identifier
                || author_target_identity_digest != expected.author_target_identity_digest
            {
                return Err(SnapshotLookupRefusal);
            }
            Ok(LookupAnswer::Missing)
        }
        LookupAbsence::Retired {
            provenance,
            agent_event_store_generation,
            agent_operation_identifier,
            author_target_identity_digest,
            daemon_subscription_identifier,
            selected_environment_revision,
            submitted_command_digest,
        } if status == 410 => {
            let echo = SnapshotEcho {
                provenance,
                agent_event_store_generation,
                agent_operation_identifier,
                author_target_identity_digest,
                daemon_subscription_identifier,
                selected_environment_revision,
                submitted_command_digest,
            };
            expected.require_echoed(&echo).map_err(|_| SnapshotLookupRefusal)?;
            Ok(LookupAnswer::Retired(Box::new(echo)))
        }
        _ => Err(SnapshotLookupRefusal),
    }
}

impl core::fmt::Debug for SnapshotLookupReceipt {
    fn fmt(&self, formatter: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        formatter.write_str("SnapshotLookupReceipt([redacted])")
    }
}

impl SelectedAuthorTransport {
    /// Compatibility entry point for callers that require an active snapshot.
    pub async fn lookup_snapshot(
        &self,
        identity: &ExecutionIdentity,
        submission: &Submission,
        authentication: &RequestAuthentication,
    ) -> Result<SnapshotLookupReceipt, SnapshotLookupRefusal> {
        match self.lookup_operation(identity, submission, authentication).await? {
            OperationLookupReceipt::Found(receipt) => Ok(receipt),
            OperationLookupReceipt::Absent(_) => Err(SnapshotLookupRefusal),
        }
    }

    /// Looks up active, missing, or retired state without issuing another POST.
    pub async fn lookup_operation(
        &self,
        identity: &ExecutionIdentity,
        submission: &Submission,
        authentication: &RequestAuthentication,
    ) -> Result<OperationLookupReceipt, SnapshotLookupRefusal> {
        self.lookup_operation_over(identity, submission, authentication, Some(false), None, None)
            .await?
            .logical()
    }

    /// Uses strict HTTP/2 for the same fixed lookup, without protocol fallback.
    pub async fn lookup_operation_http2(
        &self,
        identity: &ExecutionIdentity,
        submission: &Submission,
        authentication: &RequestAuthentication,
    ) -> Result<OperationLookupReceipt, SnapshotLookupRefusal> {
        self.lookup_operation_over(identity, submission, authentication, Some(true), None, None)
            .await?
            .logical()
    }

    /// Retrieves a persisted physical Sling job through the fixed snapshot route.
    /// A refused response is not proof of absence or permission to resubmit.
    pub async fn lookup_physical_snapshot(
        &self,
        identity: &ExecutionIdentity,
        submission: &Submission,
        sling_job_identifier: &str,
        authentication: &RequestAuthentication,
    ) -> Result<SnapshotLookupReceipt, SnapshotLookupRefusal> {
        match self
            .lookup_operation_over(
                identity,
                submission,
                authentication,
                Some(false),
                Some(sling_job_identifier),
                None,
            )
            .await?
        {
            DecodedLookup::Found(receipt) => Ok(receipt),
            _ => Err(SnapshotLookupRefusal),
        }
    }

    /// Uses strict HTTP/2 for the same physical snapshot with no fallback.
    pub async fn lookup_physical_snapshot_http2(
        &self,
        identity: &ExecutionIdentity,
        submission: &Submission,
        sling_job_identifier: &str,
        authentication: &RequestAuthentication,
    ) -> Result<SnapshotLookupReceipt, SnapshotLookupRefusal> {
        match self
            .lookup_operation_over(
                identity,
                submission,
                authentication,
                Some(true),
                Some(sling_job_identifier),
                None,
            )
            .await?
        {
            DecodedLookup::Found(receipt) => Ok(receipt),
            _ => Err(SnapshotLookupRefusal),
        }
    }

    /// Physical lookup with closed absence validation against a separately
    /// observed current generation; never reissues a failed query or a POST.
    pub async fn lookup_physical_job(
        &self,
        identity: &ExecutionIdentity,
        submission: &Submission,
        sling_job_identifier: &str,
        current_generation: u64,
        authentication: &RequestAuthentication,
    ) -> Result<PhysicalLookupReceipt, SnapshotLookupRefusal> {
        self.lookup_operation_over(
            identity,
            submission,
            authentication,
            Some(false),
            Some(sling_job_identifier),
            Some(current_generation),
        )
        .await?
        .physical()
    }

    /// Strict HTTP/2 counterpart of the physical lookup, with no fallback.
    pub async fn lookup_physical_job_http2(
        &self,
        identity: &ExecutionIdentity,
        submission: &Submission,
        sling_job_identifier: &str,
        current_generation: u64,
        authentication: &RequestAuthentication,
    ) -> Result<PhysicalLookupReceipt, SnapshotLookupRefusal> {
        self.lookup_operation_over(
            identity,
            submission,
            authentication,
            Some(true),
            Some(sling_job_identifier),
            Some(current_generation),
        )
        .await?
        .physical()
    }

    /// Looks up the same logical operation on its negotiated connection.
    /// Snapshot and absence evidence use the shared identity gates; no POST
    /// or fallback request is issued on refusal.
    pub async fn lookup_operation_negotiated(
        &self,
        identity: &ExecutionIdentity,
        submission: &Submission,
        authentication: &RequestAuthentication,
    ) -> Result<OperationLookupReceipt, SnapshotLookupRefusal> {
        self.lookup_operation_over(identity, submission, authentication, None, None, None)
            .await?.logical()
    }

    /// Retrieves a physical snapshot using the same negotiated socket.
    /// Missing or retired responses are not returned as snapshot evidence.
    pub async fn lookup_physical_snapshot_negotiated(
        &self,
        identity: &ExecutionIdentity,
        submission: &Submission,
        sling_job_identifier: &str,
        authentication: &RequestAuthentication,
    ) -> Result<SnapshotLookupReceipt, SnapshotLookupRefusal> {
        match self.lookup_operation_over(identity, submission, authentication, None,
            Some(sling_job_identifier), None).await? {
            DecodedLookup::Found(receipt) => Ok(receipt),
            _ => Err(SnapshotLookupRefusal),
        }
    }

    /// Negotiated physical lookup with absence bound to the separately observed
    /// current generation. Negotiation does not change the retained generation.
    pub async fn lookup_physical_job_negotiated(
        &self,
        identity: &ExecutionIdentity,
        submission: &Submission,
        sling_job_identifier: &str,
        current_generation: u64,
        authentication: &RequestAuthentication,
    ) -> Result<PhysicalLookupReceipt, SnapshotLookupRefusal> {
        self.lookup_operation_over(identity, submission, authentication, None,
            Some(sling_job_identifier), Some(current_generation)).await?.physical()
    }

    async fn lookup_operation_over(
        &self,
        identity: &ExecutionIdentity,
        submission: &Submission,
        authentication: &RequestAuthentication,
        http2: Option<bool>,
        physical: Option<&str>,
        current_generation: Option<u64>,
    ) -> Result<DecodedLookup, SnapshotLookupRefusal> {
        let (segments, query) = self.lookup_request(identity, submission, physical, current_generation)?;
        let started = std::time::Instant::now();
        let receipt = if http2.is_none() {
            self.finite_negotiated_query(
                http::Method::GET, &segments, &query, authentication, &http::HeaderMap::new(), b"",
            ).await
        } else if http2 == Some(true) {
            self.finite_http2_query(
                http::Method::GET, &segments, &query, authentication, &http::HeaderMap::new(), b"",
            ).await
        } else {
            self.finite_http1_query(
                http::Method::GET, &segments, &query, authentication, &http::HeaderMap::new(), b"",
            ).await
        }.map_err(|_| SnapshotLookupRefusal)?;
        Self::decode_lookup_receipt(submission, physical, current_generation, started, receipt)
    }

    /// Reads logical state with provider-owned authentication and at most one
    /// refreshed Cloud GET. Neither refusal nor absence authorizes a POST.
    pub async fn lookup_operation_authenticated(
        &self,
        identity: &ExecutionIdentity,
        submission: &Submission,
        provider: &crate::authentication::environment_provider::EnvironmentAuthenticationProvider,
        source: &dyn crate::authentication::access_token_cache::AccessTokenSource,
        reading: u64,
    ) -> Result<OperationLookupReceipt, SnapshotLookupRefusal> {
        self.lookup_authenticated(identity, submission, provider, source, reading, None, None)
            .await?.logical()
    }

    /// Reads one retained physical job with provider-owned authentication.
    /// Absence must echo the independently observed current generation and the
    /// exact queried identifier, including after credential refresh.
    pub async fn lookup_physical_job_authenticated(
        &self,
        identity: &ExecutionIdentity,
        submission: &Submission,
        sling_job_identifier: &str,
        current_generation: u64,
        provider: &crate::authentication::environment_provider::EnvironmentAuthenticationProvider,
        source: &dyn crate::authentication::access_token_cache::AccessTokenSource,
        reading: u64,
    ) -> Result<PhysicalLookupReceipt, SnapshotLookupRefusal> {
        self.lookup_authenticated(identity, submission, provider, source, reading,
            Some(sling_job_identifier), Some(current_generation)).await?.physical()
    }

    /// Reads a physical snapshot without accepting absence as snapshot evidence.
    pub async fn lookup_physical_snapshot_authenticated(
        &self,
        identity: &ExecutionIdentity,
        submission: &Submission,
        sling_job_identifier: &str,
        provider: &crate::authentication::environment_provider::EnvironmentAuthenticationProvider,
        source: &dyn crate::authentication::access_token_cache::AccessTokenSource,
        reading: u64,
    ) -> Result<SnapshotLookupReceipt, SnapshotLookupRefusal> {
        match self.lookup_authenticated(identity, submission, provider, source, reading,
            Some(sling_job_identifier), None).await? {
            DecodedLookup::Found(receipt) => Ok(receipt),
            _ => Err(SnapshotLookupRefusal),
        }
    }

    async fn lookup_authenticated(
        &self,
        identity: &ExecutionIdentity,
        submission: &Submission,
        provider: &crate::authentication::environment_provider::EnvironmentAuthenticationProvider,
        source: &dyn crate::authentication::access_token_cache::AccessTokenSource,
        reading: u64,
        physical: Option<&str>,
        current_generation: Option<u64>,
    ) -> Result<DecodedLookup, SnapshotLookupRefusal> {
        let (segments, query) = self.lookup_request(identity, submission, physical, current_generation)?;
        let started = std::time::Instant::now();
        let receipt = self.authenticated_finite_get(
            provider, source, reading, &segments, &query, &http::HeaderMap::new(),
        ).await.map_err(|_| SnapshotLookupRefusal)?;
        Self::decode_lookup_receipt(submission, physical, current_generation, started, receipt)
    }

    /// Reads logical state using the selected asynchronous credential provider.
    pub async fn lookup_operation_authenticated_async<Clock, Utc>(
        &self,
        identity: &ExecutionIdentity,
        submission: &Submission,
        provider: &crate::authentication::environment_provider::AsyncEnvironmentAuthenticationProvider,
        clock: &Clock,
        utc: &Utc,
    ) -> Result<OperationLookupReceipt, SnapshotLookupRefusal>
    where
        Clock: crate::authentication::identity_management_exchange::MonotonicClock + Sync,
        Utc: crate::authentication::token_assertion::CoordinatedUniversalTimeClock + Sync,
    {
        self.lookup_authenticated_async(identity, submission, provider, clock, utc, None, None)
            .await?.logical()
    }

    /// Reads physical state with the same generation-bound absence validation.
    pub async fn lookup_physical_job_authenticated_async<Clock, Utc>(
        &self,
        identity: &ExecutionIdentity,
        submission: &Submission,
        sling_job_identifier: &str,
        current_generation: u64,
        provider: &crate::authentication::environment_provider::AsyncEnvironmentAuthenticationProvider,
        clock: &Clock,
        utc: &Utc,
    ) -> Result<PhysicalLookupReceipt, SnapshotLookupRefusal>
    where
        Clock: crate::authentication::identity_management_exchange::MonotonicClock + Sync,
        Utc: crate::authentication::token_assertion::CoordinatedUniversalTimeClock + Sync,
    {
        self.lookup_authenticated_async(identity, submission, provider, clock, utc,
            Some(sling_job_identifier), Some(current_generation)).await?.physical()
    }

    /// Reads a physical snapshot; absence is never accepted as snapshot evidence.
    pub async fn lookup_physical_snapshot_authenticated_async<Clock, Utc>(
        &self,
        identity: &ExecutionIdentity,
        submission: &Submission,
        sling_job_identifier: &str,
        provider: &crate::authentication::environment_provider::AsyncEnvironmentAuthenticationProvider,
        clock: &Clock,
        utc: &Utc,
    ) -> Result<SnapshotLookupReceipt, SnapshotLookupRefusal>
    where
        Clock: crate::authentication::identity_management_exchange::MonotonicClock + Sync,
        Utc: crate::authentication::token_assertion::CoordinatedUniversalTimeClock + Sync,
    {
        match self.lookup_authenticated_async(identity, submission, provider, clock, utc,
            Some(sling_job_identifier), None).await? {
            DecodedLookup::Found(receipt) => Ok(receipt),
            _ => Err(SnapshotLookupRefusal),
        }
    }

    async fn lookup_authenticated_async<Clock, Utc>(
        &self,
        identity: &ExecutionIdentity,
        submission: &Submission,
        provider: &crate::authentication::environment_provider::AsyncEnvironmentAuthenticationProvider,
        clock: &Clock,
        utc: &Utc,
        physical: Option<&str>,
        current_generation: Option<u64>,
    ) -> Result<DecodedLookup, SnapshotLookupRefusal>
    where
        Clock: crate::authentication::identity_management_exchange::MonotonicClock + Sync,
        Utc: crate::authentication::token_assertion::CoordinatedUniversalTimeClock + Sync,
    {
        let (segments, query) = self.lookup_request(identity, submission, physical, current_generation)?;
        let started = std::time::Instant::now();
        let receipt = self.authenticated_finite_get_async(
            provider, clock, utc, &segments, &query, &http::HeaderMap::new(),
        ).await.map_err(|_| SnapshotLookupRefusal)?;
        Self::decode_lookup_receipt(submission, physical, current_generation, started, receipt)
    }

    fn lookup_request<'a>(
        &self,
        identity: &ExecutionIdentity,
        submission: &'a Submission,
        physical: Option<&'a str>,
        current_generation: Option<u64>,
    ) -> Result<([&'static str; 4], [(&'static str, &'a str); 1]), SnapshotLookupRefusal> {
        self.require_submission(identity, submission).map_err(|_| SnapshotLookupRefusal)?;
        if current_generation == Some(0) {
            return Err(SnapshotLookupRefusal);
        }
        let (segments, query) = if let Some(identifier) = physical {
            slingshot_domain::remote_job::AgentJobIdentifier::new(identifier)
                .map_err(|_| SnapshotLookupRefusal)?;
            (["bin", "slingshot-agent", "jobs", "snapshot"], [("sling_job_identifier", identifier)])
        } else {
            (
                ["bin", "slingshot-agent", "operations", "lookup"],
                [(
                    "agent_operation_identifier",
                    submission.operation.agent_operation_identifier.as_str(),
                )],
            )
        };
        Ok((segments, query))
    }

    fn decode_lookup_receipt(
        submission: &Submission,
        physical: Option<&str>,
        current_generation: Option<u64>,
        started: std::time::Instant,
        receipt: crate::selected_author_http::FiniteHttpReceipt,
    ) -> Result<DecodedLookup, SnapshotLookupRefusal> {
        if ![200, 404, 410].contains(&receipt.response.status)
            || receipt.response.head.location.is_some()
            || !crate::selected_author_submission::json_media_type(
                receipt.response.content_type.as_deref().unwrap_or(""),
            )
        {
            return Err(SnapshotLookupRefusal);
        }
        let expectation = SnapshotExpectation {
            agent_event_store_generation: submission.operation.agent_event_store_generation,
            agent_operation_identifier: submission.operation.agent_operation_identifier.clone(),
            author_target_identity_digest: submission
                .operation
                .author_target_identity_digest
                .clone(),
            daemon_subscription_identifier: submission.daemon_subscription_identifier.clone(),
            expected_provenance: ExpectedProvenance {
                canonical_json_contract_digest: submission
                    .provenance
                    .canonical_json_contract_digest
                    .clone(),
                command_contract: (&submission.provenance.command_contract).into(),
                transport_contract_digest: submission.provenance.transport_contract_digest.clone(),
            },
            selected_environment_revision: submission
                .operation
                .selected_environment_revision
                .clone(),
            submitted_command_digest: submission.submitted_command_digest.clone(),
        };
        if receipt.response.status != 200 {
            // Logical absence documents do not echo the queried physical job.
            // Never count a bare status or logical tombstone as physical loss.
            if let Some(identifier) = physical {
                let generation = current_generation.ok_or(SnapshotLookupRefusal)?;
                return crate::physical_job_missing::decode_physical_job_missing(
                    &receipt.response,
                    identifier,
                    generation,
                )
                .map(DecodedLookup::PhysicalMissing)
                .map_err(|_| SnapshotLookupRefusal);
            }
            return decode_lookup_absence(
                receipt.response.status,
                &receipt.response.body,
                &expectation,
            )
            .map(DecodedLookup::LogicalAbsent);
        }
        let snapshot = decode_snapshot(&receipt.response.body, &expectation)
            .map_err(|_| SnapshotLookupRefusal)?;
        if physical.is_some_and(|identifier| {
            !snapshot.physical_sling_job_identifiers.iter().any(|held| held == identifier)
        }) {
            return Err(SnapshotLookupRefusal);
        }
        let remaining_retention_milliseconds = snapshot
            .granted_retention_milliseconds
            .checked_sub(
                receipt.elapsed_milliseconds.max(
                    u64::try_from(started.elapsed().as_nanos().div_ceil(1_000_000))
                        .map_err(|_| SnapshotLookupRefusal)?,
                ),
            )
            .filter(|remaining| *remaining > 0)
            .ok_or(SnapshotLookupRefusal)?;
        Ok(DecodedLookup::Found(SnapshotLookupReceipt {
            snapshot,
            remaining_retention_milliseconds,
        }))
    }
}
