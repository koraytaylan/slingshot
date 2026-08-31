//! What another process on this machine can try against a daemon's endpoint.
//!
//! The endpoint is reachable by anything running as this account and, on a
//! misconfigured machine, by more than that. So the attacks worth modelling are
//! the ones another local process can actually mount: connecting as somebody
//! else, quoting a nonce it saw earlier, connecting from off the machine
//! entirely, and holding connections open to keep everybody else out.
//!
//! # Authority is a live nonce and nothing else
//!
//! Being able to connect proves only that the socket exists. Every request that
//! changes anything quotes the nonce the live daemon published, which an
//! attacker can only have if it could already read the runtime state - at which
//! point the endpoint is not what is protecting anything.

/// What another local process tries.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum LocalAttack {
    /// Connecting and asking what is there.
    Probe,
    /// Stopping the daemon while quoting nothing.
    StopWithoutNonce,
    /// Stopping it while quoting a nonce a previous instance published.
    StopWithStaleNonce,
    /// Stopping it while quoting the nonce the live instance published.
    StopWithLiveNonce,
    /// Connecting from another machine.
    ConnectRemotely,
    /// Holding every connection the daemon admits.
    ExhaustConnections,
}

/// Every attack, so a suite walks them rather than remembers them.
pub const EVERY_LOCAL_ATTACK: &[LocalAttack] = &[
    LocalAttack::Probe,
    LocalAttack::StopWithoutNonce,
    LocalAttack::StopWithStaleNonce,
    LocalAttack::StopWithLiveNonce,
    LocalAttack::ConnectRemotely,
    LocalAttack::ExhaustConnections,
];

/// What one attack achieves.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AttackOutcome {
    /// It is answered, because answering it gives nothing away.
    Answered,
    /// It is refused, and nothing changes.
    Refused,
    /// It succeeds, because it is what the mechanism is for.
    Authorized,
    /// It cannot reach the endpoint at all.
    Unreachable,
}

impl LocalAttack {
    /// Returns what this attack achieves against a daemon that is serving.
    #[must_use]
    pub fn outcome(self) -> AttackOutcome {
        match self {
            Self::Probe => AttackOutcome::Answered,
            Self::StopWithoutNonce | Self::StopWithStaleNonce => AttackOutcome::Refused,
            Self::StopWithLiveNonce => AttackOutcome::Authorized,
            Self::ConnectRemotely => AttackOutcome::Unreachable,
            Self::ExhaustConnections => AttackOutcome::Refused,
        }
    }

    /// Returns whether this attack changes anything durable.
    #[must_use]
    pub fn changes_durable_state(self) -> bool {
        matches!(self, Self::StopWithLiveNonce)
    }

    /// Returns whether the daemon stays usable by everybody else afterwards.
    ///
    /// Every attack but the authorized stop leaves it serving. That is the
    /// property worth having: an attacker who cannot produce the live nonce can
    /// waste a daemon's time and nothing else.
    #[must_use]
    pub fn leaves_the_daemon_serving(self) -> bool {
        !self.changes_durable_state()
    }
}
