//! Evidence for one physical lookup; never proof that an entire operation is lost.
use crate::selected_author_exchange::SelectedAuthorFiniteResponse;
use slingshot_agent_protocol::physical_job_missing::PhysicalJobMissing;
use slingshot_domain::author_agent_transport_contract::AuthorAgentTransportContract;

/// Complete selected-author absence bound to the exact physical query and the
/// independently observed current generation. Private fields prevent fabrication.
pub struct ValidatedPhysicalJobMissing {
    generation: u64,
    identifier: String,
}
impl core::fmt::Debug for ValidatedPhysicalJobMissing {
    fn fmt(&self, formatter: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        formatter.write_str("ValidatedPhysicalJobMissing([redacted])")
    }
}
impl ValidatedPhysicalJobMissing {
    /// Current store generation that answered the query.
    pub fn generation(&self) -> u64 {
        self.generation
    }
    /// Physical job actually queried and answered missing.
    pub fn sling_job_identifier(&self) -> &str {
        &self.identifier
    }
}
/// Opaque refusal; no untrusted body or physical identifier enters diagnostics.
#[derive(Debug, Clone, Copy, thiserror::Error)]
#[error("the physical job absence is not verified")]
pub struct PhysicalAbsenceRefusal;

/// Requires a complete finite HTTP 404 and exact independently held query echoes.
/// A logical tombstone, transport failure or changed generation is not absence.
pub fn decode_physical_job_missing(
    response: &SelectedAuthorFiniteResponse,
    sling_job_identifier: &str,
    current_generation: u64,
) -> Result<ValidatedPhysicalJobMissing, PhysicalAbsenceRefusal> {
    let contract = AuthorAgentTransportContract::embedded();
    slingshot_domain::remote_job::AgentJobIdentifier::new(sling_job_identifier)
        .map_err(|_| PhysicalAbsenceRefusal)?;
    if current_generation == 0
        || response.status != 404
        || response.head.location.is_some()
        || response.body.len() as u64 > contract.limit("maximum_agent_protocol_document_bytes")
        || !crate::selected_author_submission::json_media_type(
            response.content_type.as_deref().unwrap_or(""),
        )
    {
        return Err(PhysicalAbsenceRefusal);
    }
    response.head.require_acceptable().map_err(|_| PhysicalAbsenceRefusal)?;
    let wire: PhysicalJobMissing =
        serde_json::from_slice(&response.body).map_err(|_| PhysicalAbsenceRefusal)?;
    if wire.format != slingshot_agent_protocol::identity::AGENT_FORMAT
        || wire.kind != "missing"
        || wire.transport_contract_digest != AuthorAgentTransportContract::embedded_digest()
        || wire.agent_event_store_generation != current_generation
        || wire.sling_job_identifier != sling_job_identifier
    {
        return Err(PhysicalAbsenceRefusal);
    }
    Ok(ValidatedPhysicalJobMissing {
        generation: current_generation,
        identifier: wire.sling_job_identifier,
    })
}
