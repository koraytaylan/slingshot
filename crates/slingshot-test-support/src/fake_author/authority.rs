//! One continuation-key authority, and the three deployments that provide it.
//!
//! Every deployment profile answers the same interface: read, compare-and-set,
//! fence, and lease. A single-node instance implements it exactly as a cluster
//! does, which is the point rather than an inefficiency - a profile that
//! weakened the contract because it happened to be running alone would be a
//! profile whose guarantees changed when somebody added a node, and the code
//! relying on them would not know.
//!
//! Nothing here observes node count to decide how to behave. The simulation is
//! explicitly language-neutral: it does not claim that Java, a provider secret
//! service, or an Oak repository executed anything.

use std::collections::BTreeMap;
use std::sync::Mutex;

/// Which deployment provides the authority.
///
/// A label for a test to assert on, and nothing the implementation branches on.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum DeploymentProfile {
    /// A single instance, which still implements the cluster contract.
    SingleNode,
    /// Several instances sharing one repository.
    Cluster,
    /// A managed deployment whose nodes are replaced under it.
    Managed,
}

/// Every profile, so a suite can prove none of them is weaker.
pub const EVERY_PROFILE: &[DeploymentProfile] =
    &[DeploymentProfile::SingleNode, DeploymentProfile::Cluster, DeploymentProfile::Managed];

/// Bytes a continuation key may occupy.
pub const MAXIMUM_KEY_BYTES: usize = 431;

/// Bytes a whole key ring may occupy.
pub const MAXIMUM_KEY_RING_BYTES: usize = 768;

/// Reason the authority refused.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum AuthorityRefusal {
    /// The expected value is not the stored one.
    #[error("the stored value has moved on, so this write was not applied")]
    CompareFailed,
    /// The caller no longer holds the lease it is writing under.
    #[error("this caller's lease was taken, so its write cannot be applied")]
    Fenced,
    /// The key ring is absent.
    #[error("this deployment has no key ring, and one is not created implicitly")]
    Absent,
    /// The key ring is there but unreadable.
    #[error("the key ring is present and not readable, which is not an empty key ring")]
    Corrupt,
    /// A value is beyond its bound.
    #[error("a continuation key holds at most {MAXIMUM_KEY_BYTES} bytes, and this holds {actual}")]
    KeyTooLong {
        /// How long it was.
        actual: usize,
    },
    /// The ring is beyond its bound.
    #[error("a key ring holds at most {MAXIMUM_KEY_RING_BYTES} bytes, and this holds {actual}")]
    RingTooLong {
        /// How long it was.
        actual: usize,
    },
}

/// One durable, linearizable continuation-key authority.
#[derive(Debug)]
pub struct ContinuationKeyAuthority {
    /// Which deployment this is, as a label.
    profile: DeploymentProfile,
    /// The ring, when one has been created.
    ring: Mutex<Option<BTreeMap<String, String>>>,
    /// Which lease may write, and the one after it.
    fence: Mutex<u64>,
}

impl ContinuationKeyAuthority {
    /// Returns an authority with no key ring at all.
    ///
    /// Absent rather than empty. A caller that found an empty ring where it
    /// expected keys would carry on and issue new ones; one that finds nothing
    /// is told to look at why.
    #[must_use]
    pub fn absent(profile: DeploymentProfile) -> Self {
        Self { profile, ring: Mutex::new(None), fence: Mutex::new(1) }
    }

    /// Returns an authority whose ring has been created and is empty.
    #[must_use]
    pub fn created(profile: DeploymentProfile) -> Self {
        Self { profile, ring: Mutex::new(Some(BTreeMap::new())), fence: Mutex::new(1) }
    }

    /// Returns which deployment this is.
    #[must_use]
    pub fn profile(&self) -> DeploymentProfile {
        self.profile
    }

    /// Returns the lease a caller must hold to write.
    #[must_use]
    pub fn current_lease(&self) -> u64 {
        self.fence.lock().map(|held| *held).unwrap_or_default()
    }

    /// Takes the lease, invalidating whatever held it before.
    ///
    /// Returns the new lease. A node that was replaced under a managed
    /// deployment finds its writes refused rather than applied late.
    pub fn take_lease(&self) -> u64 {
        self.fence
            .lock()
            .map(|mut held| {
                *held = held.saturating_add(1);
                *held
            })
            .unwrap_or_default()
    }

    /// Reads one key.
    ///
    /// # Errors
    ///
    /// Returns [`AuthorityRefusal::Absent`] when no ring exists.
    pub fn read(&self, name: &str) -> Result<Option<String>, AuthorityRefusal> {
        let held = self.ring.lock().map_err(|_| AuthorityRefusal::Corrupt)?;
        let ring = held.as_ref().ok_or(AuthorityRefusal::Absent)?;
        Ok(ring.get(name).cloned())
    }

    /// Writes one key, if it still holds what the caller expects.
    ///
    /// # Errors
    ///
    /// Returns [`AuthorityRefusal::Fenced`] when the caller's lease was taken,
    /// [`AuthorityRefusal::CompareFailed`] when the stored value moved on,
    /// [`AuthorityRefusal::Absent`], or a bound refusal.
    pub fn compare_and_set(
        &self,
        lease: u64,
        name: &str,
        expected: Option<&str>,
        value: &str,
    ) -> Result<(), AuthorityRefusal> {
        if value.len() > MAXIMUM_KEY_BYTES {
            return Err(AuthorityRefusal::KeyTooLong { actual: value.len() });
        }
        if self.current_lease() != lease {
            return Err(AuthorityRefusal::Fenced);
        }
        let mut held = self.ring.lock().map_err(|_| AuthorityRefusal::Corrupt)?;
        let ring = held.as_mut().ok_or(AuthorityRefusal::Absent)?;
        if ring.get(name).map(String::as_str) != expected {
            return Err(AuthorityRefusal::CompareFailed);
        }
        let projected: usize = ring
            .iter()
            .filter(|(held_name, _)| held_name.as_str() != name)
            .map(|(held_name, held_value)| held_name.len() + held_value.len())
            .sum::<usize>()
            + name.len()
            + value.len();
        if projected > MAXIMUM_KEY_RING_BYTES {
            return Err(AuthorityRefusal::RingTooLong { actual: projected });
        }
        ring.insert(name.to_owned(), value.to_owned());
        Ok(())
    }

    /// Returns how many bytes the ring holds.
    ///
    /// # Errors
    ///
    /// Returns [`AuthorityRefusal::Absent`] when no ring exists.
    pub fn ring_bytes(&self) -> Result<usize, AuthorityRefusal> {
        let held = self.ring.lock().map_err(|_| AuthorityRefusal::Corrupt)?;
        let ring = held.as_ref().ok_or(AuthorityRefusal::Absent)?;
        Ok(ring.iter().map(|(name, value)| name.len() + value.len()).sum())
    }
}
