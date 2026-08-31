//! Exclusive ownership of one runtime namespace.
//!
//! A namespace has exactly one authority: the process holding its
//! operating-system owner lock. Readiness records and process identifiers are
//! diagnostics, and neither can manufacture ownership. A process identifier in
//! particular proves nothing at all: the operating system reuses them, so an
//! identifier that matches a record may name a program that has nothing to do
//! with this one. Nothing here looks one up, checks it, or signals it.
//!
//! Classifying an apparent owner as live therefore means reaching its endpoint
//! and getting back the exact nonce its record claims. A record on its own is
//! a claim someone left behind; a matching live answer is the only evidence
//! that whoever left it is still there. The owner draws one
//! random readiness nonce and keeps it alive beside the lock; that nonce is the
//! only thing that authorizes a cooperative stop, and dropping the owner
//! removes only the readiness record carrying that exact nonce, so a departing
//! owner can never disturb its replacement.

use rand::{CryptoRng, RngExt};
use slingshot_local_protocol::foundation_contract::FoundationContract;
use slingshot_local_protocol::ping;

use crate::platform_runtime::failure::PlatformFailure;
use crate::platform_runtime::locks::OwnerLock;
use crate::platform_runtime::readiness::{self, PublishedIdentity, ReadinessRecord};
use crate::runtime_namespace::RuntimeNamespace;

/// What a process learns when it asks to own a runtime namespace.
#[derive(Debug)]
pub enum Acquisition {
    /// The caller now owns the namespace.
    Owned(Box<DaemonOwnership>),
    /// Another live process owns the namespace.
    AlreadyOwned(OwnerEvidence),
}

/// Bounded evidence about the process that already owns a namespace.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OwnerEvidence {
    /// Display value of the namespace that is already owned.
    pub namespace_display: String,
    /// Readiness record the live owner published, when it has published one.
    pub readiness: Option<ReadinessRecord>,
}

/// What probing an apparent owner's endpoint established.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Liveness {
    /// The endpoint answered with the exact nonce the record claims.
    Live,
    /// The endpoint answered, but with another nonce.
    ///
    /// Someone is there and it is not who the record says, so the record is a
    /// prior instance's and the endpoint belongs to its replacement. Neither is
    /// a thing this process may clean up.
    AnotherInstance,
    /// Nothing answered, so the record is what a departed owner left.
    Departed,
}

/// Returns what an answered nonce says about an apparent owner.
///
/// Taking the answer rather than performing the probe keeps the decision
/// testable without a live endpoint, and keeps transport out of a module whose
/// subject is authority.
#[must_use]
pub fn classify_liveness(record: &ReadinessRecord, answered_nonce: Option<&str>) -> Liveness {
    match answered_nonce {
        Some(nonce) if nonce == record.readiness_nonce => Liveness::Live,
        Some(_) => Liveness::AnotherInstance,
        None => Liveness::Departed,
    }
}

/// The one authority over a runtime namespace, held for a daemon's lifetime.
#[derive(Debug)]
pub struct DaemonOwnership {
    namespace: RuntimeNamespace,
    readiness_nonce: String,
    held: OwnerLock,
    identity: Option<PublishedIdentity>,
    published: bool,
}

/// Draws one readiness nonce of the exact length the contract declares.
fn draw_readiness_nonce(contract: &FoundationContract) -> String {
    let mut generator = rand::rng();
    let mut bytes = vec![0_u8; contract.namespace.readiness_nonce_bytes as usize];
    fill_from(&mut generator, &mut bytes);
    hex::encode(bytes)
}

/// Fills a buffer from a generator the type system marks as strong.
fn fill_from(generator: &mut (impl RngExt + CryptoRng), bytes: &mut [u8]) {
    generator.fill(bytes);
}

impl DaemonOwnership {
    /// Takes exclusive ownership of one runtime namespace.
    ///
    /// Stale records left by a departed owner are recovered only once this
    /// process holds the lock, so a forged record can never displace a live
    /// owner and a recovering process can never race another recoverer.
    ///
    /// # Errors
    ///
    /// Returns [`PlatformFailure`] when the runtime state cannot be read or the
    /// lock file cannot be opened.
    pub fn acquire(
        contract: &FoundationContract,
        namespace: RuntimeNamespace,
    ) -> Result<Acquisition, PlatformFailure> {
        let Some(held) = OwnerLock::acquire(namespace.runtime_root(), namespace.digest())? else {
            let readiness = readiness::read(namespace.runtime_root(), namespace.digest())?;
            return Ok(Acquisition::AlreadyOwned(OwnerEvidence {
                namespace_display: namespace.display(),
                readiness,
            }));
        };
        // Holding the lock is what makes this safe. A live owner holds it, so
        // reaching here means nobody does, and the record can only be one a
        // departed owner left. Recovering it without the lock would be a race
        // with whoever is actually starting.
        let stale = readiness::read(namespace.runtime_root(), namespace.digest())?;
        if let Some(record) = stale {
            readiness::remove_matching(
                namespace.runtime_root(),
                namespace.digest(),
                &record.readiness_nonce,
            )?;
        }
        Ok(Acquisition::Owned(Box::new(Self {
            readiness_nonce: draw_readiness_nonce(contract),
            namespace,
            held,
            identity: None,
            published: false,
        })))
    }

    /// Returns the namespace this ownership covers.
    #[must_use]
    pub fn namespace(&self) -> &RuntimeNamespace {
        &self.namespace
    }

    /// Returns the live readiness nonce.
    ///
    /// The nonce is the only stop authority this owner has. It is drawn once,
    /// lives exactly as long as the lock, and is never reused.
    #[must_use]
    pub fn readiness_nonce(&self) -> &str {
        &self.readiness_nonce
    }

    /// Returns the path of the lock file this owner holds.
    #[must_use]
    pub fn lock_path(&self) -> &std::path::Path {
        self.held.path()
    }

    /// Records what this owner serves, before it publishes readiness.
    ///
    /// Separate from acquisition because the two answer different questions in
    /// different orders: a process takes the lock to find out whether it is the
    /// owner at all, and only an owner goes on to establish a target.
    pub fn identify(&mut self, identity: PublishedIdentity) {
        self.identity = Some(identity);
    }

    /// Returns what this owner serves, once it has been told.
    #[must_use]
    pub fn identity(&self) -> Option<&PublishedIdentity> {
        self.identity.as_ref()
    }

    /// Publishes readiness for this owner, atomically.
    ///
    /// # Errors
    ///
    /// Returns [`PlatformFailure`] when the record is beyond its bound or the
    /// runtime state cannot be written.
    pub fn publish_readiness(
        &mut self,
        contract: &FoundationContract,
        endpoint_display: &str,
    ) -> Result<(), PlatformFailure> {
        let record = ReadinessRecord {
            endpoint_display: endpoint_display.to_owned(),
            identity: self.identity.clone(),
            process_identifier: std::process::id(),
            readiness_nonce: self.readiness_nonce.clone(),
        };
        readiness::publish(
            contract,
            self.namespace.runtime_root(),
            self.namespace.digest(),
            &record,
        )?;
        self.published = true;
        Ok(())
    }

    /// Reports whether a supplied nonce authorizes stopping this owner.
    ///
    /// Only the exact live nonce authorizes a stop. A numeric process
    /// identifier, a process name, and a readiness record a prior instance left
    /// behind are never authority.
    #[must_use]
    pub fn stop_is_authorized(&self, supplied_nonce: &str) -> bool {
        ping::stop_is_authorized(&self.readiness_nonce, supplied_nonce)
    }

    /// Removes this owner's readiness record, leaving the lock file in place.
    ///
    /// # Errors
    ///
    /// Returns [`PlatformFailure`] when the runtime state cannot be read or
    /// removed.
    pub fn withdraw_readiness(&mut self) -> Result<bool, PlatformFailure> {
        let removed = readiness::remove_matching(
            self.namespace.runtime_root(),
            self.namespace.digest(),
            &self.readiness_nonce,
        )?;
        self.published = false;
        Ok(removed)
    }
}

impl Drop for DaemonOwnership {
    fn drop(&mut self) {
        if self.published {
            self.withdraw_readiness().ok();
        }
    }
}
