//! Stopping a daemon where it actually writes, and looking at what survived.
//!
//! A daemon that dies has not misbehaved: power fails, a container is evicted,
//! somebody sends a signal. So every place where durable state changes carries
//! a name and an invariant, and this walks all of them - because a phase nobody
//! named is a phase nobody checked.
//!
//! What each run reports is read back off disk rather than remembered, since
//! the whole question is what a successor would find.

use std::path::PathBuf;

use slingshot_development::daemon_chaos_subject::DaemonChaosSubject;
use slingshot_test_support::daemon_fault_checkpoints::{
    DaemonCheckpoint, DaemonFaultPlan, EVERY_DAEMON_CHECKPOINT,
};

/// Returns a temporary directory this case owns.
fn temporary_root(named: &str) -> PathBuf {
    let root = std::env::temp_dir().join(format!("chaos-{named}-{}", std::process::id()));
    std::fs::remove_dir_all(&root).ok();
    std::fs::create_dir_all(&root).expect("the temporary root is created");
    root
}

#[test]
fn every_checkpoint_says_what_must_be_true_if_the_daemon_disappears_there() {
    let mut invariants = std::collections::BTreeSet::new();
    for checkpoint in EVERY_DAEMON_CHECKPOINT {
        let held = checkpoint.invariant();
        assert!(!held.is_empty(), "{checkpoint:?} states no invariant");
        assert!(invariants.insert(held), "{checkpoint:?} restates another checkpoint's invariant");
    }
    assert_eq!(invariants.len(), EVERY_DAEMON_CHECKPOINT.len());
}

#[test]
fn only_the_checkpoints_before_anything_was_sent_permit_running_it_again() {
    for checkpoint in EVERY_DAEMON_CHECKPOINT {
        let expected = !matches!(
            checkpoint,
            DaemonCheckpoint::BeforeRemoteSubmission
                | DaemonCheckpoint::BeforeResultSettlement
                | DaemonCheckpoint::BeforeEndpointRelease
        );
        assert_eq!(
            checkpoint.permits_unconditional_retry(),
            expected,
            "{checkpoint:?} disagrees about whether a restart may just run it again"
        );
    }
}

#[test]
fn an_uninterrupted_run_establishes_everything_a_daemon_needs() {
    let root = temporary_root("uninterrupted");
    let subject = DaemonChaosSubject::under(&root).expect("the roots are made");
    let held = subject.run(DaemonFaultPlan::uninterrupted()).expect("the daemon establishes");
    assert_eq!(held.stopped_at, None);
    assert!(held.state_root_exists, "an established daemon has a state root");
    assert!(held.database_exists, "and a database under it");
    assert!(held.namespace_owned, "and it owns its namespace");
    std::fs::remove_dir_all(&root).ok();
}

#[test]
fn stopping_before_the_database_opens_leaves_nothing_durable_behind() {
    let root = temporary_root("before-database");
    let subject = DaemonChaosSubject::under(&root).expect("the roots are made");
    let held = subject
        .run(DaemonFaultPlan::stopping_at(DaemonCheckpoint::BeforeDatabaseOpen))
        .expect("stopping is not failing");
    assert_eq!(held.stopped_at, Some(DaemonCheckpoint::BeforeDatabaseOpen));
    assert!(!held.database_exists, "nothing durable exists, so a restart begins cleanly");
    assert!(!held.namespace_owned);
    std::fs::remove_dir_all(&root).ok();
}

#[test]
fn a_run_stopped_before_ownership_leaves_the_namespace_for_a_successor() {
    let root = temporary_root("before-ownership");
    let subject = DaemonChaosSubject::under(&root).expect("the roots are made");
    let held = subject
        .run(DaemonFaultPlan::stopping_at(DaemonCheckpoint::BeforeOwnership))
        .expect("stopping is not failing");
    assert!(!held.namespace_owned, "no daemon owns the namespace, so a successor may");
    assert!(held.database_exists, "and what it did write is there to be picked up");
    std::fs::remove_dir_all(&root).ok();
}

#[test]
fn a_successor_establishes_over_whatever_the_last_run_left() {
    let root = temporary_root("successor");
    let subject = DaemonChaosSubject::under(&root).expect("the roots are made");
    for checkpoint in EVERY_DAEMON_CHECKPOINT {
        subject
            .run(DaemonFaultPlan::stopping_at(*checkpoint))
            .unwrap_or_else(|failure| panic!("{checkpoint:?} refused to run: {failure}"));
    }
    let recovered = subject.run(DaemonFaultPlan::uninterrupted()).expect("a successor establishes");
    assert!(recovered.database_exists, "over the state every earlier run left");
    assert!(recovered.namespace_owned);
    std::fs::remove_dir_all(&root).ok();
}
