//! What one command invocation is, before anything acts on it.
//!
//! Parsing is separated from execution completely, and the separation is
//! load-bearing rather than tidy. An invocation that cannot be built is a usage
//! problem the caller can fix by reading what they typed; one that can is a
//! request nothing has refused yet. Between those two states nothing is opened,
//! read, started, or connected to, so a mistyped flag never leaves a process
//! behind or a half-written file.
//!
//! # Options belong to leaves, not to the line
//!
//! Output form, detachment, and the operation key are attached to the leaves
//! that can honour them rather than to every invocation. A global `--detach`
//! would be inherited by a leaf that has nothing to detach from, and later by a
//! standard-stream server that must never see one at all. Naming the leaf that
//! owns each option is how that stays true as leaves are added.
//!
//! # A key is required exactly where a repeat would be a second effect
//!
//! Whether a command needs a caller-supplied operation key is not a property of
//! this surface: it is the catalog's own idempotency classification, read here
//! rather than restated. A command that changes something is refused without a
//! key, before anything external is touched, because the key is what makes a
//! retry the same request rather than another one.

use std::collections::BTreeMap;

use slingshot_domain::command::catalog::CommandCatalog;

/// The option naming which profile to use.
pub const PROFILE_OPTION: &str = "--profile";

/// The option naming which environment to use.
pub const ENVIRONMENT_OPTION: &str = "--environment";

/// The option choosing machine-readable output.
pub const MACHINE_OUTPUT_OPTION: &str = "--machine";

/// The option that submits without waiting.
pub const DETACH_OPTION: &str = "--detach";

/// The option carrying the caller's operation key.
pub const OPERATION_KEY_OPTION: &str = "--operation-key";

/// The option naming a historical target partition.
pub const TARGET_DIGEST_OPTION: &str = "--author-target-digest";

/// The option naming the operation revision a resume was written against.
pub const EXPECTED_REVISION_OPTION: &str = "--expected-revision";

/// The option naming the recovery category a resume releases.
pub const EXPECTED_CATEGORY_OPTION: &str = "--expected-category";

/// The option carrying the manifest digest an apply quotes.
pub const REVIEWED_DIGEST_OPTION: &str = "--reviewed-digest";

/// The option bounding how much a maintenance preview selects.
pub const LIMIT_OPTION: &str = "--limit";

/// The option naming the instant a maintenance preview selects before.
pub const BEFORE_OPTION: &str = "--before";

/// The option naming a maintenance result.
pub const RESULT_IDENTIFIER_OPTION: &str = "--result-identifier";

/// The option naming the digest a read expects.
pub const EXPECTED_DIGEST_OPTION: &str = "--expected-digest";

/// The option naming where a fetched thing is written.
pub const DESTINATION_OPTION: &str = "--destination";

/// The option naming a repository path a command acts on.
pub const PATH_OPTION: &str = "--path";

/// The option naming how far below a resource a command reaches.
pub const DEPTH_OPTION: &str = "--depth";

/// The option that includes everything below a named path.
pub const RECURSIVE_OPTION: &str = "--recursive";

/// The option naming the stem a produced package is named from.
pub const PACKAGE_NAME_OPTION: &str = "--package-name";

/// The option naming the subtrees a package holds.
pub const ROOTS_OPTION: &str = "--roots";

/// The option naming the subtrees a package admits, in order.
pub const INCLUDE_OPTION: &str = "--include";

/// The option naming the subtrees a package removes, in order.
pub const EXCLUDE_OPTION: &str = "--exclude";

/// The option naming an exact primary node type a match must have.
pub const NODE_TYPE_OPTION: &str = "--node-type";

/// The option naming where in a result set to start.
pub const OFFSET_OPTION: &str = "--offset";

/// The option carrying an opaque continuation token.
pub const CONTINUATION_TOKEN_OPTION: &str = "--continuation-token";

/// The option carrying one canonical predicate object.
pub const PROPERTY_PREDICATE_OPTION: &str = "--property-predicate";

/// The option naming a template a page must record.
pub const TEMPLATE_OPTION: &str = "--template";

/// The option carrying exactly what to look for.
pub const PHRASE_OPTION: &str = "--phrase";

/// The option naming component resource types, separated by commas.
pub const RESOURCE_TYPES_OPTION: &str = "--resource-types";

/// The option choosing whether every named component must be used.
pub const MATCH_ALL_OPTION: &str = "--match-all";

/// The option naming which configuration to inspect.
pub const PERSISTENT_IDENTIFIER_OPTION: &str = "--persistent-identifier";

/// Every option this surface knows, in the order a reference lists them.
pub const EVERY_OPTION: &[&str] = &[
    PROFILE_OPTION,
    ENVIRONMENT_OPTION,
    MACHINE_OUTPUT_OPTION,
    DETACH_OPTION,
    OPERATION_KEY_OPTION,
    TARGET_DIGEST_OPTION,
    EXPECTED_REVISION_OPTION,
    EXPECTED_CATEGORY_OPTION,
    REVIEWED_DIGEST_OPTION,
    LIMIT_OPTION,
    BEFORE_OPTION,
    RESULT_IDENTIFIER_OPTION,
    EXPECTED_DIGEST_OPTION,
    DESTINATION_OPTION,
    PATH_OPTION,
    DEPTH_OPTION,
    RECURSIVE_OPTION,
    PACKAGE_NAME_OPTION,
    ROOTS_OPTION,
    INCLUDE_OPTION,
    EXCLUDE_OPTION,
    NODE_TYPE_OPTION,
    OFFSET_OPTION,
    CONTINUATION_TOKEN_OPTION,
    PROPERTY_PREDICATE_OPTION,
    TEMPLATE_OPTION,
    PHRASE_OPTION,
    RESOURCE_TYPES_OPTION,
    MATCH_ALL_OPTION,
    PERSISTENT_IDENTIFIER_OPTION,
];

/// The leaves this surface offers that are not catalog commands.
///
/// Written down because the parser has to tell a local leaf from a command
/// without asking the catalog, and a list is easier to review than a match
/// spread across a parser.
pub const LOCAL_LEAVES: &[&str] = &[
    "check-configuration",
    "daemon-ping",
    "daemon-start",
    "daemon-status",
    "daemon-stop",
    "help",
    "maintenance-apply",
    "maintenance-preview",
    "maintenance-result",
    "operation-artifact",
    "operation-list",
    "operation-restart",
    "operation-result",
    "operation-status",
    "operation-wait",
    "version",
];

/// The leaves that submit work and may therefore detach.
pub const SUBMITTING_LEAVES: &[&str] = &[];

/// The leaves a historical target partition may be named on.
pub const HISTORICAL_LEAVES: &[&str] = &[
    "maintenance-apply",
    "maintenance-preview",
    "maintenance-result",
    "operation-artifact",
    "operation-list",
    "operation-result",
    "operation-status",
];

/// The leaves that answer without reaching anything at all.
pub const METADATA_ONLY_LEAVES: &[&str] = &["help", "version"];

/// How an outcome is written.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OutputForm {
    /// For a person reading a terminal.
    Human,
    /// For something that will parse it.
    Machine,
}

/// Which profile and environment a command speaks to.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Selection {
    /// Which environment, when the caller named one.
    pub environment: Option<String>,
    /// Which profile, when the caller named one.
    pub profile: Option<String>,
}

/// One parsed invocation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Invocation {
    /// The options the leaf takes, by name.
    pub arguments: BTreeMap<String, String>,
    /// Whether it submits and returns without waiting.
    pub detached: bool,
    /// The caller's operation key, when the leaf takes one.
    pub operation_key: Option<String>,
    /// How the outcome is written, when the leaf writes one.
    pub output: Option<OutputForm>,
    /// Which profile and environment it speaks to.
    pub selection: Selection,
    /// Which leaf it is.
    pub verb: String,
}

impl Invocation {
    /// Returns whether this invocation answers without reaching anything.
    #[must_use]
    pub fn is_metadata_only(&self) -> bool {
        METADATA_ONLY_LEAVES.contains(&self.verb.as_str())
    }
}

/// Why one argument vector is not an invocation.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ParseRefusal {
    /// Nothing was asked for.
    #[error("a command line names one leaf, and this names none")]
    NoLeaf,
    /// The leaf is not one this build offers.
    #[error("{named} is not a command this build offers")]
    UnknownLeaf {
        /// What was asked for.
        named: String,
    },
    /// The option is not one this build knows.
    #[error("{named} is not an option this build knows")]
    UnknownOption {
        /// What was asked for.
        named: String,
    },
    /// The option takes a value and none followed it.
    #[error("{named} takes a value, and none followed it")]
    MissingValue {
        /// Which option.
        named: String,
    },
    /// The option was given twice.
    #[error("{named} is given once, and this gives it again")]
    RepeatedOption {
        /// Which option.
        named: String,
    },
    /// The option belongs to a leaf this is not.
    #[error("{named} belongs to another leaf, and {leaf} does not take it")]
    OptionNotOnThisLeaf {
        /// Which leaf was asked for.
        leaf: String,
        /// Which option was given.
        named: String,
    },
    /// The leaf changes something and no key was supplied.
    #[error("{leaf} changes something, so a repeat needs the key that makes it the same request")]
    OperationKeyRequired {
        /// Which leaf was asked for.
        leaf: String,
    },
    /// The leaf requires an option and none was given.
    #[error("{leaf} requires {named}")]
    RequiredOptionMissing {
        /// Which leaf was asked for.
        leaf: String,
        /// Which option is missing.
        named: String,
    },
}

/// Returns the invocation `arguments` describe.
///
/// Nothing outside this function's arguments is consulted: no configuration, no
/// file, no process, no socket. That is what lets a usage mistake cost nothing
/// and a successful parse mean only that the request is well formed.
///
/// # Errors
///
/// Returns [`ParseRefusal`] naming the first thing that is wrong.
pub fn parse(arguments: &[String]) -> Result<Invocation, ParseRefusal> {
    let (leaf, rest) = arguments.split_first().ok_or(ParseRefusal::NoLeaf)?;
    require_known_leaf(leaf)?;
    let mut invocation = Invocation {
        arguments: BTreeMap::new(),
        detached: false,
        operation_key: None,
        output: None,
        selection: Selection::default(),
        verb: leaf.clone(),
    };
    let mut position = 0;
    while position < rest.len() {
        let option = &rest[position];
        require_option_permitted(leaf, option)?;
        require_unrepeated(&invocation, option)?;
        position += 1 + absorb(&mut invocation, option, rest.get(position + 1))?;
    }
    require_complete(&invocation)?;
    Ok(invocation)
}

/// Returns whether `leaf` is a catalog command rather than a local leaf.
#[must_use]
pub fn is_catalog_command(leaf: &str) -> bool {
    CommandCatalog::published().find(leaf).is_some()
}

/// Returns whether `leaf` needs a caller-supplied operation key.
///
/// Read from the catalog's own idempotency classification rather than restated
/// here, so a command that becomes destructive does not also have to be
/// remembered in a second list.
#[must_use]
pub fn requires_operation_key(leaf: &str) -> bool {
    CommandCatalog::published()
        .find(leaf)
        .is_some_and(|descriptor| descriptor.intrinsic_idempotency.requires_operation_key())
}

/// Requires `leaf` to be one this build offers.
fn require_known_leaf(leaf: &str) -> Result<(), ParseRefusal> {
    if LOCAL_LEAVES.contains(&leaf) || is_catalog_command(leaf) {
        return Ok(());
    }
    Err(ParseRefusal::UnknownLeaf { named: leaf.to_owned() })
}

/// Requires `option` to be one `leaf` takes.
fn require_option_permitted(leaf: &str, option: &str) -> Result<(), ParseRefusal> {
    if !EVERY_OPTION.contains(&option) {
        return Err(ParseRefusal::UnknownOption { named: option.to_owned() });
    }
    if leaves_taking(option).iter().any(|held| held == leaf) {
        return Ok(());
    }
    Err(ParseRefusal::OptionNotOnThisLeaf { leaf: leaf.to_owned(), named: option.to_owned() })
}

/// Returns which leaves take `option`.
///
/// A table rather than a chain of conditions, because "which leaves take this"
/// is the question a reader has and the question a reference answers, and the
/// two should be reading the same thing.
#[must_use]
pub fn leaves_taking(option: &str) -> Vec<String> {
    let named = |leaves: &[&str]| leaves.iter().map(|leaf| (*leaf).to_owned()).collect();
    match option {
        DETACH_OPTION | OPERATION_KEY_OPTION => catalog_leaves(),
        TARGET_DIGEST_OPTION => named(HISTORICAL_LEAVES),
        EXPECTED_REVISION_OPTION | EXPECTED_CATEGORY_OPTION => named(&["operation-restart"]),
        REVIEWED_DIGEST_OPTION => named(&["maintenance-apply"]),
        BEFORE_OPTION => named(&["maintenance-preview", "operation-list"]),
        LIMIT_OPTION => {
            let mut leaves = named(&["maintenance-preview", "operation-list"]);
            leaves.extend(catalog_leaves());
            leaves
        }
        RESULT_IDENTIFIER_OPTION | EXPECTED_DIGEST_OPTION => named(&["maintenance-result"]),
        DESTINATION_OPTION => named(&["maintenance-result", "operation-artifact"]),
        PATH_OPTION
        | DEPTH_OPTION
        | RECURSIVE_OPTION
        | PACKAGE_NAME_OPTION
        | ROOTS_OPTION
        | INCLUDE_OPTION
        | EXCLUDE_OPTION
        | NODE_TYPE_OPTION
        | OFFSET_OPTION
        | CONTINUATION_TOKEN_OPTION
        | PROPERTY_PREDICATE_OPTION
        | TEMPLATE_OPTION
        | PHRASE_OPTION
        | RESOURCE_TYPES_OPTION
        | MATCH_ALL_OPTION
        | PERSISTENT_IDENTIFIER_OPTION => catalog_leaves(),
        _ => every_leaf_that_reaches_somewhere(),
    }
}

/// Returns every catalog command, as leaf names.
fn catalog_leaves() -> Vec<String> {
    CommandCatalog::published()
        .descriptors()
        .iter()
        .map(|descriptor| descriptor.wire_name.clone())
        .collect()
}

/// Returns every leaf that has somewhere to reach.
fn every_leaf_that_reaches_somewhere() -> Vec<String> {
    LOCAL_LEAVES
        .iter()
        .filter(|leaf| !METADATA_ONLY_LEAVES.contains(leaf))
        .map(|leaf| (*leaf).to_owned())
        .chain(catalog_leaves())
        .collect()
}

/// Requires `option` not to have been given already.
fn require_unrepeated(invocation: &Invocation, option: &str) -> Result<(), ParseRefusal> {
    let given = match option {
        PROFILE_OPTION => invocation.selection.profile.is_some(),
        ENVIRONMENT_OPTION => invocation.selection.environment.is_some(),
        MACHINE_OUTPUT_OPTION => invocation.output.is_some(),
        DETACH_OPTION => invocation.detached,
        OPERATION_KEY_OPTION => invocation.operation_key.is_some(),
        other => invocation.arguments.contains_key(other),
    };
    if given {
        return Err(ParseRefusal::RepeatedOption { named: option.to_owned() });
    }
    Ok(())
}

/// Records one option, and returns how many values it consumed.
fn absorb(
    invocation: &mut Invocation,
    option: &str,
    value: Option<&String>,
) -> Result<usize, ParseRefusal> {
    match option {
        MACHINE_OUTPUT_OPTION => {
            invocation.output = Some(OutputForm::Machine);
            return Ok(0);
        }
        RECURSIVE_OPTION | MATCH_ALL_OPTION => {
            invocation.arguments.insert(option.to_owned(), String::new());
            return Ok(0);
        }
        DETACH_OPTION => {
            invocation.detached = true;
            return Ok(0);
        }
        _ => {}
    }
    let value = value
        .filter(|held| !held.starts_with("--"))
        .ok_or_else(|| ParseRefusal::MissingValue { named: option.to_owned() })?;
    match option {
        PROFILE_OPTION => invocation.selection.profile = Some(value.clone()),
        ENVIRONMENT_OPTION => invocation.selection.environment = Some(value.clone()),
        OPERATION_KEY_OPTION => invocation.operation_key = Some(value.clone()),
        other => {
            invocation.arguments.insert(other.to_owned(), value.clone());
        }
    }
    Ok(1)
}

/// Requires one parsed invocation to carry everything its leaf needs.
fn require_complete(invocation: &Invocation) -> Result<(), ParseRefusal> {
    let leaf = invocation.verb.as_str();
    if requires_operation_key(leaf) && invocation.operation_key.is_none() {
        return Err(ParseRefusal::OperationKeyRequired { leaf: leaf.to_owned() });
    }
    for required in required_options(leaf) {
        if !invocation.arguments.contains_key(*required) {
            return Err(ParseRefusal::RequiredOptionMissing {
                leaf: leaf.to_owned(),
                named: (*required).to_owned(),
            });
        }
    }
    Ok(())
}

/// Returns the options `leaf` cannot do without.
#[must_use]
pub fn required_options(leaf: &str) -> &'static [&'static str] {
    match leaf {
        "operation-restart" => &[EXPECTED_REVISION_OPTION, EXPECTED_CATEGORY_OPTION],
        "maintenance-apply" => &[REVIEWED_DIGEST_OPTION],
        "maintenance-result" => {
            &[RESULT_IDENTIFIER_OPTION, EXPECTED_DIGEST_OPTION, DESTINATION_OPTION]
        }
        _ => &[],
    }
}
