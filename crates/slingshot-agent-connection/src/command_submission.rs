//! Submitting one command, and knowing afterwards whether it took.
//!
//! The hard part is not sending it. It is the moment after sending, when the
//! exchange fails before an answer arrives: the command may have been recorded
//! at the agent, or it may not, and a daemon that assumed either would be wrong
//! half the time. So the outcome is not a two-way answer with an error case but
//! a three-way answer in which "no one knows" is a first-class result, left
//! only by a proof that no request byte was written or by an answer that
//! survives every check below.
//!
//! Nothing about a submission is allocated. The operation identifier comes from
//! the local operation and the target partition; the submitted digest from the
//! transport contract, the five contract fields, the canonical byte contract,
//! the complete canonical arguments, and the artifact manifest; the idempotency
//! key is that digest. So a daemon that crashed between writing the request and
//! recording the outcome arrives at the same names when it restarts, and asks
//! about the submission by name instead of sending a second one and hoping.
//!
//! An agent that acknowledges while naming a different operation, generation,
//! target, or digest recorded something else. Believing it would create a local
//! row describing remote work that does not exist, which is worse than not
//! knowing - not knowing at least prompts a lookup. Every such mismatch
//! therefore lands in [`SubmissionOutcome::SubmissionUnknown`] rather than in an
//! error, because after the request bytes are written a failed check is
//! evidence about the response, not about the request.

use slingshot_agent_protocol::identity::{DocumentProvenance, WireOperationIdentity};
use slingshot_agent_protocol::wire_contract::{ExpectedProvenance, WireRefusal};
use slingshot_domain::author_agent_transport_contract::AuthorAgentTransportContract;
use slingshot_domain::selected_command_contract_identity::SubmittedCommandDigest;

use crate::author_cross_site_request_forgery_protection::{
    CrossSiteRequestForgeryToken, TOKEN_HEADER, TokenFailure, header_for,
};
use crate::author_hypertext_transfer_protocol_policy::{
    ResponseHead, ResponseRefusal, retry_delay_milliseconds,
};

/// The method one submission is sent with.
pub const SUBMISSION_METHOD: &str = "POST";

/// Header carrying the key that makes a resend the same submission.
pub const IDEMPOTENCY_KEY_HEADER: &str = "Idempotency-Key";

/// Header naming the origin a request was formed against.
pub const REFERER_HEADER: &str = "Referer";

/// Header naming what a request body is.
pub const CONTENT_TYPE_HEADER: &str = "Content-Type";

/// The media type a submission carries, and the only one accepted back.
pub const SUBMISSION_MEDIA_TYPE: &str = "application/json";

/// Headers this module sets, which a caller may therefore not.
///
/// Not a matter of tidiness. Each of these carries an authorisation or identity
/// decision that was made from derived values, and a caller that could override
/// one could submit under a key or an origin that nothing else agrees with.
pub const RESERVED_HEADERS: &[&str] =
    &[TOKEN_HEADER, IDEMPOTENCY_KEY_HEADER, REFERER_HEADER, CONTENT_TYPE_HEADER, "Authorization"];

/// Version the submission binding digest is derived under.
pub const SUBMISSION_BINDING_VERSION: &str = "slingshot.command-submission/1";

/// Separator between the fields of a derived digest.
const FIELD_SEPARATOR: u8 = 0;

/// How the response policy names an informational head it refused.
const INFORMATIONAL_HEAD_MECHANISM: &str = "an informational head";

/// What a command declares it will produce, before it is sent.
///
/// Derived before the request rather than learned from the response, because
/// the retention branch is reserved before a remote job exists: a reservation
/// made afterwards can fail after the work is already running.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ManifestKind {
    /// The command produces no artifact at all.
    Empty,
    /// The command loads content and produces one artifact.
    Load,
    /// The command builds a package and produces several.
    Package,
}

impl ManifestKind {
    /// Returns how this kind is spelled inside a derived digest.
    #[must_use]
    pub fn as_text(self) -> &'static str {
        match self {
            Self::Empty => "empty",
            Self::Load => "load",
            Self::Package => "package",
        }
    }
}

/// What one command's artifacts are expected to come to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ExpectedArtifactManifest {
    /// How many artifacts at most.
    pub artifact_rows: u64,
    /// How many bytes at most, across all of them.
    pub artifact_bytes: u64,
    /// Which shape of manifest this is.
    pub kind: ManifestKind,
}

impl ExpectedArtifactManifest {
    /// Returns the manifest of a command that produces nothing.
    #[must_use]
    pub fn empty() -> Self {
        Self { artifact_rows: 0, artifact_bytes: 0, kind: ManifestKind::Empty }
    }

    /// Returns the manifest a command declares, if this build can hold it.
    ///
    /// # Errors
    ///
    /// Returns [`SubmissionRefusal::ManifestBeyondCapacity`] when the declared
    /// worst case exceeds one generation's artifact capacity, which is refused
    /// here rather than discovered part-way through a transfer.
    pub fn declaring(
        kind: ManifestKind,
        artifact_rows: u64,
        artifact_bytes: u64,
    ) -> Result<Self, SubmissionRefusal> {
        let contract = AuthorAgentTransportContract::embedded();
        let (allowed_rows, allowed_bytes) = match kind {
            ManifestKind::Empty => (0, 0),
            ManifestKind::Load | ManifestKind::Package => (
                contract.formula("maximum_current_generation_artifact_rows"),
                contract.limit("maximum_current_generation_artifact_bytes"),
            ),
        };
        if artifact_rows > allowed_rows || artifact_bytes > allowed_bytes {
            return Err(SubmissionRefusal::ManifestBeyondCapacity {
                allowed_bytes,
                allowed_rows,
                declared_bytes: artifact_bytes,
                declared_rows: artifact_rows,
            });
        }
        Ok(Self { artifact_rows, artifact_bytes, kind })
    }
}

/// Which closed capacity an agent refused a submission for.
///
/// Closed, and separate from every other refusal, because a capacity refusal is
/// the one rejection proving nothing was reserved: without it a daemon hunts a
/// partially created operation that cannot exist.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CapacityDiscriminator {
    /// No room for another artifact.
    Artifact,
    /// No room for another event.
    Event,
    /// No room for another operation's execution detail.
    ExecutionDetail,
    /// No room for another operation.
    Operation,
    /// No room for another result.
    Result,
    /// No room for another snapshot.
    Snapshot,
    /// No room for another subscription.
    Subscription,
}

/// Why an agent authoritatively did not execute a submission.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NonExecution {
    /// One named capacity is full, and nothing was reserved.
    Capacity(CapacityDiscriminator),
    /// The command itself is one this agent will not run.
    Semantic,
}

/// The worst case one submission reserves before a remote job exists.
///
/// Worst case rather than expected: a reservation sized to the expected case
/// runs out when the work is unusual, which is when abandoning it costs most.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RetentionReservation {
    /// Artifacts the manifest permits.
    pub artifact_rows: u64,
    /// Events one operation may emit.
    pub event_rows: u64,
    /// Execution-detail rows one operation occupies.
    pub execution_detail_rows: u64,
    /// Results one operation may produce.
    pub result_rows: u64,
    /// Snapshots one operation may leave.
    pub snapshot_rows: u64,
    /// Subscriptions one operation registers.
    pub subscription_rows: u64,
}

impl RetentionReservation {
    /// One operation occupies exactly one execution-detail row.
    pub const EXECUTION_DETAIL_ROWS_PER_OPERATION: u64 = 1;

    /// One operation registers exactly one subscription.
    pub const SUBSCRIPTION_ROWS_PER_OPERATION: u64 = 1;

    /// One operation leaves exactly one snapshot and one result.
    pub const TERMINAL_ROWS_PER_OPERATION: u64 = 1;

    /// Returns the whole branch one submission of `manifest` must reserve.
    #[must_use]
    pub fn worst_case_for(manifest: ExpectedArtifactManifest) -> Self {
        Self {
            artifact_rows: manifest.artifact_rows,
            event_rows: AuthorAgentTransportContract::embedded()
                .limit("maximum_operation_event_rows"),
            execution_detail_rows: Self::EXECUTION_DETAIL_ROWS_PER_OPERATION,
            result_rows: Self::TERMINAL_ROWS_PER_OPERATION,
            snapshot_rows: Self::TERMINAL_ROWS_PER_OPERATION,
            subscription_rows: Self::SUBSCRIPTION_ROWS_PER_OPERATION,
        }
    }

    /// Returns whether every part of this branch is reserved, or none is.
    #[must_use]
    pub fn is_whole(&self) -> bool {
        self.execution_detail_rows == Self::EXECUTION_DETAIL_ROWS_PER_OPERATION
            && self.subscription_rows == Self::SUBSCRIPTION_ROWS_PER_OPERATION
            && self.snapshot_rows == Self::TERMINAL_ROWS_PER_OPERATION
            && self.result_rows == Self::TERMINAL_ROWS_PER_OPERATION
            && self.event_rows > 0
    }
}

/// Where in one exchange something went wrong.
///
/// What matters is not how far the exchange got but whether a request byte can
/// have reached the author: before the first byte proves the agent never saw
/// the submission, and after it proves nothing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Checkpoint {
    /// Resolving the author's name.
    NameResolution,
    /// Reading the response body.
    ResponseBody,
    /// Waiting for the response head.
    ResponseHead,
    /// Writing the request body.
    RequestBody,
    /// Writing the request head.
    RequestHead,
    /// Opening the connection.
    TransportConnect,
    /// Completing the security handshake.
    TransportLayerSecurity,
}

impl Checkpoint {
    /// Returns whether a request byte can already have reached the author.
    ///
    /// The whole three-way outcome hangs off this one answer.
    #[must_use]
    pub fn bytes_may_have_reached_author(self) -> bool {
        !matches!(
            self,
            Self::NameResolution | Self::TransportConnect | Self::TransportLayerSecurity
        )
    }

    /// Returns how long this phase may take.
    #[must_use]
    pub fn deadline_milliseconds(self) -> u64 {
        let contract = AuthorAgentTransportContract::embedded();
        match self {
            Self::NameResolution | Self::TransportConnect => {
                contract.limit("author_connect_timeout_milliseconds")
            }
            Self::TransportLayerSecurity => contract.limit("author_tls_timeout_milliseconds"),
            Self::RequestHead | Self::RequestBody => {
                contract.limit("author_request_body_timeout_milliseconds")
            }
            Self::ResponseHead => contract.limit("author_response_header_timeout_milliseconds"),
            Self::ResponseBody => contract.limit("finite_response_total_timeout_milliseconds"),
        }
    }
}

/// What a status code means for a submission, and nothing finer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StatusClass {
    /// The agent answered about this submission.
    Answered,
    /// The agent refused it, and says so authoritatively.
    Rejected,
    /// The agent says this identifier already means something else.
    Conflict,
    /// Nothing is settled, and the same submission may be sent again.
    Retryable,
    /// This build does not know what the status means.
    Unvalidated,
}

/// Statuses that mean the agent answered about this submission.
pub const ANSWERED_STATUSES: &[u16] = &[200, 201, 202];

/// Statuses that mean the agent authoritatively refused.
pub const REJECTED_STATUSES: &[u16] = &[400, 403, 422];

/// The single status that means this identifier already means something else.
pub const CONFLICT_STATUS: u16 = 409;

/// Statuses that settle nothing and permit the same submission again.
pub const RETRYABLE_STATUSES: &[u16] = &[408, 429, 500, 502, 503, 504];

/// Returns what `status` means, refusing to guess about anything else.
#[must_use]
pub fn classify_status(status: u16) -> StatusClass {
    if ANSWERED_STATUSES.contains(&status) {
        StatusClass::Answered
    } else if REJECTED_STATUSES.contains(&status) {
        StatusClass::Rejected
    } else if status == CONFLICT_STATUS {
        StatusClass::Conflict
    } else if RETRYABLE_STATUSES.contains(&status) {
        StatusClass::Retryable
    } else {
        StatusClass::Unvalidated
    }
}

/// What kept an answer from being believed.
///
/// Closed, and carrying no response text: a cause drawn from the body lets a
/// server choose this daemon's error codes and puts a remote string in a log.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UnknownCause {
    /// The body could not be read as the answer it claims to be.
    Body,
    /// A phase deadline expired after request bytes were written.
    Deadline(Checkpoint),
    /// The answer echoes a different submitted digest.
    Digest,
    /// The answer names no job identifier where it must name one.
    EmptyIdentifier,
    /// The head declares or the message carries an ambiguous framing.
    Framing,
    /// The answer echoes a different generation.
    Generation,
    /// An informational head arrived where a final one belongs.
    InformationalHead,
    /// The answer echoes a different operation.
    Identity,
    /// The response is not the media type a submission is answered in.
    Media,
    /// The answer echoes a different target partition.
    Partition,
    /// The answer arrived over a protocol, redirect, or migration refused here.
    ProtocolVersion,
    /// The subscription this submission registers is not the one echoed.
    Registration,
    /// The retention the answer grants cannot be held.
    Retention,
    /// A trailer section arrived.
    TrailerSection,
    /// Trailers were declared.
    TrailersDeclared,
    /// Bytes followed the body.
    TrailingBytes,
    /// The answer carries a field this build does not know.
    UnknownField,
    /// The status is not one this build validated.
    UnvalidatedStatus,
}

impl From<ResponseRefusal> for UnknownCause {
    fn from(refusal: ResponseRefusal) -> Self {
        match refusal {
            ResponseRefusal::TrailersDeclared => Self::TrailersDeclared,
            ResponseRefusal::UnexpectedContentCoding { .. } => Self::Media,
            ResponseRefusal::FieldTooLong { .. }
            | ResponseRefusal::TooManyFields { .. }
            | ResponseRefusal::HeadTooLong { .. } => Self::Body,
            ResponseRefusal::MigrationAttempted { mechanism } => {
                if mechanism == INFORMATIONAL_HEAD_MECHANISM {
                    Self::InformationalHead
                } else {
                    Self::ProtocolVersion
                }
            }
            ResponseRefusal::ProtocolVersion { .. } | ResponseRefusal::RedirectOffered { .. } => {
                Self::ProtocolVersion
            }
        }
    }
}

/// What submitting one command produced.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SubmissionOutcome {
    /// The agent recorded it, and named the jobs doing it.
    Accepted {
        /// The bounded, sorted, distinct physical jobs known so far.
        physical_sling_job_identifiers: Vec<String>,
        /// How long the results survive, counted from request start.
        remaining_retention_milliseconds: u64,
    },
    /// The agent already held it, and named the jobs doing it.
    Duplicate {
        /// The bounded, sorted, distinct physical jobs known so far.
        physical_sling_job_identifiers: Vec<String>,
        /// How long the results survive, counted from request start.
        remaining_retention_milliseconds: u64,
    },
    /// The agent held it once and no longer does, so nothing can be recovered.
    RecoveryWindowExpired,
    /// The agent refused it, and provably reserved nothing.
    AuthoritativeNonExecution {
        /// Which closed refusal it named.
        non_execution: NonExecution,
    },
    /// This identifier already means a different submission at the agent.
    Conflict,
    /// Nothing is settled; the identical submission may go again after a wait.
    RetryAfter {
        /// How long to wait, bounded whatever the server asked for.
        milliseconds: u64,
    },
    /// Nobody knows. The submission may or may not have been recorded.
    SubmissionUnknown {
        /// What stopped the answer being believed.
        cause: UnknownCause,
    },
    /// No request byte reached the author, so the agent never saw it.
    ConfirmedNotExecuted {
        /// Where the exchange failed, all of which precede the first byte.
        checkpoint: Checkpoint,
    },
}

impl SubmissionOutcome {
    /// Returns whether the agent provably recorded this submission.
    #[must_use]
    pub fn provably_recorded(&self) -> bool {
        matches!(self, Self::Accepted { .. } | Self::Duplicate { .. })
    }

    /// Returns whether the agent provably did not record this submission.
    ///
    /// Two things qualify and an unsettled outcome is neither: reading
    /// [`SubmissionOutcome::SubmissionUnknown`] as one is how a command runs
    /// twice.
    #[must_use]
    pub fn provably_not_recorded(&self) -> bool {
        matches!(self, Self::ConfirmedNotExecuted { .. } | Self::AuthoritativeNonExecution { .. })
    }

    /// Returns whether this outcome must be settled by asking the agent.
    #[must_use]
    pub fn requires_reconciliation(&self) -> bool {
        matches!(self, Self::SubmissionUnknown { .. })
    }
}

/// Why a submission could not be built or sent.
///
/// Everything here happens before a byte is written, which is why these are
/// errors: an outcome after that point is a [`SubmissionOutcome`].
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum SubmissionRefusal {
    /// The canonical arguments are larger than a submission may be.
    #[error("a canonical submission holds at most {allowed} bytes, and this holds {actual}")]
    TooLarge {
        /// How large one may be.
        allowed: u64,
        /// How large this is.
        actual: usize,
    },
    /// The declared artifacts do not fit one generation's capacity.
    #[error(
        "one generation holds {allowed_rows} artifacts of {allowed_bytes} bytes, and this command declares {declared_rows} of {declared_bytes}"
    )]
    ManifestBeyondCapacity {
        /// How many bytes fit.
        allowed_bytes: u64,
        /// How many artifacts fit.
        allowed_rows: u64,
        /// How many bytes this declares.
        declared_bytes: u64,
        /// How many artifacts this declares.
        declared_rows: u64,
    },
    /// A caller tried to set a header this module derives.
    #[error("this submission derives {0}, so a caller supplying it would be overriding a decision")]
    ReservedHeader(String),
    /// A caller supplied the same header twice.
    #[error("this submission carries {0} once, and a caller supplied it more than once")]
    DuplicateHeader(String),
    /// The reservation this submission needs is not whole.
    #[error("a submission reserves its complete retention branch or none of it")]
    PartialReservation,
    /// The persisted generation is not the one the agent is on now.
    #[error(
        "this submission was derived under generation {persisted}, and the agent is on {current}"
    )]
    GenerationChanged {
        /// Which generation the agent is on now.
        current: u64,
        /// Which generation the submission was derived under.
        persisted: u64,
    },
    /// The agent cannot issue continuations, so no recovery lookup can settle.
    #[error("a recovery submission needs a continuation authority, and this agent has none")]
    ContinuationAuthorityAbsent,
    /// A continuation carries a window it must instead have inherited.
    #[error("a continuation reuses the limit its token was issued under and names no offset")]
    ContinuationCarriesWindow,
    /// Nothing can be presented for the forgery-protection header.
    #[error(transparent)]
    Token(#[from] TokenFailure),
    /// The document names contracts this build does not have.
    #[error(transparent)]
    Provenance(#[from] WireRefusal),
}

/// What a discovery submission asks for.
///
/// Two shapes and no third. A continuation carrying a window would let a caller
/// widen the page its token was issued for, which that binding prevents.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DiscoveryShape {
    /// The first page, which names where to start and how much to take.
    Initial {
        /// How many rows to take.
        limit: u64,
        /// Where to start.
        offset: u64,
    },
    /// A later page, which names only the opaque token it continues.
    Continuation {
        /// The limit the originating request was issued under.
        originating_limit: u64,
        /// The opaque bytes, unread and unchanged.
        token: String,
    },
}

impl DiscoveryShape {
    /// Returns the continuation of a page taken under `originating_limit`.
    #[must_use]
    pub fn continuing(token: &str, originating_limit: u64) -> Self {
        Self::Continuation { originating_limit, token: token.to_owned() }
    }

    /// Returns the query this shape sends, in order.
    ///
    /// A continuation sends the token alone; the limit it inherited governs the
    /// page and is not restated, which would make it a caller's to change.
    #[must_use]
    pub fn query_pairs(&self) -> Vec<(&'static str, String)> {
        match self {
            Self::Initial { limit, offset } => {
                vec![("offset", offset.to_string()), ("limit", limit.to_string())]
            }
            Self::Continuation { token, .. } => vec![("continuation", token.clone())],
        }
    }

    /// Requires this shape to be one the route accepts.
    ///
    /// # Errors
    ///
    /// Returns [`SubmissionRefusal::ContinuationCarriesWindow`] when a
    /// continuation would put an offset or a limit on the wire.
    pub fn require_wellformed(&self) -> Result<(), SubmissionRefusal> {
        let named = self.query_pairs();
        let carries_window = named.iter().any(|(name, _)| matches!(*name, "offset" | "limit"));
        if matches!(self, Self::Continuation { .. }) && carries_window {
            return Err(SubmissionRefusal::ContinuationCarriesWindow);
        }
        Ok(())
    }
}

/// What must hold before a submission is sent again after a crash.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RecoveryPreconditions {
    /// Whether the agent can issue and validate continuations.
    pub continuation_authority_ready: bool,
    /// Which generation the agent is on now.
    pub current_generation: u64,
}

/// What the agent said about a submission.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SubmissionAcknowledgement {
    /// Which generation the agent recorded it under.
    pub agent_event_store_generation: u64,
    /// Which operation the agent says it recorded.
    pub agent_operation_identifier: String,
    /// Which target partition the agent recorded it in.
    pub author_target_identity_digest: String,
    /// Whether the agent already held this submission.
    pub already_accepted: bool,
    /// Which subscription the agent registered.
    pub daemon_subscription_identifier: String,
    /// How long the agent promises to keep the results.
    pub granted_retention_milliseconds: u64,
    /// Which closed refusal it named, when it refused.
    pub non_execution: Option<NonExecution>,
    /// The physical jobs the agent knows are doing this work.
    pub physical_sling_job_identifiers: Vec<String>,
    /// Whether the agent held this submission once and no longer does.
    pub retired: bool,
    /// Which digest the agent says it recorded.
    pub submitted_command_digest: String,
}

/// One whole exchange, as far as it got.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Exchange {
    /// What the agent said, when the body parsed into an answer.
    pub acknowledgement: Option<SubmissionAcknowledgement>,
    /// How many bytes the body came to.
    pub body_bytes: u64,
    /// How long the whole exchange took, from before any network work.
    pub elapsed_milliseconds: u64,
    /// Whether the message length could be read two ways.
    pub framing_ambiguous: bool,
    /// The final response head.
    pub head: ResponseHead,
    /// What the body claims to be.
    pub media_type: String,
    /// How long the server asked to be left alone.
    pub retry_after_milliseconds: Option<u64>,
    /// The status the final head carried.
    pub status: u16,
    /// Whether a trailer section arrived.
    pub trailer_section_present: bool,
    /// Whether bytes followed the body.
    pub trailing_bytes: bool,
    /// Whether the answer carried a field this build does not know.
    pub unknown_fields: bool,
}

/// What one submission sends, all of it derived.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Submission {
    /// The canonical argument bytes, exactly as they are digested and sent.
    pub canonical_arguments: String,
    /// Which subscription this submission registers.
    pub daemon_subscription_identifier: String,
    /// What this command declares it will produce.
    pub manifest: ExpectedArtifactManifest,
    /// Which operation this is at the agent.
    pub operation: WireOperationIdentity,
    /// What the document says about which contracts it means.
    pub provenance: DocumentProvenance,
    /// The digest binding contracts, arguments, and manifest together.
    pub submitted_command_digest: String,
}

impl Submission {
    /// Returns the submission one command produces.
    ///
    /// # Errors
    ///
    /// Returns [`SubmissionRefusal::TooLarge`] when the arguments do not fit.
    pub fn build(
        expected: &ExpectedProvenance,
        operation: WireOperationIdentity,
        daemon_subscription_identifier: &str,
        canonical_arguments: &str,
        manifest: ExpectedArtifactManifest,
    ) -> Result<Self, SubmissionRefusal> {
        let allowed =
            AuthorAgentTransportContract::embedded().limit("maximum_canonical_submission_bytes");
        if u64::try_from(canonical_arguments.len()).unwrap_or(u64::MAX) > allowed {
            return Err(SubmissionRefusal::TooLarge { allowed, actual: canonical_arguments.len() });
        }
        Ok(Self {
            canonical_arguments: canonical_arguments.to_owned(),
            daemon_subscription_identifier: daemon_subscription_identifier.to_owned(),
            manifest,
            operation,
            provenance: expected.provenance(),
            submitted_command_digest: bind_submitted_digest(
                expected,
                canonical_arguments,
                manifest,
            ),
        })
    }

    /// Returns the reservation this submission takes before a job can exist.
    #[must_use]
    pub fn reservation(&self) -> RetentionReservation {
        RetentionReservation::worst_case_for(self.manifest)
    }

    /// Returns the headers this submission is sent with.
    ///
    /// # Errors
    ///
    /// Returns [`SubmissionRefusal::ReservedHeader`],
    /// [`SubmissionRefusal::DuplicateHeader`],
    /// [`SubmissionRefusal::PartialReservation`], or [`SubmissionRefusal::Token`].
    pub fn request_headers(
        &self,
        held_token: Option<&CrossSiteRequestForgeryToken>,
        origin: &str,
        now_unix_milliseconds: u64,
        caller_supplied: &[(String, String)],
    ) -> Result<Vec<(String, String)>, SubmissionRefusal> {
        if !self.reservation().is_whole() {
            return Err(SubmissionRefusal::PartialReservation);
        }
        require_caller_headers_permitted(caller_supplied)?;
        let mut headers = vec![
            (CONTENT_TYPE_HEADER.to_owned(), SUBMISSION_MEDIA_TYPE.to_owned()),
            (REFERER_HEADER.to_owned(), format!("{origin}/")),
            (IDEMPOTENCY_KEY_HEADER.to_owned(), self.submitted_command_digest.clone()),
        ];
        if let Some((name, value)) =
            header_for(SUBMISSION_METHOD, held_token, origin, now_unix_milliseconds)?
        {
            headers.push((name.to_owned(), value.to_owned()));
        }
        headers.extend(caller_supplied.iter().cloned());
        Ok(headers)
    }

    /// Requires this submission to be one that may be sent again after a crash.
    ///
    /// # Errors
    ///
    /// Returns [`SubmissionRefusal::GenerationChanged`] or
    /// [`SubmissionRefusal::ContinuationAuthorityAbsent`]. A generation change
    /// blocks resubmission rather than re-deriving: the persisted identifier
    /// belongs to the old generation, and pairing it with a new one names an
    /// operation nobody has.
    pub fn require_recoverable(
        &self,
        preconditions: &RecoveryPreconditions,
    ) -> Result<(), SubmissionRefusal> {
        if preconditions.current_generation != self.operation.agent_event_store_generation {
            return Err(SubmissionRefusal::GenerationChanged {
                current: preconditions.current_generation,
                persisted: self.operation.agent_event_store_generation,
            });
        }
        if !preconditions.continuation_authority_ready {
            return Err(SubmissionRefusal::ContinuationAuthorityAbsent);
        }
        Ok(())
    }

    /// Returns what one whole exchange means for this submission.
    ///
    /// Transport settles before content and content before identity: an answer
    /// read off a message this daemon cannot frame is not an answer.
    #[must_use]
    pub fn interpret(&self, exchange: &Exchange) -> SubmissionOutcome {
        if let Err(cause) = require_transport_clean(exchange) {
            return SubmissionOutcome::SubmissionUnknown { cause };
        }
        let class = classify_status(exchange.status);
        match class {
            StatusClass::Retryable => {
                return SubmissionOutcome::RetryAfter {
                    milliseconds: retry_delay_milliseconds(exchange.retry_after_milliseconds),
                };
            }
            StatusClass::Conflict => return SubmissionOutcome::Conflict,
            StatusClass::Unvalidated => {
                return SubmissionOutcome::SubmissionUnknown {
                    cause: UnknownCause::UnvalidatedStatus,
                };
            }
            StatusClass::Answered | StatusClass::Rejected => {}
        }
        let Some(acknowledgement) = &exchange.acknowledgement else {
            return SubmissionOutcome::SubmissionUnknown { cause: UnknownCause::Body };
        };
        if let Err(cause) = self.require_echoes(acknowledgement) {
            return SubmissionOutcome::SubmissionUnknown { cause };
        }
        self.settle(class, acknowledgement, exchange.elapsed_milliseconds)
    }

    /// Returns what a failed exchange means, given where it failed.
    ///
    /// Before the first request byte this is a proof; after it, it is not.
    #[must_use]
    pub fn transport_failure(checkpoint: Checkpoint) -> SubmissionOutcome {
        if checkpoint.bytes_may_have_reached_author() {
            SubmissionOutcome::SubmissionUnknown { cause: UnknownCause::Deadline(checkpoint) }
        } else {
            SubmissionOutcome::ConfirmedNotExecuted { checkpoint }
        }
    }

    /// Requires every echoed field to be the one that was sent.
    fn require_echoes(
        &self,
        acknowledgement: &SubmissionAcknowledgement,
    ) -> Result<(), UnknownCause> {
        if acknowledgement.agent_operation_identifier != self.operation.agent_operation_identifier {
            return Err(UnknownCause::Identity);
        }
        if acknowledgement.agent_event_store_generation
            != self.operation.agent_event_store_generation
        {
            return Err(UnknownCause::Generation);
        }
        if acknowledgement.author_target_identity_digest
            != self.operation.author_target_identity_digest
        {
            return Err(UnknownCause::Partition);
        }
        if acknowledgement.submitted_command_digest != self.submitted_command_digest {
            return Err(UnknownCause::Digest);
        }
        if acknowledgement.daemon_subscription_identifier != self.daemon_subscription_identifier {
            return Err(UnknownCause::Registration);
        }
        Ok(())
    }

    /// Returns the outcome of an answer whose echoes have all been checked.
    fn settle(
        &self,
        class: StatusClass,
        acknowledgement: &SubmissionAcknowledgement,
        elapsed_milliseconds: u64,
    ) -> SubmissionOutcome {
        if let Some(non_execution) = acknowledgement.non_execution {
            return if matches!(class, StatusClass::Rejected) {
                SubmissionOutcome::AuthoritativeNonExecution { non_execution }
            } else {
                SubmissionOutcome::SubmissionUnknown { cause: UnknownCause::UnvalidatedStatus }
            };
        }
        if matches!(class, StatusClass::Rejected) {
            return SubmissionOutcome::SubmissionUnknown { cause: UnknownCause::Body };
        }
        if acknowledgement.retired {
            return if acknowledgement.physical_sling_job_identifiers.is_empty() {
                SubmissionOutcome::RecoveryWindowExpired
            } else {
                SubmissionOutcome::SubmissionUnknown { cause: UnknownCause::Body }
            };
        }
        if let Err(cause) = require_bounded_job_set(&acknowledgement.physical_sling_job_identifiers)
        {
            return SubmissionOutcome::SubmissionUnknown { cause };
        }
        let Some(remaining_retention_milliseconds) = remaining_retention_milliseconds(
            acknowledgement.granted_retention_milliseconds,
            elapsed_milliseconds,
        ) else {
            return SubmissionOutcome::SubmissionUnknown { cause: UnknownCause::Retention };
        };
        let physical_sling_job_identifiers = acknowledgement.physical_sling_job_identifiers.clone();
        if acknowledgement.already_accepted {
            SubmissionOutcome::Duplicate {
                physical_sling_job_identifiers,
                remaining_retention_milliseconds,
            }
        } else {
            SubmissionOutcome::Accepted {
                physical_sling_job_identifiers,
                remaining_retention_milliseconds,
            }
        }
    }
}

/// Returns the digest binding contracts, arguments, and manifest together.
///
/// The domain owns the five fields and their ordering, so the manifest folds in
/// afterwards: a submission declaring different artifacts for the same
/// arguments differs without the contract identity moving.
fn bind_submitted_digest(
    expected: &ExpectedProvenance,
    canonical_arguments: &str,
    manifest: ExpectedArtifactManifest,
) -> String {
    use sha2::{Digest, Sha256};
    let contract_digest = SubmittedCommandDigest::derive(
        &expected.command_contract,
        &expected.canonical_json_contract_digest,
        &expected.transport_contract_digest,
        canonical_arguments,
    );
    let mut hasher = Sha256::new();
    for field in [
        SUBMISSION_BINDING_VERSION,
        contract_digest.as_text(),
        manifest.kind.as_text(),
        &manifest.artifact_rows.to_string(),
        &manifest.artifact_bytes.to_string(),
    ] {
        hasher.update(field.as_bytes());
        hasher.update([FIELD_SEPARATOR]);
    }
    hasher.finalize().iter().map(|octet| format!("{octet:02x}")).collect()
}

/// Requires caller headers to be distinct and to set nothing derived.
fn require_caller_headers_permitted(
    caller_supplied: &[(String, String)],
) -> Result<(), SubmissionRefusal> {
    for (position, (name, _)) in caller_supplied.iter().enumerate() {
        if RESERVED_HEADERS.iter().any(|reserved| reserved.eq_ignore_ascii_case(name)) {
            return Err(SubmissionRefusal::ReservedHeader(name.clone()));
        }
        if caller_supplied
            .iter()
            .take(position)
            .any(|(earlier, _)| earlier.eq_ignore_ascii_case(name))
        {
            return Err(SubmissionRefusal::DuplicateHeader(name.clone()));
        }
    }
    Ok(())
}

/// Requires one message to be framed and typed the way an answer must be.
fn require_transport_clean(exchange: &Exchange) -> Result<(), UnknownCause> {
    exchange.head.require_acceptable().map_err(UnknownCause::from)?;
    if exchange.trailer_section_present {
        return Err(UnknownCause::TrailerSection);
    }
    if exchange.framing_ambiguous {
        return Err(UnknownCause::Framing);
    }
    if exchange.trailing_bytes {
        return Err(UnknownCause::TrailingBytes);
    }
    if exchange.media_type != SUBMISSION_MEDIA_TYPE {
        return Err(UnknownCause::Media);
    }
    let allowed =
        AuthorAgentTransportContract::embedded().limit("maximum_finite_response_body_bytes");
    if exchange.body_bytes > allowed {
        return Err(UnknownCause::Body);
    }
    if exchange.unknown_fields {
        return Err(UnknownCause::UnknownField);
    }
    Ok(())
}

/// Requires an acknowledged job set to be one this daemon can act on.
///
/// Non-empty, distinct, sorted, bounded. Sortedness is checked rather than
/// imposed: the set is compared across recoveries, and sorting hides that the
/// agent answered differently.
fn require_bounded_job_set(identifiers: &[String]) -> Result<(), UnknownCause> {
    let contract = AuthorAgentTransportContract::embedded();
    let allowed_matches = contract.limit("maximum_physical_sling_job_matches");
    let allowed_bytes = contract.limit("maximum_sling_job_identifier_bytes");
    if identifiers.is_empty() {
        return Err(UnknownCause::EmptyIdentifier);
    }
    if u64::try_from(identifiers.len()).unwrap_or(u64::MAX) > allowed_matches {
        return Err(UnknownCause::Body);
    }
    if !identifiers.is_sorted_by(|earlier, later| earlier < later) {
        return Err(UnknownCause::Body);
    }
    if identifiers.iter().any(|identifier| {
        identifier.is_empty() || u64::try_from(identifier.len()).unwrap_or(u64::MAX) > allowed_bytes
    }) {
        return Err(UnknownCause::EmptyIdentifier);
    }
    Ok(())
}

/// Returns how much of a granted retention is left, counted from request start.
///
/// Counted from before any network work, because a slow answer spends the
/// retention it describes. An exactly exhausted retention is expired: zero
/// remaining time promises nothing, and a caller would hunt vanished results.
#[must_use]
pub fn remaining_retention_milliseconds(
    granted_milliseconds: u64,
    elapsed_since_request_start_milliseconds: u64,
) -> Option<u64> {
    let cap = AuthorAgentTransportContract::embedded()
        .limit("maximum_persisted_remaining_retention_milliseconds");
    if elapsed_since_request_start_milliseconds >= granted_milliseconds {
        return None;
    }
    Some((granted_milliseconds - elapsed_since_request_start_milliseconds).min(cap))
}
