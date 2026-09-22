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

use crate::machine_outcome_envelope::{ArtifactAccess, Interruption, MachineOutcomeEnvelope};
use crate::model_context_protocol::active_request_registry::{
    ActiveRequestRegistry, AdmissionRefusal,
};
use crate::model_context_protocol::current_stateless_revision::{
    self, INVALID_REQUEST_ERROR, PARSE_ERROR, Refusal,
};
use crate::model_context_protocol::legacy_initialized_revision::{
    LegacyRefusal, LegacySession, Lifecycle, undecorated,
};
use crate::model_context_protocol::operation_execution::{self, ToolRunner};
use crate::model_context_protocol::progress_and_cancellation::ProgressRegistry;
use crate::model_context_protocol::protocol_diagnostics::ProtocolDiagnosticSink;
use crate::model_context_protocol::result_projection;
use crate::model_context_protocol::schema_projection;
use crate::model_context_protocol::standard_stream_transport::{
    LineSink, Message, MessageRefusal, OutputFailure, OutputQueue, read_message,
};
use crate::model_context_protocol::tool_catalog::{self, Provenance, ToolDescriptor};

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
    /// The installed command/control surface projected as protocol tools.
    tools: Vec<ToolDescriptor>,
    /// Where a tool call reaches the daemon, when this server has one.
    runner: Option<Box<dyn ToolRunner>>,
}

impl ::core::fmt::Debug for ServerApplication {
    fn fmt(&self, formatter: &mut ::core::fmt::Formatter<'_>) -> ::core::fmt::Result {
        formatter
            .debug_struct("ServerApplication")
            .field("tools", &self.tools.len())
            .field("reaches_a_daemon", &self.runner.is_some())
            .finish()
    }
}

impl Default for ServerApplication {
    fn default() -> Self {
        Self::new()
    }
}

impl ServerApplication {
    /// Returns the runner, when this server has one.
    fn runner_as_mut(&mut self) -> Option<&mut Box<dyn ToolRunner>> {
        self.runner.as_mut()
    }

    /// Returns a server that has answered nothing and reaches no daemon.
    ///
    /// A server with no runner advertises the catalog and answers every request
    /// that describes this build rather than running anything. That is what a
    /// check of the protocol surface needs, and it is honest about what it is:
    /// a tool call it cannot run is a local failure rather than an invented
    /// result.
    #[must_use]
    pub fn new() -> Self {
        Self::over(None)
    }

    /// Returns a server that runs tool calls through `runner`.
    #[must_use]
    pub fn over(runner: Option<Box<dyn ToolRunner>>) -> Self {
        Self {
            active: ActiveRequestRegistry::new(),
            diagnostics: ProtocolDiagnosticSink::new(),
            legacy: LegacySession::new(),
            output: OutputQueue::new(),
            progress: ProgressRegistry::new(),
            tools: tool_catalog::derive(&Provenance::recomputed()).unwrap_or_default(),
            runner,
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

    /// Takes everything this server has to say about why, for the stream that
    /// carries it.
    ///
    /// Standard output is protocol messages only, so a reason a call was not
    /// answered belongs on the diagnostic stream, and this is what lets the
    /// process holding that stream write it. Nothing here is part of an answer:
    /// a caller reading only standard output sees exactly the protocol.
    pub fn take_diagnostics(&mut self) -> Vec<String> {
        self.diagnostics.take()
    }

    /// Lets the sole standard-output writer drain queued complete responses.
    ///
    /// An active request is released only after its response has reached the
    /// sink in full. A failed writer leaves the same one failure transition for
    /// the caller to finish and never makes a queued answer look delivered.
    pub fn write_output(&mut self, sink: &mut dyn LineSink) -> bool {
        let acknowledged = self.output.acknowledged_requests().len();
        self.output.write_waiting(sink, std::time::Duration::ZERO);
        let delivered: Vec<String> = self.output.acknowledged_requests()[acknowledged..].to_vec();
        for identifier in delivered {
            self.active.acknowledged(&identifier);
        }
        self.output.accepts_more()
    }

    /// Serves one line and returns what it produced.
    pub fn serve_line(&mut self, line: &[u8]) -> Served {
        if !self.output.accepts_more() {
            return Served::Finished;
        }
        match read_message(line) {
            Err(refusal) => {
                let answer = self.unreadable(&refusal);
                self.enqueue(&answer);
                Served::Answered(answer)
            }
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
                if let Some(identifier) = parameters.get("requestId") {
                    let identifier = identifier_key(identifier);
                    self.progress.cancel(&identifier);
                    self.active.cancelling(&identifier);
                    self.active.cancelled(&identifier);
                }
            }
            other => {
                self.diagnostics.record(&format!("nothing acts on {other}"));
            }
        }
    }

    /// Answers one request, exactly once.
    fn requested(&mut self, identifier: &Value, method: &str, parameters: &Value) -> String {
        let key = identifier_key(identifier);
        if let Err(refusal) = self.active.reserve(&key) {
            let code = match refusal {
                AdmissionRefusal::Duplicate(_) => INVALID_REQUEST_ERROR,
                AdmissionRefusal::Saturated => RESOURCE_EXHAUSTED_ERROR,
            };
            return rendered_error(Some(identifier), code, &refusal.to_string());
        }
        let answered = self.answer(identifier, method, parameters);
        self.active.answered(&key);
        let line = match answered {
            Ok(result) => rendered_result(identifier, result),
            Err(refusal) => {
                let rendered = refusal.rendered();
                rendered_error_value(identifier, rendered)
            }
        };
        self.enqueue_response(&key, &line);
        line
    }

    /// Queues one answer or makes output's single terminal transition.
    fn enqueue(&mut self, line: &str) {
        if self.output.enqueue(line).is_err() {
            self.output.fail(OutputFailure::PressureExpired);
        }
    }

    /// Queues one response with the active request it settles on delivery.
    fn enqueue_response(&mut self, identifier: &str, line: &str) {
        if self.output.enqueue_response(identifier, line).is_err() {
            self.output.fail(OutputFailure::PressureExpired);
        }
    }

    /// Returns what one request is answered with.
    ///
    /// The first `initialize` selects the older era for the rest of the
    /// process: those clients send nothing about revisions after the
    /// handshake, and falling through to the stateless era would dispatch work
    /// on a session neither side had agreed. A process that never initialized
    /// is stateless, and every request says which revision it speaks.
    ///
    /// Both eras run `tools/call` through the same runner. The older era omits
    /// the modern result members; it does not omit the call.
    fn answer(
        &mut self,
        identifier: &Value,
        method: &str,
        parameters: &Value,
    ) -> Result<Value, Refusal> {
        if method == "initialize" {
            return Ok(self.legacy.initialize(requested_revision(parameters)));
        }
        let legacy = self.legacy.lifecycle() != Lifecycle::Fresh;
        if legacy {
            self.legacy.require_actionable(method).map_err(legacy_refusal)?;
        } else {
            let revision = requested_revision(parameters);
            current_stateless_revision::require_answerable(method, revision)?;
        }
        if method == "tools/call" {
            let result = self.tools_call(identifier, parameters)?;
            return Ok(if legacy {
                undecorated(result)
            } else {
                current_stateless_revision::decorated(method, result)
            });
        }
        if method == "resources/read" {
            let result = self.resources_read(identifier, parameters)?;
            return Ok(if legacy {
                undecorated(result)
            } else {
                current_stateless_revision::decorated(method, result)
            });
        }
        let payload = self.payload_for(method)?;
        Ok(if legacy {
            undecorated(payload)
        } else {
            current_stateless_revision::decorated(method, payload)
        })
    }

    /// Reads one published resource through the same daemon a tool call uses.
    ///
    /// A URI this server publishes is parsed first. An empty `contents` array
    /// after a successful parse would look like a present empty document, so a
    /// server that cannot reach the daemon answers a local-application-error
    /// document instead. An operation address is asked of the runner as
    /// `operation-result`. An artifact address is fetched as bytes and verified
    /// against the length and digest the daemon declared, because a result too
    /// large to inline is otherwise unreachable to a client holding its
    /// address. A maintenance result, which this process has no read path for,
    /// remains the same local failure rather than invented bytes.
    fn resources_read(&mut self, identifier: &Value, parameters: &Value) -> Result<Value, Refusal> {
        let uri = parameters.get("uri").and_then(Value::as_str).ok_or_else(|| {
            Refusal::ParametersUnusable {
                detail: "resources/read requires a string uri".to_owned(),
            }
        })?;
        let address = crate::model_context_protocol::resource_catalog::parse(uri)
            .map_err(|failure| Refusal::ParametersUnusable { detail: failure.to_string() })?;
        let retry_identifier =
            identifier.as_str().map_or_else(|| identifier_key(identifier), str::to_owned);
        match address {
            crate::model_context_protocol::resource_catalog::ResourceAddress::Artifact {
                namespace,
                operation_identifier,
                artifact_identifier,
            } => self.read_artifact_resource(
                uri,
                &operation_execution::ResourceNamespace {
                    profile: namespace.profile,
                    environment: namespace.environment,
                    author_target_identity_digest: namespace.author_target_identity_digest,
                },
                &operation_identifier,
                &artifact_identifier,
                &retry_identifier,
            ),
            crate::model_context_protocol::resource_catalog::ResourceAddress::Operation {
                operation_identifier,
                ..
            } => {
                let envelope =
                    self.read_operation_outcome(&operation_identifier, &retry_identifier);
                Ok(answered_resource(uri, &envelope))
            }
            crate::model_context_protocol::resource_catalog::ResourceAddress::MaintenanceResult {
                ..
            } => Ok(answered_resource(
                uri,
                &MachineOutcomeEnvelope::LocalApplicationError {
                    interruption: local_interruption(&retry_identifier),
                },
            )),
        }
    }

    /// Runs the observation one operation address names.
    fn read_operation_outcome(
        &mut self,
        operation_identifier: &str,
        retry_identifier: &str,
    ) -> MachineOutcomeEnvelope {
        let result_tool = self.tools.iter().find(|held| held.name == "operation-result").cloned();
        let Some(runner) = self.runner_as_mut() else {
            return MachineOutcomeEnvelope::LocalApplicationError {
                interruption: local_interruption(retry_identifier),
            };
        };
        let Some(tool) = result_tool else {
            return MachineOutcomeEnvelope::LocalApplicationError {
                interruption: local_interruption(retry_identifier),
            };
        };
        match runner.run(&tool, &json!({ "operation_identifier": operation_identifier })) {
            Ok(reached) => reached,
            Err(detail) => {
                self.diagnostics.record(&detail);
                MachineOutcomeEnvelope::LocalApplicationError {
                    interruption: local_interruption(retry_identifier),
                }
            }
        }
    }

    /// Reads one artifact's bytes and answers them as resource contents.
    ///
    /// The bytes come from the same boundary a tool call reaches, verified
    /// against the length and digest the daemon declared before the transfer
    /// began. JSON and textual artifacts are answered as text; every other
    /// media type is answered base64 in `blob`, which is the only encoding a
    /// resource may carry bytes in. Nothing partial is ever returned: a refused
    /// or mismatched transfer is a local failure and no contents.
    fn read_artifact_resource(
        &mut self,
        uri: &str,
        namespace: &operation_execution::ResourceNamespace,
        operation_identifier: &str,
        artifact_identifier: &str,
        retry_identifier: &str,
    ) -> Result<Value, Refusal> {
        let fetched = match self.runner_as_mut() {
            Some(runner) => runner.artifact_bytes(
                namespace,
                operation_identifier,
                artifact_identifier,
                crate::model_context_protocol::size_budget::maximum_resource_blob_bytes(),
            ),
            None => Err("this server reaches no daemon".to_owned()),
        };
        let fetched = match fetched {
            Ok(fetched) => fetched,
            Err(detail) => {
                self.diagnostics.record(&detail);
                return Ok(answered_resource(
                    uri,
                    &MachineOutcomeEnvelope::LocalApplicationError {
                        interruption: local_interruption(retry_identifier),
                    },
                ));
            }
        };
        if fetched.artifact_identifier != artifact_identifier {
            let detail = format!(
                "the daemon answered an artifact read for {artifact_identifier} with {}",
                fetched.artifact_identifier
            );
            self.diagnostics.record(&detail);
            return Ok(answered_resource(
                uri,
                &MachineOutcomeEnvelope::LocalApplicationError {
                    interruption: local_interruption(retry_identifier),
                },
            ));
        }
        let content = resource_content(uri, &fetched.media_type, &fetched.bytes);
        Ok(json!({ "contents": [content], "isError": false }))
    }

    /// Runs one `tools/call` through the runner both eras share.
    ///
    /// A call this server can run reaches the same daemon, the same registry
    /// command and the same operation identity a command line reaches, and
    /// what it answers is the document a command line writes for the same
    /// outcome. A call it cannot run is that same document with the
    /// local-failure tag: the protocol read the request and this build is what
    /// could not answer it, which is a tool result rather than a protocol
    /// error.
    fn tools_call(&mut self, identifier: &Value, parameters: &Value) -> Result<Value, Refusal> {
        let name = parameters.get("name").and_then(Value::as_str).ok_or_else(|| {
            Refusal::ParametersUnusable { detail: "tools/call requires a string name".to_owned() }
        })?;
        let arguments = parameters.get("arguments").cloned().unwrap_or_else(|| json!({}));
        let raw = serde_json::to_vec(&arguments)
            .map_err(|failure| Refusal::ParametersUnusable { detail: failure.to_string() })?;
        let (tool, accepted) =
            operation_execution::require_runnable(name, &raw, &Provenance::recomputed())
                .map_err(|failure| Refusal::ParametersUnusable { detail: failure.to_string() })?;
        // The identifier a caller quotes is the one they sent, spelled the way
        // they spelled it: a retry quoting a JSON string the protocol put
        // quotes around would not match their own request.
        let retry_identifier =
            identifier.as_str().map_or_else(|| identifier_key(identifier), str::to_owned);
        let mut reason = None;
        let envelope = match self.runner_as_mut() {
            None => MachineOutcomeEnvelope::LocalApplicationError {
                interruption: local_interruption(&retry_identifier),
            },
            Some(runner) => match runner.run(&tool, &accepted) {
                Ok(reached) => reached,
                Err(detail) => {
                    self.diagnostics.record(&detail);
                    reason = Some(detail);
                    MachineOutcomeEnvelope::LocalApplicationError {
                        interruption: local_interruption(&retry_identifier),
                    }
                }
            },
        };
        // The projection suppresses the CLI-signal tags, because they describe
        // a keystroke at a terminal this server does not have. A local failure
        // is not one of those: it is this build failing to answer a call it
        // accepted, and the document goes out whole. The reason follows it as
        // a second text item: a host shows a tool result to whoever called the
        // tool and rarely shows this process's diagnostic stream, so a reason
        // written only there would reach nobody who can act on it.
        let local_failure = envelope.tag() == "local_application_error";
        if local_failure {
            let text = crate::machine_readable_renderer::render(&envelope)
                .map_err(|refusal| Refusal::ParametersUnusable { detail: refusal.to_string() })?;
            let structured_content = serde_json::from_str(&text).unwrap_or_else(|_| json!({}));
            let mut content = vec![json!({ "type": "text", "text": text })];
            content.extend(reason.map(|detail| json!({ "type": "text", "text": detail })));
            return Ok(json!({
                "content": content,
                "structuredContent": structured_content,
                "isError": true,
            }));
        }
        // A host such as OpenCode displays `structuredContent` and ignores the
        // text items and resource links beside it. `operation-artifact` exists
        // to fetch the bytes an access envelope only names, so those bytes have
        // to be the structured content or the host repeats the envelope and
        // never shows the document. Every other tool keeps the command line's
        // envelope, which is what that host should see for those calls.
        if name == "operation-artifact"
            && let Some(inlined) = self.artifact_tool_result(identifier, &envelope, &accepted)
        {
            return Ok(inlined);
        }
        // A command that finished inside the submit call answers with an
        // artifact address when the result does not fit inline. The host
        // displays structured content and does not follow that address, so
        // the body has to be the structured content or the caller sees the
        // address and not the document.
        if let MachineOutcomeEnvelope::StructuredResultArtifactAccess { artifact } = &envelope
            && let Some(inlined) = self.inline_access(identifier, artifact)
        {
            return Ok(inlined);
        }
        let projected = result_projection::projected(&envelope, true, Vec::new())
            .map_err(|refusal| Refusal::ParametersUnusable { detail: refusal.to_string() })?;
        Ok(json!({
            "content": [{ "type": "text", "text": projected.text }],
            "structuredContent": projected.structured_content,
            "isError": projected.is_error,
        }))
    }

    /// Returns the artifact body as the tool result a structured-content host reads.
    ///
    /// `None` leaves the access envelope in place: the call did not name an
    /// artifact this process can inline, or the body would not fit in one
    /// protocol line. A fetch that was attempted and failed is a tool result
    /// whose structured content says why, because a host that only displays
    /// that member would otherwise report the access envelope as success.
    fn artifact_tool_result(
        &mut self,
        identifier: &Value,
        envelope: &MachineOutcomeEnvelope,
        arguments: &Value,
    ) -> Option<Value> {
        let artifact_identifier = arguments.get("artifact_identifier").and_then(Value::as_str)?;
        let expected = arguments.get("expected_content_digest").and_then(Value::as_str)?;
        let access = named_access(envelope, artifact_identifier)?;
        if access.content_digest != expected {
            return Some(visible_failure(
                "the digest this call quoted is not the digest the artifact access entry declares",
            ));
        }
        self.inline_access(identifier, &access)
    }

    /// Returns one artifact access entry's body as the tool result a host reads.
    ///
    /// `None` leaves the access envelope in place when the address cannot be
    /// read or the body would not fit in one protocol line. A fetch that was
    /// attempted and failed is a tool result whose structured content says why.
    fn inline_access(&mut self, identifier: &Value, access: &ArtifactAccess) -> Option<Value> {
        let address = match crate::model_context_protocol::resource_catalog::parse(&access.uri) {
            Ok(address) => address,
            Err(failure) => {
                return Some(visible_failure(&format!(
                    "the artifact access entry names an address this server cannot read: {failure}"
                )));
            }
        };
        let crate::model_context_protocol::resource_catalog::ResourceAddress::Artifact {
            namespace,
            operation_identifier,
            artifact_identifier,
        } = address
        else {
            return Some(visible_failure(
                "the artifact access entry does not name an artifact address",
            ));
        };
        if artifact_identifier != access.artifact_identifier {
            return Some(visible_failure(
                "the artifact access entry names a different artifact than this call",
            ));
        }
        let fetched = match self.runner_as_mut() {
            Some(runner) => runner.artifact_bytes(
                &operation_execution::ResourceNamespace {
                    profile: namespace.profile,
                    environment: namespace.environment,
                    author_target_identity_digest: namespace.author_target_identity_digest,
                },
                &operation_identifier,
                &artifact_identifier,
                crate::model_context_protocol::size_budget::maximum_resource_blob_bytes(),
            ),
            None => Err("this server reaches no daemon".to_owned()),
        };
        let fetched = match fetched {
            Ok(fetched) => fetched,
            Err(detail) => {
                self.diagnostics.record(&detail);
                return Some(visible_failure(&detail));
            }
        };
        if fetched.artifact_identifier != access.artifact_identifier
            || fetched.content_digest != access.content_digest
            || fetched.byte_length != access.byte_length
            || fetched.media_type != access.media_type
        {
            return Some(visible_failure(
                "the daemon's artifact bytes do not match the access entry this call quoted",
            ));
        }
        self.rendered_artifact(identifier, &fetched.media_type, &fetched.bytes)
    }

    /// Returns one verified artifact body as a tool result, or nothing when it
    /// would not fit in one protocol line.
    fn rendered_artifact(
        &self,
        identifier: &Value,
        media_type: &str,
        bytes: &[u8],
    ) -> Option<Value> {
        let (text, structured) = match artifact_body(media_type, bytes) {
            Ok(rendered) => rendered,
            Err(detail) => return Some(visible_failure(&detail)),
        };
        let result = json!({
            "content": [{ "type": "text", "text": text }],
            "structuredContent": structured,
            "isError": false,
        });
        let carried = current_stateless_revision::decorated("tools/call", result.clone());
        if rendered_result(identifier, carried).len()
            > crate::model_context_protocol::standard_stream_transport::maximum_line_bytes()
        {
            return None;
        }
        Some(result)
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

/// Returns the semantic payload one method answers with.
impl ServerApplication {
    fn payload_for(&self, method: &str) -> Result<Value, Refusal> {
        match method {
            "server/discover" => Ok(current_stateless_revision::discovery()),
            "ping" => Ok(json!({})),
            "tools/list" => Ok(json!({
                "tools": self.tools.iter().filter_map(|tool| {
                    Some(json!({
                        "name": &tool.name,
                        "title": &tool.title,
                        "description": &tool.description,
                        "inputSchema": schema_projection::input_schema(tool).ok()?,
                        "annotations": {
                            "readOnlyHint": tool.read_only_hint,
                            "destructiveHint": tool.destructive_hint,
                            "idempotentHint": tool.idempotent_hint,
                        }
                    }))
                }).collect::<Vec<_>>(),
            })),
            "resources/list" => Ok(json!({ "resources": [] })),
            "resources/templates/list" => Ok(json!({ "resourceTemplates": [
            { "uriTemplate": crate::model_context_protocol::resource_catalog::OPERATION_TEMPLATE, "name": "operation", "mimeType": "application/json" },
            { "uriTemplate": crate::model_context_protocol::resource_catalog::ARTIFACT_TEMPLATE, "name": "artifact", "mimeType": "application/octet-stream" },
            { "uriTemplate": crate::model_context_protocol::resource_catalog::MAINTENANCE_TEMPLATE, "name": "maintenance-result", "mimeType": "application/json" },
        ] })),
            other => Err(Refusal::MethodUnavailable { named: other.to_owned() }),
        }
    }
}

/// Returns the revision one request says it speaks.
fn requested_revision(parameters: &Value) -> &str {
    parameters[current_stateless_revision::REVISION_MEMBER].as_str().unwrap_or_default()
}

/// Returns the protocol refusal one older-era handshake failure becomes.
fn legacy_refusal(refusal: LegacyRefusal) -> Refusal {
    match refusal {
        LegacyRefusal::NotInitialized { named } => Refusal::NotInitialized { named },
        LegacyRefusal::MethodUnavailable { named } => Refusal::MethodUnavailable { named },
    }
}

/// Returns the local failure one tool call produces.
///
/// A call that never reached a daemon claims nothing about one, so the
/// interruption it names is the pre-receipt one: no operation exists to name,
/// and the retry identifier is what a caller quotes to find out whether
/// anything happened. What went wrong travels as a diagnostic rather than in
/// the envelope, because the envelope's interruption vocabulary states exactly
/// that and nothing about why.
fn local_interruption(retry_identifier: &str) -> Interruption {
    Interruption::PreReceipt { retry_identifier: retry_identifier.to_owned() }
}

/// Returns one envelope answered as one canonical resource content.
///
/// The text is the command line's own rendering of the same document, so a
/// client reading the resource and a caller reading the command line cannot
/// disagree about what happened.
fn answered_resource(uri: &str, envelope: &MachineOutcomeEnvelope) -> Value {
    let text = crate::machine_readable_renderer::render(envelope).unwrap_or_default();
    let is_error =
        envelope.tag() == "local_application_error" || envelope.tag() == "operation_terminal_error";
    json!({
        "contents": [{ "uri": uri, "mimeType": "application/json", "text": text }],
        "isError": is_error,
    })
}

/// Returns the access entry one outcome names for one artifact.
fn named_access(
    envelope: &MachineOutcomeEnvelope,
    artifact_identifier: &str,
) -> Option<ArtifactAccess> {
    match envelope {
        MachineOutcomeEnvelope::StructuredResultArtifactAccess { artifact }
            if artifact.artifact_identifier == artifact_identifier =>
        {
            Some(artifact.clone())
        }
        MachineOutcomeEnvelope::CommandArtifactAccess { artifacts, .. } => artifacts
            .iter()
            .find(|access| access.artifact_identifier == artifact_identifier)
            .cloned(),
        _ => None,
    }
}

/// Returns a tool failure whose structured content states the reason.
///
/// The reason is the structured content, not a second text item, because a
/// host that displays only structured content never shows the second item.
fn visible_failure(detail: &str) -> Value {
    let structured = json!({
        "outcome": "local_application_error",
        "detail": detail,
    });
    let text = structured.to_string();
    json!({
        "content": [{ "type": "text", "text": text }],
        "structuredContent": structured,
        "isError": true,
    })
}

/// Returns the artifact body as text plus the structured content a host displays.
///
/// JSON becomes the parsed document, so a host that shows structured content
/// shows the document rather than an access envelope. Any other media type
/// becomes base64 beside its media type. The text is that same document, so
/// the two members cannot disagree.
///
/// # Errors
///
/// Returns why the bytes are not a document this result can carry.
fn artifact_body(media_type: &str, bytes: &[u8]) -> Result<(String, Value), String> {
    if media_type == "application/json" || media_type.ends_with("+json") {
        let text = std::str::from_utf8(bytes)
            .map_err(|_| "the artifact is declared JSON and is not UTF-8".to_owned())?;
        let document: Value = serde_json::from_str(text)
            .map_err(|_| "the artifact is declared JSON and is not a JSON document".to_owned())?;
        // A JSON object is the structured content itself. Anything else is
        // wrapped, because structured content is an object and a host that
        // displays it would otherwise have nothing to show for an array.
        if document.is_object() {
            return Ok((text.to_owned(), document));
        }
        let structured = json!({ "document": document });
        return Ok((structured.to_string(), structured));
    }
    if media_type.starts_with("text/") {
        let text = std::str::from_utf8(bytes)
            .map_err(|_| "the artifact is declared text and is not UTF-8".to_owned())?;
        let structured = json!({ "media_type": media_type, "text": text });
        return Ok((structured.to_string(), structured));
    }
    use base64::Engine as _;
    let blob = base64::engine::general_purpose::STANDARD.encode(bytes);
    let structured = json!({ "media_type": media_type, "encoding": "base64", "blob": blob });
    Ok((structured.to_string(), structured))
}

/// Returns one artifact carried as the resource content its media type allows.
///
/// A JSON or textual artifact travels as `text` so a client reads exactly the
/// bytes a command line would have parsed. Every other media type travels
/// base64 in `blob`, because that is the only member a resource carries bytes
/// in and a media type this build cannot promise is textual would otherwise be
/// corrupted by an encoding guessed at here.
fn resource_content(uri: &str, media_type: &str, bytes: &[u8]) -> Value {
    let textual = media_type == "application/json"
        || media_type.starts_with("text/")
        || media_type.ends_with("+json");
    if textual {
        let text = String::from_utf8_lossy(bytes).into_owned();
        return json!({ "uri": uri, "mimeType": media_type, "text": text });
    }
    use base64::Engine as _;
    let blob = base64::engine::general_purpose::STANDARD.encode(bytes);
    json!({ "uri": uri, "mimeType": media_type, "blob": blob })
}

/// Returns one rendered result line.
fn rendered_result(identifier: &Value, result: Value) -> String {
    json!({ "jsonrpc": "2.0", "id": identifier, "result": result }).to_string()
}

/// Returns one rendered error line.
fn rendered_error(identifier: Option<&Value>, code: i64, message: &str) -> String {
    json!({ "jsonrpc": "2.0", "id": identifier, "error": { "code": code, "message": message } })
        .to_string()
}

/// Returns one rendered error line carrying an already-built error object.
fn rendered_error_value(identifier: &Value, error: Value) -> String {
    json!({ "jsonrpc": "2.0", "id": identifier, "error": error }).to_string()
}

/// Returns a type-preserving registry key for one JSON-RPC identifier.
///
/// String and numeric identifiers with the same human spelling are distinct
/// JSON-RPC ids, so the serialized value (including its JSON type) is the key.
fn identifier_key(identifier: &Value) -> String {
    serde_json::to_string(identifier).unwrap_or_else(|_| "null".to_owned())
}
