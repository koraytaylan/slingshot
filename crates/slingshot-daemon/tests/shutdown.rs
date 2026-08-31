//! Stopping a daemon without abandoning what it was doing.
//!
//! Stopping is not cancelling. A client that asked a daemon to stop did not ask
//! it to fail the operations it was running, and these prove it does not: work
//! in flight finishes, and only then does the daemon go.

use slingshot_daemon::shutdown::{Shutdown, ShutdownPhase, StopRefusal};

/// Characters a sixty-four-character hexadecimal value has.
const DIGEST_CHARACTERS: usize = 64;

/// Returns the nonce that authorizes stopping the fixture daemon.
fn nonce() -> String {
    "a".repeat(DIGEST_CHARACTERS)
}

/// Operations one draining fixture leaves running.
const DRAINING_OPERATIONS: usize = 2;

/// Operations left once one of two has finished.
const ONE_STILL_RUNNING: usize = 1;

/// Returns a nonce a previous instance published.
fn stale_nonce() -> String {
    "b".repeat(DIGEST_CHARACTERS)
}

#[test]
fn an_idle_daemon_stops_at_once() {
    let mut shutdown = Shutdown::running(&nonce());
    assert!(shutdown.takes_new_work());
    assert_eq!(shutdown.begin(&nonce()).expect("a stop"), ShutdownPhase::Withdrawn);
    assert!(!shutdown.takes_new_work(), "and takes nothing further");
    assert!(shutdown.close(), "with nothing running, the listener closes immediately");
    assert_eq!(shutdown.phase(), ShutdownPhase::Closed);
}

#[test]
fn a_busy_daemon_drains_before_it_goes() {
    let mut shutdown = Shutdown::running(&nonce());
    shutdown.started("operation-1");
    shutdown.started("operation-2");

    assert_eq!(
        shutdown.begin(&nonce()).expect("a stop"),
        ShutdownPhase::Draining,
        "work already running is not cancelled by a request to stop"
    );
    assert!(!shutdown.takes_new_work(), "while nothing new arrives");
    assert!(!shutdown.close(), "and the listener stays open while work is running");
    assert_eq!(shutdown.outstanding(), DRAINING_OPERATIONS);

    shutdown.finished("operation-1");
    assert_eq!(shutdown.phase(), ShutdownPhase::Draining, "one is still going");
    assert!(!shutdown.close());

    shutdown.finished("operation-2");
    assert_eq!(shutdown.phase(), ShutdownPhase::Withdrawn, "and the last one finishing ends it");
    assert!(shutdown.close(), "so the listener closes");
}

#[test]
fn a_nonce_from_another_instance_stops_nothing() {
    let mut shutdown = Shutdown::running(&nonce());
    let refused = shutdown.begin(&stale_nonce());
    assert_eq!(
        refused,
        Err(StopRefusal::StaleInstance),
        "a nonce a previous instance published proves only that a previous instance existed"
    );
    assert_eq!(shutdown.phase(), ShutdownPhase::Running, "and this one is still running");
    assert!(shutdown.takes_new_work(), "still taking work");
    assert!(!shutdown.close(), "and not closing");

    assert_eq!(shutdown.begin(&nonce()).expect("a stop"), ShutdownPhase::Withdrawn);
}

#[test]
fn stopping_twice_is_stopping_once() {
    let mut shutdown = Shutdown::running(&nonce());
    shutdown.started("operation-1");
    assert_eq!(shutdown.begin(&nonce()).expect("a stop"), ShutdownPhase::Draining);
    assert_eq!(
        shutdown.begin(&nonce()).expect("a repeat"),
        ShutdownPhase::Draining,
        "a client that asked twice is told the same thing, not moved on twice"
    );
    assert_eq!(shutdown.outstanding(), ONE_STILL_RUNNING, "and the work is still there");
}

#[test]
fn work_finishing_before_a_stop_leaves_nothing_to_drain() {
    let mut shutdown = Shutdown::running(&nonce());
    shutdown.started("operation-1");
    shutdown.finished("operation-1");
    assert_eq!(
        shutdown.phase(),
        ShutdownPhase::Running,
        "work finishing on its own does not begin a stop"
    );
    assert_eq!(shutdown.begin(&nonce()).expect("a stop"), ShutdownPhase::Withdrawn);
}
