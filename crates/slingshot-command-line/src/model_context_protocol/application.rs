//! Composing the transport, the revisions, and the services into one server.
//!
//! Assembly lives apart from the pieces it assembles: which revision answered,
//! which tool ran, and which resource was read are decisions owned elsewhere,
//! and this owns the wiring that makes exactly one of each happen per request.
//!
//! # One process, one owner of standard output
//!
//! While this server runs, ordinary command rendering is inactive. Two writers
//! on one stream produce interleaved halves of two messages, and a client
//! parsing lines cannot recover from that - so the server owns the stream for
//! as long as it owns the process.
//!
//! # Ending is bounded
//!
//! Input ending, output failing, and a client going away all reach the same
//! place: detach every waiter, release every reservation once, write nothing
//! further, and finish. None of it waits indefinitely on a writer, a reader, or
//! a diagnostic sink that has stopped moving, because a server that hung while
//! shutting down would hold the terminal it was asked to give back.

use serde_json::{Value, json};

use crate::model_context_protocol::active_request_registry::{
    ActiveRequestRegistry, AdmissionRefusal,
};
use crate::model_context_protocol::current_stateless_revision::{
    self, INVALID_REQUEST_ERROR, PARSE_ERROR, Refusal,
};
use crate::model_context_protocol::legacy_initialized_revision::{LegacySession, Lifecycle};
use crate::model_context_protocol::progress_and_cancellation::ProgressRegistry;
use crate::model_context_protocol::protocol_diagnostics::ProtocolDiagnosticSink;
use crate::model_context_protocol::standard_stream_transport::{
    Message, MessageRefusal, OutputFailure, OutputQueue, read_message,
};

/// The error a request receives when this server is already as busy as it gets.
pub const RESOURCE_EXHAUSTED_ERROR: i64 = -32_003;

/// What one line produced.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Served {
    /// One answer, to be written as one line.
    Answered(String),
    /// Nothing, because the line was a notification.
    Silent,
    /// Nothing further: input ended or output failed.
    Finished,
}

/// The whole server, over the pieces it composes.
#[derive(Debug)]
pub struct ServerApplication {
    /// Which requests are in flight.
    active: ActiveRequestRegistry,
    /// Where diagnostics go.
    diagnostics: ProtocolDiagnosticSink,
    /// Which era this session speaks, once it is known.
    legacy: LegacySession,
    /// The one queue every answer goes through.
    output: OutputQueue,
    /// Who is being told what.
    progress: ProgressRegistry,
}

impl Default for ServerApplication {
    fn default() -> Self {
        Self::new()
    }
}

impl ServerApplication {
    /// Returns a server that has answered nothing.
    #[must_use]
    pub fn new() -> Self {
        Self {
            active: ActiveRequestRegistry::new(),
            diagnostics: ProtocolDiagnosticSink::new(),
            legacy: LegacySession::new(),
            output: OutputQueue::new(),
            progress: ProgressRegistry::new(),
        }
    }

    /// Returns how many requests are in flight.
    #[must_use]
    pub fn active(&self) -> usize {
        self.active.active()
    }

    /// Returns how far the legacy handshake has got, if a client began one.
    #[must_use]
    pub fn lifecycle(&self) -> Lifecycle {
        self.legacy.lifecycle()
    }

    /// Returns how many diagnostics were dropped rather than written.
    #[must_use]
    pub fn dropped_diagnostics(&self) -> usize {
        self.diagnostics.dropped()
    }

    /// Serves one line and returns what it produced.
    pub fn serve_line(&mut self, line: &[u8]) -> Served {
        if !self.output.accepts_more() {
            return Served::Finished;
        }
        match read_message(line) {
            Err(refusal) => Served::Answered(self.unreadable(&refusal)),
            Ok(Message::Notification { method, parameters }) => {
                self.notified(&method, &parameters);
                Served::Silent
            }
            Ok(Message::Request { identifier, method, parameters }) => {
                Served::Answered(self.requested(&identifier, &method, &parameters))
            }
        }
    }

    /// Returns the error one unreadable line receives.
    fn unreadable(&mut self, refusal: &MessageRefusal) -> String {
        let code = match refusal {
            MessageRefusal::EncodingInvalid | MessageRefusal::NotReadable => PARSE_ERROR,
            _ => INVALID_REQUEST_ERROR,
        };
        self.diagnostics.record(&refusal.to_string());
        rendered_error(None, code, &refusal.to_string())
    }

    /// Acts on one notification, which is answered never.
    fn notified(&mut self, method: &str, parameters: &Value) {
        match method {
            "notifications/initialized" => {
                self.legacy.initialized();
            }
            "notifications/cancelled" => {
                if let Some(identifier) = parameters["requestId"].as_str() {
                    self.progress.cancel(identifier);
                    self.active.cancelling(identifier);
                    self.active.cancelled(identifier);
                }
            }
            other => {
                self.diagnostics.record(&format!("nothing acts on {other}"));
            }
        }
    }

    /// Answers one request, exactly once.
    fn requested(&mut self, identifier: &str, method: &str, parameters: &Value) -> String {
        if let Err(refusal) = self.active.reserve(identifier) {
            let code = match refusal {
                AdmissionRefusal::Duplicate(_) => INVALID_REQUEST_ERROR,
                AdmissionRefusal::Saturated => RESOURCE_EXHAUSTED_ERROR,
            };
            return rendered_error(Some(identifier), code, &refusal.to_string());
        }
        let answered = self.answer(method, parameters);
        self.active.answered(identifier);
        let line = match answered {
            Ok(result) => rendered_result(identifier, result),
            Err(refusal) => {
                let rendered = refusal.rendered();
                rendered_error_value(identifier, rendered)
            }
        };
        self.output.enqueue(&line).ok();
        self.active.acknowledged(identifier);
        line
    }

    /// Returns what one request is answered with.
    fn answer(&mut self, method: &str, parameters: &Value) -> Result<Value, Refusal> {
        if method == "initialize" {
            return Ok(self.legacy.initialize(requested_revision(parameters)));
        }
        let revision = requested_revision(parameters);
        current_stateless_revision::require_answerable(method, revision)?;
        let payload = match method {
            "server/discover" => current_stateless_revision::discovery(),
            "ping" => json!({}),
            "tools/list" => json!({ "tools": [] }),
            "tools/call" => json!({ "content": [] }),
            "resources/list" => json!({ "resources": [] }),
            "resources/templates/list" => json!({ "resourceTemplates": [] }),
            _ => json!({ "contents": [] }),
        };
        Ok(current_stateless_revision::decorated(method, payload))
    }

    /// Ends everything, once, and says what was detached.
    ///
    /// Idempotent, because it is reached from input ending and from output
    /// failing, and both can happen at once.
    pub fn finish(&mut self, reason: OutputFailure) -> Vec<String> {
        self.output.fail(reason);
        let detached = self.progress.detach_all();
        self.active.release_all();
        detached
    }
}

/// Returns the revision one request says it speaks.
fn requested_revision(parameters: &Value) -> &str {
    parameters[current_stateless_revision::REVISION_MEMBER].as_str().unwrap_or_default()
}

/// Returns one rendered result line.
fn rendered_result(identifier: &str, result: Value) -> String {
    json!({ "id": identifier, "result": result }).to_string()
}

/// Returns one rendered error line.
fn rendered_error(identifier: Option<&str>, code: i64, message: &str) -> String {
    json!({ "id": identifier, "error": { "code": code, "message": message } }).to_string()
}

/// Returns one rendered error line carrying an already-built error object.
fn rendered_error_value(identifier: &str, error: Value) -> String {
    json!({ "id": identifier, "error": error }).to_string()
}
