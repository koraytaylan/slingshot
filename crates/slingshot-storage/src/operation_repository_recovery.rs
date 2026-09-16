//! Reconstruction and durable recovery-resume receipts.

use super::*;

impl OperationRepository {
    /// Returns every operation in one partition, in the order it arrived.
    ///
    /// Nonterminal rows are the work still to do, in their callers' order.
    ///
    /// # Errors
    ///
    /// Returns [`RepositoryFailure`] when a stored value does not decode or the
    /// database refuses.
    pub fn reconstruct(
        &self,
        author_target_identity_digest: &str,
    ) -> Result<Vec<OperationSummary>, RepositoryFailure> {
        let transaction = self.database.connection().unchecked_transaction()?;
        let identifiers: Vec<String> = {
            let mut prepared = transaction
                .prepare(statement("reconstruct one target's operations in enqueue order"))?;
            let rows = prepared
                .query_map(rusqlite::params![author_target_identity_digest], |row| {
                    row.get::<_, String>(0)
                })?;
            rows.collect::<Result<Vec<String>, _>>()?
        };
        let mut found = Vec::with_capacity(identifiers.len());
        for identifier in identifiers {
            found.push(self.read_required(
                &transaction,
                author_target_identity_digest,
                &identifier,
            )?);
        }
        transaction.commit()?;
        Ok(found)
    }

    /// Records one recovery-resume receipt, or replays the one already there.
    ///
    /// Keyed by target and source fingerprint, so an identical request finds
    /// its own committed proof whatever the operation has done since. Whether a
    /// resume took effect cannot be reconstructed from current state.
    ///
    /// # Errors
    ///
    /// Returns [`RepositoryFailure::ReceiptsExhausted`] once the operation
    /// holds every receipt the contract allows, or a database failure.
    pub fn record_resume_receipt(
        &self,
        author_target_identity_digest: &str,
        operation_identifier: &str,
        source_fingerprint: &str,
        selected_environment_revision: &str,
        applied_operation_revision: u64,
        now_unix_milliseconds: u64,
    ) -> Result<ResumeOutcome, RepositoryFailure> {
        let transaction = write_transaction(self.database.connection())?;
        if let Some(held) = Self::receipt_within(
            &transaction,
            author_target_identity_digest,
            operation_identifier,
            source_fingerprint,
        )? {
            if held.selected_environment_revision != selected_environment_revision {
                return Ok(ResumeOutcome::Refused(ResumeEligibilityRefusal::EnvironmentRevision));
            }
            transaction.commit()?;
            return Ok(ResumeOutcome::Replayed(Box::new(held)));
        }
        self.require_receipt_capacity_within(
            &transaction,
            author_target_identity_digest,
            operation_identifier,
        )?;
        transaction.execute(
            statement("record one recovery-resume receipt"),
            rusqlite::params![
                i64::try_from(applied_operation_revision).unwrap_or(i64::MAX),
                author_target_identity_digest,
                operation_identifier,
                i64::try_from(now_unix_milliseconds).unwrap_or(i64::MAX),
                selected_environment_revision,
                source_fingerprint,
            ],
        )?;
        let written = Self::receipt_within(
            &transaction,
            author_target_identity_digest,
            operation_identifier,
            source_fingerprint,
        )?
        .ok_or_else(|| RepositoryFailure::NoSuchOperation {
            identifier: operation_identifier.to_owned(),
        })?;
        transaction.commit()?;
        Ok(ResumeOutcome::Applied(Box::new(written)))
    }

    /// Rechecks resumability and records one receipt in the same immediate transaction.
    ///
    /// Unlike [`Self::record_resume_receipt`], this is the product boundary: a
    /// stale caller can never commit an `Applied` receipt for a terminal,
    /// moved, or differently recovering operation.
    ///
    /// # Errors
    /// Returns [`RepositoryFailure`] if the operation is absent, stored values
    /// cannot be decoded, receipt capacity is exhausted, or the database refuses.
    /// Eligibility disagreements are returned as [`ResumeOutcome::Refused`].
    pub fn record_eligible_resume_receipt(
        &self,
        author_target_identity_digest: &str,
        operation_identifier: &str,
        source_fingerprint: &str,
        selected_environment_revision: &str,
        expected_recovery_category: RecoveryCategory,
        expected_operation_revision: u64,
        now_unix_milliseconds: u64,
    ) -> Result<ResumeOutcome, RepositoryFailure> {
        let transaction = write_transaction(self.database.connection())?;
        if let Some(held) = Self::receipt_within(
            &transaction,
            author_target_identity_digest,
            operation_identifier,
            source_fingerprint,
        )? {
            if held.selected_environment_revision != selected_environment_revision {
                return Ok(ResumeOutcome::Refused(ResumeEligibilityRefusal::EnvironmentRevision));
            }
            transaction.commit()?;
            return Ok(ResumeOutcome::Replayed(Box::new(held)));
        }
        let current =
            self.read_required(&transaction, author_target_identity_digest, operation_identifier)?;
        let refusal = if current.selected_environment_revision != selected_environment_revision {
            Some(ResumeEligibilityRefusal::EnvironmentRevision)
        } else if current.record.lifecycle_state.is_terminal() {
            Some(ResumeEligibilityRefusal::Terminal)
        } else if let Some(recovery) = &current.record.outstanding_recovery {
            if recovery.category != expected_recovery_category {
                Some(ResumeEligibilityRefusal::Category {
                    holding: recovery.category,
                    named: expected_recovery_category,
                })
            } else if !recovery.manual_resume_eligible {
                Some(ResumeEligibilityRefusal::NotManual)
            } else if current.record.revision != expected_operation_revision {
                Some(ResumeEligibilityRefusal::Revision {
                    expected: expected_operation_revision,
                    observed: current.record.revision,
                })
            } else {
                None
            }
        } else {
            Some(ResumeEligibilityRefusal::NotWaiting)
        };
        if let Some(refusal) = refusal {
            transaction.commit()?;
            return Ok(ResumeOutcome::Refused(refusal));
        }
        self.require_receipt_capacity_within(
            &transaction,
            author_target_identity_digest,
            operation_identifier,
        )?;
        transaction.execute(
            statement("record one recovery-resume receipt"),
            rusqlite::params![
                i64::try_from(expected_operation_revision).unwrap_or(i64::MAX),
                author_target_identity_digest,
                operation_identifier,
                i64::try_from(now_unix_milliseconds).unwrap_or(i64::MAX),
                selected_environment_revision,
                source_fingerprint,
            ],
        )?;
        // The receipt and eligibility transition are one durable fact: make
        // the operation queueable and clear the recovery hold before commit.
        // Without this fold a successful resume is acknowledged but remains
        // parked forever, so the scheduler can never claim it.
        self.activate_resumed_operation(&transaction, &current)?;
        let written = Self::receipt_within(
            &transaction,
            author_target_identity_digest,
            operation_identifier,
            source_fingerprint,
        )?
        .ok_or_else(|| RepositoryFailure::NoSuchOperation {
            identifier: operation_identifier.to_owned(),
        })?;
        transaction.commit()?;
        Ok(ResumeOutcome::Applied(Box::new(written)))
    }

    fn activate_resumed_operation(
        &self,
        transaction: &rusqlite::Transaction<'_>,
        current: &OperationSummary,
    ) -> Result<(), RepositoryFailure> {
        // Keep the recovery fact as durable evidence while making it no longer
        // manually resumable. A resume does not rewind the operation lifecycle
        // (for example, Running cannot transition back to Queued); the
        // scheduler's committed receipt is the eligibility signal.
        if current.record.outstanding_recovery.is_none() {
            return Err(RepositoryFailure::NoSuchOperation {
                identifier: current.operation_identifier.clone(),
            });
        }
        let cleared = transaction.execute(
            statement("clear one resumed operation's stale scheduler claim"),
            rusqlite::params![
                &current.author_target_identity_digest,
                &current.operation_identifier,
                encode_word(&current.record.lifecycle_state)?,
                i64::try_from(current.record.revision).unwrap_or(i64::MAX),
            ],
        )?;
        if cleared == ONE_ROW {
            Ok(())
        } else {
            Err(RepositoryFailure::RevisionMoved {
                expected: current.record.revision,
                stored: current.record.revision,
            })
        }
    }

    /// Refuses a fresh receipt once the transaction sees an operation at its bound.
    fn require_receipt_capacity_within(
        &self,
        connection: &rusqlite::Connection,
        author_target_identity_digest: &str,
        operation_identifier: &str,
    ) -> Result<(), RepositoryFailure> {
        let counted: i64 = connection.query_row(
            statement("count one operation's recovery-resume receipts"),
            rusqlite::params![author_target_identity_digest, operation_identifier],
            |row| row.get(0),
        )?;
        let held = u64::try_from(counted).map_err(|_| RepositoryFailure::NotDecodable {
            column: "recovery-resume receipt count",
            detail: format!("{counted} is not a non-negative count"),
        })?;
        let facts = CapacityFacts {
            held,
            limit: self.policy.recovery_resume_receipts_per_operation,
            wanted: 1,
        };
        if facts.fits() {
            Ok(())
        } else {
            Err(RepositoryFailure::ReceiptsExhausted { allowed: facts.limit })
        }
    }

    /// Checks caller-supplied receipt identity and time before opening a write.
    fn require_activation_identity(
        expected: &crate::agent_job_repository::AgentSubmission,
        receipt: &RecoveryResumeReceipt,
        now_unix_milliseconds: u64,
    ) -> Result<(), RepositoryFailure> {
        let identity = &expected.identity;
        if receipt.operation_identifier != identity.operation_identifier
            || receipt.selected_environment_revision != identity.selected_environment_revision
            || now_unix_milliseconds < receipt.recorded_at_unix_milliseconds
        {
            return Err(RepositoryFailure::RemoteObservationMoved);
        }
        Ok(())
    }

    /// Activates the exact persisted resume receipt once, guarded by the local
    /// source revision and complete remote child. A stale/replayed receipt does
    /// not clear a later pause. This is not a scheduler lease or permission to POST.
    /// Returns `None` if the persisted operation is terminal or its revision no
    /// longer matches the receipt; neither case changes the operation.
    ///
    /// # Errors
    /// Returns [`RepositoryFailure`] for mismatched receipt, child or recovery
    /// evidence, a time preceding the receipt, an absent operation, an invalid
    /// recovery fold, a bound violation, or a database/readback refusal.
    pub fn activate_retained_recovery(
        &self,
        expected: &crate::agent_job_repository::AgentSubmission,
        receipt: &RecoveryResumeReceipt,
        category: RecoveryCategory,
        now_unix_milliseconds: u64,
    ) -> Result<Option<OperationSummary>, RepositoryFailure> {
        Self::require_activation_identity(expected, receipt, now_unix_milliseconds)?;
        let identity = &expected.identity;
        let transaction = write_transaction(self.database.connection())?;
        let persisted = Self::receipt_within(
            &transaction,
            &identity.author_target_identity_digest,
            &identity.operation_identifier,
            &receipt.source_fingerprint,
        )?;
        if persisted.as_ref() != Some(receipt) {
            return Err(RepositoryFailure::RemoteObservationMoved);
        }
        let stored = self.read_required(
            &transaction,
            &identity.author_target_identity_digest,
            &identity.operation_identifier,
        )?;
        if stored.record.revision != receipt.applied_operation_revision
            || stored.record.lifecycle_state.is_terminal()
        {
            return Ok(None);
        }
        let remote = crate::agent_job_repository::read_submission(
            &transaction,
            &identity.author_target_identity_digest,
            &identity.agent_operation_identifier,
        )
        .map_err(|_| RepositoryFailure::RemoteObservationMoved)?;
        if remote.as_ref() != Some(expected)
            || expected.terminal_disposition.is_some()
            || stored.selected_environment_revision != receipt.selected_environment_revision
        {
            return Err(RepositoryFailure::RemoteObservationMoved);
        }
        let recovery = stored
            .record
            .outstanding_recovery
            .as_ref()
            .filter(|fact| fact.manual_resume_eligible && fact.category == category)
            .ok_or(RepositoryFailure::RemoteObservationMoved)?;
        let fact = OperationFact::Recovery {
            recovery: RecoveryFact {
                manual_resume_eligible: false,
                attempt_count: 0,
                retry_delay_milliseconds: 0,
                retry_observed_at_unix_milliseconds: now_unix_milliseconds,
                detail: "recovery activated by a persisted resume receipt".to_owned(),
                ..recovery.clone()
            },
        };
        Self::require_bounded(&fact)?;
        let folded = stored.record.fold(&fact)?;
        self.write_folded(&transaction, &stored, &folded, None)?;
        let activated = self.read_required(
            &transaction,
            &identity.author_target_identity_digest,
            &identity.operation_identifier,
        )?;
        transaction.commit()?;
        Ok(Some(activated))
    }

    /// Returns one recovery-resume receipt, or nothing.
    ///
    /// # Errors
    ///
    /// Returns [`RepositoryFailure`] when the database refuses.
    pub fn read_resume_receipt(
        &self,
        author_target_identity_digest: &str,
        operation_identifier: &str,
        source_fingerprint: &str,
    ) -> Result<Option<RecoveryResumeReceipt>, RepositoryFailure> {
        Self::receipt_within(
            self.database.connection(),
            author_target_identity_digest,
            operation_identifier,
            source_fingerprint,
        )
    }

    /// Reads one receipt, inside whatever transaction the caller has open.
    fn receipt_within(
        connection: &rusqlite::Connection,
        author_target_identity_digest: &str,
        operation_identifier: &str,
        source_fingerprint: &str,
    ) -> Result<Option<RecoveryResumeReceipt>, RepositoryFailure> {
        let mut prepared = connection.prepare(statement(
            "read one recovery-resume receipt by operation and source fingerprint",
        ))?;
        let row = prepared
            .query_row(
                rusqlite::params![
                    author_target_identity_digest,
                    operation_identifier,
                    source_fingerprint
                ],
                |row| Ok(Self::receipt_from(row, source_fingerprint)),
            )
            .optional()?;
        row.transpose()
    }

    /// Returns the receipt one row spells.
    fn receipt_from(
        row: &rusqlite::Row<'_>,
        source_fingerprint: &str,
    ) -> Result<RecoveryResumeReceipt, RepositoryFailure> {
        Ok(RecoveryResumeReceipt {
            applied_operation_revision: count(row, "applied_operation_revision")?,
            operation_identifier: row.get("operation_identifier")?,
            recorded_at_unix_milliseconds: count(row, "recorded_at_unix_milliseconds")?,
            selected_environment_revision: row.get("selected_environment_revision")?,
            source_fingerprint: source_fingerprint.to_owned(),
        })
    }
}
