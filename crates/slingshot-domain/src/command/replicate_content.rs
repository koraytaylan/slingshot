//! Offering content to the author replication service.
//!
//! Two things about this command are easy to get wrong and are therefore said
//! plainly here.
//!
//! First, what success means. `accepted_item_count` counts paths the *author*
//! accepted for replication. It is not delivery: nothing here observes a
//! publisher, and a result saying every path was accepted says nothing about
//! whether any of them arrived. Naming the field for admission rather than for
//! replication is the whole point.
//!
//! Second, what a failure claims. A rejection or a budget failure says the
//! current path was not accepted, so a zero accepted count is an honest claim
//! of no effect and a positive one is an honest claim of partial effect. An
//! unknown outcome says something different and weaker: the accepted count is
//! only the durably confirmed prefix, and the current path may or may not have
//! been admitted. That distinction exists because the alternative - guessing -
//! would let a retry replicate something twice or report as unchanged something
//! that changed.
//!
//! A recursive command collects its whole manifest before offering anything, so
//! content added while the offering runs does not join it. The manifest is what
//! was there when the command started, and it is finite.

use serde::de::Error as _;
use serde::{Deserialize, Serialize};

use crate::command::command_identity::CommandContract;
use crate::command::query_paths::require_strictly_ascending;
use crate::command::repository_path::RepositoryPath;

/// Returns the most paths one replication may collect.
#[must_use]
pub fn maximum_replication_candidate_paths() -> u64 {
    CommandContract::embedded().limit("maximum_replication_candidate_paths")
}

/// Returns how long collecting them may take.
#[must_use]
pub fn maximum_replication_traversal_duration_milliseconds() -> u64 {
    CommandContract::embedded().limit("maximum_replication_traversal_duration_milliseconds")
}

/// Returns how long offering them may take.
#[must_use]
pub fn maximum_replication_admission_duration_milliseconds() -> u64 {
    CommandContract::embedded().limit("maximum_replication_admission_duration_milliseconds")
}

/// Reason a replication value could not be built.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum ReplicationFailure {
    /// A manifest is empty, unordered, or larger than the contract allows.
    #[error("a replication manifest is nonempty, strictly ascending, and at most {maximum} paths", maximum = maximum_replication_candidate_paths())]
    ManifestNotCanonical,
    /// The counts do not add up to the manifest they describe.
    #[error("an accepted count and a remaining count sum to the manifest size")]
    CountsDoNotSum,
    /// A failure names a path that is not the one it stopped at.
    #[error("a replication failure names the manifest path it stopped at")]
    CurrentPathNotInManifest,
    /// A result does not answer the command it claims to answer.
    #[error("a replication result echoes the source its command asked about")]
    NotThisRequest,
}

/// Where one path stands in the offering.
///
/// Durable, and checked before each offer. `InFlight` is the state that makes
/// an interrupted run resolvable: a path found in it after a restart resolves
/// to an unknown outcome and is never offered again, which is what stops an
/// ambiguous retry from silently admitting the same content twice.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AdmissionCheckpoint {
    /// Not yet offered. A physical retry may resume here.
    NotStarted,
    /// Offered, with no durable answer yet.
    InFlight,
    /// Durably accepted by the author. Never offered again.
    Accepted,
}

impl AdmissionCheckpoint {
    /// Returns whether a physical retry may offer this path.
    ///
    /// Only a path that was never offered. `InFlight` is ambiguous and
    /// `Accepted` is done, and re-offering either would risk a second
    /// admission of the same content.
    #[must_use]
    pub fn may_be_offered(self) -> bool {
        matches!(self, Self::NotStarted)
    }
}

/// One request to replicate content.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ReplicateContentCommand {
    /// Content to replicate.
    pub path: RepositoryPath,
    /// Whether to include everything below it.
    pub recursive: bool,
}

/// The paths one replication will offer, fixed before any is offered.
///
/// Collected completely first, so content added while the offering runs does
/// not join it. Deduplicated and ascending, so the same node is never offered
/// twice and the order a retry resumes in is the order it started with.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(transparent)]
pub struct ReplicationManifest {
    /// The paths, strictly ascending.
    paths: Vec<RepositoryPath>,
}

impl ReplicationManifest {
    /// Returns the manifest `paths` describe.
    ///
    /// # Errors
    ///
    /// Returns [`ReplicationFailure::ManifestNotCanonical`] when the manifest is
    /// empty, repeats a path, is out of order, or is larger than the contract
    /// allows.
    pub fn new(paths: Vec<RepositoryPath>) -> Result<Self, ReplicationFailure> {
        let bounded = !paths.is_empty()
            && u64::try_from(paths.len()).unwrap_or(u64::MAX)
                <= maximum_replication_candidate_paths();
        if !bounded || require_strictly_ascending(paths.iter()).is_err() {
            return Err(ReplicationFailure::ManifestNotCanonical);
        }
        Ok(Self { paths })
    }

    /// Returns the paths, strictly ascending.
    #[must_use]
    pub fn paths(&self) -> &[RepositoryPath] {
        &self.paths
    }

    /// Returns how many paths this manifest holds.
    #[must_use]
    pub fn size(&self) -> u64 {
        u64::try_from(self.paths.len()).unwrap_or(u64::MAX)
    }
}

/// Why a replication produced no successful result before offering anything.
///
/// Each carries the source it was asked about and nothing else. None of them
/// admitted a single path, so none carries a count that might read as partial
/// progress.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "failure", rename_all = "snake_case", deny_unknown_fields)]
pub enum PreflightRefusal {
    /// The source is not there.
    SourceNotFound {
        /// Source that is not there.
        source_path: RepositoryPath,
    },
    /// The source is there and unreadable.
    SourceAccessDenied {
        /// Source that could not be read.
        source_path: RepositoryPath,
    },
    /// The subtree holds more paths than the contract collects.
    CandidateLimitExceeded {
        /// Source whose subtree is too large.
        source_path: RepositoryPath,
    },
    /// Collecting them ran out of time.
    TraversalBudgetExceeded {
        /// Source that was being collected.
        source_path: RepositoryPath,
    },
}

impl PreflightRefusal {
    /// Returns the source this refusal names.
    #[must_use]
    pub fn source_path(&self) -> &RepositoryPath {
        match self {
            Self::SourceNotFound { source_path }
            | Self::SourceAccessDenied { source_path }
            | Self::CandidateLimitExceeded { source_path }
            | Self::TraversalBudgetExceeded { source_path } => source_path,
        }
    }

    /// Requires this refusal to answer `command`.
    ///
    /// # Errors
    ///
    /// Returns [`ReplicationFailure::NotThisRequest`] when the echoed source is
    /// another request's.
    pub fn require_answers(
        &self,
        command: &ReplicateContentCommand,
    ) -> Result<(), ReplicationFailure> {
        if *self.source_path() == command.path {
            Ok(())
        } else {
            Err(ReplicationFailure::NotThisRequest)
        }
    }
}

/// Why an offering stopped partway.
///
/// The three differ in what they claim about the current path, which is the
/// only thing a caller can act on. Rejection and budget say it was not
/// accepted. Unknown says nobody can tell.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AdmissionOutcome {
    /// The author refused the current path. It was not accepted.
    AdmissionRejected,
    /// Time ran out before the current path was offered. It was not accepted.
    AdmissionBudgetExceeded,
    /// The current path was offered and no durable answer arrived.
    AdmissionOutcomeUnknown,
}

impl AdmissionOutcome {
    /// Returns whether this outcome proves the current path was not accepted.
    #[must_use]
    pub fn proves_current_path_not_accepted(self) -> bool {
        !matches!(self, Self::AdmissionOutcomeUnknown)
    }
}

/// One offering that stopped partway.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AdmissionRefusal {
    /// Paths durably accepted before this one.
    pub accepted_item_count: u64,
    /// Path the offering stopped at.
    pub current_path: RepositoryPath,
    /// Which of the three this is.
    pub failure: AdmissionOutcome,
    /// This path and every later one.
    pub remaining_item_count: u64,
}

impl AdmissionRefusal {
    /// Requires the counts to describe `manifest` and to stop where they say.
    ///
    /// # Errors
    ///
    /// Returns [`ReplicationFailure::CountsDoNotSum`] when the two counts do
    /// not add to the manifest size, and
    /// [`ReplicationFailure::CurrentPathNotInManifest`] when the named path is
    /// not the one at the accepted count's position.
    pub fn require_consistent(
        &self,
        manifest: &ReplicationManifest,
    ) -> Result<(), ReplicationFailure> {
        let total = self
            .accepted_item_count
            .checked_add(self.remaining_item_count)
            .ok_or(ReplicationFailure::CountsDoNotSum)?;
        if total != manifest.size() {
            return Err(ReplicationFailure::CountsDoNotSum);
        }
        let position = usize::try_from(self.accepted_item_count)
            .map_err(|_| ReplicationFailure::CountsDoNotSum)?;
        if manifest.paths().get(position) != Some(&self.current_path) {
            return Err(ReplicationFailure::CurrentPathNotInManifest);
        }
        Ok(())
    }
}

/// What a completed replication admitted.
///
/// Reached only when every path in the manifest was accepted, so the count
/// always equals the manifest size. It is author-side admission, never
/// publisher delivery.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ReplicateContentResult {
    /// Paths the author accepted, which is all of them.
    pub accepted_item_count: u64,
}

impl ReplicateContentResult {
    /// Returns the result for a manifest that was accepted in full.
    #[must_use]
    pub fn complete(manifest: &ReplicationManifest) -> Self {
        Self { accepted_item_count: manifest.size() }
    }

    /// Requires this result to describe `manifest` in full.
    ///
    /// # Errors
    ///
    /// Returns [`ReplicationFailure::CountsDoNotSum`] when the count is
    /// anything but the manifest size, because a success that admitted only
    /// some of its paths is not a success.
    pub fn require_complete(
        &self,
        manifest: &ReplicationManifest,
    ) -> Result<(), ReplicationFailure> {
        if self.accepted_item_count == manifest.size() {
            Ok(())
        } else {
            Err(ReplicationFailure::CountsDoNotSum)
        }
    }
}

/// One manifest exactly as it is written on the wire.
#[derive(Debug, Deserialize)]
#[serde(transparent)]
struct ManifestDocument {
    /// The paths this manifest holds.
    paths: Vec<RepositoryPath>,
}

impl<'de> Deserialize<'de> for ReplicationManifest {
    fn deserialize<Source: serde::Deserializer<'de>>(
        deserializer: Source,
    ) -> Result<Self, Source::Error> {
        let document = ManifestDocument::deserialize(deserializer)?;
        Self::new(document.paths).map_err(Source::Error::custom)
    }
}
