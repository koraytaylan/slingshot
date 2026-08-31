//! Reporting what the selected configuration actually resolves to.
//!
//! This is the command a person runs before trusting anything else, so what it
//! must not do is as important as what it does. It reads configuration and the
//! files that configuration references, and it reaches nothing else: no daemon,
//! no process, no network. A check that could fail because a daemon was down
//! would be useless exactly when it is needed.
//!
//! # The report says what is wrong and not where
//!
//! Failures come through as Plan 0002's own closed diagnostics, unchanged: a
//! source class, a stage, a structural location from the bounded manifest
//! vocabulary, a code, and a count. Nothing is added - no path, no name, no
//! digest, no value, no suggestion. A report that named the file it could not
//! read would enumerate the configuration root for whoever ran it, and one that
//! preserved discovery order would say which source was read first.
//!
//! # Success says the two derived values and nothing about how
//!
//! When a selection resolves, the report carries the author-target identity and
//! the selected-environment revision. Both are nonsecret and both are derived
//! from normalized configuration rather than fetched, so producing them says
//! nothing to anybody and reveals nothing that rotating a credential would
//! change.

use slingshot_configuration::profile_loader::{ConfigurationDiagnostic, LoadedProfiles, summarize};
use slingshot_domain::profile::AdobeExperienceManagerDeployment;

use crate::invocation::Selection;
use crate::target_selection::{SelectionRefusal, author_address, select, selected_deployment};

/// What one configuration check found.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CheckReport {
    /// The selection resolves, and these are the values it resolves to.
    Resolved(Box<ResolvedFacts>),
    /// It does not, and these are the only things said about why.
    Refused {
        /// The configuration's own diagnostics, coalesced and bounded.
        diagnostics: Vec<ConfigurationDiagnostic>,
    },
    /// The names themselves were unusable, so nothing was looked up.
    ///
    /// Kept apart from a configuration refusal on purpose. A name the grammar
    /// does not admit is a typing mistake this surface can describe, and
    /// issuing a configuration code for it would put a diagnostic in the closed
    /// vocabulary that Plan 0002 never defined. An incomplete pair is a
    /// different matter: Plan 0002 refuses that itself, with its own code, and
    /// this passes that refusal through rather than restating it.
    NotSelected {
        /// Why nothing was selected.
        refusal: SelectionRefusal,
    },
}

impl CheckReport {
    /// Returns whether the selected configuration is usable.
    #[must_use]
    pub fn is_resolved(&self) -> bool {
        matches!(self, Self::Resolved(_))
    }

    /// Returns the diagnostics, when there are any.
    #[must_use]
    pub fn diagnostics(&self) -> &[ConfigurationDiagnostic] {
        match self {
            Self::Resolved(_) | Self::NotSelected { .. } => &[],
            Self::Refused { diagnostics } => diagnostics,
        }
    }
}

/// What a resolved selection derives.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedFacts {
    /// The nonsecret address the selected environment's author answers on.
    pub author_target: String,
    /// Which product the selected environment runs.
    pub deployment: AdobeExperienceManagerDeployment,
    /// Which environment was selected.
    pub environment: String,
    /// Which profile was selected.
    pub profile: String,
    /// Whether the selected environment carries a cleartext warning.
    pub warned_cleartext_transport: bool,
}

/// Returns what checking `selection` against `loaded` found.
///
/// Reads nothing beyond the profiles it was handed. Everything a check could
/// want to say about a daemon belongs to the leaves that talk to one, so this
/// stays a statement about this machine.
#[must_use]
pub fn check(loaded: &LoadedProfiles, selection: &Selection) -> CheckReport {
    match select(loaded, selection) {
        Ok(resolved) => CheckReport::Resolved(Box::new(ResolvedFacts {
            author_target: author_address(&resolved, loaded),
            deployment: selected_deployment(&resolved, loaded),
            environment: resolved.environment_name().as_text().to_owned(),
            profile: resolved.profile_name().as_text().to_owned(),
            warned_cleartext_transport: resolved.insecure_author_transport_warning().is_some(),
        })),
        Err(SelectionRefusal::Configuration { diagnostics, .. }) => {
            CheckReport::Refused { diagnostics: summarize(diagnostics) }
        }
        Err(refusal) => CheckReport::NotSelected { refusal },
    }
}
