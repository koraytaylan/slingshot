//! Looking at replication agents and their queues, and moving one along.
//!
//! `--agent` names which agent everywhere. The flush takes an optional
//! `--expected-entry-count`, which is the whole reason it is safe to run on a
//! queue somebody is still looking at: state what you saw, and a queue that grew
//! since then refuses instead of emptying more than you meant.

use slingshot_domain::command::catalog::Command;
use slingshot_domain::command::flush_replication_queue::FlushReplicationQueueCommand;
use slingshot_domain::command::inspect_replication_queue::InspectReplicationQueueCommand;
use slingshot_domain::command::platform_service_identity::{
    ReplicationAgentIdentifier, ReplicationQueueEntryIdentifier,
};
use slingshot_domain::command::replication_agent::{
    InspectReplicationAgentCommand, ListReplicationAgentsCommand,
};
use slingshot_domain::command::retry_replication_queue_entry::RetryReplicationQueueEntryCommand;

use crate::commands::content::{RequestRefusal, require_key, required};
use crate::commands::operational_values::{optional_count, unusable};
use crate::commands::path_query::window;
use crate::invocation::{AGENT_OPTION, ENTRY_OPTION, EXPECTED_ENTRY_COUNT_OPTION, Invocation};

/// The wire name of the agent listing.
pub const LIST_REPLICATION_AGENTS: &str = "list_replication_agents";

/// The wire name of the agent inspection.
pub const INSPECT_REPLICATION_AGENT: &str = "inspect_replication_agent";

/// The wire name of the queue inspection.
pub const INSPECT_REPLICATION_QUEUE: &str = "inspect_replication_queue";

/// The wire name of the flush.
pub const FLUSH_REPLICATION_QUEUE: &str = "flush_replication_queue";

/// The wire name of the retry.
pub const RETRY_REPLICATION_QUEUE_ENTRY: &str = "retry_replication_queue_entry";

/// Every command this family builds.
const NAMES: &[&str] = &[
    LIST_REPLICATION_AGENTS,
    INSPECT_REPLICATION_AGENT,
    INSPECT_REPLICATION_QUEUE,
    FLUSH_REPLICATION_QUEUE,
    RETRY_REPLICATION_QUEUE_ENTRY,
];

/// Returns the typed request one invocation describes.
///
/// # Errors
///
/// Returns [`RequestRefusal`] naming the first thing that is wrong, or that this
/// family builds no such command.
pub fn build(invocation: &Invocation) -> Result<Command, RequestRefusal> {
    if !NAMES.contains(&invocation.verb.as_str()) {
        return Err(RequestRefusal::AnotherCommand { named: invocation.verb.clone() });
    }
    require_key(invocation)?;
    match invocation.verb.as_str() {
        LIST_REPLICATION_AGENTS => {
            Ok(Command::ListReplicationAgents(ListReplicationAgentsCommand {
                result_window: window(invocation)?,
            }))
        }
        INSPECT_REPLICATION_AGENT => {
            Ok(Command::InspectReplicationAgent(InspectReplicationAgentCommand {
                agent_identifier: agent(invocation)?,
            }))
        }
        INSPECT_REPLICATION_QUEUE => {
            Ok(Command::InspectReplicationQueue(InspectReplicationQueueCommand {
                agent_identifier: agent(invocation)?,
                result_window: window(invocation)?,
            }))
        }
        FLUSH_REPLICATION_QUEUE => {
            Ok(Command::FlushReplicationQueue(FlushReplicationQueueCommand {
                agent_identifier: agent(invocation)?,
                expected_entry_count: optional_count(invocation, EXPECTED_ENTRY_COUNT_OPTION)?,
            }))
        }
        _ => Ok(Command::RetryReplicationQueueEntry(RetryReplicationQueueEntryCommand {
            agent_identifier: agent(invocation)?,
            entry_identifier: ReplicationQueueEntryIdentifier::parse(required(
                invocation,
                ENTRY_OPTION,
            )?)
            .map_err(|_| unusable(ENTRY_OPTION))?,
        })),
    }
}

/// Returns the agent one invocation names.
fn agent(invocation: &Invocation) -> Result<ReplicationAgentIdentifier, RequestRefusal> {
    ReplicationAgentIdentifier::parse(required(invocation, AGENT_OPTION)?)
        .map_err(|_| unusable(AGENT_OPTION))
}
