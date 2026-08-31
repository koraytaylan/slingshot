//! Turning an outcome into an exit status a script can branch on.
//!
//! The distinctions kept here are the ones a caller must act on differently.
//! An agent that refused the work, an agent that ran it and failed, an outcome
//! nobody can settle, and a success whose result is gone are four different
//! situations, and a script that could not tell them apart would retry the one
//! that must not be retried.
//!
//! # The exit comes from the disposition and never from a name
//!
//! What a failure is called is a category the agent chose from a closed list;
//! what it means for execution is a disposition the daemon derived. Choosing an
//! exit by matching a category's spelling would make a renamed category change
//! a script's behaviour, and would make two categories that mean the same thing
//! exit differently.
//!
//! # The failure object survives the classification
//!
//! Nothing here replaces, summarizes, or drops what the agent reported. The
//! exit is an additional fact for a script that cannot parse; the object is
//! still there for one that can.

/// Everything went as asked.
pub const SUCCESS: i32 = 0;

/// The invocation itself was wrong.
pub const USAGE: i32 = 2;

/// The agent refused the work, and provably ran nothing.
pub const AGENT_REJECTION: i32 = 3;

/// The agent ran it and it failed.
pub const REMOTE_FAILURE: i32 = 4;

/// Nobody can say what happened.
pub const INDETERMINATE: i32 = 5;

/// It succeeded and what it produced can no longer be had.
pub const UNAVAILABLE: i32 = 6;

/// Something local went wrong before anything was settled.
pub const LOCAL_FAILURE: i32 = 7;

/// A person interrupted it.
pub const INTERRUPTED: i32 = 130;

/// Every exit this build returns, in ascending order.
pub const EVERY_EXIT: &[i32] = &[
    SUCCESS,
    USAGE,
    AGENT_REJECTION,
    REMOTE_FAILURE,
    INDETERMINATE,
    UNAVAILABLE,
    LOCAL_FAILURE,
    INTERRUPTED,
];

/// What the daemon says an ending means for execution.
///
/// The closed set this build classifies from. A disposition it did not know
/// would be an outcome it could not honestly exit on, which is why the set is
/// closed rather than open with a fallback.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TerminalDisposition {
    /// It provably did not run.
    AuthoritativeNonExecution,
    /// It ran and failed, possibly with partial effects.
    AuthoritativeRemoteFailure,
    /// It ran and succeeded, and something after that did not.
    AuthoritativeRemoteSuccess,
    /// Nobody can tell, so the daemon failed closed.
    FailClosedIndeterminate,
}

/// Returns the exit one terminal disposition produces.
///
/// The disposition alone. The category the agent named travels with the answer
/// and never chooses the exit, so renaming one changes no script's behaviour.
#[must_use]
pub fn exit_for(disposition: TerminalDisposition) -> i32 {
    match disposition {
        TerminalDisposition::AuthoritativeNonExecution => AGENT_REJECTION,
        TerminalDisposition::AuthoritativeRemoteFailure => REMOTE_FAILURE,
        TerminalDisposition::AuthoritativeRemoteSuccess => UNAVAILABLE,
        TerminalDisposition::FailClosedIndeterminate => INDETERMINATE,
    }
}

/// Returns whether an exit tells a script it may run the command again.
///
/// Only two. A refusal ran nothing, so the same request may go again once
/// whatever it refused for is fixed; a usage mistake never reached anything. An
/// indeterminate outcome may not, because running it again is exactly the risk
/// the disposition exists to describe.
#[must_use]
pub fn permits_another_attempt(exit: i32) -> bool {
    exit == AGENT_REJECTION || exit == USAGE
}
