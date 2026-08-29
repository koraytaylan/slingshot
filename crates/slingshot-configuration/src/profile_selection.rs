//! Explicit and default selection of one profile environment.
//!
//! Selection is never first-found. A caller either names both a profile and an
//! environment, or the configuration root names both in its selection document;
//! anything in between is refused. Choosing for the caller would mean a command
//! could be aimed at a different server by adding a file, and the operator who
//! ran it would have no way to tell.
//!
//! A failure says which class of source failed, at which stage, at which
//! structural location, and why - not which name was asked for, which names
//! were available, or how many there were. A missing profile and a missing
//! environment must look the same whatever they were called, because otherwise
//! the refusal itself enumerates the root.

use slingshot_domain::configuration_snapshot::ConfigurationReference;
use slingshot_domain::profile::{
    Environment, EnvironmentAuthentication, EnvironmentName, InsecureAuthorTransportWarning,
    ProfileName,
};
use slingshot_domain::profile_authentication_contract::ConfigurationFailureCode;
use slingshot_domain::profile_authentication_contract::ProfileAuthenticationContract;
use slingshot_domain::selected_environment_revision::{
    AuthenticationPrincipalIdentity, AuthorTargetIdentityDigest, CanonicalMetascopeSet,
    IdentityFailure, RevisionFields, SelectedEnvironmentRevision, TrustPolicyIdentity,
};

use crate::profile_loader::{
    ConfigurationDiagnostic, DiagnosticSourceClass, DiagnosticStage, LoadedProfiles,
};

/// Separator between the two names of a namespace key.
///
/// Neither name may contain it, because both match a grammar of lowercase
/// letters, digits, and hyphens, so one key has one reading.
const NAMESPACE_SEPARATOR: char = '/';

/// The names a caller supplied on its own invocation.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct RequestedSelection {
    /// Profile the caller named, when it named one.
    pub profile: Option<ProfileName>,
    /// Environment the caller named, when it named one.
    pub environment: Option<EnvironmentName>,
}

/// One resolved profile environment.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProfileSelection {
    /// Name of the selected profile.
    profile_name: ProfileName,
    /// Name of the selected environment.
    environment_name: EnvironmentName,
    /// Reference the selected profile was read from.
    profile_source: ConfigurationReference,
    /// Reference the selection document was read from, when there was one.
    selection_source: Option<ConfigurationReference>,
    /// Warning the selected environment carries, if any.
    warning: Option<InsecureAuthorTransportWarning>,
}

impl ProfileSelection {
    /// Returns the name of the selected profile.
    #[must_use]
    pub fn profile_name(&self) -> &ProfileName {
        &self.profile_name
    }

    /// Returns the name of the selected environment.
    #[must_use]
    pub fn environment_name(&self) -> &EnvironmentName {
        &self.environment_name
    }

    /// Returns the reference the selected profile was read from.
    ///
    /// The provenance is internal: the identity builder needs it, and nothing
    /// a caller sees carries it.
    #[must_use]
    pub fn profile_source(&self) -> &ConfigurationReference {
        &self.profile_source
    }

    /// Returns the reference the selection document was read from.
    #[must_use]
    pub fn selection_source(&self) -> Option<&ConfigurationReference> {
        self.selection_source.as_ref()
    }

    /// Returns the warning the selected environment carries, if any.
    #[must_use]
    pub fn insecure_author_transport_warning(&self) -> Option<InsecureAuthorTransportWarning> {
        self.warning
    }

    /// Returns the key one daemon namespace is derived from.
    ///
    /// It is built from the two names alone, so the same pair always names the
    /// same namespace and no source reference, order, or digest reaches it.
    #[must_use]
    pub fn namespace_key(&self) -> String {
        format!("{}{NAMESPACE_SEPARATOR}{}", self.profile_name, self.environment_name)
    }

    /// Returns the selected environment of `loaded`.
    ///
    /// # Panics
    ///
    /// Panics when `loaded` is not the collection this selection was resolved
    /// against, which no caller can arrange: the selection is only ever built
    /// by [`resolve`] from that same collection.
    #[must_use]
    pub fn environment_of<'profiles>(
        &self,
        loaded: &'profiles LoadedProfiles,
    ) -> &'profiles Environment {
        loaded
            .profiles()
            .get(&self.profile_name)
            .and_then(|profile| profile.environments().get(&self.environment_name))
            .expect("the selection was resolved against this collection")
    }
}

impl ProfileSelection {
    /// Returns the target identity and revision of this selection.
    ///
    /// Everything that goes in is nonsecret and normalized, and everything that
    /// would move on its own - a source digest, a timestamp, a file identity, a
    /// permission, a credential byte - stays out. That is what makes a password
    /// or key rotation at the same principal and address leave both values
    /// exactly as they were.
    ///
    /// # Errors
    ///
    /// Returns an [`IdentityFailure`] when a required field is empty or a
    /// length cannot be framed.
    pub fn revision(
        &self,
        loaded: &LoadedProfiles,
        principal: AuthenticationPrincipalIdentity,
        canonical_metascope_set: CanonicalMetascopeSet,
        identity_management_trust_policy_identity: TrustPolicyIdentity,
        author_trust_policy_identity: TrustPolicyIdentity,
    ) -> Result<(AuthorTargetIdentityDigest, SelectedEnvironmentRevision), IdentityFailure> {
        let environment = self.environment_of(loaded);
        let deployment = environment.deployment().as_text();
        let author = environment.author_connection_target();
        let target = AuthorTargetIdentityDigest::build(deployment, author.as_text(), principal)?;
        let credential = match environment.authentication() {
            EnvironmentAuthentication::DeveloperConsoleServiceCredentialsFile {
                credentials_file,
            } => Some(credentials_file.as_text().to_owned()),
            EnvironmentAuthentication::BasicCredentials { .. } => None,
        };
        let fields = RevisionFields {
            profile_name: self.profile_name.as_text().to_owned(),
            environment_name: self.environment_name.as_text().to_owned(),
            profile_source_reference: self.profile_source.as_text().to_owned(),
            selection_source_reference: self
                .selection_source
                .as_ref()
                .map(|source| source.as_text().to_owned()),
            author_target_identity: target,
            publisher_base_address: environment.publisher_metadata().as_text().to_owned(),
            authentication_method: environment.authentication().method().to_owned(),
            credential_source_reference: credential,
            certificate_source_reference: environment
                .additional_certificate_authority_file()
                .map(|source| source.as_text().to_owned()),
            proxy_policy: ProfileAuthenticationContract::embedded().literals.proxy_policy.clone(),
            allow_insecure_author_transport: self.warning.is_some(),
            canonical_metascope_set,
            identity_management_trust_policy_identity,
            author_trust_policy_identity,
        };
        Ok((target, SelectedEnvironmentRevision::build(&fields)?))
    }
}

/// Resolves one profile environment from an explicit or a default pair.
///
/// # Errors
///
/// Returns [`ConfigurationFailureCode::SelectionIncomplete`] when exactly one
/// name is available, [`ConfigurationFailureCode::ProfileNotFound`] when no
/// loaded profile declares the selected name, and
/// [`ConfigurationFailureCode::EnvironmentNotFound`] when that profile defines
/// no environment of the selected name. The failure names neither.
pub fn resolve(
    loaded: &LoadedProfiles,
    requested: &RequestedSelection,
) -> Result<ProfileSelection, Vec<ConfigurationDiagnostic>> {
    let (profile_name, environment_name) = chosen_names(loaded, requested)?;
    let profile = loaded
        .profiles()
        .get(&profile_name)
        .ok_or_else(|| refusal("profile", ConfigurationFailureCode::ProfileNotFound))?;
    let environment = profile
        .environments()
        .get(&environment_name)
        .ok_or_else(|| refusal("environment", ConfigurationFailureCode::EnvironmentNotFound))?;
    let profile_source = loaded
        .source_of(&profile_name)
        .ok_or_else(|| refusal("profile", ConfigurationFailureCode::ProfileNotFound))?;
    Ok(ProfileSelection {
        profile_name,
        environment_name,
        profile_source: profile_source.clone(),
        selection_source: loaded.selection_source().cloned(),
        warning: environment.insecure_author_transport_warning(),
    })
}

/// Returns the pair a caller or the configuration root named.
///
/// An explicit pair wins over a default one, but only when it is a pair: one
/// name on its own is a request that cannot be completed, whatever the root
/// happens to say, because completing it from a different source is exactly the
/// silent choice this refuses to make.
fn chosen_names(
    loaded: &LoadedProfiles,
    requested: &RequestedSelection,
) -> Result<(ProfileName, EnvironmentName), Vec<ConfigurationDiagnostic>> {
    match (&requested.profile, &requested.environment) {
        (Some(profile), Some(environment)) => Ok((profile.clone(), environment.clone())),
        (None, None) => loaded
            .selection()
            .map(|document| (document.profile().clone(), document.environment().clone()))
            .ok_or_else(|| refusal("selection", ConfigurationFailureCode::SelectionIncomplete)),
        _ => Err(refusal("selection", ConfigurationFailureCode::SelectionIncomplete)),
    }
}

/// Returns the one diagnostic a selection failure reports.
fn refusal(
    structural_location: &'static str,
    code: ConfigurationFailureCode,
) -> Vec<ConfigurationDiagnostic> {
    vec![ConfigurationDiagnostic::once(
        DiagnosticSourceClass::Selection,
        DiagnosticStage::Selection,
        structural_location,
        code,
    )]
}
