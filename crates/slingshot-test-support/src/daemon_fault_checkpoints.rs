//! Where a daemon may be stopped, and what has to be true if it is.
//!
//! A daemon that dies has not misbehaved: power fails, a container is evicted,
//! somebody sends a signal. So every place where durable state changes has a
//! name here, and every name carries the invariant that must hold when the
//! process disappears at that exact point. What makes the inventory useful is
//! that it is closed: a phase nobody named is a phase nobody checked.
//!
//! # The invariant is written down beside the checkpoint
//!
//! Not in a comment somewhere and not in the head of whoever wrote the phase.
//! A checkpoint whose invariant is unstated is a checkpoint whose test can
//! assert anything and still look right.

/// Where a daemon may be stopped mid-way through changing durable state.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum DaemonCheckpoint {
    /// After the state root exists, before the database is opened.
    BeforeDatabaseOpen,
    /// After the database is opened, before migrations are applied.
    BeforeMigrations,
    /// After migrations, before the namespace is owned.
    BeforeOwnership,
    /// After ownership, before the endpoint is bound.
    BeforeEndpointBind,
    /// After the endpoint is bound, before readiness is published.
    BeforeReadinessPublication,
    /// After a request is admitted, before it is scheduled.
    BeforeScheduling,
    /// After an attempt is fenced, before the remote is asked to run it.
    BeforeRemoteSubmission,
    /// After the remote answered, before the answer is settled durably.
    BeforeResultSettlement,
    /// After a stop is authorized, before the endpoint is released.
    BeforeEndpointRelease,
}

/// Every checkpoint, so a suite walks them all rather than remembers them.
pub const EVERY_DAEMON_CHECKPOINT: &[DaemonCheckpoint] = &[
    DaemonCheckpoint::BeforeDatabaseOpen,
    DaemonCheckpoint::BeforeMigrations,
    DaemonCheckpoint::BeforeOwnership,
    DaemonCheckpoint::BeforeEndpointBind,
    DaemonCheckpoint::BeforeReadinessPublication,
    DaemonCheckpoint::BeforeScheduling,
    DaemonCheckpoint::BeforeRemoteSubmission,
    DaemonCheckpoint::BeforeResultSettlement,
    DaemonCheckpoint::BeforeEndpointRelease,
];

impl DaemonCheckpoint {
    /// Returns what must be true if the daemon disappears here.
    #[must_use]
    pub fn invariant(self) -> &'static str {
        match self {
            Self::BeforeDatabaseOpen => "nothing durable exists, so a restart begins cleanly",
            Self::BeforeMigrations => {
                "the database is at whatever version it was, and a restart migrates it"
            }
            Self::BeforeOwnership => "no daemon owns the namespace, so a successor may",
            Self::BeforeEndpointBind => {
                "ownership is held by nobody running, and a successor takes it"
            }
            Self::BeforeReadinessPublication => {
                "no client can have been told this daemon was ready"
            }
            Self::BeforeScheduling => {
                "the request is admitted and unscheduled, so a restart runs it"
            }
            Self::BeforeRemoteSubmission => {
                "the attempt is fenced and unsent, so recovery decides whether it ran"
            }
            Self::BeforeResultSettlement => {
                "the answer is known to nobody durable, so it is asked for again"
            }
            Self::BeforeEndpointRelease => "the stop is authorized, and a successor may bind",
        }
    }

    /// Returns whether a restart at this point may run remote work again.
    ///
    /// Only before anything was sent. Once an attempt has left, whether it ran
    /// is a question for recovery rather than an assumption for a restart, and
    /// a daemon that assumed would run somebody's command twice.
    #[must_use]
    pub fn permits_unconditional_retry(self) -> bool {
        matches!(
            self,
            Self::BeforeDatabaseOpen
                | Self::BeforeMigrations
                | Self::BeforeOwnership
                | Self::BeforeEndpointBind
                | Self::BeforeReadinessPublication
                | Self::BeforeScheduling
        )
    }
}

/// What one chaos run was told to do.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct DaemonFaultPlan {
    /// Where the daemon stops, when it is told to stop anywhere.
    armed: Option<DaemonCheckpoint>,
}

impl DaemonFaultPlan {
    /// Returns a plan that stops nowhere.
    #[must_use]
    pub fn uninterrupted() -> Self {
        Self::default()
    }

    /// Returns a plan that stops at one checkpoint.
    #[must_use]
    pub fn stopping_at(checkpoint: DaemonCheckpoint) -> Self {
        Self { armed: Some(checkpoint) }
    }

    /// Returns whether this run stops at `checkpoint`.
    #[must_use]
    pub fn stops_at(&self, checkpoint: DaemonCheckpoint) -> bool {
        self.armed == Some(checkpoint)
    }

    /// Returns where this run stops, when it stops anywhere.
    #[must_use]
    pub fn armed(&self) -> Option<DaemonCheckpoint> {
        self.armed
    }
}
