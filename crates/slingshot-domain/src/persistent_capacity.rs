//! What one runtime namespace may keep, and what it does when it cannot keep more.
//!
//! Every bound here comes from the runtime contract. None is redeclared, and
//! none has a fallback: a namespace that could invent its own limit when the
//! manifest did not name one would be a namespace whose behaviour depends on
//! which build it is running, which is exactly what a manifest exists to stop.
//!
//! The one value this module computes rather than reads is the individual
//! artifact bound, and it computes it as a proof rather than a definition. Every
//! command that can produce a remote artifact declares how large that artifact
//! may be, and the daemon declares how large a canonical structured result may
//! be. The manifest's `maximum_individual_artifact_bytes` has to be the largest
//! of those, because a slot a command may fill and the store may not hold is a
//! command that cannot succeed.
//!
//! Reaching a limit is never a reason to delete anything. A namespace at its
//! bound refuses new work and says what the bound was, what is being held
//! against it, and which maintenance would release some. Deleting a terminal
//! row to make space would destroy the record of work that actually happened,
//! and nobody asked for that.

use crate::daemon_runtime_contract::DaemonRuntimeContract;

/// Limit names a namespace is held to, in the order a reader would want them.
pub const NAMESPACE_LIMIT_NAMES: &[&str] = &[
    "maximum_retained_operation_rows",
    "maximum_recovery_resume_receipts_per_operation",
    "maximum_terminal_maintenance_application_receipts_per_target",
    "maximum_committed_plus_reserved_artifact_bytes",
];

/// Formula names a namespace is held to.
pub const NAMESPACE_FORMULA_NAMES: &[&str] = &[
    "maximum_terminal_maintenance_result_associations_per_target",
    "maximum_individual_artifact_bytes",
    "persistent_filesystem_safety_reserve_bytes",
];

/// Command limits naming how large a remote artifact slot may be.
///
/// One entry per command that can produce an artifact rather than an inline
/// result. A command added without its bound appearing here would be a slot
/// nothing proved the store can hold.
pub const REMOTE_ARTIFACT_SLOT_LIMITS: &[&str] =
    &["maximum_package_output_bytes", "maximum_loaded_content_artifact_bytes"];

/// What a namespace is holding against one bound.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CapacityFacts {
    /// How much is held now.
    pub held: u64,
    /// How much may be held.
    pub limit: u64,
    /// How much this request wanted to add.
    pub wanted: u64,
}

impl CapacityFacts {
    /// Returns whether `wanted` more would stay within the bound.
    #[must_use]
    pub fn fits(self) -> bool {
        self.held.checked_add(self.wanted).is_some_and(|total| total <= self.limit)
    }
}

/// What a namespace ran out of, and what would release some.
///
/// The guidance is part of the refusal rather than something a caller has to
/// look up, because a caller that cannot tell which maintenance would help has
/// been told it is stuck rather than told what to do.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum CapacityRefusal {
    /// This namespace holds every operation row it may.
    #[error(
        "this namespace holds {facts:?} operation rows; terminal-operation maintenance releases \
         rows for operations that have already ended"
    )]
    OperationRows {
        /// What is held against the bound.
        facts: CapacityFacts,
    },
    /// This operation holds every resume receipt it may.
    #[error(
        "this operation holds {facts:?} resume receipts; terminal-operation maintenance releases \
         receipts with the operation that owns them"
    )]
    ResumeReceipts {
        /// What is held against the bound.
        facts: CapacityFacts,
    },
    /// This target holds every maintenance-application receipt it may.
    #[error(
        "this target holds {facts:?} maintenance-application receipts; retiring a prior receipt \
         releases one"
    )]
    MaintenanceReceipts {
        /// What is held against the bound.
        facts: CapacityFacts,
    },
    /// This target holds every maintenance-result association it may.
    #[error(
        "this target holds {facts:?} maintenance-result associations; retiring the receipts that \
         own them releases some"
    )]
    MaintenanceAssociations {
        /// What is held against the bound.
        facts: CapacityFacts,
    },
    /// This namespace has committed and reserved every artifact byte it may.
    #[error(
        "this namespace holds {facts:?} artifact bytes; terminal-operation maintenance releases \
         the artifacts of operations that have already ended"
    )]
    ArtifactBytes {
        /// What is held against the bound.
        facts: CapacityFacts,
    },
    /// One artifact is larger than any artifact may be.
    #[error("one artifact holds at most {limit} bytes, and this one wants {wanted}")]
    ArtifactTooLarge {
        /// How large one may be.
        limit: u64,
        /// How large this one is.
        wanted: u64,
    },
}

/// Reason the manifest cannot be the source of a namespace's bounds.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum PolicyFailure {
    /// A declared slot is larger than any artifact this daemon can hold.
    #[error("{slot} declares {declared} bytes, past the {allowed} an artifact may hold")]
    SlotUnrepresentable {
        /// How large the slot may be.
        declared: u64,
        /// How large an artifact may be.
        allowed: u64,
        /// Which command limit declares it.
        slot: String,
    },
    /// The artifact bound is not the largest thing that has to fit in it.
    #[error("the artifact bound is {declared}, and the largest declared value is {largest}")]
    ArtifactBoundNotTheLargest {
        /// What the manifest says.
        declared: u64,
        /// What the declarations require.
        largest: u64,
    },
}

/// The bounds one runtime namespace is held to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PersistentCapacityPolicy {
    /// Bytes committed plus reserved artifacts may occupy together.
    pub committed_plus_reserved_artifact_bytes: u64,
    /// Bytes one artifact may occupy.
    pub individual_artifact_bytes: u64,
    /// Maintenance-application receipts one target may hold.
    pub maintenance_application_receipts_per_target: u64,
    /// Maintenance-result associations one target may hold.
    pub maintenance_result_associations_per_target: u64,
    /// Operation rows this namespace may hold.
    pub retained_operation_rows: u64,
    /// Resume receipts one operation may hold.
    pub recovery_resume_receipts_per_operation: u64,
}

impl PersistentCapacityPolicy {
    /// Returns the bounds the embedded runtime contract names.
    #[must_use]
    pub fn embedded() -> Self {
        let contract = DaemonRuntimeContract::embedded();
        Self {
            committed_plus_reserved_artifact_bytes: contract
                .limit("maximum_committed_plus_reserved_artifact_bytes"),
            individual_artifact_bytes: contract.formula("maximum_individual_artifact_bytes"),
            maintenance_application_receipts_per_target: contract
                .limit("maximum_terminal_maintenance_application_receipts_per_target"),
            maintenance_result_associations_per_target: contract
                .formula("maximum_terminal_maintenance_result_associations_per_target"),
            retained_operation_rows: contract.limit("maximum_retained_operation_rows"),
            recovery_resume_receipts_per_operation: contract
                .limit("maximum_recovery_resume_receipts_per_operation"),
        }
    }

    /// Requires every declared artifact producer to fit the artifact bound.
    ///
    /// The bound has to be exactly the largest declared value rather than
    /// merely at least it. Larger would be a promise the manifest makes and
    /// nothing needs; smaller would be a command whose declared result cannot
    /// be stored, which is a command that cannot succeed.
    ///
    /// # Errors
    ///
    /// Returns [`PolicyFailure::SlotUnrepresentable`] for a declared slot the
    /// store could not hold, or [`PolicyFailure::ArtifactBoundNotTheLargest`]
    /// when the bound is not the largest declared value.
    pub fn require_artifact_bound_covers(
        &self,
        declared_slots: &[(&str, u64)],
        canonical_structured_result_bytes: u64,
    ) -> Result<(), PolicyFailure> {
        let mut largest = canonical_structured_result_bytes;
        for (slot, declared) in declared_slots {
            if *declared > self.individual_artifact_bytes {
                return Err(PolicyFailure::SlotUnrepresentable {
                    declared: *declared,
                    allowed: self.individual_artifact_bytes,
                    slot: (*slot).to_owned(),
                });
            }
            largest = largest.max(*declared);
        }
        if largest != self.individual_artifact_bytes {
            return Err(PolicyFailure::ArtifactBoundNotTheLargest {
                declared: self.individual_artifact_bytes,
                largest,
            });
        }
        Ok(())
    }

    /// Returns whether one artifact of `wanted` bytes may exist at all.
    ///
    /// # Errors
    ///
    /// Returns [`CapacityRefusal::ArtifactTooLarge`].
    pub fn require_artifact_representable(&self, wanted: u64) -> Result<(), CapacityRefusal> {
        if wanted > self.individual_artifact_bytes {
            return Err(CapacityRefusal::ArtifactTooLarge {
                limit: self.individual_artifact_bytes,
                wanted,
            });
        }
        Ok(())
    }
}
