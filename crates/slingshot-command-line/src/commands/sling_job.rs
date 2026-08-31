//! Looking at queues and the jobs in them, and stopping one.
//!
//! `--states` is required on the job search for the reason it is required on the
//! workflow search, and `--topic` narrows it further when a caller knows which
//! topic they are worried about.

use slingshot_domain::command::cancel_sling_job::CancelSlingJobCommand;
use slingshot_domain::command::catalog::Command;
use slingshot_domain::command::find_sling_jobs::FindSlingJobsCommand;
use slingshot_domain::command::inspect_sling_job::InspectSlingJobCommand;
use slingshot_domain::command::list_sling_job_queues::ListSlingJobQueuesCommand;
use slingshot_domain::command::process_identity::{
    RequestedSlingJobStates, SlingJobIdentifier, SlingJobState, SlingJobTopic,
};

use crate::commands::content::{RequestRefusal, require_key, required};
use crate::commands::operational_values::{list, optional_text, unusable};
use crate::commands::path_query::window;
use crate::invocation::{Invocation, JOB_OPTION, STATES_OPTION, TOPIC_OPTION};

/// The wire name of the queue listing.
pub const LIST_SLING_JOB_QUEUES: &str = "list_sling_job_queues";

/// The wire name of the job search.
pub const FIND_SLING_JOBS: &str = "find_sling_jobs";

/// The wire name of the job inspection.
pub const INSPECT_SLING_JOB: &str = "inspect_sling_job";

/// The wire name of the cancellation.
pub const CANCEL_SLING_JOB: &str = "cancel_sling_job";

/// Every command this family builds.
const NAMES: &[&str] =
    &[LIST_SLING_JOB_QUEUES, FIND_SLING_JOBS, INSPECT_SLING_JOB, CANCEL_SLING_JOB];

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
        LIST_SLING_JOB_QUEUES => Ok(Command::ListSlingJobQueues(ListSlingJobQueuesCommand {
            result_window: window(invocation)?,
        })),
        FIND_SLING_JOBS => find(invocation),
        INSPECT_SLING_JOB => Ok(Command::InspectSlingJob(InspectSlingJobCommand {
            job_identifier: job(invocation)?,
        })),
        _ => {
            Ok(Command::CancelSlingJob(CancelSlingJobCommand { job_identifier: job(invocation)? }))
        }
    }
}

/// Returns the job search one invocation describes.
fn find(invocation: &Invocation) -> Result<Command, RequestRefusal> {
    let states: Vec<SlingJobState> = list(invocation, STATES_OPTION)?;
    let topic = optional_text(invocation, TOPIC_OPTION)
        .map(|stated| SlingJobTopic::parse(&stated).map_err(|_| unusable(TOPIC_OPTION)))
        .transpose()?;
    Ok(Command::FindSlingJobs(FindSlingJobsCommand {
        result_window: window(invocation)?,
        states: RequestedSlingJobStates::new(states).map_err(|_| unusable(STATES_OPTION))?,
        topic,
    }))
}

/// Returns the job one invocation names.
fn job(invocation: &Invocation) -> Result<SlingJobIdentifier, RequestRefusal> {
    SlingJobIdentifier::parse(required(invocation, JOB_OPTION)?).map_err(|_| unusable(JOB_OPTION))
}
