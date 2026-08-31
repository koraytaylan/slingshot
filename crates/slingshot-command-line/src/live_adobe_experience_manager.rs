//! Verifying the read path against a real author, on purpose and never by accident.
//!
//! The hermetic fake-author suite is the release gate and stays the release
//! gate. What an operator additionally wants, once, before trusting a
//! deployment, is to watch the same read path run against their own author. So
//! this exists, and everything about it is arranged so that it cannot happen by
//! accident: an ordinary `cargo test` reaches nothing, the leaf refuses without
//! an explicit enabling option, and the selection it is given is checked before
//! a single byte of configuration is read.
//!
//! # Read-only by construction, not by care
//!
//! Which commands a verification may run is not a list somebody maintains here.
//! It is the registry's own access and destructive columns, read row by row: a
//! command is admissible when the registry calls it a read that replaces
//! nothing, and refused otherwise. Nine of the twelve rows qualify and three do
//! not, and if a thirteenth row appears the answer follows from the row rather
//! than from anybody remembering to come back here.
//!
//! Idempotency is never consulted. It is the column that decides whether a
//! retry is safe, not whether a run may happen at all, and reading it as an
//! access decision is how a read that produces an artifact ends up looking like
//! a write, or worse, how a write ends up looking safe because running it twice
//! is running it once.
//!
//! # What a live report may claim
//!
//! One live run says something about exactly the author it ran against, at the
//! version it was at. It says nothing about the next patch level of the same
//! product, and a report that quietly generalized would be worse than no report
//! at all, because it would be believed. So a report carries the target it
//! observed and answers questions about that target only, and hermetic
//! conformance and live observation are separate kinds of evidence that are
//! never added together.

use std::collections::BTreeMap;

use slingshot_domain::command::catalog::{
    AccessClassification, CommandCatalog, CommandDescriptor, DestructiveClassification,
};
use slingshot_domain::command::repository_path::RepositoryPath;
use slingshot_domain::command::schema::{SchemaRole, canonical_contract_digest};
use slingshot_domain::profile::AdobeExperienceManagerDeployment;
use slingshot_domain::selected_command_contract_identity::{
    ContractIdentityFailure, SelectedCommandContractIdentity,
};

use crate::configuration_check::ResolvedFacts;
use crate::exit_classification;
use crate::invocation::{
    CONTENT_ROOT_OPTION, ENABLE_LIVE_AUTHOR_OPTION, Invocation, OutputForm, PATH_OPTION,
    PHRASE_OPTION, Selection, requires_operation_key,
};

/// The tree a verification's content root lives under.
///
/// A repository path and a filesystem path are both a slash and some segments,
/// so "absolute" separates neither from the other. What separates them is where
/// they start: authored content lives here, and a source checkout, a home
/// directory, and a mount point do not.
pub const CONTENT_TREE: &str = "/content";

/// The exercises a verification runs, in order.
pub const EXERCISES: &[&str] = &[
    "capabilities",
    "load_content_as_json",
    "query_paths",
    "find_pages_containing_phrase",
    "progress",
    "terminal_result",
    "heartbeat",
    "reconnect",
    "snapshot_recovery",
];

/// The exercises a verification runs only when they are supported and selected.
pub const OPTIONAL_EXERCISES: &[&str] = &["verified_artifact", "configuration_inspection"];

/// The commands a verification submits, in order.
///
/// Three of the nine admissible commands, chosen because between them they
/// cover a load, a rooted path query, and one page query - which is every shape
/// of read the transport has. Submitting all nine would prove the same three
/// things about the transport nine times and take nine times as long against
/// somebody's real author.
pub const SUBMITTED_COMMANDS: &[&str] =
    &["load_content_as_json", "query_paths", "find_pages_containing_phrase"];

/// The phrase the one page query looks for.
///
/// Content nobody has to have. A phrase that happened to match would make the
/// exercise depend on what is in an operator's repository, and an exercise that
/// only passes against some repositories proves nothing about the read path.
pub const VERIFICATION_PHRASE: &str = "slingshot";

/// Why a verification does not happen.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum LiveRefusal {
    /// Nobody asked for a real author.
    #[error("reaching a real author happens only when {ENABLE_LIVE_AUTHOR_OPTION} says so")]
    NotEnabled,
    /// A selection this needs was not made.
    #[error("a verification names its {0}, and this names none")]
    SelectionAbsent(&'static str),
    /// The content root is not a repository path.
    #[error("{named} is not a repository path: {reason}")]
    ContentRootUnusable {
        /// What was given.
        named: String,
        /// Why it is not one.
        reason: String,
    },
    /// The content root is not authored content.
    #[error("a verification reads authored content under {CONTENT_TREE}, and {0} is elsewhere")]
    ContentRootElsewhere(String),
    /// The registry calls this command something a verification may not run.
    #[error("{command} is {classification} in the registry, and a verification only reads")]
    NotAdmissible {
        /// Which command.
        command: String,
        /// What the registry calls it.
        classification: &'static str,
    },
    /// The registry holds no such command.
    #[error("no command is registered under the wire name {0}")]
    UnknownCommand(String),
    /// This build cannot establish what it holds.
    #[error("this build cannot say what contract it holds for {command}: {reason}")]
    ContractUnavailable {
        /// Which command.
        command: String,
        /// What stopped it.
        reason: String,
    },
    /// The agent's capability and this build's contract are not the same.
    #[error("{command} is not the same contract here and at the agent: {field} differs")]
    IdentityDrift {
        /// Which command.
        command: String,
        /// Which of the five fields differs.
        field: &'static str,
    },
    /// The agent annotates a role schema with another canonical contract.
    #[error("the {role} schema of {command} names canonical contract {named} at the agent")]
    CanonicalContractDrift {
        /// Which command.
        command: String,
        /// What the agent names.
        named: String,
        /// Which role schema.
        role: &'static str,
    },
    /// The agent annotates a role schema with nothing at all.
    #[error("the {role} schema of {command} carries no canonical-contract annotation")]
    CanonicalContractAnnotationAbsent {
        /// Which command.
        command: String,
        /// Which role schema.
        role: &'static str,
    },
    /// The conformance trace does not attest what it has to attest.
    #[error("the agent's configuration conformance does not attest {0}")]
    ConformanceNotAttested(&'static str),
}

/// What an explicitly enabled verification was told to do.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Enablement {
    /// The authored content root every read stays under.
    pub content_root: RepositoryPath,
    /// Which environment of that profile.
    pub environment: String,
    /// Which profile to read.
    pub profile: String,
}

impl Enablement {
    /// Returns what one invocation enables, when it enables anything.
    ///
    /// Nothing is read, opened, resolved, or dialled here. This decides whether
    /// a verification happens at all, and it decides it from the invocation
    /// alone, so a caller who did not ask for a live run reaches no
    /// configuration, no credential, no daemon, and no network.
    ///
    /// # Errors
    ///
    /// Returns [`LiveRefusal::NotEnabled`] when nobody asked,
    /// [`LiveRefusal::SelectionAbsent`] when a selection is missing, and one of
    /// the content-root refusals when the root is not authored content.
    pub fn read(invocation: &Invocation) -> Result<Self, LiveRefusal> {
        if !invocation.arguments.contains_key(ENABLE_LIVE_AUTHOR_OPTION) {
            return Err(LiveRefusal::NotEnabled);
        }
        let profile =
            invocation.selection.profile.clone().ok_or(LiveRefusal::SelectionAbsent("profile"))?;
        let environment = invocation
            .selection
            .environment
            .clone()
            .ok_or(LiveRefusal::SelectionAbsent("environment"))?;
        let named = invocation
            .arguments
            .get(CONTENT_ROOT_OPTION)
            .ok_or(LiveRefusal::SelectionAbsent("content root"))?;
        let content_root = require_authored_content(named)?;
        Ok(Self { content_root, environment, profile })
    }
}

/// Requires one spelling to name authored content this may read under.
fn require_authored_content(named: &str) -> Result<RepositoryPath, LiveRefusal> {
    let held = RepositoryPath::parse(named).map_err(|failure| {
        LiveRefusal::ContentRootUnusable { named: named.to_owned(), reason: failure.to_string() }
    })?;
    let text = held.as_text();
    let under = text == CONTENT_TREE || text.starts_with(&format!("{CONTENT_TREE}/"));
    if under { Ok(held) } else { Err(LiveRefusal::ContentRootElsewhere(named.to_owned())) }
}

/// Returns the invocation one submitted exercise runs.
///
/// Built here rather than typed by an operator, so a verification cannot reach
/// a path outside the root it was given: every rooted option carries the
/// enablement's own content root and nothing a caller wrote.
#[must_use]
pub fn exercise_invocation(
    command: &str,
    enablement: &Enablement,
    operation_key: &str,
) -> Invocation {
    let mut arguments = BTreeMap::new();
    arguments.insert(PATH_OPTION.to_owned(), enablement.content_root.as_text().to_owned());
    if command == PHRASE_EXERCISE {
        arguments.insert(PHRASE_OPTION.to_owned(), VERIFICATION_PHRASE.to_owned());
    }
    Invocation {
        arguments,
        detached: false,
        operation_key: requires_operation_key(command).then(|| operation_key.to_owned()),
        output: Some(OutputForm::Machine),
        selection: Selection {
            environment: Some(enablement.environment.clone()),
            profile: Some(enablement.profile.clone()),
        },
        verb: command.to_owned(),
    }
}

/// The one submitted exercise that looks for a phrase.
const PHRASE_EXERCISE: &str = "find_pages_containing_phrase";

/// Whether a verification may run one command.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Admission {
    /// The registry calls it a read that replaces nothing.
    Admissible,
    /// The registry calls it something else.
    Refused(&'static str),
}

/// What the registry calls a command a verification refuses to run.
const WRITES_CONTENT: &str = "a write";

/// What the registry calls a read that can still replace what was visible.
const REPLACES_CONTENT: &str = "destructive";

/// Returns whether a verification may run the command `descriptor` describes.
///
/// Two columns decide it and the third is never read. Intrinsic idempotency
/// says whether running something twice is running it once, which is a question
/// about retries; using it here would let a write that happens to be idempotent
/// look admissible, and that is exactly the mistake this harness exists to make
/// impossible.
#[must_use]
pub fn admission_for(descriptor: &CommandDescriptor) -> Admission {
    match (descriptor.access, descriptor.destructive) {
        (AccessClassification::Write, _) => Admission::Refused(WRITES_CONTENT),
        (AccessClassification::Read, DestructiveClassification::Destructive) => {
            Admission::Refused(REPLACES_CONTENT)
        }
        (AccessClassification::Read, DestructiveClassification::NonDestructive) => {
            Admission::Admissible
        }
    }
}

/// Returns every command a verification may run, in registry order.
#[must_use]
pub fn admissible(catalog: &CommandCatalog) -> Vec<&CommandDescriptor> {
    catalog
        .descriptors()
        .iter()
        .filter(|descriptor| admission_for(descriptor) == Admission::Admissible)
        .collect()
}

/// Returns every command a verification refuses, in registry order.
#[must_use]
pub fn refused(catalog: &CommandCatalog) -> Vec<&CommandDescriptor> {
    catalog
        .descriptors()
        .iter()
        .filter(|descriptor| admission_for(descriptor) != Admission::Admissible)
        .collect()
}

/// Requires one command to be one a verification may run, before any dispatch.
///
/// # Errors
///
/// Returns [`LiveRefusal::UnknownCommand`] for a name the registry does not
/// hold and [`LiveRefusal::NotAdmissible`] for one it holds and refuses, with
/// the command named in both.
pub fn require_admissible(catalog: &CommandCatalog, wire_name: &str) -> Result<(), LiveRefusal> {
    let descriptor =
        catalog.find(wire_name).ok_or_else(|| LiveRefusal::UnknownCommand(wire_name.to_owned()))?;
    match admission_for(descriptor) {
        Admission::Admissible => Ok(()),
        Admission::Refused(classification) => {
            Err(LiveRefusal::NotAdmissible { command: wire_name.to_owned(), classification })
        }
    }
}

/// What the selected agent says about one command.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OfferedCapability {
    /// The canonical contract each role schema is annotated with, by role.
    pub canonical_contract_annotations: BTreeMap<String, String>,
    /// The five fields naming exactly which contract it holds.
    pub identity: SelectedCommandContractIdentity,
}

/// Requires the agent's capability to be the contract this build installed.
///
/// The annotations are authenticated before the identity, and separately from
/// it, because they answer a different question: a role schema written under
/// another byte contract can still have the digest this build expects, and a
/// digest match would then be a coincidence rather than agreement.
///
/// # Errors
///
/// Returns [`LiveRefusal`] naming the command and the exact thing that differs.
pub fn require_agreement(offered: &OfferedCapability) -> Result<(), LiveRefusal> {
    let command = offered.identity.command_wire_name.clone();
    let installed = SelectedCommandContractIdentity::installed(&command)
        .map_err(|failure| contract_unavailable(&command, &failure))?;
    let expected = canonical_contract_digest();
    for role in SchemaRole::both() {
        let named = offered.canonical_contract_annotations.get(role.as_text());
        match named {
            None => {
                return Err(LiveRefusal::CanonicalContractAnnotationAbsent {
                    command,
                    role: role.as_text(),
                });
            }
            Some(named) if *named != expected => {
                return Err(LiveRefusal::CanonicalContractDrift {
                    command,
                    named: named.clone(),
                    role: role.as_text(),
                });
            }
            Some(_) => {}
        }
    }
    require_same_identity(&command, &offered.identity, &installed)
}

/// Returns the refusal one identity failure is.
fn contract_unavailable(command: &str, failure: &ContractIdentityFailure) -> LiveRefusal {
    LiveRefusal::ContractUnavailable { command: command.to_owned(), reason: failure.to_string() }
}

/// Requires all five identity fields to be the same, naming the first that is not.
fn require_same_identity(
    command: &str,
    offered: &SelectedCommandContractIdentity,
    installed: &SelectedCommandContractIdentity,
) -> Result<(), LiveRefusal> {
    let differing = [
        ("wire name", &offered.command_wire_name, &installed.command_wire_name),
        (
            "semantic version",
            &offered.command_semantic_contract_version,
            &installed.command_semantic_contract_version,
        ),
        (
            "limits digest",
            &offered.command_contract_limits_digest,
            &installed.command_contract_limits_digest,
        ),
        ("argument schema", &offered.argument_schema_digest, &installed.argument_schema_digest),
        ("result schema", &offered.result_schema_digest, &installed.result_schema_digest),
    ]
    .into_iter()
    .find(|(_, held, expected)| held != expected);
    match differing {
        None => Ok(()),
        Some((field, _, _)) => {
            Err(LiveRefusal::IdentityDrift { command: command.to_owned(), field })
        }
    }
}

/// What the agent attests about one configuration inspection.
///
/// Every count is exact. "At least one read" and "no more than one acquisition"
/// are different claims from "exactly one", and only the exact ones rule out an
/// implementation that read a value twice, enumerated keys twice, or reached a
/// rejected value at all.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ConfigurationConformance {
    /// Whether a bounded response with no partial handling was produced.
    pub bounded_without_partial_handling: bool,
    /// How many complete keys-only enumerations happened.
    pub complete_keys_only_enumerations: u64,
    /// Whether an escaped lookup through the listing call alone was used.
    pub escaped_listing_only_lookup: bool,
    /// Whether hostile carriers were refused.
    pub hostile_carriers_refused: bool,
    /// Whether classification happened before any value was read.
    pub metatype_and_redaction_before_value: bool,
    /// Whether the persistent identifier was checked again afterwards.
    pub persistent_identifier_postchecked: bool,
    /// How many times the properties were acquired.
    pub property_acquisitions: u64,
    /// How many reads happened for rejected or redacted values.
    pub reads_of_rejected_values: u64,
    /// How many reads happened for each visible value.
    pub reads_of_each_visible_value: u64,
}

/// How many times a conforming inspection acquires or enumerates or reads.
const EXACTLY_ONCE: u64 = 1;

/// How many times a conforming inspection reads a value it rejected.
const NEVER: u64 = 0;

/// How many claims a conformance trace makes.
const CONFORMANCE_CLAIMS: usize = 9;

impl ConfigurationConformance {
    /// Requires this trace to attest a conforming inspection.
    ///
    /// A successful value proves nothing about how it was obtained, so none of
    /// this is inferred from one. Either the agent attests the trace or the
    /// evidence is unsupported.
    ///
    /// # Errors
    ///
    /// Returns [`LiveRefusal::ConformanceNotAttested`] naming the first claim
    /// the trace does not make.
    pub fn require_attested(&self) -> Result<(), LiveRefusal> {
        let claims: [(&'static str, bool); CONFORMANCE_CLAIMS] = [
            ("an escaped listing-only lookup", self.escaped_listing_only_lookup),
            ("a persistent-identifier postcheck", self.persistent_identifier_postchecked),
            ("exactly one property acquisition", self.property_acquisitions == EXACTLY_ONCE),
            (
                "exactly one complete keys-only enumeration",
                self.complete_keys_only_enumerations == EXACTLY_ONCE,
            ),
            ("bounded handling with no partial result", self.bounded_without_partial_handling),
            ("refusal of hostile carriers", self.hostile_carriers_refused),
            ("classification before any value was read", self.metatype_and_redaction_before_value),
            ("no read of a rejected or redacted value", self.reads_of_rejected_values == NEVER),
            (
                "exactly one read of each visible value",
                self.reads_of_each_visible_value == EXACTLY_ONCE,
            ),
        ];
        match claims.into_iter().find(|(_, held)| !held) {
            None => Ok(()),
            Some((claim, _)) => Err(LiveRefusal::ConformanceNotAttested(claim)),
        }
    }
}

/// Which kind of evidence a report carries.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Evidence {
    /// The hermetic suite, which is the release gate.
    HermeticConformance,
    /// One run against one real author, which is not.
    LiveObservation,
}

impl Evidence {
    /// Returns how this evidence is written.
    #[must_use]
    pub fn as_text(self) -> &'static str {
        match self {
            Self::HermeticConformance => "hermetic-conformance",
            Self::LiveObservation => "live-observation",
        }
    }
}

/// Roughly how long something took, without pinning a number nobody can repeat.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DurationCategory {
    /// It answered without a wait worth reporting.
    Immediate,
    /// It took long enough that progress mattered.
    Extended,
}

impl DurationCategory {
    /// Returns how this category is written.
    #[must_use]
    pub fn as_text(self) -> &'static str {
        match self {
            Self::Immediate => "immediate",
            Self::Extended => "extended",
        }
    }
}

/// How one exercise came out.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResultClassification {
    /// It produced the result the contract describes.
    Succeeded,
    /// The author refused it, which is an answer.
    Refused,
    /// Nothing was reachable, which is not.
    Unavailable,
}

impl ResultClassification {
    /// Returns how this classification is written.
    #[must_use]
    pub fn as_text(self) -> &'static str {
        match self {
            Self::Succeeded => "succeeded",
            Self::Refused => "refused",
            Self::Unavailable => "unavailable",
        }
    }
}

/// What one verification observed, and about what.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Report {
    /// Which incarnation of the agent's event store answered.
    pub agent_event_store_generation: u64,
    /// Whether the connector recorded dial attempts to the author alone.
    pub author_only: bool,
    /// Which command was exercised.
    pub command: String,
    /// Which product the target runs.
    pub deployment: AdobeExperienceManagerDeployment,
    /// Roughly how long it took.
    pub duration: DurationCategory,
    /// Which kind of evidence this is.
    pub evidence: Evidence,
    /// Which operation it was, so it can be looked up.
    pub operation_identifier: String,
    /// How it came out.
    pub result: ResultClassification,
    /// The nonsecret identity of what it ran against.
    pub target: String,
}

/// Separator between a report's field name and its value.
const FIELD_SEPARATOR: &str = ": ";

impl Report {
    /// Returns whether this observation says anything about `target`.
    ///
    /// One live run is evidence about the author it ran against and about
    /// nothing else. The next patch level of the same product is a different
    /// target, and answering otherwise would turn one observation into a claim
    /// nobody made.
    #[must_use]
    pub fn covers(&self, target: &str) -> bool {
        self.evidence == Evidence::LiveObservation && self.target == target
    }

    /// Returns this report, one field per line, in a fixed order.
    #[must_use]
    pub fn rendered(&self) -> String {
        let author_only = if self.author_only { "yes" } else { "no" };
        let generation = self.agent_event_store_generation.to_string();
        let lines = [
            ("agent-event-store-generation", generation.as_str()),
            ("author-only", author_only),
            ("command", self.command.as_str()),
            ("deployment", self.deployment.as_text()),
            ("duration", self.duration.as_text()),
            ("evidence", self.evidence.as_text()),
            ("operation", self.operation_identifier.as_str()),
            ("result", self.result.as_text()),
            ("target", self.target.as_str()),
        ];
        let mut rendered = String::new();
        for (named, held) in lines {
            rendered.push_str(named);
            rendered.push_str(FIELD_SEPARATOR);
            rendered.push_str(held);
            rendered.push('\n');
        }
        rendered
    }
}

/// What is said when a verification has no author to verify against.
pub const NO_SELECTED_AUTHOR: &str = "this selection names no author to verify against";

/// The generation a verification records when the daemon named none.
const UNSTATED_GENERATION: u64 = 0;

/// Returns what one exercise observed about the selected author.
#[must_use]
pub fn live_report(command: &str, identifier: &str, facts: &ResolvedFacts, exit: i32) -> Report {
    Report {
        agent_event_store_generation: UNSTATED_GENERATION,
        author_only: true,
        command: command.to_owned(),
        deployment: facts.deployment,
        duration: DurationCategory::Immediate,
        evidence: Evidence::LiveObservation,
        operation_identifier: identifier.to_owned(),
        result: classified(exit),
        target: facts.author_target.clone(),
    }
}

/// Returns how one exit classifies as a result.
fn classified(exit: i32) -> ResultClassification {
    match exit {
        exit_classification::SUCCESS => ResultClassification::Succeeded,
        exit_classification::UNAVAILABLE => ResultClassification::Unavailable,
        _ => ResultClassification::Refused,
    }
}
