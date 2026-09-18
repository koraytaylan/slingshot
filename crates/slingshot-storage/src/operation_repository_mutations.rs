//! Transactional operation transitions and terminal publication.

use super::*;

impl OperationRepository {
    /// Folds one fact into an operation, under compare-and-set.
    ///
    /// A stale revision writes nothing and says so, which is how two writers
    /// racing on one operation produce a winner. A fold that changes nothing
    /// commits nothing.
    ///
    /// # Errors
    ///
    /// Returns [`RepositoryFailure`] naming the first rule the write breaks: a
    /// missing row, a stale revision, a bounded text over its bound, or a fact
    /// the domain refuses.
    pub fn apply(
        &self,
        author_target_identity_digest: &str,
        operation_identifier: &str,
        expected_revision: u64,
        fact: &OperationFact,
        now_unix_milliseconds: u64,
    ) -> Result<OperationSummary, RepositoryFailure> {
        Self::require_bounded(fact)?;
        self.mutate(
            author_target_identity_digest,
            operation_identifier,
            expected_revision,
            None,
            |stored| {
                let folded = stored.record.fold(fact)?;
                let settled = Self::settlement(stored, &folded, now_unix_milliseconds);
                Ok((folded.revision != stored.record.revision)
                    .then(|| (stored.clone(), folded, settled)))
            },
            None,
        )
    }

    /// Applies a local execution fact only while the claimed scheduler fence
    /// has crossed its no-return checkpoint.
    ///
    /// # Errors
    /// Returns the errors of [`Self::apply`], and refuses a missing executor
    /// checkpoint or a scheduler fence that no longer matches.
    pub fn apply_with_scheduler_fence(
        &self,
        author_target_identity_digest: &str,
        operation_identifier: &str,
        expected_revision: u64,
        fact: &OperationFact,
        now_unix_milliseconds: u64,
        scheduler_fence: u64,
    ) -> Result<OperationSummary, RepositoryFailure> {
        Self::require_bounded(fact)?;
        self.mutate(
            author_target_identity_digest,
            operation_identifier,
            expected_revision,
            None,
            |stored| {
                let folded = stored.record.fold(fact)?;
                let settled = Self::settlement(stored, &folded, now_unix_milliseconds);
                Ok((folded.revision != stored.record.revision)
                    .then(|| (stored.clone(), folded, settled)))
            },
            Some(scheduler_fence),
        )
    }

    /// Applies a local fact only while the exact remote child used to validate
    /// it remains current. Both records are checked in one write transaction.
    ///
    /// # Errors
    /// Returns the errors of [`Self::apply`], and refuses changed, missing or
    /// terminal retained child evidence or a mismatched environment revision.
    pub fn apply_for_retained_agent(
        &self,
        expected: &crate::agent_job_repository::AgentSubmission,
        expected_revision: u64,
        fact: &OperationFact,
        now_unix_milliseconds: u64,
    ) -> Result<OperationSummary, RepositoryFailure> {
        Self::require_bounded(fact)?;
        self.mutate(
            &expected.identity.author_target_identity_digest,
            &expected.identity.operation_identifier,
            expected_revision,
            Some(expected),
            |stored| {
                if stored.selected_environment_revision
                    != expected.identity.selected_environment_revision
                {
                    return Err(RepositoryFailure::RemoteObservationMoved);
                }
                let folded = stored.record.fold(fact)?;
                let settled = Self::settlement(stored, &folded, now_unix_milliseconds);
                Ok((folded.revision != stored.record.revision)
                    .then(|| (stored.clone(), folded, settled)))
            },
            None,
        )
    }

    /// Records one failed recovery attempt while the entire captured subscription
    /// context still matches. This changes only local recovery scheduling and
    /// cannot replace existing execution evidence or mark the operation terminal.
    ///
    /// # Errors
    /// Returns [`RepositoryFailure`] for a different database, invalid time,
    /// missing member or operation, stale view/revision, terminal local work,
    /// changed environment/evidence, a nonconsecutive attempt count, invalid
    /// recovery fold or bounds, or a database refusal.
    pub fn record_subscription_probe_recovery(
        &self,
        ledger: &crate::agent_subscription_ledger::AgentSubscriptionLedger,
        expected: &crate::agent_subscription_ledger::SubscriptionRecoveryView<'_>,
        agent_operation_identifier: &str,
        expected_revision: u64,
        recovery: RecoveryFact,
        now: u64,
    ) -> Result<OperationSummary, RepositoryFailure> {
        let refuse = || RepositoryFailure::RemoteObservationMoved;
        if !self.database.shares_database_with(ledger.database()) || now > i64::MAX as u64 {
            return Err(refuse());
        }
        let member = expected
            .members()
            .iter()
            .find(|member| member.identity.agent_operation_identifier == agent_operation_identifier)
            .ok_or_else(refuse)?;
        if now < member.recorded_at_unix_milliseconds {
            return Err(refuse());
        }
        let fact = OperationFact::Recovery { recovery: recovery.clone() };
        Self::require_bounded(&fact)?;
        let transaction = write_transaction(self.database.connection())?;
        ledger.require_recovery_current(&transaction, expected).map_err(|_| refuse())?;
        let identity = &member.identity;
        let stored = self.read_required(
            &transaction,
            &identity.author_target_identity_digest,
            &identity.operation_identifier,
        )?;
        require_revision(&stored, expected_revision)?;
        let previous = stored.record.outstanding_recovery.as_ref();
        if stored.record.lifecycle_state.is_terminal()
            || stored.selected_environment_revision != identity.selected_environment_revision
            || previous.is_some_and(|held| held.evidence != recovery.evidence)
            || recovery.attempt_count
                != previous
                    .map_or(Some(1), |held| held.attempt_count.checked_add(1))
                    .ok_or_else(refuse)?
        {
            return Err(refuse());
        }
        let folded = stored.record.fold(&fact)?;
        self.write_folded(
            &transaction,
            &stored,
            &folded,
            Self::settlement(&stored, &folded, now),
            false,
        )?;
        let result = self.read_required(
            &transaction,
            &identity.author_target_identity_digest,
            &identity.operation_identifier,
        )?;
        transaction.commit()?;
        Ok(result)
    }

    /// Requires a different nonzero generation and a usable local database/time.
    /// This does not authenticate the caller's remote generation evidence.
    fn require_generation_change(
        &self,
        ledger: &crate::agent_subscription_ledger::AgentSubscriptionLedger,
        expected: &crate::agent_subscription_ledger::SubscriptionRecoveryView<'_>,
        current_generation: u64,
        now: u64,
    ) -> Result<(), RepositoryFailure> {
        if !self.database.shares_database_with(ledger.database())
            || current_generation == 0
            || current_generation == expected.ledger().agent_event_store_generation
            || now > i64::MAX as u64
        {
            return Err(RepositoryFailure::RemoteObservationMoved);
        }
        Ok(())
    }

    /// Settles unavailable prior-generation truth only while the complete
    /// subscription recovery view still matches in one write transaction.
    /// The caller must authenticate the generation change and establish missing
    /// physical truth first. Network refusal is not this evidence. Known success
    /// must instead continue its separate result-acquisition path.
    ///
    /// # Errors
    /// Returns [`RepositoryFailure`] for a different database, invalid generation
    /// or time, missing member or operation, stale view/revision, terminal work,
    /// mismatched environment, known success/nonexecution evidence, an illegal
    /// fold, or a database refusal. A refusal commits no settlement.
    pub fn settle_unavailable_generation(
        &self,
        ledger: &crate::agent_subscription_ledger::AgentSubscriptionLedger,
        expected: &crate::agent_subscription_ledger::SubscriptionRecoveryView<'_>,
        agent_operation_identifier: &str,
        current_generation: u64,
        expected_revision: u64,
        now: u64,
    ) -> Result<OperationSummary, RepositoryFailure> {
        use slingshot_domain::operation::{
            OperationExecutionCertainty, RecoveryExecutionEvidence, TerminalFailure,
            TerminalFailureDisposition, TerminalFailureKind,
        };
        let refuse = || RepositoryFailure::RemoteObservationMoved;
        self.require_generation_change(ledger, expected, current_generation, now)?;
        let member = expected
            .members()
            .iter()
            .find(|member| member.identity.agent_operation_identifier == agent_operation_identifier)
            .ok_or_else(refuse)?;
        if member.identity.agent_event_store_generation == current_generation
            || now < member.recorded_at_unix_milliseconds
            || member.observation.state.is_terminal()
            || member.terminal_disposition.is_some()
        {
            return Err(refuse());
        }
        let transaction = write_transaction(self.database.connection())?;
        ledger.require_recovery_current(&transaction, expected).map_err(|_| refuse())?;
        let identity = &member.identity;
        let stored = self.read_required(
            &transaction,
            &identity.author_target_identity_digest,
            &identity.operation_identifier,
        )?;
        require_revision(&stored, expected_revision)?;
        if stored.record.lifecycle_state.is_terminal()
            || stored.selected_environment_revision != identity.selected_environment_revision
            || stored.record.outstanding_recovery.as_ref().is_some_and(|recovery| {
                matches!(
                    recovery.evidence,
                    RecoveryExecutionEvidence::AuthoritativeRemoteSuccess
                        | RecoveryExecutionEvidence::ExecutionCertainty {
                            certainty: OperationExecutionCertainty::ConfirmedNotExecuted
                        }
                )
            })
        {
            return Err(refuse());
        }
        let certainty = stored
            .record
            .outstanding_recovery
            .as_ref()
            .and_then(|recovery| match recovery.evidence {
                RecoveryExecutionEvidence::ExecutionCertainty { certainty }
                    if certainty != OperationExecutionCertainty::ConfirmedNotExecuted =>
                {
                    Some(certainty)
                }
                _ => None,
            })
            .unwrap_or(OperationExecutionCertainty::RemoteOutcomeUnknown);
        let fact = OperationFact::Terminal {
            failure: TerminalFailure {
                kind: TerminalFailureKind::RemoteStateLost,
                disposition: TerminalFailureDisposition::FailClosedIndeterminate { certainty },
                metadata: None,
            },
        };
        let folded = stored.record.fold(&fact)?;
        self.write_folded(&transaction, &stored, &folded, Some(now), false)?;
        let result = self.read_required(
            &transaction,
            &identity.author_target_identity_digest,
            &identity.operation_identifier,
        )?;
        transaction.commit()?;
        Ok(result)
    }

    /// Atomically records a command-validated no-effect refusal and its complete
    /// failed remote snapshot. Callers must authenticate and classify the failure
    /// first; unknown outcomes must never enter this method.
    ///
    /// # Errors
    /// Returns [`RepositoryFailure`] for missing, changed or terminal retained
    /// work, mismatched local revision/environment, known successful execution,
    /// invalid failure metadata/fold, rejected snapshot persistence, or a
    /// database refusal. Local and remote writes roll back together.
    pub fn settle_rejected_agent_snapshot(
        &self,
        expected: &crate::agent_job_repository::AgentSubmission,
        expected_revision: u64,
        snapshot: &crate::agent_job_repository::FailedAgentSnapshot,
        diagnosis: Option<crate::agent_job_repository::RejectedAgentDiagnosis>,
        category: Option<String>,
        now: u64,
    ) -> Result<OperationSummary, RepositoryFailure> {
        self.settle_failed_agent_snapshot(
            expected,
            expected_revision,
            snapshot,
            diagnosis,
            category,
            false,
            now,
        )
    }

    /// Atomically settles a command-validated positive replication admission
    /// count as remote failure, never as nonexecution or publisher delivery.
    ///
    /// # Errors
    /// Returns the errors of [`Self::settle_rejected_agent_snapshot`], with
    /// snapshot validation for partial admission rather than nonexecution.
    pub fn settle_partial_admission_snapshot(
        &self,
        expected: &crate::agent_job_repository::AgentSubmission,
        expected_revision: u64,
        snapshot: &crate::agent_job_repository::FailedAgentSnapshot,
        category: Option<String>,
        now: u64,
    ) -> Result<OperationSummary, RepositoryFailure> {
        self.settle_failed_agent_snapshot(
            expected,
            expected_revision,
            snapshot,
            None,
            category,
            true,
            now,
        )
    }

    fn settle_failed_agent_snapshot(
        &self,
        expected: &crate::agent_job_repository::AgentSubmission,
        expected_revision: u64,
        snapshot: &crate::agent_job_repository::FailedAgentSnapshot,
        diagnosis: Option<crate::agent_job_repository::RejectedAgentDiagnosis>,
        category: Option<String>,
        partial_admission: bool,
        now: u64,
    ) -> Result<OperationSummary, RepositoryFailure> {
        use slingshot_domain::operation::{
            OperationExecutionCertainty, RecoveryExecutionEvidence, TerminalFailure,
            TerminalFailureDisposition, TerminalFailureKind,
        };
        let refuse = || RepositoryFailure::RemoteObservationMoved;
        let transaction = write_transaction(self.database.connection())?;
        let identity = &expected.identity;
        let current = crate::agent_job_repository::read_submission(
            &transaction,
            &identity.author_target_identity_digest,
            &identity.agent_operation_identifier,
        )
        .map_err(|_| refuse())?;
        if current.as_ref() != Some(expected) || expected.terminal_disposition.is_some() {
            return Err(refuse());
        }
        let stored = self.read_required(
            &transaction,
            &identity.author_target_identity_digest,
            &identity.operation_identifier,
        )?;
        require_revision(&stored, expected_revision)?;
        if stored.selected_environment_revision != identity.selected_environment_revision
            || stored.record.lifecycle_state.is_terminal()
            || stored.record.outstanding_recovery.as_ref().is_some_and(|fact| {
                fact.evidence == RecoveryExecutionEvidence::AuthoritativeRemoteSuccess
            })
        {
            return Err(refuse());
        }
        let fact = OperationFact::Terminal {
            failure: TerminalFailure {
                kind: if partial_admission {
                    TerminalFailureKind::RemoteFailed
                } else {
                    TerminalFailureKind::Rejected
                },
                disposition: if partial_admission {
                    TerminalFailureDisposition::AuthoritativeRemoteFailure
                } else {
                    TerminalFailureDisposition::AuthoritativeNonExecution {
                        certainty: OperationExecutionCertainty::ConfirmedNotExecuted,
                    }
                },
                metadata: diagnosis.map(|diagnosis| diagnosis.as_text().to_owned()).or(category),
            },
        };
        Self::require_bounded(&fact)?;
        let folded = stored.record.fold(&fact)?;
        self.write_folded(&transaction, &stored, &folded, Some(now), false)?;
        crate::agent_job_repository::write_failed_snapshot(
            &transaction,
            expected,
            snapshot,
            partial_admission,
            now,
        )
        .map_err(|_| refuse())?;
        let result = self.read_required(
            &transaction,
            &identity.author_target_identity_digest,
            &identity.operation_identifier,
        )?;
        transaction.commit()?;
        Ok(result)
    }

    /// Requires a local artifact identity and digest in one supported remote slot.
    fn require_artifact_acquisition_identity(
        artifact_identifier: &str,
        artifact_slot: &str,
        content_digest: &str,
    ) -> Result<(), RepositoryFailure> {
        if [artifact_identifier, content_digest].iter().any(|text| {
            text.len() != crate::artifact_store::DIGEST_CHARACTERS
                || !text.bytes().all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
        }) || !matches!(artifact_slot, "content_package" | "loaded_content_json")
        {
            return Err(RepositoryFailure::RemoteObservationMoved);
        }
        Ok(())
    }

    /// Records or reuses the first artifact-acquisition start while the local
    /// revision and full retained child still match. The anchor cannot drift
    /// to a different artifact or be refreshed by a retry.
    ///
    /// # Errors
    /// Returns [`RepositoryFailure`] for invalid identity, slot, digest or time;
    /// changed retained evidence or local revision; absent or terminal local work;
    /// missing authoritative remote success; a conflicting prior anchor; or a
    /// database/readback refusal. Refused transactions commit no anchor changes.
    pub fn begin_artifact_acquisition(
        &self,
        expected: &crate::agent_job_repository::AgentSubmission,
        expected_revision: u64,
        artifact_identifier: &str,
        artifact_slot: &str,
        content_digest: &str,
        now: u64,
    ) -> Result<u64, RepositoryFailure> {
        let refuse = || RepositoryFailure::RemoteObservationMoved;
        Self::require_artifact_acquisition_identity(
            artifact_identifier,
            artifact_slot,
            content_digest,
        )?;
        let now = i64::try_from(now).map_err(|_| refuse())?;
        let transaction = write_transaction(self.database.connection())?;
        let identity = &expected.identity;
        let current = crate::agent_job_repository::read_submission(
            &transaction,
            &identity.author_target_identity_digest,
            &identity.agent_operation_identifier,
        )
        .map_err(|_| refuse())?;
        if current.as_ref() != Some(expected) || expected.terminal_disposition.is_some() {
            return Err(refuse());
        }
        let stored = self.read_required(
            &transaction,
            &identity.author_target_identity_digest,
            &identity.operation_identifier,
        )?;
        require_revision(&stored, expected_revision)?;
        if stored.selected_environment_revision != identity.selected_environment_revision
            || stored.record.lifecycle_state.is_terminal()
            || !stored.record.outstanding_recovery.as_ref().is_some_and(|fact| fact.evidence == slingshot_domain::operation::RecoveryExecutionEvidence::AuthoritativeRemoteSuccess)
        { return Err(refuse()); }
        transaction.execute(
            statement("record the first artifact acquisition for one retained child"),
            rusqlite::params![
                artifact_identifier,
                artifact_slot,
                content_digest,
                now,
                identity.author_target_identity_digest,
                identity.agent_operation_identifier
            ],
        )?;
        let (identifier, slot, digest, started): (String, String, String, i64) = transaction
            .query_row(
                statement("read one retained artifact acquisition anchor"),
                rusqlite::params![
                    identity.author_target_identity_digest,
                    identity.agent_operation_identifier
                ],
                |row| {
                    Ok((
                        row.get("acquisition_artifact_identifier")?,
                        row.get("acquisition_artifact_slot")?,
                        row.get("acquisition_content_digest")?,
                        row.get("acquisition_started_at_unix_milliseconds")?,
                    ))
                },
            )?;
        if identifier != artifact_identifier || slot != artifact_slot || digest != content_digest {
            return Err(refuse());
        }
        let started = u64::try_from(started).map_err(|_| refuse())?;
        transaction.commit()?;
        Ok(started)
    }

    /// Commits a complete successful result and its terminal lifecycle together.
    ///
    /// The operation, verified artifact associations, result representation,
    /// settlement time, and recovery clearing share one immediate transaction.
    /// Consequently a reader sees either the prior recoverable operation or a
    /// complete success, never a terminal row awaiting a second result write.
    ///
    /// # Errors
    /// Returns [`RepositoryFailure`] for invalid settlement shape or bounds,
    /// missing or changed local work, an illegal lifecycle fold, conflicting
    /// artifact length, undecodable stored values, or a database refusal.
    pub fn settle_success(
        &self,
        author_target_identity_digest: &str,
        operation_identifier: &str,
        settlement: &SuccessfulSettlement,
    ) -> Result<OperationSummary, RepositoryFailure> {
        self.settle_success_guarded(
            author_target_identity_digest,
            operation_identifier,
            settlement,
            None,
            None,
            &[],
            None,
        )
    }

    /// Commits a local successful result only while the scheduler fence that
    /// crossed the executor's no-return checkpoint is still the current one.
    /// A worker that lost before execution, or a stale worker after a later
    /// claim, cannot settle the retained operation.
    ///
    /// # Errors
    /// Returns the errors of [`Self::settle_success`], and refuses a missing
    /// executor checkpoint or a scheduler fence that no longer matches.
    pub fn settle_success_with_scheduler_fence(
        &self,
        author_target_identity_digest: &str,
        operation_identifier: &str,
        settlement: &SuccessfulSettlement,
        scheduler_fence: u64,
    ) -> Result<OperationSummary, RepositoryFailure> {
        self.settle_success_guarded(
            author_target_identity_digest,
            operation_identifier,
            settlement,
            None,
            None,
            &[],
            Some(scheduler_fence),
        )
    }

    /// Publishes a verified remote result only while the exact retained child
    /// and local owner revision still match, in the same immediate transaction.
    /// This is a persistence guard, not proof of wire/result validation.
    ///
    /// # Errors
    /// Returns the errors of [`Self::settle_success`], and refuses changed,
    /// terminal or missing retained child evidence or a changed environment.
    pub fn settle_success_for_retained_agent(
        &self,
        expected: &crate::agent_job_repository::AgentSubmission,
        settlement: &SuccessfulSettlement,
    ) -> Result<OperationSummary, RepositoryFailure> {
        self.settle_success_guarded(
            &expected.identity.author_target_identity_digest,
            &expected.identity.operation_identifier,
            settlement,
            Some(expected),
            None,
            &[],
            None,
        )
    }

    /// Atomically publishes the successful remote snapshot and complete local result.
    /// The exact child and local revision guards apply to all writes.
    ///
    /// # Errors
    /// Returns the errors of [`Self::settle_success_for_retained_agent`], and
    /// refuses a snapshot that cannot be persisted against the retained child.
    pub fn settle_success_for_agent_snapshot(
        &self,
        expected: &crate::agent_job_repository::AgentSubmission,
        settlement: &SuccessfulSettlement,
        snapshot: &crate::agent_job_repository::SuccessfulAgentSnapshot,
    ) -> Result<OperationSummary, RepositoryFailure> {
        self.settle_success_guarded(
            &expected.identity.author_target_identity_digest,
            &expected.identity.operation_identifier,
            settlement,
            Some(expected),
            Some(snapshot),
            &[],
            None,
        )
    }

    /// Publishes a complete artifact result and consumes exactly its producer
    /// holds in the same guarded transaction. A failure preserves every hold.
    ///
    /// # Errors
    /// Returns the errors of [`Self::settle_success_for_agent_snapshot`], and
    /// refuses empty or mismatched publication counts or a hold that cannot be
    /// consumed for its matching artifact. All writes roll back on refusal.
    pub fn settle_success_for_publications(
        &self,
        expected: &crate::agent_job_repository::AgentSubmission,
        settlement: &SuccessfulSettlement,
        snapshot: &crate::agent_job_repository::SuccessfulAgentSnapshot,
        publications: &[crate::persistent_capacity::ArtifactPublication],
    ) -> Result<OperationSummary, RepositoryFailure> {
        if publications.len() != settlement.artifacts.len() || publications.is_empty() {
            return Err(RepositoryFailure::RemoteObservationMoved);
        }
        self.settle_success_guarded(
            &expected.identity.author_target_identity_digest,
            &expected.identity.operation_identifier,
            settlement,
            Some(expected),
            Some(snapshot),
            publications,
            None,
        )
    }

    /// Rechecks the exact retained child under the settlement write lock.
    fn require_settlement_remote(
        transaction: &rusqlite::Transaction<'_>,
        remote: Option<&crate::agent_job_repository::AgentSubmission>,
    ) -> Result<(), RepositoryFailure> {
        if let Some(expected) = remote {
            let current = crate::agent_job_repository::read_submission(
                transaction,
                &expected.identity.author_target_identity_digest,
                &expected.identity.agent_operation_identifier,
            )
            .map_err(|_| RepositoryFailure::RemoteObservationMoved)?;
            if current.as_ref() != Some(expected) || expected.terminal_disposition.is_some() {
                return Err(RepositoryFailure::RemoteObservationMoved);
            }
        }
        Ok(())
    }

    /// Rechecks the retained scheduler checkpoint under the settlement write lock.
    fn require_settlement_fence(
        transaction: &rusqlite::Transaction<'_>,
        author_target_identity_digest: &str,
        operation_identifier: &str,
        scheduler_fence: Option<u64>,
    ) -> Result<(), RepositoryFailure> {
        if let Some(expected_fence) = scheduler_fence {
            let (current_fence, checkpoint): (Option<i64>, Option<String>) = transaction
                .query_row(
                    statement("read one retained operation execution fence"),
                    rusqlite::params![author_target_identity_digest, operation_identifier],
                    |row| Ok((row.get(0)?, row.get(1)?)),
                )
                .optional()?
                .ok_or_else(|| RepositoryFailure::NoSuchOperation {
                    identifier: operation_identifier.to_owned(),
                })?;
            if current_fence != Some(i64::try_from(expected_fence).unwrap_or(i64::MAX))
                || checkpoint.is_none()
            {
                return Err(RepositoryFailure::RemoteObservationMoved);
            }
        }
        Ok(())
    }

    fn settle_success_guarded(
        &self,
        author_target_identity_digest: &str,
        operation_identifier: &str,
        settlement: &SuccessfulSettlement,
        remote: Option<&crate::agent_job_repository::AgentSubmission>,
        snapshot: Option<&crate::agent_job_repository::SuccessfulAgentSnapshot>,
        publications: &[crate::persistent_capacity::ArtifactPublication],
        scheduler_fence: Option<u64>,
    ) -> Result<OperationSummary, RepositoryFailure> {
        let disposition = settlement.disposition()?;
        if let Some(inline) = &settlement.inline_result {
            require_within("inline result", "maximum_inline_machine_result_bytes", inline)?;
        }
        let transaction = write_transaction(self.database.connection())?;
        Self::require_settlement_remote(&transaction, remote)?;
        let stored =
            self.read_required(&transaction, author_target_identity_digest, operation_identifier)?;
        if remote.is_some_and(|expected| {
            stored.selected_environment_revision != expected.identity.selected_environment_revision
        }) {
            return Err(RepositoryFailure::RemoteObservationMoved);
        }
        Self::require_settlement_fence(
            &transaction,
            author_target_identity_digest,
            operation_identifier,
            scheduler_fence,
        )?;
        require_revision(&stored, settlement.expected_revision)?;
        if stored.record.lifecycle_state != settlement.expected_lifecycle_state {
            return Err(RepositoryFailure::LifecycleMoved {
                expected: settlement.expected_lifecycle_state,
                stored: stored.record.lifecycle_state,
            });
        }
        let folded = stored.record.fold(&OperationFact::Lifecycle {
            lifecycle_state: OperationLifecycleState::Succeeded,
        })?;
        let folded = OperationRecord { outstanding_recovery: None, ..folded };
        for artifact in &settlement.artifacts {
            self.write_settled_artifact(
                &transaction,
                author_target_identity_digest,
                operation_identifier,
                artifact,
                settlement.settled_at_unix_milliseconds,
            )?;
        }
        let carried = OperationSummary {
            result_disposition: Some(disposition),
            result_inline_bytes: settlement.inline_result.clone(),
            ..stored.clone()
        };
        self.write_folded(
            &transaction,
            &carried,
            &folded,
            Some(settlement.settled_at_unix_milliseconds),
            false,
        )?;
        if let Some(snapshot) = snapshot {
            crate::agent_job_repository::write_successful_snapshot(
                &transaction,
                remote.ok_or(RepositoryFailure::RemoteObservationMoved)?,
                snapshot,
                settlement.settled_at_unix_milliseconds,
            )
            .map_err(|_| RepositoryFailure::RemoteObservationMoved)?;
        }
        for (publication, artifact) in publications.iter().zip(&settlement.artifacts) {
            let changed = transaction.execute(
                statement("consume one completed artifact publication"),
                rusqlite::params![
                    publication.identifier(),
                    artifact.artifact_identifier,
                    artifact.content_digest
                ],
            )?;
            if changed != 1 {
                return Err(RepositoryFailure::RemoteObservationMoved);
            }
        }
        let current =
            self.read_required(&transaction, author_target_identity_digest, operation_identifier)?;
        transaction.commit()?;
        Ok(current)
    }

    /// Writes one already-verified artifact association inside a success transaction.
    fn write_settled_artifact(
        &self,
        transaction: &rusqlite::Transaction<'_>,
        author_target_identity_digest: &str,
        operation_identifier: &str,
        artifact: &ProducedArtifact,
        now_unix_milliseconds: u64,
    ) -> Result<(), RepositoryFailure> {
        let byte_length = i64::try_from(artifact.byte_length).unwrap_or(i64::MAX);
        let existing: Option<i64> = transaction
            .query_row(
                statement("read one artifact blob's recorded length"),
                rusqlite::params![artifact.content_digest],
                |row| row.get(0),
            )
            .optional()?;
        if let Some(existing) = existing {
            let stored = u64::try_from(existing).map_err(|_| RepositoryFailure::NotDecodable {
                column: "artifact_blob.byte_length",
                detail: format!("{existing} is below zero"),
            })?;
            if stored != artifact.byte_length {
                return Err(RepositoryFailure::ArtifactLengthConflict {
                    digest: artifact.content_digest.clone(),
                    stored,
                    provided: artifact.byte_length,
                });
            }
        }
        transaction.execute(
            statement("record one artifact's content, once per digest"),
            rusqlite::params![
                byte_length,
                artifact.content_digest,
                i64::try_from(now_unix_milliseconds).unwrap_or(i64::MAX),
            ],
        )?;
        transaction.execute(
            statement("associate one artifact with the operation slot it fills"),
            rusqlite::params![
                artifact.artifact_identifier,
                artifact.artifact_slot,
                author_target_identity_digest,
                byte_length,
                artifact.content_digest,
                artifact.media_type,
                operation_identifier,
            ],
        )?;
        Ok(())
    }
}
