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

use slingshot_agent_protocol::identity::DocumentProvenance;
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

/// Every stage a terminal result passes, in the order it passes them.
///
/// Written down as data so the order a reader sees is the order the code runs.
/// A refusal names the stage it stopped at, which is how a caller learns
/// whether the document was unreadable, unauthenticated, or simply about
/// something else.
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

/// What one artifact echo says about itself.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ArtifactEcho {
    /// How many bytes it holds.
    pub byte_length: u64,
    /// What it is.
    pub media_type: String,
    /// Which declared slot it fills.
    pub slot: String,
    /// What it suggests being called.
    pub suggested_name: String,
}

/// One terminal result document, as the agent sent it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TerminalResultDocument {
    /// The canonical result bytes, exactly as they were validated.
    pub canonical_result: String,
    /// The remote artifacts it says it produced.
    pub declared_artifacts: Vec<ArtifactEcho>,
    /// Which contracts it was produced under.
    pub provenance: DocumentProvenance,
    /// Which submission it ends.
    pub submitted_command_digest: String,
}

/// What this daemon knows about the submission it is expecting a result for.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResultExpectation {
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

/// What believing one result produced.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ValidatedResult {
    /// The canonical bytes, unchanged by having been validated.
    pub canonical_result: String,
    /// The remote artifacts the command declared and the result echoed.
    pub declared_artifacts: Vec<ArtifactEcho>,
    /// Where the result is kept locally.
    pub disposition: LocalDisposition,
}

/// Requires one result to be one this daemon may act on.
///
/// Ordered, and the order is the point: the bound before the parse, the
/// contracts before the schemas, the digest before anything is written down.
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
    if document.submitted_command_digest != expectation.submitted_command_digest {
        return Err(ResultRefusal::AnotherSubmission);
    }
    require_declared_artifacts(&expectation.wire_name, &document.declared_artifacts)?;
    require_one_form(&expectation.wire_name, document)?;
    Ok(ValidatedResult {
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
