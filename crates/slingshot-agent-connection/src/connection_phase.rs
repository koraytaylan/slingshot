//! Exclusive connection-phase deadlines, including ready-at-boundary results.
use std::future::Future;
use tokio::time::{Duration, Instant, timeout_at};

pub(crate) async fn within<T, E: Copy>(
    duration: Duration,
    operation: impl Future<Output = Result<T, E>>,
    expired: E,
) -> Result<T, E> {
    let end = Instant::now() + duration;
    let result = timeout_at(end, operation).await.map_err(|_| expired)?;
    // Tokio may poll a ready operation before its timer. A late socket/result
    // must be dropped, not promoted into the next phase with a fresh budget.
    if Instant::now() >= end {
        return Err(expired);
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};
    struct Dropped<'a>(&'a AtomicUsize);
    impl Drop for Dropped<'_> {
        fn drop(&mut self) {
            self.0.fetch_add(1, Ordering::SeqCst);
        }
    }

    #[tokio::test(start_paused = true)]
    async fn ready_results_at_or_beyond_the_boundary_never_escape_the_phase() {
        for elapsed in [9, 10, 11] {
            let drops = AtomicUsize::new(0);
            let result = within(
                Duration::from_millis(10),
                async {
                    let owned = Dropped(&drops);
                    tokio::time::advance(Duration::from_millis(elapsed)).await;
                    Ok(owned)
                },
                "deadline",
            )
            .await;
            assert_eq!(result.is_ok(), elapsed < 10);
            drop(result);
            assert_eq!(drops.load(Ordering::SeqCst), 1);
        }
        assert_eq!(
            within(
                Duration::from_millis(10),
                async {
                    tokio::time::advance(Duration::from_millis(10)).await;
                    Err::<(), _>("phase failure")
                },
                "deadline"
            )
            .await,
            Err("deadline")
        );
    }

    #[tokio::test(start_paused = true)]
    async fn independent_budgets_preserve_early_failure_and_cancel_owned_work() {
        assert_eq!(
            within(Duration::from_millis(10), async { Err::<(), _>("connect") }, "deadline").await,
            Err("connect")
        );
        let drops = AtomicUsize::new(0);
        assert!(
            within(
                Duration::from_millis(10),
                async {
                    let _owned = Dropped(&drops);
                    std::future::pending::<Result<(), &str>>().await
                },
                "connect deadline"
            )
            .await
            .is_err()
        );
        assert_eq!(drops.load(Ordering::SeqCst), 1);
        // The next phase receives its own budget, not a reused absolute end.
        assert_eq!(
            within(
                Duration::from_millis(20),
                async {
                    tokio::time::sleep(Duration::from_millis(19)).await;
                    Ok(())
                },
                "TLS deadline"
            )
            .await,
            Ok(())
        );
    }
}
