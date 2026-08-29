//! In-memory lease of one cloud access token.
//!
//! A token is worth exactly as much as the credentials that produced it, so it
//! lives only in this process's memory, is never written down, and is replaced
//! rather than shared when it stops being usable.
//!
//! Two callers must never produce two exchanges for the same reason. A
//! scheduled refresh and an unauthorized response are the same need, so both
//! join one flight: the first caller exchanges while the rest wait, and they
//! find the replacement rather than each asking for their own. Otherwise a
//! server that rejects one request would receive as many exchanges as there are
//! callers, which is how a rejection becomes an outage.
//!
//! A lease names the generation it was taken from. That is what makes a
//! forced refresh safe to retry: a caller holding a stale lease has already
//! been given the replacement and is told so, rather than evicting a token
//! somebody else just installed.

use std::sync::Mutex;

use slingshot_domain::profile_authentication_contract::ConfigurationFailureCode;

use crate::authentication::identity_management_exchange::{AccessToken, ExchangeFailure};

/// Produces one access token, exchanging when asked.
///
/// The cache never builds an exchange of its own, so a test scripts what an
/// exchange produces - including a failure - without a network.
pub trait AccessTokenSource {
    /// Exchanges once and returns what came back.
    ///
    /// # Errors
    ///
    /// Returns whatever the exchange itself refused with.
    fn exchange(&self) -> Result<AccessToken, ExchangeFailure>;
}

/// The identity of one cache.
///
/// It is opaque and unrelated to any credential byte, so two snapshots never
/// share a cell and nothing about a token can be inferred from it. It has no
/// rendering and no serialization on purpose: an identity that could be written
/// down would end up somewhere it could be correlated.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AccessTokenCacheIdentity {
    /// A value that is unique within this process.
    value: u64,
}

/// A caller's claim on one installed generation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AccessTokenLease {
    /// Cache the lease was taken from.
    identity: AccessTokenCacheIdentity,
    /// Generation the lease names.
    generation: u64,
}

impl AccessTokenLease {
    /// Returns the generation this lease names.
    #[must_use]
    pub fn generation(&self) -> u64 {
        self.generation
    }
}

/// What one cache currently holds.
#[derive(Debug, Default)]
struct CacheState {
    /// The installed token, when one is installed.
    installed: Option<AccessToken>,
    /// Generation the installed token belongs to.
    generation: u64,
    /// Whether the installed generation may still be used.
    usable: bool,
}

/// One process-memory cache of cloud access tokens.
#[derive(Debug)]
pub struct CloudAccessTokenCache {
    /// Identity of this cache.
    identity: AccessTokenCacheIdentity,
    /// The cell every caller synchronizes through.
    state: Mutex<CacheState>,
}

impl CloudAccessTokenCache {
    /// Returns an empty cache with `identity`.
    ///
    /// The identity is supplied rather than generated so a concurrency test is
    /// deterministic without giving anything a way to compare two caches by
    /// their contents.
    #[must_use]
    pub fn with_identity(identity: u64) -> Self {
        Self {
            identity: AccessTokenCacheIdentity { value: identity },
            state: Mutex::new(CacheState::default()),
        }
    }

    /// Returns the identity of this cache.
    #[must_use]
    pub fn identity(&self) -> AccessTokenCacheIdentity {
        self.identity
    }

    /// Returns the token to use at `reading`, exchanging if one is needed.
    ///
    /// # Errors
    ///
    /// Returns whatever the exchange refused with, and
    /// [`ConfigurationFailureCode::AccessTokenInstallationGenerationExhausted`]
    /// when the generation counter has no room left.
    pub fn token<Outcome>(
        &self,
        reading: u64,
        source: &dyn AccessTokenSource,
        use_token: impl FnOnce(&AccessToken) -> Outcome,
    ) -> Result<(Outcome, AccessTokenLease), ExchangeFailure> {
        let mut state = self.locked();
        let fresh = state
            .installed
            .as_ref()
            .is_some_and(|token| state.usable && !token.refresh_required(reading));
        if !fresh {
            self.install(&mut state, source)?;
        }
        let token = state.installed.as_ref().ok_or_else(exhausted)?;
        Ok((use_token(token), self.lease(state.generation)))
    }

    /// Replaces the generation `lease` names, if it is still the current one.
    ///
    /// A caller whose request was rejected asks for this. A caller holding a
    /// stale lease has already been given the replacement, so it is handed that
    /// generation rather than being allowed to evict a token that is not the
    /// one its request used.
    ///
    /// # Errors
    ///
    /// Returns whatever the exchange refused with, and
    /// [`ConfigurationFailureCode::AccessTokenInstallationGenerationExhausted`]
    /// when the generation counter has no room left.
    pub fn refresh_after_unauthorized<Outcome>(
        &self,
        lease: AccessTokenLease,
        source: &dyn AccessTokenSource,
        use_token: impl FnOnce(&AccessToken) -> Outcome,
    ) -> Result<(Outcome, AccessTokenLease), ExchangeFailure> {
        let mut state = self.locked();
        let current = lease.identity == self.identity && lease.generation == state.generation;
        if current && state.usable {
            state.usable = false;
            if let Some(token) = state.installed.as_mut() {
                token.scrub();
            }
        }
        if !state.usable {
            self.install(&mut state, source)?;
        }
        let token = state.installed.as_ref().ok_or_else(exhausted)?;
        Ok((use_token(token), self.lease(state.generation)))
    }

    /// Exchanges once and installs what came back as the next generation.
    fn install(
        &self,
        state: &mut CacheState,
        source: &dyn AccessTokenSource,
    ) -> Result<(), ExchangeFailure> {
        let replacement = source.exchange()?;
        state.generation = state.generation.checked_add(1).ok_or_else(exhausted)?;
        state.installed = Some(replacement);
        state.usable = true;
        Ok(())
    }

    /// Returns a lease naming `generation`.
    fn lease(&self, generation: u64) -> AccessTokenLease {
        AccessTokenLease { identity: self.identity, generation }
    }

    /// Returns the cell, recovering the state a panicking caller left behind.
    ///
    /// A caller that panicked mid-exchange installed nothing, so the state is
    /// exactly as consistent as it was before; refusing to serve anybody
    /// afterwards would turn one caller's failure into the cache's.
    fn locked(&self) -> std::sync::MutexGuard<'_, CacheState> {
        self.state.lock().unwrap_or_else(std::sync::PoisonError::into_inner)
    }
}

/// Returns the failure an exhausted generation counter produces.
fn exhausted() -> ExchangeFailure {
    ExchangeFailure::new(ConfigurationFailureCode::AccessTokenInstallationGenerationExhausted)
}
