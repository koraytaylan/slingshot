//! Which tools this server offers, derived from the command registry.
//!
//! One tool per published command, named and described from the registry
//! itself, so a command that exists is a tool that exists and a command that
//! changes is a tool that changes with it. A hand-written list beside the
//! registry would be a second inventory to keep in step, and the first thing to
//! drift would be exactly the part a client relies on: whether a tool is safe
//! to call twice.
//!
//! # The safety annotations are mechanical
//!
//! Read-only, destructive, and idempotent are the registry's own
//! classifications, mapped one for one rather than judged again here. A tool
//! whose annotation disagreed with the registry would be telling a model host
//! something the daemon does not believe.
//!
//! # Provenance is recomputed, not remembered
//!
//! Every digest this catalog stands on is computed from the bytes this build
//! embeds. A recorded string that happened to match would prove only that
//! somebody wrote it down.

use slingshot_domain::author_agent_transport_contract::AuthorAgentTransportContract;
use slingshot_domain::command::catalog::{CommandCatalog, CommandDescriptor};
use slingshot_domain::command::command_identity::CommandContract;
use slingshot_domain::command::schema::{self, SchemaRole};

/// The controls this server offers beside the registry's commands.
///
/// Fixed, because they are about operations rather than about content: what is
/// running, how it is going, what it produced, and what may be released. A
/// maintenance result is absent on purpose - it is addressed as a resource,
/// which is what lets it be read without inventing an operation for it.
pub const EVERY_CONTROL: &[&str] = &[
    "operation-list",
    "operation-status",
    "operation-wait",
    "operation-restart",
    "operation-result",
    "operation-artifact",
    "maintenance-preview",
    "maintenance-apply",
];

/// Whether a tool requires the caller's operation key.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KeyPresence {
    /// The caller supplies one, because a rerun is otherwise new work.
    Required,
    /// The caller may supply one, because a rerun is the same work anyway.
    Optional,
    /// The tool takes none, because it starts no work.
    Absent,
}

/// One tool, as a client sees it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ToolDescriptor {
    /// What it is called.
    pub name: String,
    /// What it is called, for a person.
    pub title: String,
    /// What it does.
    pub description: String,
    /// Whether calling it changes nothing.
    pub read_only_hint: bool,
    /// Whether calling it may remove something.
    pub destructive_hint: bool,
    /// Whether calling it twice is calling it once.
    pub idempotent_hint: bool,
    /// Whether it requires the caller's operation key.
    pub operation_key: KeyPresence,
}

/// The digests this catalog stands on, recomputed from the bytes it stands on.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Provenance {
    /// The author-agent transport contract this build carries.
    pub author_agent_transport_contract_digest: String,
    /// The canonical-JavaScript-Object-Notation contract this build carries.
    pub canonical_contract_digest: String,
    /// The annotation each role schema carries, which is that same digest.
    pub role_annotations: Vec<String>,
    /// The digest of the schema manifest this build publishes.
    pub command_schema_manifest_digest: String,
}

impl Provenance {
    /// Returns the provenance of this build, computed rather than recorded.
    #[must_use]
    pub fn recomputed() -> Self {
        let canonical = schema::canonical_contract_digest();
        let annotations =
            SchemaRole::both().into_iter().map(annotation_of).collect::<Vec<String>>();
        Self {
            author_agent_transport_contract_digest: AuthorAgentTransportContract::embedded_digest(),
            canonical_contract_digest: canonical,
            role_annotations: annotations,
            command_schema_manifest_digest: manifest_digest(),
        }
    }

    /// Returns whether every part of this provenance agrees with `other`.
    #[must_use]
    pub fn agrees_with(&self, other: &Self) -> bool {
        self == other
    }
}

/// Returns the canonical-contract annotation one role's schemas carry.
fn annotation_of(role: SchemaRole) -> String {
    let named = schema::COMMAND_WIRE_NAMES.first().copied().unwrap_or_default();
    schema::command_schema(named, role)[schema::CANONICAL_CONTRACT_ANNOTATION]
        .as_str()
        .unwrap_or_default()
        .to_owned()
}

/// Returns the digest of the schema manifest this build publishes.
fn manifest_digest() -> String {
    let manifest = schema::schema_manifest();
    let bytes = serde_json::to_vec(&manifest).unwrap_or_default();
    use sha2::Digest;
    let mut digest = sha2::Sha256::new();
    digest.update(&bytes);
    digest.finalize().iter().map(|byte| format!("{byte:02x}")).collect()
}

/// Why this catalog refuses to be built.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum CatalogRefusal {
    /// Two tools would answer to one name.
    #[error("{0} names two tools, and a client calling it would reach either")]
    NameCollision(String),
    /// A command declares a semantic version this build does not project.
    #[error("{named} declares version {declared}, and this build projects {expected}")]
    VersionUnsupported {
        /// Which command.
        named: String,
        /// What it declares.
        declared: String,
        /// What this build projects.
        expected: String,
    },
    /// The provenance this catalog stands on does not agree with this build.
    #[error("{0} does not agree with the bytes this build carries")]
    ProvenanceDrifted(String),
}

/// Returns every tool this server offers, in the order it offers them.
///
/// The registry's commands first, in the registry's own order, then the
/// controls. Provenance is checked before a single descriptor is produced, so a
/// build whose contracts have moved offers nothing rather than offering tools
/// whose schemas describe another build's commands.
///
/// # Errors
///
/// Returns [`CatalogRefusal`] naming the first thing that stops the catalog.
pub fn derive(against: &Provenance) -> Result<Vec<ToolDescriptor>, CatalogRefusal> {
    let recomputed = Provenance::recomputed();
    require_agreeing(against, &recomputed)?;
    let published = CommandCatalog::published();
    let mut tools = Vec::new();
    for descriptor in published.descriptors() {
        require_projectable(descriptor)?;
        tools.push(tool_for(descriptor));
    }
    for control in EVERY_CONTROL {
        tools.push(control_tool(control));
    }
    require_distinct(&tools)?;
    Ok(tools)
}

/// Requires the provenance a caller holds to be this build's own.
fn require_agreeing(held: &Provenance, recomputed: &Provenance) -> Result<(), CatalogRefusal> {
    let named = [
        (
            "the author-agent transport contract",
            &held.author_agent_transport_contract_digest,
            &recomputed.author_agent_transport_contract_digest,
        ),
        (
            "the canonical-JavaScript-Object-Notation contract",
            &held.canonical_contract_digest,
            &recomputed.canonical_contract_digest,
        ),
        (
            "the command schema manifest",
            &held.command_schema_manifest_digest,
            &recomputed.command_schema_manifest_digest,
        ),
    ];
    for (what, held_value, recomputed_value) in named {
        if held_value != recomputed_value {
            return Err(CatalogRefusal::ProvenanceDrifted(what.to_owned()));
        }
    }
    if held.role_annotations != recomputed.role_annotations {
        return Err(CatalogRefusal::ProvenanceDrifted("a role schema annotation".to_owned()));
    }
    Ok(())
}

/// Requires one command to be one this build projects.
fn require_projectable(descriptor: &CommandDescriptor) -> Result<(), CatalogRefusal> {
    let contract = CommandContract::embedded();
    let expected = contract.command_semantic_contract_versions.get(&descriptor.wire_name);
    if expected != Some(&descriptor.command_semantic_contract_version) {
        return Err(CatalogRefusal::VersionUnsupported {
            named: descriptor.wire_name.clone(),
            declared: descriptor.command_semantic_contract_version.clone(),
            expected: expected.cloned().unwrap_or_default(),
        });
    }
    Ok(())
}

/// Requires no two tools to answer to one name.
fn require_distinct(tools: &[ToolDescriptor]) -> Result<(), CatalogRefusal> {
    let mut seen = std::collections::BTreeSet::new();
    for tool in tools {
        if !seen.insert(tool.name.clone()) {
            return Err(CatalogRefusal::NameCollision(tool.name.clone()));
        }
    }
    Ok(())
}

/// Returns the tool one registry command becomes.
fn tool_for(descriptor: &CommandDescriptor) -> ToolDescriptor {
    let idempotent = !descriptor.intrinsic_idempotency.requires_operation_key();
    ToolDescriptor {
        name: descriptor.wire_name.clone(),
        title: descriptor.title.clone(),
        description: descriptor.description.clone(),
        read_only_hint: descriptor.access.read_only_hint(),
        destructive_hint: descriptor.destructive.destructive_hint(),
        idempotent_hint: idempotent,
        operation_key: if idempotent { KeyPresence::Optional } else { KeyPresence::Required },
    }
}

/// Returns the tool one control becomes.
fn control_tool(named: &str) -> ToolDescriptor {
    ToolDescriptor {
        name: named.to_owned(),
        title: named.replace('-', " "),
        description: control_description(named).to_owned(),
        read_only_hint: !CHANGING_CONTROLS.contains(&named),
        destructive_hint: DESTRUCTIVE_CONTROLS.contains(&named),
        idempotent_hint: true,
        operation_key: KeyPresence::Absent,
    }
}

/// The controls that change something about an operation.
const CHANGING_CONTROLS: &[&str] = &["operation-restart", "maintenance-apply"];

/// The controls that may remove something.
const DESTRUCTIVE_CONTROLS: &[&str] = &["maintenance-apply"];

/// What each control does, in the words a model host shows the model choosing it.
///
/// A control is chosen by what it says it does, and the controls are only
/// distinguishable by when a caller needs each one. Each description therefore
/// names the situation it answers and where its arguments come from.
const CONTROL_DESCRIPTIONS: &[(&str, &str)] = &[
    (
        "operation-list",
        "Lists the identifiers of this target's operations, most recent first, twenty-five at a \
         time unless limit says otherwise. Pass the continuation_token a page answered with to \
         read the next one. Use it to find an operation identifier you no longer hold, then ask \
         operation-status about it.",
    ),
    (
        "operation-status",
        "Reports where one operation has got to: its lifecycle state, its latest progress, and, \
         when it is paused for a person, the recovery category and revision operation-restart \
         needs. Takes the operation_identifier a detached call or operation-list returned.",
    ),
    (
        "operation-wait",
        "Waits for one operation to end and answers with its result or its terminal failure. \
         Use it after a call made with detached set to true, with the operation_identifier that \
         call returned.",
    ),
    (
        "operation-restart",
        "Makes one operation that is paused for manual recovery eligible to run again. \
         expected_operation_revision and expected_recovery_category are the values \
         operation-status reported, so a restart applies only to the state that was reviewed.",
    ),
    (
        "operation-result",
        "Answers with one succeeded operation's committed result: the document itself when it \
         fits inline, otherwise the artifact identifier and content digest operation-artifact \
         reads it with.",
    ),
    (
        "operation-artifact",
        "Answers with the bytes of one artifact an operation produced, verified against the \
         content digest its result declared. artifact_identifier and expected_content_digest \
         come from the operation's result.",
    ),
    (
        "maintenance-preview",
        "Previews which ended operations recorded before before_unix_milliseconds local \
         maintenance would release, at most limit of them, and changes nothing. The preview's \
         manifest digest is what maintenance-apply takes.",
    ),
    (
        "maintenance-apply",
        "Releases exactly the ended operations one preview listed, and nothing else. \
         reviewed_manifest_digest is the digest maintenance-preview answered with; a manifest \
         that has changed since is refused. Nothing on the author is touched.",
    ),
];

/// Returns what one control does.
fn control_description(named: &str) -> &'static str {
    CONTROL_DESCRIPTIONS
        .iter()
        .find(|(control, _)| *control == named)
        .map_or("", |(_, description)| description)
}
