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

use std::path::Path;

use slingshot_domain::author_agent_transport_contract::AuthorAgentTransportContract;
use slingshot_domain::command::catalog::CommandCatalog;
use slingshot_local_protocol::control::{HelloResult, operation_compatibility};
use slingshot_local_protocol::message::{OperationEnvelope, OperationRequest, OperationResponse};

use crate::configuration_check::CheckReport;
use crate::daemon_answer::{
    AccessContext, Admitted, Submission, maintained, observed, recovering, recovery_facts,
    submitted,
};
use crate::daemon_connection::{
    ExchangeFailure, ExpectedTarget, ObservedOwner, OwnerDisposition, classify_owner,
};
use crate::daemon_process::DaemonExpectation;
use crate::daemon_request::{
    build_command, expected_digest, expected_revision, maintenance_request, observation_request,
    required, spoken_operation_version,
};
use crate::exit_classification;
use crate::interrupt::{self, Phase, SignalOutcome};
use crate::invocation::{
    EVERY_OPTION, Invocation, LOCAL_LEAVES, METADATA_ONLY_LEAVES, OPERATION_IDENTIFIER_OPTION,
    OPERATION_NAMING_LEAVES, SERVE_LEAF, Selection, is_catalog_command,
};
use crate::machine_outcome_envelope::MachineOutcomeEnvelope;
use crate::operation_submission;
use crate::target_selection::{
    NAMESPACE_ONLY_LEAVES, NamespacePair, TargetRequirement, namespace_of, requirement_of,
};
use slingshot_configuration::profile_loader::ConfigurationDiagnostic;

/// What this product is called wherever it names itself.
const PRODUCT_NAME: &str = "slingshot";

/// The leaf that answers with this build's version.
const VERSION_LEAF: &str = "version";

/// What a start reports when a daemon was already there.
const ADOPTED_STATE: &str = "adopted";

/// What a start reports when it made one.
const CREATED_STATE: &str = "created";

/// What a stop reports when the owner released the endpoint.
const STOPPED_STATE: &str = "stopped";

/// What a probe reports when a daemon owns the namespace.
const SERVING_STATE: &str = "serving";

/// What a probe reports when nothing owns the namespace.
const ABSENT_STATE: &str = "absent";

/// What every request identifier this build invents begins with.
const REQUEST_IDENTIFIER_PREFIX: &str = "command-line-";

/// What is said when the owner speaks no version this build speaks.
const NO_SHARED_VERSION: &str =
    "that daemon serves no operation-protocol version this build speaks; inspect it or stop it";

/// The revision an operation stands at the moment it is admitted.
///
/// One. Admission is the first thing that happens to an operation, so a signal
/// arriving before its status is read still names a revision a caller can
/// quote, and quoting it finds the operation rather than nothing.
const ADMITTED_REVISION: u64 = 1;

/// The first line of the help this build prints.
const HELP_HEADING: &str = "slingshot - one command line over one daemon";

/// The heading above the leaves help lists.
const HELP_COMMANDS: &str = "commands:";

/// The heading above the registry commands help lists.
const HELP_CATALOG: &str = "commands this daemon runs against an author:";

/// The heading above the options help lists.
const HELP_OPTIONS: &str = "options:";

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
    /// Handing the standard streams to the protocol server.
    ModelContextProtocolServer,
}

impl Service {
    /// Returns whether reaching this service talks to a versioned daemon.
    ///
    /// The two that do not are the reason the distinction exists: they must
    /// keep working when a daemon is absent or incompatible, which is exactly
    /// when somebody runs them.
    #[must_use]
    pub fn is_versioned(self) -> bool {
        !matches!(
            self,
            Self::Metadata
                | Self::ConfigurationCheck
                | Self::DaemonLifecycle
                | Self::ModelContextProtocolServer
        )
    }
}

/// The observation leaves, which read or release one operation.
pub const OBSERVATION_LEAVES: &[&str] = OPERATION_NAMING_LEAVES;

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
    if leaf == SERVE_LEAF {
        return Ok(Service::ModelContextProtocolServer);
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

// ---------------------------------------------------------------- boundaries

/// The contracts a build was made against.
///
/// Injected rather than read at the point of use, so a scenario can hand this
/// application a build whose contracts have moved and watch the gate refuse
/// without rebuilding anything.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Provenance {
    /// Which author-agent transport contract this build carries.
    pub author_agent_transport_contract_digest: String,
    /// Which daemon runtime contract this build carries.
    pub daemon_runtime_contract_digest: String,
}

impl Provenance {
    /// Returns the contracts this build actually embeds.
    ///
    /// Recomputed from the embedded bytes rather than remembered, so a build
    /// whose contract changed cannot describe itself with the old digest.
    #[must_use]
    pub fn embedded() -> Self {
        Self {
            author_agent_transport_contract_digest: AuthorAgentTransportContract::embedded_digest(),
            daemon_runtime_contract_digest: DaemonExpectation::embedded_runtime_digest(),
        }
    }

    /// Returns whether both contracts are the ones `other` names.
    #[must_use]
    pub fn agrees_with(&self, other: &Self) -> bool {
        self == other
    }
}

/// Reading configuration, and the files configuration names.
pub trait ConfigurationBoundary {
    /// Returns what checking one selection found.
    fn check(&self, selection: &Selection) -> CheckReport;
}

/// Placing bytes somewhere a caller asked for.
pub trait FilesystemBoundary {
    /// Writes `bytes` where `destination` says.
    ///
    /// # Errors
    ///
    /// Returns what the operating system said, unchanged.
    fn place(&self, destination: &Path, bytes: &[u8]) -> Result<(), String>;
}

/// Creating the daemon process that owns a namespace.
pub trait ProcessBoundary {
    /// Starts one daemon for the namespace this invocation named.
    ///
    /// # Errors
    ///
    /// Returns what stopped it, in words a person can act on.
    fn start_daemon(&self, namespace: &NamespacePair) -> Result<(), String>;
}

/// Talking to the daemon that owns a namespace.
///
/// Two ways of asking, because they are two different questions. The probe asks
/// whether anybody is there at all and is answered by the retained surface,
/// which every daemon serves however old it is. The greeting asks what the
/// owner was built against and which protocol it speaks, and only a daemon that
/// serves versioned operations can answer it. A lifecycle command that needed
/// the greeting would report an absent daemon whenever it met one it could not
/// run operations on, which is exactly when somebody needs to find and stop it.
pub trait DaemonBoundary {
    /// Returns the live readiness nonce of the owner, when there is one.
    ///
    /// # Errors
    ///
    /// Returns [`ExchangeFailure`] when something is listening and the exchange
    /// fails; nothing listening is `Ok(None)` rather than a failure.
    fn owner_nonce(&self, namespace: &NamespacePair) -> Result<Option<String>, ExchangeFailure>;

    /// Asks the owner who it is.
    ///
    /// # Errors
    ///
    /// Returns [`ExchangeFailure`] when nothing is listening or it refuses.
    fn hello(&self, namespace: &NamespacePair) -> Result<HelloResult, ExchangeFailure>;

    /// Asks the owner to stop, quoting the nonce it published.
    ///
    /// # Errors
    ///
    /// Returns [`ExchangeFailure`], including the refusal a stale nonce gets.
    fn stop(&self, namespace: &NamespacePair, readiness_nonce: &str)
    -> Result<(), ExchangeFailure>;

    /// Sends one versioned operation request and returns what came back.
    ///
    /// # Errors
    ///
    /// Returns [`ExchangeFailure`] when the exchange fails or is refused.
    fn operate(
        &self,
        namespace: &NamespacePair,
        envelope: &OperationEnvelope,
    ) -> Result<OperationResponse, ExchangeFailure>;
}

/// Reading the time, for the one identifier a run may have to invent.
pub trait ClockBoundary {
    /// Returns milliseconds since the epoch.
    fn milliseconds_since_epoch(&self) -> u64;
}

/// Learning that somebody asked this run to stop.
///
/// The boundary answers whether a signal arrived; how far the run had got is
/// this application's own knowledge, and asking the operating system for it
/// would be asking the wrong party. That split is what lets the same signal
/// produce a pre-receipt account in one place and a post-receipt one in
/// another, from one bit.
pub trait SignalBoundary {
    /// Reports whether somebody has asked this run to stop.
    fn stop_requested(&self) -> bool;
}

/// Reaching something across a network.
///
/// Held and reached nowhere. Every remote conversation this product has belongs
/// to the daemon, which authenticates, retries, and records it; a command line
/// that opened its own would do none of that. The boundary exists so a fake can
/// count the calls and the suite can prove the count is zero, which turns a
/// silent future mistake into a failing test.
pub trait NetworkBoundary {
    /// Reports whether `authority` answers.
    fn authority_answers(&self, authority: &str) -> bool;
}

// --------------------------------------------------------------- the outcome

/// What one run produced, before anything is written.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Answer {
    /// One outcome, rendered in whichever form the caller asked for.
    Envelope(Box<MachineOutcomeEnvelope>),
    /// Text this build answers out of itself.
    Text(String),
    /// A local refusal, which is a diagnostic rather than an outcome.
    ///
    /// Deliberately not an envelope. The envelope vocabulary is closed and
    /// describes what happened to an operation; a run that never reached one
    /// has no operation to describe, and inventing a tag for it would let a
    /// consumer parse a local mistake as a remote answer.
    Refusal(String),
}

/// One finished run.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Completion {
    /// What it produced.
    pub answer: Answer,
    /// What it has to say about why, in a closed vocabulary and no other.
    ///
    /// Separate from the answer because the two go to different streams and
    /// serve different readers. A refused configuration check still answers -
    /// the selection does not resolve - and the diagnostics say what was wrong
    /// with it, in the exact words the configuration produced them in.
    pub diagnostics: Vec<String>,
    /// What the process exits with.
    pub exit: i32,
}

/// Why a run ends without an outcome.
///
/// Three, because three exits are what a caller can act on differently: fix the
/// invocation, wait for what is missing, or look at this machine.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RunRefusal {
    /// The invocation itself is wrong.
    Usage(String),
    /// What it needed is not there.
    Unavailable(String),
    /// Something on this machine failed.
    Local(String),
    /// Somebody asked the run to stop while it was waiting.
    Halted(Box<Phase>),
}

impl RunRefusal {
    /// Returns the run this refusal ends.
    #[must_use]
    pub fn completion(self) -> Completion {
        let (message, exit) = match self {
            Self::Halted(phase) => return interrupted(&phase),
            Self::Usage(message) => (message, exit_classification::USAGE),
            Self::Unavailable(message) => (message, exit_classification::UNAVAILABLE),
            Self::Local(message) => (message, exit_classification::LOCAL_FAILURE),
        };
        Completion { answer: Answer::Refusal(message), diagnostics: Vec::new(), exit }
    }
}

// ----------------------------------------------------------- the application

/// The whole command surface, over the boundaries it is given.
///
/// Nothing here opens a file, starts a process, or connects to anything. Every
/// effect goes through a boundary, which is what lets the suite prove that help
/// reaches none of them and that a configuration check reaches only the two it
/// is allowed.
pub struct CommandLineApplication<'boundaries> {
    /// Reading the time.
    pub clock: &'boundaries dyn ClockBoundary,
    /// Reading configuration.
    pub configuration: &'boundaries dyn ConfigurationBoundary,
    /// Talking to a daemon.
    pub daemon: &'boundaries dyn DaemonBoundary,
    /// Writing where a caller asked.
    pub filesystem: &'boundaries dyn FilesystemBoundary,
    /// Reaching across a network, which nothing does.
    pub network: &'boundaries dyn NetworkBoundary,
    /// Creating a daemon.
    pub process: &'boundaries dyn ProcessBoundary,
    /// The contracts this build was made against.
    pub provenance: Provenance,
    /// Learning that somebody asked this run to stop.
    pub signals: &'boundaries dyn SignalBoundary,
}

impl ::core::fmt::Debug for CommandLineApplication<'_> {
    fn fmt(&self, formatter: &mut ::core::fmt::Formatter<'_>) -> ::core::fmt::Result {
        formatter
            .debug_struct("CommandLineApplication")
            .field("provenance", &self.provenance)
            .finish()
    }
}

impl CommandLineApplication<'_> {
    /// Runs one parsed invocation to exactly one completion.
    #[must_use]
    pub fn run(&self, invocation: &Invocation) -> Completion {
        self.answer(invocation).unwrap_or_else(RunRefusal::completion)
    }

    /// Returns what one invocation produced, or why it produced nothing.
    fn answer(&self, invocation: &Invocation) -> Result<Completion, RunRefusal> {
        if let Some(halted) = self
            .halted(&Phase::BeforeReceipt { retry_operation_identifier: self.request_identifier() })
        {
            return Ok(halted);
        }
        match self.dispatch(invocation)? {
            Service::Metadata => Ok(metadata(invocation)),
            Service::ConfigurationCheck => self.check_configuration(invocation),
            Service::DaemonLifecycle => self.daemon_lifecycle(invocation),
            Service::OperationSubmission => self.submit(invocation),
            Service::OperationObservation => self.observe(invocation),
            Service::OperationMaintenance => self.maintain(invocation),
            Service::ModelContextProtocolServer => Ok(served()),
        }
    }

    /// Returns the one service this invocation may reach.
    fn dispatch(&self, invocation: &Invocation) -> Result<Service, RunRefusal> {
        let agrees = self.provenance.agrees_with(&Provenance::embedded());
        require_dispatchable(invocation, agrees).map_err(|refusal| match refusal {
            DispatchRefusal::ProvenanceRefused => RunRefusal::Unavailable(refusal.to_string()),
            DispatchRefusal::Unroutable { .. } => RunRefusal::Usage(refusal.to_string()),
        })
    }

    /// Returns what checking the selected configuration found.
    fn check_configuration(&self, invocation: &Invocation) -> Result<Completion, RunRefusal> {
        let report = self.configuration.check(&invocation.selection);
        let (profile, environment) = match &report {
            CheckReport::Resolved(facts) => (facts.profile.clone(), facts.environment.clone()),
            CheckReport::Refused { .. } | CheckReport::NotSelected { .. } => (
                invocation.selection.profile.clone().unwrap_or_default(),
                invocation.selection.environment.clone().unwrap_or_default(),
            ),
        };
        let exit = if report.is_resolved() {
            exit_classification::SUCCESS
        } else {
            exit_classification::LOCAL_FAILURE
        };
        let envelope = MachineOutcomeEnvelope::ConfigurationReport {
            environment,
            profile,
            resolved: report.is_resolved(),
        };
        Ok(Completion {
            answer: Answer::Envelope(Box::new(envelope)),
            diagnostics: report.diagnostics().iter().map(stated).collect(),
            exit,
        })
    }

    /// Returns the namespace one invocation names.
    fn namespace(&self, invocation: &Invocation) -> Result<NamespacePair, RunRefusal> {
        namespace_of(&invocation.selection)
            .map_err(|refusal| RunRefusal::Usage(refusal.to_string()))
    }

    /// Finds, creates, or stops the daemon that owns a namespace.
    fn daemon_lifecycle(&self, invocation: &Invocation) -> Result<Completion, RunRefusal> {
        let namespace = self.namespace(invocation)?;
        let owner = self
            .daemon
            .owner_nonce(&namespace)
            .map_err(|failure| RunRefusal::Unavailable(failure.to_string()))?;
        let state = self.act_on_owner(invocation, &namespace, owner)?;
        let envelope = MachineOutcomeEnvelope::DaemonControl {
            action: invocation.verb.clone(),
            state: state.to_owned(),
        };
        Ok(Completion {
            answer: Answer::Envelope(Box::new(envelope)),
            diagnostics: Vec::new(),
            exit: exit_classification::SUCCESS,
        })
    }

    /// Returns what one lifecycle leaf does about the owner it found.
    ///
    /// A start adopts what is already there rather than contending with it, and
    /// a stop quotes the nonce that owner published rather than a remembered
    /// one, so neither can end an instance that replaced the one it meant.
    fn act_on_owner(
        &self,
        invocation: &Invocation,
        namespace: &NamespacePair,
        owner: Option<String>,
    ) -> Result<&'static str, RunRefusal> {
        match (invocation.verb.as_str(), owner) {
            ("daemon-start", Some(_)) => Ok(ADOPTED_STATE),
            ("daemon-start", None) => {
                self.process.start_daemon(namespace).map_err(RunRefusal::Unavailable)?;
                Ok(CREATED_STATE)
            }
            ("daemon-stop", Some(nonce)) => {
                self.daemon
                    .stop(namespace, &nonce)
                    .map_err(|failure| RunRefusal::Unavailable(failure.to_string()))?;
                Ok(STOPPED_STATE)
            }
            (_, Some(_)) => Ok(SERVING_STATE),
            (_, None) => Ok(ABSENT_STATE),
        }
    }

    /// Returns the owner a versioned invocation may send work to.
    ///
    /// Three checks, in the order that makes each one meaningful: the daemon
    /// answers, it agrees with this build about the contracts and the protocol,
    /// and it serves the target this caller named. A client that skipped the
    /// last would send somebody else's target the work it meant for its own.
    fn owner(
        &self,
        invocation: &Invocation,
        namespace: &NamespacePair,
    ) -> Result<HelloResult, RunRefusal> {
        let phase = Phase::BeforeReceipt { retry_operation_identifier: self.request_identifier() };
        let hello = self.reached(self.daemon.hello(namespace), &phase)?;
        let spoken = spoken_operation_version();
        let compatibility = operation_compatibility(
            &hello,
            &[spoken],
            &self.provenance.daemon_runtime_contract_digest,
        );
        if !compatibility.permits_operations() {
            let refusal = compatibility.refusal().map(|error| error.message).unwrap_or_default();
            return Err(RunRefusal::Unavailable(refusal));
        }
        self.require_expected_owner(invocation, &hello, namespace, spoken)?;
        Ok(hello)
    }

    /// Requires the owner to be serving what this caller expects.
    fn require_expected_owner(
        &self,
        invocation: &Invocation,
        hello: &HelloResult,
        namespace: &NamespacePair,
        spoken: u32,
    ) -> Result<(), RunRefusal> {
        let expected = ExpectedTarget {
            author_target_identity_digest: expected_digest(invocation, hello),
            operation_protocol_version: u64::from(spoken),
            selected_environment_revision: expected_revision(invocation, hello),
        };
        let observed = ObservedOwner {
            author_target_identity_digest: hello.author_target_identity_digest.clone(),
            namespace_display: namespace.key(),
            readiness_nonce: hello.readiness_nonce.clone(),
            selected_environment_revision: hello.selected_environment_revision.clone(),
            supported_operation_versions: hello
                .supported_operation_protocol_versions
                .iter()
                .map(|version| u64::from(*version))
                .collect(),
        };
        match classify_owner(&expected, Some(&observed)) {
            OwnerDisposition::Matching { .. } => Ok(()),
            OwnerDisposition::Mismatched { guidance, .. } => Err(RunRefusal::Unavailable(guidance)),
            OwnerDisposition::Absent | OwnerDisposition::OperationIncompatible { .. } => {
                Err(RunRefusal::Unavailable(NO_SHARED_VERSION.to_owned()))
            }
        }
    }

    /// Sends one versioned request to the owner of a namespace.
    fn exchange(
        &self,
        namespace: &NamespacePair,
        hello: &HelloResult,
        request: OperationRequest,
    ) -> Result<OperationResponse, RunRefusal> {
        let phase = Phase::BeforeReceipt { retry_operation_identifier: self.request_identifier() };
        let envelope = OperationEnvelope {
            author_target_identity_digest: hello.author_target_identity_digest.clone(),
            daemon_runtime_contract_digest: hello.daemon_runtime_contract_digest.clone(),
            operation_protocol_version: spoken_operation_version(),
            request,
            request_identifier: self.request_identifier(),
            selected_environment_revision: hello.selected_environment_revision.clone(),
        };
        envelope.require_well_formed().map_err(|failure| RunRefusal::Local(failure.to_string()))?;
        self.reached(self.daemon.operate(namespace, &envelope), &phase)
    }

    /// Returns what one exchange reached, or why it reached nothing.
    ///
    /// A failed exchange after somebody asked the run to stop is that stop
    /// rather than an unavailable daemon: the daemon was there, and the run
    /// walked away from it. Reporting it as unavailable would tell a caller
    /// something false about the daemon and lose the account of what they did.
    fn reached<Answered>(
        &self,
        outcome: Result<Answered, ExchangeFailure>,
        phase: &Phase,
    ) -> Result<Answered, RunRefusal> {
        match outcome {
            Ok(answered) => Ok(answered),
            Err(_) if self.signals.stop_requested() => {
                Err(RunRefusal::Halted(Box::new(phase.clone())))
            }
            Err(failure) => Err(RunRefusal::Unavailable(failure.to_string())),
        }
    }

    /// Returns what this run reports if somebody has asked it to stop.
    ///
    /// Consulted at each point where the honest account changes, so the phase
    /// reported is the one the run had actually reached rather than the one it
    /// started in.
    fn halted(&self, phase: &Phase) -> Option<Completion> {
        self.signals.stop_requested().then(|| interrupted(phase))
    }

    /// Returns the identifier this run puts on its request.
    fn request_identifier(&self) -> String {
        format!("{REQUEST_IDENTIFIER_PREFIX}{}", self.clock.milliseconds_since_epoch())
    }
}

impl CommandLineApplication<'_> {
    /// Submits one catalog command and reports what the daemon admitted.
    fn submit(&self, invocation: &Invocation) -> Result<Completion, RunRefusal> {
        let command =
            build_command(invocation).map_err(|refusal| RunRefusal::Usage(refusal.to_string()))?;
        let namespace = self.namespace(invocation)?;
        let hello = self.owner(invocation, &namespace)?;
        let expectation = self.expectation(invocation, &hello);
        let handshake = (
            hello.daemon_runtime_contract_digest.as_str(),
            hello.author_target_identity_digest.as_str(),
            hello.selected_environment_revision.as_str(),
        );
        let identifier = self.request_identifier();
        let prepared =
            operation_submission::prepare(invocation, command, &expectation, handshake, || {
                identifier
            })
            .map_err(|refusal| RunRefusal::Usage(refusal.to_string()))?;
        let request = OperationRequest::Execute {
            command: serde_json::to_value(&prepared.command)
                .map_err(|failure| RunRefusal::Local(failure.to_string()))?,
            operation_identifier: prepared.key_source.identifier().to_owned(),
            workflow_correlation_identifier: None,
        };
        let response = self.exchange(&namespace, &hello, request)?;
        match submitted(&response)? {
            Submission::Ended(completion) => Ok(*completion),
            Submission::Admitted(admitted) => {
                if let Some(halted) = self.halted(&Phase::Observing {
                    operation_identifier: admitted.operation_identifier.clone(),
                    revision: ADMITTED_REVISION,
                }) {
                    return Ok(halted);
                }
                self.receipt(&namespace, &hello, &admitted)
            }
        }
    }

    /// Returns the receipt one admitted operation stands at.
    ///
    /// The revision comes from the status this client reads immediately after,
    /// because admission is the moment before an operation has a history and
    /// the wire receipt says so by carrying no revision at all.
    fn receipt(
        &self,
        namespace: &NamespacePair,
        hello: &HelloResult,
        admitted: &Admitted,
    ) -> Result<Completion, RunRefusal> {
        let request = OperationRequest::OperationStatus {
            operation_identifier: admitted.operation_identifier.clone(),
        };
        let response = self.exchange(namespace, hello, request)?;
        let OperationResponse::Status { operation_revision, .. } = response else {
            return self.resolved(
                namespace,
                hello,
                &response,
                &self.access(namespace, hello, admitted),
            );
        };
        Ok(Completion {
            answer: Answer::Envelope(Box::new(MachineOutcomeEnvelope::OperationReceipt {
                operation_identifier: admitted.operation_identifier.clone(),
                replayed: admitted.replayed,
                revision: operation_revision,
            })),
            diagnostics: Vec::new(),
            exit: exit_classification::SUCCESS,
        })
    }

    /// Returns what one answer means, reading a revision when it needs one.
    fn resolved(
        &self,
        namespace: &NamespacePair,
        hello: &HelloResult,
        response: &OperationResponse,
        context: &AccessContext,
    ) -> Result<Completion, RunRefusal> {
        let Some((category, evidence, operation_identifier)) = recovery_facts(response) else {
            return observed(response, context);
        };
        let revision = self.revision_of(namespace, hello, &operation_identifier)?;
        Ok(recovering(category, evidence, revision))
    }

    /// Returns the revision one operation stands at.
    fn revision_of(
        &self,
        namespace: &NamespacePair,
        hello: &HelloResult,
        operation_identifier: &str,
    ) -> Result<u64, RunRefusal> {
        let request = OperationRequest::OperationStatus {
            operation_identifier: operation_identifier.to_owned(),
        };
        match self.exchange(namespace, hello, request)? {
            OperationResponse::Status { operation_revision, .. } => Ok(operation_revision),
            other => Err(unreadable(&other)),
        }
    }

    /// Returns where the things one exchange may name can be fetched from.
    fn access(
        &self,
        namespace: &NamespacePair,
        hello: &HelloResult,
        admitted: &Admitted,
    ) -> AccessContext {
        AccessContext {
            author_target_identity_digest: hello.author_target_identity_digest.clone(),
            environment: namespace.environment.clone(),
            operation_identifier: admitted.operation_identifier.clone(),
            profile: namespace.profile.clone(),
        }
    }

    /// Reads or releases one operation.
    fn observe(&self, invocation: &Invocation) -> Result<Completion, RunRefusal> {
        let namespace = self.namespace(invocation)?;
        let hello = self.owner(invocation, &namespace)?;
        let request = observation_request(invocation)?;
        let admitted = Admitted {
            operation_identifier: required(invocation, OPERATION_IDENTIFIER_OPTION)?.to_owned(),
            replayed: false,
        };
        let response = self.exchange(&namespace, &hello, request)?;
        self.resolved(&namespace, &hello, &response, &self.access(&namespace, &hello, &admitted))
    }

    /// Lists operations, or previews, applies, or reads maintenance.
    fn maintain(&self, invocation: &Invocation) -> Result<Completion, RunRefusal> {
        let namespace = self.namespace(invocation)?;
        let hello = self.owner(invocation, &namespace)?;
        let partition = expected_digest(invocation, &hello);
        let request = maintenance_request(invocation, &partition)?;
        let response = self.exchange(&namespace, &hello, request)?;
        let context = AccessContext {
            author_target_identity_digest: partition,
            environment: namespace.environment.clone(),
            operation_identifier: String::new(),
            profile: namespace.profile.clone(),
        };
        maintained(&response, &context)
    }

    /// Returns what this client expects the daemon it found to be.
    fn expectation(&self, invocation: &Invocation, hello: &HelloResult) -> DaemonExpectation {
        DaemonExpectation {
            author_target_identity_digest: expected_digest(invocation, hello),
            runtime_contract_digest: self.provenance.daemon_runtime_contract_digest.clone(),
            selected_environment_revision: expected_revision(invocation, hello),
        }
    }
}

// --------------------------------------------------------------- the answers

/// Returns one configuration diagnostic in the words it was produced in.
///
/// Five fields and nothing else. A path, a name, a value, or a suggestion would
/// tell whoever ran this what the configuration root holds, and the closed
/// vocabulary exists precisely so that a diagnostic cannot.
fn stated(diagnostic: &ConfigurationDiagnostic) -> String {
    format!(
        "{} {} {} {} x{}",
        diagnostic.source_class.as_text(),
        diagnostic.stage.as_text(),
        diagnostic.structural_location,
        diagnostic.code.code(),
        diagnostic.occurrences
    )
}

/// Returns the refusal one unreadable answer produces.
fn unreadable(response: &OperationResponse) -> RunRefusal {
    RunRefusal::Local(format!("that daemon answered a status read with {response:?}"))
}

/// Returns what a run that handed its streams to the protocol server produced.
///
/// Nothing, on either stream. While that server runs it owns standard output,
/// and a command-line rendering written beside its messages would corrupt every
/// client parsing them.
fn served() -> Completion {
    Completion {
        answer: Answer::Text(String::new()),
        diagnostics: Vec::new(),
        exit: exit_classification::SUCCESS,
    }
}

/// Returns what this build answers out of itself.
fn metadata(invocation: &Invocation) -> Completion {
    let text = if invocation.verb == VERSION_LEAF {
        format!("{PRODUCT_NAME} {}", env!("CARGO_PKG_VERSION"))
    } else {
        help_text()
    };
    Completion {
        answer: Answer::Text(text),
        diagnostics: Vec::new(),
        exit: exit_classification::SUCCESS,
    }
}

/// Returns the help this build prints, from the vocabulary it actually has.
fn help_text() -> String {
    let mut lines = vec![HELP_HEADING.to_owned(), String::new(), HELP_COMMANDS.to_owned()];
    for leaf in LOCAL_LEAVES {
        lines.push(format!("  {leaf}"));
    }
    lines.push(String::new());
    lines.push(HELP_CATALOG.to_owned());
    for descriptor in CommandCatalog::published().descriptors() {
        lines.push(format!("  {} - {}", descriptor.wire_name, descriptor.title));
    }
    lines.push(String::new());
    lines.push(HELP_OPTIONS.to_owned());
    for option in EVERY_OPTION {
        lines.push(format!("  {option}"));
    }
    lines.join("\n")
}

/// Returns what a run interrupted in `phase` reports.
fn interrupted(phase: &Phase) -> Completion {
    match interrupt::on_signal(phase) {
        SignalOutcome::CommittedWork => Completion {
            answer: Answer::Text(String::new()),
            diagnostics: Vec::new(),
            exit: exit_classification::SUCCESS,
        },
        SignalOutcome::Interrupted { interruption } => Completion {
            answer: Answer::Envelope(Box::new(MachineOutcomeEnvelope::LocalApplicationError {
                interruption,
            })),
            diagnostics: Vec::new(),
            exit: exit_classification::INTERRUPTED,
        },
    }
}
