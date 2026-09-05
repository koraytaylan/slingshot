//! Closed response from authenticated subscription high-water capture.
use serde::{Deserialize, Serialize};

/// One captured position for the explicitly echoed subscription and generation.
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SubscriptionHighWater {
    /// Closed agent document version.
    pub format: String,
    /// Installed transport contract, not a single command's provenance.
    pub transport_contract_digest: String,
    /// Filtered subscription actually captured.
    pub daemon_subscription_identifier: String,
    /// Generation in which capture occurred.
    pub agent_event_store_generation: u64,
    /// Captured position; reconciliation must still cover it before installation.
    pub high_water_cursor: String,
}

impl core::fmt::Debug for SubscriptionHighWater {
    fn fmt(&self, formatter: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        formatter.write_str("SubscriptionHighWater([redacted])")
    }
}

/// Language-neutral closed schema.
pub const SCHEMA: &str =
    include_str!("../../../schemas/agent-protocol/job/subscription-high-water.json");
