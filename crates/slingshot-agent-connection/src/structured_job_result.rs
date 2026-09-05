//! Believing a result, in an order that cannot be shortened.
//!
//! A terminal result is the one document that turns remote work into local
//! truth, so every check it passes happens before the next one and none of them
//! is skippable. The order is not stylistic: a bound applied after
//! deserialization is a bound on memory already spent, a schema applied before
//! the byte contract is a schema applied to bytes nobody agreed were canonical,
//! and a correlation checked after persistence is a correlation checked too
//! late.
//!
//! # The check a schema cannot make
//!
//! A result produced by the same command with different arguments satisfies the
//! variant, the shape, and every echoed fact the domain can compare. It is
//! wrong anyway, and the only thing that says so is the submitted digest. So
//! the digest is checked, and it is checked before anything is written down.
//!
//! # Inline and artifact are alternatives, never both
//!
//! A result carrying data twice is a result whose two copies can disagree. The
//! sizes at which each is permitted come from the command contract rather than
//! from the transport: the general transport ceiling is a ceiling on what may
//! travel, not a licence for a load result to travel inline past the size its
//! own contract allows.

pub use slingshot_agent_protocol::terminal_result::{ArtifactEcho, TerminalResultDocument};
use slingshot_agent_protocol::wire_contract::{ExpectedProvenance, WireRefusal};
use slingshot_domain::author_agent_transport_contract::AuthorAgentTransportContract;
use slingshot_domain::command::artifact::{
    ArtifactRequirement, ArtifactSlotDeclaration, CONTENT_PACKAGE_MEDIA_TYPE, CONTENT_PACKAGE_SLOT,
    LOADED_CONTENT_MEDIA_TYPE, LOADED_CONTENT_SLOT,
};
use slingshot_domain::command::catalog::CommandCatalog;
use slingshot_domain::command::load_content_as_javascript_object_notation::maximum_agent_inline_loaded_document_bytes;
use slingshot_domain::daemon_runtime_contract::DaemonRuntimeContract;

/// The slot a locally externalized result is stored in.
pub const STRUCTURED_RESULT_SLOT: &str = "structured_result";

/// The media type a locally externalized result is stored as.
pub const STRUCTURED_RESULT_MEDIA_TYPE: &str = "application/json";

/// The required stages of complete terminal-result validation.
///
/// This inventory is not proof that a helper performs every stage. The wire
/// decoder and metadata checker below each document their narrower guarantees.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ValidationStage {
    /// The document is no larger than a document may be.
    TransportBound,
    /// It names the transport contract and the canonical byte contract.
    ContractDigests,
    /// Both schema roles carry the annotations this build authenticates.
    RoleAnnotations,
    /// The five-field contract identity is the installed one, unchanged.
    ContractIdentity,
    /// It ends the submission this daemon actually made.
    SubmittedDigest,
    /// Its raw bytes are canonical under the byte contract.
    RawCanonicalBytes,
    /// Its decoded shape satisfies the result schema.
    DecodedShape,
    /// It converts into the typed result this command answers with.
    TypedConversion,
    /// The typed result answers the request that was persisted.
    RequestCorrelation,
}

/// The stages, in order.
pub const STAGE_ORDER: &[ValidationStage] = &[
    ValidationStage::TransportBound,
    ValidationStage::ContractDigests,
    ValidationStage::RoleAnnotations,
    ValidationStage::ContractIdentity,
    ValidationStage::SubmittedDigest,
    ValidationStage::RawCanonicalBytes,
    ValidationStage::DecodedShape,
    ValidationStage::TypedConversion,
    ValidationStage::RequestCorrelation,
];

/// A wire document could not pass bounded decoding and canonical-byte checks.
/// No remote payload or parser detail escapes through this diagnostic.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[error("the terminal result document is not acceptable")]
pub struct TerminalResultDecodeRefusal;

/// Decodes the closed result envelope, checks exact expected provenance and
/// submitted digest, then verifies raw canonical result bytes. This returns a
/// document, not a validated command result: schema, ordered-array, typed
/// request correlation and artifact-identity checks remain the caller's gates.
pub fn decode_terminal_result(
    body: &[u8],
    expectation: &ResultExpectation,
) -> Result<TerminalResultDocument, TerminalResultDecodeRefusal> {
    if body.len() as u64 > maximum_document_bytes() {
        return Err(TerminalResultDecodeRefusal);
    }
    let document: TerminalResultDocument =
        serde_json::from_slice(body).map_err(|_| TerminalResultDecodeRefusal)?;
    expectation
        .expected_provenance
        .require_matching(&document.provenance)
        .map_err(|_| TerminalResultDecodeRefusal)?;
    if document.operation != expectation.operation
        || document.daemon_subscription_identifier != expectation.daemon_subscription_identifier
        || document.submitted_command_digest != expectation.submitted_command_digest
        || expectation.wire_name != document.provenance.command_contract.command_wire_name
        || document.canonical_result.len() as u64 > maximum_agent_inline_result_bytes()
    {
        return Err(TerminalResultDecodeRefusal);
    }
    slingshot_domain::command::canonical_json::require_canonical_bytes(
        document.canonical_result.as_bytes(),
    )
    .map_err(|_| TerminalResultDecodeRefusal)?;
    require_envelope_schema(&document)?;
    Ok(document)
}

/// Checks the published envelope with only embedded reference resources.
/// Serde runs first to reject duplicate keys; the outer byte bound precedes both.
fn require_envelope_schema(
    document: &TerminalResultDocument,
) -> Result<(), TerminalResultDecodeRefusal> {
    static VALIDATOR: std::sync::OnceLock<Result<jsonschema::Validator, ()>> =
        std::sync::OnceLock::new();
    let validator = VALIDATOR.get_or_init(|| {
        let mut options = jsonschema::options().with_draft(jsonschema::Draft::Draft202012);
        for source in [
            include_str!("../../../schemas/agent-protocol/identity/operation.json"),
            include_str!("../../../schemas/agent-protocol/identity/command-contract.json"),
            include_str!("../../../schemas/agent-protocol/common/provenance.json"),
        ] {
            let schema: serde_json::Value = serde_json::from_str(source).map_err(|_| ())?;
            let uri = schema["$id"].as_str().ok_or(())?.to_owned();
            options = options
                .with_resource(uri, jsonschema::Resource::from_contents(schema).map_err(|_| ())?);
        }
        let schema = serde_json::from_str(slingshot_agent_protocol::terminal_result::SCHEMA)
            .map_err(|_| ())?;
        options.build(&schema).map_err(|_| ())
    });
    let validator = validator.as_ref().map_err(|_| TerminalResultDecodeRefusal)?;
    let value = serde_json::to_value(document).map_err(|_| TerminalResultDecodeRefusal)?;
    if !validator.is_valid(&value) {
        return Err(TerminalResultDecodeRefusal);
    }
    Ok(())
}

/// Decodes a result and checks the installed schema, canonical array ordering,
/// typed conversion and correlation with the retained command. The caller must
/// supply the recomputed retained submission digest in `expectation`.
///
/// Artifact identifiers/content digests and durable operation/revision guards
/// remain separate prerequisites before local settlement.
///
/// # Errors
///
/// Returns an opaque refusal at the first failed gate, without payload details.
pub fn decode_result_for_command(
    body: &[u8],
    expectation: &ResultExpectation,
    command: &slingshot_domain::command::catalog::Command,
) -> Result<ValidatedResult, TerminalResultDecodeRefusal> {
    use slingshot_domain::command::canonical_json::{ArrayOrderInventory, require_array_order};
    use slingshot_domain::command::catalog::{CommandResult, validate_result_for_command};
    use slingshot_domain::command::schema::{
        SchemaRole, canonical_contract_digest, command_schema,
    };
    use slingshot_domain::selected_command_contract_identity::SelectedCommandContractIdentity;

    if body.len() as u64 > maximum_document_bytes() {
        return Err(TerminalResultDecodeRefusal);
    }
    let installed = SelectedCommandContractIdentity::installed(command.wire_name())
        .map_err(|_| TerminalResultDecodeRefusal)?;
    if expectation.wire_name != command.wire_name()
        || expectation.expected_provenance.command_contract != installed
        || expectation.expected_provenance.canonical_json_contract_digest
            != canonical_contract_digest()
        || expectation.expected_provenance.transport_contract_digest
            != AuthorAgentTransportContract::embedded_digest()
    {
        return Err(TerminalResultDecodeRefusal);
    }
    let document = decode_terminal_result(body, expectation)?;
    let catalog = CommandCatalog::published();
    let descriptor = catalog.find(command.wire_name()).ok_or(TerminalResultDecodeRefusal)?;
    if document.canonical_result.len() as u64 > descriptor.maximum_result_bytes {
        return Err(TerminalResultDecodeRefusal);
    }
    let mut value: serde_json::Value = serde_json::from_str(&document.canonical_result)
        .map_err(|_| TerminalResultDecodeRefusal)?;
    // The inventory is the same embedded artifact authenticated by the installed
    // canonical contract digest; callers cannot substitute a permissive inventory.
    let contract: serde_json::Value =
        serde_json::from_str(include_str!("../../../schemas/command-canonical-json-1.json"))
            .map_err(|_| TerminalResultDecodeRefusal)?;
    let pointers =
        serde_json::from_value(contract["arrays"][command.wire_name()]["result"].clone())
            .map_err(|_| TerminalResultDecodeRefusal)?;
    let inventory = ArrayOrderInventory::new(pointers).map_err(|_| TerminalResultDecodeRefusal)?;
    require_array_order(&value, &inventory).map_err(|_| TerminalResultDecodeRefusal)?;
    let schema = command_schema(command.wire_name(), SchemaRole::Result);
    let validator =
        jsonschema::draft202012::new(&schema).map_err(|_| TerminalResultDecodeRefusal)?;
    if !validator.is_valid(&value) {
        return Err(TerminalResultDecodeRefusal);
    }
    // Role schemas carry no catalog discriminator. Add it only after validating
    // the closed remote shape, so an attacker-supplied tag cannot be overwritten.
    value
        .as_object_mut()
        .ok_or(TerminalResultDecodeRefusal)?
        .insert("command".to_owned(), command.wire_name().into());
    let result: CommandResult =
        serde_json::from_value(value).map_err(|_| TerminalResultDecodeRefusal)?;
    validate_result_for_command(command, &result).map_err(|_| TerminalResultDecodeRefusal)?;
    use slingshot_domain::command::load_content_as_javascript_object_notation::LoadContentAsJavaScriptObjectNotationResult;
    let artifact = match &result {
        CommandResult::DownloadContentPackage(result) => Some(&result.artifact),
        CommandResult::LoadContentAsJson(
            LoadContentAsJavaScriptObjectNotationResult::Artifact { artifact, .. },
        ) => Some(artifact),
        _ => None,
    };
    // The typed result owns the inline/artifact alternative and its logical
    // content bound. The descriptor envelope's size is not the loaded content's
    // size, and cannot decide that alternative.
    let expected_echoes: Vec<ArtifactEcho> = artifact
        .into_iter()
        .map(|artifact| ArtifactEcho {
            byte_length: artifact.byte_length,
            media_type: artifact.media_type.as_text().to_owned(),
            slot: artifact.slot.as_text().to_owned(),
            suggested_name: artifact.suggested_file_name.as_text().to_owned(),
        })
        .collect();
    if document.declared_artifacts != expected_echoes {
        return Err(TerminalResultDecodeRefusal);
    }
    Ok(ValidatedResult {
        remote_artifact: artifact.cloned(),
        disposition: local_disposition(document.canonical_result.len() as u64)
            .map_err(|_| TerminalResultDecodeRefusal)?,
        canonical_result: document.canonical_result,
        declared_artifacts: document.declared_artifacts,
    })
}

/// What this daemon knows about the submission it is expecting a result for.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResultExpectation {
    /// Independently retained operation identity, not supplied by the response.
    pub operation: slingshot_agent_protocol::identity::WireOperationIdentity,
    /// Independently retained subscription.
    pub daemon_subscription_identifier: String,
    /// Which contracts this build has.
    pub expected_provenance: ExpectedProvenance,
    /// Which submission the result must end.
    pub submitted_command_digest: String,
    /// Which command it answers.
    pub wire_name: String,
}

/// Why one result cannot be believed.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ResultRefusal {
    /// The document is larger than one document may be.
    #[error("a result document holds at most {allowed} bytes, and this holds {actual}")]
    TooLarge {
        /// How large one may be.
        allowed: u64,
        /// How large this is.
        actual: u64,
    },
    /// It names contracts this build does not have.
    #[error(transparent)]
    Provenance(#[from] WireRefusal),
    /// It ends a submission this daemon did not make.
    #[error("this result ends a submission this daemon did not make")]
    AnotherSubmission,
    /// It carries data twice, and the two copies can disagree.
    #[error("a result carries its data inline or as an artifact, and this carries both")]
    BothForms,
    /// It fills a slot the command never declared.
    #[error("{command} declares no {slot}, so a result filling it answers another command")]
    UndeclaredSlot {
        /// Which command was asked.
        command: String,
        /// Which slot the result filled.
        slot: String,
    },
    /// It fills one declared slot twice.
    #[error("one result fills each declared slot once, and this filled {slot} again")]
    DuplicateSlot {
        /// Which slot it filled twice.
        slot: String,
    },
    /// It omits a slot the command requires.
    #[error("{command} requires {slot}, and this result omits it")]
    RequiredSlotOmitted {
        /// Which command was asked.
        command: String,
        /// Which slot is missing.
        slot: String,
    },
    /// One echo does not match what the command declared.
    #[error("the {slot} echo is not what {command} declares one looks like")]
    EchoDrifted {
        /// Which command was asked.
        command: String,
        /// Which slot drifted.
        slot: String,
    },
    /// A load result travelled inline past the size its own contract allows.
    #[error("a loaded document travels inline through {allowed} bytes, and this holds {actual}")]
    InlineLoadTooLarge {
        /// How large an inline loaded document may be.
        allowed: u64,
        /// How large this is.
        actual: u64,
    },
}

/// Where the validated result is kept locally.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LocalDisposition {
    /// Small enough to travel in the local response itself.
    Inline,
    /// Kept beside the operation, in one stable slot, as its own bytes.
    LocalArtifact {
        /// What it is stored as.
        media_type: &'static str,
        /// Which slot it is stored in.
        slot: &'static str,
    },
}

/// Returns how large a result document may be at all.
#[must_use]
pub fn maximum_document_bytes() -> u64 {
    AuthorAgentTransportContract::embedded().limit("maximum_agent_protocol_document_bytes")
}

/// Returns how large an inline result may be on the wire.
#[must_use]
pub fn maximum_agent_inline_result_bytes() -> u64 {
    AuthorAgentTransportContract::embedded().limit("maximum_agent_inline_result_bytes")
}

/// Returns how large a result may be before it is kept beside the operation.
#[must_use]
pub fn maximum_inline_machine_result_bytes() -> u64 {
    DaemonRuntimeContract::embedded().limit("maximum_inline_machine_result_bytes")
}

/// Returns where a validated result of `canonical_bytes` is kept.
///
/// The machine bound is about what a local caller can be handed in one
/// response, and the transport bound is about what may arrive at all. A result
/// between them is kept beside the operation as its own bytes rather than
/// refused, because it is a perfectly good result that is merely large.
///
/// # Errors
///
/// Returns [`ResultRefusal::TooLarge`] past the transport bound.
pub fn local_disposition(canonical_bytes: u64) -> Result<LocalDisposition, ResultRefusal> {
    let allowed = maximum_agent_inline_result_bytes();
    if canonical_bytes > allowed {
        return Err(ResultRefusal::TooLarge { allowed, actual: canonical_bytes });
    }
    if canonical_bytes <= maximum_inline_machine_result_bytes() {
        return Ok(LocalDisposition::Inline);
    }
    Ok(LocalDisposition::LocalArtifact {
        media_type: STRUCTURED_RESULT_MEDIA_TYPE,
        slot: STRUCTURED_RESULT_SLOT,
    })
}

/// Result metadata checked by [`require_valid`], not proof of schema or request
/// correlation validation.
#[derive(Clone, PartialEq, Eq)]
pub struct ValidatedResult {
    /// Typed descriptor retained by the command-bound decoder for subsequent
    /// deterministic identity and transfer checks. The legacy metadata-only
    /// checker does not produce this evidence.
    pub remote_artifact: Option<slingshot_domain::command::artifact::ArtifactDescriptor>,
    /// The canonical bytes, unchanged by having been validated.
    pub canonical_result: String,
    /// The remote artifacts the command declared and the result echoed.
    pub declared_artifacts: Vec<ArtifactEcho>,
    /// Where the result is kept locally.
    pub disposition: LocalDisposition,
}

impl core::fmt::Debug for ValidatedResult {
    fn fmt(&self, formatter: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        formatter.write_str("ValidatedResult([redacted])")
    }
}

/// Checks provenance, submission digest, artifact metadata and storage size.
///
/// This legacy helper does not parse the canonical result or check its schema,
/// ordered arrays, typed result or request correlation. Its return value alone
/// must not authorize successful local settlement.
///
/// # Errors
///
/// Returns [`ResultRefusal`] naming the first thing that is wrong, all of which
/// leave every snapshot, state, result, and artifact fact untouched.
pub fn require_valid(
    expectation: &ResultExpectation,
    document: &TerminalResultDocument,
) -> Result<ValidatedResult, ResultRefusal> {
    let document_bytes = u64::try_from(document.canonical_result.len()).unwrap_or(u64::MAX);
    let allowed = maximum_document_bytes();
    if document_bytes > allowed {
        return Err(ResultRefusal::TooLarge { allowed, actual: document_bytes });
    }
    expectation.expected_provenance.require_matching(&document.provenance)?;
    if document.operation != expectation.operation
        || document.daemon_subscription_identifier != expectation.daemon_subscription_identifier
        || document.submitted_command_digest != expectation.submitted_command_digest
    {
        return Err(ResultRefusal::AnotherSubmission);
    }
    require_declared_artifacts(&expectation.wire_name, &document.declared_artifacts)?;
    require_one_form(&expectation.wire_name, document)?;
    Ok(ValidatedResult {
        remote_artifact: None,
        canonical_result: document.canonical_result.clone(),
        declared_artifacts: document.declared_artifacts.clone(),
        disposition: local_disposition(document_bytes)?,
    })
}

/// Returns the artifact slots `wire_name` declares.
#[must_use]
pub fn declared_slots(wire_name: &str) -> Vec<ArtifactSlotDeclaration> {
    CommandCatalog::published()
        .find(wire_name)
        .map(|descriptor| descriptor.remote_artifact_slots.clone())
        .unwrap_or_default()
}

/// Requires every echoed artifact to be one the command declared.
///
/// Both directions. An echo for a slot the command never declared is a result
/// answering something else, and an omitted required slot is a result that did
/// not do what it says it did.
///
/// # Errors
///
/// Returns [`ResultRefusal::UndeclaredSlot`], [`ResultRefusal::DuplicateSlot`],
/// [`ResultRefusal::EchoDrifted`], or [`ResultRefusal::RequiredSlotOmitted`].
pub fn require_declared_artifacts(
    wire_name: &str,
    echoes: &[ArtifactEcho],
) -> Result<(), ResultRefusal> {
    let declared = declared_slots(wire_name);
    for (position, echo) in echoes.iter().enumerate() {
        let Some(slot) = declared.iter().find(|slot| slot.slot.as_text() == echo.slot) else {
            return Err(ResultRefusal::UndeclaredSlot {
                command: wire_name.to_owned(),
                slot: echo.slot.clone(),
            });
        };
        if echoes.iter().take(position).any(|earlier| earlier.slot == echo.slot) {
            return Err(ResultRefusal::DuplicateSlot { slot: echo.slot.clone() });
        }
        require_echo_matches(wire_name, slot, echo)?;
    }
    for slot in &declared {
        let filled = echoes.iter().any(|echo| echo.slot == slot.slot.as_text());
        if matches!(slot.requirement, ArtifactRequirement::Required) && !filled {
            return Err(ResultRefusal::RequiredSlotOmitted {
                command: wire_name.to_owned(),
                slot: slot.slot.as_text().to_owned(),
            });
        }
    }
    Ok(())
}

/// Requires one echo to be what its declaration says one looks like.
fn require_echo_matches(
    wire_name: &str,
    declaration: &ArtifactSlotDeclaration,
    echo: &ArtifactEcho,
) -> Result<(), ResultRefusal> {
    let drifted =
        || ResultRefusal::EchoDrifted { command: wire_name.to_owned(), slot: echo.slot.clone() };
    if echo.media_type != declaration.media_type.as_text() {
        return Err(drifted());
    }
    if echo.byte_length > declaration.maximum_byte_length || echo.byte_length == 0 {
        return Err(drifted());
    }
    if echo.suggested_name.is_empty() || echo.suggested_name.contains('/') {
        return Err(drifted());
    }
    Ok(())
}

/// Requires a result to carry its data once.
///
/// A loaded document is the case with two legal forms, and the size decides
/// which. Inline through the command contract's own bound and its declared
/// alternative above it: the general transport ceiling governs what may travel
/// at all, and does not widen the inline form of a command that says otherwise.
fn require_one_form(
    wire_name: &str,
    document: &TerminalResultDocument,
) -> Result<(), ResultRefusal> {
    let loaded = document.declared_artifacts.iter().find(|echo| echo.slot == LOADED_CONTENT_SLOT);
    let inline_bytes = u64::try_from(document.canonical_result.len()).unwrap_or(u64::MAX);
    if wire_name != loading_command() {
        return Ok(());
    }
    let allowed = maximum_agent_inline_loaded_document_bytes();
    match loaded {
        Some(echo) => {
            if echo.media_type != LOADED_CONTENT_MEDIA_TYPE {
                return Err(ResultRefusal::EchoDrifted {
                    command: wire_name.to_owned(),
                    slot: echo.slot.clone(),
                });
            }
            if inline_bytes > allowed {
                return Ok(());
            }
            Err(ResultRefusal::BothForms)
        }
        None if inline_bytes > allowed => {
            Err(ResultRefusal::InlineLoadTooLarge { allowed, actual: inline_bytes })
        }
        None => Ok(()),
    }
}

/// Returns the command whose result has two legal forms.
#[must_use]
pub fn loading_command() -> &'static str {
    "load_content_as_json"
}

/// Returns the command whose result requires an artifact.
#[must_use]
pub fn packaging_command() -> &'static str {
    "download_content_package"
}

/// Returns the echo a package result must produce.
///
/// Stated so a test can compare against the declaration rather than against a
/// literal, which is what keeps the two from drifting apart quietly.
#[must_use]
pub fn package_slot_and_media_type() -> (&'static str, &'static str) {
    (CONTENT_PACKAGE_SLOT, CONTENT_PACKAGE_MEDIA_TYPE)
}

/// How one configuration value was classified before it was read.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Classification {
    /// The metatype says this is a password.
    Password,
    /// The metatype says this is ordinary.
    NonPassword,
    /// There is no metatype evidence at all.
    Unavailable,
}

impl Classification {
    /// Returns whether a value of this classification may be read.
    ///
    /// Only an ordinary one. A password is redacted without being read, and an
    /// unclassified value is treated as a password: the absence of evidence
    /// that something is safe is not evidence that it is.
    #[must_use]
    pub fn permits_value_access(self) -> bool {
        matches!(self, Self::NonPassword)
    }
}

/// One step of reading a configuration dictionary.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DictionaryStep {
    /// The complete key inventory was taken.
    KeyInventory {
        /// Every key the snapshot holds, in order.
        keys: Vec<String>,
    },
    /// Every key was classified and its redaction planned.
    RedactionPlanned,
    /// One key's value was read.
    ValueAccess {
        /// Which key was read.
        key: String,
    },
}

/// Why one dictionary observation is discarded.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum TraceRefusal {
    /// The inventory was not taken first.
    #[error("a complete key inventory comes before anything else is done with a dictionary")]
    InventoryNotFirst,
    /// A value was read before redaction was planned.
    #[error("reading {key} before redaction was planned reads a value nobody classified")]
    ValueReadBeforePlanning {
        /// Which key was read too early.
        key: String,
    },
    /// A value was read that the classification forbids.
    #[error("{key} is classified as one this daemon does not read")]
    ForbiddenValueRead {
        /// Which key was read.
        key: String,
    },
    /// A value was read twice.
    #[error("one visible key is read once, and {key} was read again")]
    RepeatedValueRead {
        /// Which key was read twice.
        key: String,
    },
    /// A key was read that the inventory never named.
    #[error("{key} is not in the inventory this observation was planned from")]
    KeyNotInventoried {
        /// Which key was read.
        key: String,
    },
    /// The inventory names one key twice.
    #[error("a key inventory names each key once, and this names {key} twice")]
    DuplicateInventoryKey {
        /// Which key appears twice.
        key: String,
    },
}

/// Requires one dictionary observation to have been taken the only legal way.
///
/// The inventory first, the classification next, and only then one read for
/// each key that classification permits. The order is what keeps a password out
/// of the process: a value read before it is classified has already been read
/// by the time anybody decides it should not have been.
///
/// # Errors
///
/// Returns [`TraceRefusal`] naming the first thing done out of order, which
/// discards the whole observation rather than the offending step.
pub fn require_two_phase_access(
    steps: &[DictionaryStep],
    classification_of: &dyn Fn(&str) -> Classification,
) -> Result<(), TraceRefusal> {
    let Some(DictionaryStep::KeyInventory { keys }) = steps.first() else {
        return Err(TraceRefusal::InventoryNotFirst);
    };
    for (position, key) in keys.iter().enumerate() {
        if keys.iter().take(position).any(|earlier| earlier == key) {
            return Err(TraceRefusal::DuplicateInventoryKey { key: key.clone() });
        }
    }
    let mut planned = false;
    let mut read: Vec<&str> = Vec::new();
    for step in steps.iter().skip(1) {
        match step {
            DictionaryStep::KeyInventory { .. } => return Err(TraceRefusal::InventoryNotFirst),
            DictionaryStep::RedactionPlanned => planned = true,
            DictionaryStep::ValueAccess { key } => {
                require_readable(key, keys, planned, &read, classification_of)?;
                read.push(key);
            }
        }
    }
    Ok(())
}

/// Requires one value read to be one this observation planned for.
fn require_readable(
    key: &str,
    keys: &[String],
    planned: bool,
    read: &[&str],
    classification_of: &dyn Fn(&str) -> Classification,
) -> Result<(), TraceRefusal> {
    if !planned {
        return Err(TraceRefusal::ValueReadBeforePlanning { key: key.to_owned() });
    }
    if !keys.iter().any(|inventoried| inventoried == key) {
        return Err(TraceRefusal::KeyNotInventoried { key: key.to_owned() });
    }
    if read.contains(&key) {
        return Err(TraceRefusal::RepeatedValueRead { key: key.to_owned() });
    }
    if !classification_of(key).permits_value_access() {
        return Err(TraceRefusal::ForbiddenValueRead { key: key.to_owned() });
    }
    Ok(())
}
