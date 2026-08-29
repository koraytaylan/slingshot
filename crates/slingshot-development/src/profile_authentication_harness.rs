//! Repository harness for the profile and authentication boundaries.
//!
//! The inward crates each prove their own contract. What none of them can prove
//! alone is the claim the whole plan is for: that a command reaching an author
//! never reaches anything else. Proving that needs the pieces composed - a
//! committed configuration root, a loader, a selection, a snapshot, a provider -
//! with listeners standing where traffic must not go.
//!
//! A trap here is a listener rather than a closed port. A closed port cannot
//! tell a caller that never tried from one whose connection was refused, and the
//! claim is about the first.
//!
//! The scanner exists because redaction is a property of everything a failure
//! touches, not of one rendering. It is given the standard streams, the
//! diagnostics, the debug renderings, and the request bytes together, and asked
//! whether any sentinel survived anywhere.

use std::collections::BTreeSet;

use slingshot_test_support::identity_management_server::ScriptedIdentityManagementServer;

/// A listener that must receive nothing at all.
///
/// It accepts connections on purpose. Refusing them would make a caller that
/// never tried indistinguishable from one whose connection was refused, and the
/// claim being made is about the first.
#[derive(Debug)]
pub struct Trap {
    /// The listener itself.
    listener: ScriptedIdentityManagementServer,
    /// What the trap stands for, for a failure message.
    name: &'static str,
}

impl Trap {
    /// Returns a trap named `name`.
    #[must_use]
    pub fn named(name: &'static str) -> Self {
        Self { listener: ScriptedIdentityManagementServer::trap(), name }
    }

    /// Returns the address the trap listens on.
    #[must_use]
    pub fn address(&self) -> String {
        self.listener.address().to_string()
    }

    /// Returns how many requests reached the trap.
    #[must_use]
    pub fn arrivals(&self) -> usize {
        self.listener.received().len()
    }

    /// Reports the failure an occupied trap describes.
    ///
    /// # Errors
    ///
    /// Returns the trap's name and its arrival count when anything arrived.
    pub fn require_empty(&self) -> Result<(), String> {
        let arrivals = self.arrivals();
        if arrivals == 0 {
            return Ok(());
        }
        Err(format!("{} received {arrivals} requests", self.name))
    }
}

/// Collects everything a transcript rendered and looks for what must not be in
/// it.
///
/// Redaction is a property of every place a value can surface, so the scanner
/// is given the renderings, the diagnostics, and the request bytes together
/// rather than each being checked where it was produced.
#[derive(Debug, Default)]
pub struct SecretScanner {
    /// Values that must appear nowhere.
    sentinels: BTreeSet<String>,
    /// Everything the transcript produced.
    observed: Vec<String>,
}

impl SecretScanner {
    /// Returns a scanner looking for `sentinels`.
    #[must_use]
    pub fn looking_for(sentinels: &[&str]) -> Self {
        Self {
            sentinels: sentinels.iter().map(|sentinel| (*sentinel).to_owned()).collect(),
            observed: Vec::new(),
        }
    }

    /// Records one rendering the transcript produced.
    pub fn observe(&mut self, rendering: impl Into<String>) {
        self.observed.push(rendering.into());
    }

    /// Records one sequence of bytes the transcript produced.
    pub fn observe_bytes(&mut self, bytes: &[u8]) {
        self.observed.push(String::from_utf8_lossy(bytes).into_owned());
    }

    /// Returns every sentinel that survived into something observed.
    ///
    /// Each sentinel is looked for verbatim and in the two encodings a value
    /// most often survives in, because an encoded secret is still the secret.
    #[must_use]
    pub fn survivors(&self) -> Vec<String> {
        use base64::Engine;
        use base64::engine::general_purpose::{STANDARD, URL_SAFE_NO_PAD};

        let mut found = Vec::new();
        for sentinel in &self.sentinels {
            let spellings = [
                sentinel.clone(),
                STANDARD.encode(sentinel.as_bytes()),
                URL_SAFE_NO_PAD.encode(sentinel.as_bytes()),
                sentinel.bytes().map(|byte| format!("{byte:02x}")).collect(),
            ];
            for spelling in spellings {
                if self.observed.iter().any(|rendering| rendering.contains(&spelling)) {
                    found.push(format!("{sentinel} survived as {spelling}"));
                }
            }
        }
        found
    }

    /// Reports the failure any surviving sentinel describes.
    ///
    /// # Errors
    ///
    /// Returns every survivor when the transcript rendered one.
    pub fn require_clean(&self) -> Result<(), Vec<String>> {
        let survivors = self.survivors();
        if survivors.is_empty() {
            return Ok(());
        }
        Err(survivors)
    }
}

/// Reports whether one root set holds `certificate`.
///
/// The author route may hold a selected additional authority; the
/// identity-management route may not, and that is a set membership question
/// rather than a matter of what a builder happens to be handed.
#[must_use]
pub fn holds_certificate(roots: &[Vec<u8>], certificate: &[u8]) -> bool {
    roots.iter().any(|root| root == certificate)
}
