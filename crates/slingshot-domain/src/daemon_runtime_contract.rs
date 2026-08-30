//! The one authority for every value this plan's runtime spends.
//!
//! Every wire bound, storage bound, scheduling bound, diagnostic bound, and
//! maintenance bound the daemon uses comes from `policy/daemon-runtime-contract-1.json`
//! and from nowhere else. A consumer may name a typed accessor; it may not
//! define a second constant with the same meaning, because the moment two exist
//! the one that is wrong is whichever was edited second.
//!
//! The manifest separates `limits` from `formulas` on purpose. A limit is a
//! decision somebody made. A formula is arithmetic over limits, and writing the
//! answer down beside the operands means a reader can check it - which the
//! vectors do, computing each one from its primitive operands rather than
//! calling the code under test.
//!
//! There is no deployment override. Changing one value requires a new operation
//! protocol version and a new manifest format rather than quietly changing
//! version one, because every admitted operation and every maintenance receipt
//! persists the digest it was accepted under.
//!
//! # Maintenance-result identity
//!
//! [`MaintenanceResultIdentifier`] is deliberately operation-free. A maintenance
//! result outlives the operation that produced it and is read back by target and
//! identifier alone, so its preimage carries a target, a kind, a reviewed source
//! digest, and a content digest - and nothing that could smuggle an operation
//! identifier, a command name, or an artifact slot into a name that is supposed
//! to be independent of all three. Every field is fixed width and the kind byte
//! is closed, so the ninety-seven octets have exactly one reading.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use sha2::Digest as _;

/// Format the manifest declares.
pub const DAEMON_RUNTIME_CONTRACT_FORMAT: &str = "slingshot.daemon-runtime-contract/1";

/// Operation protocol version this manifest belongs to.
pub const DAEMON_OPERATION_PROTOCOL_VERSION: u64 = 1;

/// Domain separator every maintenance-result preimage begins with.
pub const MAINTENANCE_RESULT_DOMAIN: &str = "slingshot.maintenance-result/1";

/// Byte that ends the domain separator.
pub const MAINTENANCE_RESULT_DOMAIN_TERMINATOR: u8 = 0;

/// Databases present at once while one is being replaced.
const REPLACEMENT_DATABASE_COPIES: u64 = 2;

/// Retained documents each application receipt owns: its preview and its result.
const DOCUMENTS_PER_RECEIPT: u64 = 2;

/// The one current unapplied preview a target may hold.
const CURRENT_PREVIEW_ASSOCIATIONS: u64 = 1;

/// Octets a SHA-256 digest occupies.
pub const DIGEST_OCTETS: usize = 32;

/// Octets one maintenance-result preimage carries after its separator.
pub const MAINTENANCE_RESULT_PREIMAGE_OCTETS: usize = DIGEST_OCTETS * 3 + 1;

/// Characters a digest is rendered with.
pub const DIGEST_CHARACTERS: usize = DIGEST_OCTETS * 2;

/// Bytes of the committed manifest, embedded at compile time.
const EMBEDDED_MANIFEST: &str = include_str!("../../../policy/daemon-runtime-contract-1.json");

/// Bytes of the committed sidecar, embedded at compile time.
const EMBEDDED_SIDECAR: &str = include_str!("../../../policy/daemon-runtime-contract-1.sha256");

/// Reason the runtime contract could not be read.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum DaemonRuntimeContractFailure {
    /// The manifest bytes are not a valid contract document.
    #[error("the daemon runtime contract could not be read: {0}")]
    Unreadable(String),
    /// The manifest declares a format this build does not implement.
    #[error("the daemon runtime contract declares {0}")]
    UnsupportedFormat(String),
    /// The manifest belongs to another operation protocol version.
    #[error("the daemon runtime contract belongs to operation protocol version {0}")]
    UnsupportedVersion(u64),
    /// The manifest is not in the canonical form its readers regenerate.
    #[error("the daemon runtime contract is not in canonical form")]
    NotCanonical,
    /// A formula does not equal the arithmetic it stands for.
    #[error("the daemon runtime contract records {name} as a value its operands do not produce")]
    FormulaInconsistent {
        /// Formula that disagrees with its operands.
        name: String,
    },
    /// The sidecar does not describe the manifest beside it.
    #[error("the daemon runtime contract sidecar does not describe the manifest")]
    DigestMismatch,
}

/// Reason a maintenance-result identifier could not be read.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum MaintenanceResultIdentifierFailure {
    /// The rendering is not sixty-four lowercase hexadecimal characters.
    #[error(
        "a maintenance result identifier is exactly {DIGEST_CHARACTERS} lowercase hexadecimal characters"
    )]
    NotCanonical,
}

/// Which document a maintenance result is.
///
/// Closed, and one octet wide in the preimage, so two kinds of result for one
/// target and one manifest are two identifiers rather than a collision.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MaintenanceResultKind {
    /// A preview of what maintenance would do.
    Preview,
    /// The receipt of maintenance that was applied.
    Application,
}

impl MaintenanceResultKind {
    /// Returns the octet this kind contributes to a preimage.
    #[must_use]
    pub fn octet(self) -> u8 {
        match self {
            Self::Preview => 0x00,
            Self::Application => 0x01,
        }
    }
}

/// The name one maintenance result is read back by.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct MaintenanceResultIdentifier {
    /// The digest, in lowercase hexadecimal.
    value: String,
}

impl MaintenanceResultIdentifier {
    /// Returns the identifier these four facts derive.
    ///
    /// Fixed widths throughout, so the octets have one reading. Nothing about
    /// the operation that produced the result participates, which is what lets
    /// the result be read back after that operation is gone.
    #[must_use]
    pub fn derive(
        target: &[u8; DIGEST_OCTETS],
        kind: MaintenanceResultKind,
        reviewed_manifest: &[u8; DIGEST_OCTETS],
        content: &[u8; DIGEST_OCTETS],
    ) -> Self {
        let mut hasher = sha2::Sha256::new();
        hasher.update(MAINTENANCE_RESULT_DOMAIN.as_bytes());
        hasher.update([MAINTENANCE_RESULT_DOMAIN_TERMINATOR]);
        hasher.update(target);
        hasher.update([kind.octet()]);
        hasher.update(reviewed_manifest);
        hasher.update(content);
        Self { value: render(&hasher.finalize()) }
    }

    /// Returns the octets a derivation hashes after its separator.
    ///
    /// Exposed so a test can count them and read them back rather than trusting
    /// that the derivation hashed what it claimed to.
    #[must_use]
    pub fn preimage(
        target: &[u8; DIGEST_OCTETS],
        kind: MaintenanceResultKind,
        reviewed_manifest: &[u8; DIGEST_OCTETS],
        content: &[u8; DIGEST_OCTETS],
    ) -> Vec<u8> {
        let mut octets = Vec::with_capacity(MAINTENANCE_RESULT_PREIMAGE_OCTETS);
        octets.extend_from_slice(target);
        octets.push(kind.octet());
        octets.extend_from_slice(reviewed_manifest);
        octets.extend_from_slice(content);
        octets
    }

    /// Returns the identifier `spelling` names.
    ///
    /// # Errors
    ///
    /// Returns [`MaintenanceResultIdentifierFailure::NotCanonical`] for
    /// anything but exactly sixty-four lowercase hexadecimal characters.
    pub fn parse(spelling: &str) -> Result<Self, MaintenanceResultIdentifierFailure> {
        let canonical = spelling.len() == DIGEST_CHARACTERS
            && spelling
                .chars()
                .all(|character| character.is_ascii_hexdigit() && !character.is_ascii_uppercase());
        if canonical {
            Ok(Self { value: spelling.to_owned() })
        } else {
            Err(MaintenanceResultIdentifierFailure::NotCanonical)
        }
    }

    /// Returns the identifier, in lowercase hexadecimal.
    #[must_use]
    pub fn as_text(&self) -> &str {
        &self.value
    }
}

impl TryFrom<String> for MaintenanceResultIdentifier {
    type Error = MaintenanceResultIdentifierFailure;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        Self::parse(&value)
    }
}

impl From<MaintenanceResultIdentifier> for String {
    fn from(identifier: MaintenanceResultIdentifier) -> Self {
        identifier.value
    }
}

/// Returns `octets` in lowercase hexadecimal.
fn render(octets: &[u8]) -> String {
    octets.iter().map(|octet| format!("{octet:02x}")).collect()
}

/// The digest the manifest is admitted under.
///
/// Advertised by hello without making retained control depend on it, and
/// persisted with every admitted operation and maintenance receipt. A record
/// written under an older digest stays readable history; nothing is upgraded in
/// place.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct DaemonRuntimeContractDigest {
    /// The digest, in lowercase hexadecimal.
    value: String,
}

impl DaemonRuntimeContractDigest {
    /// Returns the digest, in lowercase hexadecimal.
    #[must_use]
    pub fn as_text(&self) -> &str {
        &self.value
    }
}

/// Every value this plan's runtime spends.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DaemonRuntimeContract {
    /// Format this manifest declares.
    pub format: String,
    /// Arithmetic over the limits, written down beside its operands.
    pub formulas: BTreeMap<String, u64>,
    /// Decisions somebody made.
    pub limits: BTreeMap<String, u64>,
    /// Operation protocol version this manifest belongs to.
    pub operation_protocol_version: u64,
}

impl DaemonRuntimeContract {
    /// Returns the contract this build embedded.
    ///
    /// # Panics
    ///
    /// Panics when the committed manifest is unreadable, which is a defect in
    /// this repository rather than in any caller's input.
    #[must_use]
    pub fn embedded() -> &'static Self {
        static PARSED: std::sync::OnceLock<DaemonRuntimeContract> = std::sync::OnceLock::new();
        PARSED.get_or_init(|| {
            Self::parse(EMBEDDED_MANIFEST).expect("the committed runtime contract is valid")
        })
    }

    /// Returns the bytes this build embedded.
    #[must_use]
    pub fn embedded_manifest() -> &'static str {
        EMBEDDED_MANIFEST
    }

    /// Returns the digest the committed sidecar records.
    ///
    /// # Panics
    ///
    /// Panics when the sidecar does not describe the manifest beside it.
    #[must_use]
    pub fn embedded_digest() -> DaemonRuntimeContractDigest {
        let recorded = EMBEDDED_SIDECAR.trim_end_matches('\n').to_owned();
        let computed = render(&sha2::Sha256::digest(EMBEDDED_MANIFEST.as_bytes()));
        assert_eq!(recorded, computed, "the committed sidecar does not describe the manifest");
        DaemonRuntimeContractDigest { value: recorded }
    }

    /// Returns the contract `text` spells.
    ///
    /// # Errors
    ///
    /// Returns [`DaemonRuntimeContractFailure`] for a document that is
    /// unreadable, declares another format or version, is not in the canonical
    /// form readers regenerate, or records a formula its operands do not
    /// produce.
    pub fn parse(text: &str) -> Result<Self, DaemonRuntimeContractFailure> {
        let contract: Self = serde_json::from_str(text)
            .map_err(|failure| DaemonRuntimeContractFailure::Unreadable(failure.to_string()))?;
        if contract.format != DAEMON_RUNTIME_CONTRACT_FORMAT {
            return Err(DaemonRuntimeContractFailure::UnsupportedFormat(contract.format));
        }
        if contract.operation_protocol_version != DAEMON_OPERATION_PROTOCOL_VERSION {
            return Err(DaemonRuntimeContractFailure::UnsupportedVersion(
                contract.operation_protocol_version,
            ));
        }
        if contract.render()? != text {
            return Err(DaemonRuntimeContractFailure::NotCanonical);
        }
        contract.require_consistent_formulas()?;
        Ok(contract)
    }

    /// Returns the canonical bytes this contract writes.
    ///
    /// # Errors
    ///
    /// Returns [`DaemonRuntimeContractFailure::Unreadable`] when the contract
    /// cannot be written, which no valid contract provokes.
    pub fn render(&self) -> Result<String, DaemonRuntimeContractFailure> {
        let written = serde_json::to_string(self)
            .map_err(|failure| DaemonRuntimeContractFailure::Unreadable(failure.to_string()))?;
        Ok(format!("{written}\n"))
    }

    /// Returns the limit named `name`.
    ///
    /// # Panics
    ///
    /// Panics when the manifest declares no limit of that name, which means a
    /// caller has invented one rather than reading one.
    #[must_use]
    pub fn limit(&self, name: &str) -> u64 {
        *self
            .limits
            .get(name)
            .unwrap_or_else(|| panic!("the runtime contract declares no limit named {name}"))
    }

    /// Returns the formula named `name`.
    ///
    /// # Panics
    ///
    /// Panics when the manifest declares no formula of that name.
    #[must_use]
    pub fn formula(&self, name: &str) -> u64 {
        *self
            .formulas
            .get(name)
            .unwrap_or_else(|| panic!("the runtime contract declares no formula named {name}"))
    }

    /// Requires every formula to equal the arithmetic it stands for.
    ///
    /// Checked in unsigned 64-bit arithmetic throughout: an overflow is a
    /// failure rather than a wrapped answer that would look plausible.
    ///
    /// # Errors
    ///
    /// Returns [`DaemonRuntimeContractFailure::FormulaInconsistent`] at the
    /// first formula whose operands produce something else.
    pub fn require_consistent_formulas(&self) -> Result<(), DaemonRuntimeContractFailure> {
        let page = self.limit("sqlite_page_bytes");
        let frame_header = self.limit("sqlite_write_ahead_log_frame_header_bytes");
        let frame = frame_header.checked_add(page);
        let database = page.checked_mul(self.limit("maximum_sqlite_database_pages"));
        let transaction = page.checked_mul(self.limit("maximum_sqlite_write_transaction_pages"));
        let transaction_log = frame.and_then(|frame| {
            self.limit("maximum_sqlite_write_transaction_pages").checked_mul(frame)
        });
        let log = frame
            .and_then(|frame| {
                self.limit("maximum_sqlite_write_ahead_log_frames").checked_mul(frame)
            })
            .and_then(|frames| {
                frames.checked_add(self.limit("sqlite_write_ahead_log_header_bytes"))
            });
        let shared = self.limit("maximum_sqlite_shared_memory_bytes");
        let physical = database
            .and_then(|database| log.and_then(|log| database.checked_add(log)))
            .and_then(|held| held.checked_add(shared));
        let replacement = database
            .and_then(|database| database.checked_mul(REPLACEMENT_DATABASE_COPIES))
            .and_then(|doubled| log.and_then(|log| doubled.checked_add(log)))
            .and_then(|held| held.checked_add(shared));
        let expected = [
            ("maximum_sqlite_database_bytes", database),
            ("maximum_sqlite_write_transaction_bytes", transaction),
            ("maximum_sqlite_write_transaction_write_ahead_log_bytes", transaction_log),
            ("maximum_sqlite_write_ahead_log_bytes", log),
            (
                "sqlite_write_backpressure_bytes",
                log.and_then(|log| transaction_log.and_then(|held| log.checked_sub(held))),
            ),
            ("maximum_sqlite_physical_bytes", physical),
            ("maximum_sqlite_replacement_physical_bytes", replacement),
            ("persistent_filesystem_safety_reserve_bytes", replacement),
            (
                "maximum_total_diagnostic_bytes",
                self.limit("maximum_diagnostic_file_bytes")
                    .checked_mul(self.limit("maximum_retained_diagnostic_files")),
            ),
            (
                "maximum_terminal_maintenance_result_associations_per_target",
                self.limit("maximum_terminal_maintenance_application_receipts_per_target")
                    .checked_mul(DOCUMENTS_PER_RECEIPT)
                    .and_then(|held| held.checked_add(CURRENT_PREVIEW_ASSOCIATIONS)),
            ),
        ];
        for (name, produced) in expected {
            if produced != Some(self.formula(name)) {
                return Err(DaemonRuntimeContractFailure::FormulaInconsistent {
                    name: name.to_owned(),
                });
            }
        }
        Ok(())
    }
}
