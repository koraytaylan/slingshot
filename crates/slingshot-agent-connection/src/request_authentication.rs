//! Presenting a credential to an author, once, and never keeping it anywhere.
//!
//! Two provider kinds, and the difference is where the credential comes from
//! rather than what a request does with it. A Basic credential is in the
//! selected snapshot already; a Bearer token is exchanged for and expires. So
//! only the second can fail in a way a retry might fix, and only the second has
//! anything to refresh.
//!
//! One refresh per request, and that bound is the point. A token that is
//! rejected immediately after being obtained is not a token that will work on
//! the third attempt: something else is wrong, and retrying is a way of turning
//! one failed request into a burst of them against a system that is already
//! refusing.
//!
//! Nothing here renders a credential. Failures name the provider and the
//! outcome, never the value, because a diagnostic that carried one would put it
//! in exactly the places diagnostics go.

/// Which kind of credential an author expects.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProviderKind {
    /// A credential the selected snapshot already holds.
    Basic,
    /// A token exchanged for, which expires and can be refreshed.
    Bearer,
}

impl ProviderKind {
    /// Returns the authorization scheme this kind presents.
    #[must_use]
    pub fn scheme(self) -> &'static str {
        match self {
            Self::Basic => "Basic",
            Self::Bearer => "Bearer",
        }
    }

    /// Returns whether a rejected credential of this kind can be refreshed.
    ///
    /// Only a token can. A Basic credential that the author rejects is the
    /// wrong credential, and asking the snapshot for it again produces the same
    /// one - so retrying would be retrying something already known to fail.
    #[must_use]
    pub fn is_refreshable(self) -> bool {
        matches!(self, Self::Bearer)
    }
}

/// How many times one request may refresh its credential.
pub const MAXIMUM_REFRESHES_PER_REQUEST: u32 = 1;

/// Why a request could not be authenticated.
///
/// Every variant names the provider and what happened, and none of them can
/// carry a credential: there is nowhere in these shapes to put one.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum AuthenticationFailure {
    /// The provider could not supply a credential at all.
    #[error("the {kind:?} provider could not supply a credential")]
    ProviderUnavailable {
        /// Which provider.
        kind: ProviderKind,
    },
    /// The author rejected the credential, and it cannot be refreshed.
    #[error("the author rejected the {kind:?} credential, and this kind has nothing to refresh")]
    RejectedAndNotRefreshable {
        /// Which provider.
        kind: ProviderKind,
    },
    /// The author rejected a freshly obtained credential.
    #[error(
        "the author rejected a freshly obtained {kind:?} credential; something other than \
         expiry is wrong, and retrying would make one failed request into several"
    )]
    RejectedAfterRefresh {
        /// Which provider.
        kind: ProviderKind,
    },
}

/// What one attempt at an authenticated request produced.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AttemptOutcome {
    /// The author accepted it.
    Accepted,
    /// The author rejected the credential.
    CredentialRejected,
}

/// One request presenting a credential, and refreshing at most once.
#[derive(Debug)]
pub struct AuthenticatedRequest {
    /// Which provider supplies the credential.
    kind: ProviderKind,
    /// How many times this request has refreshed.
    refreshes: u32,
}

impl AuthenticatedRequest {
    /// Returns a request that has not presented anything yet.
    #[must_use]
    pub fn using(kind: ProviderKind) -> Self {
        Self { kind, refreshes: 0 }
    }

    /// Returns which provider supplies this request's credential.
    #[must_use]
    pub fn kind(&self) -> ProviderKind {
        self.kind
    }

    /// Returns how many times this request has refreshed.
    #[must_use]
    pub fn refreshes(&self) -> u32 {
        self.refreshes
    }

    /// Decides what to do about `outcome`.
    ///
    /// # Errors
    ///
    /// Returns [`AuthenticationFailure`] when there is nothing further to try.
    pub fn observe(&mut self, outcome: AttemptOutcome) -> Result<Retry, AuthenticationFailure> {
        match outcome {
            AttemptOutcome::Accepted => Ok(Retry::Done),
            AttemptOutcome::CredentialRejected if !self.kind.is_refreshable() => {
                Err(AuthenticationFailure::RejectedAndNotRefreshable { kind: self.kind })
            }
            AttemptOutcome::CredentialRejected
                if self.refreshes >= MAXIMUM_REFRESHES_PER_REQUEST =>
            {
                Err(AuthenticationFailure::RejectedAfterRefresh { kind: self.kind })
            }
            AttemptOutcome::CredentialRejected => {
                self.refreshes += 1;
                Ok(Retry::AfterRefreshing)
            }
        }
    }
}

/// What a request does next.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Retry {
    /// Nothing; the author accepted it.
    Done,
    /// Obtain a fresh credential and present it once more.
    AfterRefreshing,
}

/// Returns the header value a credential of `kind` is presented in.
///
/// Takes the encoded credential rather than the parts of one, so nothing here
/// ever holds a user name, a password, or a token in a form it could
/// accidentally render. The value is written and dropped.
#[must_use]
pub fn authorization_header(kind: ProviderKind, encoded_credential: &str) -> String {
    format!("{} {encoded_credential}", kind.scheme())
}
