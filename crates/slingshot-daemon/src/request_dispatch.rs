//! Sending each request to the one service that answers it.
//!
//! Dispatch is deliberately thin. It decides which service a request belongs
//! to, refuses what this daemon cannot serve, and does nothing else - no
//! validation a service would repeat, no state of its own, and no answer it
//! invents. A dispatcher that started answering things would be a second place
//! the daemon's behaviour lived, and the two would drift.
//!
//! Two refusals are its own, and both are about compatibility rather than
//! content. A request in an operation protocol version this daemon does not
//! serve is refused here, because no service could sensibly interpret it. And
//! retained control - hello, ping, status, stop - is answered whatever the
//! operation version says, because a client that cannot run operations still
//! has to be able to find out what it is talking to and ask it to stop. A
//! daemon that refused everything to an incompatible client would leave that
//! client unable to do the one thing that would fix the situation.
//!
//! Once stopping has begun, new work is refused and everything already
//! attached is left alone. Stopping is not cancelling: an operation that is
//! running keeps running, and a client waiting on one keeps waiting until the
//! daemon actually goes.

/// What a client asked the daemon to do.
///
/// Grouped by what answers it rather than by name, because the grouping is the
/// decision this module makes and spelling it out is what makes the decision
/// reviewable.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RequestKind {
    /// Retained control: hello, ping, daemon status, and stop.
    RetainedControl,
    /// Submitting new work.
    Execute,
    /// Asking about work that exists.
    Query,
    /// Waiting on work that exists.
    Wait,
    /// Reading an artifact.
    ArtifactRead,
    /// Previewing or applying maintenance.
    Maintenance,
    /// Resuming a paused operation.
    ResumeRecovery,
}

impl RequestKind {
    /// Returns whether this kind is answered whatever the operation version is.
    ///
    /// Retained control alone. Everything else speaks the operation protocol,
    /// so a client that cannot speak it cannot be answered - but it can still
    /// be told what this daemon is and asked to stop.
    #[must_use]
    pub fn survives_incompatibility(self) -> bool {
        matches!(self, Self::RetainedControl)
    }

    /// Returns whether this kind creates new work.
    ///
    /// The distinction a stopping daemon needs: creating work is refused, and
    /// everything else carries on until the daemon actually goes.
    #[must_use]
    pub fn creates_work(self) -> bool {
        matches!(self, Self::Execute)
    }
}

/// What this daemon is prepared to serve.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DispatchPolicy {
    /// Whether this daemon has begun stopping.
    pub stopping: bool,
    /// The operation protocol versions it can serve, ascending.
    pub supported_operation_versions: Vec<u64>,
}

/// Why a request was refused before it reached a service.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum DispatchRefusal {
    /// This daemon does not speak that operation protocol version.
    #[error(
        "this daemon serves operation versions {supported:?}, and the request is version {asked}"
    )]
    IncompatibleOperationVersion {
        /// What the client asked for.
        asked: u64,
        /// What this daemon serves.
        supported: Vec<u64>,
    },
    /// This daemon is stopping and takes no new work.
    #[error("this daemon is stopping and is taking no new work; work already running continues")]
    Stopping,
}

/// Where one request goes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Dispatch {
    /// To the service that answers this kind.
    Serve(RequestKind),
    /// Nowhere, and this says why.
    Refuse(DispatchRefusal),
}

impl DispatchPolicy {
    /// Returns where one request goes.
    ///
    /// Compatibility is decided before stopping, because an incompatible client
    /// is told something true about this daemon whatever else is happening,
    /// while "stopping" would send it away to reconnect to a successor it also
    /// could not talk to.
    #[must_use]
    pub fn dispatch(&self, kind: RequestKind, operation_version: u64) -> Dispatch {
        if kind.survives_incompatibility() {
            return Dispatch::Serve(kind);
        }
        if !self.supported_operation_versions.contains(&operation_version) {
            return Dispatch::Refuse(DispatchRefusal::IncompatibleOperationVersion {
                asked: operation_version,
                supported: self.supported_operation_versions.clone(),
            });
        }
        if self.stopping && kind.creates_work() {
            return Dispatch::Refuse(DispatchRefusal::Stopping);
        }
        Dispatch::Serve(kind)
    }
}
