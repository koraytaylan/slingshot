//! Runtime-owned subscriptions with automatic reader-capacity release.

use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};
use tokio::sync::Notify;
use tokio_util::sync::CancellationToken;

use super::{Detachment, WaitBounds, WaitRefusal, WaitUpdate, WaiterRegistry};

#[derive(Debug)]
struct State {
    registries: BTreeMap<String, WaiterRegistry>,
    readers: usize,
}

/// One runtime's bounded local observers, independent of author subscriptions.
/// The service must serialize repository observation, attachment, and committed
/// publication under its runtime lock, so attaching cannot miss a commit.
#[derive(Debug, Clone)]
pub struct RuntimeWaiters {
    state: Arc<Mutex<State>>,
    changed: Arc<Notify>,
    stopping: CancellationToken,
    capacity: usize,
}

/// Why an observer cannot be registered; no refusal changes durable work.
#[derive(Debug, thiserror::Error)]
pub enum AttachRefusal {
    /// Runtime-wide observer capacity is already held.
    #[error("local observer capacity is exhausted")]
    Capacity,
    /// This operation's observer capacity is already held.
    #[error(transparent)]
    OperationCapacity(#[from] WaitRefusal),
    /// A caller claimed a revision newer than the retained operation.
    #[error("the observed revision is newer than the retained operation")]
    FutureRevision,
    /// The runtime is shutting down.
    #[error("the runtime is stopping")]
    Stopping,
}

impl RuntimeWaiters {
    /// Creates bounded observation state owned by the runtime cancellation scope.
    #[must_use]
    pub fn new(stopping: CancellationToken) -> Self {
        Self {
            state: Arc::new(Mutex::new(State { registries: BTreeMap::new(), readers: 0 })),
            changed: Arc::new(Notify::new()),
            stopping,
            capacity: slingshot_local_protocol::foundation_contract::FoundationContract::embedded()
                .server
                .connection_capacity as usize,
        }
    }

    /// Registers against a current persisted observation while the runtime lock
    /// excludes publication. Terminal state already observed closes immediately.
    ///
    /// # Errors
    /// Returns a capacity, shutdown, or future-revision refusal without mutation.
    pub fn attach(
        &self,
        operation: &str,
        observed: u64,
        current: WaitUpdate,
    ) -> Result<RuntimeWait, AttachRefusal> {
        if self.stopping.is_cancelled() {
            return Err(AttachRefusal::Stopping);
        }
        if observed > current.revision() {
            return Err(AttachRefusal::FutureRevision);
        }
        let finished = current.ends_the_wait() && observed == current.revision();
        let mut state = self.state.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        if state.readers >= self.capacity {
            return Err(AttachRefusal::Capacity);
        }
        let registry = state
            .registries
            .entry(operation.to_owned())
            .or_insert_with(|| WaiterRegistry::new(WaitBounds::embedded(), current.revision()));
        let ticket = registry.attach(observed, Some(current))?;
        state.readers += 1;
        Ok(RuntimeWait {
            owner: self.clone(),
            operation: operation.to_owned(),
            ticket: Some(ticket),
            finished,
        })
    }

    /// Publishes only after the corresponding repository commit has succeeded.
    /// No observer means no registry or retained event history is allocated.
    pub fn publish(&self, operation: &str, update: &WaitUpdate) {
        let mut state = self.state.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        if let Some(registry) = state.registries.get_mut(operation) {
            registry.publish(update);
        }
        drop(state);
        self.changed.notify_waiters();
    }

    /// Number of currently attached observers, across all operations.
    #[must_use]
    pub fn attached(&self) -> usize {
        self.state.lock().unwrap_or_else(std::sync::PoisonError::into_inner).readers
    }
}

/// A connection-owned subscription. Dropping it releases exactly its reader
/// slot and removes the operation registry when the final observer leaves.
#[derive(Debug)]
pub struct RuntimeWait {
    owner: RuntimeWaiters,
    operation: String,
    ticket: Option<u64>,
    finished: bool,
}

impl RuntimeWait {
    /// Takes an already queued catch-up update without awaiting a future
    /// broadcast. Service dispatch uses this to answer the first framed wait
    /// immediately; the handle remains available to an async owner for later
    /// updates and is dropped safely when the connection closes.
    pub fn take_ready(&mut self) -> Option<WaitUpdate> {
        if self.finished {
            return None;
        }
        let update = {
            let mut state =
                self.owner.state.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
            state.registries.get_mut(&self.operation)?.take(self.ticket?)
        }?;
        if update.ends_the_wait() {
            self.finished = true;
            self.detach();
        }
        Some(update)
    }
    /// Waits without a read timeout. Runtime shutdown or caller cancellation
    /// detaches this observer only; neither creates an operation outcome.
    pub async fn next(&mut self, cancelled: &CancellationToken) -> Option<WaitUpdate> {
        loop {
            if self.finished || cancelled.is_cancelled() || self.owner.stopping.is_cancelled() {
                self.detach();
                return None;
            }
            let changed = self.owner.changed.clone();
            let notified = changed.notified();
            tokio::pin!(notified);
            // Register before inspecting the queue, closing the check/sleep race.
            notified.as_mut().enable();
            let update = {
                let mut state =
                    self.owner.state.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
                state.registries.get_mut(&self.operation)?.take(self.ticket?)
            };
            if let Some(update) = update {
                if update.ends_the_wait() {
                    self.finished = true;
                    self.detach();
                }
                return Some(update);
            }
            tokio::select! {
                _ = notified => {},
                _ = cancelled.cancelled() => { self.detach(); return None; },
                _ = self.owner.stopping.cancelled() => { self.detach(); return None; },
            }
        }
    }

    fn detach(&mut self) {
        let Some(ticket) = self.ticket.take() else {
            return;
        };
        self.finished = true;
        let mut state = self.owner.state.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        if let Some(registry) = state.registries.get_mut(&self.operation) {
            if registry.detach(ticket, Detachment::Disconnected) {
                state.readers -= 1;
            }
            if state
                .registries
                .get(&self.operation)
                .is_some_and(|registry| registry.attached() == 0)
            {
                state.registries.remove(&self.operation);
            }
        }
    }
}

impl Drop for RuntimeWait {
    fn drop(&mut self) {
        self.detach();
    }
}
