//! Running one protocol tool call as the invocation a command line would make.
//!
//! The protocol server and the command line reach the same daemon through the
//! same application, so a tool call is turned into the invocation its arguments
//! describe and run through it. A second path to an author would be a second
//! place the same checks live, and the two would eventually disagree about what
//! a request did.
//!
//! It lives in its own file because the command line is over the file ceiling
//! with it inlined, and because what it owns is one thing: the translation from
//! a protocol call to an invocation, and the product's own boundaries behind it.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::AtomicBool;

use serde_json::Value;
use slingshot_domain::command::catalog::{Command, CommandCatalog};
use slingshot_local_protocol::foundation_contract::FoundationContract;

use crate::application::{Answer, CommandLineApplication, Provenance};
use crate::command_line::{
    ProductConfiguration, ProductDaemon, ProductFilesystem, ProductNetwork, ProductProcess,
    ProductRequestIdentity, ProductSignals, runtime_root,
};
use crate::invocation::{
    ARTIFACT_OPTION, ENVIRONMENT_OPTION, EXPECTED_DIGEST_OPTION, Invocation,
    OPERATION_IDENTIFIER_OPTION, OutputForm, PROFILE_OPTION, SERVE_LEAF, Selection,
    TARGET_DIGEST_OPTION,
};
use crate::machine_outcome_envelope::MachineOutcomeEnvelope;
use crate::model_context_protocol::operation_execution::{
    self, FetchedArtifact, ResourceNamespace, ToolRunner,
};
use crate::model_context_protocol::schema_projection;
use crate::model_context_protocol::tool_catalog::{KeyPresence, ToolDescriptor};
use crate::target_selection::namespace_of;

/// The leaf that reads one operation's artifact.
const ARTIFACT_LEAF_NAME: &str = "operation-artifact";

/// The leaf that reads one operation's committed result.
const RESULT_LEAF_NAME: &str = "operation-result";

/// Runs one tool call as the invocation a command line would have made.
///
/// The protocol server and the command line reach the same daemon through the
/// same application, so a tool call is turned into the invocation its
/// arguments describe and run through it. A second path to an author would be
/// a second place the same checks live, and the two would eventually disagree
/// about what a request did.
struct ProductToolRunner {
    /// The contract every frame is written under.
    contract: FoundationContract,
    /// Where this run's daemon objects live.
    runtime_root: PathBuf,
    /// Which profile and environment the process was started for.
    selection: Selection,
    /// The executable a daemon would be started from, when one is.
    executable: PathBuf,
    /// Cancels this local request without cancelling retained remote work.
    cancellation: Arc<AtomicBool>,
}

impl ProductToolRunner {
    /// Runs `work` with the application a command line would have assembled.
    ///
    /// One composition, so a tool call and an artifact read cannot reach the
    /// daemon through two arrangements of the same boundaries - which would be
    /// two places a check could differ.
    fn with_application<Produced>(
        &self,
        work: impl FnOnce(&CommandLineApplication<'_>) -> Produced,
    ) -> Produced {
        let contract = &self.contract;
        let executable = &self.executable;
        let request_identity = ProductRequestIdentity;
        let configuration = ProductConfiguration;
        let filesystem = ProductFilesystem;
        let network = ProductNetwork;
        let signals = ProductSignals::from_flag(Arc::clone(&self.cancellation));
        let daemon = ProductDaemon::new(contract, &self.runtime_root, signals.flag());
        let process = ProductProcess {
            contract,
            executable: executable.to_path_buf(),
            runtime_root: self.runtime_root.clone(),
        };
        work(&CommandLineApplication {
            request_identity: &request_identity,
            configuration: &configuration,
            daemon: &daemon,
            filesystem: &filesystem,
            network: &network,
            process: &process,
            provenance: Provenance::embedded(),
            signals: &signals,
        })
    }
}

impl ToolRunner for ProductToolRunner {
    fn run(
        &mut self,
        tool: &ToolDescriptor,
        arguments: &Value,
    ) -> Result<MachineOutcomeEnvelope, String> {
        let invocation = tool_invocation(tool, arguments, &self.selection)?;
        self.with_application(|application| match application.run(&invocation).answer {
            Answer::Envelope(envelope) => Ok(*envelope),
            Answer::Refusal(message) => Err(message),
            Answer::Text(text) => {
                Err(format!("{} answered text rather than an outcome: {text}", tool.name))
            }
        })
    }

    fn artifact_bytes(
        &mut self,
        namespace: &ResourceNamespace,
        operation_identifier: &str,
        artifact_identifier: &str,
        maximum_bytes: u64,
    ) -> Result<FetchedArtifact, String> {
        // The address names the target it is about, and the daemon that serves
        // it is selected by the profile and environment the address carries.
        // Assuming this process's own selection would answer an address for one
        // target with another target's bytes.
        self.with_application(|application| {
            // A resource address knows only which artifact it names, and an
            // artifact read quotes the digest it expects. The digest is
            // resolved from the operation's own committed result before the
            // transfer, the same metadata-then-read order a maintenance result
            // uses, and never derived from the address or carried over from an
            // earlier answer.
            let expected_content_digest =
                declared_digest(application, namespace, operation_identifier, artifact_identifier)?;
            let invocation = artifact_read_invocation(
                namespace,
                operation_identifier,
                artifact_identifier,
                &expected_content_digest,
            );
            let (declared, bytes) = application
                .artifact_bytes(&invocation, maximum_bytes)
                .map_err(|refusal| refusal.message().to_owned())?;
            if declared.artifact_identifier != artifact_identifier {
                return Err(format!(
                    "the daemon answered an artifact read for {} with {}",
                    artifact_identifier, declared.artifact_identifier
                ));
            }
            Ok(FetchedArtifact {
                artifact_identifier: declared.artifact_identifier,
                author_target_identity_digest: namespace.author_target_identity_digest.clone(),
                byte_length: declared.byte_length,
                content_digest: declared.content_digest,
                media_type: declared.media_type,
                bytes,
            })
        })
    }
}

/// Returns the digest the daemon declared for one artifact of one operation.
///
/// The daemon holds no metadata route for operation artifacts, so the
/// operation's own committed result is the document that describes them: an
/// over-inline result is answered as an access entry carrying its descriptor,
/// and a command that produced a package or loaded-content artifact names it in
/// its inline result. The digest comes from that result rather than from the
/// address, an identifier derivation, or anything this process remembered.
///
/// # Errors
///
/// Returns what stopped the resolution, in words a caller can act on.
fn declared_digest(
    application: &CommandLineApplication<'_>,
    namespace: &ResourceNamespace,
    operation_identifier: &str,
    artifact_identifier: &str,
) -> Result<String, String> {
    let invocation = operation_result_invocation(namespace, operation_identifier);
    let envelope = match application.run(&invocation).answer {
        Answer::Envelope(envelope) => *envelope,
        Answer::Refusal(message) => return Err(message),
        Answer::Text(text) => {
            return Err(format!("operation-result answered text rather than an outcome: {text}"));
        }
    };
    declared_in(&envelope, artifact_identifier).ok_or_else(|| {
        format!("operation {operation_identifier} declares no artifact named {artifact_identifier}")
    })
}

/// Returns the digest one outcome declares for one artifact.
///
/// Three shapes can name an artifact, and each names it in the one place its
/// own schema declares: an over-inline result carries the access entry for its
/// `structured_result` slot, a projected command result carries the entries it
/// replaced descriptors with, and an inline result carries the descriptor under
/// its `artifact` member. Nothing is inferred from a digest-shaped value found
/// somewhere else in a document, because a result may legitimately hold a
/// document of its own.
fn declared_in(envelope: &MachineOutcomeEnvelope, artifact_identifier: &str) -> Option<String> {
    match envelope {
        MachineOutcomeEnvelope::StructuredResultArtifactAccess { artifact }
            if artifact.artifact_identifier == artifact_identifier =>
        {
            Some(artifact.content_digest.clone())
        }
        MachineOutcomeEnvelope::CommandArtifactAccess { artifacts, .. } => artifacts
            .iter()
            .find(|access| access.artifact_identifier == artifact_identifier)
            .map(|access| access.content_digest.clone()),
        MachineOutcomeEnvelope::OperationResult { result } => {
            descriptor_digest(result.get("artifact")?, artifact_identifier)
        }
        _ => None,
    }
}

/// Returns the digest one artifact descriptor declares for one artifact.
///
/// A descriptor is a closed shape of six members, and requiring all six keeps a
/// document that merely resembles one from being read as an artifact's
/// description. The identifier must also be the one the address named, so a
/// result naming several artifacts cannot be answered with the wrong one's
/// digest.
fn descriptor_digest(descriptor: &Value, artifact_identifier: &str) -> Option<String> {
    let members = descriptor.as_object()?;
    let named = members.get("identifier").and_then(Value::as_str) == Some(artifact_identifier);
    let shaped = members.contains_key("slot")
        && members.contains_key("media_type")
        && members.contains_key("byte_length")
        && members.contains_key("digest")
        && members.contains_key("suggested_file_name");
    let digest = members.get("digest").and_then(Value::as_str)?;
    let canonical = slingshot_local_protocol::message::digest_is_canonical(digest);
    (named && shaped && canonical).then(|| digest.to_owned())
}

/// Returns the invocation one resource address describes as an artifact read.
///
/// The address carries everything the read needs: which profile and environment
/// name the daemon, which target digests the partition, which artifact the bytes
/// belong to, and the digest the operation's own result declared for it. What
/// this client verifies is the length and digest the daemon declares before the
/// transfer begins.
///
/// # Panics
///
/// Panics when the address carries a profile or environment this build cannot
/// name, which no parsed resource address does.
fn artifact_read_invocation(
    namespace: &ResourceNamespace,
    operation_identifier: &str,
    artifact_identifier: &str,
    expected_content_digest: &str,
) -> Invocation {
    let mut arguments = BTreeMap::new();
    arguments.insert(OPERATION_IDENTIFIER_OPTION.to_owned(), operation_identifier.to_owned());
    arguments.insert(ARTIFACT_OPTION.to_owned(), artifact_identifier.to_owned());
    arguments.insert(EXPECTED_DIGEST_OPTION.to_owned(), expected_content_digest.to_owned());
    arguments
        .insert(TARGET_DIGEST_OPTION.to_owned(), namespace.author_target_identity_digest.clone());
    Invocation {
        arguments,
        command: None,
        detached: false,
        operation_key: None,
        output: Some(OutputForm::Machine),
        selection: Selection {
            environment: Some(namespace.environment.clone()),
            profile: Some(namespace.profile.clone()),
        },
        verb: ARTIFACT_LEAF_NAME.to_owned(),
    }
}

/// Returns the invocation that reads one operation's committed result.
fn operation_result_invocation(
    namespace: &ResourceNamespace,
    operation_identifier: &str,
) -> Invocation {
    let mut arguments = BTreeMap::new();
    arguments.insert(OPERATION_IDENTIFIER_OPTION.to_owned(), operation_identifier.to_owned());
    arguments
        .insert(TARGET_DIGEST_OPTION.to_owned(), namespace.author_target_identity_digest.clone());
    Invocation {
        arguments,
        command: None,
        detached: false,
        operation_key: None,
        output: Some(OutputForm::Machine),
        selection: Selection {
            environment: Some(namespace.environment.clone()),
            profile: Some(namespace.profile.clone()),
        },
        verb: RESULT_LEAF_NAME.to_owned(),
    }
}

/// Returns what a protocol server's tool calls run through.
///
/// A run whose selection or runtime root cannot be resolved still serves the
/// protocol, and every call it cannot run is told why rather than failing
/// without a reason.
pub(crate) fn tool_runner(
    invocation: &Invocation,
    executable: &Path,
    cancellation: Arc<AtomicBool>,
) -> Box<dyn ToolRunner> {
    match (runtime_root(invocation), namespace_of(&invocation.selection)) {
        (Ok(root), Ok(_)) => Box::new(ProductToolRunner {
            contract: FoundationContract::embedded(),
            runtime_root: root,
            selection: invocation.selection.clone(),
            executable: executable.to_path_buf(),
            cancellation,
        }),
        (Err(reason), _) => Box::new(UnavailableToolRunner { reason }),
        (_, Err(refusal)) => Box::new(UnavailableToolRunner {
            reason: format!(
                "this server was started without a usable target ({refusal}); start it with \
                     `{SERVE_LEAF} {PROFILE_OPTION} <profile> {ENVIRONMENT_OPTION} <environment>`"
            ),
        }),
    }
}

/// Answers every tool call with the one reason this server cannot run any.
struct UnavailableToolRunner {
    /// Why no call can reach a daemon, in words a caller can act on.
    reason: String,
}

impl ToolRunner for UnavailableToolRunner {
    fn run(
        &mut self,
        _tool: &ToolDescriptor,
        _arguments: &Value,
    ) -> Result<MachineOutcomeEnvelope, String> {
        Err(self.reason.clone())
    }
}

/// Returns the invocation one tool call describes.
///
/// The two kinds of tool are answered differently because they are different
/// things. A registry command carries the command's own argument document,
/// which is exactly what the tool's declared `inputSchema` describes, so a call
/// becomes the typed command the schema names rather than a command line spelled
/// with options: `root_path` is the command's member and `--path` is the option
/// that fills it, and renaming one into the other would be inventing a mapping
/// the registry never declared. A control is not a command at all - it is one of
/// the observation or maintenance leaves - so its declared members are mapped to
/// the options those leaves read, in one table both sides can find.
///
/// # Errors
///
/// Returns what stopped the call, in words a caller can act on.
pub fn tool_invocation(
    tool: &ToolDescriptor,
    arguments: &Value,
    selection: &Selection,
) -> Result<Invocation, String> {
    let held = arguments.as_object().ok_or_else(|| "a tool call carries an object".to_owned())?;
    let detached = schema_projection::detached(arguments);
    // A key the caller supplies identifies an operation whose rerun must be
    // that same operation, which is exactly what a command that cannot repeat
    // harmlessly needs. A command that may omit its key is given one by the
    // command line instead: the declared schema offers the member as optional,
    // so a caller's spelling of it is a preference rather than a request the
    // command line has a rule for.
    let operation_key = match tool.operation_key {
        KeyPresence::Required => operation_execution::supplied_key(arguments).map(str::to_owned),
        KeyPresence::Optional | KeyPresence::Absent => None,
    };
    let is_command = CommandCatalog::published().find(&tool.name).is_some();
    let (arguments, command) = if is_command {
        (BTreeMap::new(), Some(tool_command(tool, held)?))
    } else {
        (schema_projection::control_options(tool, arguments)?, None)
    };
    Ok(Invocation {
        arguments,
        command,
        detached,
        operation_key,
        output: Some(OutputForm::Machine),
        selection: selection.clone(),
        verb: tool.name.clone(),
    })
}

/// Returns the typed command one tool call's arguments describe.
///
/// The arguments are the command's own document, so this is a deserialization
/// rather than a translation: a member the command does not declare is an error
/// the command itself names, and a value outside the command's own vocabulary
/// fails where the command's own constructor would have refused it.
fn tool_command(
    tool: &ToolDescriptor,
    held: &serde_json::Map<String, Value>,
) -> Result<Command, String> {
    let mut document = held.clone();
    document.remove(schema_projection::OPERATION_KEY_MEMBER);
    document.remove(schema_projection::DETACHED_MEMBER);
    document.insert("command".to_owned(), Value::String(tool.name.clone()));
    serde_json::from_value::<Command>(Value::Object(document))
        .map_err(|failure| format!("{} did not accept these arguments: {failure}", tool.name))
}

#[cfg(test)]
mod digest_resolution_tests {
    use super::*;
    use crate::machine_outcome_envelope::ArtifactAccess;

    /// A canonical digest for the fixtures.
    const DIGEST: &str = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";

    /// How many bytes the fixture artifacts hold.
    ///
    /// Nothing reads it as a length: it is one descriptor member that has to be
    /// present for the shape to be a descriptor at all.
    const FIXTURE_BYTE_LENGTH: u64 = 17;

    /// A second canonical digest, distinct from the first.
    const OTHER_DIGEST: &str = "fedcba9876543210fedcba9876543210fedcba9876543210fedcba9876543210";

    /// Returns one artifact descriptor document as a result writes it.
    fn descriptor(identifier: &str, digest: &str) -> Value {
        serde_json::json!({
            "identifier": identifier,
            "slot": "content_package",
            "media_type": "application/zip",
            "byte_length": FIXTURE_BYTE_LENGTH,
            "digest": digest,
            "suggested_file_name": "package.zip",
        })
    }

    #[test]
    fn an_inline_result_answers_the_descriptor_it_names() {
        let envelope = MachineOutcomeEnvelope::OperationResult {
            result: serde_json::json!({
                "artifact": descriptor("package-one", DIGEST),
                "disposition": "artifact",
                "path": "/content",
            }),
        };
        assert_eq!(declared_in(&envelope, "package-one").as_deref(), Some(DIGEST));
        assert_eq!(
            declared_in(&envelope, "package-two"),
            None,
            "a result cannot answer for an artifact it does not name"
        );
    }

    #[test]
    fn an_artifact_access_entry_answers_only_its_own_identifier() {
        let envelope = MachineOutcomeEnvelope::StructuredResultArtifactAccess {
            artifact: ArtifactAccess {
                artifact_identifier: "structured_result".to_owned(),
                author_target_identity_digest: DIGEST.to_owned(),
                byte_length: FIXTURE_BYTE_LENGTH,
                content_digest: DIGEST.to_owned(),
                media_type: "application/json".to_owned(),
                operation_identifier: "operation".to_owned(),
                uri: "slingshot://profiles/local/environments/author/targets/x/operations/operation/artifacts/structured_result".to_owned(),
            },
        };
        assert_eq!(declared_in(&envelope, "structured_result").as_deref(), Some(DIGEST));
        assert_eq!(declared_in(&envelope, "content_package"), None);
    }

    #[test]
    fn a_document_that_merely_resembles_a_descriptor_is_not_one() {
        let envelope = MachineOutcomeEnvelope::OperationResult {
            result: serde_json::json!({
                "document": {
                    "identifier": "package-one",
                    "digest": DIGEST,
                    "slot": "content_package",
                }
            }),
        };
        assert_eq!(
            declared_in(&envelope, "package-one"),
            None,
            "a descriptor carries all six members or it is not one"
        );
    }

    #[test]
    fn a_descriptor_whose_digest_is_not_canonical_answers_nothing() {
        let envelope = MachineOutcomeEnvelope::OperationResult {
            result: serde_json::json!({
                "artifact": descriptor("package-one", "NOT-A-DIGEST"),
            }),
        };
        assert_eq!(declared_in(&envelope, "package-one"), None);
    }

    #[test]
    fn several_artifacts_answer_each_with_its_own_digest() {
        let envelope = MachineOutcomeEnvelope::OperationResult {
            result: serde_json::json!({
                "artifact": descriptor("package-one", DIGEST),
                "alternative": descriptor("package-two", OTHER_DIGEST),
            }),
        };
        assert_eq!(declared_in(&envelope, "package-one").as_deref(), Some(DIGEST));
        assert_eq!(declared_in(&envelope, "package-two"), None);
    }

    /// Returns the namespace the invocation fixtures address.
    fn namespace() -> ResourceNamespace {
        ResourceNamespace {
            author_target_identity_digest: OTHER_DIGEST.to_owned(),
            environment: "author".to_owned(),
            profile: "local".to_owned(),
        }
    }

    #[test]
    fn an_artifact_read_quotes_the_digest_it_resolved() {
        // The leaf cannot act without the digest, so an invocation that omits
        // it is one no artifact read would ever run: the server would answer a
        // local failure for every address a client holds. This pins the option
        // and the target the address named.
        let invocation =
            artifact_read_invocation(&namespace(), "operation", "structured_result", DIGEST);
        assert_eq!(invocation.verb, ARTIFACT_LEAF_NAME);
        assert_eq!(
            invocation.arguments.get(EXPECTED_DIGEST_OPTION).map(String::as_str),
            Some(DIGEST)
        );
        assert_eq!(
            invocation.arguments.get(ARTIFACT_OPTION).map(String::as_str),
            Some("structured_result")
        );
        assert_eq!(
            invocation.arguments.get(OPERATION_IDENTIFIER_OPTION).map(String::as_str),
            Some("operation")
        );
        assert_eq!(
            invocation.arguments.get(TARGET_DIGEST_OPTION).map(String::as_str),
            Some(OTHER_DIGEST)
        );
        assert_eq!(invocation.selection.profile.as_deref(), Some("local"));
        assert_eq!(invocation.selection.environment.as_deref(), Some("author"));
    }

    #[test]
    fn the_result_read_that_resolves_a_digest_names_only_the_operation_and_target() {
        // The digest resolution reads the operation's own committed result, so
        // it carries the identity of what it reads and nothing that would
        // describe an artifact read.
        let invocation = operation_result_invocation(&namespace(), "operation");
        assert_eq!(invocation.verb, RESULT_LEAF_NAME);
        assert_eq!(
            invocation.arguments.get(OPERATION_IDENTIFIER_OPTION).map(String::as_str),
            Some("operation")
        );
        assert_eq!(
            invocation.arguments.get(TARGET_DIGEST_OPTION).map(String::as_str),
            Some(OTHER_DIGEST)
        );
        assert!(
            !invocation.arguments.contains_key(EXPECTED_DIGEST_OPTION),
            "a result read is not an artifact read"
        );
    }
}
