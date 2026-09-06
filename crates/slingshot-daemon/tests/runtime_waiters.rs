//! Local observers own no durable state and release capacity on every exit.

use slingshot_daemon::operation_wait::runtime::{AttachRefusal, RuntimeWaiters};
use slingshot_daemon::operation_wait::{WaitBounds, WaitUpdate};
use tokio_util::sync::CancellationToken;

fn progress(revision: u64) -> WaitUpdate {
    WaitUpdate::Progress { detail: "working".to_owned(), revision }
}

#[tokio::test]
async fn publication_wakes_pending_readers_and_cancellation_only_detaches_its_reader() {
    let stopping = CancellationToken::new();
    let waiters = RuntimeWaiters::new(stopping.clone());
    let cancelled = CancellationToken::new();
    let mut first = waiters.attach("operation", 1, progress(1)).unwrap();
    let mut second = waiters.attach("operation", 1, progress(1)).unwrap();
    let first_cancel = cancelled.clone();
    let pending = tokio::spawn(async move { first.next(&first_cancel).await });
    tokio::task::yield_now().await;
    cancelled.cancel();
    assert!(pending.await.unwrap().is_none());
    assert_eq!(waiters.attached(), 1);
    let pending = tokio::spawn(async move { second.next(&CancellationToken::new()).await });
    tokio::task::yield_now().await;
    waiters.publish("operation", &WaitUpdate::Terminal { revision: 2 });
    assert_eq!(
        tokio::time::timeout(std::time::Duration::from_secs(1), pending).await.unwrap().unwrap(),
        Some(WaitUpdate::Terminal { revision: 2 })
    );
    assert_eq!(waiters.attached(), 0);
    let mut stopping_reader = waiters.attach("operation", 2, progress(2)).unwrap();
    stopping.cancel();
    assert!(stopping_reader.next(&CancellationToken::new()).await.is_none());
    assert_eq!(waiters.attached(), 0);
    assert!(matches!(waiters.attach("operation", 2, progress(2)), Err(AttachRefusal::Stopping)));
}

#[tokio::test]
async fn catch_up_is_immediate_and_terminal_reads_release_capacity_without_another_poll() {
    let waiters = RuntimeWaiters::new(CancellationToken::new());
    let mut reader = waiters.attach("operation", 1, progress(3)).unwrap();
    waiters.publish("operation", &progress(2));
    assert_eq!(reader.next(&CancellationToken::new()).await, Some(progress(3)));
    waiters.publish("other-operation", &WaitUpdate::Terminal { revision: 4 });
    waiters.publish("operation", &WaitUpdate::Terminal { revision: 4 });
    assert_eq!(
        reader.next(&CancellationToken::new()).await,
        Some(WaitUpdate::Terminal { revision: 4 })
    );
    assert_eq!(waiters.attached(), 0);
    assert!(reader.next(&CancellationToken::new()).await.is_none());
    assert!(matches!(
        waiters.attach("operation", 5, progress(4)),
        Err(AttachRefusal::FutureRevision)
    ));
    let mut observed_terminal =
        waiters.attach("operation", 4, WaitUpdate::Terminal { revision: 4 }).unwrap();
    assert!(observed_terminal.next(&CancellationToken::new()).await.is_none());
    assert_eq!(waiters.attached(), 0);
}

#[test]
fn capacity_and_drop_are_bounded_across_operations_and_reusable() {
    let waiters = RuntimeWaiters::new(CancellationToken::new());
    let per_operation = WaitBounds::embedded().waiters_per_operation as usize;
    let global = slingshot_local_protocol::foundation_contract::FoundationContract::embedded()
        .server
        .connection_capacity as usize;
    let mut held = Vec::new();
    for _ in 0..per_operation.min(global) {
        held.push(waiters.attach("operation", 1, progress(1)).unwrap());
    }
    assert!(waiters.attach("operation", 1, progress(1)).is_err());
    for index in held.len()..global {
        held.push(waiters.attach(&format!("operation-{index}"), 1, progress(1)).unwrap());
    }
    assert!(matches!(waiters.attach("another", 1, progress(1)), Err(AttachRefusal::Capacity)));
    held.pop();
    let replacement = waiters.attach("replacement", 1, progress(1)).unwrap();
    assert_eq!(waiters.attached(), global);
    drop(replacement);
    drop(held);
    assert_eq!(waiters.attached(), 0);
    // No registry remains to suppress a new incarnation's first observation.
    let fresh = waiters.attach("operation", 0, progress(1)).unwrap();
    assert_eq!(waiters.attached(), 1);
    drop(fresh);
    assert_eq!(waiters.attached(), 0);
}
