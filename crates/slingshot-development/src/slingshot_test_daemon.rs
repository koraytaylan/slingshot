//! Composing a daemon with an executor that runs a script.
//!
//! This is the only place in the workspace where a daemon meets a fake, and it
//! lives here rather than in the daemon crate for one reason: no product build
//! has a dependency edge to this crate or to test support, so no product build
//! can reach this composition even by mistake. The daemon crate contains only
//! the unavailable executor, and it is composed unconditionally.
//!
//! Reached through the existing development binary's `test-daemon` subcommand
//! rather than through a binary of its own. A third workspace binary would be a
//! third thing a release has to account for, and this needs to exist only while
//! a test is running.

use slingshot_domain::operation_executor::{
    ExecutionIdentity, OperationExecutorOutcome, ProgressPort,
};
use slingshot_test_support::fake_operation_executor::{FakeOperationExecutor, InvocationCount};

/// Name of the development subcommand that runs this composition.
pub const TEST_DAEMON_COMMAND: &str = "test-daemon";

/// A progress port that keeps what it was told, for a test to read back.
#[derive(Debug, Default)]
pub struct RecordedProgress {
    /// Every note reported, in order.
    reported: std::sync::Mutex<Vec<String>>,
}

impl RecordedProgress {
    /// Returns every note reported so far, in order.
    #[must_use]
    pub fn reported(&self) -> Vec<String> {
        self.reported.lock().map(|held| held.clone()).unwrap_or_default()
    }
}

impl ProgressPort for RecordedProgress {
    fn report(&self, detail: &str) {
        if let Ok(mut held) = self.reported.lock() {
            held.push(detail.to_owned());
        }
    }
}

/// A progress port that drops everything, as a disconnected consumer does.
#[derive(Debug, Default, Clone, Copy)]
pub struct DroppedProgress;

impl ProgressPort for DroppedProgress {
    fn report(&self, _detail: &str) {}
}

/// One daemon composed with a scripted executor.
#[derive(Debug)]
pub struct TestDaemonComposition {
    /// The executor this composition runs operations through.
    executor: FakeOperationExecutor,
}

impl TestDaemonComposition {
    /// Returns a composition whose unscripted executions produce `fallback`.
    #[must_use]
    pub fn new(fallback: OperationExecutorOutcome) -> Self {
        Self { executor: FakeOperationExecutor::new(fallback) }
    }

    /// Returns the executor, so a test can script it.
    #[must_use]
    pub fn executor(&self) -> &FakeOperationExecutor {
        &self.executor
    }

    /// Runs one operation through this composition.
    pub fn execute(
        &self,
        identity: &ExecutionIdentity,
        progress: &dyn ProgressPort,
    ) -> OperationExecutorOutcome {
        self.executor.run(identity, progress)
    }

    /// Returns what one operation was asked to do.
    #[must_use]
    pub fn invocations(
        &self,
        author_target_identity_digest: &str,
        operation_identifier: &str,
    ) -> InvocationCount {
        self.executor.invocations(author_target_identity_digest, operation_identifier)
    }
}
