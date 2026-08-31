//! Daemons a test starts, and what they do when they are asked.
//!
//! A scripted lifecycle rather than a real process, because the properties
//! worth proving are about what the client concludes and what it does next: how
//! many times it spawned, whether it spawned at all, and whether a stale nonce
//! reached a replacement. Those are countable here and expensive to observe
//! through a real process, and observing them through a real one would mean the
//! test's timing decided whether it passed.
//!
//! The recording is the point. Every probe, spawn, and stop is written down, so
//! a claim that status never spawns is checked by counting rather than by
//! reading the code that was supposed to make it true.

use std::sync::Mutex;

/// What a probe of one namespace found.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProbeAnswer {
    /// Nothing is serving there.
    Absent,
    /// Something is serving, and this is what it said about itself.
    Serving(Box<Handshake>),
    /// Something is there and did not answer properly.
    Unhealthy,
}

/// What a serving daemon says about itself when it is asked.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Handshake {
    /// Which partition it serves.
    pub author_target_identity_digest: String,
    /// The nonce a stop must quote.
    pub current_nonce: String,
    /// Which runtime contract it was built against.
    pub runtime_contract_digest: String,
    /// Which environment revision it serves.
    pub selected_environment_revision: String,
}

/// What one scripted daemon does over its life.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Lifecycle {
    /// Nothing is there and nothing appears.
    Absent,
    /// Nothing is there, and one spawn makes it serve.
    StartsOnDemand(Box<Handshake>),
    /// Nothing is there, and a spawn produces a process that exits.
    ExitsEarly {
        /// What it said on the way out.
        detail: String,
    },
    /// Nothing is there, and a spawn produces one that never becomes ready.
    NeverReady,
    /// Something is already serving.
    AlreadyServing(Box<Handshake>),
    /// Something is there and answers badly.
    Unhealthy,
}

/// What stopping produced.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StopAnswer {
    /// It was serving, the nonce matched, and it released the endpoint.
    Released,
    /// The nonce named an owner that is no longer there.
    NonceStale,
    /// Nothing was serving.
    Absent,
}

/// One scripted daemon, and everything it was asked.
#[derive(Debug)]
pub struct ScriptedDaemon {
    /// What it does.
    lifecycle: Mutex<Lifecycle>,
    /// How many probes it answered.
    probes: Mutex<u32>,
    /// How many spawns it was asked for.
    spawns: Mutex<u32>,
    /// The nonces a stop quoted, in order.
    stops: Mutex<Vec<String>>,
}

impl ScriptedDaemon {
    /// Returns a daemon that behaves as `lifecycle` says.
    #[must_use]
    pub fn following(lifecycle: Lifecycle) -> Self {
        Self {
            lifecycle: Mutex::new(lifecycle),
            probes: Mutex::new(0),
            spawns: Mutex::new(0),
            stops: Mutex::new(Vec::new()),
        }
    }

    /// Returns how many probes it answered.
    #[must_use]
    pub fn probes(&self) -> u32 {
        *self.probes.lock().expect("the counter is not poisoned")
    }

    /// Returns how many spawns it was asked for.
    #[must_use]
    pub fn spawns(&self) -> u32 {
        *self.spawns.lock().expect("the counter is not poisoned")
    }

    /// Returns the nonces a stop quoted, in order.
    #[must_use]
    pub fn stops(&self) -> Vec<String> {
        self.stops.lock().expect("the record is not poisoned").clone()
    }

    /// Answers one probe.
    pub fn probe(&self) -> ProbeAnswer {
        *self.probes.lock().expect("the counter is not poisoned") += 1;
        match &*self.lifecycle.lock().expect("the lifecycle is not poisoned") {
            Lifecycle::AlreadyServing(handshake) | Lifecycle::StartsOnDemand(handshake) => {
                ProbeAnswer::Serving(handshake.clone())
            }
            Lifecycle::Unhealthy => ProbeAnswer::Unhealthy,
            Lifecycle::Absent | Lifecycle::ExitsEarly { .. } | Lifecycle::NeverReady => {
                ProbeAnswer::Absent
            }
        }
    }

    /// Answers one probe of a namespace nothing has started yet.
    ///
    /// The distinction from [`Self::probe`] is what a lifecycle that starts on
    /// demand does: before a spawn it is absent, and after one it serves.
    pub fn probe_before_start(&self) -> ProbeAnswer {
        *self.probes.lock().expect("the counter is not poisoned") += 1;
        match &*self.lifecycle.lock().expect("the lifecycle is not poisoned") {
            Lifecycle::AlreadyServing(handshake) => ProbeAnswer::Serving(handshake.clone()),
            Lifecycle::Unhealthy => ProbeAnswer::Unhealthy,
            _ => ProbeAnswer::Absent,
        }
    }

    /// Records one spawn, and says what the child did.
    ///
    /// # Errors
    ///
    /// Returns the detail a child that exited printed on its way out.
    pub fn spawn(&self) -> Result<(), String> {
        *self.spawns.lock().expect("the counter is not poisoned") += 1;
        match &*self.lifecycle.lock().expect("the lifecycle is not poisoned") {
            Lifecycle::ExitsEarly { detail } => Err(detail.clone()),
            _ => Ok(()),
        }
    }

    /// Answers one stop quoting `nonce`.
    pub fn stop(&self, nonce: &str) -> StopAnswer {
        self.stops.lock().expect("the record is not poisoned").push(nonce.to_owned());
        let mut lifecycle = self.lifecycle.lock().expect("the lifecycle is not poisoned");
        let serving = match &*lifecycle {
            Lifecycle::AlreadyServing(handshake) | Lifecycle::StartsOnDemand(handshake) => {
                Some(handshake.clone())
            }
            _ => None,
        };
        let Some(handshake) = serving else {
            return StopAnswer::Absent;
        };
        if handshake.current_nonce != nonce {
            return StopAnswer::NonceStale;
        }
        *lifecycle = Lifecycle::Absent;
        StopAnswer::Released
    }
}
