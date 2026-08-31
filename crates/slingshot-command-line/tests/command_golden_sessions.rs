//! Every session this executable can be driven through, byte for byte.
//!
//! A golden session is one argument vector, one recorded transcript, and one
//! exit. The transcript is compared exactly rather than searched, because the
//! streams are the product's contract with every script that will ever read
//! them: a line that moves between them, a word that changes, or a second line
//! where there was one all break a caller, and all three are invisible to an
//! assertion that only looks for a substring.
//!
//! # What is pinned, and what a daemon would have to answer for
//!
//! The daemon this build talks to serves the retained control surface, so the
//! sessions here pin the whole local surface - parsing, target selection,
//! dispatch, configuration checking, daemon lifecycle, both output forms, and
//! every exit those reach - and they pin what a versioned leaf does when the
//! daemon answers that it does not serve the method. Sessions for an admitted
//! operation, a receipt, a publication, or a terminal failure would need a
//! daemon that answers a versioned request, and asserting them against one that
//! cannot would pin a fiction.
//!
//! # Updating a fixture
//!
//! Run with `SLINGSHOT_REVIEW_COMMAND_GOLDEN_SESSIONS=1` set, then read the
//! diff. A fixture that changes without somebody choosing the change is the
//! defect this suite exists to catch, so the rewrite is deliberate and never
//! automatic.

use std::path::{Path, PathBuf};
use std::time::Duration;

use slingshot_command_line::exit_classification::LOCAL_FAILURE;
use slingshot_command_line::exit_classification::{
    EVERY_EXIT, INTERRUPTED, SUCCESS, UNAVAILABLE, USAGE,
};
use slingshot_command_line::invocation::{LOCAL_LEAVES, METADATA_ONLY_LEAVES};
use slingshot_daemon::platform_runtime::endpoint::{self, EndpointAddress};
use slingshot_daemon::platform_runtime::locks::OwnerLock;
use slingshot_daemon::runtime_namespace::RuntimeNamespace;
use slingshot_domain::command::catalog::CommandCatalog;
use slingshot_local_protocol::foundation_contract::FoundationContract;
use slingshot_test_support::process_harness::{
    CapturedProcess, DeliverableSignal, ExecutablePath, ProcessHarness, ProcessRequest,
};
use slingshot_test_support::runtime_harness::{TemporaryRuntimeRoot, wait_until};

/// Where the scenario sources and their expected bytes live.
const FIXTURE_DIRECTORY: &str = "../slingshot-test-support/fixtures/command-golden-sessions";

/// The scenario source every local session is read from.
const SESSION_SOURCE: &str = "sessions.jsonl";

/// Where one session's expected bytes live.
const EXPECTED_DIRECTORY: &str = "expected";

/// The variable that arms a fixture rewrite.
const REVIEW_VARIABLE: &str = "SLINGSHOT_REVIEW_COMMAND_GOLDEN_SESSIONS";

/// The command a reviewer runs to rewrite a fixture.
const REVIEW_COMMAND: &str = "SLINGSHOT_REVIEW_COMMAND_GOLDEN_SESSIONS=1 \
     cargo test -p slingshot-command-line --test command_golden_sessions";

/// Profile every session names.
const PROFILE: &str = "local";

/// Environment every session names.
const ENVIRONMENT: &str = "author";

/// The one value a transcript replaces, because it moves between runs.
const INVENTED_IDENTIFIER_PREFIX: &str = "command-line-";

/// What an invented identifier is written as once it is normalized.
const NORMALIZED_IDENTIFIER: &str = "command-line-<instant>";

/// What the scenario's temporary root is written as once it is normalized.
const NORMALIZED_ROOT: &str = "<runtime-root>";

/// How long a session waits for a child that should answer at once.
const PROMPT_DEADLINE: Duration = Duration::from_secs(30);

/// How long an interrupted session waits for its child to notice.
const SIGNAL_DEADLINE: Duration = Duration::from_secs(10);

/// The exits the compiled surface reaches, and therefore the ones pinned here.
///
/// The other four - an agent rejection, a remote failure, an indeterminate
/// outcome, and a local failure - are answers about an operation, and reaching
/// one needs a daemon that admits operations. They are classified and proved
/// where the classification lives; pinning a session for one against a daemon
/// that cannot produce it would pin a fiction.
const REACHED_EXITS: &[i32] = &[SUCCESS, USAGE, UNAVAILABLE, INTERRUPTED];

/// Returns the product executable these sessions drive.
fn product_executable() -> ExecutablePath {
    ExecutablePath::new(PathBuf::from(env!("CARGO_BIN_EXE_slingshot")))
        .expect("the product executable was built")
}

/// Returns where the fixtures live.
fn fixtures() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(FIXTURE_DIRECTORY)
}

/// One session, as the source declares it.
#[derive(Debug, Clone, serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct Session {
    /// What this session is called, and what its expected bytes are named.
    name: String,
    /// What it runs.
    arguments: Vec<String>,
    /// Why it is here, for whoever reads the source rather than the bytes.
    intent: String,
    /// Whether it runs against absence or against one real daemon.
    kind: String,
}

/// The kind of session that runs against absence.
const AGAINST_ABSENCE: &str = "local";

/// The kind of session that runs against one real daemon, in order.
const AGAINST_A_DAEMON: &str = "owned";

/// The kind of session that is interrupted while it waits.
const AGAINST_SILENCE: &str = "interrupted";

/// The kind of session whose answer depends on this account's configuration.
///
/// Its bytes cannot be committed: the configuration root is the account's own
/// home, chosen by the operating system rather than by the environment, exactly
/// so that nothing ambient can redirect where a credential is read from. What
/// is pinned instead is everything a caller depends on and nothing that differs
/// between machines: the exit, the single line of answer, and the grammar of
/// every diagnostic.
const AGAINST_THIS_ACCOUNT: &str = "shaped";

/// Returns every declared session.
fn declared_sessions() -> Vec<Session> {
    let path = fixtures().join(SESSION_SOURCE);
    let text = std::fs::read_to_string(&path)
        .unwrap_or_else(|failure| panic!("{} could not be read: {failure}", path.display()));
    text.lines()
        .filter(|line| !line.trim().is_empty() && !line.starts_with('#'))
        .map(|line| serde_json::from_str(line).expect("every session row reads"))
        .collect()
}

/// Returns the transcript one captured run produces.
///
/// Two values are replaced, and only two: the identifier a run invents when the
/// caller supplied none, and the temporary root the scenario chose. Both differ
/// between runs for reasons that have nothing to do with the product, and
/// everything else is compared byte for byte.
fn transcript(root: &Path, captured: &CapturedProcess) -> String {
    let exit = captured.status.code().unwrap_or_default();
    format!(
        "exit: {exit}\nstandard output:\n{}standard error:\n{}",
        normalized(root, &captured.standard_output),
        normalized(root, &captured.standard_error)
    )
}

/// Returns one stream with the two values that move between runs replaced.
fn normalized(root: &Path, stream: &str) -> String {
    let stream = stream.replace(&root.to_string_lossy().into_owned(), NORMALIZED_ROOT);
    let stream = stream.as_str();
    let mut written = String::new();
    let mut scanning = stream;
    while let Some(position) = scanning.find(INVENTED_IDENTIFIER_PREFIX) {
        let (before, after) = scanning.split_at(position);
        written.push_str(before);
        written.push_str(NORMALIZED_IDENTIFIER);
        let digits = after[INVENTED_IDENTIFIER_PREFIX.len()..]
            .find(|character: char| !character.is_ascii_digit())
            .unwrap_or(after.len() - INVENTED_IDENTIFIER_PREFIX.len());
        scanning = &after[INVENTED_IDENTIFIER_PREFIX.len() + digits..];
    }
    written.push_str(scanning);
    written
}

/// Compares one transcript with the bytes committed for it.
fn matches_fixture(name: &str, produced: &str) {
    let path = fixtures().join(EXPECTED_DIRECTORY).join(format!("{name}.txt"));
    if std::env::var(REVIEW_VARIABLE).is_ok() {
        std::fs::write(&path, produced).expect("the expected bytes are written");
        return;
    }
    let expected = std::fs::read_to_string(&path).unwrap_or_else(|failure| {
        panic!(
            "{} could not be read: {failure}; rewrite it with `{REVIEW_COMMAND}`",
            path.display()
        )
    });
    assert_eq!(produced, expected, "{name} changed; review it with `{REVIEW_COMMAND}`");
}

/// Runs one argument vector under a runtime root and returns what it produced.
fn run(root: &Path, arguments: &[String]) -> CapturedProcess {
    let mut named = if takes_a_root(arguments) {
        vec!["--runtime-root".to_owned(), root.to_string_lossy().into_owned()]
    } else {
        Vec::new()
    };
    named.extend(arguments.iter().cloned());
    let words: Vec<&str> = named.iter().map(String::as_str).collect();
    let harness = ProcessHarness::new();
    harness
        .run_within(&product_executable(), &ProcessRequest::new(&words), PROMPT_DEADLINE)
        .expect("the product executable runs")
}

/// Returns whether one session's leaf takes a runtime root.
///
/// A metadata leaf takes no options at all, which is the point of it: help and
/// version answer out of this build and reach nothing that a root could move.
fn takes_a_root(arguments: &[String]) -> bool {
    !arguments.iter().any(|word| {
        METADATA_ONLY_LEAVES.contains(&word.as_str()) || METADATA_OPTIONS.contains(&word.as_str())
    })
}

/// The options that name a metadata leaf.
const METADATA_OPTIONS: &[&str] = &["--version", "--help"];

/// Returns the target words every session that names one carries.
fn addressing(environment: &str) -> Vec<String> {
    vec![
        "--profile".to_owned(),
        PROFILE.to_owned(),
        "--environment".to_owned(),
        environment.to_owned(),
    ]
}

#[test]
fn every_session_against_absence_produces_the_bytes_committed_for_it() {
    let root = TemporaryRuntimeRoot::create("g").expect("the temporary root is created");
    for session in declared_sessions().iter().filter(|row| row.kind == AGAINST_ABSENCE) {
        let produced = run(root.path(), &session.arguments);
        matches_fixture(&session.name, &transcript(root.path(), &produced));
    }
    assert!(
        !OwnerLock::path_for(root.path(), "unused").exists(),
        "a session against absence creates no owner"
    );
}

#[test]
fn every_leaf_this_build_offers_has_a_session() {
    let sessions = declared_sessions();
    let written: Vec<String> = sessions.iter().map(|session| session.arguments.join(" ")).collect();
    let published = CommandCatalog::published();
    let every_leaf = LOCAL_LEAVES
        .iter()
        .map(|leaf| (*leaf).to_owned())
        .chain(published.descriptors().iter().map(|descriptor| descriptor.wire_name.clone()));
    for leaf in every_leaf {
        let spelled = leaf.replace('-', " ");
        assert!(
            written.iter().any(|run| run.contains(&leaf) || run.contains(&spelled)),
            "{leaf} has no golden session"
        );
    }
    assert!(sessions.iter().all(|session| !session.intent.is_empty()), "every session says why");
}

#[test]
fn every_session_is_named_once_and_has_its_bytes() {
    let mut named: Vec<String> = declared_sessions().into_iter().map(|row| row.name).collect();
    let declared = named.len();
    named.sort();
    named.dedup();
    assert_eq!(named.len(), declared, "a session name is used twice");
    let pinned: Vec<String> = declared_sessions()
        .into_iter()
        .filter(|row| row.kind != AGAINST_THIS_ACCOUNT)
        .map(|row| row.name)
        .collect();
    for name in &pinned {
        let path = fixtures().join(EXPECTED_DIRECTORY).join(format!("{name}.txt"));
        assert!(path.is_file(), "{name} has no expected bytes; write them with `{REVIEW_COMMAND}`");
    }
    let held = std::fs::read_dir(fixtures().join(EXPECTED_DIRECTORY))
        .expect("the expected directory is readable")
        .filter_map(Result::ok)
        .filter(|entry| entry.path().extension().is_some_and(|held| held == "txt"))
        .count();
    assert_eq!(held, pinned.len(), "an expected file belongs to no session");
}

#[test]
fn the_sessions_reach_every_exit_this_surface_produces() {
    let root = TemporaryRuntimeRoot::create("x").expect("the temporary root is created");
    let mut reached = Vec::new();
    for session in declared_sessions().iter().filter(|row| row.kind == AGAINST_ABSENCE) {
        let produced = run(root.path(), &session.arguments);
        reached.push(produced.status.code().unwrap_or_default());
    }
    for exit in REACHED_EXITS {
        if *exit == INTERRUPTED {
            continue;
        }
        assert!(reached.contains(exit), "no session exits {exit}");
    }
    for exit in &reached {
        assert!(EVERY_EXIT.contains(exit), "{exit} is not a documented exit");
    }
}

/// Returns whether nothing owns one namespace.
fn owner_is_free(root: &Path, digest: &str) -> bool {
    OwnerLock::acquire(root, digest).expect("the lock file opens").is_some()
}

/// Returns the namespace one environment names under a root.
fn namespace_of(root: &Path, environment: &str) -> RuntimeNamespace {
    RuntimeNamespace::name(&FoundationContract::embedded(), root, PROFILE, environment)
        .expect("the target names a namespace")
}

/// Sends one nonce-bound stop and reports whether the daemon accepted it.
fn stop_quoting(address: &EndpointAddress, nonce: &str) -> bool {
    let contract = FoundationContract::embedded();
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("the runtime builds");
    runtime.block_on(async {
        let request = slingshot_local_protocol::envelope::ControlRequest {
            control_version: contract.control.version,
            request_identifier: "golden-session".to_owned(),
            method: slingshot_local_protocol::ping::STOP_METHOD.to_owned(),
            arguments: serde_json::json!({ "readiness_nonce": nonce }),
        };
        slingshot_command_line::daemon_connection::exchange(&contract, address, &request)
            .await
            .is_ok_and(|response| {
                response.outcome == slingshot_local_protocol::envelope::ResponseOutcome::Success
            })
    })
}

#[test]
fn the_owned_sessions_run_in_order_against_one_real_daemon() {
    let root = TemporaryRuntimeRoot::create("o").expect("the temporary root is created");
    let namespace = namespace_of(root.path(), ENVIRONMENT);
    for session in declared_sessions().iter().filter(|row| row.kind == AGAINST_A_DAEMON) {
        let produced = run(root.path(), &session.arguments);
        matches_fixture(&session.name, &transcript(root.path(), &produced));
    }
    assert!(
        wait_until(FoundationContract::embedded().shutdown.cooperative_stop(), || owner_is_free(
            root.path(),
            namespace.digest()
        )),
        "the last owned session stopped the daemon the first one created"
    );
}

#[test]
fn a_stale_nonce_never_stops_the_replacement_that_followed_it() {
    let contract = FoundationContract::embedded();
    let root = TemporaryRuntimeRoot::create("s").expect("the temporary root is created");
    let namespace = namespace_of(root.path(), ENVIRONMENT);
    let address = endpoint::endpoint_address(&contract, root.path(), namespace.digest())
        .expect("the endpoint is named");

    let created = run(
        root.path(),
        &[addressing(ENVIRONMENT), vec!["daemon".to_owned(), "start".to_owned()]].concat(),
    );
    assert!(created.status.success(), "{created:?}");
    let stale = published_nonce(root.path(), &namespace).expect("the first owner published one");
    assert!(stop_quoting(&address, &stale), "the live nonce stops the daemon it names");
    assert!(
        wait_until(contract.shutdown.cooperative_stop(), || owner_is_free(
            root.path(),
            namespace.digest()
        )),
        "the stopped daemon released its owner lock"
    );

    let replaced = run(
        root.path(),
        &[addressing(ENVIRONMENT), vec!["daemon".to_owned(), "start".to_owned()]].concat(),
    );
    assert!(replaced.status.success(), "{replaced:?}");
    let fresh = published_nonce(root.path(), &namespace).expect("the replacement published one");
    assert_ne!(fresh, stale, "a replacement publishes its own nonce");
    assert!(!stop_quoting(&address, &stale), "a stale nonce cannot stop the replacement");
    assert!(!owner_is_free(root.path(), namespace.digest()), "the replacement is still serving");
    assert!(stop_quoting(&address, &fresh), "its own nonce still stops it");
    assert!(wait_until(contract.shutdown.cooperative_stop(), || owner_is_free(
        root.path(),
        namespace.digest()
    )));
}

/// Returns the nonce the daemon owning one namespace published.
fn published_nonce(root: &Path, namespace: &RuntimeNamespace) -> Option<String> {
    slingshot_daemon::platform_runtime::readiness::read(root, namespace.digest())
        .expect("the record is readable")
        .map(|record| record.readiness_nonce)
}

#[test]
fn an_unresponsive_owned_child_ends_through_its_retained_handle() {
    let contract = FoundationContract::embedded();
    let root = TemporaryRuntimeRoot::create("c").expect("the temporary root is created");
    let namespace = namespace_of(root.path(), ENVIRONMENT);
    let harness = ProcessHarness::new();
    let words = [
        vec!["--runtime-root".to_owned(), root.path().to_string_lossy().into_owned()],
        addressing(ENVIRONMENT),
        vec!["daemon".to_owned(), "serve".to_owned()],
    ]
    .concat();
    let spoken: Vec<&str> = words.iter().map(String::as_str).collect();
    let mut child = harness
        .start_retained(&product_executable(), &ProcessRequest::new(&spoken))
        .expect("the daemon child starts");
    assert!(
        wait_until(contract.startup.explicit_start_total(), || !owner_is_free(
            root.path(),
            namespace.digest()
        )),
        "the child took ownership"
    );
    let identifier = child.identifier();
    child.end_within(PROMPT_DEADLINE).expect("the child ended through its handle");
    assert!(child.is_reaped());
    assert!(child.deliver(DeliverableSignal::Kill).is_err(), "a reaped handle reaches nothing");
    assert!(
        wait_until(contract.shutdown.cooperative_stop(), || owner_is_free(
            root.path(),
            namespace.digest()
        )),
        "the ended child released its owner lock"
    );
    assert_ne!(identifier, 0, "the identifier is recorded as a diagnostic");
}

/// Accepts one connection and never answers it.
///
/// A daemon that is there and silent is what makes an interrupt observable: the
/// run gets past connecting and is waiting for an answer when the signal
/// arrives, which is the phase the account it prints describes.
fn silent_endpoint(address: &EndpointAddress) -> std::thread::JoinHandle<()> {
    let EndpointAddress::UnixDomainSocket(path) = address;
    let path = path.clone();
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).expect("the endpoint directory exists");
    }
    let listener = std::os::unix::net::UnixListener::bind(&path).expect("the endpoint binds");
    std::thread::spawn(move || {
        let mut held = Vec::new();
        while let Ok((stream, _)) = listener.accept() {
            held.push(stream);
        }
    })
}

/// Runs one invocation against a silent endpoint and interrupts it.
fn interrupted_session(root: &Path, arguments: &[String]) -> CapturedProcess {
    let words = [
        vec!["--runtime-root".to_owned(), root.to_string_lossy().into_owned()],
        arguments.to_vec(),
    ]
    .concat();
    let spoken: Vec<&str> = words.iter().map(String::as_str).collect();
    let harness = ProcessHarness::new();
    let mut child = harness
        .start_retained(&product_executable(), &ProcessRequest::new(&spoken))
        .expect("the child starts");
    if child.wait_within(SETTLING_DEADLINE).is_ok() {
        let finished = child.capture_within(SIGNAL_DEADLINE).unwrap_or_else(|_| {
            panic!("the child finished before the signal and could not be read")
        });
        panic!("the child finished before the signal: {}", transcript(root, &finished));
    }
    child.deliver(DeliverableSignal::Interrupt).expect("the interrupt is delivered");
    child.capture_within(SIGNAL_DEADLINE).expect("the interrupted child finishes")
}

/// How long a session waits to be sure a child is blocked rather than done.
const SETTLING_DEADLINE: Duration = Duration::from_millis(400);

#[test]
fn an_interrupted_run_says_how_far_it_got_and_exits_one_hundred_and_thirty() {
    let contract = FoundationContract::embedded();
    let root = TemporaryRuntimeRoot::create("i").expect("the temporary root is created");
    let namespace = namespace_of(root.path(), ENVIRONMENT);
    let address = endpoint::endpoint_address(&contract, root.path(), namespace.digest())
        .expect("the endpoint is named");
    let listening = silent_endpoint(&address);

    for session in declared_sessions().iter().filter(|row| row.kind == AGAINST_SILENCE) {
        let produced = interrupted_session(root.path(), &session.arguments);
        assert_eq!(produced.status.code(), Some(INTERRUPTED), "{}", session.name);
        let (written, claim) = if session.arguments.iter().any(|word| word == "--machine") {
            (&produced.standard_error, "a machine run writes one envelope and nothing else")
        } else {
            (&produced.standard_output, "an interrupted run publishes no answer")
        };
        assert!(written.is_empty(), "{}: {claim}", session.name);
        matches_fixture(&session.name, &transcript(root.path(), &produced));
    }
    assert!(owner_is_free(root.path(), namespace.digest()), "an interrupted run owns nothing");
    drop(listening);
}

/// How many fields one closed configuration diagnostic is written with.
const DIAGNOSTIC_FIELDS: usize = 5;

/// What every diagnostic this executable writes begins with.
const DIAGNOSTIC_PREFIX: &str = "slingshot: ";

/// What the count field of a diagnostic begins with.
const COUNT_MARK: &str = "x";

#[test]
fn a_configuration_check_says_what_is_wrong_without_saying_where() {
    let root = TemporaryRuntimeRoot::create("k").expect("the temporary root is created");
    let home = std::env::var("HOME").unwrap_or_default();
    for session in declared_sessions().iter().filter(|row| row.kind == AGAINST_THIS_ACCOUNT) {
        let produced = run(root.path(), &session.arguments);
        let exit = produced.status.code().unwrap_or_default();
        assert!(
            exit == SUCCESS || exit == LOCAL_FAILURE,
            "{} exits {exit}, which a check never does",
            session.name
        );
        let answered: Vec<&str> = produced.standard_output.lines().collect();
        assert_eq!(answered.len(), 1, "{} answers on one line", session.name);
        for line in produced.standard_error.lines() {
            let stated = line.strip_prefix(DIAGNOSTIC_PREFIX).unwrap_or_else(|| {
                panic!("{} writes a diagnostic that names itself: {line}", session.name)
            });
            let fields: Vec<&str> = stated.split(' ').collect();
            assert_eq!(fields.len(), DIAGNOSTIC_FIELDS, "{}: {stated}", session.name);
            assert!(
                fields[DIAGNOSTIC_FIELDS - 1].starts_with(COUNT_MARK),
                "{}: {stated} ends with how many times it happened",
                session.name
            );
            assert!(!stated.contains('/'), "{}: {stated} names a path", session.name);
            assert!(
                home.is_empty() || !stated.contains(&home),
                "{}: {stated} names this account's home",
                session.name
            );
        }
    }
}
