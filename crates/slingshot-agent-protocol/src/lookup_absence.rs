//! Closed lookup responses that contain no active snapshot.

use crate::identity::DocumentProvenance;
use serde::{Deserialize, Serialize};

/// Missing echoes request context, not invented command provenance. Retired
/// echoes the complete retained tombstone provenance before it can end recovery.
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum LookupAbsence {
    /// HTTP 404: no mapping exists in the named generation and partition.
    Missing {
        /// The closed agent document version.
        format: String,
        /// The transport contract used to perform this lookup.
        transport_contract_digest: String,
        /// The generation actually consulted.
        agent_event_store_generation: u64,
        /// The requested logical operation.
        agent_operation_identifier: String,
        /// The author partition actually consulted.
        author_target_identity_digest: String,
    },
    /// HTTP 410: the same submission existed and its recovery window expired.
    Retired {
        /// Versioned, exact command and transport provenance from the tombstone.
        provenance: DocumentProvenance,
        /// The retained generation.
        agent_event_store_generation: u64,
        /// The retained logical operation.
        agent_operation_identifier: String,
        /// The retained author partition.
        author_target_identity_digest: String,
        /// The retained subscription.
        daemon_subscription_identifier: String,
        /// The retained selected revision.
        selected_environment_revision: String,
        /// The retained command binding.
        submitted_command_digest: String,
    },
}

impl core::fmt::Debug for LookupAbsence {
    fn fmt(&self, formatter: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        formatter.write_str("LookupAbsence([redacted])")
    }
}
