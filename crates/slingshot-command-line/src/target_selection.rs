//! Deciding which profile and environment a command speaks to.
//!
//! Three leaves need three different things, and conflating them costs
//! something real each time. Help and version need no target at all, and asking
//! for one would make them fail on a machine with no configuration - which is
//! exactly the machine somebody runs them on. The daemon lifecycle probes need
//! only the two names, because a daemon that is already running is found by its
//! namespace and a caller must be able to stop one whose profile has since
//! become unreadable. Everything else needs the whole selection, because it is
//! going to act against an author and the identity it acts under has to be
//! derived from what is actually configured.
//!
//! # Names before content
//!
//! The namespace is built from the two names alone and never from profile
//! content. That is what lets a stop reach a daemon whose configuration broke
//! after it started, and it is why the name resolution is a separate function
//! from the complete one rather than a first step inside it.
//!
//! # Nothing here reaches an author
//!
//! Deriving the target identity and the environment revision reads the
//! configuration and nothing else. Both values are nonsecret and both are
//! derived rather than fetched, so resolving a target says nothing to anybody.

use slingshot_configuration::profile_loader::{ConfigurationDiagnostic, LoadedProfiles};
use slingshot_configuration::profile_selection::{ProfileSelection, RequestedSelection, resolve};
use slingshot_domain::profile::{EnvironmentName, ProfileName};

use crate::invocation::{METADATA_ONLY_LEAVES, Selection};

/// The leaves that are found by namespace and need no profile content.
///
/// A caller must be able to stop a daemon whose configuration broke after it
/// started. Requiring the content here would make the broken configuration
/// unfixable without killing the process by hand.
pub const NAMESPACE_ONLY_LEAVES: &[&str] = &["daemon-ping", "daemon-status", "daemon-stop"];

/// The separator between the two names of a namespace.
const NAMESPACE_SEPARATOR: char = '/';

/// What one leaf needs before it can act.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TargetRequirement {
    /// Nothing at all.
    None,
    /// The two names, and no profile content.
    NamespaceOnly,
    /// The whole selection, including the identity it will act under.
    Complete,
}

/// Returns what `leaf` needs before it can act.
#[must_use]
pub fn requirement_of(leaf: &str) -> TargetRequirement {
    if METADATA_ONLY_LEAVES.contains(&leaf) {
        return TargetRequirement::None;
    }
    if NAMESPACE_ONLY_LEAVES.contains(&leaf) {
        return TargetRequirement::NamespaceOnly;
    }
    TargetRequirement::Complete
}

/// The two names one daemon is found by.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NamespacePair {
    /// Which environment.
    pub environment: String,
    /// Which profile.
    pub profile: String,
}

impl NamespacePair {
    /// Returns the key this pair is one daemon under.
    ///
    /// The same spelling the configuration crate produces, because two spellings
    /// of one namespace would be two daemons.
    #[must_use]
    pub fn key(&self) -> String {
        format!("{}{NAMESPACE_SEPARATOR}{}", self.profile, self.environment)
    }
}

/// A complete target, with the identity a command will act under.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedTarget {
    /// The partition this acts in.
    pub author_target_identity_digest: String,
    /// The two names it is found by.
    pub namespace: NamespacePair,
    /// The revision it acts under.
    pub selected_environment_revision: String,
}

/// Why a target could not be resolved.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum SelectionRefusal {
    /// One name was supplied and the other was not.
    #[error("a target is a profile and an environment, and this names one of them")]
    SelectionIncomplete,
    /// The configuration refused, in its own closed vocabulary.
    #[error("the configuration refused with {count} diagnostics")]
    Configuration {
        /// How many it produced.
        count: usize,
        /// What they were, unchanged.
        diagnostics: Vec<ConfigurationDiagnostic>,
    },
    /// A name is not one a profile or environment may be called.
    #[error("{named} is not a name a profile or an environment may have")]
    NameUnusable {
        /// What was supplied.
        named: String,
    },
}

/// Returns the namespace two supplied names make, reading no profile content.
///
/// # Errors
///
/// Returns [`SelectionRefusal::SelectionIncomplete`] or
/// [`SelectionRefusal::NameUnusable`].
pub fn namespace_of(selection: &Selection) -> Result<NamespacePair, SelectionRefusal> {
    let (Some(profile), Some(environment)) = (&selection.profile, &selection.environment) else {
        return Err(SelectionRefusal::SelectionIncomplete);
    };
    require_usable_name(profile)?;
    require_usable_name(environment)?;
    Ok(NamespacePair { environment: environment.clone(), profile: profile.clone() })
}

/// Requires one supplied name to be one the configuration grammar admits.
fn require_usable_name(named: &str) -> Result<(), SelectionRefusal> {
    let usable = !named.is_empty()
        && named.chars().all(|character| {
            character.is_ascii_lowercase() || character.is_ascii_digit() || character == '-'
        });
    if usable { Ok(()) } else { Err(SelectionRefusal::NameUnusable { named: named.to_owned() }) }
}

/// Returns what a caller asked for, as the configuration crate reads it.
///
/// # Errors
///
/// Returns [`SelectionRefusal::NameUnusable`] when a supplied name is not one
/// the grammar admits, which is refused here rather than turned into a lookup
/// that could not have matched anything.
pub fn requested(selection: &Selection) -> Result<RequestedSelection, SelectionRefusal> {
    let named = |held: &Option<String>| -> Result<Option<String>, SelectionRefusal> {
        match held {
            Some(value) => {
                require_usable_name(value)?;
                Ok(Some(value.clone()))
            }
            None => Ok(None),
        }
    };
    Ok(RequestedSelection {
        environment: named(&selection.environment)?
            .and_then(|value| EnvironmentName::parse(&value).ok()),
        profile: named(&selection.profile)?.and_then(|value| ProfileName::parse(&value).ok()),
    })
}

/// Returns the selection `loaded` and `selection` resolve to.
///
/// # Errors
///
/// Returns [`SelectionRefusal::Configuration`] carrying the configuration's own
/// closed diagnostics, unchanged. They are passed through rather than
/// summarized because the vocabulary is deliberately bounded and rewording it
/// here would put a second, looser one beside it.
pub fn select(
    loaded: &LoadedProfiles,
    selection: &Selection,
) -> Result<ProfileSelection, SelectionRefusal> {
    resolve(loaded, &requested(selection)?).map_err(|diagnostics| SelectionRefusal::Configuration {
        count: diagnostics.len(),
        diagnostics,
    })
}

/// Returns the namespace one resolved selection is a daemon under.
#[must_use]
pub fn namespace_of_selection(selection: &ProfileSelection) -> NamespacePair {
    NamespacePair {
        environment: selection.environment_name().as_text().to_owned(),
        profile: selection.profile_name().as_text().to_owned(),
    }
}
