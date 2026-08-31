//! The token an author requires, and the one thing it actually protects against.
//!
//! Cross-site request forgery is a browser problem: a page somewhere else
//! causes a browser to send a request carrying credentials the browser already
//! holds. A daemon is not a browser and has no ambient credentials to be
//! abused, so this protection does nothing for it. It is implemented anyway,
//! exactly, because the author requires it and a request without it is refused.
//!
//! That framing matters for what the code does. The token is fetched, presented
//! and refreshed because the far side asks, not because anything here believes
//! it is safer for having done so - so there is no partial credit, no fallback,
//! and no place where a missing token is worked around.
//!
//! The two deployment eras ask for it slightly differently, and the difference
//! is the context prefix in front of the route. A client that guessed would
//! fetch from a path that does not exist on one of them, so the prefix is part
//! of what is configured rather than something to discover by trying.

/// The route a token is fetched from, after any context prefix.
pub const TOKEN_ROUTE: &str = "/libs/granite/csrf/token";

/// The header a token is presented in.
pub const TOKEN_HEADER: &str = "CSRF-Token";

/// Methods that require a token.
///
/// The ones that change something. A token on a read would be ceremony: there
/// is nothing for a forged read to do that reading normally would not.
pub const PROTECTED_METHODS: &[&str] = &["POST", "PUT", "PATCH", "DELETE"];

/// Which deployment era an author is.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DeploymentEra {
    /// A managed deployment, served from the root.
    Cloud,
    /// A deployment that may sit behind a context prefix.
    ManagedServices {
        /// The prefix in front of every route, without a trailing separator.
        context_prefix: String,
    },
}

impl DeploymentEra {
    /// Returns where a token is fetched from on this deployment.
    ///
    /// Built from what is configured rather than discovered by trying, because
    /// a client that guessed would fetch from a path that does not exist on one
    /// era and interpret the refusal as something else.
    #[must_use]
    pub fn token_route(&self) -> String {
        match self {
            Self::Cloud => TOKEN_ROUTE.to_owned(),
            Self::ManagedServices { context_prefix } => format!("{context_prefix}{TOKEN_ROUTE}"),
        }
    }
}

/// Why a request could not carry a token.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum TokenFailure {
    /// No token has been fetched.
    #[error("this request changes something and no token has been fetched")]
    Absent,
    /// The token has expired.
    #[error("the token expired at {expired_at}, and an expired token is refused rather than sent")]
    Expired {
        /// When it expired.
        expired_at: u64,
    },
    /// The token was fetched for another origin.
    #[error("this token was fetched for another author, and presenting it there would leak it")]
    AnotherOrigin,
}

/// One token, and how long it lasts.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CrossSiteRequestForgeryToken {
    /// When it stops being presented.
    pub expires_at_unix_milliseconds: u64,
    /// Which author it was fetched from.
    pub origin: String,
    /// The value, which is presented and never logged.
    pub value: String,
}

impl CrossSiteRequestForgeryToken {
    /// Returns the header value one request presents, if this token may be.
    ///
    /// # Errors
    ///
    /// Returns [`TokenFailure::Expired`] or [`TokenFailure::AnotherOrigin`].
    pub fn present_to(
        &self,
        origin: &str,
        now_unix_milliseconds: u64,
    ) -> Result<(&'static str, &str), TokenFailure> {
        if self.origin != origin {
            return Err(TokenFailure::AnotherOrigin);
        }
        if now_unix_milliseconds >= self.expires_at_unix_milliseconds {
            return Err(TokenFailure::Expired { expired_at: self.expires_at_unix_milliseconds });
        }
        Ok((TOKEN_HEADER, &self.value))
    }
}

/// Returns whether a request using `method` needs a token.
#[must_use]
pub fn requires_token(method: &str) -> bool {
    PROTECTED_METHODS.contains(&method)
}

/// Returns the token a request must present, or why it cannot.
///
/// A request that changes something and has no token is refused here rather
/// than sent and refused by the author. Sending it would mean the author has to
/// decide, and an author deciding is one more place the request could be
/// interpreted before being rejected.
///
/// # Errors
///
/// Returns [`TokenFailure`] naming why nothing can be presented.
pub fn header_for<'token>(
    method: &str,
    held: Option<&'token CrossSiteRequestForgeryToken>,
    origin: &str,
    now_unix_milliseconds: u64,
) -> Result<Option<(&'static str, &'token str)>, TokenFailure> {
    if !requires_token(method) {
        return Ok(None);
    }
    let token = held.ok_or(TokenFailure::Absent)?;
    token.present_to(origin, now_unix_milliseconds).map(Some)
}
