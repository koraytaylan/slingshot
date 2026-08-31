//! Watching an operation without being able to affect it.
//!
//! Every test here is ultimately the same assertion from a different angle: a
//! waiter cannot change the work. Slow, stopped, disconnected, cancelled, or at
//! the maximum count, the operation is exactly what it would have been with
//! nobody watching - and a clock advancing on its own turns nothing pending
//! into anything failed.

use slingshot_daemon::operation_wait::{
    Detachment, WaitBounds, WaitRefusal, WaitUpdate, WaiterRegistry,
};

/// Updates one waiter's queue may hold, from the runtime contract.
const QUEUE_UPDATES: u64 = 32;

/// Waiters one operation may have, from the runtime contract.
const WAITERS_PER_OPERATION: u64 = 64;

/// The revision every fixture operation starts at.
const FIRST_REVISION: u64 = 1;

/// The revision one step after [`FIRST_REVISION`].
const SECOND_REVISION: u64 = FIRST_REVISION + 1;

/// The revision one step after [`SECOND_REVISION`].
const THIRD_REVISION: u64 = SECOND_REVISION + 1;

/// The revision one step after [`THIRD_REVISION`].
const FOURTH_REVISION: u64 = THIRD_REVISION + 1;

/// The revision one step after [`FOURTH_REVISION`].
const FIFTH_REVISION: u64 = FOURTH_REVISION + 1;

/// The revision one step after [`FIFTH_REVISION`].
const SIXTH_REVISION: u64 = FIFTH_REVISION + 1;

/// The revision one step after [`SIXTH_REVISION`].
const SEVENTH_REVISION: u64 = SIXTH_REVISION + 1;

/// The revision one step after [`SEVENTH_REVISION`].
const EIGHTH_REVISION: u64 = SEVENTH_REVISION + 1;

/// The revision one step after [`EIGHTH_REVISION`].
const NINTH_REVISION: u64 = EIGHTH_REVISION + 1;

/// The revision one step after [`NINTH_REVISION`].
const TENTH_REVISION: u64 = NINTH_REVISION + 1;

/// Updates one waiter's queue may hold under the small fixture bounds.
const SMALL_QUEUE_UPDATES: u64 = 2;

/// Waiters one operation may have under the small fixture bounds.
const SMALL_WAITERS: u64 = 3;

/// Returns bounds small enough for a test to fill a queue.
fn small_bounds() -> WaitBounds {
    WaitBounds { queue_updates: SMALL_QUEUE_UPDATES, waiters_per_operation: SMALL_WAITERS }
}

/// Returns one progress update at `revision`.
fn progress(revision: u64) -> WaitUpdate {
    WaitUpdate::Progress { detail: format!("step {revision}"), revision }
}

#[test]
fn the_bounds_are_the_manifest_s_own() {
    let bounds = WaitBounds::embedded();
    assert_eq!(bounds.queue_updates, QUEUE_UPDATES);
    assert_eq!(bounds.waiters_per_operation, WAITERS_PER_OPERATION);
}

#[test]
fn every_waiter_sees_strictly_increasing_revisions_and_the_same_ending() {
    let mut registry = WaiterRegistry::new(WaitBounds::embedded(), FIRST_REVISION);
    let tickets: Vec<u64> = (0..SMALL_WAITERS)
        .map(|_| registry.attach(FIRST_REVISION, None).expect("room for a waiter"))
        .collect();

    for revision in SECOND_REVISION..=FOURTH_REVISION {
        registry.publish(&progress(revision));
    }
    registry.publish(&WaitUpdate::Terminal { revision: FIFTH_REVISION });

    for ticket in tickets {
        let mut seen = Vec::new();
        while let Some(update) = registry.take(ticket) {
            seen.push(update.revision());
        }
        assert_eq!(
            seen,
            vec![SECOND_REVISION, THIRD_REVISION, FOURTH_REVISION, FIFTH_REVISION],
            "each waiter saw the same increasing sequence"
        );
        assert_eq!(
            registry.waiter(ticket).expect("a waiter").delivered_revision(),
            FIFTH_REVISION,
            "and each ended at the same revision"
        );
    }
}

#[test]
fn a_revision_that_is_not_newer_reaches_nobody() {
    let mut registry = WaiterRegistry::new(WaitBounds::embedded(), FIRST_REVISION);
    let ticket = registry.attach(FIRST_REVISION, None).expect("room for a waiter");
    registry.publish(&progress(THIRD_REVISION));
    registry.publish(&progress(SECOND_REVISION));
    registry.publish(&progress(THIRD_REVISION));

    let mut seen = Vec::new();
    while let Some(update) = registry.take(ticket) {
        seen.push(update.revision());
    }
    assert_eq!(
        seen,
        vec![THIRD_REVISION],
        "what a waiter sees is strictly increasing however the caller stitched its reads together"
    );
}

#[test]
fn a_waiter_that_is_already_behind_is_told_at_once_rather_than_left_hanging() {
    let mut registry = WaiterRegistry::new(WaitBounds::embedded(), SEVENTH_REVISION);
    let late = registry
        .attach(FIRST_REVISION, Some(WaitUpdate::Terminal { revision: SEVENTH_REVISION }))
        .expect("room for a waiter");
    assert_eq!(
        registry.take(late).map(|update| update.revision()),
        Some(SEVENTH_REVISION),
        "a client that read a status, decided to wait, and missed the ending still hears it"
    );

    let current = registry
        .attach(SEVENTH_REVISION, Some(WaitUpdate::Terminal { revision: SEVENTH_REVISION }))
        .expect("room for a waiter");
    assert_eq!(
        registry.take(current),
        None,
        "while a waiter that has already seen the newest revision is told nothing twice"
    );
}

#[test]
fn a_full_queue_loses_detail_and_never_loses_the_answer() {
    let mut registry = WaiterRegistry::new(small_bounds(), FIRST_REVISION);
    let ticket = registry.attach(FIRST_REVISION, None).expect("room for a waiter");
    for revision in SECOND_REVISION..=SIXTH_REVISION {
        registry.publish(&progress(revision));
    }
    assert_eq!(
        registry.waiter(ticket).expect("a waiter").queued(),
        2,
        "the queue never grew past its bound"
    );

    registry.publish(&WaitUpdate::RecoveryRequired { revision: SEVENTH_REVISION });
    registry.publish(&WaitUpdate::Terminal { revision: EIGHTH_REVISION });
    let mut seen = Vec::new();
    while let Some(update) = registry.take(ticket) {
        seen.push(update);
    }
    assert!(
        seen.iter().any(WaitUpdate::ends_the_wait),
        "and the ending survived the pressure, because it is what the client is waiting for: \
         {seen:?}"
    );
    assert!(
        seen.iter().any(|update| matches!(update, WaitUpdate::RecoveryRequired { .. })),
        "as did the recovery, for the same reason"
    );
}

#[test]
fn a_waiter_that_stopped_reading_does_not_grow_another_waiter_s_queue() {
    let mut registry = WaiterRegistry::new(small_bounds(), FIRST_REVISION);
    let blocked = registry.attach(FIRST_REVISION, None).expect("room for a waiter");
    let reading = registry.attach(FIRST_REVISION, None).expect("room for another");

    for revision in SECOND_REVISION..=TENTH_REVISION {
        registry.publish(&progress(revision));
        registry.take(reading);
    }
    assert!(
        registry.waiter(blocked).expect("a waiter").queued()
            <= usize::try_from(SMALL_QUEUE_UPDATES).expect("a countable bound"),
        "the blocked waiter's queue stayed inside its own bound"
    );
    assert!(
        registry.waiter(reading).expect("a waiter").queued()
            <= usize::try_from(SMALL_QUEUE_UPDATES).expect("a countable bound"),
        "and so did the reading waiter's, independently of it"
    );
}

#[test]
fn the_waiter_past_the_bound_is_refused_and_changes_nothing() {
    let mut registry = WaiterRegistry::new(small_bounds(), FIRST_REVISION);
    for _ in 0..SMALL_WAITERS {
        registry.attach(FIRST_REVISION, None).expect("room below the bound");
    }
    let published = registry.published_revision();

    let refused = registry.attach(FIRST_REVISION, None);
    assert!(
        matches!(refused, Err(WaitRefusal::WaitersExhausted { held, limit })
            if held == SMALL_WAITERS && limit == SMALL_WAITERS),
        "one waiter past the bound is refused: {refused:?}"
    );
    assert_eq!(
        registry.attached(),
        usize::try_from(SMALL_WAITERS).expect("a countable bound"),
        "and nothing was attached"
    );
    assert_eq!(
        registry.published_revision(),
        published,
        "and a client that could not watch has not affected the work"
    );
}

#[test]
fn detaching_for_any_reason_leaves_the_operation_and_the_others_alone() {
    for reason in [
        Detachment::Cancelled,
        Detachment::Disconnected,
        Detachment::DaemonStopping,
        Detachment::WriteDeadlineElapsed,
    ] {
        let mut registry = WaiterRegistry::new(WaitBounds::embedded(), FIRST_REVISION);
        let leaving = registry.attach(FIRST_REVISION, None).expect("room for a waiter");
        let staying = registry.attach(FIRST_REVISION, None).expect("room for another");
        registry.publish(&progress(SECOND_REVISION));

        assert!(registry.detach(leaving, reason), "{reason:?} detaches the waiter it names");
        assert!(!registry.detach(leaving, reason), "and detaching it again finds nothing");
        assert_eq!(registry.attached(), 1, "only the one that left is gone");

        registry.publish(&WaitUpdate::Terminal { revision: THIRD_REVISION });
        let mut seen = Vec::new();
        while let Some(update) = registry.take(staying) {
            seen.push(update.revision());
        }
        assert_eq!(
            seen,
            vec![SECOND_REVISION, THIRD_REVISION],
            "{reason:?}: the waiter that stayed heard everything, including the ending"
        );
    }
}

#[test]
fn a_registry_with_no_waiters_still_advances_and_nothing_waits_on_a_clock() {
    let mut registry = WaiterRegistry::new(WaitBounds::embedded(), FIRST_REVISION);
    registry.publish(&progress(SECOND_REVISION));
    registry.publish(&WaitUpdate::Terminal { revision: THIRD_REVISION });
    assert_eq!(
        registry.published_revision(),
        THIRD_REVISION,
        "work proceeds with nobody watching, which is the whole point"
    );

    let late = registry
        .attach(FIRST_REVISION, Some(WaitUpdate::Terminal { revision: THIRD_REVISION }))
        .expect("room for a waiter");
    assert_eq!(
        registry.take(late).map(|update| update.revision()),
        Some(THIRD_REVISION),
        "and a waiter arriving after the ending is told the ending"
    );
}
