//! Builds one selected runtime snapshot from one already-verified generation.

use super::{
    cloud_service_credentials::CloudServiceCredentials,
    environment_provider::{SelectedEnvironmentSnapshot, SnapshotAuthentication, SnapshotMaterial},
};
use crate::transport_policy::{AuthorTrustInput, IdentityManagementTrustInput};
use slingshot_configuration::{
    additional_certificate_authority::AdditionalAuthorCertificates,
    configuration_generation::SourceRole,
    platform_trust::{PlatformTrustSnapshot, PlatformTrustSource},
    profile_loader::LoadedProfiles,
    profile_selection::{RequestedSelection, resolve},
};
use slingshot_domain::{
    profile::EnvironmentAuthentication,
    secret_value::SecretValue,
    selected_environment_revision::{AuthenticationPrincipalIdentity, CanonicalMetascopeSet},
};

/// A selected snapshot could not be built; source content never reaches errors.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum RuntimeSnapshotRefusal {
    /// The requested selection could not resolve against this generation.
    #[error("the runtime profile selection could not be resolved")]
    Selection,
    /// Retained credential/certificate material could not be parsed.
    #[error("the runtime configuration material could not be established")]
    Material,
    /// The frozen platform/author trust policy could not be established.
    #[error("the runtime trust snapshot could not be established")]
    Trust,
    /// The selected target and revision could not be derived.
    #[error("the runtime selection identity could not be established")]
    Identity,
}

/// Consumes the loaded generation, parses only selected credential/certificate
/// sources, and snapshots platform trust once. No credential-file reopen, network
/// exchange, endpoint override or caller-supplied identity crosses this factory.
pub fn build_runtime_snapshot(
    loaded: LoadedProfiles,
    requested: &RequestedSelection,
    platform: &dyn PlatformTrustSource,
) -> Result<SelectedEnvironmentSnapshot, RuntimeSnapshotRefusal> {
    let selection = resolve(&loaded, requested).map_err(|_| RuntimeSnapshotRefusal::Selection)?;
    let chosen = selection.environment_of(&loaded);
    let (authentication, principal, metascopes) = match chosen.authentication() {
        EnvironmentAuthentication::BasicCredentials { user_name, password } => (
            SnapshotAuthentication::BasicCredentials {
                user_name: user_name.clone(),
                password: SecretValue::from_bytes(password.expose_secret_bytes().to_vec()),
            },
            AuthenticationPrincipalIdentity::basic("basic", user_name.as_text())
                .map_err(|_| RuntimeSnapshotRefusal::Identity)?,
            CanonicalMetascopeSet::empty(),
        ),
        EnvironmentAuthentication::DeveloperConsoleServiceCredentialsFile { credentials_file } => {
            let document = loaded
                .retained_source(credentials_file, SourceRole::ServiceCredentials)
                .ok_or(RuntimeSnapshotRefusal::Material)?;
            let credentials = CloudServiceCredentials::parse(document)
                .map_err(|_| RuntimeSnapshotRefusal::Material)?;
            let principal = credentials.principal();
            let metascopes = CanonicalMetascopeSet::from_values(
                &credentials
                    .metascopes()
                    .values()
                    .map_err(|_| RuntimeSnapshotRefusal::Material)?
                    .iter()
                    .map(|value| (*value).to_owned())
                    .collect::<Vec<_>>(),
            );
            (
                SnapshotAuthentication::ServiceCredentials { credentials: Box::new(credentials) },
                principal,
                metascopes,
            )
        }
    };
    let extension = chosen
        .additional_certificate_authority_file()
        .map(|reference| {
            loaded
                .retained_source(reference, SourceRole::AdditionalCertificateAuthority)
                .ok_or(RuntimeSnapshotRefusal::Material)?
                .lend_bytes_for_inspection(AdditionalAuthorCertificates::parse)
                .map_err(|_| RuntimeSnapshotRefusal::Material)
        })
        .transpose()?;
    let platform =
        PlatformTrustSnapshot::take(platform).map_err(|_| RuntimeSnapshotRefusal::Trust)?;
    let identity_management_trust = IdentityManagementTrustInput::from_platform(&platform)
        .map_err(|_| RuntimeSnapshotRefusal::Trust)?;
    let author_trust = AuthorTrustInput::from_platform_and_extension(&platform, extension.as_ref())
        .map_err(|_| RuntimeSnapshotRefusal::Trust)?;
    let (target, revision) = selection
        .revision(
            &loaded,
            principal,
            metascopes,
            identity_management_trust.identity(),
            author_trust.identity(),
        )
        .map_err(|_| RuntimeSnapshotRefusal::Identity)?;
    Ok(SelectedEnvironmentSnapshot::assemble(
        &selection,
        SnapshotMaterial {
            author: chosen.author_connection_target().clone(),
            publisher: chosen.publisher_metadata().clone(),
            deployment: chosen.deployment(),
            authentication,
            principal,
            target,
            revision,
            identity_management_trust,
            author_trust,
        },
    ))
}
