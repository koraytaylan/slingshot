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
    ownership: DaemonOwnership,
}

impl DaemonService {
    /// Builds the service of one owned runtime namespace.
    #[must_use]
    pub fn new(contract: FoundationContract, ownership: DaemonOwnership) -> Self {
        Self { contract, ownership }
    }

    /// Returns the foundation contract this service is bounded by.
    #[must_use]
    pub fn contract(&self) -> &FoundationContract {
        &self.contract
    }

    /// Returns the ownership this service answers for.
    #[must_use]
    pub fn ownership(&self) -> &DaemonOwnership {
        &self.ownership
    }

    /// Returns the ownership this service answers for, for withdrawal.
    pub fn ownership_mut(&mut self) -> &mut DaemonOwnership {
        &mut self.ownership
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
        if !self.ownership.stop_is_authorized(&arguments.readiness_nonce) {
            let refusal = ping::stale_instance_refusal();
            return ServiceOutcome::Respond(self.render(&request.request_identifier, Err(refusal)));
        }
        let acknowledgement =
            self.render_result(&request.request_identifier, &StopResult { acknowledged: true });
        ServiceOutcome::RespondThenStop(acknowledgement)
    }

    /// Builds the result of one retained ping.
    fn ping_result(&self) -> PingResult {
        PingResult {
            product_version: PRODUCT_VERSION.to_owned(),
            process_identifier: std::process::id(),
            profile: self.ownership.namespace().profile().to_owned(),
            environment: self.ownership.namespace().environment().to_owned(),
            readiness_nonce: self.ownership.readiness_nonce().to_owned(),
            supported_operation_protocol_versions: Vec::new(),
        }
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
