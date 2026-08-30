//! The one identity a Slingshot installation keeps, and the ledger beside it.
//!
//! Remote subscriptions are derived from this identifier. A daemon that found
//! the record missing and quietly made a new one would strand every one of
//! them: the remote would still be holding work under the old identity, and
//! nothing would be looking for it. So a missing, corrupt, or mismatched record
//! beside any existing target state is refused, every byte is left alone, and
//! recovering the identity is a person's deliberate act rather than a daemon's
//! silent one.
//!
//! # Why the ledger is in the same record
//!
//! Registering a target and holding the identity are one fact. If they were two
//! records, a crash between them would leave an installation that had an
//! identity and a target that did not know it, or the reverse - and each of
//! those needs its own recovery story. One atomically replaced record has one
//! story: it is either the old contents or the new ones.
//!
//! An `Initializing` entry is the one intermediate state, and it exists because
//! creating a database cannot be inside the same atomic write. Resuming one is
//! allowed only when the database is absent or exactly matches what the entry
//! staged; every other combination refuses.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

/// Format the installation record declares.
pub const INSTALLATION_RECORD_FORMAT: &str = "slingshot.installation-state/1";

/// Characters the identifier is rendered with.
pub const IDENTIFIER_CHARACTERS: usize = 64;

/// Reason an installation value could not be read.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum InstallationFailure {
    /// The identifier is not sixty-four lowercase hexadecimal characters.
    #[error(
        "an installation identifier is exactly {IDENTIFIER_CHARACTERS} lowercase hexadecimal characters"
    )]
    IdentifierNotCanonical,
    /// The record declares a format this build does not implement.
    #[error("the installation record declares another format")]
    UnsupportedFormat,
    /// A target is already registered and cannot be staged again.
    #[error("a registered target is not staged again")]
    AlreadyRegistered,
    /// A target that was never staged cannot be registered.
    #[error("a target is staged before it is registered")]
    NotStaged,
}

/// What one Slingshot installation is called.
///
/// Nonsecret and stable. It names an installation to a remote, which is why
/// losing it matters and why nothing here will invent a replacement.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct InstallationIdentifier {
    /// The identifier, in lowercase hexadecimal.
    value: String,
}

impl InstallationIdentifier {
    /// Returns the identifier `spelling` names.
    ///
    /// # Errors
    ///
    /// Returns [`InstallationFailure::IdentifierNotCanonical`] for anything but
    /// exactly sixty-four lowercase hexadecimal characters.
    pub fn parse(spelling: &str) -> Result<Self, InstallationFailure> {
        let canonical = spelling.len() == IDENTIFIER_CHARACTERS
            && spelling
                .chars()
                .all(|character| character.is_ascii_hexdigit() && !character.is_ascii_uppercase());
        if canonical {
            Ok(Self { value: spelling.to_owned() })
        } else {
            Err(InstallationFailure::IdentifierNotCanonical)
        }
    }

    /// Returns the identifier, in lowercase hexadecimal.
    #[must_use]
    pub fn as_text(&self) -> &str {
        &self.value
    }
}

impl TryFrom<String> for InstallationIdentifier {
    type Error = InstallationFailure;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        Self::parse(&value)
    }
}

impl From<InstallationIdentifier> for String {
    fn from(identifier: InstallationIdentifier) -> Self {
        identifier.value
    }
}

/// Where one target stands in the ledger.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TargetRegistration {
    /// Its database is being created.
    ///
    /// The one intermediate state, because creating a database cannot happen
    /// inside the atomic write that records it.
    Initializing,
    /// Its database exists and carries this installation's identifier.
    Registered,
}

/// The one record an installation keeps.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct InstallationRecord {
    /// Format this record declares.
    pub format: String,
    /// What this installation is called.
    pub installation_identifier: InstallationIdentifier,
    /// Where each target stands, by runtime namespace.
    pub targets: BTreeMap<String, TargetRegistration>,
}

impl InstallationRecord {
    /// Returns a record for a new installation with no targets yet.
    #[must_use]
    pub fn new(installation_identifier: InstallationIdentifier) -> Self {
        Self {
            format: INSTALLATION_RECORD_FORMAT.to_owned(),
            installation_identifier,
            targets: BTreeMap::new(),
        }
    }

    /// Requires this record to be one this build can act on.
    ///
    /// # Errors
    ///
    /// Returns [`InstallationFailure::UnsupportedFormat`] for another format.
    pub fn require_supported(&self) -> Result<(), InstallationFailure> {
        if self.format == INSTALLATION_RECORD_FORMAT {
            Ok(())
        } else {
            Err(InstallationFailure::UnsupportedFormat)
        }
    }

    /// Returns this record with `namespace` staged.
    ///
    /// # Errors
    ///
    /// Returns [`InstallationFailure::AlreadyRegistered`] when the target is
    /// registered already; staging one twice would lose the fact that its
    /// database is real.
    pub fn stage(&self, namespace: &str) -> Result<Self, InstallationFailure> {
        if self.targets.get(namespace) == Some(&TargetRegistration::Registered) {
            return Err(InstallationFailure::AlreadyRegistered);
        }
        let mut staged = self.clone();
        staged.targets.insert(namespace.to_owned(), TargetRegistration::Initializing);
        Ok(staged)
    }

    /// Returns this record with `namespace` registered.
    ///
    /// # Errors
    ///
    /// Returns [`InstallationFailure::NotStaged`] when the target was never
    /// staged, because a registration with no staging is a registration for a
    /// database nobody watched being made.
    pub fn register(&self, namespace: &str) -> Result<Self, InstallationFailure> {
        match self.targets.get(namespace) {
            Some(TargetRegistration::Initializing) => {
                let mut registered = self.clone();
                registered.targets.insert(namespace.to_owned(), TargetRegistration::Registered);
                Ok(registered)
            }
            Some(TargetRegistration::Registered) => Ok(self.clone()),
            None => Err(InstallationFailure::NotStaged),
        }
    }

    /// Returns where `namespace` stands, when it is in the ledger at all.
    #[must_use]
    pub fn registration(&self, namespace: &str) -> Option<TargetRegistration> {
        self.targets.get(namespace).copied()
    }
}

/// What the state root looks like when a daemon starts.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ObservedState {
    /// Whether a database exists for the target being started.
    pub database_present: bool,
    /// Whether the database's identifier equals the record's.
    pub database_identifier_matches: bool,
    /// Whether the global record exists.
    pub record_present: bool,
    /// Whether it could be read.
    pub record_readable: bool,
    /// Whether anything at all exists under the state root.
    pub state_root_occupied: bool,
    /// Where the target stands in the ledger.
    pub registration: Option<TargetRegistration>,
}

/// What a daemon may do with what it found.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StartupDisposition {
    /// Nothing exists anywhere: create the identity.
    CreateInstallation,
    /// Stage this target and create its database.
    StageTarget,
    /// Finish a staging that was interrupted.
    ResumeStaging,
    /// Everything is in place.
    Proceed,
    /// Refuse, change nothing, and tell a person.
    Refuse {
        /// Why, in one bounded phrase.
        reason: &'static str,
    },
}

/// Returns what a daemon may do with `observed`.
///
/// Refusals dominate. The only case that creates an identity is a state root
/// with nothing in it at all - not merely a missing record, because a missing
/// record beside existing target state is exactly the situation where inventing
/// a replacement would strand the subscriptions those targets already hold.
#[must_use]
pub fn classify_startup(observed: ObservedState) -> StartupDisposition {
    if !observed.record_present {
        return if observed.state_root_occupied {
            StartupDisposition::Refuse { reason: "the installation record is missing" }
        } else {
            StartupDisposition::CreateInstallation
        };
    }
    if !observed.record_readable {
        return StartupDisposition::Refuse { reason: "the installation record is unreadable" };
    }
    classify_target(observed)
}

/// Returns what a daemon may do about one target, given a readable record.
fn classify_target(observed: ObservedState) -> StartupDisposition {
    match (observed.registration, observed.database_present) {
        (None, false) => StartupDisposition::StageTarget,
        (None, true) => StartupDisposition::Refuse {
            reason: "a database exists for a target the ledger does not know",
        },
        (Some(TargetRegistration::Initializing), false) => StartupDisposition::ResumeStaging,
        (Some(TargetRegistration::Initializing), true) => {
            if observed.database_identifier_matches {
                StartupDisposition::ResumeStaging
            } else {
                StartupDisposition::Refuse {
                    reason: "a staged database carries another installation's identifier",
                }
            }
        }
        (Some(TargetRegistration::Registered), false) => {
            StartupDisposition::Refuse { reason: "a registered target has no database" }
        }
        (Some(TargetRegistration::Registered), true) => {
            if observed.database_identifier_matches {
                StartupDisposition::Proceed
            } else {
                StartupDisposition::Refuse {
                    reason: "the database carries another installation's identifier",
                }
            }
        }
    }
}

impl StartupDisposition {
    /// Returns whether this disposition leaves every byte alone.
    #[must_use]
    pub fn changes_nothing(self) -> bool {
        matches!(self, Self::Refuse { .. } | Self::Proceed)
    }

    /// Returns whether this disposition creates an identity.
    #[must_use]
    pub fn creates_identity(self) -> bool {
        matches!(self, Self::CreateInstallation)
    }
}
