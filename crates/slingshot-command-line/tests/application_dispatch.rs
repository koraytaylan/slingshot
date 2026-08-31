//! One invocation, one service, one exit.
//!
//! Not zero and not two. A leaf that fell through would leave a caller with a
//! successful exit and nothing done; one that reached two services would
//! perform an unowned side effect on the way to the one it meant. So the
//! routing is walked exhaustively - every local leaf in the fixture and every
//! published command from the registry - and a leaf that routes nowhere fails
//! here rather than at a caller's terminal.
//!
//! Provenance is checked once, in the dispatcher, rather than in each service.
//! A gate per service is a gate somebody eventually forgets, and the path they
//! forget is the one that reaches a versioned daemon without agreeing with it.
//! The suite asserts that every versioned service is refused when the contracts
//! disagree and that the three unversioned ones still work - because a daemon
//! being absent or incompatible is exactly when somebody runs them.
//!
//! The dispatch matrix and the module inventory are compared against each
//! other, not each against itself, so a family added to one and not the other
//! is a finding.

use std::cell::Cell;
use std::path::Path;

use slingshot_command_line::application::{
    Answer, ClockBoundary, CommandLineApplication, Completion, ConfigurationBoundary,
    DaemonBoundary, DispatchRefusal, FilesystemBoundary, MAINTENANCE_LEAVES, NetworkBoundary,
    OBSERVATION_LEAVES, ProcessBoundary, Provenance, Service, SignalBoundary,
    needs_complete_target, require_dispatchable, service_for,
};
use slingshot_command_line::command_line;
use slingshot_command_line::configuration_check::{CheckReport, ResolvedFacts};
use slingshot_command_line::daemon_connection::ExchangeFailure;
use slingshot_command_line::exit_classification::{EVERY_EXIT, INTERRUPTED, UNAVAILABLE};
use slingshot_command_line::invocation::{
    EXPECTED_REVISION_OPTION, Invocation, LOCAL_LEAVES, METADATA_ONLY_LEAVES, Selection,
    TARGET_DIGEST_OPTION, requires_operation_key,
};
use slingshot_command_line::target_selection::NamespacePair;
use slingshot_domain::command::catalog::CommandCatalog;
use slingshot_local_protocol::control::HelloResult;
use slingshot_local_protocol::message::{OperationEnvelope, OperationResponse};

/// Profile every scenario names.
const PROFILE: &str = "local";

/// Environment every scenario names.
const ENVIRONMENT: &str = "author";

/// The target digest the scenario daemon serves.
const DIGEST: &str = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";

/// A target digest no scenario daemon serves.
const OTHER_DIGEST: &str = "fedcba9876543210fedcba9876543210fedcba9876543210fedcba9876543210";

/// A contract digest no build carries.
const DRIFTED_DIGEST: &str = "1111111111111111111111111111111111111111111111111111111111111111";

/// The environment revision the scenario daemon resolved.
const REVISION: &str = "abcdefabcdefabcdefabcdefabcdefabcdefabcdefabcdefabcdefabcdefabcd";

/// An environment revision no scenario daemon resolved.
const OTHER_REVISION: &str = "9999999999999999999999999999999999999999999999999999999999999999";

/// The readiness nonce the scenario daemon published.
const NONCE: &str = "3c3c3c3c3c3c3c3c3c3c3c3c3c3c3c3c";

/// The namespace the scenario daemon owns.
const NAMESPACE: &str = "local/author";

/// The endpoint the scenario daemon listens on.
const ENDPOINT: &str = "/scenario/endpoint";

/// The version the scenario daemon reports.
const PRODUCT_VERSION: &str = "0.1.0";

/// The operation-protocol version every side of a scenario speaks.
const SPOKEN_VERSION: u32 = 1;

/// The operation a scenario daemon admits.
const OPERATION_IDENTIFIER: &str = "scenario-operation";

/// The instant every scenario clock reports.
const FIXED_MILLISECONDS: u64 = 1_700_000_000_000;

/// The exit an unavailable daemon produces.
const UNAVAILABLE_EXIT: i32 = UNAVAILABLE;

/// The exit an interrupted run produces.
const INTERRUPTED_EXIT: i32 = INTERRUPTED;

/// Every argument vector the binary entry is pinned against, and its exit.
const EVERY_BINARY_CASE: &[(&[&str], i32)] =
    &[(&["--version"], 0), (&["help"], 0), (&["not-a-command"], 2), (&["daemon", "ping"], 2)];

/// Where the dispatch matrix lives.
const FIXTURES: &str = "tests/fixtures/application-dispatch";

/// Where the module inventory the scaffold fixed lives.
const SCAFFOLD_FIXTURE: &str = "tests/fixtures/command-line-module-scaffold/leaves.txt";

/// The parent the command family leaves are declared by.
const COMMAND_FAMILY: &str = "commands";

/// Returns every row of the dispatch matrix.
fn matrix() -> Vec<serde_json::Value> {
    let text = std::fs::read_to_string(format!("{FIXTURES}/leaves.jsonl"))
        .expect("the matrix is readable");
    text.lines().map(|line| serde_json::from_str(line).expect("each line is one row")).collect()
}

/// Returns the command families the matrix names, in order.
fn families() -> Vec<String> {
    let text = std::fs::read_to_string(format!("{FIXTURES}/command-families.txt"))
        .expect("the families are readable");
    text.lines()
        .map(str::trim)
        .filter(|line| !line.is_empty() && !line.starts_with('#'))
        .map(str::to_owned)
        .collect()
}

/// Returns one invocation of `leaf`, carrying whatever it insists on.
fn invocation(leaf: &str) -> Invocation {
    Invocation {
        arguments: std::collections::BTreeMap::new(),
        detached: false,
        operation_key: requires_operation_key(leaf).then(|| "operation-one".to_owned()),
        output: None,
        selection: Selection::default(),
        verb: leaf.to_owned(),
    }
}

/// Returns how one service is spelled in the matrix.
fn service_spelling(service: Service) -> &'static str {
    match service {
        Service::Metadata => "metadata",
        Service::ConfigurationCheck => "configuration-check",
        Service::DaemonLifecycle => "daemon-lifecycle",
        Service::OperationSubmission => "operation-submission",
        Service::OperationObservation => "operation-observation",
        Service::OperationMaintenance => "operation-maintenance",
        Service::ModelContextProtocolServer => "model-context-protocol-server",
    }
}

#[test]
fn every_local_leaf_reaches_the_service_the_matrix_names() {
    for row in matrix() {
        let leaf = row["leaf"].as_str().expect("a leaf");
        let service =
            service_for(&invocation(leaf)).unwrap_or_else(|refusal| panic!("{leaf}: {refusal}"));
        assert_eq!(
            service_spelling(service),
            row["service"].as_str().expect("a service"),
            "{leaf}"
        );
    }
}

#[test]
fn every_leaf_this_build_offers_routes_somewhere_and_nothing_falls_through() {
    for leaf in LOCAL_LEAVES {
        service_for(&invocation(leaf))
            .unwrap_or_else(|refusal| panic!("{leaf} routes nowhere: {refusal}"));
    }
    for descriptor in CommandCatalog::published().descriptors() {
        let leaf = descriptor.wire_name.as_str();
        assert_eq!(
            service_for(&invocation(leaf)).unwrap_or_else(|refusal| panic!("{leaf}: {refusal}")),
            Service::OperationSubmission,
            "{leaf}: a command reaches the daemon by being submitted, like every other"
        );
    }
    assert_eq!(
        service_for(&invocation("teleport")),
        Err(DispatchRefusal::Unroutable { named: "teleport".to_owned() }),
        "and a leaf nobody routes is a defect here rather than a surprise at a terminal"
    );
}

#[test]
fn no_leaf_is_claimed_by_two_services() {
    let mut claimed: Vec<(&str, Service)> = Vec::new();
    for leaf in LOCAL_LEAVES {
        let service = service_for(&invocation(leaf)).expect("it routes");
        assert!(!claimed.iter().any(|(held, _)| held == leaf), "{leaf} is routed more than once");
        claimed.push((leaf, service));
    }
    for leaf in OBSERVATION_LEAVES {
        assert!(LOCAL_LEAVES.contains(leaf), "{leaf} is a local leaf the parser knows");
    }
    for leaf in MAINTENANCE_LEAVES {
        assert!(LOCAL_LEAVES.contains(leaf), "{leaf} is a local leaf the parser knows");
    }
    let overlap = OBSERVATION_LEAVES.iter().any(|leaf| MAINTENANCE_LEAVES.contains(leaf));
    assert!(!overlap, "the two tables are disjoint, so no leaf reaches two services");
}

#[test]
fn provenance_is_checked_once_and_the_three_unversioned_services_survive_it() {
    let catalog = CommandCatalog::published();
    let every: Vec<String> = LOCAL_LEAVES
        .iter()
        .map(|leaf| (*leaf).to_owned())
        .chain(catalog.descriptors().iter().map(|descriptor| descriptor.wire_name.clone()))
        .collect();
    for leaf in &every {
        let asked = invocation(leaf);
        let service = service_for(&asked).expect("it routes");
        let refused = require_dispatchable(&asked, false);
        if service.is_versioned() {
            assert_eq!(
                refused,
                Err(DispatchRefusal::ProvenanceRefused),
                "{leaf}: a gate per service is a gate somebody eventually forgets"
            );
        } else {
            assert_eq!(
                refused,
                Ok(service),
                "{leaf}: a daemon being absent or incompatible is exactly when this is run"
            );
        }
        assert_eq!(require_dispatchable(&asked, true), Ok(service), "{leaf}");
    }
}

#[test]
fn only_the_services_that_talk_to_a_versioned_daemon_are_versioned() {
    for service in [Service::Metadata, Service::ConfigurationCheck, Service::DaemonLifecycle] {
        assert!(!service.is_versioned(), "{service:?} must keep working without a daemon");
    }
    for service in
        [Service::OperationSubmission, Service::OperationObservation, Service::OperationMaintenance]
    {
        assert!(service.is_versioned(), "{service:?} speaks a versioned protocol");
    }
}

#[test]
fn the_target_a_leaf_needs_is_read_from_one_table_and_not_two() {
    for leaf in METADATA_ONLY_LEAVES {
        assert!(!needs_complete_target(&invocation(leaf)), "{leaf} needs no target");
    }
    for descriptor in CommandCatalog::published().descriptors() {
        assert!(
            needs_complete_target(&invocation(&descriptor.wire_name)),
            "{}: it acts against an author",
            descriptor.wire_name
        );
    }
}

#[test]
fn the_dispatch_matrix_and_the_module_inventory_name_the_same_eight_families() {
    let inventory = std::fs::read_to_string(SCAFFOLD_FIXTURE).expect("the scaffold is readable");
    let mut declared: Vec<(usize, String)> = inventory
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty() && !line.starts_with('#'))
        .filter_map(|line| {
            let mut parts = line.split('|');
            let path = parts.next()?.to_owned();
            let parent = parts.next()?;
            let position = parts.next()?;
            if parent != COMMAND_FAMILY {
                return None;
            }
            let name = path.rsplit('/').next()?.strip_suffix(".rs")?.to_owned();
            Some((position.parse().ok()?, name))
        })
        .collect();
    declared.sort_by_key(|(position, _)| *position);
    let ordered: Vec<String> = declared.into_iter().map(|(_, name)| name).collect();
    assert_eq!(
        ordered,
        families(),
        "a family added to one fixture and not the other is a finding rather than a detail"
    );
}

// ------------------------------------------------- what each service touches

/// How many times each boundary was reached.
#[derive(Debug, Default)]
struct Reached {
    /// Configuration reads.
    configuration: Cell<u32>,
    /// Daemon exchanges of every kind.
    daemon: Cell<u32>,
    /// Versioned operation exchanges alone.
    operations: Cell<u32>,
    /// Files written.
    filesystem: Cell<u32>,
    /// Networks reached.
    network: Cell<u32>,
    /// Daemons created.
    process: Cell<u32>,
}

impl Reached {
    /// Returns how many boundaries were reached at all.
    fn total(&self) -> u32 {
        self.configuration.get()
            + self.daemon.get()
            + self.filesystem.get()
            + self.network.get()
            + self.process.get()
    }

    /// Records one call against one counter.
    fn counted(counter: &Cell<u32>) {
        counter.set(counter.get() + 1);
    }
}

/// Every boundary, counting what it was asked and answering the scenario.
#[derive(Debug)]
struct Fakes {
    /// What the configuration answers.
    report: CheckReport,
    /// What the daemon answers when greeted.
    greeting: Option<HelloResult>,
    /// What the daemon answers a versioned request with.
    answer: OperationResponse,
    /// Whether an owner is serving.
    owner: Option<String>,
    /// Whether a signal arrived.
    interrupted: bool,
    /// What was reached.
    reached: Reached,
}

impl Default for Fakes {
    fn default() -> Self {
        Self {
            report: CheckReport::Resolved(Box::new(ResolvedFacts {
                environment: ENVIRONMENT.to_owned(),
                profile: PROFILE.to_owned(),
                warned_cleartext_transport: false,
            })),
            greeting: Some(greeting(DIGEST, REVISION)),
            answer: OperationResponse::Accepted {
                operation_identifier: OPERATION_IDENTIFIER.to_owned(),
            },
            owner: Some(NONCE.to_owned()),
            interrupted: false,
            reached: Reached::default(),
        }
    }
}

impl ConfigurationBoundary for Fakes {
    fn check(&self, _selection: &Selection) -> CheckReport {
        Reached::counted(&self.reached.configuration);
        self.report.clone()
    }
}

impl FilesystemBoundary for Fakes {
    fn place(&self, _destination: &Path, _bytes: &[u8]) -> Result<(), String> {
        Reached::counted(&self.reached.filesystem);
        Ok(())
    }
}

impl NetworkBoundary for Fakes {
    fn authority_answers(&self, _authority: &str) -> bool {
        Reached::counted(&self.reached.network);
        false
    }
}

impl ProcessBoundary for Fakes {
    fn start_daemon(&self, _namespace: &NamespacePair) -> Result<(), String> {
        Reached::counted(&self.reached.process);
        Ok(())
    }
}

impl ClockBoundary for Fakes {
    fn milliseconds_since_epoch(&self) -> u64 {
        FIXED_MILLISECONDS
    }
}

impl SignalBoundary for Fakes {
    fn stop_requested(&self) -> bool {
        self.interrupted
    }
}

impl DaemonBoundary for Fakes {
    fn owner_nonce(&self, _namespace: &NamespacePair) -> Result<Option<String>, ExchangeFailure> {
        Reached::counted(&self.reached.daemon);
        Ok(self.owner.clone())
    }

    fn hello(&self, _namespace: &NamespacePair) -> Result<HelloResult, ExchangeFailure> {
        Reached::counted(&self.reached.daemon);
        self.greeting.clone().ok_or_else(|| ExchangeFailure::Absent(ENDPOINT.to_owned()))
    }

    fn stop(
        &self,
        _namespace: &NamespacePair,
        _readiness_nonce: &str,
    ) -> Result<(), ExchangeFailure> {
        Reached::counted(&self.reached.daemon);
        Ok(())
    }

    fn operate(
        &self,
        _namespace: &NamespacePair,
        _envelope: &OperationEnvelope,
    ) -> Result<OperationResponse, ExchangeFailure> {
        Reached::counted(&self.reached.daemon);
        Reached::counted(&self.reached.operations);
        Ok(self.answer.clone())
    }
}

/// Returns what a daemon says about itself.
fn greeting(target: &str, revision: &str) -> HelloResult {
    HelloResult {
        author_target_identity_digest: target.to_owned(),
        daemon_runtime_contract_digest: Provenance::embedded().daemon_runtime_contract_digest,
        product_version: PRODUCT_VERSION.to_owned(),
        readiness_nonce: NONCE.to_owned(),
        runtime_namespace: NAMESPACE.to_owned(),
        selected_environment_revision: revision.to_owned(),
        supported_operation_protocol_versions: vec![SPOKEN_VERSION],
    }
}

/// Returns the invocation one leaf and its arguments make.
fn invoking(leaf: &str, arguments: &[(&str, &str)]) -> Invocation {
    Invocation {
        arguments: arguments
            .iter()
            .map(|(named, value)| ((*named).to_owned(), (*value).to_owned()))
            .collect(),
        detached: false,
        operation_key: None,
        output: None,
        selection: Selection {
            environment: Some(ENVIRONMENT.to_owned()),
            profile: Some(PROFILE.to_owned()),
        },
        verb: leaf.to_owned(),
    }
}

/// Runs one invocation against fakes and returns what it produced.
fn against(fakes: &Fakes, provenance: Provenance, invocation: &Invocation) -> Completion {
    let application = CommandLineApplication {
        clock: fakes,
        configuration: fakes,
        daemon: fakes,
        filesystem: fakes,
        network: fakes,
        process: fakes,
        provenance,
        signals: fakes,
    };
    application.run(invocation)
}

#[test]
fn help_and_version_reach_no_boundary_at_all() {
    for leaf in METADATA_ONLY_LEAVES {
        let fakes = Fakes::default();
        let completion = against(&fakes, Provenance::embedded(), &invoking(leaf, &[]));
        assert_eq!(completion.exit, 0, "{leaf}");
        assert!(matches!(completion.answer, Answer::Text(_)), "{leaf}");
        assert_eq!(fakes.reached.total(), 0, "{leaf} reached a boundary");
    }
}

#[test]
fn a_configuration_check_reaches_configuration_and_nothing_else() {
    let fakes = Fakes::default();
    let completion = against(&fakes, Provenance::embedded(), &invoking("check-configuration", &[]));
    assert_eq!(completion.exit, 0);
    assert_eq!(fakes.reached.configuration.get(), 1);
    assert_eq!(fakes.reached.total(), 1, "a check reaches only configuration");
}

#[test]
fn a_versioned_leaf_refused_for_provenance_reaches_no_daemon() {
    let fakes = Fakes::default();
    let drifted = Provenance {
        author_agent_transport_contract_digest: DRIFTED_DIGEST.to_owned(),
        daemon_runtime_contract_digest: Provenance::embedded().daemon_runtime_contract_digest,
    };
    let completion = against(&fakes, drifted, &invoking("operation-list", &[]));
    assert_eq!(completion.exit, UNAVAILABLE_EXIT);
    assert!(matches!(completion.answer, Answer::Refusal(_)));
    assert_eq!(fakes.reached.daemon.get(), 0, "a refused leaf reaches no daemon");
}

#[test]
fn the_three_unversioned_services_survive_a_provenance_refusal() {
    let drifted = Provenance {
        author_agent_transport_contract_digest: DRIFTED_DIGEST.to_owned(),
        daemon_runtime_contract_digest: DRIFTED_DIGEST.to_owned(),
    };
    for leaf in ["help", "version", "check-configuration", "daemon-ping"] {
        let fakes = Fakes::default();
        let completion = against(&fakes, drifted.clone(), &invoking(leaf, &[]));
        assert_eq!(completion.exit, 0, "{leaf}");
    }
}

#[test]
fn a_daemon_backed_leaf_cannot_bypass_target_or_revision_validation() {
    for (option, value) in
        [(TARGET_DIGEST_OPTION, OTHER_DIGEST), (EXPECTED_REVISION_OPTION, OTHER_REVISION)]
    {
        let fakes = Fakes::default();
        let completion = against(
            &fakes,
            Provenance::embedded(),
            &invoking("operation-list", &[(option, value)]),
        );
        assert_eq!(completion.exit, UNAVAILABLE_EXIT, "{option}");
        assert_eq!(fakes.reached.operations.get(), 0, "{option} still sent an operation");
    }
}

#[test]
fn one_invocation_produces_one_answer_and_one_exit() {
    let fakes = Fakes::default();
    let completion = against(&fakes, Provenance::embedded(), &invoking("daemon-status", &[]));
    assert_eq!(fakes.reached.daemon.get(), 1, "a status asks once");
    assert!(matches!(completion.answer, Answer::Envelope(_)));
    assert_eq!(completion.exit, 0);
}

#[test]
fn an_interrupted_run_writes_no_answer_and_exits_one_hundred_and_thirty() {
    let fakes = Fakes { interrupted: true, ..Fakes::default() };
    let completion = against(&fakes, Provenance::embedded(), &invoking("operation-list", &[]));
    assert_eq!(completion.exit, INTERRUPTED_EXIT);
    assert_eq!(fakes.reached.total(), 0, "an interrupted run reaches nothing");
}

#[test]
fn the_binary_maps_classifications_onto_the_documented_exits() {
    for (arguments, expected) in EVERY_BINARY_CASE {
        let mut output = Vec::new();
        let mut diagnostics = Vec::new();
        let named: Vec<String> = arguments.iter().map(|word| (*word).to_owned()).collect();
        let exit = command_line::run(
            &named,
            Path::new(env!("CARGO_BIN_EXE_slingshot")),
            &mut output,
            &mut diagnostics,
        );
        assert_eq!(exit, *expected, "{arguments:?}");
        assert!(EVERY_EXIT.contains(expected), "{expected} is a documented exit");
    }
}
