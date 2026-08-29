//! Probe for the cancellation-tokens capability.
//!
//! Requires cooperative cancellation that propagates to a child token, resolves
//! a waiting task, and reports its state without polling.

use tokio_util::sync::CancellationToken;

#[test]
fn cancellation_propagates_from_a_parent_to_its_child_token() {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("the runtime builds");
    runtime.block_on(async {
        let parent = CancellationToken::new();
        let child = parent.child_token();
        assert!(!child.is_cancelled());
        let waiting = tokio::spawn({
            let child = child.clone();
            async move {
                child.cancelled().await;
                "observed"
            }
        });
        parent.cancel();
        assert_eq!(waiting.await.expect("the waiting task finishes"), "observed");
        assert!(child.is_cancelled(), "the child observes the parent's cancellation");
    });
}
