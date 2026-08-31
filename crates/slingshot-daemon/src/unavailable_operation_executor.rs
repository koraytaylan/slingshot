//! The only executor a product build composes: one that runs nothing.
//!
//! Plan 0004 builds the daemon that will run operations, and does not build the
//! thing that talks to a remote system. So the product binary composes an
//! executor that refuses, and refuses in a way that is a fact rather than a
//! surprise: a stable terminal failure saying the work provably did not run.
//!
//! Refusing before admission matters more than refusing at all. An operation
//! that reached a row and then failed would be an operation a client could
//! find, wait on, and reasonably ask about, all describing work no part of this
//! build can do. Nothing is admitted, so there is nothing to find.
//!
//! A test double is deliberately absent from this crate. The fake lives in test
//! support and is composed only by the development binary, so no product build
//! contains a code path that could execute anything.

use slingshot_domain::operation::{
    OperationExecutionCertainty, TerminalFailure, TerminalFailureDisposition, TerminalFailureKind,
};
use slingshot_domain::operation_executor::{
    ExecutionIdentity, OperationExecutor, OperationExecutorOutcome, ProgressPort,
};

/// What this executor tells a caller, in words rather than a code.
pub const UNAVAILABLE_DETAIL: &str =
    "this build composes no operation executor, so the command provably did not run";

/// An executor that admits nothing and runs nothing.
#[derive(Debug, Clone, Copy, Default)]
pub struct UnavailableOperationExecutor;

impl UnavailableOperationExecutor {
    /// Returns the outcome this executor always produces.
    ///
    /// `Rejected` with `AuthoritativeNonExecution` and `ConfirmedNotExecuted`,
    /// which is the honest pairing: nothing was submitted anywhere, so the
    /// certainty about whether it ran is not uncertainty at all.
    #[must_use]
    pub fn outcome() -> OperationExecutorOutcome {
        OperationExecutorOutcome::TerminalFailure {
            failure: TerminalFailure {
                disposition: TerminalFailureDisposition::AuthoritativeNonExecution {
                    certainty: OperationExecutionCertainty::ConfirmedNotExecuted,
                },
                kind: TerminalFailureKind::Rejected,
                metadata: Some(UNAVAILABLE_DETAIL.to_owned()),
            },
        }
    }
}

impl OperationExecutor for UnavailableOperationExecutor {
    fn execute(
        &self,
        _identity: &ExecutionIdentity,
        _command: &slingshot_domain::command::catalog::Command,
        _progress: &dyn ProgressPort,
    ) -> OperationExecutorOutcome {
        Self::outcome()
    }
}
