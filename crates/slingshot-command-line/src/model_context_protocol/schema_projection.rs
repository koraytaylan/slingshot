//! Turning one registry command's schemas into a tool's declared schema.
//!
//! Derived from the registry rather than restated, so a tool cannot describe
//! arguments the command does not accept. The projection composes: the
//! command's own argument schema is taken whole and the two members this
//! protocol adds are placed beside it, so a change to the command reaches the
//! tool without anybody editing the tool.
//!
//! # Three checks, in one order, and the first that fails wins
//!
//! Raw bytes first, decoded shape second, typed construction third. The order
//! is the point. A document whose bytes are not canonical is refused before its
//! shape is examined, because parsing and reserializing it into compliance
//! would accept a document the language-neutral validator rejects - and the
//! agent on the other side runs that validator. A shape that passes says
//! nothing about whether the values can be constructed; that is the third
//! check, and it is stronger than a schema can express.
//!
//! # An output schema says what this tool can answer, and no more
//!
//! The envelope vocabulary is closed, and most of its tags are impossible for
//! any given tool. Declaring all of them would tell a client to expect answers
//! it will never receive, and hide the one answer it should have handled.

use serde_json::{Value, json};

use slingshot_domain::command::canonical_json::{CanonicalFailure, require_canonical_bytes};
use slingshot_domain::command::catalog::CommandCatalog;
use slingshot_domain::command::schema::{self, CANONICAL_CONTRACT_ANNOTATION, SchemaRole};

use crate::model_context_protocol::tool_catalog::{KeyPresence, ToolDescriptor};

/// The member a caller supplies to make a rerun the same request.
pub const OPERATION_KEY_MEMBER: &str = "operation_key";

/// The member a caller supplies to return without waiting.
pub const DETACHED_MEMBER: &str = "detached";

/// The fewest bytes an operation key may carry.
pub const LEAST_OPERATION_KEY_BYTES: u64 = 1;

/// The most bytes an operation key may carry.
pub const MOST_OPERATION_KEY_BYTES: u64 = 256;

/// Which check refused one document.
///
/// Recorded rather than collapsed into one failure, because which check said no
/// is what tells a caller whether to fix their serializer, their shape, or
/// their values - three different mistakes with three different fixes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Stage {
    /// The bytes are not the canonical form of any document.
    RawBytes,
    /// The decoded document does not match the declared shape.
    DecodedShape,
    /// The values cannot be constructed into the command they describe.
    TypedConstruction,
}

/// Why one document is not one this tool accepts.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("{stage:?} refused this document: {detail}")]
pub struct ProjectionRefusal {
    /// Which check refused it.
    pub stage: Stage,
    /// What was wrong, in words a caller can act on.
    pub detail: String,
}

impl ProjectionRefusal {
    /// Returns the refusal one check makes.
    fn at(stage: Stage, detail: impl Into<String>) -> Self {
        Self { stage, detail: detail.into() }
    }
}

/// Returns the input schema one tool declares.
///
/// # Errors
///
/// Returns [`ProjectionRefusal`] when the registry publishes no command of that
/// name, which is a defect here rather than a caller's mistake.
pub fn input_schema(tool: &ToolDescriptor) -> Result<Value, ProjectionRefusal> {
    let published = CommandCatalog::published();
    let Some(_descriptor) = published.find(&tool.name) else {
        return control_input_schema(tool);
    };
    let mut projected = schema::command_schema(&tool.name, SchemaRole::Arguments);
    let object = projected.as_object_mut().ok_or_else(|| {
        ProjectionRefusal::at(Stage::DecodedShape, "an argument schema is an object")
    })?;
    let properties =
        object.entry("properties").or_insert_with(|| json!({})).as_object_mut().ok_or_else(
            || ProjectionRefusal::at(Stage::DecodedShape, "properties are an object"),
        )?;
    properties.insert(OPERATION_KEY_MEMBER.to_owned(), operation_key_schema());
    properties.insert(DETACHED_MEMBER.to_owned(), json!({ "type": "boolean" }));
    if tool.operation_key == KeyPresence::Required {
        let required = object.entry("required").or_insert_with(|| json!([]));
        if let Some(members) = required.as_array_mut() {
            members.push(json!(OPERATION_KEY_MEMBER));
        }
    }
    Ok(projected)
}

/// Returns the schema an operation key is bounded by.
fn operation_key_schema() -> Value {
    json!({
        "type": "string",
        "minLength": LEAST_OPERATION_KEY_BYTES,
        "maxLength": MOST_OPERATION_KEY_BYTES,
    })
}

/// Returns the input schema one control declares.
fn control_input_schema(tool: &ToolDescriptor) -> Result<Value, ProjectionRefusal> {
    let mut properties = serde_json::Map::new();
    let mut required = Vec::new();
    for member in CONTROL_MEMBERS
        .iter()
        .filter(|(named, _)| named == &tool.name)
        .flat_map(|(_, members)| members.iter())
    {
        properties.insert((*member).to_owned(), json!({ "type": "string" }));
        required.push(json!(member));
    }
    Ok(json!({
        "$schema": schema::SCHEMA_DIALECT,
        "type": "object",
        "additionalProperties": false,
        "properties": properties,
        "required": required,
        CANONICAL_CONTRACT_ANNOTATION: schema::canonical_contract_digest(),
    }))
}

/// What each control requires a caller to name.
const CONTROL_MEMBERS: &[(&str, &[&str])] = &[
    ("operation-list", &[]),
    ("operation-status", &["operation_identifier"]),
    ("operation-wait", &["operation_identifier"]),
    ("operation-restart", &["operation_identifier", "expected_recovery_category"]),
    ("operation-result", &["operation_identifier"]),
    ("operation-artifact", &["operation_identifier", "artifact_identifier"]),
    ("maintenance-preview", &["author_target_identity_digest"]),
    ("maintenance-apply", &["author_target_identity_digest", "reviewed_manifest_digest"]),
];

/// Returns the outcome tags one tool can answer with.
#[must_use]
pub fn answerable_tags(tool: &ToolDescriptor) -> Vec<&'static str> {
    if CommandCatalog::published().find(&tool.name).is_some() {
        return COMMAND_TAGS.to_vec();
    }
    CONTROL_TAGS
        .iter()
        .find(|(named, _)| *named == tool.name)
        .map(|(_, tags)| tags.to_vec())
        .unwrap_or_default()
}

/// The answers a registry command can produce.
const COMMAND_TAGS: &[&str] = &[
    "operation_receipt",
    "operation_status",
    "operation_result",
    "operation_terminal_error",
    "operation_recovery_required",
    "command_artifact_access",
    "structured_result_artifact_access",
];

/// The answers each control can produce.
const CONTROL_TAGS: &[(&str, &[&str])] = &[
    ("operation-list", &["operation_list_page"]),
    ("operation-status", &["operation_status", "operation_recovery_required"]),
    ("operation-wait", &["operation_status", "operation_result", "operation_terminal_error"]),
    ("operation-restart", &["operation_resume_receipt"]),
    ("operation-result", &["operation_result", "structured_result_artifact_access"]),
    ("operation-artifact", &["command_artifact_access"]),
    ("maintenance-preview", &["maintenance_preview"]),
    ("maintenance-apply", &["maintenance_result_access"]),
];

/// Returns the output schema one tool declares.
#[must_use]
pub fn output_schema(tool: &ToolDescriptor) -> Value {
    json!({
        "$schema": schema::SCHEMA_DIALECT,
        "type": "object",
        "required": ["outcome"],
        "properties": { "outcome": { "enum": answerable_tags(tool) } },
        CANONICAL_CONTRACT_ANNOTATION: schema::canonical_contract_digest(),
    })
}

/// Requires one document to pass every check, in order.
///
/// The raw bytes are kept and validated as they arrived. Parsing and
/// reserializing them into compliance would accept a document the
/// language-neutral validator rejects, and the agent on the other side runs
/// that validator.
///
/// # Errors
///
/// Returns [`ProjectionRefusal`] naming the check that refused it.
pub fn require_acceptable(tool: &ToolDescriptor, raw: &[u8]) -> Result<Value, ProjectionRefusal> {
    let decoded = require_canonical_bytes(raw).map_err(canonical_refusal)?;
    require_declared_shape(tool, &decoded)?;
    Ok(decoded)
}

/// Returns the refusal one canonical failure makes.
fn canonical_refusal(failure: CanonicalFailure) -> ProjectionRefusal {
    ProjectionRefusal::at(Stage::RawBytes, failure.to_string())
}

/// Requires one decoded document to match the shape its tool declares.
fn require_declared_shape(tool: &ToolDescriptor, decoded: &Value) -> Result<(), ProjectionRefusal> {
    let object = decoded.as_object().ok_or_else(|| {
        ProjectionRefusal::at(Stage::DecodedShape, "an argument document is an object")
    })?;
    let key = object.get(OPERATION_KEY_MEMBER).and_then(Value::as_str);
    match (tool.operation_key, key) {
        (KeyPresence::Required, None) => Err(ProjectionRefusal::at(
            Stage::DecodedShape,
            format!("{} requires {OPERATION_KEY_MEMBER}", tool.name),
        )),
        (KeyPresence::Absent, Some(_)) => Err(ProjectionRefusal::at(
            Stage::DecodedShape,
            format!("{} starts no work, so it takes no {OPERATION_KEY_MEMBER}", tool.name),
        )),
        (_, Some(supplied)) => require_bounded_key(supplied),
        (_, None) => Ok(()),
    }
}

/// Requires one supplied operation key to be inside its bounds.
fn require_bounded_key(supplied: &str) -> Result<(), ProjectionRefusal> {
    let held = u64::try_from(supplied.len()).unwrap_or(u64::MAX);
    if !(LEAST_OPERATION_KEY_BYTES..=MOST_OPERATION_KEY_BYTES).contains(&held) {
        return Err(ProjectionRefusal::at(
            Stage::DecodedShape,
            format!(
                "an operation key carries between {LEAST_OPERATION_KEY_BYTES} and \
                 {MOST_OPERATION_KEY_BYTES} bytes, and this carries {held}"
            ),
        ));
    }
    Ok(())
}
