//! The private supervision channel of one detached test daemon.
//!
//! A test that starts a daemon keeps the exact child handle, unreaped, until it
//! makes one disposition: it observes that the child already exited, or it
//! terminates the child through that same handle and waits for it. Nothing here
//! looks a process up, checks its identity, and then signals it, so a
//! replacement that happens to reuse a numeric process identifier can never be
//! reached. The token is unguessable and bound to one instance, so a token kept
//! from an earlier daemon cannot redirect a disposition either.

use std::process::{Child, ExitStatus};
use std::time::{Duration, Instant};

use rand::{CryptoRng, RngExt};

/// Interval between two polls while waiting for a terminated child to be reaped.
const REAP_POLL_INTERVAL: Duration = Duration::from_millis(5);

/// An unguessable value that names one supervised child.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SupervisionToken(u128);

impl SupervisionToken {
    /// Draws one token from a cryptographically strong generator.
    fn draw(generator: &mut (impl RngExt + CryptoRng)) -> Self {
        let mut bytes = [0_u8; size_of::<u128>()];
        generator.fill(&mut bytes[..]);
        Self(u128::from_le_bytes(bytes))
    }
}

/// How one supervised child reached its end.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Disposition {
    /// The child had already exited when the disposition was made.
    AlreadyExited(ExitStatus),
    /// The child was terminated through its handle and then waited for.
    Terminated(ExitStatus),
}

/// Reason a disposition could not be made.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum SupervisionFailure {
    /// The supplied token does not name this child.
    #[error("the supplied supervision token does not name this child")]
    ForeignToken,
    /// This child has already been disposed of once.
    #[error("this child has already been disposed of")]
    AlreadyDisposed,
    /// The child did not finish inside the supplied deadline.
    #[error("the child did not finish within {0:?}")]
    DeadlineElapsed(Duration),
    /// The operating system refused the disposition.
    #[error("the child could not be disposed of: {0}")]
    Refused(String),
}

/// One detached child a test keeps until it makes exactly one disposition.
#[derive(Debug)]
pub struct SupervisedChild {
    child: Child,
    token: SupervisionToken,
    process_identifier: u32,
    disposed: bool,
}

impl SupervisedChild {
    /// Adopts one already-started child.
    ///
    /// The child is retained unreaped from this point until one disposition is
    /// made, so its handle stays valid and its identity cannot be recycled.
    #[must_use]
    pub fn adopt(child: Child) -> Self {
        let process_identifier = child.id();
        let mut generator = rand::rng();
        Self {
            child,
            token: SupervisionToken::draw(&mut generator),
            process_identifier,
            disposed: false,
        }
    }

    /// Returns the token that names this child.
    #[must_use]
    pub fn token(&self) -> SupervisionToken {
        self.token
    }

    /// Returns the child's numeric process identifier, for output correlation.
    ///
    /// The value is a diagnostic. No path in this module looks it up, compares
    /// it, or signals it.
    #[must_use]
    pub fn process_identifier(&self) -> u32 {
        self.process_identifier
    }

    /// Reports whether a disposition has already been made.
    #[must_use]
    pub fn is_disposed(&self) -> bool {
        self.disposed
    }

    /// Makes the one disposition this child accepts.
    ///
    /// An already-exited child is recorded without anything being signalled. A
    /// running child is terminated through this handle and then waited for,
    /// inside `deadline`.
    ///
    /// # Errors
    ///
    /// Returns [`SupervisionFailure::ForeignToken`] for a token that names
    /// another child, [`SupervisionFailure::AlreadyDisposed`] for a second
    /// disposition, [`SupervisionFailure::DeadlineElapsed`] when the child does
    /// not finish in time, and [`SupervisionFailure::Refused`] when the
    /// operating system refuses.
    pub fn dispose(
        &mut self,
        token: SupervisionToken,
        deadline: Duration,
    ) -> Result<Disposition, SupervisionFailure> {
        if token != self.token {
            return Err(SupervisionFailure::ForeignToken);
        }
        if self.disposed {
            return Err(SupervisionFailure::AlreadyDisposed);
        }
        self.disposed = true;
        if let Some(status) = self
            .child
            .try_wait()
            .map_err(|failure| SupervisionFailure::Refused(failure.to_string()))?
        {
            return Ok(Disposition::AlreadyExited(status));
        }
        self.child.kill().map_err(|failure| SupervisionFailure::Refused(failure.to_string()))?;
        self.wait_for_exit(deadline).map(Disposition::Terminated)
    }

    /// Waits for the terminated child through its own handle.
    fn wait_for_exit(&mut self, deadline: Duration) -> Result<ExitStatus, SupervisionFailure> {
        let started = Instant::now();
        loop {
            match self.child.try_wait() {
                Err(failure) => return Err(SupervisionFailure::Refused(failure.to_string())),
                Ok(Some(status)) => return Ok(status),
                Ok(None) if started.elapsed() >= deadline => {
                    return Err(SupervisionFailure::DeadlineElapsed(deadline));
                }
                Ok(None) => std::thread::sleep(REAP_POLL_INTERVAL),
            }
        }
    }
}
