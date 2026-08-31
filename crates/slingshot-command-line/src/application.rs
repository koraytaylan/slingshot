//! Composing the whole command surface into one runnable application.
//!
//! Assembly lives apart from the pieces it assembles, so changing what a
//! command does never means editing the thing that runs it. What this owns is
//! the routing: which invocation reaches which service, that exactly one does,
//! and that one final rendering decision and one exit come out the other end.
//!
//! # One invocation reaches exactly one service
//!
//! Not zero and not two. A leaf that fell through would leave a caller with a
//! successful exit and nothing done; one that reached two would perform an
//! unowned side effect on the way to the one it meant. The routing is therefore
//! total over the invocation vocabulary and the suite counts the services each
//! path reached.
//!
//! # Provenance is checked before a versioned service, never after
//!
//! Everything that talks to a daemon goes through one gate, so a build whose
//! runtime or transport contract has moved cannot reach a versioned service by
//! taking a path somebody forgot to guard.

use crate::invocation::{Invocation, METADATA_ONLY_LEAVES, is_catalog_command};
use crate::target_selection::{NAMESPACE_ONLY_LEAVES, TargetRequirement, requirement_of};

/// Which service one invocation reaches.
///
/// Closed, and one per invocation. A vocabulary that admitted two would let a
/// leaf perform an unowned side effect on the way to the one it meant.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Service {
    /// Answering out of this build alone.
    Metadata,
    /// Reading configuration and the files it names.
    ConfigurationCheck,
    /// Finding, starting, or stopping a daemon.
    DaemonLifecycle,
    /// Submitting a catalog command.
    OperationSubmission,
    /// Reading or releasing an operation.
    OperationObservation,
    /// Listing operations, or previewing and applying maintenance.
    OperationMaintenance,
}

impl Service {
    /// Returns whether reaching this service talks to a versioned daemon.
    ///
    /// The two that do not are the reason the distinction exists: they must
    /// keep working when a daemon is absent or incompatible, which is exactly
    /// when somebody runs them.
    #[must_use]
    pub fn is_versioned(self) -> bool {
        !matches!(self, Self::Metadata | Self::ConfigurationCheck | Self::DaemonLifecycle)
    }
}

/// The observation leaves, which read or release one operation.
pub const OBSERVATION_LEAVES: &[&str] = &[
    "operation-artifact",
    "operation-restart",
    "operation-result",
    "operation-status",
    "operation-wait",
];

/// The leaves that list or maintain.
pub const MAINTENANCE_LEAVES: &[&str] =
    &["maintenance-apply", "maintenance-preview", "maintenance-result", "operation-list"];

/// Why one invocation reaches nothing.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum DispatchRefusal {
    /// The leaf is not one this application routes.
    #[error("{named} is not a command this application routes")]
    Unroutable {
        /// What was asked for.
        named: String,
    },
    /// The build's provenance does not match the daemon's.
    #[error("this build and that daemon disagree about the contracts they were built against")]
    ProvenanceRefused,
}

/// Returns the one service `invocation` reaches.
///
/// # Errors
///
/// Returns [`DispatchRefusal::Unroutable`] for a leaf this application does not
/// route, which is a defect rather than a caller's mistake: the parser refuses
/// unknown leaves before anything reaches here.
pub fn service_for(invocation: &Invocation) -> Result<Service, DispatchRefusal> {
    let leaf = invocation.verb.as_str();
    if METADATA_ONLY_LEAVES.contains(&leaf) {
        return Ok(Service::Metadata);
    }
    if leaf == "check-configuration" {
        return Ok(Service::ConfigurationCheck);
    }
    if NAMESPACE_ONLY_LEAVES.contains(&leaf) || leaf == "daemon-start" {
        return Ok(Service::DaemonLifecycle);
    }
    if OBSERVATION_LEAVES.contains(&leaf) {
        return Ok(Service::OperationObservation);
    }
    if MAINTENANCE_LEAVES.contains(&leaf) {
        return Ok(Service::OperationMaintenance);
    }
    if is_catalog_command(leaf) {
        return Ok(Service::OperationSubmission);
    }
    Err(DispatchRefusal::Unroutable { named: leaf.to_owned() })
}

/// Requires one invocation to be allowed to reach the service it routes to.
///
/// Provenance is checked here, once, rather than in each service. A gate per
/// service is a gate somebody eventually forgets, and the path they forget is
/// the one that reaches a versioned daemon without agreeing with it.
///
/// # Errors
///
/// Returns [`DispatchRefusal::ProvenanceRefused`] when a versioned service is
/// asked for and the contracts do not agree.
pub fn require_dispatchable(
    invocation: &Invocation,
    provenance_agrees: bool,
) -> Result<Service, DispatchRefusal> {
    let service = service_for(invocation)?;
    if service.is_versioned() && !provenance_agrees {
        return Err(DispatchRefusal::ProvenanceRefused);
    }
    Ok(service)
}

/// Returns whether reaching `service` needs a complete target.
///
/// Read from the same table the target resolution uses rather than restated, so
/// a leaf cannot need one here and not there.
#[must_use]
pub fn needs_complete_target(invocation: &Invocation) -> bool {
    matches!(requirement_of(&invocation.verb), TargetRequirement::Complete)
}
