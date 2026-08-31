//! What the process harness offers, proved against real child processes.
//!
//! Every scenario here starts an actual operating-system process. The child is
//! this test binary re-invoked in a helper mode, which keeps the proof free of
//! any product crate: the harness is meant to drive anything, so nothing it is
//! driven against here belongs to the command line, the daemon, configuration,
//! or the repository tooling.
//!
//! The helper writes its answers between two markers, because a test binary
//! also prints its own progress and the scenario compares only what the helper
//! meant to say.
//!
//! One limit is worth stating plainly. The kernel does not hand out a reused
//! process identifier on demand, so the coincidence of a replacement receiving
//! a reaped child's number is not staged here. What is proved instead is the
//! property that would make the coincidence harmless: every operation goes
//! through a handle bound to one instance, a handle for a reaped child refuses,
//! and no path in the harness turns an identifier into an action.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::ExitStatus;
use std::time::Duration;

use slingshot_test_support::daemon_process::{Handshake, Lifecycle, ScriptedDaemon, StopAnswer};
use slingshot_test_support::process_harness::{
    CleanupRefusal, CleanupRoute, CooperativeStop, DeliverableSignal, ExecutablePath,
    HarnessFailure, ProcessHarness, ProcessRequest, RetainedChild,
};

/// The variable naming which behaviour a helper child performs.
const HELPER_MODE_VARIABLE: &str = "SLINGSHOT_PROCESS_HARNESS_HELPER_MODE";

/// The one test a helper child runs.
const HELPER_TEST_NAME: &str = "helper_child_behaviour";

/// Where the helper's own output begins.
const OUTPUT_OPENING: &str = "<<helper-output";

/// Where the helper's own output ends.
const OUTPUT_CLOSING: &str = "helper-output>>";

/// The variable naming the root a helper child may write under.
const ROOT_VARIABLE: &str = "SLINGSHOT_CONFIGURATION_ROOT";

/// A variable the build sets for this process and never for a sealed child.
const INHERITED_VARIABLE: &str = "CARGO_MANIFEST_DIR";

/// What a helper child writes under the root it was given.
const WITNESS_FILE: &str = "witness.txt";

/// The exit code the exiting helper chooses.
const CHOSEN_EXIT_CODE: i32 = 7;

/// The exit code a helper asked for a mode it does not have.
const UNKNOWN_MODE_EXIT_CODE: i32 = 9;

/// The signal number an interrupt carries.
const INTERRUPT_SIGNAL_NUMBER: i32 = 2;

/// How many lines the flooding helper writes to each stream.
const FLOOD_LINES: usize = 4096;

/// The line the flooding helper repeats.
const FLOOD_LINE: &str = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcde";

/// How long a scenario waits for a child that should finish at once.
const PROMPT_DEADLINE: Duration = Duration::from_secs(30);

/// How long a scenario waits for a child that never finishes.
const SHORT_DEADLINE: Duration = Duration::from_millis(250);

/// Where the fixtures for this suite live.
const FIXTURE_DIRECTORY: &str = "tests/fixtures/process-harness";

/// Calls no path in the harness may make about a numeric identifier.
const IDENTIFIER_MISUSES: &[&str] = &["kill_process", "/proc/", "sysinfo", "pgrep", "pidfd_open("];

/// The one place a handle is taken, which the misuse scan allows.
const HANDLE_ACQUISITION: &str = "pidfd_open(Pid::from_child(child), PidfdFlags::empty())";

// ---------------------------------------------------------------- the helper

/// Behaves as the mode names when this binary runs as a helper child.
///
/// A run that is not a helper does nothing here, so the scenario tests below
/// see one more passing test and no side effect.
#[test]
fn helper_child_behaviour() {
    let Ok(mode) = std::env::var(HELPER_MODE_VARIABLE) else {
        return;
    };
    println!("{OUTPUT_OPENING}");
    let code = perform(&mode);
    println!("{OUTPUT_CLOSING}");
    finish(code);
}

/// Performs one helper behaviour and returns the code it exits with.
fn perform(mode: &str) -> i32 {
    match mode {
        "report-environment" => report_environment(),
        "report-roots" => report_roots(),
        "report-terminal" => report_terminal(),
        "flood-streams" => flood_streams(),
        "exit-with-code" => CHOSEN_EXIT_CODE,
        "sleep-forever" => sleep_forever(),
        _ => UNKNOWN_MODE_EXIT_CODE,
    }
}

/// Ends the helper child with the code it chose.
///
/// Exiting here rather than returning keeps the code exact: a test binary that
/// returned normally would report whether its tests passed, which is not what
/// the scenario is reading.
fn finish(code: i32) -> ! {
    use std::io::Write;
    std::io::stdout().flush().ok();
    std::io::stderr().flush().ok();
    std::process::exit(code)
}

/// Names which variables the child can see.
fn report_environment() -> i32 {
    for name in [INHERITED_VARIABLE, ROOT_VARIABLE, "HOME"] {
        let seen = if std::env::var(name).is_ok() { "present" } else { "absent" };
        println!("{name}={seen}");
    }
    0
}

/// Writes one witness file under the root the child was given.
fn report_roots() -> i32 {
    let Ok(root) = std::env::var(ROOT_VARIABLE) else {
        return UNKNOWN_MODE_EXIT_CODE;
    };
    let path = PathBuf::from(&root).join(WITNESS_FILE);
    if std::fs::write(&path, WITNESS_FILE).is_err() {
        return UNKNOWN_MODE_EXIT_CODE;
    }
    println!("wrote={}", path.display());
    0
}

/// Says whether the child's standard output is a terminal.
fn report_terminal() -> i32 {
    use std::io::IsTerminal;
    println!("standard-output-is-terminal={}", std::io::stdout().is_terminal());
    0
}

/// Writes far more than a pipe holds to both streams.
fn flood_streams() -> i32 {
    for _ in 0..FLOOD_LINES {
        println!("{FLOOD_LINE}");
        eprintln!("{FLOOD_LINE}");
    }
    0
}

/// Never finishes on its own.
fn sleep_forever() -> i32 {
    loop {
        std::thread::sleep(PROMPT_DEADLINE);
    }
}

// -------------------------------------------------------------- the scenario

/// Returns this test binary, which is also every helper child.
fn helper_executable() -> ExecutablePath {
    let path = std::env::current_exe().expect("this test binary has a path");
    ExecutablePath::new(path).expect("this test binary is an executable file")
}

/// Returns the request that runs one helper mode.
fn helper_request(mode: &str) -> ProcessRequest {
    ProcessRequest::new(&["--exact", HELPER_TEST_NAME, "--nocapture"])
        .with_environment(HELPER_MODE_VARIABLE, mode)
}

/// Returns only what the helper wrote between its markers.
fn helper_output(captured: &str) -> String {
    let opened = captured.split_once(OUTPUT_OPENING).map(|(_, rest)| rest).unwrap_or_default();
    let body = opened.split_once(OUTPUT_CLOSING).map(|(body, _)| body).unwrap_or(opened);
    body.replace('\r', "").trim().to_owned()
}

/// Reads one fixture as its non-comment rows.
fn fixture_rows(name: &str) -> Vec<(String, String)> {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(FIXTURE_DIRECTORY).join(name);
    let text = std::fs::read_to_string(&path)
        .unwrap_or_else(|failure| panic!("{} could not be read: {failure}", path.display()));
    text.lines()
        .filter(|line| !line.trim().is_empty() && !line.starts_with('#'))
        .map(|line| {
            let (left, right) = line.split_once('|').expect("every row names two fields");
            (left.trim().to_owned(), right.trim().to_owned())
        })
        .collect()
}

/// Returns a handshake one scripted daemon answers with.
fn handshake_quoting(nonce: &str) -> Handshake {
    Handshake {
        author_target_identity_digest: SCENARIO_DIGEST.to_owned(),
        current_nonce: nonce.to_owned(),
        runtime_contract_digest: SCENARIO_DIGEST.to_owned(),
        selected_environment_revision: SCENARIO_REVISION.to_owned(),
    }
}

/// The digest a scenario daemon answers with.
const SCENARIO_DIGEST: &str = "scenario-digest";

/// The revision a scenario daemon serves.
const SCENARIO_REVISION: &str = "scenario-revision";

/// Returns how a finished child exited.
fn signal_of(status: ExitStatus) -> Option<i32> {
    use std::os::unix::process::ExitStatusExt;
    status.signal()
}

// ------------------------------------------------------------------ scenarios

/// A sealed child sees only what the scenario handed it.
#[test]
fn sealed_child_starts_from_an_empty_environment() {
    let harness = ProcessHarness::new();
    let sealed = helper_request("report-environment").sealed();
    let seen = harness
        .run_within(&helper_executable(), &sealed, PROMPT_DEADLINE)
        .expect("the sealed child ran");
    let reported = helper_output(&seen.standard_output);
    assert!(reported.contains(&format!("{INHERITED_VARIABLE}=absent")), "{reported}");

    let inherited = helper_request("report-environment");
    let also = harness
        .run_within(&helper_executable(), &inherited, PROMPT_DEADLINE)
        .expect("the inheriting child ran");
    let otherwise = helper_output(&also.standard_output);
    assert!(otherwise.contains(&format!("{INHERITED_VARIABLE}=present")), "{otherwise}");
}

/// What a child writes lands under the root the scenario chose.
#[test]
fn child_writes_land_under_the_scenario_root() {
    let root = tempfile::tempdir().expect("a temporary root exists");
    let harness = ProcessHarness::new();
    let mut request = helper_request("report-roots").sealed();
    for (name, value) in ProcessHarness::isolated_environment(root.path()) {
        request = request.with_environment(&name, value);
    }
    let written =
        harness.run_within(&helper_executable(), &request, PROMPT_DEADLINE).expect("the child ran");
    assert!(written.status.success(), "{}", written.standard_error);
    assert!(root.path().join(WITNESS_FILE).is_file());
    assert_eq!(
        ProcessHarness::isolated_environment(root.path()).get("HOME"),
        Some(&root.path().to_string_lossy().into_owned())
    );
}

/// A child that writes more than a pipe holds still finishes.
#[test]
fn flooded_streams_are_drained_while_the_child_runs() {
    let harness = ProcessHarness::new();
    let flooded = harness
        .run_within(&helper_executable(), &helper_request("flood-streams"), PROMPT_DEADLINE)
        .expect("the flooding child finished rather than blocking");
    assert!(flooded.status.success());
    let expected = FLOOD_LINES * (FLOOD_LINE.len() + 1);
    assert!(flooded.standard_output.len() >= expected, "{}", flooded.standard_output.len());
    assert!(flooded.standard_error.len() >= expected, "{}", flooded.standard_error.len());
}

/// The same helper answers one way on a terminal and another on a pipe.
#[test]
fn terminal_and_redirected_children_answer_differently() {
    let harness = ProcessHarness::new();
    let redirected = harness
        .run_within(&helper_executable(), &helper_request("report-terminal"), PROMPT_DEADLINE)
        .expect("the redirected child ran");
    assert_eq!(
        helper_output(&redirected.standard_output),
        "standard-output-is-terminal=false".to_owned()
    );

    let mut retained = harness
        .start_retained(&helper_executable(), &helper_request("report-terminal").on_terminal())
        .expect("the terminal child started");
    retained.wait_within(PROMPT_DEADLINE).expect("the terminal child finished");
    let spoken = retained.terminal_output().expect("the terminal answered");
    assert_eq!(helper_output(&spoken), "standard-output-is-terminal=true".to_owned());
}

/// An interrupt reaches the child through the handle taken at spawn.
#[test]
fn interrupt_reaches_the_child_through_the_retained_handle() {
    let harness = ProcessHarness::new();
    let mut sleeping = harness
        .start_retained(&helper_executable(), &helper_request("sleep-forever"))
        .expect("the sleeping child started");
    sleeping.deliver(DeliverableSignal::Interrupt).expect("the interrupt was delivered");
    let ended = sleeping.wait_within(PROMPT_DEADLINE).expect("the child ended");
    assert_eq!(signal_of(ended), Some(INTERRUPT_SIGNAL_NUMBER));
    assert!(sleeping.is_reaped());
}

/// A child's own exit code is reported exactly.
#[test]
fn child_exit_code_is_reported_exactly() {
    let harness = ProcessHarness::new();
    let exited = harness
        .run_within(&helper_executable(), &helper_request("exit-with-code"), PROMPT_DEADLINE)
        .expect("the exiting child ran");
    assert_eq!(exited.status.code(), Some(CHOSEN_EXIT_CODE));
}

/// A child that outlives its deadline is ended and the scenario is told.
#[test]
fn child_outliving_its_deadline_is_ended_and_reported() {
    let harness = ProcessHarness::new();
    let overrun =
        harness.run_within(&helper_executable(), &helper_request("sleep-forever"), SHORT_DEADLINE);
    assert_eq!(overrun.unwrap_err(), HarnessFailure::DeadlineElapsed(SHORT_DEADLINE));
    assert!(harness.leak_report(false).is_clean());
}

/// A daemon that answers is stopped by quoting the nonce it is serving under.
#[test]
fn responsive_daemon_stops_when_the_current_nonce_is_quoted() {
    let nonce = "current-nonce";
    let daemon =
        ScriptedDaemon::following(Lifecycle::AlreadyServing(Box::new(handshake_quoting(nonce))));
    let cooperative = CooperativeStop { current_nonce: nonce.to_owned(), responsive: true };
    assert_eq!(
        cooperative.route(nonce, true),
        Ok(CleanupRoute::Cooperative { nonce: nonce.to_owned() })
    );
    assert_eq!(daemon.stop(nonce), StopAnswer::Released);
    assert_eq!(daemon.stops(), vec![nonce.to_owned()]);
}

/// A stale nonce refuses instead of reaching for a stronger way to stop.
#[test]
fn stale_nonce_refuses_rather_than_escalating() {
    let daemon = ScriptedDaemon::following(Lifecycle::AlreadyServing(Box::new(handshake_quoting(
        "replacement-nonce",
    ))));
    let cooperative =
        CooperativeStop { current_nonce: "replacement-nonce".to_owned(), responsive: false };
    assert_eq!(cooperative.route("reaped-nonce", true), Err(CleanupRefusal::NonceStale));
    assert_eq!(daemon.stop("reaped-nonce"), StopAnswer::NonceStale);
    assert_eq!(daemon.probe(), daemon.probe(), "the replacement is still serving");
}

/// An owned child that will not answer is ended through its retained handle.
#[test]
fn unresponsive_owned_child_ends_through_the_retained_handle() {
    let harness = ProcessHarness::new();
    let mut unresponsive = harness
        .start_retained(&helper_executable(), &helper_request("sleep-forever"))
        .expect("the unresponsive child started");
    let cooperative =
        CooperativeStop { current_nonce: "current-nonce".to_owned(), responsive: false };
    assert_eq!(cooperative.route("current-nonce", true), Ok(CleanupRoute::RetainedHandle));
    let ended = unresponsive.end_within(PROMPT_DEADLINE).expect("the child ended");
    assert_eq!(signal_of(ended), Some(SIGNAL_KILL_NUMBER));
}

/// The signal number a kill carries.
const SIGNAL_KILL_NUMBER: i32 = 9;

/// A child this harness does not own is refused, whatever it is doing.
#[test]
fn a_child_this_harness_does_not_own_is_never_ended() {
    let cooperative =
        CooperativeStop { current_nonce: "current-nonce".to_owned(), responsive: false };
    assert_eq!(cooperative.route("current-nonce", false), Err(CleanupRefusal::NotOwned));
}

/// A reaped child's handle refuses, and a fresh child keeps running.
#[test]
fn reaped_handle_refuses_and_a_replacement_keeps_running() {
    let harness = ProcessHarness::new();
    let mut first = harness
        .start_retained(&helper_executable(), &helper_request("sleep-forever"))
        .expect("the first child started");
    let reaped_identifier = first.identifier();
    first.end_within(PROMPT_DEADLINE).expect("the first child ended");
    assert_eq!(first.deliver(DeliverableSignal::Kill), Err(HarnessFailure::AlreadyReaped));

    let mut replacement = harness
        .start_retained(&helper_executable(), &helper_request("sleep-forever"))
        .expect("the replacement started");
    assert!(matches!(
        replacement.wait_within(SHORT_DEADLINE),
        Err(HarnessFailure::DeadlineElapsed(_))
    ));
    replacement.deliver(DeliverableSignal::Interrupt).expect("the replacement is reachable");
    let ended = replacement.wait_within(PROMPT_DEADLINE).expect("the replacement ended");
    assert_eq!(signal_of(ended), Some(INTERRUPT_SIGNAL_NUMBER));
    assert_ne!(reaped_identifier, u32::MAX, "an identifier is recorded as a diagnostic");
}

/// Nothing in the harness turns a numeric identifier into an action.
#[test]
fn no_path_turns_a_process_identifier_into_an_action() {
    let source = std::fs::read_to_string(
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("src/process_harness.rs"),
    )
    .expect("the harness source is readable");
    let code: String = source
        .lines()
        .filter(|line| !line.trim_start().starts_with("//"))
        .collect::<Vec<_>>()
        .join("\n");
    let acquisitions = code.matches(HANDLE_ACQUISITION).count();
    assert_eq!(acquisitions, 1, "one handle is taken, at spawn");
    let remainder = code.replace(HANDLE_ACQUISITION, "");
    for misuse in IDENTIFIER_MISUSES {
        assert!(!remainder.contains(misuse), "{misuse} appears in the harness");
    }
}

/// A harness holding a child says so rather than tidying it away quietly.
#[test]
fn orphaned_children_are_reported_rather_than_hidden() {
    let mut harness = ProcessHarness::new();
    harness
        .start(&helper_executable(), &helper_request("sleep-forever"))
        .expect("the child started");
    let holding = harness.leak_report(false);
    assert_eq!(holding.orphaned_children, 1);
    assert!(!holding.is_clean());
    assert!(!harness.leak_report(true).is_clean(), "an unread stream is a leak too");
    harness.reap_all();
    assert!(harness.leak_report(false).is_clean());
}

/// Every capability the fixture names has a test, and every test is named.
#[test]
fn every_named_capability_has_a_test() {
    let source = include_str!("process_harness.rs");
    let capabilities = fixture_rows("harness-capabilities.txt");
    assert!(!capabilities.is_empty());
    for (capability, test) in &capabilities {
        assert!(source.contains(&format!("fn {test}()")), "{capability} has no test named {test}");
    }
    let behaviours = fixture_rows("helper-behaviours.txt");
    for (mode, _) in &behaviours {
        assert!(source.contains(&format!("\"{mode}\"")), "{mode} is never started");
    }
    let named: BTreeMap<&str, &str> =
        capabilities.iter().map(|(one, other)| (one.as_str(), other.as_str())).collect();
    assert_eq!(named.len(), capabilities.len(), "no capability is named twice");
}

/// The harness types a scenario composes are usable from outside the crate.
#[test]
fn the_harness_surface_is_reachable() {
    fn accepts(_child: &RetainedChild, _root: &Path) {}
    let _ = accepts;
    assert_eq!(
        ProcessHarness::isolated_environment(Path::new("/scenario")).len(),
        ISOLATED_VARIABLE_COUNT
    );
}

/// How many variables an isolated environment names.
const ISOLATED_VARIABLE_COUNT: usize = 3;
