//! Closed subscription reset response; no command provenance is invented for a
//! subscription spanning multiple independently retained commands.
use serde::{Deserialize, Serialize};

/// The two reset conditions a remote response may establish.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ResetReason {
    /// The requested store generation is no longer current.
    GenerationChanged,
    /// The supplied cursor expired within the same generation.
    CursorExpired,
}

/// Explicit request echoes and one captured current-generation position.
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EventStreamResetRequired {
    /// Closed agent document version.
    pub format: String,
    /// Installed transport contract, independent of any command contract.
    pub transport_contract_digest: String,
    /// The subscription actually requested.
    pub daemon_subscription_identifier: String,
    /// The generation supplied on the request.
    pub requested_agent_event_store_generation: u64,
    /// Exact Last-Event-ID, or null when no cursor was supplied.
    #[serde(deserialize_with = "required_cursor")]
    pub requested_last_event_identifier: Option<String>,
    /// The current generation whose high-water position was captured.
    pub agent_event_store_generation: u64,
    /// Captured position, not permission to install or resume it yet.
    pub high_water_cursor: String,
    /// Required status/generation/cursor relationship.
    pub reason: ResetReason,
}

fn required_cursor<'de, D: serde::Deserializer<'de>>(
    deserializer: D,
) -> Result<Option<String>, D::Error> {
    Option::<String>::deserialize(deserializer)
}

impl core::fmt::Debug for EventStreamResetRequired {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str("EventStreamResetRequired([redacted])")
    }
}

/// Language-neutral closed schema.
pub const SCHEMA: &str =
    include_str!("../../../schemas/agent-protocol/job/event-stream-reset.json");
