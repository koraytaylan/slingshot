//! Building one content package from roots and ordered selection filters.
//!
//! The command returns a descriptor, never bytes. Package bytes stay opaque to
//! the result, Slingshot never installs or imports what it built, and the
//! recorded import mode is a warning to whoever does - not evidence that
//! anything was imported.
//!
//! # What the filters mean
//!
//! Inclusion and exclusion are both anchor-and-subtree rules. A path matching
//! an inclusion expression is an anchor, and it plus its descendants inside the
//! root union are admitted; a path matching an exclusion expression is an
//! anchor, and it plus its descendants are removed. Exclusion wins whatever
//! order the expressions were written in, so an exclusion of `/content/bar/baz`
//! removes that node and everything under it rather than leaving unmatched
//! descendants behind.
//!
//! With no inclusion expression at all, each root is its own anchor - "package
//! this subtree" is the obvious meaning of naming a root and nothing else.
//!
//! # Structural ancestors
//!
//! FileVault needs the nodes between a root and the selected content to exist
//! in order to reach it. Those ancestors are carried as directories alone: no
//! properties, no siblings, no children of their own. The distinction matters
//! because an ancestor is usually somebody else's content, and packaging its
//! properties because it happened to be on the way would export things nobody
//! asked for.

use serde::de::Error as _;
use serde::{Deserialize, Serialize};

use crate::command::artifact::{ArtifactDescriptor, ArtifactSlotDeclaration};
use crate::command::command_identity::CommandContract;
use crate::command::package_selection::PackagePathSelectionExpression;
use crate::command::query_paths::require_strictly_ascending;
use crate::command::repository_path::RepositoryPath;

/// Suffix the suggested file name always ends with.
pub const PACKAGE_FILE_NAME_SUFFIX: &str = ".zip";

/// Profile this contract builds packages under.
pub const FILEVAULT_PROFILE: &str = "slingshot.filevault-merge-properties/1";

/// Import mode that profile records.
pub const FILEVAULT_IMPORT_MODE: &str = "merge_properties";

/// Access-control handling that profile records.
pub const FILEVAULT_ACCESS_CONTROL_HANDLING: &str = "ignore";

/// Smallest FileVault implementation that profile is supported on.
pub const MINIMUM_FILEVAULT_VERSION: &str = "3.5.0";

/// Capabilities that implementation must expose.
pub const REQUIRED_FILEVAULT_CAPABILITIES: &[&str] = &["merge_properties", "matchProperties"];

/// Returns the largest package name this contract accepts.
#[must_use]
pub fn maximum_package_name_bytes() -> u64 {
    CommandContract::embedded().limit("maximum_package_name_bytes")
}

/// Returns the largest suggested file name this contract produces.
#[must_use]
pub fn maximum_package_suggested_file_name_bytes() -> u64 {
    CommandContract::embedded().limit("maximum_package_suggested_file_name_bytes")
}

/// Returns the most roots one request may name.
#[must_use]
pub fn maximum_package_roots() -> u64 {
    CommandContract::embedded().limit("maximum_package_roots")
}

/// Returns the most inclusion expressions one request may name.
#[must_use]
pub fn maximum_package_inclusion_expressions() -> u64 {
    CommandContract::embedded().limit("maximum_package_inclusion_expressions")
}

/// Returns the most exclusion expressions one request may name.
#[must_use]
pub fn maximum_package_exclusion_expressions() -> u64 {
    CommandContract::embedded().limit("maximum_package_exclusion_expressions")
}

/// Returns the most paths one selection may admit.
#[must_use]
pub fn maximum_package_selected_paths() -> u64 {
    CommandContract::embedded().limit("maximum_package_selected_paths")
}

/// Returns the largest package this contract produces.
#[must_use]
pub fn maximum_package_output_bytes() -> u64 {
    CommandContract::embedded().limit("maximum_package_output_bytes")
}

/// Reason a package value could not be built.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum PackageFailure {
    /// A package name is empty, over bound, or outside its alphabet.
    #[error("a package name is nonempty ASCII letters, digits, hyphens, and underscores, at most {maximum} bytes", maximum = maximum_package_name_bytes())]
    NameNotCanonical,
    /// The suggested file name that name produces is over its own bound.
    #[error("a suggested file name is at most {maximum} bytes", maximum = maximum_package_suggested_file_name_bytes())]
    FileNameTooLong,
    /// A request named no root, repeated one, or listed them out of order.
    #[error("package roots are a nonempty strictly ascending set of at most {maximum}", maximum = maximum_package_roots())]
    RootsNotCanonical,
    /// A request named more expressions than the contract allows.
    #[error("a filter collection stays inside its named bound")]
    TooManyExpressions,
    /// A selection admitted more paths than the contract allows.
    #[error("a selection admits at most {maximum} paths", maximum = maximum_package_selected_paths())]
    TooManySelectedPaths,
    /// An artifact does not fill the slot this command declares.
    #[error(
        "a package artifact fills the slot this command declares, with its exact media type and suggested file name"
    )]
    ArtifactDoesNotMatchSlot,
    /// A result does not answer the command it claims to answer.
    #[error("a package result carries the suggested file name its command's package name produces")]
    NotThisRequest,
}

/// The stem a package's file name is built from.
///
/// Deliberately narrow: ASCII letters, digits, hyphen, and underscore. A name
/// that has to survive a filesystem, a download header, and an archive index
/// should not contain anything any of them would argue about.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct PackageName {
    /// The stem, exactly as it arrived.
    value: String,
}

impl PackageName {
    /// Returns the name `spelling` carries.
    ///
    /// # Errors
    ///
    /// Returns [`PackageFailure::NameNotCanonical`] for an empty, over-bound,
    /// or out-of-alphabet spelling, and [`PackageFailure::FileNameTooLong`]
    /// when the file name it produces is over its own separate bound.
    pub fn new(spelling: impl Into<String>) -> Result<Self, PackageFailure> {
        let value = spelling.into();
        let canonical = !value.is_empty()
            && u64::try_from(value.len()).unwrap_or(u64::MAX) <= maximum_package_name_bytes()
            && value
                .chars()
                .all(|character| character.is_ascii_alphanumeric() || "-_".contains(character));
        if !canonical {
            return Err(PackageFailure::NameNotCanonical);
        }
        let name = Self { value };
        if u64::try_from(name.suggested_file_name().len()).unwrap_or(u64::MAX)
            > maximum_package_suggested_file_name_bytes()
        {
            return Err(PackageFailure::FileNameTooLong);
        }
        Ok(name)
    }

    /// Returns the stem, exactly as it arrived.
    #[must_use]
    pub fn as_text(&self) -> &str {
        &self.value
    }

    /// Returns the file name this package suggests.
    ///
    /// Presentation metadata. It does not take part in the artifact identifier,
    /// so renaming a package does not rename the thing it produced.
    #[must_use]
    pub fn suggested_file_name(&self) -> String {
        format!("{}{PACKAGE_FILE_NAME_SUFFIX}", self.value)
    }
}

impl TryFrom<String> for PackageName {
    type Error = PackageFailure;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        Self::new(value)
    }
}

impl From<PackageName> for String {
    fn from(name: PackageName) -> Self {
        name.value
    }
}

/// The roots one package is built from.
///
/// A union, deduplicated and ascending. Overlapping roots collapse, so naming
/// `/content` and `/content/example` packages the first subtree once rather
/// than packaging the second twice.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(transparent)]
pub struct PackageRoots {
    /// The roots, ascending.
    paths: Vec<RepositoryPath>,
}

impl PackageRoots {
    /// Returns the roots `paths` name.
    ///
    /// # Errors
    ///
    /// Returns [`PackageFailure::RootsNotCanonical`] when the collection is
    /// empty, repeats a root, is out of order, or is larger than the contract
    /// allows.
    pub fn new(paths: Vec<RepositoryPath>) -> Result<Self, PackageFailure> {
        let bounded = !paths.is_empty()
            && u64::try_from(paths.len()).unwrap_or(u64::MAX) <= maximum_package_roots();
        if !bounded || require_strictly_ascending(paths.iter()).is_err() {
            return Err(PackageFailure::RootsNotCanonical);
        }
        Ok(Self { paths })
    }

    /// Returns the roots, ascending.
    #[must_use]
    pub fn paths(&self) -> &[RepositoryPath] {
        &self.paths
    }

    /// Returns whether any root is at or above `path`.
    #[must_use]
    pub fn contain(&self, path: &RepositoryPath) -> bool {
        self.paths.iter().any(|root| crate::command::query_paths::anchor_contains(root, path))
    }
}

impl<'de> Deserialize<'de> for PackageRoots {
    fn deserialize<Source: serde::Deserializer<'de>>(
        deserializer: Source,
    ) -> Result<Self, Source::Error> {
        let paths = Vec::<RepositoryPath>::deserialize(deserializer)?;
        Self::new(paths).map_err(Source::Error::custom)
    }
}

/// One ordered collection of selection expressions.
///
/// Order is preserved and is not a precedence: every expression is evaluated,
/// with no short-circuiting, so the work one candidate costs is the same
/// whatever the answer turns out to be.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(transparent)]
pub struct SelectionFilters {
    /// The expressions, in the order they were given.
    expressions: Vec<PackagePathSelectionExpression>,
}

impl SelectionFilters {
    /// Returns the collection `expressions` spell, bounded by `maximum`.
    ///
    /// # Errors
    ///
    /// Returns [`PackageFailure::TooManyExpressions`] above that bound.
    pub fn new(
        expressions: Vec<PackagePathSelectionExpression>,
        maximum: u64,
    ) -> Result<Self, PackageFailure> {
        if u64::try_from(expressions.len()).unwrap_or(u64::MAX) > maximum {
            return Err(PackageFailure::TooManyExpressions);
        }
        Ok(Self { expressions })
    }

    /// Returns the expressions, in the order they were given.
    #[must_use]
    pub fn expressions(&self) -> &[PackagePathSelectionExpression] {
        &self.expressions
    }

    /// Returns whether `candidate` matches any expression here.
    ///
    /// Every expression is evaluated. The answer is the same as a
    /// short-circuiting one would give; the cost is not, and the cost is what
    /// this command charges.
    #[must_use]
    pub fn any_matches(&self, candidate: &RepositoryPath) -> bool {
        let mut matched = false;
        for expression in &self.expressions {
            if expression.matches(candidate).unwrap_or(false) {
                matched = true;
            }
        }
        matched
    }
}

impl<'de> Deserialize<'de> for SelectionFilters {
    fn deserialize<Source: serde::Deserializer<'de>>(
        deserializer: Source,
    ) -> Result<Self, Source::Error> {
        let expressions = Vec::<PackagePathSelectionExpression>::deserialize(deserializer)?;
        let widest =
            maximum_package_inclusion_expressions().max(maximum_package_exclusion_expressions());
        Self::new(expressions, widest).map_err(Source::Error::custom)
    }
}

/// One request to build a content package.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DownloadContentPackageCommand {
    /// Paths whose subtrees are removed, in order.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub exclusion_filters: Option<SelectionFilters>,
    /// Paths whose subtrees are admitted, in order.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub inclusion_filters: Option<SelectionFilters>,
    /// Stem the produced file is named from.
    pub package_name: PackageName,
    /// Subtrees to package.
    pub roots: PackageRoots,
}

impl DownloadContentPackageCommand {
    /// Requires both filter collections to fit their separate bounds.
    ///
    /// # Errors
    ///
    /// Returns [`PackageFailure::TooManyExpressions`] when either is larger
    /// than its own maximum.
    pub fn require_bounded_filters(&self) -> Result<(), PackageFailure> {
        let bounded = |filters: Option<&SelectionFilters>, maximum: u64| {
            filters.is_none_or(|filters| {
                u64::try_from(filters.expressions().len()).unwrap_or(u64::MAX) <= maximum
            })
        };
        if bounded(self.inclusion_filters.as_ref(), maximum_package_inclusion_expressions())
            && bounded(self.exclusion_filters.as_ref(), maximum_package_exclusion_expressions())
        {
            Ok(())
        } else {
            Err(PackageFailure::TooManyExpressions)
        }
    }
}

/// What one enumeration selected, and what it needs to reach it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct PackageSelection {
    /// Paths an exclusion expression anchored, ascending.
    pub exclusion_anchors: Vec<RepositoryPath>,
    /// Paths an inclusion expression anchored, ascending.
    pub include_anchors: Vec<RepositoryPath>,
    /// Paths packaged with their properties, ascending.
    pub selected_content: Vec<RepositoryPath>,
    /// Paths packaged as directories alone, ascending.
    pub structural_ancestors: Vec<RepositoryPath>,
}

impl PackageSelection {
    /// Returns what `candidates` select under `command`.
    ///
    /// `candidates` is every path the enumeration visited, ascending. A
    /// candidate is selected when some include anchor is at or above it and no
    /// exclusion anchor is - exclusion winning outright is what makes an
    /// exclusion of a subtree remove the whole subtree.
    ///
    /// # Errors
    ///
    /// Returns [`PackageFailure::TooManySelectedPaths`] when the selection is
    /// larger than the contract admits.
    pub fn compute(
        command: &DownloadContentPackageCommand,
        candidates: &[RepositoryPath],
    ) -> Result<Self, PackageFailure> {
        let inside = |path: &RepositoryPath| command.roots.contain(path);
        let include_anchors: Vec<RepositoryPath> = match command.inclusion_filters.as_ref() {
            None => command.roots.paths().to_vec(),
            Some(filters) => candidates
                .iter()
                .filter(|path| inside(path) && filters.any_matches(path))
                .cloned()
                .collect(),
        };
        let exclusion_anchors: Vec<RepositoryPath> = command
            .exclusion_filters
            .as_ref()
            .map(|filters| {
                candidates.iter().filter(|path| filters.any_matches(path)).cloned().collect()
            })
            .unwrap_or_default();
        let anchored = |anchors: &[RepositoryPath], path: &RepositoryPath| {
            anchors.iter().any(|anchor| crate::command::query_paths::anchor_contains(anchor, path))
        };
        let selected_content: Vec<RepositoryPath> = candidates
            .iter()
            .filter(|path| {
                inside(path)
                    && anchored(&include_anchors, path)
                    && !anchored(&exclusion_anchors, path)
            })
            .cloned()
            .collect();
        if u64::try_from(selected_content.len()).unwrap_or(u64::MAX)
            > maximum_package_selected_paths()
        {
            return Err(PackageFailure::TooManySelectedPaths);
        }
        let structural_ancestors = structural_ancestors(command.roots.paths(), &selected_content);
        Ok(Self { exclusion_anchors, include_anchors, selected_content, structural_ancestors })
    }

    /// Returns whether this selection packages nothing.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.selected_content.is_empty()
    }
}

/// Returns the nodes between the roots and the selected content.
///
/// Only what is needed to reach the selection: a path that is itself selected
/// is not also structural, and a root that is above nothing selected
/// contributes nothing.
fn structural_ancestors(
    roots: &[RepositoryPath],
    selected: &[RepositoryPath],
) -> Vec<RepositoryPath> {
    let mut ancestors: Vec<RepositoryPath> = Vec::new();
    for path in selected {
        let mut walking = path.parent();
        while let Some(ancestor) = walking {
            let inside_a_root = roots
                .iter()
                .any(|root| crate::command::query_paths::anchor_contains(root, &ancestor));
            if !inside_a_root {
                break;
            }
            if !selected.contains(&ancestor) && !ancestors.contains(&ancestor) {
                ancestors.push(ancestor.clone());
            }
            walking = ancestor.parent();
        }
    }
    ancestors.sort_by(|left, right| left.as_text().as_bytes().cmp(right.as_text().as_bytes()));
    ancestors
}

/// Which budget a package build ran out of.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PackageBudget {
    /// Paths the enumeration visited.
    CandidatePaths,
    /// Matcher cells the expressions filled.
    PatternEvaluations,
    /// Paths the selection admitted.
    SelectedPaths,
    /// Bytes the filter document reached.
    FilterDocumentBytes,
    /// Bytes the package manifest reached.
    PackageManifestBytes,
    /// Entries the archive reached.
    ArchiveEntries,
    /// Bytes read out of the repository.
    UncompressedInputBytes,
    /// Bytes the package itself reached.
    PackageOutputBytes,
}

/// Which collection an expression that was refused came from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FilterCollection {
    /// The inclusion filters.
    Inclusion,
    /// The exclusion filters.
    Exclusion,
}

/// Why a package build produced no artifact.
///
/// Every category except `ArtifactPublicationOutcomeUnknown` published nothing,
/// and says so. Outcome unknown says the opposite of nothing: an artifact may
/// exist, which is why it forbids another build until the operation has been
/// reconciled rather than inviting a retry that would publish twice.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "failure", rename_all = "snake_case", deny_unknown_fields)]
pub enum DownloadContentPackageRefusal {
    /// One expression was refused, named by where it stood.
    ///
    /// The index and the collection, never the text: an expression a caller
    /// wrote is a caller's business and echoing it into a failure would put it
    /// somewhere it was not sent.
    PatternRejected {
        /// Which collection it came from.
        collection: FilterCollection,
        /// Where it stood in that collection, counting from zero.
        expression_index: u64,
    },
    /// The installed FileVault cannot build under this profile.
    FilevaultProfileUnsupported,
    /// Some path cannot be written into a filter document.
    FilevaultFilterUnrepresentable,
    /// A root is not there.
    RootNotFound {
        /// Root that is not there.
        root_path: RepositoryPath,
    },
    /// A root is there and unreadable.
    RootAccessDenied {
        /// Root that could not be read.
        root_path: RepositoryPath,
    },
    /// Reading the repository failed.
    RepositoryReadFailed,
    /// Building the package failed.
    FilevaultPackageFailed,
    /// Unpublished staging could not be removed.
    StagingCleanupFailed,
    /// Publishing the artifact was refused, provably before it happened.
    ArtifactPublicationFailed,
    /// Publishing the artifact may or may not have happened.
    ArtifactPublicationOutcomeUnknown,
    /// The build ran out of one of its budgets.
    EvaluationBudgetExceeded {
        /// Budget that ran out.
        budget: PackageBudget,
    },
}

impl DownloadContentPackageRefusal {
    /// Returns whether this refusal proves no artifact was published.
    #[must_use]
    pub fn proves_no_publication(&self) -> bool {
        !matches!(self, Self::ArtifactPublicationOutcomeUnknown)
    }
}

/// What a completed package build produced.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DownloadContentPackageResult {
    /// Descriptor of the package's bytes, which are not here.
    pub artifact: ArtifactDescriptor,
}

impl DownloadContentPackageResult {
    /// Requires this result to answer `command`.
    ///
    /// # Errors
    ///
    /// Returns [`PackageFailure::ArtifactDoesNotMatchSlot`] when the descriptor
    /// does not fill the declared slot, and [`PackageFailure::NotThisRequest`]
    /// when its suggested file name is not the one this command's package name
    /// produces.
    pub fn require_answers(
        &self,
        command: &DownloadContentPackageCommand,
    ) -> Result<(), PackageFailure> {
        let declaration = ArtifactSlotDeclaration::content_package();
        declaration.admit(&self.artifact).map_err(|_| PackageFailure::ArtifactDoesNotMatchSlot)?;
        if self.artifact.media_type != declaration.media_type {
            return Err(PackageFailure::ArtifactDoesNotMatchSlot);
        }
        if self.artifact.suggested_file_name.as_text() != command.package_name.suggested_file_name()
        {
            return Err(PackageFailure::NotThisRequest);
        }
        Ok(())
    }
}
