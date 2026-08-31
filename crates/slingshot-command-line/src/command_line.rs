//! Turning one argument vector into one process exit.
//!
//! This is where the product's boundaries are actually built: the configuration
//! it reads, the daemon it talks to, the process it may create, the clock, the
//! signal, and the streams it writes. Everything above it decides; only this
//! layer does anything, which is what lets every decision be proved without a
//! process and every effect be pinned with one.
//!
//! # One leaf, however it is spelled
//!
//! A leaf is one word to the parser and often two to a person: `daemon start`
//! and `daemon-start` name the same thing. The leading words are joined until
//! they name a leaf this build offers, so the spelling a person reaches for and
//! the spelling the vocabulary uses do not have to be the same.
//!
//! # The daemon's own entry is not a leaf
//!
//! `daemon serve` is how a start creates its child. It is not part of the
//! command vocabulary, takes no output form, and exits on its own taxonomy,
//! because the only caller that ever writes it is this executable.

use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use slingshot_configuration::profile_loader::{
    ConfigurationDiagnostic, DiagnosticSourceClass, DiagnosticStage, LoadedProfiles,
};
use slingshot_daemon::platform_runtime::endpoint::{self, EndpointAddress};
use slingshot_daemon::runtime_namespace::RuntimeNamespace;
use slingshot_domain::profile_authentication_contract::ConfigurationFailureCode;
use slingshot_local_protocol::control::{HELLO_METHOD, HelloResult};
use slingshot_local_protocol::envelope::{ControlRequest, ControlResponse, ResponseOutcome};
use slingshot_local_protocol::foundation_contract::FoundationContract;
use slingshot_local_protocol::message::{OperationEnvelope, OperationResponse};
use slingshot_local_protocol::ping::STOP_METHOD;

use crate::application::{
    Answer, ClockBoundary, CommandLineApplication, Completion, ConfigurationBoundary,
    DaemonBoundary, FilesystemBoundary, NetworkBoundary, ProcessBoundary, Provenance,
    SignalBoundary,
};
use crate::configuration_check::{self, CheckReport};
use crate::daemon_connection::{self, ExchangeFailure};
use crate::daemon_entry::{self, DaemonEntryArguments, DaemonEntryOutcome};
use crate::exit_classification;
use crate::explicit_daemon_start::{self, TargetRuntime};
use crate::human_renderer;
use crate::invocation::{
    self, ENVIRONMENT_OPTION, Invocation, OutputForm, PROFILE_OPTION, RUNTIME_ROOT_OPTION,
    Selection,
};
use crate::machine_readable_renderer;
use crate::target_selection::NamespacePair;

/// Exit status of a command that finished.
pub const EXIT_SUCCESS: u8 = 0;

/// Exit status of a command whose arguments do not name a usable target.
pub const EXIT_TARGET_UNUSABLE: u8 = 2;

/// Exit status of a run whose own runtime state could not be used.
pub const EXIT_RUNTIME_UNUSABLE: u8 = 7;

/// Exit status of a daemon process that found its namespace already owned.
pub const EXIT_ALREADY_OWNED: u8 = 8;

/// What the daemon's own entry is called once its words are joined.
const SERVE_LEAF: &str = "daemon-serve";

/// How many words a leaf may be spelled with.
const MAXIMUM_LEAF_WORDS: usize = 2;

/// How many arguments one option and its value occupy.
const OPTION_AND_VALUE: usize = 2;

/// What every diagnostic this executable writes begins with.
const DIAGNOSTIC_PREFIX: &str = "slingshot: ";

/// How often a waiting exchange asks whether it has been asked to stop.
const STOP_POLL_INTERVAL: std::time::Duration = std::time::Duration::from_millis(20);

/// What an exchange reports when a signal ended it rather than an answer.
const STOP_REQUESTED: &str = "the run was asked to stop while it was waiting";

/// Identifier this executable puts on its retained control requests.
const CONTROL_REQUEST_IDENTIFIER: &str = "command-line";

/// Returns one argument vector as the parser reads it: the leaf, then options.
///
/// Two spellings are accepted and normalized here rather than in the
/// vocabulary. A leaf may be written as one hyphenated word or as its words
/// separated by spaces, and the options a person types first are just as valid
/// as the ones they type last. Neither is a second vocabulary: after this, one
/// argument vector has one reading.
///
/// `--version` and `--help` name their leaves too, because that is what a
/// person types and refusing it would teach them nothing.
#[must_use]
pub fn normalized(arguments: &[String]) -> Vec<String> {
    let mut leaf = Vec::new();
    let mut rest = Vec::new();
    let mut position = 0;
    while position < arguments.len() {
        let word = arguments[position].clone();
        if let Some(named) = metadata_leaf(&word) {
            return vec![named.to_owned()];
        }
        if word.starts_with("--") {
            let value = arguments.get(position + 1).filter(|held| !held.starts_with("--"));
            rest.push(word.clone());
            position += 1;
            if invocation::takes_a_value(&word)
                && let Some(value) = value
            {
                rest.push(value.clone());
                position += 1;
            }
            continue;
        }
        if leaf.len() < MAXIMUM_LEAF_WORDS && !names_something(&leaf) {
            leaf.push(word);
            position += 1;
            continue;
        }
        rest.push(word);
        position += 1;
    }
    let mut named = vec![longest_leaf(&leaf)];
    named.extend(rest);
    named
}

/// Returns the leaf one metadata option names.
fn metadata_leaf(word: &str) -> Option<&'static str> {
    match word {
        "--version" => Some("version"),
        "--help" => Some("help"),
        _ => None,
    }
}

/// Returns whether these words already name a leaf.
fn names_something(words: &[String]) -> bool {
    !words.is_empty() && invocation::names_a_leaf(&words.join("-"))
}

/// Returns the longest spelling of these words that names a leaf.
fn longest_leaf(words: &[String]) -> String {
    for taken in (1..=words.len()).rev() {
        let leaf = words[..taken].join("-");
        if invocation::names_a_leaf(&leaf) {
            return leaf;
        }
    }
    words.join("-")
}

/// Runs one argument vector and returns the exit it produced.
///
/// # Errors
///
/// Returns the exit as its value rather than as a failure; every way a run can
/// end is one of the classified exits, including the ways it can end badly.
#[must_use]
pub fn run(
    arguments: &[String],
    executable: &Path,
    output: &mut dyn Write,
    diagnostics: &mut dyn Write,
) -> i32 {
    let named = normalized(arguments);
    if named.first().is_some_and(|leaf| leaf == SERVE_LEAF) {
        return serve(&named[1..], diagnostics);
    }
    let invocation = match invocation::parse(&named) {
        Ok(invocation) => invocation,
        Err(refusal) => {
            write_diagnostic(diagnostics, &refusal.to_string());
            return exit_classification::USAGE;
        }
    };
    let completion = complete(&invocation, executable);
    write_completion(&completion, invocation.output, output, diagnostics)
}

/// Returns what one parsed invocation produced against the real boundaries.
fn complete(invocation: &Invocation, executable: &Path) -> Completion {
    let contract = FoundationContract::embedded();
    let runtime_root = match runtime_root(invocation) {
        Ok(root) => root,
        Err(reason) => {
            return Completion {
                answer: Answer::Refusal(reason),
                diagnostics: Vec::new(),
                exit: exit_classification::LOCAL_FAILURE,
            };
        }
    };
    let clock = ProductClock;
    let configuration = ProductConfiguration;
    let filesystem = ProductFilesystem;
    let network = ProductNetwork;
    let signals = ProductSignals::watching();
    let daemon = ProductDaemon::new(&contract, &runtime_root, signals.flag());
    let process = ProductProcess {
        contract: &contract,
        executable: executable.to_path_buf(),
        runtime_root: runtime_root.clone(),
    };
    let application = CommandLineApplication {
        clock: &clock,
        configuration: &configuration,
        daemon: &daemon,
        filesystem: &filesystem,
        network: &network,
        process: &process,
        provenance: Provenance::embedded(),
        signals: &signals,
    };
    application.run(invocation)
}

/// Returns the runtime root this invocation acts under.
fn runtime_root(invocation: &Invocation) -> Result<PathBuf, String> {
    if let Some(named) = invocation.arguments.get(RUNTIME_ROOT_OPTION) {
        return Ok(PathBuf::from(named));
    }
    let directories = directories::ProjectDirs::from("", "", "slingshot")
        .ok_or_else(|| "this account has no home directory".to_owned())?;
    let root = directories.runtime_dir().unwrap_or_else(|| directories.data_dir());
    Ok(root.to_path_buf())
}

/// Writes one completion where each of its parts belongs.
fn write_completion(
    completion: &Completion,
    form: Option<OutputForm>,
    output: &mut dyn Write,
    diagnostics: &mut dyn Write,
) -> i32 {
    for diagnostic in &completion.diagnostics {
        write_diagnostic(diagnostics, diagnostic);
    }
    match &completion.answer {
        Answer::Refusal(message) => write_diagnostic(diagnostics, message),
        Answer::Text(text) if text.is_empty() => {}
        Answer::Text(text) => writeln!(output, "{text}").unwrap_or_default(),
        Answer::Envelope(envelope) if form == Some(OutputForm::Machine) => {
            match machine_readable_renderer::render(envelope) {
                Ok(rendered) => writeln!(output, "{rendered}").unwrap_or_default(),
                Err(refusal) => {
                    write_diagnostic(diagnostics, &refusal.to_string());
                    return exit_classification::LOCAL_FAILURE;
                }
            }
        }
        Answer::Envelope(envelope) => {
            let rendered = human_renderer::render(envelope);
            if !rendered.standard_output.is_empty() {
                writeln!(output, "{}", rendered.standard_output).unwrap_or_default();
            }
            if !rendered.standard_error.is_empty() {
                write_diagnostic(diagnostics, &rendered.standard_error);
            }
        }
    }
    completion.exit
}

/// Writes one diagnostic where diagnostics go.
fn write_diagnostic(diagnostics: &mut dyn Write, message: &str) {
    writeln!(diagnostics, "{DIAGNOSTIC_PREFIX}{message}").unwrap_or_default();
}

/// Serves one namespace as the child a start created.
fn serve(options: &[String], diagnostics: &mut dyn Write) -> i32 {
    let mut profile = String::new();
    let mut environment = String::new();
    let mut root = PathBuf::new();
    let mut position = 0;
    while position + 1 < options.len() {
        let value = options[position + 1].clone();
        match options[position].as_str() {
            PROFILE_OPTION => profile = value,
            ENVIRONMENT_OPTION => environment = value,
            RUNTIME_ROOT_OPTION => root = PathBuf::from(value),
            _ => {}
        }
        position += OPTION_AND_VALUE;
    }
    let contract = FoundationContract::embedded();
    let entry = DaemonEntryArguments::new(&root, &profile, &environment);
    let shutdown = tokio_util::sync::CancellationToken::new();
    let runtime = match tokio::runtime::Builder::new_multi_thread().enable_all().build() {
        Ok(runtime) => runtime,
        Err(failure) => {
            write_diagnostic(diagnostics, &failure.to_string());
            return i32::from(EXIT_RUNTIME_UNUSABLE);
        }
    };
    match runtime.block_on(daemon_entry::run_daemon_entry(&contract, &entry, shutdown)) {
        Ok(DaemonEntryOutcome::Served) => i32::from(EXIT_SUCCESS),
        Ok(DaemonEntryOutcome::AlreadyOwned) => i32::from(EXIT_ALREADY_OWNED),
        Err(failure) => {
            write_diagnostic(diagnostics, &failure.to_string());
            i32::from(EXIT_RUNTIME_UNUSABLE)
        }
    }
}

// ------------------------------------------------------- the real boundaries

/// The wall clock.
#[derive(Debug)]
struct ProductClock;

impl ClockBoundary for ProductClock {
    fn milliseconds_since_epoch(&self) -> u64 {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|elapsed| u64::try_from(elapsed.as_millis()).unwrap_or(u64::MAX))
            .unwrap_or_default()
    }
}

/// The configuration this account has.
#[derive(Debug)]
struct ProductConfiguration;

impl ConfigurationBoundary for ProductConfiguration {
    fn check(&self, selection: &Selection) -> CheckReport {
        match loaded_profiles() {
            Ok(loaded) => configuration_check::check(&loaded, selection),
            Err(diagnostics) => CheckReport::Refused { diagnostics },
        }
    }
}

/// Returns the profiles this account's configuration root holds.
#[cfg(unix)]
fn loaded_profiles() -> Result<LoadedProfiles, Vec<ConfigurationDiagnostic>> {
    use slingshot_configuration::configuration_root::{
        ConfigurationRoot, OperatingSystemAccountResolver,
    };
    use slingshot_configuration::credential_filesystem::UnixConfigurationFilesystem;
    use slingshot_configuration::profile_loader::load_profiles;
    let root = ConfigurationRoot::resolve(&OperatingSystemAccountResolver)
        .map_err(|failure| vec![root_diagnostic(failure.code, failure.structural_location)])?;
    let authority = UnixConfigurationFilesystem::new(root)
        .map_err(|failure| vec![authority_diagnostic(failure.code, failure.structural_location)])?;
    load_profiles(authority)
}

/// Returns the profiles this account's configuration root holds.
#[cfg(windows)]
fn loaded_profiles() -> Result<LoadedProfiles, Vec<ConfigurationDiagnostic>> {
    use slingshot_configuration::configuration_root::{
        ConfigurationRoot, OperatingSystemAccountResolver,
    };
    use slingshot_configuration::credential_filesystem::WindowsConfigurationFilesystem;
    use slingshot_configuration::profile_loader::load_profiles;
    let root = ConfigurationRoot::resolve(&OperatingSystemAccountResolver)
        .map_err(|failure| vec![root_diagnostic(failure.code, failure.structural_location)])?;
    let authority = WindowsConfigurationFilesystem::new(root)
        .map_err(|failure| vec![authority_diagnostic(failure.code, failure.structural_location)])?;
    load_profiles(authority)
}

/// Returns the diagnostic one unresolvable configuration root produces.
///
/// The configuration crate's own closed vocabulary, unchanged. A root that
/// cannot be resolved is a root-resolution failure of the root itself, and
/// saying it in any other words would put a second vocabulary beside the one
/// every other configuration failure already speaks.
fn root_diagnostic(
    code: ConfigurationFailureCode,
    structural_location: &'static str,
) -> ConfigurationDiagnostic {
    ConfigurationDiagnostic::once(
        DiagnosticSourceClass::ConfigurationRoot,
        DiagnosticStage::RootResolution,
        structural_location,
        code,
    )
}

/// Returns the diagnostic one refused configuration root produces.
fn authority_diagnostic(
    code: ConfigurationFailureCode,
    structural_location: &'static str,
) -> ConfigurationDiagnostic {
    ConfigurationDiagnostic::once(
        DiagnosticSourceClass::ConfigurationRoot,
        DiagnosticStage::FilesystemAuthority,
        structural_location,
        code,
    )
}

/// The filesystem a caller asked something to be written to.
#[derive(Debug)]
struct ProductFilesystem;

impl FilesystemBoundary for ProductFilesystem {
    fn place(&self, destination: &Path, bytes: &[u8]) -> Result<(), String> {
        std::fs::write(destination, bytes).map_err(|failure| failure.to_string())
    }
}

/// The network, which nothing here reaches.
#[derive(Debug)]
struct ProductNetwork;

impl NetworkBoundary for ProductNetwork {
    fn authority_answers(&self, _authority: &str) -> bool {
        false
    }
}

/// Whether somebody has asked this run to stop.
#[derive(Debug)]
struct ProductSignals {
    /// Set once, by the thread that waits for the signal.
    requested: Arc<AtomicBool>,
}

impl ProductSignals {
    /// Returns the flag this boundary answers from.
    ///
    /// Shared with the exchanges, so a signal that arrives while one is waiting
    /// ends the wait instead of being noticed after it. A run blocked on a
    /// daemon that never answers is exactly when somebody presses this.
    fn flag(&self) -> Arc<AtomicBool> {
        Arc::clone(&self.requested)
    }

    /// Returns a boundary watching for the interrupt a person types.
    fn watching() -> Self {
        let requested = Arc::new(AtomicBool::new(false));
        let armed = Arc::clone(&requested);
        std::thread::spawn(move || {
            let Ok(runtime) = tokio::runtime::Builder::new_current_thread().enable_all().build()
            else {
                return;
            };
            if runtime.block_on(tokio::signal::ctrl_c()).is_ok() {
                armed.store(true, Ordering::SeqCst);
            }
        });
        Self { requested }
    }
}

impl SignalBoundary for ProductSignals {
    fn stop_requested(&self) -> bool {
        self.requested.load(Ordering::SeqCst)
    }
}

/// Creating the daemon that owns a namespace.
struct ProductProcess<'contract> {
    /// The contract a start runs under.
    contract: &'contract FoundationContract,
    /// This executable, which the child is created from.
    executable: PathBuf,
    /// Where the namespace's objects live.
    runtime_root: PathBuf,
}

impl ProcessBoundary for ProductProcess<'_> {
    fn start_daemon(&self, namespace: &NamespacePair) -> Result<(), String> {
        let target = TargetRuntime {
            runtime_root: self.runtime_root.clone(),
            profile: namespace.profile.clone(),
            environment: namespace.environment.clone(),
        };
        let runtime = tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .build()
            .map_err(|failure| failure.to_string())?;
        runtime
            .block_on(explicit_daemon_start::explicit_start(
                self.contract,
                &target,
                &self.executable,
                CONTROL_REQUEST_IDENTIFIER,
            ))
            .map(|_| ())
            .map_err(|failure| failure.to_string())
    }
}

/// Waits until somebody asks this run to stop.
async fn stopped(requested: &AtomicBool) {
    while !requested.load(Ordering::SeqCst) {
        tokio::time::sleep(STOP_POLL_INTERVAL).await;
    }
}

/// Talking to the daemon that owns a namespace.
struct ProductDaemon<'contract> {
    /// The contract every frame is written under.
    contract: &'contract FoundationContract,
    /// Where the namespace's objects live.
    runtime_root: PathBuf,
    /// The runtime every exchange is driven on.
    runtime: Option<tokio::runtime::Runtime>,
    /// Whether somebody has asked this run to stop.
    stop_requested: Arc<AtomicBool>,
}

impl<'contract> ProductDaemon<'contract> {
    /// Returns a boundary that reaches daemons under one runtime root.
    fn new(
        contract: &'contract FoundationContract,
        runtime_root: &Path,
        stop_requested: Arc<AtomicBool>,
    ) -> Self {
        Self {
            contract,
            runtime_root: runtime_root.to_path_buf(),
            runtime: tokio::runtime::Builder::new_multi_thread().enable_all().build().ok(),
            stop_requested,
        }
    }

    /// Returns where the daemon owning `namespace` listens.
    fn address(&self, namespace: &NamespacePair) -> Result<EndpointAddress, ExchangeFailure> {
        let named = RuntimeNamespace::name(
            self.contract,
            &self.runtime_root,
            &namespace.profile,
            &namespace.environment,
        )
        .map_err(|failure| ExchangeFailure::Transport(failure.to_string()))?;
        endpoint::endpoint_address(self.contract, &self.runtime_root, named.digest())
            .map_err(|failure| ExchangeFailure::Transport(failure.to_string()))
    }

    /// Runs one exchange to completion.
    fn driven<Answered>(
        &self,
        work: impl std::future::Future<Output = Result<Answered, ExchangeFailure>>,
    ) -> Result<Answered, ExchangeFailure> {
        let runtime = self
            .runtime
            .as_ref()
            .ok_or_else(|| ExchangeFailure::Transport("no runtime could be built".to_owned()))?;
        runtime.block_on(async {
            tokio::select! {
                answered = work => answered,
                () = stopped(&self.stop_requested) => {
                    Err(ExchangeFailure::Transport(STOP_REQUESTED.to_owned()))
                }
            }
        })
    }

    /// Sends one retained control request and returns what came back.
    fn control(
        &self,
        namespace: &NamespacePair,
        method: &str,
        arguments: serde_json::Value,
    ) -> Result<ControlResponse, ExchangeFailure> {
        let address = self.address(namespace)?;
        let request = ControlRequest {
            control_version: self.contract.control.version,
            request_identifier: CONTROL_REQUEST_IDENTIFIER.to_owned(),
            method: method.to_owned(),
            arguments,
        };
        self.driven(daemon_connection::exchange(self.contract, &address, &request))
    }
}

impl DaemonBoundary for ProductDaemon<'_> {
    fn owner_nonce(&self, namespace: &NamespacePair) -> Result<Option<String>, ExchangeFailure> {
        let address = self.address(namespace)?;
        match self.driven(daemon_connection::ping(
            self.contract,
            &address,
            CONTROL_REQUEST_IDENTIFIER,
        )) {
            Ok(result) => Ok(Some(result.readiness_nonce)),
            Err(ExchangeFailure::Absent(_)) => Ok(None),
            Err(failure) => Err(failure),
        }
    }

    fn hello(&self, namespace: &NamespacePair) -> Result<HelloResult, ExchangeFailure> {
        let response = self.control(namespace, HELLO_METHOD, serde_json::json!({}))?;
        match (response.outcome, response.result, response.error) {
            (ResponseOutcome::Success, Some(result), _) => serde_json::from_value(result)
                .map_err(|failure| ExchangeFailure::Unreadable(failure.to_string())),
            (_, _, Some(error)) => {
                Err(ExchangeFailure::Refused { code: error.code, message: error.message })
            }
            _ => Err(ExchangeFailure::Unreadable("the greeting carried nothing".to_owned())),
        }
    }

    fn stop(
        &self,
        namespace: &NamespacePair,
        readiness_nonce: &str,
    ) -> Result<(), ExchangeFailure> {
        let arguments = serde_json::json!({ "readiness_nonce": readiness_nonce });
        let response = self.control(namespace, STOP_METHOD, arguments)?;
        match (response.outcome, response.error) {
            (ResponseOutcome::Success, _) => Ok(()),
            (_, Some(error)) => {
                Err(ExchangeFailure::Refused { code: error.code, message: error.message })
            }
            _ => Err(ExchangeFailure::Unreadable("the stop carried nothing".to_owned())),
        }
    }

    fn operate(
        &self,
        namespace: &NamespacePair,
        envelope: &OperationEnvelope,
    ) -> Result<OperationResponse, ExchangeFailure> {
        let address = self.address(namespace)?;
        self.driven(daemon_connection::exchange_operation(self.contract, &address, envelope))
    }
}
