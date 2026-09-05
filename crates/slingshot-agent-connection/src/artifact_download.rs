//! Fetching one artifact from a route this daemon builds, and nothing else.
//!
//! The agent's terminal result says an artifact exists; it does not say where.
//! The route is constructed here from the selected snapshot's own base, the
//! operation identifier, and the slot, with every segment encoded once. No
//! server-supplied location is accepted and no redirect is followed, including
//! a same-origin one: selecting one author origin means asking nowhere else,
//! and following a redirect with a credential attached is how a credential
//! reaches somewhere it was never issued for.
//!
//! # Nothing is written until everything is proved
//!
//! Bytes stream into a staging file while the length, the digest, and - for a
//! loaded document - the canonical form are checked as they arrive. Publication
//! is an atomic rename that happens only after the framed end proves the body
//! ended where it said it would, with no trailer, no trailing byte, the exact
//! expected length, and the exact expected digest. A failed attempt removes the
//! partial file and keeps the mapping, so the retry asks for the same artifact
//! under the same names rather than a fresh one.
//!
//! # An unavailable artifact says so in a closed shape or says nothing
//!
//! Only a fully identified refusal - the right generation, operation,
//! artifact, and slot - may conclude anything. A bare, malformed, or
//! mismatched one is a protocol failure to recover from, because letting an
//! unidentified response select a terminal disposition would let any server
//! that can reach this daemon end its operations.

use slingshot_domain::author_agent_transport_contract::AuthorAgentTransportContract;
use slingshot_domain::command::artifact::{
    CONTENT_PACKAGE_MEDIA_TYPE, CONTENT_PACKAGE_SLOT, LOADED_CONTENT_MEDIA_TYPE,
    LOADED_CONTENT_SLOT,
};

use crate::author_hypertext_transfer_protocol_policy::{ResponseHead, ResponseRefusal};
use crate::structured_job_result::STRUCTURED_RESULT_SLOT;

/// The fixed route artifacts are fetched from, beneath the author's base.
pub const ARTIFACT_ROUTE: &str = "/bin/slingshot-agent/operations";

/// Characters a route segment keeps as itself.
const UNRESERVED: &str = "ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-._~";

/// The content codings a body may arrive under.
///
/// Absent or identity, and automatic decompression is off. A compressed body
/// has a decoded length nobody declared, so the expected-length check would be
/// checking the wrong number.
pub const PERMITTED_CONTENT_CODINGS: &[&str] = &["identity"];

/// Which remote slots exist, and what each one holds.
pub const REMOTE_SLOT_MEDIA_TYPES: &[(&str, &str)] = &[
    (CONTENT_PACKAGE_SLOT, CONTENT_PACKAGE_MEDIA_TYPE),
    (LOADED_CONTENT_SLOT, LOADED_CONTENT_MEDIA_TYPE),
];

/// A bounded, identity-checked unavailable document. Only this evidence may
/// reach unavailable/grace classification; raw status codes prove nothing.
#[derive(Debug)]
pub struct ValidatedArtifactUnavailable {
    reason: slingshot_agent_protocol::artifact_unavailable::UnavailableReason,
}

impl ValidatedArtifactUnavailable {
    /// The reason paired with its required HTTP status.
    pub fn reason(&self) -> slingshot_agent_protocol::artifact_unavailable::UnavailableReason {
        self.reason
    }
}

/// Opaque protocol refusal without remote metadata in diagnostics.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[error("artifact unavailable response is not verified")]
pub struct UnavailableDecodeRefusal;

/// Decodes a closed unavailable body against the retained submission and local
/// artifact derivation. The caller must first validate the HTTP head/framing
/// and bind the submission to its selected connection and retained command.
pub fn decode_artifact_unavailable(
    status: u16,
    body: &[u8],
    submission: &crate::command_submission::Submission,
    artifact_identifier: &str,
    artifact_slot: &str,
) -> Result<ValidatedArtifactUnavailable, UnavailableDecodeRefusal> {
    use slingshot_agent_protocol::artifact_unavailable::{ArtifactUnavailable, UnavailableReason};
    if body.len() as u64
        > AuthorAgentTransportContract::embedded().limit("maximum_agent_protocol_document_bytes")
        || artifact_identifier.is_empty()
        || artifact_identifier.len() > 128
        || submission.operation.agent_event_store_generation == 0
        || submission.operation.agent_operation_identifier.len() != 64
        || !submission
            .operation
            .agent_operation_identifier
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    {
        return Err(UnavailableDecodeRefusal);
    }
    require_remote_slot(artifact_slot).map_err(|_| UnavailableDecodeRefusal)?;
    let document: ArtifactUnavailable =
        serde_json::from_slice(body).map_err(|_| UnavailableDecodeRefusal)?;
    let installed = slingshot_domain::selected_command_contract_identity::SelectedCommandContractIdentity::installed(&submission.provenance.command_contract.command_wire_name).map_err(|_| UnavailableDecodeRefusal)?;
    slingshot_agent_protocol::wire_contract::ExpectedProvenance {
        command_contract: installed,
        canonical_json_contract_digest:
            slingshot_domain::command::schema::canonical_contract_digest(),
        transport_contract_digest: AuthorAgentTransportContract::embedded_digest(),
    }
    .require_matching(&document.provenance)
    .map_err(|_| UnavailableDecodeRefusal)?;
    if document.provenance != submission.provenance
        || document.agent_event_store_generation
            != submission.operation.agent_event_store_generation
        || document.agent_operation_identifier != submission.operation.agent_operation_identifier
        || document.artifact_identifier != artifact_identifier
        || document.artifact_slot != artifact_slot
        || !matches!(
            (status, document.reason),
            (404, UnavailableReason::Missing) | (410, UnavailableReason::RetentionExpired)
        )
    {
        return Err(UnavailableDecodeRefusal);
    }
    Ok(ValidatedArtifactUnavailable { reason: document.reason })
}

/// Why one artifact could not be fetched or published.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum DownloadRefusal {
    /// A route placeholder was not in the protocol's canonical ASCII grammar.
    #[error("the artifact route contains a noncanonical placeholder")]
    InvalidRouteSegment,
    /// The operation belongs to a different frozen author connection.
    ///
    /// No request was made.  This is distinct from a server redirect because
    /// the mismatch was found locally before any response existed.
    #[error("the operation belongs to another selected author connection")]
    SelectedConnectionMismatch,
    /// The response head is one the shared policy refuses.
    #[error(transparent)]
    Head(#[from] ResponseRefusal),
    /// The response tried to send this daemon somewhere else.
    #[error("this artifact is asked for at one route, and a server naming another is refused")]
    ServerRouteOffered {
        /// Where it pointed.
        offered: String,
    },
    /// The slot is one no remote artifact may fill.
    #[error("{slot} is a local slot, and no remote artifact fills it")]
    LocalSlot {
        /// Which slot was named.
        slot: String,
    },
    /// The slot is one this build does not know.
    #[error("{slot} is not a slot any command declares")]
    UnknownSlot {
        /// Which slot was named.
        slot: String,
    },
    /// The body is not what the manifest said it would be.
    #[error("this slot holds {expected}, and the body announced {named}")]
    MediaTypeDrifted {
        /// What the manifest declared.
        expected: String,
        /// What the body announced.
        named: String,
    },
    /// The body arrived under a coding whose decoded length nobody declared.
    #[error("this body is encoded as {named}, and its decoded length is nobody's declaration")]
    ContentCoding {
        /// What it was encoded as.
        named: String,
    },
    /// A trailer section arrived that the head never declared.
    #[error("a trailer nobody declared means the body did not end where it said")]
    UndeclaredTrailer,
    /// Bytes followed the body.
    #[error("bytes after the body mean the body did not end where it said")]
    TrailingBytes,
    /// The body is not the length the manifest said.
    #[error("this artifact holds {expected} bytes, and {actual} arrived")]
    LengthDrifted {
        /// How long the manifest said.
        expected: u64,
        /// How long it was.
        actual: u64,
    },
    /// The body is not the content the manifest said.
    #[error("this artifact does not digest to what the manifest said it would")]
    DigestDrifted,
    /// The body is longer than the slot admits.
    #[error("this slot admits {allowed} bytes, and this reached {actual}")]
    SlotMaximumExceeded {
        /// How long one may be.
        allowed: u64,
        /// How long it reached.
        actual: u64,
    },
    /// A loaded document is not canonical.
    #[error("a loaded document is canonical, and these bytes are not")]
    NotCanonical,
}

/// Returns `segment` with every reserved character percent-encoded, once.
#[must_use]
pub fn encoded_segment(segment: &str) -> String {
    let mut encoded = String::new();
    for octet in segment.bytes() {
        if UNRESERVED.as_bytes().contains(&octet) {
            encoded.push(char::from(octet));
        } else {
            encoded.push_str(&format!("%{octet:02X}"));
        }
    }
    encoded
}

/// Returns the one route this artifact is fetched from.
///
/// Built from the snapshot's own base and nothing a response said. Every
/// placeholder must already have the protocol's canonical ASCII spelling.
#[must_use]
pub fn artifact_route(
    author_base: &str,
    agent_operation_identifier: &str,
    artifact_slot: &str,
) -> Result<String, DownloadRefusal> {
    let maximum =
        AuthorAgentTransportContract::embedded().limit("maximum_agent_operation_identifier_bytes");
    if agent_operation_identifier.is_empty()
        || agent_operation_identifier.len() as u64 > maximum
        || !agent_operation_identifier.bytes().all(|byte| {
            byte.is_ascii_lowercase() || byte.is_ascii_digit() || matches!(byte, b'-' | b'_')
        })
    {
        return Err(DownloadRefusal::InvalidRouteSegment);
    }
    require_remote_slot(artifact_slot)?;
    Ok(format!(
        "{}{ARTIFACT_ROUTE}/{}/artifacts/{}",
        author_base.trim_end_matches('/'),
        encoded_segment(agent_operation_identifier),
        encoded_segment(artifact_slot)
    ))
}

/// Requires a slot to be one a remote artifact may fill.
///
/// # Errors
///
/// Returns [`DownloadRefusal::LocalSlot`] or [`DownloadRefusal::UnknownSlot`].
pub fn require_remote_slot(artifact_slot: &str) -> Result<&'static str, DownloadRefusal> {
    if artifact_slot == STRUCTURED_RESULT_SLOT {
        return Err(DownloadRefusal::LocalSlot { slot: artifact_slot.to_owned() });
    }
    REMOTE_SLOT_MEDIA_TYPES
        .iter()
        .find(|(slot, _)| *slot == artifact_slot)
        .map(|(_, media_type)| *media_type)
        .ok_or_else(|| DownloadRefusal::UnknownSlot { slot: artifact_slot.to_owned() })
}

/// What the manifest says one artifact will be.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExpectedArtifact {
    /// What it will digest to.
    pub artifact_digest: String,
    /// Which slot it fills.
    pub artifact_slot: String,
    /// How long it will be.
    pub byte_length: u64,
    /// What it will be.
    pub media_type: String,
}

/// The head of one artifact response, as far as this policy reads it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ArtifactResponseHead {
    /// What the body announces it is.
    pub content_type: String,
    /// The shared response head.
    pub head: ResponseHead,
}

/// Requires one response head to be one this artifact may stream from.
///
/// # Errors
///
/// Returns [`DownloadRefusal`] naming the first thing that is wrong, before a
/// byte of body is read.
pub fn require_streamable(
    expected: &ExpectedArtifact,
    announced: &ArtifactResponseHead,
) -> Result<(), DownloadRefusal> {
    if let Some(offered) = &announced.head.location {
        return Err(DownloadRefusal::ServerRouteOffered { offered: offered.clone() });
    }
    announced.head.require_acceptable()?;
    if let Some(coding) = &announced.head.content_coding
        && !PERMITTED_CONTENT_CODINGS.contains(&coding.as_str())
    {
        return Err(DownloadRefusal::ContentCoding { named: coding.clone() });
    }
    if announced.content_type != expected.media_type {
        return Err(DownloadRefusal::MediaTypeDrifted {
            expected: expected.media_type.clone(),
            named: announced.content_type.clone(),
        });
    }
    Ok(())
}

/// How one transfer ended.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TransferEnd {
    /// The body ended where the framing said it would.
    Framed,
    /// A trailer section arrived that nobody declared.
    UndeclaredTrailer,
    /// Bytes followed the body.
    TrailingBytes,
    /// The connection went before the body ended.
    Interrupted,
}

/// One transfer in progress, bounded as it goes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ArtifactTransfer {
    /// What the manifest said this would be.
    expected: ExpectedArtifact,
    /// How many bytes have arrived.
    received: u64,
    /// How large one artifact in this slot may be.
    slot_maximum: u64,
}

impl ArtifactTransfer {
    /// Returns a transfer of `expected`, bounded by `slot_maximum`.
    #[must_use]
    pub fn of(expected: ExpectedArtifact, slot_maximum: u64) -> Self {
        Self { expected, received: 0, slot_maximum }
    }

    /// Returns how many bytes have arrived.
    #[must_use]
    pub fn received(&self) -> u64 {
        self.received
    }

    /// Records `bytes` more, refusing before they are written anywhere.
    ///
    /// Both bounds are checked as the bytes arrive rather than at the end. A
    /// server that sends more than it declared has already cost this daemon the
    /// disk it wrote by the time a final check notices.
    ///
    /// # Errors
    ///
    /// Returns [`DownloadRefusal::LengthDrifted`] or
    /// [`DownloadRefusal::SlotMaximumExceeded`].
    pub fn absorb(&mut self, bytes: u64) -> Result<(), DownloadRefusal> {
        let reached = self.received.saturating_add(bytes);
        if reached > self.slot_maximum {
            return Err(DownloadRefusal::SlotMaximumExceeded {
                allowed: self.slot_maximum,
                actual: reached,
            });
        }
        if reached > self.expected.byte_length {
            return Err(DownloadRefusal::LengthDrifted {
                expected: self.expected.byte_length,
                actual: reached,
            });
        }
        self.received = reached;
        Ok(())
    }

    /// Returns whether this transfer may be published, or why it may not.
    ///
    /// Everything at once, because publication is the moment all of it has to
    /// be true together: a body that ended cleanly at the wrong length and a
    /// body of the right length that never ended are both failures, and a
    /// partial file is removed either way.
    ///
    /// # Errors
    ///
    /// Returns [`DownloadRefusal`] naming the first thing that is not true.
    pub fn require_publishable(
        &self,
        end: TransferEnd,
        observed_digest: &str,
    ) -> Result<(), DownloadRefusal> {
        match end {
            TransferEnd::UndeclaredTrailer => return Err(DownloadRefusal::UndeclaredTrailer),
            TransferEnd::TrailingBytes => return Err(DownloadRefusal::TrailingBytes),
            TransferEnd::Interrupted => {
                return Err(DownloadRefusal::LengthDrifted {
                    expected: self.expected.byte_length,
                    actual: self.received,
                });
            }
            TransferEnd::Framed => {}
        }
        if self.received != self.expected.byte_length {
            return Err(DownloadRefusal::LengthDrifted {
                expected: self.expected.byte_length,
                actual: self.received,
            });
        }
        if observed_digest != self.expected.artifact_digest {
            return Err(DownloadRefusal::DigestDrifted);
        }
        Ok(())
    }
}

/// Legacy metadata-only input for unavailable policy tests. This is not the
/// wire document and does not prove the protocol's artifact-identifier echo;
/// network callers must use `decode_artifact_unavailable` first.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ArtifactUnavailable {
    /// Which operation it claims to be about.
    pub agent_operation_identifier: String,
    /// Which incarnation of the store it claims to be from.
    pub agent_event_store_generation: u64,
    /// Which artifact it claims to be about.
    pub artifact_digest: String,
    /// Which slot it claims to be about.
    pub artifact_slot: String,
    /// Which of the two closed reasons it gives.
    pub reason: UnavailableReason,
}

/// The two reasons an artifact may be unavailable.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UnavailableReason {
    /// It is not there, which may still be propagation delay.
    Missing,
    /// It was there and the window closed, which is final.
    RetentionExpired,
}

/// What an unavailable answer may conclude.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UnavailableOutcome {
    /// It may not be there yet, so ask again until the grace runs out.
    Grace {
        /// When asking again stops being the answer.
        until_unix_milliseconds: u64,
    },
    /// It succeeded remotely and the result can no longer be fetched.
    ResultUnavailable,
    /// The answer proves nothing about this daemon's artifact.
    ProtocolInvalid,
}

/// Returns what one unavailable answer means for the artifact that was asked for.
///
/// Every identifying field has to match. An answer that identifies nothing, or
/// identifies something else, is a protocol failure rather than a conclusion:
/// letting it select a disposition would let any server that can reach this
/// daemon end its operations.
#[must_use]
pub fn unavailable_outcome(
    expected: &ExpectedArtifact,
    agent_operation_identifier: &str,
    agent_event_store_generation: u64,
    answered: Option<&ArtifactUnavailable>,
    asked_at_unix_milliseconds: u64,
    now_unix_milliseconds: u64,
) -> UnavailableOutcome {
    let Some(answered) = answered else {
        return UnavailableOutcome::ProtocolInvalid;
    };
    let identified = answered.agent_operation_identifier == agent_operation_identifier
        && answered.agent_event_store_generation == agent_event_store_generation
        && answered.artifact_digest == expected.artifact_digest
        && answered.artifact_slot == expected.artifact_slot;
    if !identified {
        return UnavailableOutcome::ProtocolInvalid;
    }
    match answered.reason {
        UnavailableReason::RetentionExpired => UnavailableOutcome::ResultUnavailable,
        UnavailableReason::Missing => {
            let until = asked_at_unix_milliseconds.saturating_add(missing_grace_milliseconds());
            if now_unix_milliseconds < until {
                UnavailableOutcome::Grace { until_unix_milliseconds: until }
            } else {
                UnavailableOutcome::ResultUnavailable
            }
        }
    }
}

/// Returns how long a missing artifact is given before it means something.
#[must_use]
pub fn missing_grace_milliseconds() -> u64 {
    AuthorAgentTransportContract::embedded().limit("missing_operation_grace_milliseconds")
}
