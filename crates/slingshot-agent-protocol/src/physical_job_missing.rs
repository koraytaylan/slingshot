//! Closed absence of a queried physical Sling job, not logical-operation loss.
/// The physical route cannot invent command provenance for an unknown job.
#[derive(Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PhysicalJobMissing {
    /// Closed agent document version.
    pub format: String,
    /// Installed transport contract under which the lookup was answered.
    pub transport_contract_digest: String,
    /// Literal missing discriminator; the decoder enforces its value.
    pub kind: String,
    /// Current store generation independently observed by recovery.
    pub agent_event_store_generation: u64,
    /// Exact decoded physical identifier queried, not an invented logical ID.
    pub sling_job_identifier: String,
}
impl core::fmt::Debug for PhysicalJobMissing {
    fn fmt(&self, formatter: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        formatter.write_str("PhysicalJobMissing([redacted])")
    }
}
/// Closed language-neutral schema.
pub const SCHEMA: &str =
    include_str!("../../../schemas/agent-protocol/job/physical-job-missing.json");
