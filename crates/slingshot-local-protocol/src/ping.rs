//! The retained ping and nonce-bound stop methods.
//!
//! Ping is an existing-owner probe: it reports the product version, the
//! diagnostic process identifier, the target the daemon owns, the live
//! readiness nonce, and the operation-protocol versions the daemon supports.
//! Stop carries the exact live readiness nonce; a nonce any prior instance
//! published is refused without any side effect, so a stale caller can never
//! stop a replacement.

use serde::{Deserialize, Serialize};

use crate::envelope::{ControlError, STALE_DAEMON_INSTANCE_CODE};
use crate::foundation_contract::FoundationContract;

/// Method name of the retained existing-owner probe.
pub const PING_METHOD: &str = "daemon.ping";

/// Method name of the retained nonce-bound cooperative stop.
pub const STOP_METHOD: &str = "daemon.stop";

/// Result of one retained ping.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PingResult {
    /// Version of the product the daemon was built from.
    pub product_version: String,
    /// Process identifier of the daemon, as a diagnostic and never as authority.
    pub process_identifier: u32,
    /// Profile the daemon owns.
    pub profile: String,
    /// Environment the daemon owns.
    pub environment: String,
    /// Live readiness nonce, rendered in lowercase hexadecimal.
    pub readiness_nonce: String,
    /// Operation-protocol versions the daemon serves beyond the control surface.
    pub supported_operation_protocol_versions: Vec<u32>,
}

/// Arguments of one retained cooperative stop.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StopArguments {
    /// Readiness nonce the caller believes the live daemon published.
    pub readiness_nonce: String,
}

/// Result of one accepted cooperative stop.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StopResult {
    /// Whether the daemon acknowledged the stop before shutting down.
    pub acknowledged: bool,
}

/// Reports whether a rendered nonce has the exact contract shape.
#[must_use]
pub fn nonce_is_well_formed(contract: &FoundationContract, nonce: &str) -> bool {
    nonce.len() == contract.namespace.readiness_nonce_rendered_bytes as usize
        && nonce.bytes().all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

/// Returns the refusal a stop receives when its nonce is not the live one.
#[must_use]
pub fn stale_instance_refusal() -> ControlError {
    ControlError::new(
        STALE_DAEMON_INSTANCE_CODE,
        "the supplied readiness nonce belongs to an instance that is no longer running",
    )
}

/// Compares a supplied nonce with the live one in constant time over its bytes.
///
/// A mismatch is authoritative: the daemon changes no state and reports
/// [`stale_instance_refusal`].
#[must_use]
pub fn stop_is_authorized(live_nonce: &str, supplied_nonce: &str) -> bool {
    if live_nonce.len() != supplied_nonce.len() {
        return false;
    }
    live_nonce
        .bytes()
        .zip(supplied_nonce.bytes())
        .fold(0_u8, |difference, (live, supplied)| difference | (live ^ supplied))
        == 0
}
