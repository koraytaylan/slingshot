//! Turning one invocation into one request.
//!
//! The mirror of reading a daemon's answer, and kept apart from the assembly
//! for the same reason: what a leaf asks for is a question about the
//! vocabulary, and which service asks it is a question about routing. A change
//! to one is not a change to the other.
//!
//! Every builder is asked which command an invocation describes, and the family
//! that owns it answers. Asking them all rather than consulting a table of leaf
//! names keeps one list: each family already knows what it builds, and a second
//! list beside it would be a second thing to keep in step.

use slingshot_domain::command::catalog::Command;
use slingshot_domain::daemon_runtime_contract::{
    DAEMON_OPERATION_PROTOCOL_VERSION, DaemonRuntimeContract,
};
use slingshot_local_protocol::control::HelloResult;
use slingshot_local_protocol::message::OperationRequest;

use crate::application::RunRefusal;
use crate::commands::content::RequestRefusal;
use crate::commands::{
    asset_lifecycle, asset_query, authorizable, configuration, content, content_fragment,
    experience_fragment, package, page_lifecycle, page_mutation, page_query, path_query,
    platform_configuration, replication, replication_queue, resource_mapping, sling_job, workflow,
};
use crate::invocation::{
    ARTIFACT_OPTION, CONTINUATION_TOKEN_OPTION, EXPECTED_CATEGORY_OPTION, EXPECTED_DIGEST_OPTION,
    EXPECTED_REVISION_OPTION, Invocation, LIMIT_OPTION, OPERATION_IDENTIFIER_OPTION,
    RESULT_IDENTIFIER_OPTION, REVIEWED_DIGEST_OPTION, TARGET_DIGEST_OPTION,
};
use crate::operation_maintenance::{MAXIMUM_PAGE_SIZE, MAXIMUM_PREVIEW_LIMIT};

/// Returns the typed command one catalog invocation describes.
///
/// Every builder is asked, and the one whose command this is answers. Asking
/// them all rather than consulting a table of leaf names keeps one list: each
/// family already knows which commands it builds, and a second list beside it
/// would be a second thing to keep in step.
///
/// Every command is asked whether the request contradicts itself before it
/// leaves, so a move into its own subtree, a mutation that changes nothing, or a
/// group asked to contain itself is refused here rather than at the author.
///
/// # Errors
///
/// Returns [`RequestRefusal`] naming the first thing that is wrong, that the
/// request contradicts itself, or that no family builds this command.
pub fn build_command(invocation: &Invocation) -> Result<Command, RequestRefusal> {
    for build in EVERY_COMMAND_BUILDER {
        match build(invocation) {
            Err(RequestRefusal::AnotherCommand { .. }) => continue,
            Ok(built) => {
                built.require_usable().map_err(|why| RequestRefusal::RequestUnusable {
                    named: why.wire_name.to_owned(),
                    because: why.reason,
                })?;
                return Ok(built);
            }
            refused => return refused,
        }
    }
    Err(RequestRefusal::AnotherCommand { named: invocation.verb.clone() })
}

/// One family's builder.
type CommandBuilder = fn(&Invocation) -> Result<Command, RequestRefusal>;

/// Every family that turns an invocation into a typed command.
const EVERY_COMMAND_BUILDER: &[CommandBuilder] = &[
    asset_lifecycle::build,
    asset_query::build,
    authorizable::build,
    configuration::build,
    content::build,
    content_fragment::build,
    experience_fragment::build,
    package::build,
    page_lifecycle::build,
    page_mutation::build,
    page_query::build,
    path_query::build,
    platform_configuration::build,
    replication::build,
    replication_queue::build,
    resource_mapping::build,
    sling_job::build,
    workflow::build,
];

/// Returns the value of one option a leaf cannot act without.
///
/// # Errors
///
/// Returns [`RunRefusal::Usage`] naming the option that is missing.
pub fn required<'invocation>(
    invocation: &'invocation Invocation,
    named: &str,
) -> Result<&'invocation str, RunRefusal> {
    invocation
        .arguments
        .get(named)
        .map(String::as_str)
        .ok_or_else(|| RunRefusal::Usage(format!("{named} names what this command acts on")))
}

/// Returns one option's value read as a whole number.
///
/// # Errors
///
/// Returns [`RunRefusal::Usage`] when the value is not a whole number.
pub fn counted(invocation: &Invocation, named: &str, absent: u64) -> Result<u64, RunRefusal> {
    match invocation.arguments.get(named) {
        None => Ok(absent),
        Some(stated) => {
            stated.parse().map_err(|_| RunRefusal::Usage(format!("{named} takes a whole number")))
        }
    }
}

/// Returns the request one observation leaf describes.
///
/// # Errors
///
/// Returns [`RunRefusal::Usage`] naming the first thing the leaf needs and did
/// not get.
pub fn observation_request(invocation: &Invocation) -> Result<OperationRequest, RunRefusal> {
    let operation_identifier = required(invocation, OPERATION_IDENTIFIER_OPTION)?.to_owned();
    match invocation.verb.as_str() {
        "operation-status" => Ok(OperationRequest::OperationStatus { operation_identifier }),
        "operation-wait" => Ok(OperationRequest::Wait { operation_identifier }),
        "operation-result" => Ok(OperationRequest::Result { operation_identifier }),
        "operation-restart" => Ok(OperationRequest::ResumeOperationRecovery {
            expected_operation_revision: counted(invocation, EXPECTED_REVISION_OPTION, 0)?,
            expected_recovery_category: required(invocation, EXPECTED_CATEGORY_OPTION)?.to_owned(),
            operation_identifier,
        }),
        _ => Ok(OperationRequest::ArtifactRead {
            artifact_identifier: required(invocation, ARTIFACT_OPTION)?.to_owned(),
            expected_content_digest: required(invocation, EXPECTED_DIGEST_OPTION)?.to_owned(),
            operation_identifier,
            preferred_chunk_bytes: preferred_chunk_bytes(),
            starting_byte_offset: 0,
        }),
    }
}

/// Returns the request one maintenance leaf describes.
///
/// # Errors
///
/// Returns [`RunRefusal::Usage`] naming the first thing the leaf needs and did
/// not get.
pub fn maintenance_request(
    invocation: &Invocation,
    partition: &str,
) -> Result<OperationRequest, RunRefusal> {
    let author_target_identity_digest = partition.to_owned();
    match invocation.verb.as_str() {
        "operation-list" => Ok(OperationRequest::ListOperations {
            cursor: invocation.arguments.get(CONTINUATION_TOKEN_OPTION).cloned(),
            lifecycle_states: Vec::new(),
            page_size: paged(invocation, MAXIMUM_PAGE_SIZE)?,
        }),
        "maintenance-preview" => Ok(OperationRequest::TerminalMaintenancePreview {
            author_target_identity_digest,
            maximum_operations: paged(invocation, MAXIMUM_PREVIEW_LIMIT)?,
        }),
        "maintenance-apply" => Ok(OperationRequest::TerminalMaintenanceApply {
            author_target_identity_digest,
            reviewed_manifest_digest: required(invocation, REVIEWED_DIGEST_OPTION)?.to_owned(),
        }),
        _ => Ok(OperationRequest::MaintenanceResultMetadata {
            author_target_identity_digest,
            maintenance_result_identifier: required(invocation, RESULT_IDENTIFIER_OPTION)?
                .to_owned(),
        }),
    }
}

/// Returns the page size one invocation asks for, inside its bound.
///
/// # Errors
///
/// Returns [`RunRefusal::Usage`] when the asked-for page is outside the bound.
pub fn paged(invocation: &Invocation, bound: u64) -> Result<u32, RunRefusal> {
    let asked = counted(invocation, LIMIT_OPTION, bound)?;
    if asked == 0 || asked > bound {
        return Err(RunRefusal::Usage(format!("{LIMIT_OPTION} is between one and {bound}")));
    }
    u32::try_from(asked).map_err(|_| RunRefusal::Usage(format!("{LIMIT_OPTION} is too large")))
}

/// Returns the partition this invocation acts in.
pub fn expected_digest(invocation: &Invocation, hello: &HelloResult) -> String {
    invocation
        .arguments
        .get(TARGET_DIGEST_OPTION)
        .cloned()
        .unwrap_or_else(|| hello.author_target_identity_digest.clone())
}

/// Returns the environment revision this invocation acts under.
pub fn expected_revision(invocation: &Invocation, hello: &HelloResult) -> String {
    invocation
        .arguments
        .get(EXPECTED_REVISION_OPTION)
        .cloned()
        .unwrap_or_else(|| hello.selected_environment_revision.clone())
}

/// Returns how large a chunk this build asks a transfer for.
///
/// The contract's own bound. Asking for more would be clamped anyway, and
/// asking for less would make a large artifact cost more round trips than the
/// daemon and this client have both agreed to pay.
pub fn preferred_chunk_bytes() -> u32 {
    let allowed = DaemonRuntimeContract::embedded().limit("maximum_local_artifact_chunk_bytes");
    u32::try_from(allowed).unwrap_or(u32::MAX)
}

/// Returns the operation-protocol version this build speaks.
///
/// Read from the runtime contract rather than written down here, so a build
/// cannot claim a version its contract does not describe.
pub fn spoken_operation_version() -> u32 {
    u32::try_from(DAEMON_OPERATION_PROTOCOL_VERSION).unwrap_or(u32::MAX)
}
