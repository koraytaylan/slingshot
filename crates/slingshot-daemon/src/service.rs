//! The target-scoped application service.
//!
//! The service turns one frame payload into one response frame. It holds the
//! ownership of its namespace, so it can answer with the live readiness nonce
//! and decide whether a stop is authorized, and it never dispatches a method
//! for a request whose control version it does not speak.

use slingshot_local_protocol::control::{HELLO_METHOD, HelloResult, RUNTIME_UNAVAILABLE_CODE};
use slingshot_local_protocol::envelope::{
    self, ControlError, ControlRequest, ControlResponse, METHOD_NOT_FOUND_CODE,
};
use slingshot_local_protocol::foundation_contract::FoundationContract;
use slingshot_local_protocol::message::{OperationRequest, OperationResponse};
use slingshot_local_protocol::ping::{
    self, PING_METHOD, PingResult, STOP_METHOD, StopArguments, StopResult,
};

use crate::ownership::DaemonOwnership;
use crate::runtime_builder::DurableRuntime;
use tokio_util::sync::CancellationToken;

/// Keep SQLite behind a mutex: connections are movable, but not shareable.
/// The runtime retains its namespace lock until all runtime resources close.
#[derive(Debug)]
enum ServiceLifetime {
    Control(DaemonOwnership),
    Runtime(Box<std::sync::Mutex<DurableRuntime>>),
}

/// Version of the product this daemon was built from.
const PRODUCT_VERSION: &str = env!("CARGO_PKG_VERSION");

/// What the connection loop must do after one request.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ServiceOutcome {
    /// Write the frame and keep serving.
    Respond(Vec<u8>),
    /// Write the frame, then shut the service down in order.
    RespondThenStop(Vec<u8>),
    /// Write a bounded ordered response stream and keep serving.
    RespondMany(Vec<Vec<u8>>),
}

impl ServiceOutcome {
    /// Returns the frame this outcome carries.
    #[must_use]
    pub fn frame(&self) -> &[u8] {
        match self {
            Self::Respond(frame) | Self::RespondThenStop(frame) => frame,
            Self::RespondMany(frames) => frames.first().map(Vec::as_slice).unwrap_or(&[]),
        }
    }

    /// Reports whether this outcome ends the service.
    #[must_use]
    pub fn stops(&self) -> bool {
        matches!(self, Self::RespondThenStop(_))
    }
}

/// The service one daemon offers for one runtime namespace.
#[derive(Debug)]
pub struct DaemonService {
    contract: FoundationContract,
    lifetime: ServiceLifetime,
    reviewed_maintenance: std::sync::Mutex<
        std::collections::BTreeMap<
            String,
            slingshot_storage::maintenance::TerminalMaintenanceManifest,
        >,
    >,
}

impl DaemonService {
    /// Builds the service of one owned runtime namespace.
    #[must_use]
    pub fn new(contract: FoundationContract, ownership: DaemonOwnership) -> Self {
        Self {
            contract,
            lifetime: ServiceLifetime::Control(ownership),
            reviewed_maintenance: std::sync::Mutex::new(std::collections::BTreeMap::new()),
        }
    }

    /// Retains the complete selected runtime for every connection's lifetime.
    /// This does not publish readiness or claim operation protocol support.
    #[must_use]
    pub fn from_runtime(contract: FoundationContract, mut runtime: DurableRuntime) -> Self {
        let target = runtime.context().target();
        let identity = crate::platform_runtime::readiness::PublishedIdentity {
            author_target_identity_digest: target.author_target_identity_digest.clone(),
            daemon_runtime_contract_digest: target.daemon_runtime_contract_digest.clone(),
            retained_control_version: contract.control.version,
            selected_environment_revision: target.selected_environment_revision.clone(),
            supported_operation_versions: vec![
                slingshot_domain::daemon_runtime_contract::DaemonRuntimeContract::embedded()
                    .operation_protocol_version,
            ],
        };
        runtime.ownership_mut().identify(identity);
        Self {
            contract,
            lifetime: ServiceLifetime::Runtime(Box::new(std::sync::Mutex::new(runtime))),
            reviewed_maintenance: std::sync::Mutex::new(std::collections::BTreeMap::new()),
        }
    }

    /// Returns the foundation contract this service is bounded by.
    #[must_use]
    pub fn contract(&self) -> &FoundationContract {
        &self.contract
    }

    /// Returns the nonce of the ownership this service retains.
    #[must_use]
    pub fn readiness_nonce(&self) -> String {
        self.with_ownership(|ownership| ownership.readiness_nonce().to_owned())
    }

    fn with_ownership<T>(&self, read: impl FnOnce(&DaemonOwnership) -> T) -> T {
        match &self.lifetime {
            ServiceLifetime::Control(ownership) => read(ownership),
            ServiceLifetime::Runtime(runtime) => {
                let runtime = runtime.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
                read(runtime.ownership())
            }
        }
    }

    /// Returns exclusive ownership access before the service is shared.
    pub fn ownership_mut(&mut self) -> &mut DaemonOwnership {
        match &mut self.lifetime {
            ServiceLifetime::Control(ownership) => ownership,
            ServiceLifetime::Runtime(runtime) => {
                runtime.get_mut().unwrap_or_else(std::sync::PoisonError::into_inner).ownership_mut()
            }
        }
    }

    /// Answers one frame payload.
    ///
    /// A payload that is not a readable request, breaks a bound, or names
    /// another control version is refused before any method is read. A method
    /// outside the retained surface is refused after decoding, and neither
    /// refusal changes any state.
    #[must_use]
    pub fn answer(&self, payload: &[u8]) -> ServiceOutcome {
        if serde_json::from_slice::<serde_json::Value>(payload)
            .ok()
            .is_some_and(|value| value.get("operation_protocol_version").is_some())
        {
            return self.answer_operation(payload);
        }
        match envelope::decode_request(&self.contract, payload) {
            Err(refused) => {
                let identifier = refused.request_identifier.unwrap_or_default();
                ServiceOutcome::Respond(self.render(&identifier, Err(refused.error)))
            }
            Ok(request) => self.dispatch(&request),
        }
    }

    fn answer_operation(&self, payload: &[u8]) -> ServiceOutcome {
        let ServiceLifetime::Runtime(runtime) = &self.lifetime else {
            return self.render_operation(OperationResponse::ExecutorUnavailable);
        };
        let guard = runtime.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        let target = guard.context().target();
        let served = crate::operation_submission::ServedTarget {
            author_target_identity_digest: target.author_target_identity_digest.clone(),
            selected_environment_revision: target.selected_environment_revision.clone(),
            daemon_runtime_contract_digest: target.daemon_runtime_contract_digest.clone(),
            execution_available: true,
        };
        let version = slingshot_domain::daemon_runtime_contract::DaemonRuntimeContract::embedded()
            .operation_protocol_version as u32;
        let bound = match crate::operation_dispatch::BoundRequest::decode(
            &self.contract,
            &served,
            &[version],
            payload,
        ) {
            Ok(bound) => bound,
            Err(response) => return self.render_operation(response),
        };
        if matches!(bound.request(), OperationRequest::ArtifactRead { .. }) {
            return match bound.artifact(guard.operations(), guard.installation(), guard.artifacts())
            {
                Ok(Some(stream)) => self.render_operation_stream(stream),
                Ok(None) => self.render_operation(OperationResponse::MalformedFrame {
                    detail: "the artifact request is malformed".to_owned(),
                }),
                Err(response) => self.render_operation(response),
            };
        }
        if matches!(bound.request(), OperationRequest::MaintenanceResultRead { .. }) {
            return match bound.maintenance_read(guard.database(), guard.artifacts()) {
                Ok(Some(stream)) => self.render_operation_stream(stream),
                Ok(None) => self.render_operation(OperationResponse::MalformedFrame {
                    detail: "the maintenance request is malformed".to_owned(),
                }),
                Err(response) => self.render_operation(response),
            };
        }
        if matches!(bound.request(), OperationRequest::Wait { .. }) {
            let response = match bound.wait(guard.operations(), guard.waiters()) {
                Ok(Some(mut waiter)) => match waiter.take_ready() {
                    Some(crate::operation_wait::WaitUpdate::Progress { detail, revision }) => {
                        OperationResponse::Progress {
                            detail,
                            operation_identifier: bound
                                .request()
                                .operation_identifier()
                                .unwrap_or_default()
                                .to_owned(),
                            operation_revision: revision,
                        }
                    }
                    Some(crate::operation_wait::WaitUpdate::RecoveryRequired { revision }) => {
                        OperationResponse::Status {
                            lifecycle_state: "recovery_required".to_owned(),
                            operation_identifier: bound
                                .request()
                                .operation_identifier()
                                .unwrap_or_default()
                                .to_owned(),
                            operation_revision: revision,
                        }
                    }
                    Some(crate::operation_wait::WaitUpdate::Resumed { revision }) => {
                        OperationResponse::Status {
                            lifecycle_state: "resumed".to_owned(),
                            operation_identifier: bound
                                .request()
                                .operation_identifier()
                                .unwrap_or_default()
                                .to_owned(),
                            operation_revision: revision,
                        }
                    }
                    Some(crate::operation_wait::WaitUpdate::Terminal { revision }) => bound
                        .result(guard.operations(), guard.installation())
                        .unwrap_or(OperationResponse::Status {
                            lifecycle_state: "terminal".to_owned(),
                            operation_identifier: bound
                                .request()
                                .operation_identifier()
                                .unwrap_or_default()
                                .to_owned(),
                            operation_revision: revision,
                        }),
                    None => OperationResponse::InternalFailure {
                        detail: "the wait observer was closed before an update was available"
                            .to_owned(),
                    },
                },
                Ok(None) => OperationResponse::MalformedFrame {
                    detail: "the wait request is malformed".to_owned(),
                },
                Err(response) => response,
            };
            return self.render_operation(response);
        }
        let response = match bound.request() {
            OperationRequest::Execute { .. } => {
                let prepared = match bound.prepare_admission(guard.installation()) {
                    Ok(Some(prepared)) => prepared,
                    Ok(None) => {
                        return self.render_operation(OperationResponse::MalformedFrame {
                            detail: "the operation request is malformed".to_owned(),
                        });
                    }
                    Err(response) => return self.render_operation(response),
                };
                let active = std::collections::BTreeSet::new();
                let now = unix_milliseconds();
                prepared.persist_scheduled(guard.operations(), &active, now).unwrap_or(
                    OperationResponse::InternalFailure {
                        detail: "the operation could not be admitted".to_owned(),
                    },
                )
            }
            OperationRequest::OperationStatus { .. } => {
                bound.status(guard.operations()).unwrap_or(OperationResponse::MalformedFrame {
                    detail: "the operation request is malformed".to_owned(),
                })
            }
            OperationRequest::ListOperations { .. } => {
                bound.list(guard.operations()).unwrap_or(OperationResponse::MalformedFrame {
                    detail: "the operation request is malformed".to_owned(),
                })
            }
            OperationRequest::Result { .. } => bound
                .result(guard.operations(), guard.installation())
                .unwrap_or(OperationResponse::MalformedFrame {
                    detail: "the operation request is malformed".to_owned(),
                }),
            OperationRequest::ResumeOperationRecovery { .. } => {
                bound.resume(guard.operations(), unix_milliseconds()).ok().flatten().unwrap_or(
                    OperationResponse::InternalFailure {
                        detail: "the recovery request could not be applied".to_owned(),
                    },
                )
            }
            OperationRequest::MaintenanceResultMetadata { .. } => bound
                .maintenance_metadata(guard.database())
                .unwrap_or(OperationResponse::MalformedFrame {
                    detail: "the maintenance request is malformed".to_owned(),
                }),
            OperationRequest::TerminalMaintenancePreview {
                before_unix_milliseconds,
                maximum_operations,
                ..
            } => {
                let allowed = slingshot_domain::daemon_runtime_contract::DaemonRuntimeContract::embedded()
                    .limit("maximum_terminal_maintenance_operations");
                if *maximum_operations == 0 || u64::from(*maximum_operations) > allowed {
                    return self.render_operation(OperationResponse::MalformedFrame {
                        detail: "the maintenance preview bound is outside the runtime limit".to_owned(),
                    });
                }
                match slingshot_storage::maintenance::preview(
                    guard.database(),
                    &target.author_target_identity_digest,
                    *before_unix_milliseconds,
                    u64::from(*maximum_operations),
                ) {
                    Ok(manifest) => {
                        let digest = manifest.digest();
                        let rendered = serde_json::to_value(manifest.clone())
                            .unwrap_or(serde_json::Value::Null);
                        self.reviewed_maintenance
                            .lock()
                            .unwrap_or_else(std::sync::PoisonError::into_inner)
                            .insert(digest.clone(), manifest);
                        OperationResponse::MaintenancePreview {
                            manifest: rendered,
                            reviewed_manifest_digest: digest,
                        }
                    }
                    Err(_) => OperationResponse::InternalFailure {
                        detail: "the maintenance preview could not be read".to_owned(),
                    },
                }
            }
            OperationRequest::TerminalMaintenanceApply { reviewed_manifest_digest, .. } => {
                let reviewed = self
                    .reviewed_maintenance
                    .lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner)
                    .get(reviewed_manifest_digest)
                    .cloned();
                let Some(reviewed) = reviewed else {
                    return self.render_operation(OperationResponse::InternalFailure { detail: "the reviewed maintenance manifest is unavailable; request a new preview".to_owned() });
                };
                match slingshot_storage::maintenance::apply(
                    guard.database(),
                    &reviewed,
                    unix_milliseconds(),
                ) {
                    Ok(slingshot_storage::maintenance::ApplyOutcome::Applied(receipt)) => {
                        let result =
                            slingshot_storage::maintenance_results::result_identifiers_for_receipt(
                                guard.database(),
                                &target.author_target_identity_digest,
                                &receipt.application_receipt_identifier,
                            )
                            .ok()
                            .and_then(|mut values| {
                                if values.len() == 1 { values.pop() } else { None }
                            });
                        self.reviewed_maintenance
                            .lock()
                            .unwrap_or_else(std::sync::PoisonError::into_inner)
                            .remove(reviewed_manifest_digest);
                        OperationResponse::MaintenanceApplied {
                            application_receipt_identifier: receipt.application_receipt_identifier,
                            maintenance_result_identifier: result
                                .unwrap_or_else(|| reviewed_manifest_digest.clone()),
                        }
                    }
                    Ok(slingshot_storage::maintenance::ApplyOutcome::Replayed(receipt)) => {
                        let result =
                            slingshot_storage::maintenance_results::result_identifiers_for_receipt(
                                guard.database(),
                                &target.author_target_identity_digest,
                                &receipt.application_receipt_identifier,
                            )
                            .ok()
                            .and_then(|mut values| {
                                if values.len() == 1 { values.pop() } else { None }
                            });
                        self.reviewed_maintenance
                            .lock()
                            .unwrap_or_else(std::sync::PoisonError::into_inner)
                            .remove(reviewed_manifest_digest);
                        OperationResponse::MaintenanceReplayed {
                            application_receipt_identifier: receipt.application_receipt_identifier,
                            maintenance_result_identifier: result
                                .unwrap_or_else(|| reviewed_manifest_digest.clone()),
                        }
                    }
                    Err(_) => OperationResponse::InternalFailure {
                        detail:
                            "the reviewed maintenance manifest no longer matches retained state"
                                .to_owned(),
                    },
                }
            }
            OperationRequest::Wait { .. }
            | OperationRequest::ArtifactRead { .. }
            | OperationRequest::MaintenanceResultRead { .. } => {
                OperationResponse::InternalFailure {
                    detail:
                        "this operation method requires its streaming or maintenance coordinator"
                            .to_owned(),
                }
            }
        };
        self.render_operation(response)
    }

    /// Holds a bound wait observer until the next durable update or caller/root
    /// cancellation. The runtime mutex is released before awaiting; the waiter
    /// registry, not a mutex guard, owns the observation lifetime.
    pub async fn wait_for_update(
        &self,
        payload: &[u8],
        cancellation: &CancellationToken,
    ) -> Result<OperationResponse, OperationResponse> {
        let ServiceLifetime::Runtime(runtime) = &self.lifetime else {
            return Err(OperationResponse::ExecutorUnavailable);
        };
        let (mut waiter, operation) = {
            let guard = runtime.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
            let target = guard.context().target();
            let served = crate::operation_submission::ServedTarget {
                author_target_identity_digest: target.author_target_identity_digest.clone(),
                selected_environment_revision: target.selected_environment_revision.clone(),
                daemon_runtime_contract_digest: target.daemon_runtime_contract_digest.clone(),
                execution_available: true,
            };
            let version = slingshot_domain::daemon_runtime_contract::DaemonRuntimeContract::embedded().operation_protocol_version as u32;
            let bound = crate::operation_dispatch::BoundRequest::decode(&self.contract, &served, &[version], payload)?;
            let operation = bound.request().operation_identifier().unwrap_or_default().to_owned();
            let waiter = bound.wait(guard.operations(), guard.waiters())?
                .ok_or(OperationResponse::MalformedFrame { detail: "the wait request is malformed".to_owned() })?;
            (waiter, operation)
        };
        let update = waiter.next(cancellation).await.ok_or(OperationResponse::InternalFailure { detail: "the wait observer was cancelled before an update".to_owned() })?;
        Ok(match update {
            crate::operation_wait::WaitUpdate::Progress { detail, revision } => OperationResponse::Progress { detail, operation_identifier: operation, operation_revision: revision },
            crate::operation_wait::WaitUpdate::RecoveryRequired { revision } => OperationResponse::Status { lifecycle_state: "recovery_required".to_owned(), operation_identifier: operation, operation_revision: revision },
            crate::operation_wait::WaitUpdate::Resumed { revision } => OperationResponse::Status { lifecycle_state: "resumed".to_owned(), operation_identifier: operation, operation_revision: revision },
            crate::operation_wait::WaitUpdate::Terminal { revision } => OperationResponse::Status { lifecycle_state: "terminal".to_owned(), operation_identifier: operation, operation_revision: revision },
        })
    }

    fn render_operation(&self, response: OperationResponse) -> ServiceOutcome {
        let payload = serde_json::to_vec(&response).unwrap_or_else(|_| {
            serde_json::to_vec(&OperationResponse::InternalFailure {
                detail: "the operation response could not be rendered".to_owned(),
            })
            .expect("static operation refusal renders")
        });
        let frame = slingshot_local_protocol::framing::render(&self.contract.framing, &payload)
            .unwrap_or_else(|_| Vec::new());
        ServiceOutcome::Respond(frame)
    }

    fn render_operation_stream<I>(&self, stream: I) -> ServiceOutcome
    where
        I: IntoIterator<Item = OperationResponse>,
    {
        let frames = stream
            .into_iter()
            .map(|response| {
                let payload = serde_json::to_vec(&response).unwrap_or_else(|_| {
                    serde_json::to_vec(&OperationResponse::InternalFailure {
                        detail: "the operation response could not be rendered".to_owned(),
                    })
                    .expect("static refusal")
                });
                slingshot_local_protocol::framing::render(&self.contract.framing, &payload)
                    .unwrap_or_default()
            })
            .collect();
        ServiceOutcome::RespondMany(frames)
    }

    /// Dispatches one decoded request to the retained control surface.
    fn dispatch(&self, request: &ControlRequest) -> ServiceOutcome {
        match request.method.as_str() {
            HELLO_METHOD => self.dispatch_hello(request),
            PING_METHOD => ServiceOutcome::Respond(
                self.render(&request.request_identifier, Ok(self.ping_result())),
            ),
            STOP_METHOD => self.dispatch_stop(request),
            other => {
                let error = ControlError::new(
                    METHOD_NOT_FOUND_CODE,
                    format!("the retained control surface has no method {other}"),
                );
                ServiceOutcome::Respond(self.render(&request.request_identifier, Err(error)))
            }
        }
    }

    /// A greeting describes retained startup facts, never client-supplied facts.
    fn dispatch_hello(&self, request: &ControlRequest) -> ServiceOutcome {
        if !request.arguments.as_object().is_some_and(serde_json::Map::is_empty) {
            return ServiceOutcome::Respond(self.render(
                &request.request_identifier,
                Err(ControlError::new(
                    envelope::MALFORMED_REQUEST_CODE,
                    "a greeting carries an empty arguments object",
                )),
            ));
        }
        let hello = self.with_ownership(|ownership| {
            ownership.identity().map(|identity| HelloResult {
                author_target_identity_digest: identity.author_target_identity_digest.clone(),
                daemon_runtime_contract_digest: identity.daemon_runtime_contract_digest.clone(),
                product_version: PRODUCT_VERSION.to_owned(),
                readiness_nonce: ownership.readiness_nonce().to_owned(),
                runtime_namespace: ownership.namespace().display(),
                selected_environment_revision: identity.selected_environment_revision.clone(),
                supported_operation_protocol_versions: vec![
                    slingshot_domain::daemon_runtime_contract::DaemonRuntimeContract::embedded()
                        .operation_protocol_version as u32,
                ],
            })
        });
        match hello {
            Some(hello) => {
                ServiceOutcome::Respond(self.render_result(&request.request_identifier, &hello))
            }
            None => ServiceOutcome::Respond(self.render(
                &request.request_identifier,
                Err(ControlError::new(
                    RUNTIME_UNAVAILABLE_CODE,
                    "the daemon has no established selected runtime",
                )),
            )),
        }
    }

    /// Answers one cooperative stop, which only the live nonce authorizes.
    fn dispatch_stop(&self, request: &ControlRequest) -> ServiceOutcome {
        let arguments: Result<StopArguments, _> = serde_json::from_value(request.arguments.clone());
        let Ok(arguments) = arguments else {
            let error = ControlError::new(
                envelope::MALFORMED_REQUEST_CODE,
                "a cooperative stop carries the live readiness nonce",
            );
            return ServiceOutcome::Respond(self.render(&request.request_identifier, Err(error)));
        };
        if !self
            .with_ownership(|ownership| ownership.stop_is_authorized(&arguments.readiness_nonce))
        {
            let refusal = ping::stale_instance_refusal();
            return ServiceOutcome::Respond(self.render(&request.request_identifier, Err(refusal)));
        }
        let acknowledgement =
            self.render_result(&request.request_identifier, &StopResult { acknowledged: true });
        ServiceOutcome::RespondThenStop(acknowledgement)
    }

    /// Builds the result of one retained ping.
    fn ping_result(&self) -> PingResult {
        self.with_ownership(|ownership| PingResult {
            product_version: PRODUCT_VERSION.to_owned(),
            process_identifier: std::process::id(),
            profile: ownership.namespace().profile().to_owned(),
            environment: ownership.namespace().environment().to_owned(),
            readiness_nonce: ownership.readiness_nonce().to_owned(),
            supported_operation_protocol_versions: vec![
                slingshot_domain::daemon_runtime_contract::DaemonRuntimeContract::embedded()
                    .operation_protocol_version as u32,
            ],
        })
    }

    /// Renders one served result as a frame.
    fn render_result(&self, request_identifier: &str, result: &impl serde::Serialize) -> Vec<u8> {
        let response = ControlResponse::served(&self.contract, request_identifier, result)
            .expect("a retained result always renders");
        response
            .render_frame(&self.contract)
            .expect("a retained response is within the frame limit")
    }

    /// Renders one served result or structured refusal as a frame.
    fn render(
        &self,
        request_identifier: &str,
        outcome: Result<PingResult, ControlError>,
    ) -> Vec<u8> {
        match outcome {
            Ok(result) => self.render_result(request_identifier, &result),
            Err(error) => {
                let response = ControlResponse::refused(&self.contract, request_identifier, error);
                response
                    .render_frame(&self.contract)
                    .expect("a retained refusal is within the frame limit")
            }
        }
    }
}

fn unix_milliseconds() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .ok()
        .and_then(|duration| u64::try_from(duration.as_millis()).ok())
        .unwrap_or(u64::MAX)
}

impl Drop for DaemonService {
    fn drop(&mut self) {
        // Stop advertising before SQLite, transport and the lock are released.
        // Ownership's drop remains a second best-effort cleanup on failure.
        let _ = self.ownership_mut().withdraw_readiness();
    }
}
