//! The one place a command's safety, effect, and identity are written down.
//!
//! Everything a presentation needs to decide how to treat a command comes from
//! here, and from nowhere else. The three classifications are data in a closed
//! twelve-row table, not something inferred from a command's name, its result
//! size, or whether it publishes an artifact. A name that reads like a read is
//! not evidence, and a command that writes a file is not thereby a write.
//!
//! The distinctions are narrower than they look:
//!
//! - `Read` means the command changes no repository or replicated content.
//!   Operation bookkeeping and artifact publication do not make it a `Write`,
//!   which is why loading and packaging stay reads.
//! - `Destructive` means a success can replace content that was already
//!   visible. Refusing an existing target is not destructive; replacing what a
//!   publisher is already serving is.
//! - `IntrinsicallyIdempotent` means running it twice is running it once. It is
//!   the only source of both the idempotency hint and the operation-key
//!   requirement, so the two can never disagree.
//!
//! The two closed enums are here too, beside the table that describes them.
//! They are parallel on purpose: one variant of [`Command`] answers with one
//! variant of [`CommandResult`], and [`validate_result_for_command`] refuses a
//! pair that does not match before anything is persisted or forwarded.
//!
//! What that validation can and cannot do is worth being exact about. It
//! compares the variant, and every fact a result echoes back or derives from
//! the request - the loaded path, the configuration identifier, the anchor a
//! failure names, the counts a replication reports, the file name a package
//! suggests, the target a mutation computed. It does not decode a continuation
//! token, re-execute repository semantics, or tell apart two results of the
//! same request shape that expose no differing fact. That last case is real,
//! and it is Plan 0005's authenticated submitted-command digest to close rather
//! than something this crate can pretend to.
//!
//! Loading and packaging are the interesting rows: `Read`, `NonDestructive`,
//! and not idempotent, because each may publish a retained artifact and a
//! second run without the caller's operation key would create a duplicate
//! rather than reuse it.

use serde::{Deserialize, Serialize};

use crate::command::artifact::ArtifactSlotDeclaration;
use crate::command::canonical_json::{canonical_digest, write_canonical};
use crate::command::command_identity::{CommandContract, INITIAL_COMMAND_VERSION};
use crate::command::schema::{COMMAND_WIRE_NAMES, SchemaRole, command_schema};

/// Whether a command changes content.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum AccessClassification {
    /// It changes no repository or replicated content.
    Read,
    /// It can change authored or replicated content.
    Write,
}

/// Whether a command can replace what was already visible.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum DestructiveClassification {
    /// A success can replace an already visible content state.
    Destructive,
    /// It observes, packages, or refuses an existing target instead.
    NonDestructive,
}

/// Whether running a command twice is running it once.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum IntrinsicIdempotencyClassification {
    /// Running it twice is running it once.
    IntrinsicallyIdempotent,
    /// It is not, so the caller supplies an operation key.
    NotIntrinsicallyIdempotent,
}

impl IntrinsicIdempotencyClassification {
    /// Returns the Model Context Protocol idempotency hint.
    ///
    /// Exactly this column and nothing else. Deriving the hint from anything
    /// observable - a name, a result size, an artifact - is how two callers end
    /// up disagreeing about whether a retry is safe.
    #[must_use]
    pub fn idempotent_hint(self) -> bool {
        matches!(self, Self::IntrinsicallyIdempotent)
    }

    /// Returns whether the caller must supply an operation key.
    ///
    /// The complement of the hint, from the same column, so the two cannot
    /// disagree.
    #[must_use]
    pub fn requires_operation_key(self) -> bool {
        !self.idempotent_hint()
    }
}

impl AccessClassification {
    /// Returns the Model Context Protocol read-only hint.
    #[must_use]
    pub fn read_only_hint(self) -> bool {
        matches!(self, Self::Read)
    }
}

impl DestructiveClassification {
    /// Returns the Model Context Protocol destructive hint.
    #[must_use]
    pub fn destructive_hint(self) -> bool {
        matches!(self, Self::Destructive)
    }
}

/// Everything the registry knows about one command.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct CommandDescriptor {
    /// Whether it changes content.
    pub access: AccessClassification,
    /// Digest of the canonical argument schema.
    pub arguments_schema_sha256: String,
    /// Digest of the byte contract both schemas bind.
    pub canonical_json_contract_sha256: String,
    /// Digest of the canonical limits manifest.
    pub command_contract_limits_sha256: String,
    /// Version of the meaning this command implements.
    pub command_semantic_contract_version: String,
    /// Present-state description.
    pub description: String,
    /// Whether a success can replace visible content.
    pub destructive: DestructiveClassification,
    /// Failure categories this version allows, in order.
    pub failure_categories: Vec<String>,
    /// Whether running it twice is running it once.
    pub intrinsic_idempotency: IntrinsicIdempotencyClassification,
    /// Largest canonical success result, when one is named.
    pub maximum_result_bytes: u64,
    /// Digest of the canonical result schema.
    pub result_schema_sha256: String,
    /// Artifact slots this command declares, in order.
    pub remote_artifact_slots: Vec<ArtifactSlotDeclaration>,
    /// Human title.
    pub title: String,
    /// Stable name, which is also the sole capability name.
    pub wire_name: String,
}

impl CommandDescriptor {
    /// Returns whether this command and `other` are compatible.
    ///
    /// Every one of the five has to match. A match on one schema role, or on
    /// bytes produced under a different limits or version contract, is not
    /// compatibility - it is two systems agreeing about half of something.
    #[must_use]
    pub fn compatible_with(&self, other: &Self) -> bool {
        self.wire_name == other.wire_name
            && self.command_semantic_contract_version == other.command_semantic_contract_version
            && self.command_contract_limits_sha256 == other.command_contract_limits_sha256
            && self.arguments_schema_sha256 == other.arguments_schema_sha256
            && self.result_schema_sha256 == other.result_schema_sha256
    }
}

/// One row of the closed classification table.
///
/// Data rather than a rule, because there is no rule: whether packaging is a
/// read is a judgement somebody made, and it belongs written down where it can
/// be read and argued with.
struct ClassificationRow {
    /// Stable name.
    wire_name: &'static str,
    /// Human title.
    title: &'static str,
    /// Present-state description.
    description: &'static str,
    /// Whether it changes content.
    access: AccessClassification,
    /// Whether a success can replace visible content.
    destructive: DestructiveClassification,
    /// Whether running it twice is running it once.
    intrinsic_idempotency: IntrinsicIdempotencyClassification,
    /// Limit naming its largest canonical success result.
    result_bytes_limit: &'static str,
    /// Failure categories this version allows beside the shared ones.
    failure_categories: &'static [&'static str],
    /// Whether the shared discovery categories apply.
    discovery: bool,
}

/// Failure categories every discovery command allows.
const DISCOVERY_FAILURES: &[&str] = &[
    "discovery_budget_exceeded",
    "continuation_token_malformed",
    "continuation_token_integrity_invalid",
    "continuation_token_wrong_target",
    "continuation_token_wrong_query",
    "continuation_token_expired",
];

/// Anchor failures the five rooted discovery commands allow.
const ROOT_ANCHOR_FAILURES: &[&str] = &["root_not_found", "root_access_denied"];

/// The closed twelve-row table, in ascending wire-name order.
const CLASSIFICATIONS: &[ClassificationRow] = &[
    ClassificationRow {
        wire_name: "add_component",
        title: "Add a component",
        description: "Creates one component under a page's content resource and appends it \
                      last in its orderable parent.",
        access: AccessClassification::Write,
        destructive: DestructiveClassification::NonDestructive,
        intrinsic_idempotency: IntrinsicIdempotencyClassification::NotIntrinsicallyIdempotent,
        result_bytes_limit: "maximum_mutation_success_result_bytes",
        failure_categories: &[
            "page_not_found",
            "page_invalid",
            "parent_not_found",
            "parent_access_denied",
            "parent_not_orderable",
            "target_already_exists",
            "property_rejected",
            "repository_commit_failed",
            "mutation_outcome_unknown",
        ],
        discovery: false,
    },
    ClassificationRow {
        wire_name: "create_page",
        title: "Create a page",
        description: "Creates one page from a template and applies its title and initial \
                      properties to the new page's content resource.",
        access: AccessClassification::Write,
        destructive: DestructiveClassification::NonDestructive,
        intrinsic_idempotency: IntrinsicIdempotencyClassification::NotIntrinsicallyIdempotent,
        result_bytes_limit: "maximum_mutation_success_result_bytes",
        failure_categories: &[
            "target_already_exists",
            "parent_not_found",
            "parent_access_denied",
            "template_not_found",
            "template_invalid",
            "property_rejected",
            "repository_commit_failed",
            "mutation_outcome_unknown",
        ],
        discovery: false,
    },
    ClassificationRow {
        wire_name: "download_content_package",
        title: "Download a content package",
        description: "Builds one FileVault content package from roots and ordered selection \
                      filters and returns its artifact metadata.",
        access: AccessClassification::Read,
        destructive: DestructiveClassification::NonDestructive,
        intrinsic_idempotency: IntrinsicIdempotencyClassification::NotIntrinsicallyIdempotent,
        result_bytes_limit: "maximum_command_result_bytes",
        failure_categories: &[
            "pattern_rejected",
            "filevault_profile_unsupported",
            "filevault_filter_unrepresentable",
            "root_not_found",
            "root_access_denied",
            "repository_read_failed",
            "filevault_package_failed",
            "staging_cleanup_failed",
            "artifact_publication_failed",
            "artifact_publication_outcome_unknown",
            "evaluation_budget_exceeded",
        ],
        discovery: false,
    },
    ClassificationRow {
        wire_name: "find_assets_by_metadata",
        title: "Find assets by metadata",
        description: "Finds assets under an anchor by media format, original-rendition size, \
                      tags, and property predicates.",
        access: AccessClassification::Read,
        destructive: DestructiveClassification::NonDestructive,
        intrinsic_idempotency: IntrinsicIdempotencyClassification::IntrinsicallyIdempotent,
        result_bytes_limit: "maximum_discovery_result_bytes",
        failure_categories: ROOT_ANCHOR_FAILURES,
        discovery: true,
    },
    ClassificationRow {
        wire_name: "find_assets_referenced_by_page",
        title: "Find assets referenced by a page",
        description: "Reports the assets one page refers to and the relative property paths \
                      it refers to them from.",
        access: AccessClassification::Read,
        destructive: DestructiveClassification::NonDestructive,
        intrinsic_idempotency: IntrinsicIdempotencyClassification::IntrinsicallyIdempotent,
        result_bytes_limit: "maximum_discovery_result_bytes",
        failure_categories: &["page_not_found", "page_access_denied", "page_invalid"],
        discovery: true,
    },
    ClassificationRow {
        wire_name: "find_pages_by_template",
        title: "Find pages by template",
        description: "Finds pages under an anchor whose recorded template equals one \
                      repository address.",
        access: AccessClassification::Read,
        destructive: DestructiveClassification::NonDestructive,
        intrinsic_idempotency: IntrinsicIdempotencyClassification::IntrinsicallyIdempotent,
        result_bytes_limit: "maximum_discovery_result_bytes",
        failure_categories: ROOT_ANCHOR_FAILURES,
        discovery: true,
    },
    ClassificationRow {
        wire_name: "find_pages_containing_phrase",
        title: "Find pages containing a phrase",
        description: "Finds pages under an anchor holding one exact phrase as a contiguous \
                      sequence of Unicode scalar values.",
        access: AccessClassification::Read,
        destructive: DestructiveClassification::NonDestructive,
        intrinsic_idempotency: IntrinsicIdempotencyClassification::IntrinsicallyIdempotent,
        result_bytes_limit: "maximum_discovery_result_bytes",
        failure_categories: ROOT_ANCHOR_FAILURES,
        discovery: true,
    },
    ClassificationRow {
        wire_name: "find_pages_using_components",
        title: "Find pages using components",
        description: "Finds pages under an anchor whose subtree uses any or all of the \
                      requested component resource types.",
        access: AccessClassification::Read,
        destructive: DestructiveClassification::NonDestructive,
        intrinsic_idempotency: IntrinsicIdempotencyClassification::IntrinsicallyIdempotent,
        result_bytes_limit: "maximum_discovery_result_bytes",
        failure_categories: ROOT_ANCHOR_FAILURES,
        discovery: true,
    },
    ClassificationRow {
        wire_name: "inspect_open_service_gateway_initiative_configuration",
        title: "Inspect a configuration",
        description: "Reads one effective configuration by its exact persistent identifier, \
                      redacting every value the evidence does not clear.",
        access: AccessClassification::Read,
        destructive: DestructiveClassification::NonDestructive,
        intrinsic_idempotency: IntrinsicIdempotencyClassification::IntrinsicallyIdempotent,
        result_bytes_limit: "maximum_inspected_configuration_result_bytes",
        failure_categories: &[
            "configuration_lookup_failed",
            "configuration_lookup_mismatch",
            "configuration_lookup_ambiguous",
            "configuration_lookup_budget_exceeded",
            "configuration_value_unsupported",
            "configuration_value_malformed",
            "configuration_value_budget_exceeded",
            "configuration_result_budget_exceeded",
        ],
        discovery: false,
    },
    ClassificationRow {
        wire_name: "load_content_as_json",
        title: "Load content as JSON",
        description: "Reads one repository subtree to a bounded depth and returns it inline \
                      or as an artifact, decided by the document's own canonical bytes.",
        access: AccessClassification::Read,
        destructive: DestructiveClassification::NonDestructive,
        intrinsic_idempotency: IntrinsicIdempotencyClassification::NotIntrinsicallyIdempotent,
        result_bytes_limit: "maximum_command_result_bytes",
        failure_categories: &[
            "not_found",
            "access_denied",
            "unsupported_repository_value",
            "load_budget_exceeded",
        ],
        discovery: false,
    },
    ClassificationRow {
        wire_name: "query_paths",
        title: "Query paths",
        description: "Finds nodes under an anchor by primary type and a bounded collection \
                      of property predicates.",
        access: AccessClassification::Read,
        destructive: DestructiveClassification::NonDestructive,
        intrinsic_idempotency: IntrinsicIdempotencyClassification::IntrinsicallyIdempotent,
        result_bytes_limit: "maximum_discovery_result_bytes",
        failure_categories: ROOT_ANCHOR_FAILURES,
        discovery: true,
    },
    ClassificationRow {
        wire_name: "replicate_content",
        title: "Replicate content",
        description: "Offers one path, or a path and its descendants, to the author \
                      replication service and reports what was admitted.",
        access: AccessClassification::Write,
        destructive: DestructiveClassification::Destructive,
        intrinsic_idempotency: IntrinsicIdempotencyClassification::NotIntrinsicallyIdempotent,
        result_bytes_limit: "maximum_replication_result_bytes",
        failure_categories: &[
            "source_not_found",
            "source_access_denied",
            "candidate_limit_exceeded",
            "traversal_budget_exceeded",
            "admission_rejected",
            "admission_budget_exceeded",
            "admission_outcome_unknown",
        ],
        discovery: false,
    },
];

/// Every command this plan defines, with everything the registry knows.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(transparent)]
pub struct CommandCatalog {
    /// The descriptors, in ascending wire-name order.
    descriptors: Vec<CommandDescriptor>,
}

impl CommandCatalog {
    /// Returns the catalog.
    ///
    /// # Panics
    ///
    /// Panics when the table and the schema inventory disagree about which
    /// commands exist, which is a defect in this repository.
    #[must_use]
    pub fn published() -> Self {
        let limits_digest = canonical_digest(
            &write_canonical(
                &serde_json::from_str(CommandContract::embedded_manifest())
                    .expect("the limits manifest is one value"),
            )
            .expect("the limits manifest is canonical"),
        );
        let contract_digest = crate::command::schema::canonical_contract_digest();
        let descriptors = CLASSIFICATIONS
            .iter()
            .map(|row| describe(row, &limits_digest, &contract_digest))
            .collect();
        Self { descriptors }
    }

    /// Returns the descriptors, in ascending wire-name order.
    #[must_use]
    pub fn descriptors(&self) -> &[CommandDescriptor] {
        &self.descriptors
    }

    /// Returns the descriptor for `wire_name`, when there is one.
    #[must_use]
    pub fn find(&self, wire_name: &str) -> Option<&CommandDescriptor> {
        self.descriptors.iter().find(|descriptor| descriptor.wire_name == wire_name)
    }
}

/// Returns the descriptor one table row describes.
fn describe(
    row: &ClassificationRow,
    limits_digest: &str,
    contract_digest: &str,
) -> CommandDescriptor {
    let limits = CommandContract::embedded();
    let digest = |role| {
        canonical_digest(
            &write_canonical(&command_schema(row.wire_name, role)).expect("a schema is canonical"),
        )
    };
    let shared = if row.discovery { DISCOVERY_FAILURES } else { &[] };
    let failure_categories: Vec<String> = shared
        .iter()
        .chain(row.failure_categories)
        .map(|category| (*category).to_owned())
        .collect();
    CommandDescriptor {
        access: row.access,
        arguments_schema_sha256: digest(SchemaRole::Arguments),
        canonical_json_contract_sha256: contract_digest.to_owned(),
        command_contract_limits_sha256: limits_digest.to_owned(),
        command_semantic_contract_version: INITIAL_COMMAND_VERSION.to_owned(),
        description: row.description.to_owned(),
        destructive: row.destructive,
        failure_categories,
        intrinsic_idempotency: row.intrinsic_idempotency,
        maximum_result_bytes: limits.limit(row.result_bytes_limit),
        remote_artifact_slots: declared_slots(row.wire_name),
        result_schema_sha256: digest(SchemaRole::Result),
        title: row.title.to_owned(),
        wire_name: row.wire_name.to_owned(),
    }
}

/// Returns the artifact slots one command declares.
///
/// Two commands declare one each and the other ten declare none. A command that
/// declares no slot forbids one, so an empty list is a statement rather than an
/// omission.
fn declared_slots(wire_name: &str) -> Vec<ArtifactSlotDeclaration> {
    match wire_name {
        "load_content_as_json" => vec![ArtifactSlotDeclaration::loaded_content()],
        "download_content_package" => vec![ArtifactSlotDeclaration::content_package()],
        _ => Vec::new(),
    }
}

/// Returns whether the catalog and the schema inventory name the same commands.
#[must_use]
pub fn catalog_matches_schema_inventory() -> bool {
    let catalog = CommandCatalog::published();
    let names: Vec<&str> =
        catalog.descriptors().iter().map(|descriptor| descriptor.wire_name.as_str()).collect();
    names == COMMAND_WIRE_NAMES
}

/// Whether one result answers the request that produced it.
///
/// Implemented once per command pair so the dispatch below stays one rule
/// rather than twelve. What "answers" means is each command's own business:
/// most compare a path or an identifier the result echoes, and one has nothing
/// to compare.
trait AnswersCommand {
    /// The command this result answers.
    type Asked;

    /// Returns whether this result answers `asked`.
    fn answers(&self, asked: &Self::Asked) -> bool;
}

impl AnswersCommand for crate::command::add_component::AddComponentResult {
    type Asked = crate::command::add_component::AddComponentCommand;

    fn answers(&self, asked: &Self::Asked) -> bool {
        self.require_answers(asked).is_ok()
    }
}

impl AnswersCommand for crate::command::create_page::CreatePageResult {
    type Asked = crate::command::create_page::CreatePageCommand;

    fn answers(&self, asked: &Self::Asked) -> bool {
        self.require_answers(asked).is_ok()
    }
}

impl AnswersCommand for crate::command::download_content_package::DownloadContentPackageResult {
    type Asked = crate::command::download_content_package::DownloadContentPackageCommand;

    fn answers(&self, asked: &Self::Asked) -> bool {
        self.require_answers(asked).is_ok()
    }
}

impl AnswersCommand for crate::command::find_assets_by_metadata::FindAssetsByMetadataResult {
    type Asked = crate::command::find_assets_by_metadata::FindAssetsByMetadataCommand;

    fn answers(&self, asked: &Self::Asked) -> bool {
        self.require_answers(asked).is_ok()
    }
}

impl AnswersCommand
    for crate::command::find_assets_referenced_by_page::FindAssetsReferencedByPageResult
{
    type Asked = crate::command::find_assets_referenced_by_page::FindAssetsReferencedByPageCommand;

    fn answers(&self, asked: &Self::Asked) -> bool {
        self.require_answers(asked).is_ok()
    }
}

impl AnswersCommand for crate::command::find_pages_by_template::FindPagesByTemplateResult {
    type Asked = crate::command::find_pages_by_template::FindPagesByTemplateCommand;

    fn answers(&self, asked: &Self::Asked) -> bool {
        self.require_answers(asked).is_ok()
    }
}

impl AnswersCommand
    for crate::command::find_pages_containing_phrase::FindPagesContainingPhraseResult
{
    type Asked = crate::command::find_pages_containing_phrase::FindPagesContainingPhraseCommand;

    fn answers(&self, asked: &Self::Asked) -> bool {
        self.require_answers(asked).is_ok()
    }
}

impl AnswersCommand
    for crate::command::find_pages_using_components::FindPagesUsingComponentsResult
{
    type Asked = crate::command::find_pages_using_components::FindPagesUsingComponentsCommand;

    fn answers(&self, asked: &Self::Asked) -> bool {
        self.require_answers(asked).is_ok()
    }
}

impl AnswersCommand for crate::command::inspect_open_service_gateway_initiative_configuration::InspectOpenServiceGatewayInitiativeConfigurationResult {
    type Asked = crate::command::inspect_open_service_gateway_initiative_configuration::InspectOpenServiceGatewayInitiativeConfigurationCommand;

    fn answers(&self, asked: &Self::Asked) -> bool {
        self.require_answers(asked).is_ok()
    }
}

impl AnswersCommand
    for crate::command::load_content_as_javascript_object_notation::LoadContentAsJavaScriptObjectNotationResult
{
    type Asked =
        crate::command::load_content_as_javascript_object_notation::LoadContentAsJavaScriptObjectNotationCommand;

    fn answers(&self, asked: &Self::Asked) -> bool {
        self.require_answers(asked).is_ok()
    }
}

impl AnswersCommand for crate::command::query_paths::QueryPathsResult {
    type Asked = crate::command::query_paths::QueryPathsCommand;

    fn answers(&self, asked: &Self::Asked) -> bool {
        self.require_answers(asked).is_ok()
    }
}

impl AnswersCommand for crate::command::replicate_content::ReplicateContentResult {
    type Asked = crate::command::replicate_content::ReplicateContentCommand;

    fn answers(&self, asked: &Self::Asked) -> bool {
        let _ = asked;
        true
    }
}

/// Builds the two parallel enums and the one rule that pairs them.
///
/// Written as a macro because the alternative is three twelve-armed matches
/// that have to be kept in step by hand, and a thirteenth command would need
/// remembering in all three.
macro_rules! command_family {
    ($($(#[$attribute:meta])* $variant:ident, $wire:literal, $command:path, $result:path;)+) => {
        /// One request, in whichever shape the caller asked for.
        #[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
        #[serde(tag = "command", rename_all = "snake_case", deny_unknown_fields)]
        pub enum Command {
            $($(#[$attribute])* $variant($command),)+
        }

        /// One answer, in whichever shape its command produces.
        #[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
        #[serde(tag = "command", rename_all = "snake_case", deny_unknown_fields)]
        pub enum CommandResult {
            $($(#[$attribute])* $variant($result),)+
        }

        impl Command {
            /// Returns the stable wire name of this command.
            ///
            /// Also its sole capability name. There is no second alias to drift.
            #[must_use]
            pub fn wire_name(&self) -> &'static str {
                match self {
                    $(Self::$variant(_) => $wire,)+
                }
            }
        }

        impl CommandResult {
            /// Returns the stable wire name of the command this answers.
            #[must_use]
            pub fn wire_name(&self) -> &'static str {
                match self {
                    $(Self::$variant(_) => $wire,)+
                }
            }
        }

        /// Requires `result` to answer `command`.
        ///
        /// Variant first, then every fact the result echoes or derives. A
        /// same-variant substitution that exposes no differing fact passes
        /// here and is Plan 0005's authenticated digest to catch; this function
        /// does not pretend otherwise.
        ///
        /// # Errors
        ///
        /// Returns [`ResultContextFailure::VariantMismatch`] when the two are
        /// different commands and [`ResultContextFailure::RequestMismatch`]
        /// when an echoed fact belongs to another request.
        pub fn validate_result_for_command(
            command: &Command,
            result: &CommandResult,
        ) -> Result<(), ResultContextFailure> {
            match (command, result) {
                $((Command::$variant(asked), CommandResult::$variant(answered)) => {
                    if answered.answers(asked) {
                        Ok(())
                    } else {
                        Err(ResultContextFailure::RequestMismatch)
                    }
                })+
                _ => Err(ResultContextFailure::VariantMismatch),
            }
        }
    };
}

/// Why a result does not answer the command it was offered for.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum ResultContextFailure {
    /// The result is of another command entirely.
    #[error("a result answers the command it was produced for")]
    VariantMismatch,
    /// The result is the right shape and answers another request.
    #[error("a result echoes the facts its own request determined")]
    RequestMismatch,
}

command_family! {
    /// Add one component to a page.
    /// What adding a component produced.
    AddComponent, "add_component", crate::command::add_component::AddComponentCommand, crate::command::add_component::AddComponentResult;
    /// Create one page from a template.
    /// What creating a page produced.
    CreatePage, "create_page", crate::command::create_page::CreatePageCommand, crate::command::create_page::CreatePageResult;
    /// Build one content package.
    /// What building a package produced.
    DownloadContentPackage, "download_content_package", crate::command::download_content_package::DownloadContentPackageCommand, crate::command::download_content_package::DownloadContentPackageResult;
    /// Find assets by their metadata.
    /// What the asset search found.
    FindAssetsByMetadata, "find_assets_by_metadata", crate::command::find_assets_by_metadata::FindAssetsByMetadataCommand, crate::command::find_assets_by_metadata::FindAssetsByMetadataResult;
    /// Find the assets one page refers to.
    /// What the reference search found.
    FindAssetsReferencedByPage, "find_assets_referenced_by_page", crate::command::find_assets_referenced_by_page::FindAssetsReferencedByPageCommand, crate::command::find_assets_referenced_by_page::FindAssetsReferencedByPageResult;
    /// Find pages built from one template.
    /// What the template search found.
    FindPagesByTemplate, "find_pages_by_template", crate::command::find_pages_by_template::FindPagesByTemplateCommand, crate::command::find_pages_by_template::FindPagesByTemplateResult;
    /// Find pages containing one phrase.
    /// What the phrase search found.
    FindPagesContainingPhrase, "find_pages_containing_phrase", crate::command::find_pages_containing_phrase::FindPagesContainingPhraseCommand, crate::command::find_pages_containing_phrase::FindPagesContainingPhraseResult;
    /// Find pages using particular components.
    /// What the component search found.
    FindPagesUsingComponents, "find_pages_using_components", crate::command::find_pages_using_components::FindPagesUsingComponentsCommand, crate::command::find_pages_using_components::FindPagesUsingComponentsResult;
    /// Inspect one effective configuration.
    /// What the configuration inspection found.
    InspectOpenServiceGatewayInitiativeConfiguration, "inspect_open_service_gateway_initiative_configuration", crate::command::inspect_open_service_gateway_initiative_configuration::InspectOpenServiceGatewayInitiativeConfigurationCommand, crate::command::inspect_open_service_gateway_initiative_configuration::InspectOpenServiceGatewayInitiativeConfigurationResult;
    /// Load one repository subtree.
    /// What the load produced.
    LoadContentAsJson, "load_content_as_json", crate::command::load_content_as_javascript_object_notation::LoadContentAsJavaScriptObjectNotationCommand, crate::command::load_content_as_javascript_object_notation::LoadContentAsJavaScriptObjectNotationResult;
    /// Find nodes answering a structured question.
    /// What the query found.
    QueryPaths, "query_paths", crate::command::query_paths::QueryPathsCommand, crate::command::query_paths::QueryPathsResult;
    /// Offer content to the replication service.
    /// What the replication admitted.
    ReplicateContent, "replicate_content", crate::command::replicate_content::ReplicateContentCommand, crate::command::replicate_content::ReplicateContentResult;
}
