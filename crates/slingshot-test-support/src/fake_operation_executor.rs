//! An executor that runs a script instead of a remote system.
//!
//! Every outcome the boundary permits has to be reachable in a test, including
//! the ones a real system produces rarely and at the worst moment: a submission
//! whose fate is unknown, a remote that succeeded while the result did not
//! arrive, a retry policy that ran out. A fake driven by a script reaches all of
//! them on demand and in a fixed order, so a test that depends on one of those
//! moments is deterministic rather than lucky.
//!
//! It also counts what it was asked to do, per target and identifier. That is
//! what lets a restart or idempotency test tell a replay from a second
//! execution, which is the distinction those tests exist to make and the one
//! nothing else can observe from outside.
//!
//! This crate is reachable only from tests and from the development binary.
//! No product build has an edge to it, so nothing here can be composed into
//! something a user runs.

use std::collections::BTreeMap;
use std::sync::Mutex;

use slingshot_domain::operation_executor::{
    ExecutionIdentity, OperationExecutorOutcome, ProgressPort,
};

/// One step a scripted execution takes before it produces its outcome.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ScriptedStep {
    /// Report one bounded progress note.
    Progress {
        /// What to report.
        detail: String,
    },
}

/// What one scripted execution does, and what it ends as.
#[derive(Debug, Clone)]
pub struct Script {
    /// The outcome this execution produces.
    pub outcome: OperationExecutorOutcome,
    /// What it does before producing it, in order.
    pub steps: Vec<ScriptedStep>,
}

/// How many times one operation has been executed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct InvocationCount {
    /// Executions that ran a script.
    pub executed: u32,
    /// Requests answered without running one, because the script ran out.
    pub replayed: u32,
}

/// An executor that follows a script and remembers what it was asked.
#[derive(Debug)]
pub struct FakeOperationExecutor {
    /// What each operation's executions do, in order, keyed by identifier.
    scripts: Mutex<BTreeMap<(String, String), Vec<Script>>>,
    /// What each operation was asked to do.
    invocations: Mutex<BTreeMap<(String, String), InvocationCount>>,
    /// What every execution does when no script names it.
    fallback: OperationExecutorOutcome,
}

impl FakeOperationExecutor {
    /// Returns an executor whose unscripted executions all produce `fallback`.
    #[must_use]
    pub fn new(fallback: OperationExecutorOutcome) -> Self {
        Self {
            scripts: Mutex::new(BTreeMap::new()),
            invocations: Mutex::new(BTreeMap::new()),
            fallback,
        }
    }

    /// Scripts what one operation's next execution does.
    ///
    /// Scripts are consumed in order, so a test that wants a recovery followed
    /// by a success writes exactly that and gets exactly that.
    pub fn script(
        &self,
        author_target_identity_digest: &str,
        operation_identifier: &str,
        script: Script,
    ) {
        if let Ok(mut held) = self.scripts.lock() {
            held.entry((author_target_identity_digest.to_owned(), operation_identifier.to_owned()))
                .or_default()
                .push(script);
        }
    }

    /// Returns what one operation was asked to do.
    ///
    /// Keyed by target as well as identifier, so an identifier used against two
    /// targets counts as the two separate operations it is.
    #[must_use]
    pub fn invocations(
        &self,
        author_target_identity_digest: &str,
        operation_identifier: &str,
    ) -> InvocationCount {
        self.invocations
            .lock()
            .ok()
            .and_then(|held| {
                held.get(&(
                    author_target_identity_digest.to_owned(),
                    operation_identifier.to_owned(),
                ))
                .copied()
            })
            .unwrap_or(InvocationCount { executed: 0, replayed: 0 })
    }

    /// Runs whatever `identity` is scripted to do next.
    ///
    /// Progress goes out before the outcome and is never waited on: a consumer
    /// that stopped listening must not be able to stall an execution, and the
    /// only way to be sure of that is for the executor never to learn whether
    /// anyone heard.
    pub fn run(
        &self,
        identity: &ExecutionIdentity,
        progress: &dyn ProgressPort,
    ) -> OperationExecutorOutcome {
        let key =
            (identity.author_target_identity_digest.clone(), identity.operation_identifier.clone());
        let scripted = self.scripts.lock().ok().and_then(|mut held| {
            held.get_mut(&key)
                .and_then(|queued| if queued.is_empty() { None } else { Some(queued.remove(0)) })
        });
        self.record(&key, scripted.is_some());
        let Some(script) = scripted else {
            return self.fallback.clone();
        };
        for step in &script.steps {
            let ScriptedStep::Progress { detail } = step;
            progress.report(detail);
        }
        script.outcome
    }

    /// Counts one invocation, executed or replayed.
    fn record(&self, key: &(String, String), executed: bool) {
        if let Ok(mut held) = self.invocations.lock() {
            let counted =
                held.entry(key.clone()).or_insert(InvocationCount { executed: 0, replayed: 0 });
            if executed {
                counted.executed += 1;
            } else {
                counted.replayed += 1;
            }
        }
    }
}
