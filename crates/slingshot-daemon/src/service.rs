//! The target-scoped application service.
//!
//! The service turns one frame payload into one response frame. It holds the
//! ownership of its namespace, so it can answer with the live readiness nonce
//! and decide whether a stop is authorized, and it never dispatches a method
//! for a request whose control version it does not speak.

use slingshot_local_protocol::envelope::{
    self, ControlError, ControlRequest, ControlResponse, METHOD_NOT_FOUND_CODE,
};
use slingshot_local_protocol::foundation_contract::FoundationContract;
use slingshot_local_protocol::ping::{
    self, PING_METHOD, PingResult, STOP_METHOD, StopArguments, StopResult,
};

use crate::ownership::DaemonOwnership;
use crate::runtime_builder::DurableRuntime;

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
}

impl ServiceOutcome {
    /// Returns the frame this outcome carries.
    #[must_use]
    pub fn frame(&self) -> &[u8] {
        match self {
            Self::Respond(frame) | Self::RespondThenStop(frame) => frame,
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
}

impl DaemonService {
    /// Builds the service of one owned runtime namespace.
    #[must_use]
    pub fn new(contract: FoundationContract, ownership: DaemonOwnership) -> Self {
        Self { contract, lifetime: ServiceLifetime::Control(ownership) }
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
            // Dispatch is still control-only; never advertise an uninstalled surface.
            supported_operation_versions: Vec::new(),
        };
        runtime.ownership_mut().identify(identity);
        Self {
            contract,
            lifetime: ServiceLifetime::Runtime(Box::new(std::sync::Mutex::new(runtime))),
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
        match envelope::decode_request(&self.contract, payload) {
            Err(refused) => {
                let identifier = refused.request_identifier.unwrap_or_default();
                ServiceOutcome::Respond(self.render(&identifier, Err(refused.error)))
            }
            Ok(request) => self.dispatch(&request),
        }
    }

    /// Dispatches one decoded request to the retained control surface.
    fn dispatch(&self, request: &ControlRequest) -> ServiceOutcome {
        match request.method.as_str() {
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
            supported_operation_protocol_versions: Vec::new(),
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

impl Drop for DaemonService {
    fn drop(&mut self) {
        // Stop advertising before SQLite, transport and the lock are released.
        // Ownership's drop remains a second best-effort cleanup on failure.
        let _ = self.ownership_mut().withdraw_readiness();
    }
}
