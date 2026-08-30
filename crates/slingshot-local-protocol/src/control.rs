//! Inspecting and stopping a daemon you cannot otherwise talk to.
//!
//! The point of a stable control surface is that it keeps working when nothing
//! else does. A client built against another operation-protocol version, or
//! against another daemon-runtime contract, can still greet the daemon, read its
//! status, ping it, and stop it by name. Without that, an incompatible client
//! would be left with a running daemon it can neither use nor shut down, and the
//! only remaining option would be to find the process and signal it - which is
//! exactly the thing every other rule here exists to prevent.
//!
//! So this surface is deliberately independent of operation-protocol evolution.
//! It reuses Plan 0001's retained `daemon.ping` and `daemon.stop` spellings
//! unchanged and adds two more, and an incompatibility is answered with a
//! structured response rather than by closing the connection.
//!
//! # Two rules that make the server's job unambiguous
//!
//! Hello is the first complete request on a connection. That is not ceremony:
//! it gives the server exactly one pre-hello deadline boundary, so it never has
//! to decide which timeout applies to a connection that has said nothing
//! interpretable yet.
//!
//! Stop is authorized by the exact nonce the live instance published, and by
//! nothing else. A namespace is routing information, a target is a name, and a
//! process identifier is a diagnostic; none of them identifies *this* instance,
//! and a caller holding a stale nonce must not be able to stop the replacement
//! that took over after the one it was talking to went away.

use serde::{Deserialize, Serialize};

use crate::envelope::ControlError;
use crate::foundation_contract::FoundationContract;
use crate::ping::{PING_METHOD, STOP_METHOD};

/// Method name of the greeting every connection begins with.
pub const HELLO_METHOD: &str = "daemon.hello";

/// Method name of the bounded status report.
pub const STATUS_METHOD: &str = "daemon.status";

/// Every method this surface serves, whatever the operation protocol says.
pub const RETAINED_METHODS: &[&str] = &[HELLO_METHOD, PING_METHOD, STATUS_METHOD, STOP_METHOD];

/// Error code returned when a connection speaks before greeting.
pub const HELLO_REQUIRED_CODE: &str = "hello_required";

/// Error code returned when no operation-protocol version is shared.
pub const INCOMPATIBLE_OPERATION_PROTOCOL_CODE: &str = "incompatible_operation_protocol";

/// Error code returned when the daemon-runtime contract digests differ.
pub const INCOMPATIBLE_RUNTIME_CONTRACT_CODE: &str = "incompatible_daemon_runtime_contract";

/// Characters a rendered digest occupies.
const DIGEST_CHARACTERS: usize = 64;

/// What the daemon says about itself when a connection opens.
///
/// The target identity is the opaque digest and nothing else. A principal
/// member, a user name, or an organization identifier would be the same fact in
/// a form somebody could read, and this response is sent to any client that can
/// reach the socket.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HelloResult {
    /// Opaque digest of the target this daemon serves.
    pub author_target_identity_digest: String,
    /// Digest of the runtime contract this daemon was built against.
    pub daemon_runtime_contract_digest: String,
    /// Version of the product the daemon was built from.
    pub product_version: String,
    /// Live readiness nonce, rendered in lowercase hexadecimal.
    pub readiness_nonce: String,
    /// Namespace this daemon owns.
    pub runtime_namespace: String,
    /// Exact revision of the environment selection it resolved.
    pub selected_environment_revision: String,
    /// Operation-protocol versions it serves, which may share none with a caller.
    pub supported_operation_protocol_versions: Vec<u32>,
}

/// What the daemon reports when asked how it is.
///
/// Bounded on purpose: a status report that grew with the work a daemon had
/// done would eventually be the one response an incompatible client could not
/// receive.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DaemonStatusResult {
    /// Whether the daemon is admitting new work.
    pub admitting: bool,
    /// Operations accepted and not yet finished.
    pub in_flight_operations: u32,
    /// Operations waiting to start.
    pub pending_operations: u32,
    /// Seconds since this instance became ready.
    pub uptime_seconds: u64,
}

/// Reason a control request could not be served.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum ControlFailure {
    /// A request arrived before the connection greeted.
    #[error("a connection greets before it asks for anything else")]
    HelloRequired,
    /// A greeting arrived on a connection that had already greeted.
    #[error("a connection greets once")]
    AlreadyGreeted,
    /// A digest is not sixty-four lowercase hexadecimal characters.
    #[error("a digest is exactly {DIGEST_CHARACTERS} lowercase hexadecimal characters")]
    DigestNotCanonical,
    /// A daemon advertised more versions than the contract allows.
    #[error("a daemon advertises at most the contract's collection bound of versions")]
    TooManyVersions,
}

/// Where one control connection is in its conversation.
///
/// One bit of state, and the reason it exists is the server's deadlines rather
/// than politeness: a connection that has greeted is one whose next request can
/// be read under the ordinary timeout, and one that has not is still inside the
/// pre-hello boundary.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct ControlConversation {
    /// Whether the greeting has been served.
    greeted: bool,
}

impl ControlConversation {
    /// Returns a conversation that has not greeted.
    #[must_use]
    pub fn new() -> Self {
        Self { greeted: false }
    }

    /// Returns whether the greeting has been served.
    #[must_use]
    pub fn has_greeted(self) -> bool {
        self.greeted
    }

    /// Admits one method, recording the greeting when it arrives.
    ///
    /// # Errors
    ///
    /// Returns [`ControlFailure::HelloRequired`] for anything before the
    /// greeting and [`ControlFailure::AlreadyGreeted`] for a second one.
    pub fn admit(&mut self, method: &str) -> Result<(), ControlFailure> {
        match (method == HELLO_METHOD, self.greeted) {
            (true, false) => {
                self.greeted = true;
                Ok(())
            }
            (true, true) => Err(ControlFailure::AlreadyGreeted),
            (false, true) => Ok(()),
            (false, false) => Err(ControlFailure::HelloRequired),
        }
    }
}

/// Returns the refusal a request receives before the greeting.
#[must_use]
pub fn hello_required_refusal() -> ControlError {
    ControlError::new(
        HELLO_REQUIRED_CODE,
        "a control connection greets before it asks for anything else",
    )
}

/// What a caller and a daemon can do together.
///
/// An incompatibility is a fact about the pair, not a fault of either, and it
/// leaves the retained surface working. That is the whole point: an
/// incompatible client can still see what it is talking to and stop it by name.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OperationCompatibility {
    /// Both sides agree, and versioned operations may be sent.
    Compatible {
        /// Version they agreed on.
        version: u32,
    },
    /// No operation-protocol version is shared.
    NoSharedVersion,
    /// A version is shared and the runtime contracts differ.
    RuntimeContractDiffers,
}

impl OperationCompatibility {
    /// Returns whether versioned operations may be sent.
    #[must_use]
    pub fn permits_operations(self) -> bool {
        matches!(self, Self::Compatible { .. })
    }

    /// Returns whether the retained surface stays usable.
    ///
    /// Always. There is no incompatibility that takes inspection or an explicit
    /// stop away, because a daemon nobody can stop is worse than a daemon
    /// nobody can use.
    #[must_use]
    pub fn permits_retained_control(self) -> bool {
        true
    }

    /// Returns the structured refusal this incompatibility answers with.
    #[must_use]
    pub fn refusal(self) -> Option<ControlError> {
        match self {
            Self::Compatible { .. } => None,
            Self::NoSharedVersion => Some(ControlError::new(
                INCOMPATIBLE_OPERATION_PROTOCOL_CODE,
                "the daemon serves no operation-protocol version this client speaks; \
                 inspect it or stop it explicitly by its current nonce",
            )),
            Self::RuntimeContractDiffers => Some(ControlError::new(
                INCOMPATIBLE_RUNTIME_CONTRACT_CODE,
                "the daemon was built against another runtime contract; \
                 inspect it or stop it explicitly by its current nonce",
            )),
        }
    }
}

/// Returns what a caller and the daemon `hello` describes can do together.
///
/// The caller's versions are consulted first, because a shared version is what
/// makes the digest comparison meaningful at all. Nothing here guesses a
/// version, downgrades silently, or routes anywhere else.
#[must_use]
pub fn operation_compatibility(
    hello: &HelloResult,
    caller_versions: &[u32],
    caller_digest: &str,
) -> OperationCompatibility {
    let shared = hello
        .supported_operation_protocol_versions
        .iter()
        .filter(|version| caller_versions.contains(version))
        .max();
    match shared {
        None => OperationCompatibility::NoSharedVersion,
        Some(version) if hello.daemon_runtime_contract_digest == caller_digest => {
            OperationCompatibility::Compatible { version: *version }
        }
        Some(_) => OperationCompatibility::RuntimeContractDiffers,
    }
}

impl HelloResult {
    /// Requires this greeting to be one a daemon could truthfully send.
    ///
    /// # Errors
    ///
    /// Returns [`ControlFailure::DigestNotCanonical`] for a digest that is not
    /// sixty-four lowercase hexadecimal characters and
    /// [`ControlFailure::TooManyVersions`] above the contract's collection
    /// bound.
    pub fn require_well_formed(&self, contract: &FoundationContract) -> Result<(), ControlFailure> {
        for digest in [&self.author_target_identity_digest, &self.daemon_runtime_contract_digest] {
            let canonical = digest.len() == DIGEST_CHARACTERS
                && digest.chars().all(|character| {
                    character.is_ascii_hexdigit() && !character.is_ascii_uppercase()
                });
            if !canonical {
                return Err(ControlFailure::DigestNotCanonical);
            }
        }
        let advertised =
            u32::try_from(self.supported_operation_protocol_versions.len()).unwrap_or(u32::MAX);
        if advertised > contract.framing.maximum_collection_items {
            return Err(ControlFailure::TooManyVersions);
        }
        Ok(())
    }
}
