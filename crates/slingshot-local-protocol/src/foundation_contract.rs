//! The canonical Plan 0001 limit source.
//!
//! `support/foundation-contract.toml` is embedded into this crate and parsed
//! through one typed interface. Product code, fixtures, scripts, and tests read
//! every wire, namespace, endpoint, startup, and process-harness limit from
//! [`FoundationContract`], so a limit exists exactly once in the repository.

use std::time::Duration;

use serde::Deserialize;

/// Bytes of the committed manifest, embedded at compile time.
const EMBEDDED_MANIFEST: &str = include_str!("../../../support/foundation-contract.toml");

/// Format identifier the manifest must declare.
pub const CONTRACT_FORMAT: &str = "slingshot.foundation-contract/1";

/// Reason the foundation contract could not be read.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ContractFailure {
    /// The manifest bytes are not a valid contract document.
    #[error("the foundation contract could not be read: {0}")]
    Unreadable(String),
    /// The manifest declares a format this build does not implement.
    #[error("the foundation contract declares the format {0}")]
    UnsupportedFormat(String),
    /// A field that must be positive is zero.
    #[error("the foundation contract field {0} must be positive")]
    NotPositive(&'static str),
    /// A rendered length does not match the byte length it renders.
    #[error("the foundation contract field {rendered} does not render {raw}")]
    RenderedLengthMismatch {
        /// Field holding the rendered length.
        rendered: &'static str,
        /// Field holding the raw byte length.
        raw: &'static str,
    },
}

/// Version of the retained control surface.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "kebab-case", deny_unknown_fields)]
pub struct ControlLimits {
    /// Control version every retained request and response declares.
    pub version: u32,
}

/// Bounds of one framed control message.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "kebab-case", deny_unknown_fields)]
pub struct FramingLimits {
    /// Width of the unsigned payload length prefix, in bytes.
    pub length_prefix_bytes: u32,
    /// Largest payload one frame may carry, in bytes.
    pub maximum_payload_bytes: u32,
    /// Deepest container nesting a payload may reach.
    pub maximum_nesting_depth: u32,
    /// Most entries any one array or object in a payload may hold.
    pub maximum_collection_items: u32,
}

/// Bounds of the names a control message carries.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "kebab-case", deny_unknown_fields)]
pub struct NameLimits {
    /// Longest caller request identifier, in bytes.
    pub request_identifier_bytes: u32,
    /// Longest method name, in bytes.
    pub method_bytes: u32,
    /// Longest structured error code, in bytes.
    pub error_code_bytes: u32,
    /// Longest structured error message, in bytes.
    pub error_message_bytes: u32,
    /// Longest profile name, in bytes.
    pub profile_bytes: u32,
    /// Longest environment name, in bytes.
    pub environment_bytes: u32,
}

/// Bounds of one runtime namespace and its endpoint.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "kebab-case", deny_unknown_fields)]
pub struct NamespaceLimits {
    /// Length of the namespace digest, in bytes.
    pub digest_bytes: u32,
    /// Length of the rendered namespace digest, in bytes.
    pub digest_rendered_bytes: u32,
    /// Length of the readiness nonce, in bytes.
    pub readiness_nonce_bytes: u32,
    /// Length of the rendered readiness nonce, in bytes.
    pub readiness_nonce_rendered_bytes: u32,
    /// Largest readiness record, in bytes.
    pub readiness_record_bytes: u32,
    /// Longest Unix domain socket address, in bytes.
    pub unix_socket_address_bytes: u32,
    /// Longest Windows named-pipe name, in UTF-16 code units.
    pub windows_named_pipe_name_code_units: u32,
    /// Flag every Windows named-pipe server creation must carry.
    pub windows_named_pipe_flag: String,
}

/// Capacity and deadlines of the local server.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "kebab-case", deny_unknown_fields)]
pub struct ServerLimits {
    /// Most connections the server serves at once.
    pub connection_capacity: u32,
    /// Deadline for the first control frame of a connection.
    pub initial_control_frame_milliseconds: u64,
    /// Deadline between two reads while a frame is incomplete.
    pub incomplete_frame_read_idle_milliseconds: u64,
    /// Deadline for completing one frame, whatever its progress.
    pub absolute_frame_completion_milliseconds: u64,
    /// Deadline for writing one response.
    pub response_write_milliseconds: u64,
}

/// Deadlines of the explicit daemon start protocol.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "kebab-case", deny_unknown_fields)]
pub struct StartupLimits {
    /// Deadline for the whole explicit start, including retries.
    pub explicit_start_total_milliseconds: u64,
    /// Longest delay between two start retries.
    pub start_retry_maximum_delay_milliseconds: u64,
}

/// Deadlines of cooperative and supervised shutdown.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "kebab-case", deny_unknown_fields)]
pub struct ShutdownLimits {
    /// Deadline for a nonce-bound cooperative stop.
    pub cooperative_stop_milliseconds: u64,
    /// Deadline for one supervised terminate-and-wait disposition.
    pub supervised_termination_and_wait_milliseconds: u64,
}

/// Values the multi-process test harness is bounded by.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "kebab-case", deny_unknown_fields)]
pub struct ProcessHarnessLimits {
    /// Slack added to a real deadline before a harness gives up.
    pub scheduling_tolerance_milliseconds: u64,
    /// Number of clients the walking proof releases behind one barrier.
    pub walking_start_client_count: u32,
}

/// Every Plan 0001 limit, read from the committed manifest.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "kebab-case", deny_unknown_fields)]
pub struct FoundationContract {
    /// Format identifier of the manifest.
    pub format: String,
    /// Retained control surface version.
    pub control: ControlLimits,
    /// Frame bounds.
    pub framing: FramingLimits,
    /// Name bounds.
    pub names: NameLimits,
    /// Runtime namespace bounds.
    pub namespace: NamespaceLimits,
    /// Local server capacity and deadlines.
    pub server: ServerLimits,
    /// Explicit start deadlines.
    pub startup: StartupLimits,
    /// Shutdown deadlines.
    pub shutdown: ShutdownLimits,
    /// Process harness values.
    pub process_harness: ProcessHarnessLimits,
}

/// Number of rendered characters one byte occupies in lowercase hexadecimal.
const RENDERED_BYTES_PER_RAW_BYTE: u32 = 2;

impl FoundationContract {
    /// Returns the contract embedded in this build.
    ///
    /// # Panics
    ///
    /// Panics when the embedded manifest is not a valid contract, which is a
    /// build-time defect rather than a runtime condition.
    #[must_use]
    pub fn embedded() -> Self {
        Self::parse(EMBEDDED_MANIFEST).expect("the embedded foundation contract is valid")
    }

    /// Returns the exact manifest bytes embedded in this build.
    #[must_use]
    pub fn embedded_manifest() -> &'static str {
        EMBEDDED_MANIFEST
    }

    /// Reads a contract from manifest text.
    ///
    /// # Errors
    ///
    /// Returns [`ContractFailure::Unreadable`] when the text is not a contract
    /// document, including when a field is missing, repeated, negative,
    /// overflowed, differently encoded, or outside the closed schema;
    /// [`ContractFailure::UnsupportedFormat`] for another format identifier;
    /// [`ContractFailure::NotPositive`] when a positive field is zero; and
    /// [`ContractFailure::RenderedLengthMismatch`] when a rendered length does
    /// not match the bytes it renders.
    pub fn parse(manifest: &str) -> Result<Self, ContractFailure> {
        let contract: Self = toml::from_str(manifest)
            .map_err(|failure| ContractFailure::Unreadable(failure.to_string()))?;
        if contract.format != CONTRACT_FORMAT {
            return Err(ContractFailure::UnsupportedFormat(contract.format));
        }
        contract.evaluate_positive()?;
        contract.evaluate_rendered_lengths()?;
        Ok(contract)
    }

    /// Returns every field that must be positive, paired with its name.
    fn positive_fields(&self) -> Vec<(&'static str, u64)> {
        vec![
            ("control.version", u64::from(self.control.version)),
            ("framing.length-prefix-bytes", u64::from(self.framing.length_prefix_bytes)),
            ("framing.maximum-payload-bytes", u64::from(self.framing.maximum_payload_bytes)),
            ("framing.maximum-nesting-depth", u64::from(self.framing.maximum_nesting_depth)),
            ("framing.maximum-collection-items", u64::from(self.framing.maximum_collection_items)),
            ("names.request-identifier-bytes", u64::from(self.names.request_identifier_bytes)),
            ("names.method-bytes", u64::from(self.names.method_bytes)),
            ("names.error-code-bytes", u64::from(self.names.error_code_bytes)),
            ("names.error-message-bytes", u64::from(self.names.error_message_bytes)),
            ("names.profile-bytes", u64::from(self.names.profile_bytes)),
            ("names.environment-bytes", u64::from(self.names.environment_bytes)),
            ("namespace.digest-bytes", u64::from(self.namespace.digest_bytes)),
            ("namespace.readiness-nonce-bytes", u64::from(self.namespace.readiness_nonce_bytes)),
            ("namespace.readiness-record-bytes", u64::from(self.namespace.readiness_record_bytes)),
            (
                "namespace.unix-socket-address-bytes",
                u64::from(self.namespace.unix_socket_address_bytes),
            ),
            (
                "namespace.windows-named-pipe-name-code-units",
                u64::from(self.namespace.windows_named_pipe_name_code_units),
            ),
            ("server.connection-capacity", u64::from(self.server.connection_capacity)),
            (
                "server.initial-control-frame-milliseconds",
                self.server.initial_control_frame_milliseconds,
            ),
            (
                "server.incomplete-frame-read-idle-milliseconds",
                self.server.incomplete_frame_read_idle_milliseconds,
            ),
            (
                "server.absolute-frame-completion-milliseconds",
                self.server.absolute_frame_completion_milliseconds,
            ),
            ("server.response-write-milliseconds", self.server.response_write_milliseconds),
            (
                "startup.explicit-start-total-milliseconds",
                self.startup.explicit_start_total_milliseconds,
            ),
            (
                "startup.start-retry-maximum-delay-milliseconds",
                self.startup.start_retry_maximum_delay_milliseconds,
            ),
            ("shutdown.cooperative-stop-milliseconds", self.shutdown.cooperative_stop_milliseconds),
            (
                "shutdown.supervised-termination-and-wait-milliseconds",
                self.shutdown.supervised_termination_and_wait_milliseconds,
            ),
            (
                "process-harness.scheduling-tolerance-milliseconds",
                self.process_harness.scheduling_tolerance_milliseconds,
            ),
            (
                "process-harness.walking-start-client-count",
                u64::from(self.process_harness.walking_start_client_count),
            ),
        ]
    }

    /// Refuses a zero in any field that must be positive.
    fn evaluate_positive(&self) -> Result<(), ContractFailure> {
        match self.positive_fields().into_iter().find(|(_, value)| *value == 0) {
            Some((name, _)) => Err(ContractFailure::NotPositive(name)),
            None => Ok(()),
        }
    }

    /// Refuses a rendered length that does not match the bytes it renders.
    fn evaluate_rendered_lengths(&self) -> Result<(), ContractFailure> {
        let pairs = [
            (
                "namespace.digest-rendered-bytes",
                "namespace.digest-bytes",
                self.namespace.digest_rendered_bytes,
                self.namespace.digest_bytes,
            ),
            (
                "namespace.readiness-nonce-rendered-bytes",
                "namespace.readiness-nonce-bytes",
                self.namespace.readiness_nonce_rendered_bytes,
                self.namespace.readiness_nonce_bytes,
            ),
        ];
        for (rendered, raw, rendered_value, raw_value) in pairs {
            if rendered_value != raw_value * RENDERED_BYTES_PER_RAW_BYTE {
                return Err(ContractFailure::RenderedLengthMismatch { rendered, raw });
            }
        }
        Ok(())
    }
}

impl ServerLimits {
    /// Returns the deadline for the first control frame of a connection.
    #[must_use]
    pub const fn initial_control_frame(&self) -> Duration {
        Duration::from_millis(self.initial_control_frame_milliseconds)
    }

    /// Returns the deadline between two reads while a frame is incomplete.
    #[must_use]
    pub const fn incomplete_frame_read_idle(&self) -> Duration {
        Duration::from_millis(self.incomplete_frame_read_idle_milliseconds)
    }

    /// Returns the deadline for completing one frame.
    #[must_use]
    pub const fn absolute_frame_completion(&self) -> Duration {
        Duration::from_millis(self.absolute_frame_completion_milliseconds)
    }

    /// Returns the deadline for writing one response.
    #[must_use]
    pub const fn response_write(&self) -> Duration {
        Duration::from_millis(self.response_write_milliseconds)
    }
}

impl StartupLimits {
    /// Returns the deadline for the whole explicit start.
    #[must_use]
    pub const fn explicit_start_total(&self) -> Duration {
        Duration::from_millis(self.explicit_start_total_milliseconds)
    }

    /// Returns the longest delay between two start retries.
    #[must_use]
    pub const fn start_retry_maximum_delay(&self) -> Duration {
        Duration::from_millis(self.start_retry_maximum_delay_milliseconds)
    }
}

impl ShutdownLimits {
    /// Returns the deadline for a nonce-bound cooperative stop.
    #[must_use]
    pub const fn cooperative_stop(&self) -> Duration {
        Duration::from_millis(self.cooperative_stop_milliseconds)
    }

    /// Returns the deadline for one supervised terminate-and-wait disposition.
    #[must_use]
    pub const fn supervised_termination_and_wait(&self) -> Duration {
        Duration::from_millis(self.supervised_termination_and_wait_milliseconds)
    }
}

impl ProcessHarnessLimits {
    /// Returns the slack a harness adds to a real deadline.
    #[must_use]
    pub const fn scheduling_tolerance(&self) -> Duration {
        Duration::from_millis(self.scheduling_tolerance_milliseconds)
    }
}
