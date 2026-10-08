//! Reserving requests on the coordinator and rendering their worker results.
//!
//! A cancelled identifier remains reserved until its local wait has detached.
//! A completed identifier remains reserved until its response has been written.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use serde_json::Value;

use super::{
    ServerApplication, identifier_key, legacy_refusal, rendered_error, rendered_error_value,
    rendered_result, requested_revision,
};
use crate::model_context_protocol::active_request_registry::AdmissionRefusal;
use crate::model_context_protocol::current_stateless_revision::{self, Refusal};
use crate::model_context_protocol::legacy_initialized_revision::{Lifecycle, undecorated};
use crate::model_context_protocol::operation_execution::ToolRunner;
use crate::model_context_protocol::standard_stream_transport::{
    Message, maximum_line_bytes, read_message,
};

/// One admitted call, with the revision selected before it leaves intake.
pub(super) struct DeferredRequest {
    /// Original JSON identifier, preserved on the wire.
    identifier: Value,
    /// Typed identifier used by the active registry.
    key: String,
    /// The tool or resource method to execute.
    method: String,
    /// Validated transport parameters; semantic validation precedes execution.
    parameters: Value,
    /// Whether the coordinator negotiated the initialized revision.
    legacy: bool,
    /// Ends only this local wait, never a retained remote operation.
    pub(super) cancellation: Arc<AtomicBool>,
}

/// A worker's answer, correlated with its exact reservation.
pub(super) struct Completion {
    /// The active identifier to settle.
    key: String,
    /// Distinguishes a late worker from a later reuse of its identifier.
    cancellation: Arc<AtomicBool>,
    /// Serialized response, bounded before entering the completion channel.
    line: String,
    /// Bounded diagnostic records carried separately from protocol output.
    diagnostics: Vec<String>,
}

impl DeferredRequest {
    /// Builds a bounded fallback before transferring the request to a thread.
    pub(super) fn failed(&self) -> Completion {
        Completion {
            key: self.key.clone(),
            cancellation: Arc::clone(&self.cancellation),
            line: rendered_error(
                Some(&self.identifier),
                current_stateless_revision::INTERNAL_ERROR,
                "the local worker failed; retry this request",
            ),
            diagnostics: Vec::new(),
        }
    }

    /// Executes one request using the same renderers as direct application calls.
    pub(super) fn execute(self, runner: Box<dyn ToolRunner>) -> Completion {
        if self.cancellation.load(Ordering::SeqCst) {
            return self.failed();
        }
        let mut application = ServerApplication::over(Some(runner));
        let answer = if self.method == "tools/call" {
            application.tools_call(&self.identifier, &self.parameters)
        } else {
            application.resources_read(&self.identifier, &self.parameters)
        };
        let mut line = match answer {
            Ok(result) => {
                let result = if self.legacy {
                    undecorated(result)
                } else {
                    current_stateless_revision::decorated(&self.method, result)
                };
                rendered_result(&self.identifier, result)
            }
            Err(refusal) => rendered_error_value(&self.identifier, refusal.rendered()),
        };
        if line.len() > maximum_line_bytes() {
            line = rendered_error(
                Some(&self.identifier),
                super::RESOURCE_EXHAUSTED_ERROR,
                "the response exceeds the transport limit; request a smaller result",
            );
        }
        Completion {
            key: self.key,
            cancellation: self.cancellation,
            line,
            diagnostics: application.take_diagnostics(),
        }
    }
}

impl ServerApplication {
    /// Handles controls immediately and reserves calls before dispatch.
    pub(super) fn prepare_line(
        &mut self,
        line: &[u8],
        worker_limit: usize,
    ) -> Option<DeferredRequest> {
        let message = match read_message(line) {
            Ok(message) => message,
            Err(refusal) => {
                let answer = self.unreadable(&refusal);
                self.enqueue(&answer);
                return None;
            }
        };
        match message {
            Message::Notification { method, parameters } => {
                self.notified(&method, &parameters);
                None
            }
            Message::Request { identifier, method, parameters } => {
                if method == "tools/call" || method == "resources/read" {
                    self.defer(identifier, method, parameters, worker_limit)
                } else {
                    self.requested(&identifier, &method, &parameters);
                    None
                }
            }
        }
    }

    /// Validates the negotiated era before any worker can cause effects.
    pub(super) fn require_actionable(
        &self,
        method: &str,
        parameters: &Value,
    ) -> Result<bool, Refusal> {
        let legacy = self.legacy.lifecycle() != Lifecycle::Fresh;
        if legacy {
            self.legacy.require_actionable(method).map_err(legacy_refusal)?;
        } else {
            current_stateless_revision::require_answerable(method, requested_revision(parameters))?;
        }
        Ok(legacy)
    }

    /// Reserves the identifier and refuses excess workers without dispatching.
    fn defer(
        &mut self,
        identifier: Value,
        method: String,
        parameters: Value,
        worker_limit: usize,
    ) -> Option<DeferredRequest> {
        let key = identifier_key(&identifier);
        if let Err(refusal) = self.active.reserve(&key) {
            let code = match refusal {
                AdmissionRefusal::Duplicate(_) => current_stateless_revision::INVALID_REQUEST_ERROR,
                AdmissionRefusal::Saturated => super::RESOURCE_EXHAUSTED_ERROR,
            };
            self.enqueue(&rendered_error(Some(&identifier), code, &refusal.to_string()));
            return None;
        }
        let legacy = match self.require_actionable(&method, &parameters) {
            Ok(legacy) => legacy,
            Err(refusal) => {
                self.settle_refusal(&key, &rendered_error_value(&identifier, refusal.rendered()));
                return None;
            }
        };
        if self.pending.len() >= worker_limit {
            self.settle_refusal(
                &key,
                &rendered_error(
                    Some(&identifier),
                    super::RESOURCE_EXHAUSTED_ERROR,
                    "all local workers are occupied; retry after a request completes",
                ),
            );
            return None;
        }
        let cancellation = Arc::new(AtomicBool::new(false));
        self.pending.insert(key.clone(), Arc::clone(&cancellation));
        Some(DeferredRequest { identifier, key, method, parameters, legacy, cancellation })
    }

    /// Retains refused request identities until their error has been delivered.
    fn settle_refusal(&mut self, key: &str, line: &str) {
        self.active.answered(key);
        self.enqueue_response(key, line);
    }

    /// Settles only the worker still owning this reservation.
    pub(super) fn completed(&mut self, completion: Completion) {
        if !self
            .pending
            .get(&completion.key)
            .is_some_and(|cancellation| Arc::ptr_eq(cancellation, &completion.cancellation))
        {
            return;
        }
        self.pending.remove(&completion.key);
        self.progress.cancel(&completion.key);
        if completion.cancellation.load(Ordering::SeqCst) {
            self.active.cancelled(&completion.key);
        } else {
            self.active.answered(&completion.key);
            self.enqueue_response(&completion.key, &completion.line);
        }
        for diagnostic in completion.diagnostics {
            self.diagnostics.record(&diagnostic);
        }
    }
}
