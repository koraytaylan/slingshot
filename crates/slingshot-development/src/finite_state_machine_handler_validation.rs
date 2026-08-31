//! What a handler table has to say before this product will act on it.
//!
//! The external executor owns the grammar of its own handlers and decides what
//! is a valid table. This validates the part that is about Slingshot: which
//! tool a handler calls, what identity it passes, and whether the executable it
//! names is one this machine can actually run. Neither side recreates the
//! other's judgement, and the split is deliberate - a validator that also
//! accepted the grammar would be a second implementation of it, drifting.
//!
//! # Nothing is defaulted on this side
//!
//! The executor has defaults for retry and for advance payloads. Copying them
//! here would mean two places deciding what an omitted field means, and the
//! copies would diverge exactly when somebody changed one. So a handler this
//! product acts on spells out all four retry members and, for every advance,
//! its payload and its stamps; an omission is refused rather than filled in.
//!
//! # A key names a command effect, and only a command effect
//!
//! A registry command carries the workflow's operation key, because a rerun of
//! that occurrence must be the same operation. A maintenance control carries
//! none: it is identified by its target and its reviewed digest, and a key
//! would invent an operation identity for something that has none - which is
//! the whole reason maintenance results are addressed the way they are.

use std::collections::BTreeSet;
use std::path::Path;

use serde::Deserialize;
use serde_json::Value;

/// The format a handler table declares.
pub const HANDLER_FORMAT: &str = "fsm.handlers/1";

/// The member a registry command handler passes the workflow's key in.
pub const OPERATION_KEY_MEMBER: &str = "operation_key";

/// Members a maintenance control handler may never carry.
pub const REFUSED_MAINTENANCE_MEMBERS: &[&str] =
    &[OPERATION_KEY_MEMBER, "operation_identifier", "artifact_identifier"];

/// The members a retry policy spells out, all of them, every time.
pub const RETRY_MEMBERS: &[&str] =
    &["attempts", "backoff_ms", "initial_delay_ms", "maximum_delay_ms"];

/// The members every advance spells out.
pub const ADVANCE_MEMBERS: &[&str] = &["payload", "stamps"];

/// Arguments this product does not accept from a handler.
///
/// A wait time belongs to the executor, which owns the handler deadline. A
/// second one here would mean two timers deciding when one call has taken too
/// long, and whichever fired first would make the other one a lie.
pub const REFUSED_ARGUMENTS: &[&str] = &["wait_ms", "wait_seconds", "timeout_ms"];

/// The shortest handler deadline this product acts under, in milliseconds.
pub const LEAST_HANDLER_TIMEOUT_MILLISECONDS: u64 = 1_000;

/// The longest handler deadline this product acts under, in milliseconds.
pub const MOST_HANDLER_TIMEOUT_MILLISECONDS: u64 = 3_600_000;

/// The most attempts a retry policy may declare.
pub const MOST_RETRY_ATTEMPTS: u64 = 16;

/// One handler, as a table declares it.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Handler {
    /// The arguments the tool is called with.
    pub arguments: Value,
    /// What this handler is a handler for.
    pub effect: String,
    /// What runs it, with its executable first.
    pub argv: Vec<String>,
    /// What kind of handler it is.
    pub kind: String,
    /// What happens when it fails.
    pub on_failed: String,
    /// What happens when it succeeds.
    pub on_ok: String,
    /// What it retries, and how.
    pub retry: Value,
    /// How long it may take.
    pub timeout_ms: u64,
    /// Which tool it calls.
    pub tool: String,
    /// What each advance carries.
    #[serde(default)]
    pub advances: Vec<Value>,
}

/// Why one handler is not one this product acts on.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum HandlerRefusal {
    /// The table declares another format.
    #[error("a handler table declares {HANDLER_FORMAT}, and this declares {0}")]
    ForeignFormat(String),
    /// The handler names no executable, or one this machine cannot run.
    #[error("{detail}")]
    ExecutableUnusable {
        /// What is wrong with it.
        detail: String,
    },
    /// The tool is not one this build offers.
    #[error("{0} is not a tool this build offers")]
    ToolUnknown(String),
    /// A registry command carries no operation key.
    #[error("{0} is a registry command, and a rerun of it is the same operation only with a key")]
    KeyAbsent(String),
    /// A maintenance control carries an identity it has none of.
    #[error("{named} is a maintenance control and carries {member}, which it has none of")]
    IdentityInvented {
        /// Which member.
        member: String,
        /// Which handler.
        named: String,
    },
    /// A retry policy leaves something to a default.
    #[error("{0} is left to a default, and two places deciding that would disagree")]
    LeftToADefault(String),
    /// A bound is outside what this product acts under.
    #[error("{named} is {held}, outside what this product acts under")]
    OutsideItsBound {
        /// How much it is.
        held: u64,
        /// Which bound.
        named: String,
    },
    /// An argument names something this product does not accept.
    #[error("{0} belongs to the executor, and a second one here would be a second timer")]
    ArgumentRefused(String),
}

/// Whether a handler calls a registry command or a maintenance control.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ToolKind {
    /// A registry command, which starts durable work.
    RegistryCommand,
    /// A maintenance control, which starts none.
    MaintenanceControl,
    /// Another control, which starts none either.
    Observation,
}

/// Returns which kind of tool one handler calls.
///
/// # Errors
///
/// Returns [`HandlerRefusal::ToolUnknown`] for a tool this build does not
/// offer, because a handler naming one would fail at the first call rather than
/// at validation, and by then the workflow has started.
pub fn kind_of(tool: &str, offered: &BTreeSet<String>) -> Result<ToolKind, HandlerRefusal> {
    if !offered.contains(tool) {
        return Err(HandlerRefusal::ToolUnknown(tool.to_owned()));
    }
    if tool.starts_with("maintenance-") {
        return Ok(ToolKind::MaintenanceControl);
    }
    if tool.starts_with("operation-") {
        return Ok(ToolKind::Observation);
    }
    Ok(ToolKind::RegistryCommand)
}

/// Requires one handler to be one this product acts on.
///
/// Every check is about this side of the boundary. Whether the table is a valid
/// handler table at all is the executor's judgement, made by the executor.
///
/// # Errors
///
/// Returns [`HandlerRefusal`] naming the first thing that stops the handler.
pub fn require_actionable(
    handler: &Handler,
    offered: &BTreeSet<String>,
) -> Result<ToolKind, HandlerRefusal> {
    require_runnable_executable(handler)?;
    let kind = kind_of(&handler.tool, offered)?;
    require_identity(handler, kind)?;
    require_spelled_out(handler)?;
    require_bounds(handler)?;
    require_no_refused_argument(handler)?;
    Ok(kind)
}

/// Requires the handler's own executable to be one this machine can run.
fn require_runnable_executable(handler: &Handler) -> Result<(), HandlerRefusal> {
    let Some(named) = handler.argv.first() else {
        return Err(HandlerRefusal::ExecutableUnusable {
            detail: "a handler names what runs it".to_owned(),
        });
    };
    slingshot_test_support::finite_state_machine_executable::FiniteStateMachineExecutable::at(
        Path::new(named),
    )
    .map(|_| ())
    .map_err(|failure| HandlerRefusal::ExecutableUnusable { detail: failure.to_string() })
}

/// Requires the handler to carry the identity its kind of tool has.
fn require_identity(handler: &Handler, kind: ToolKind) -> Result<(), HandlerRefusal> {
    let carried = |member: &str| !handler.arguments[member].is_null();
    match kind {
        ToolKind::RegistryCommand => {
            if carried(OPERATION_KEY_MEMBER) {
                Ok(())
            } else {
                Err(HandlerRefusal::KeyAbsent(handler.tool.clone()))
            }
        }
        ToolKind::MaintenanceControl => {
            for member in REFUSED_MAINTENANCE_MEMBERS {
                if carried(member) {
                    return Err(HandlerRefusal::IdentityInvented {
                        member: (*member).to_owned(),
                        named: handler.tool.clone(),
                    });
                }
            }
            Ok(())
        }
        ToolKind::Observation => Ok(()),
    }
}

/// Requires everything this product reads to be written down rather than defaulted.
fn require_spelled_out(handler: &Handler) -> Result<(), HandlerRefusal> {
    for member in RETRY_MEMBERS {
        if handler.retry[member].is_null() {
            return Err(HandlerRefusal::LeftToADefault(format!("retry.{member}")));
        }
    }
    for (position, advance) in handler.advances.iter().enumerate() {
        for member in ADVANCE_MEMBERS {
            if advance[member].is_null() {
                return Err(HandlerRefusal::LeftToADefault(format!(
                    "advances[{position}].{member}"
                )));
            }
        }
    }
    Ok(())
}

/// Requires every declared bound to be one this product acts under.
fn require_bounds(handler: &Handler) -> Result<(), HandlerRefusal> {
    if !(LEAST_HANDLER_TIMEOUT_MILLISECONDS..=MOST_HANDLER_TIMEOUT_MILLISECONDS)
        .contains(&handler.timeout_ms)
    {
        return Err(HandlerRefusal::OutsideItsBound {
            held: handler.timeout_ms,
            named: "timeout_ms".to_owned(),
        });
    }
    let attempts = handler.retry["attempts"].as_u64().unwrap_or_default();
    if attempts == 0 || attempts > MOST_RETRY_ATTEMPTS {
        return Err(HandlerRefusal::OutsideItsBound {
            held: attempts,
            named: "retry.attempts".to_owned(),
        });
    }
    Ok(())
}

/// Requires the arguments to name nothing this product does not accept.
fn require_no_refused_argument(handler: &Handler) -> Result<(), HandlerRefusal> {
    let Some(members) = handler.arguments.as_object() else {
        return Ok(());
    };
    for refused in REFUSED_ARGUMENTS {
        if members.contains_key(*refused) {
            return Err(HandlerRefusal::ArgumentRefused((*refused).to_owned()));
        }
    }
    Ok(())
}

// ------------------------------------------------- what a key is made out of

/// The format the key preimage declares.
pub const KEY_PREIMAGE_FORMAT: &str = "slingshot.workflow-effect-operation-key/1";

/// What every key begins with.
pub const KEY_PREFIX: &str = "slingshot-workflow-effect-1-";

/// How many bytes one input may carry.
pub const MOST_INPUT_UTF8_BYTES: usize = 128;

/// How many bytes one suffix may carry.
pub const MOST_SUFFIX_BYTES: usize = 15;

/// How many bytes one key may carry.
pub const MOST_KEY_BYTES: usize = 107;

/// Every suffix a key may carry, and no others.
///
/// Two, and the second one exists for exactly one thing: the compensating
/// effect, which acts on the same occurrence as the effect it compensates and
/// must not be mistaken for it.
pub const EVERY_SUFFIX: &[&str] = &["", "-backup-restore"];

/// Why one key cannot be derived.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum KeyRefusal {
    /// An input is empty, and an empty name names nothing.
    #[error("{0} is empty, and a key derived from nothing identifies nothing")]
    Empty(String),
    /// An input is longer than the contract admits.
    #[error("{named} carries {held} bytes, past the {MOST_INPUT_UTF8_BYTES} the contract admits")]
    TooLong {
        /// How many bytes it carries.
        held: usize,
        /// Which input.
        named: String,
    },
    /// An input carries a code point the contract refuses.
    #[error("{0} carries a control code point, which no name this contract admits carries")]
    ControlCodePoint(String),
    /// The suffix is not one of the two.
    #[error("{0} is not a suffix this contract admits")]
    SuffixUnknown(String),
}

/// Returns the exact preimage one occurrence hashes.
///
/// One object, no whitespace, members in byte order, and the integer in minimal
/// base ten. Every one of those is load-bearing: two implementations that
/// agreed on the members and differed on their order would derive different
/// keys for the same occurrence, and the retry that was meant to attach to
/// existing work would start new work instead.
///
/// # Errors
///
/// Returns [`KeyRefusal`] naming the first input the contract refuses.
pub fn key_preimage(
    workflow_operation_namespace: &str,
    instance_request_identifier: &str,
    occurrence: u64,
) -> Result<String, KeyRefusal> {
    require_admitted("workflow_operation_namespace", workflow_operation_namespace)?;
    require_admitted("instance_request_identifier", instance_request_identifier)?;
    Ok(format!(
        "{{\"format\":\"{KEY_PREIMAGE_FORMAT}\",\"instance_request_identifier\":\"{}\",\
         \"occurrence\":{occurrence},\"workflow_operation_namespace\":\"{}\"}}",
        escaped(instance_request_identifier),
        escaped(workflow_operation_namespace)
    ))
}

/// Returns one input with the two characters that need escaping escaped.
///
/// Only two. Every other admitted code point is emitted as itself, because a
/// contract that escaped more would have to say exactly which more, and two
/// implementations would eventually disagree about the list.
fn escaped(input: &str) -> String {
    input.replace('\\', "\\\\").replace('"', "\\\"")
}

/// Requires one input to be a name this contract admits.
fn require_admitted(named: &str, input: &str) -> Result<(), KeyRefusal> {
    if input.is_empty() {
        return Err(KeyRefusal::Empty(named.to_owned()));
    }
    if input.len() > MOST_INPUT_UTF8_BYTES {
        return Err(KeyRefusal::TooLong { held: input.len(), named: named.to_owned() });
    }
    let controlled =
        input.chars().any(|held| matches!(held, '\u{0}'..='\u{1f}' | '\u{7f}'..='\u{9f}'));
    if controlled {
        return Err(KeyRefusal::ControlCodePoint(named.to_owned()));
    }
    Ok(())
}

/// Returns the key one command-effect occurrence carries.
///
/// The same occurrence always derives the same key, whatever happened in
/// between: that is what makes a retry the same operation and a restart
/// transparent. Two deliberate occurrences, or two stores with their own
/// namespaces, derive different keys and therefore start different work.
///
/// # Errors
///
/// Returns [`KeyRefusal`] naming the first input the contract refuses.
pub fn workflow_effect_operation_key(
    workflow_operation_namespace: &str,
    instance_request_identifier: &str,
    occurrence: u64,
    suffix: &str,
) -> Result<String, KeyRefusal> {
    if !EVERY_SUFFIX.contains(&suffix) || suffix.len() > MOST_SUFFIX_BYTES {
        return Err(KeyRefusal::SuffixUnknown(suffix.to_owned()));
    }
    let preimage =
        key_preimage(workflow_operation_namespace, instance_request_identifier, occurrence)?;
    use sha2::Digest;
    let mut digest = sha2::Sha256::new();
    digest.update(preimage.as_bytes());
    let held: String = digest.finalize().iter().map(|byte| format!("{byte:02x}")).collect();
    Ok(format!("{KEY_PREFIX}{held}{suffix}"))
}
