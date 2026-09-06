//! The composition a product build actually runs work through.
//!
//! Every piece it needs exists already and is proved on its own: deriving and
//! sending a submission, supervising a filtered stream, reconciling by
//! snapshot, believing a result, fetching an artifact. What this adds is the
//! order they happen in and the single decision each stage is allowed to make,
//! so that an execution ends exactly once and never ends on something that was
//! not proved.
//!
//! # The ports are injected because the network is not the subject
//!
//! What the author says is somebody else's problem; what this daemon concludes
//! from it is this module's. So the four things that talk to the author are a
//! trait, and the suite drives them directly. That is not a testing
//! convenience: it is the same separation that lets the executor be composed at
//! startup, before any connection exists, and refuse work it must not resume.
//!
//! # Nothing unresolved is reported as an ending
//!
//! A submission whose fate is unclear, a stream that dropped, an artifact that
//! is not there yet - each is outstanding work with a recovery category, not a
//! failure. Reporting one as terminal would settle an operation on this
//! daemon's own difficulty, which is the mistake the whole recovery vocabulary
//! exists to make hard.

use slingshot_agent_connection::authentication::environment_provider::SelectedAuthorConnection;
use slingshot_agent_connection::selected_author_transport::{
    SelectedAuthorTransport, SelectedAuthorTransportFailure,
};
use slingshot_domain::command::catalog::Command;
use slingshot_domain::operation::{
    OperationExecutionCertainty, RecoveryCategory, RecoveryExecutionEvidence, RecoveryFact,
    TerminalFailure, TerminalFailureDisposition, TerminalFailureKind,
};
use slingshot_domain::operation_executor::{
    ExecutionFuture, ExecutionIdentity, OperationExecutor, OperationExecutorOutcome,
    ProducedArtifact, ProgressPort,
};

use crate::operation::remote_submission::HandoffDisposition;

/// What the executor says while it is working.
pub const SUBMITTING_DETAIL: &str = "submitting to the author";

/// What it says once the agent holds the work.
pub const SUPERVISING_DETAIL: &str = "supervising the agent's event stream";

/// What it says while it is fetching what the work produced.
pub const COMPLETING_DETAIL: &str = "completing the artifacts the result declares";

/// How the agent's answer about one execution came out.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AgentSettlement {
    /// It ran and produced this.
    Succeeded {
        /// The canonical result, when it is small enough to travel inline.
        inline_result: Option<String>,
    },
    /// It ran and did not succeed.
    Failed {
        /// Which published category it failed under.
        category: String,
    },
    /// It provably did not run.
    NotExecuted {
        /// Which published category refused it.
        category: String,
    },
    /// Nobody knows yet, and this says what is outstanding.
    Outstanding {
        /// The complete recovery decision, including post-success evidence,
        /// attempt budget, pause and the original retry-time anchor.
        recovery: RecoveryFact,
    },
    /// A validated terminal decision whose effect disposition must not be
    /// reconstructed from a category string by the executor.
    Terminal {
        /// The complete authoritative or fail-closed terminal decision.
        failure: TerminalFailure,
    },
}

/// The durable completion decision after remote success has been established.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ArtifactCompletion {
    /// All declared artifacts have been verified and published.
    Published {
        /// Published artifacts in slot order.
        artifacts: Vec<ProducedArtifact>,
    },
    /// Acquisition remains pending under its saved recovery policy.
    Recovery {
        /// Complete recovery evidence, pause state and retry timing.
        recovery: RecoveryFact,
    },
    /// Validated evidence establishes that the successful result cannot be
    /// acquired. This never means that the remote command failed or did not run.
    Unavailable,
}

/// The four things this executor needs an author for.
///
/// A trait rather than a client, because the order and the conclusions are the
/// subject here and the transport is not. Each returns what this daemon may
/// conclude, never a wire value.
pub trait AuthorPorts: ::core::fmt::Debug {
    /// Derives and sends the submission for `identity`.
    fn submit<'a>(
        &'a self,
        identity: &'a ExecutionIdentity,
        command: &'a Command,
    ) -> ExecutionFuture<'a, HandoffDisposition>;

    /// Waits for the agent to say what became of it.
    fn settle<'a>(
        &'a self,
        identity: &'a ExecutionIdentity,
    ) -> ExecutionFuture<'a, AgentSettlement>;

    /// Fetches and publishes everything the result declared.
    ///
    /// Preserves the completion owner's durable recovery or unavailability
    /// decision instead of reducing every refusal to a new generic retry.
    fn complete_artifacts<'a>(
        &'a self,
        identity: &'a ExecutionIdentity,
    ) -> ExecutionFuture<'a, ArtifactCompletion>;
}

/// The protocol work a product author adapter performs after it has bound an
/// operation to the selected author connection.
///
/// This is deliberately below [`ProductAuthorPorts`].  A protocol
/// implementation receives neither a caller-selected endpoint nor loose
/// target/revision strings: the only connection context it can receive is the
/// direct immutable transport built from the author-only selection accepted at
/// startup.
pub trait AuthorAgentProtocol: ::core::fmt::Debug {
    /// Derives and sends one bound submission.
    fn submit<'a>(
        &'a self,
        transport: &'a SelectedAuthorTransport,
        identity: &'a ExecutionIdentity,
        command: &'a Command,
    ) -> ExecutionFuture<'a, HandoffDisposition>;

    /// Supervises and reconciles one bound operation.
    fn settle<'a>(
        &'a self,
        transport: &'a SelectedAuthorTransport,
        identity: &'a ExecutionIdentity,
    ) -> ExecutionFuture<'a, AgentSettlement>;

    /// Fetches and publishes the artifacts declared by one bound operation.
    fn complete_artifacts<'a>(
        &'a self,
        transport: &'a SelectedAuthorTransport,
        identity: &'a ExecutionIdentity,
    ) -> ExecutionFuture<'a, ArtifactCompletion>;
}

/// The sole product implementation of the executor's author boundary.
///
/// It owns the selected connection rather than accepting endpoint or trust
/// inputs per call.  This is the composition point where the validated
/// configuration snapshot meets the protocol implementation; no publisher
/// address, ambient proxy, or reloadable trust source can cross it.
#[derive(Debug)]
pub struct ProductAuthorPorts<'protocol> {
    /// The frozen direct connector for the selected endpoint and trust policy.
    transport: ProductTransport<'protocol>,
    /// The protocol codec operating only through `transport`.
    protocol: &'protocol dyn AuthorAgentProtocol,
}

#[derive(Debug)]
enum ProductTransport<'runtime> {
    Owned(SelectedAuthorTransport),
    Runtime(&'runtime SelectedAuthorTransport),
}

impl core::ops::Deref for ProductTransport<'_> {
    type Target = SelectedAuthorTransport;
    fn deref(&self) -> &Self::Target {
        match self {
            Self::Owned(transport) => transport,
            Self::Runtime(transport) => transport,
        }
    }
}

impl<'protocol> ProductAuthorPorts<'protocol> {
    /// Returns the product author boundary over one selected connection.
    #[must_use]
    pub fn new(
        connection: SelectedAuthorConnection,
        protocol: &'protocol dyn AuthorAgentProtocol,
    ) -> Result<Self, SelectedAuthorTransportFailure> {
        Ok(Self {
            transport: ProductTransport::Owned(SelectedAuthorTransport::new(connection)?),
            protocol,
        })
    }

    /// Uses the runtime's already established immutable connector, without
    /// reconstructing trust configuration or introducing another client.
    pub fn over_transport(
        transport: &'protocol SelectedAuthorTransport,
        protocol: &'protocol dyn AuthorAgentProtocol,
    ) -> Self {
        Self { transport: ProductTransport::Runtime(transport), protocol }
    }
}

impl AuthorPorts for ProductAuthorPorts<'_> {
    fn submit<'a>(
        &'a self,
        identity: &'a ExecutionIdentity,
        command: &'a Command,
    ) -> ExecutionFuture<'a, HandoffDisposition> {
        if self.transport.require_execution(identity).is_err() {
            return Box::pin(async { HandoffDisposition::Conflict });
        }
        self.protocol.submit(&self.transport, identity, command)
    }

    fn settle<'a>(
        &'a self,
        identity: &'a ExecutionIdentity,
    ) -> ExecutionFuture<'a, AgentSettlement> {
        if self.transport.require_execution(identity).is_err() {
            return Box::pin(async {
                AgentSettlement::Outstanding {
                    recovery: RecoveryFact {
                        category: RecoveryCategory::OperationLookup,
                        evidence: RecoveryExecutionEvidence::ExecutionCertainty {
                            certainty: OperationExecutionCertainty::RemoteOutcomeUnknown,
                        },
                        attempt_count: 0,
                        detail: String::new(),
                        manual_resume_eligible: true,
                        retry_delay_milliseconds: 0,
                        retry_observed_at_unix_milliseconds: 0,
                    },
                }
            });
        }
        self.protocol.settle(&self.transport, identity)
    }

    fn complete_artifacts<'a>(
        &'a self,
        identity: &'a ExecutionIdentity,
    ) -> ExecutionFuture<'a, ArtifactCompletion> {
        if self.transport.require_execution(identity).is_err() {
            return Box::pin(async { awaiting_retrieval(RecoveryCategory::ArtifactTransfer) });
        }
        self.protocol.complete_artifacts(&self.transport, identity)
    }
}

/// The executor a product build installs.
#[derive(Debug)]
pub struct AuthorAgentOperationExecutor<'ports> {
    /// What it reaches the author through.
    ports: &'ports dyn AuthorPorts,
}

impl<'ports> AuthorAgentOperationExecutor<'ports> {
    /// What this executor is called wherever a composition is described.
    pub const NAME: &'static str = "author-agent";

    /// Returns an executor that reaches the author through `ports`.
    #[must_use]
    pub fn over(ports: &'ports dyn AuthorPorts) -> Self {
        Self { ports }
    }
}

/// Returns the outcome one handoff disposition produces on its own.
///
/// Only two of them end anything before the agent has said what happened. The
/// rest are outstanding work: reporting them as endings would settle an
/// operation on a transport difficulty rather than on a remote fact.
#[must_use]
pub fn outcome_of_handoff(disposition: &HandoffDisposition) -> Option<OperationExecutorOutcome> {
    match disposition {
        HandoffDisposition::Accepted
        | HandoffDisposition::Duplicate
        | HandoffDisposition::ReconcileRetained => None,
        HandoffDisposition::NotExecuted => Some(refused("the agent recorded nothing")),
        HandoffDisposition::RecoveryWindowExpired => Some(failed_closed(
            TerminalFailureKind::RemoteStateLost,
            OperationExecutionCertainty::RemoteOutcomeUnknown,
        )),
        HandoffDisposition::Conflict => Some(failed_closed(
            TerminalFailureKind::IntegrityFailure,
            OperationExecutionCertainty::RemoteOutcomeUnknown,
        )),
        HandoffDisposition::RetryAfter { milliseconds } => Some(unresolved(
            RecoveryCategory::AmbiguousSubmission,
            OperationExecutionCertainty::SubmissionUnknown,
            *milliseconds,
        )),
        HandoffDisposition::Unknown => Some(unresolved(
            RecoveryCategory::AmbiguousSubmission,
            OperationExecutionCertainty::SubmissionUnknown,
            0,
        )),
    }
}

/// Returns the outcome an authoritative refusal produces.
fn refused(detail: &str) -> OperationExecutorOutcome {
    OperationExecutorOutcome::TerminalFailure {
        failure: TerminalFailure {
            disposition: TerminalFailureDisposition::AuthoritativeNonExecution {
                certainty: OperationExecutionCertainty::ConfirmedNotExecuted,
            },
            kind: TerminalFailureKind::Rejected,
            metadata: Some(detail.to_owned()),
        },
    }
}

/// Returns the outcome an unresolvable answer produces.
///
/// Fail-closed rather than outstanding, because these are the two answers no
/// amount of asking again improves: the agent no longer holds the work, or two
/// accounts of it disagree.
fn failed_closed(
    kind: TerminalFailureKind,
    certainty: OperationExecutionCertainty,
) -> OperationExecutorOutcome {
    OperationExecutorOutcome::TerminalFailure {
        failure: TerminalFailure {
            disposition: TerminalFailureDisposition::FailClosedIndeterminate { certainty },
            kind,
            metadata: None,
        },
    }
}

/// Returns the outcome unresolved work produces.
fn unresolved(
    category: RecoveryCategory,
    certainty: OperationExecutionCertainty,
    retry_delay_milliseconds: u64,
) -> OperationExecutorOutcome {
    OperationExecutorOutcome::RecoveryRequired {
        recovery: RecoveryFact {
            attempt_count: 0,
            category,
            detail: String::new(),
            evidence: RecoveryExecutionEvidence::ExecutionCertainty { certainty },
            manual_resume_eligible: true,
            retry_delay_milliseconds,
            retry_observed_at_unix_milliseconds: 0,
        },
    }
}

/// Returns the outcome work that provably succeeded but is not here produces.
fn awaiting_retrieval(category: RecoveryCategory) -> ArtifactCompletion {
    ArtifactCompletion::Recovery {
        recovery: RecoveryFact {
            attempt_count: 0,
            category,
            detail: String::new(),
            evidence: RecoveryExecutionEvidence::AuthoritativeRemoteSuccess,
            manual_resume_eligible: true,
            retry_delay_milliseconds: 0,
            retry_observed_at_unix_milliseconds: 0,
        },
    }
}

impl OperationExecutor for AuthorAgentOperationExecutor<'_> {
    fn execute<'a>(
        &'a self,
        identity: &'a ExecutionIdentity,
        command: &'a Command,
        progress: &'a dyn ProgressPort,
    ) -> ExecutionFuture<'a, OperationExecutorOutcome> {
        Box::pin(async move {
            progress.report(SUBMITTING_DETAIL);
            let handoff = self.ports.submit(identity, command).await;
            if let Some(settled) = outcome_of_handoff(&handoff) {
                return settled;
            }
            progress.report(SUPERVISING_DETAIL);
            match self.ports.settle(identity).await {
                AgentSettlement::Outstanding { recovery } => {
                    OperationExecutorOutcome::RecoveryRequired { recovery }
                }
                AgentSettlement::Terminal { failure } => {
                    OperationExecutorOutcome::TerminalFailure { failure }
                }
                AgentSettlement::NotExecuted { category } => refused(&category),
                AgentSettlement::Failed { category } => OperationExecutorOutcome::TerminalFailure {
                    failure: TerminalFailure {
                        disposition: TerminalFailureDisposition::AuthoritativeRemoteFailure,
                        kind: TerminalFailureKind::RemoteFailed,
                        metadata: Some(category),
                    },
                },
                AgentSettlement::Succeeded { inline_result } => {
                    self.publish(identity, inline_result, progress).await
                }
            }
        })
    }
}

impl AuthorAgentOperationExecutor<'_> {
    /// Returns the outcome of fetching what a successful execution produced.
    ///
    /// An artifact that will not publish leaves the execution outstanding under
    /// a success this daemon already believes. Calling it a failure would
    /// retract a remote fact because of a local retrieval, which is the one
    /// direction the evidence never runs.
    async fn publish(
        &self,
        identity: &ExecutionIdentity,
        inline_result: Option<String>,
        progress: &dyn ProgressPort,
    ) -> OperationExecutorOutcome {
        progress.report(COMPLETING_DETAIL);
        match self.ports.complete_artifacts(identity).await {
            ArtifactCompletion::Published { artifacts } => {
                OperationExecutorOutcome::Succeeded { artifacts, inline_result }
            }
            ArtifactCompletion::Recovery { recovery } => {
                OperationExecutorOutcome::RecoveryRequired { recovery }
            }
            ArtifactCompletion::Unavailable => OperationExecutorOutcome::TerminalFailure {
                failure: TerminalFailure {
                    kind: TerminalFailureKind::ResultUnavailable,
                    disposition: TerminalFailureDisposition::AuthoritativeRemoteSuccess,
                    metadata: None,
                },
            },
        }
    }
}
