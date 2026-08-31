//! Where a conversation with an author can break, and what that leaves known.
//!
//! Every fault here is a place in one request where the connection can stop,
//! and what matters about each is not the failure but the honest answer
//! afterwards: whether the command provably did not run, whether it may have
//! run, or whether it ran and its answer was lost. Those three are different
//! facts with different consequences, and a transport that collapsed them would
//! make a caller either repeat work or abandon it.
//!
//! # The dividing line is the first byte of the request
//!
//! Nothing sent means nothing ran. The moment any of the request has left, the
//! far side may have acted on it, and no amount of local evidence can say
//! otherwise - so the answer changes from a fact into a question, and the only
//! honest thing to do is go and look.

/// Where one conversation stops.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum NetworkFault {
    /// The name could not be resolved.
    NameUnresolved,
    /// The connection was refused.
    ConnectionRefused,
    /// The secure handshake failed.
    HandshakeFailed,
    /// The connection died before any request byte was written.
    BeforeRequestBytes,
    /// It died after some request bytes were written.
    AfterRequestBytes,
    /// It died after the whole request was written and before any answer.
    AfterRequestComplete,
    /// It died after the answer's head arrived and before its body.
    AfterResponseHead,
    /// It died partway through the answer's body.
    DuringResponseBody,
    /// The answer arrived whole and the connection died before it was recorded.
    AfterCompleteResponse,
}

/// Every fault, so a suite walks them rather than remembers them.
pub const EVERY_NETWORK_FAULT: &[NetworkFault] = &[
    NetworkFault::NameUnresolved,
    NetworkFault::ConnectionRefused,
    NetworkFault::HandshakeFailed,
    NetworkFault::BeforeRequestBytes,
    NetworkFault::AfterRequestBytes,
    NetworkFault::AfterRequestComplete,
    NetworkFault::AfterResponseHead,
    NetworkFault::DuringResponseBody,
    NetworkFault::AfterCompleteResponse,
];

/// What is honestly known about execution after one fault.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExecutionKnowledge {
    /// It provably did not run.
    ConfirmedNotExecuted,
    /// Whether it was submitted is unknown.
    SubmissionUnknown,
    /// It was submitted and its outcome is unknown.
    RemoteOutcomeUnknown,
}

/// Which phase of one request a fault belongs to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RequestPhase {
    /// Finding and reaching the far side.
    Connecting,
    /// Establishing that it is the far side it claims to be.
    Securing,
    /// Sending.
    Requesting,
    /// Receiving.
    Responding,
}

impl NetworkFault {
    /// Returns what is honestly known about execution after this fault.
    #[must_use]
    pub fn knowledge(self) -> ExecutionKnowledge {
        match self {
            Self::NameUnresolved
            | Self::ConnectionRefused
            | Self::HandshakeFailed
            | Self::BeforeRequestBytes => ExecutionKnowledge::ConfirmedNotExecuted,
            Self::AfterRequestBytes | Self::AfterRequestComplete => {
                ExecutionKnowledge::SubmissionUnknown
            }
            Self::AfterResponseHead | Self::DuringResponseBody | Self::AfterCompleteResponse => {
                ExecutionKnowledge::RemoteOutcomeUnknown
            }
        }
    }

    /// Returns which phase this fault belongs to.
    ///
    /// Connecting and securing are separate on purpose. They fail for different
    /// reasons, they are worth different deadlines, and a transport that timed
    /// them together would blame a slow network for a refused certificate.
    #[must_use]
    pub fn phase(self) -> RequestPhase {
        match self {
            Self::NameUnresolved | Self::ConnectionRefused => RequestPhase::Connecting,
            Self::HandshakeFailed => RequestPhase::Securing,
            Self::BeforeRequestBytes | Self::AfterRequestBytes | Self::AfterRequestComplete => {
                RequestPhase::Requesting
            }
            Self::AfterResponseHead | Self::DuringResponseBody | Self::AfterCompleteResponse => {
                RequestPhase::Responding
            }
        }
    }

    /// Returns whether a caller may simply send the request again.
    ///
    /// Only when nothing was sent. Anything else has to be reconciled by asking
    /// the far side what it knows, because sending again could be the second
    /// time.
    #[must_use]
    pub fn permits_plain_retry(self) -> bool {
        self.knowledge() == ExecutionKnowledge::ConfirmedNotExecuted
    }

    /// Returns whether recovery has to look the operation up before deciding.
    #[must_use]
    pub fn requires_lookup(self) -> bool {
        !self.permits_plain_retry()
    }
}

/// One scripted conversation, and where it breaks.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NetworkFaultScript {
    /// Where it breaks.
    pub fault: NetworkFault,
    /// How many times it breaks before it is allowed to work.
    pub occurrences: u32,
}

impl NetworkFaultScript {
    /// Returns a script that breaks once and then works.
    #[must_use]
    pub fn once(fault: NetworkFault) -> Self {
        Self { fault, occurrences: 1 }
    }

    /// Returns whether attempt `attempt` is one this script breaks.
    #[must_use]
    pub fn breaks_on(&self, attempt: u32) -> bool {
        attempt < self.occurrences
    }
}
