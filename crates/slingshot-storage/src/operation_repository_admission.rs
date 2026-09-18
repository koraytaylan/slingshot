//! Admission: turning one request into the row that answers for it.

use super::*;

impl OperationRepository {
    /// Admits one operation, or returns the row that already answers for it.
    ///
    /// One `synchronous = FULL` transaction reserves the arrival sequence,
    /// writes the row as `queued`, and commits; a caller is told it was
    /// admitted only after that commit returns. A row already under that name
    /// is a replay when the revision and fingerprint match, and otherwise a
    /// conflict, which writes nothing.
    ///
    /// # Errors
    ///
    /// Returns [`RepositoryFailure`] for a bounded field over its bound, a
    /// namespace at its capacity, or a refusal.
    pub fn admit(
        &self,
        request: &AdmissionRequest,
        now_unix_milliseconds: u64,
    ) -> Result<AdmissionOutcome, RepositoryFailure> {
        self.admit_checked(request, now_unix_milliseconds, None)
    }

    /// Admits under pending-work bounds in the same transaction as the insert.
    /// Replays and conflicts are resolved before capacity is consulted.
    ///
    /// # Errors
    ///
    /// Returns [`RepositoryFailure::PendingCapacity`] without inserting a row,
    /// or the same persistent/storage refusals as [`Self::admit`].
    pub fn admit_with_pending_capacity(
        &self,
        request: &AdmissionRequest,
        now_unix_milliseconds: u64,
        capacity: PendingAdmissionCapacity<'_>,
    ) -> Result<AdmissionOutcome, RepositoryFailure> {
        self.admit_checked(request, now_unix_milliseconds, Some(capacity))
    }

    fn admit_checked(
        &self,
        request: &AdmissionRequest,
        now_unix_milliseconds: u64,
        pending_capacity: Option<PendingAdmissionCapacity<'_>>,
    ) -> Result<AdmissionOutcome, RepositoryFailure> {
        if let Some(workflow) = request.workflow_correlation_identifier.as_deref() {
            require_within(
                "workflow_correlation_identifier",
                "maximum_workflow_correlation_identifier_bytes",
                workflow,
            )?;
        }
        let transaction = write_transaction(self.database.connection())?;
        let outcome = match Self::read_within(
            &transaction,
            &request.author_target_identity_digest,
            &request.operation_identifier,
        )? {
            Some(stored) => Self::classify_stored(request, stored),
            None => {
                if let Some(capacity) = pending_capacity {
                    Self::require_pending_room(&transaction, request, capacity)?;
                }
                self.require_room()?;
                self.insert(&transaction, request, now_unix_milliseconds)?;
                let admitted = self.read_required(
                    &transaction,
                    &request.author_target_identity_digest,
                    &request.operation_identifier,
                )?;
                AdmissionOutcome::Admitted(Box::new(admitted))
            }
        };
        transaction.commit()?;
        Ok(outcome)
    }

    fn require_pending_room(
        transaction: &rusqlite::Transaction<'_>,
        request: &AdmissionRequest,
        capacity: PendingAdmissionCapacity<'_>,
    ) -> Result<(), RepositoryFailure> {
        let mut query =
            transaction.prepare(statement("count waiting operations during admission"))?;
        let mut rows = query.query(rusqlite::params![request.author_target_identity_digest])?;
        let mut global = 0_u64;
        let mut caller = 0_u64;
        while let Some(row) = rows.next()? {
            let identifier: String = row.get(0)?;
            if capacity.active_operations.contains(&identifier) {
                continue;
            }
            global = global.saturating_add(1);
            let identity: Option<String> = row.get(1)?;
            if identity == request.caller_identity {
                caller = caller.saturating_add(1);
            }
        }
        for (held, limit, per_caller) in
            [(global, capacity.global_pending, false), (caller, capacity.pending_per_caller, true)]
        {
            if held >= limit {
                return Err(RepositoryFailure::PendingCapacity { held, limit, per_caller });
            }
        }
        Ok(())
    }

    /// Requires this namespace to have room for one more operation row.
    ///
    /// Asked only where a row would be created, so a replay consumes nothing
    /// and is never refused for space.
    fn require_room(&self) -> Result<(), RepositoryFailure> {
        self.account().require_room_for_operation()?;
        Ok(())
    }

    /// Returns the accounting this namespace is held to.
    fn account(&self) -> PersistentCapacityAccount<'_> {
        PersistentCapacityAccount::new(&self.database, self.policy)
    }

    /// Returns what a stored row makes of a repeated identifier.
    fn classify_stored(request: &AdmissionRequest, stored: OperationSummary) -> AdmissionOutcome {
        let repeat = classify_repeat(
            &stored.command_fingerprint,
            &stored.selected_environment_revision,
            &request.command_fingerprint,
            &request.selected_environment_revision,
        );
        match repeat {
            RepeatedIdentifier::Retry => AdmissionOutcome::Replayed(Box::new(stored)),
            RepeatedIdentifier::Conflict => AdmissionOutcome::Conflict(Box::new(stored)),
        }
    }

    /// Writes one new operation row inside `transaction`.
    fn insert(
        &self,
        transaction: &rusqlite::Transaction<'_>,
        request: &AdmissionRequest,
        now_unix_milliseconds: u64,
    ) -> Result<(), RepositoryFailure> {
        let sequence: i64 = transaction.query_row(
            statement("reserve the next enqueue sequence inside one target partition"),
            rusqlite::params![request.author_target_identity_digest],
            |row| row.get(0),
        )?;
        let admitted = OperationRecord::admitted();
        transaction.execute(
            statement("admit one operation"),
            rusqlite::params![
                request.author_target_identity,
                request.author_target_identity_digest,
                request.caller_identity,
                request.canonical_command,
                request.command_fingerprint.as_text(),
                request.command_wire_name,
                request.daemon_runtime_contract_digest,
                sequence,
                request.installation_identifier.as_text(),
                encode_word(&admitted.lifecycle_state)?,
                request.operation_identifier,
                i64::try_from(admitted.revision).unwrap_or(i64::MAX),
                i64::try_from(now_unix_milliseconds).unwrap_or(i64::MAX),
                request.selected_environment_revision,
                request.workflow_correlation_identifier,
            ],
        )?;
        Ok(())
    }
}
