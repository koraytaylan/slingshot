//! Counting what a namespace holds, and refusing before it holds too much.
//!
//! Every count here is read from the rows that are authoritative for it rather
//! than from a counter kept alongside them. A counter can drift; a `COUNT` over
//! the table cannot, and it makes restart reconstruction free - there is
//! nothing to reconstruct, because there was never a second copy of the truth.
//!
//! Reservations are the exception, and deliberately so. An artifact being
//! installed has no row yet, so its bytes are held in memory against the same
//! bound until the association commits. That is the right lifetime: a
//! reservation belongs to an installation in progress, and an installation
//! interrupted by a restart is not in progress any more. What it leaves behind
//! is the staged file the artifact store deliberately does not delete, which
//! nothing addresses and which no count includes.
//!
//! One account belongs to one open database, and a database belongs to one
//! daemon process, so the reservations are process-local by construction. The
//! check and the take are still one critical section rather than two, because
//! an invariant that holds only because of what a type happens not to implement
//! is an invariant nobody wrote down.
//!
//! Refusal always happens before mutation. Nothing here deletes a terminal row,
//! a receipt, or committed content to make room: reaching a bound is a fact to
//! report, with what is held and what would release some, not a licence to
//! destroy the record of work that happened.

use std::sync::Mutex;

use slingshot_domain::persistent_capacity::{
    CapacityFacts, CapacityRefusal, PersistentCapacityPolicy,
};

use crate::database::OperationDatabase;

/// Returns the text of the inventoried statement with `purpose`.
fn statement(purpose: &str) -> &'static str {
    crate::sqlite_statement_inventory::STATEMENTS
        .iter()
        .find(|inventoried| inventoried.purpose == purpose)
        .map(|inventoried| inventoried.text)
        .unwrap_or_else(|| panic!("the inventory holds a statement for {purpose}"))
}

/// Reason a capacity question could not be answered or acted on.
#[derive(Debug, thiserror::Error)]
pub enum AccountingFailure {
    /// The namespace is at or past a bound.
    #[error(transparent)]
    Refused(#[from] CapacityRefusal),
    /// The database refused.
    #[error("the database refused: {0}")]
    DatabaseRefused(String),
    /// A stored count is not one.
    #[error("a stored count is {0}, which is not a count")]
    NotACount(i64),
    /// A thread failed while holding the reservations.
    #[error("the reservations cannot be read, because a holder of them failed")]
    LockPoisoned,
}

/// Returns a database refusal as this module's failure.
fn refused(failure: rusqlite::Error) -> AccountingFailure {
    AccountingFailure::DatabaseRefused(failure.to_string())
}

/// One artifact's bytes, held against the bound until its association commits.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ArtifactReservation {
    /// How many bytes are held.
    pub byte_length: u64,
    /// Which reservation this is.
    pub ticket: u64,
}

/// What one namespace is currently holding.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NamespaceUsage {
    /// Bytes committed content occupies.
    pub committed_artifact_bytes: u64,
    /// Operation rows held.
    pub operation_rows: u64,
    /// Bytes reservations are holding.
    pub reserved_artifact_bytes: u64,
}

/// The accounting for one runtime namespace.
#[derive(Debug)]
pub struct PersistentCapacityAccount<'database> {
    /// The database the authoritative rows live in.
    database: &'database OperationDatabase,
    /// The bounds this namespace is held to.
    policy: PersistentCapacityPolicy,
    /// Reservations for installations in progress.
    reservations: Mutex<Vec<ArtifactReservation>>,
    /// The next reservation's ticket.
    next_ticket: Mutex<u64>,
}

impl<'database> PersistentCapacityAccount<'database> {
    /// Returns the accounting for the namespace `database` holds.
    #[must_use]
    pub fn new(database: &'database OperationDatabase, policy: PersistentCapacityPolicy) -> Self {
        Self { database, policy, reservations: Mutex::new(Vec::new()), next_ticket: Mutex::new(1) }
    }

    /// Returns the bounds this namespace is held to.
    #[must_use]
    pub fn policy(&self) -> PersistentCapacityPolicy {
        self.policy
    }

    /// Returns one count from the rows that are authoritative for it.
    fn count(
        &self,
        purpose: &str,
        parameters: &[&dyn rusqlite::ToSql],
    ) -> Result<u64, AccountingFailure> {
        let counted: i64 = self
            .database
            .connection()
            .query_row(statement(purpose), parameters, |row| row.get(0))
            .map_err(refused)?;
        u64::try_from(counted).map_err(|_| AccountingFailure::NotACount(counted))
    }

    /// Returns what this namespace is currently holding.
    ///
    /// # Errors
    ///
    /// Returns [`AccountingFailure::DatabaseRefused`].
    pub fn usage(&self) -> Result<NamespaceUsage, AccountingFailure> {
        Ok(NamespaceUsage {
            committed_artifact_bytes: self
                .count("measure the bytes this namespace's committed content occupies", &[])?,
            operation_rows: self.count("count this namespace's retained operation rows", &[])?,
            reserved_artifact_bytes: self.reserved_bytes(),
        })
    }

    /// Returns how many bytes reservations are holding.
    fn reserved_bytes(&self) -> u64 {
        self.reservations
            .lock()
            .map(|held| held.iter().map(|reservation| reservation.byte_length).sum())
            .unwrap_or_default()
    }
}

impl PersistentCapacityAccount<'_> {
    /// Requires room for one more operation row.
    ///
    /// Asked before admission writes anything, so a namespace at its bound
    /// refuses with a row count that is still true rather than after creating
    /// the row that made it false.
    ///
    /// # Errors
    ///
    /// Returns [`CapacityRefusal::OperationRows`] wrapped in
    /// [`AccountingFailure::Refused`].
    pub fn require_room_for_operation(&self) -> Result<CapacityFacts, AccountingFailure> {
        let facts = CapacityFacts {
            held: self.count("count this namespace's retained operation rows", &[])?,
            limit: self.policy.retained_operation_rows,
            wanted: 1,
        };
        if facts.fits() { Ok(facts) } else { Err(CapacityRefusal::OperationRows { facts }.into()) }
    }

    /// Requires room for one more resume receipt on `operation_identifier`.
    ///
    /// # Errors
    ///
    /// Returns [`CapacityRefusal::ResumeReceipts`].
    pub fn require_room_for_resume_receipt(
        &self,
        author_target_identity_digest: &str,
        operation_identifier: &str,
    ) -> Result<CapacityFacts, AccountingFailure> {
        let facts = CapacityFacts {
            held: self.count(
                "count one operation's recovery-resume receipts",
                &[&author_target_identity_digest, &operation_identifier],
            )?,
            limit: self.policy.recovery_resume_receipts_per_operation,
            wanted: 1,
        };
        if facts.fits() { Ok(facts) } else { Err(CapacityRefusal::ResumeReceipts { facts }.into()) }
    }

    /// Requires room for one more maintenance-application receipt on a target.
    ///
    /// # Errors
    ///
    /// Returns [`CapacityRefusal::MaintenanceReceipts`].
    pub fn require_room_for_maintenance_receipt(
        &self,
        author_target_identity_digest: &str,
    ) -> Result<CapacityFacts, AccountingFailure> {
        let facts = CapacityFacts {
            held: self.count(
                "count one target's maintenance-application receipts",
                &[&author_target_identity_digest],
            )?,
            limit: self.policy.maintenance_application_receipts_per_target,
            wanted: 1,
        };
        if facts.fits() {
            Ok(facts)
        } else {
            Err(CapacityRefusal::MaintenanceReceipts { facts }.into())
        }
    }

    /// Requires room for `wanted` more maintenance-result associations.
    ///
    /// # Errors
    ///
    /// Returns [`CapacityRefusal::MaintenanceAssociations`].
    pub fn require_room_for_maintenance_associations(
        &self,
        author_target_identity_digest: &str,
        wanted: u64,
    ) -> Result<CapacityFacts, AccountingFailure> {
        let facts = CapacityFacts {
            held: self.count(
                "count one target's maintenance-result associations",
                &[&author_target_identity_digest],
            )?,
            limit: self.policy.maintenance_result_associations_per_target,
            wanted,
        };
        if facts.fits() {
            Ok(facts)
        } else {
            Err(CapacityRefusal::MaintenanceAssociations { facts }.into())
        }
    }

    /// Holds `byte_length` bytes against the artifact bound.
    ///
    /// Content already committed under `content_digest` reserves nothing: the
    /// bytes are already being counted, and installing them again would produce
    /// the same one file. So a duplicate consumes no second allocation, which
    /// is the same rule the store follows by addressing content with its
    /// digest.
    ///
    /// # Errors
    ///
    /// Returns [`CapacityRefusal::ArtifactTooLarge`] for one artifact past the
    /// individual bound, or [`CapacityRefusal::ArtifactBytes`] when committed
    /// plus reserved would cross the aggregate.
    pub fn reserve_artifact(
        &self,
        content_digest: Option<&str>,
        byte_length: u64,
    ) -> Result<Option<ArtifactReservation>, AccountingFailure> {
        self.policy.require_artifact_representable(byte_length)?;
        if let Some(digest) = content_digest
            && self.committed_length(digest)?.is_some()
        {
            return Ok(None);
        }
        let committed =
            self.count("measure the bytes this namespace's committed content occupies", &[])?;
        let mut held = self.reservations.lock().map_err(|_| AccountingFailure::LockPoisoned)?;
        let facts = CapacityFacts {
            held: committed
                .saturating_add(held.iter().map(|reservation| reservation.byte_length).sum()),
            limit: self.policy.committed_plus_reserved_artifact_bytes,
            wanted: byte_length,
        };
        if !facts.fits() {
            return Err(CapacityRefusal::ArtifactBytes { facts }.into());
        }
        let reservation = ArtifactReservation { byte_length, ticket: self.claim_ticket() };
        held.push(reservation);
        Ok(Some(reservation))
    }

    /// Releases one reservation without committing it.
    ///
    /// An installation that was abandoned held bytes it never used, and holding
    /// them afterwards would refuse work for space nothing occupies.
    pub fn release(&self, reservation: ArtifactReservation) {
        if let Ok(mut held) = self.reservations.lock() {
            held.retain(|holding| holding.ticket != reservation.ticket);
        }
    }

    /// Converts one reservation into committed usage.
    ///
    /// The caller has already committed the blob row, so the bytes are now
    /// counted by the authoritative table. Dropping the reservation at the same
    /// moment is what makes the total neither double-count them nor lose them.
    pub fn commit(&self, reservation: ArtifactReservation) {
        self.release(reservation);
    }

    /// Returns the length of committed content, when it is committed.
    fn committed_length(&self, content_digest: &str) -> Result<Option<u64>, AccountingFailure> {
        let mut prepared = self
            .database
            .connection()
            .prepare(statement("read one artifact blob's recorded length"))
            .map_err(refused)?;
        let found = prepared
            .query_row(rusqlite::params![content_digest], |row| row.get::<_, i64>(0))
            .map(Some)
            .or_else(|failure| match failure {
                rusqlite::Error::QueryReturnedNoRows => Ok(None),
                other => Err(refused(other)),
            })?;
        found
            .map(|length| u64::try_from(length).map_err(|_| AccountingFailure::NotACount(length)))
            .transpose()
    }

    /// Returns the next reservation's ticket.
    fn claim_ticket(&self) -> u64 {
        self.next_ticket
            .lock()
            .map(|mut next| {
                let ticket = *next;
                *next = next.saturating_add(1);
                ticket
            })
            .unwrap_or_default()
    }
}
