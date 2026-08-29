//! Outbound transport policy for the selected environment.
//!
//! Two routes leave this process: one to Adobe Identity Management Services,
//! carrying the credentials that produce an access token, and one to the
//! selected Adobe Experience Manager author. They must not share a trust
//! decision, because the operator who extends author trust to a corporate
//! authority is not asking to extend it to the place their credentials go.
//!
//! The separation is a type, not a convention. The identity-management client
//! builder accepts only [`IdentityManagementTrustInput`], which can be built
//! only from a platform snapshot; the author client builder accepts only
//! [`AuthorTrustInput`]. Neither converts into the other, so an additional
//! certificate cannot reach the credential-bearing route however it is passed
//! around.
//!
//! Both routes connect directly. Ambient proxy variables are ignored rather
//! than honoured, because a variable an unrelated tool set is not a decision
//! this process should make about where credentials go. And there is no
//! publisher builder at all: a publisher address is metadata, and the way to
//! guarantee nothing dials it is to have nothing that can.

use slingshot_configuration::additional_certificate_authority::AdditionalAuthorCertificates;
use slingshot_configuration::platform_trust::PlatformTrustSnapshot;
use slingshot_domain::profile_authentication_contract::ProfileAuthenticationContract;
use slingshot_domain::selected_environment_revision::{IdentityFailure, TrustPolicyIdentity};

/// The only roots the identity-management client may be built from.
///
/// It carries the platform snapshot alone. There is no constructor that accepts
/// an additional certificate, an author union, or an author identity.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IdentityManagementTrustInput {
    /// Roots the route verifies against.
    roots: Vec<Vec<u8>>,
    /// Identity of those roots under this route's own domain.
    identity: TrustPolicyIdentity,
}

impl IdentityManagementTrustInput {
    /// Returns the identity-management input of one platform snapshot.
    ///
    /// # Errors
    ///
    /// Returns an [`IdentityFailure`] when a length cannot be framed.
    pub fn from_platform(snapshot: &PlatformTrustSnapshot) -> Result<Self, IdentityFailure> {
        Ok(Self {
            roots: snapshot.roots().to_vec(),
            identity: TrustPolicyIdentity::identity_management(snapshot.roots())?,
        })
    }

    /// Returns the roots the route verifies against.
    #[must_use]
    pub fn roots(&self) -> &[Vec<u8>] {
        &self.roots
    }

    /// Returns the identity of those roots.
    #[must_use]
    pub fn identity(&self) -> TrustPolicyIdentity {
        self.identity
    }
}

/// The only roots the author client may be built from.
///
/// It carries the platform snapshot together with whatever the selected
/// environment added, which is what an author trust extension means.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AuthorTrustInput {
    /// Roots the route verifies against.
    roots: Vec<Vec<u8>>,
    /// Identity of those roots under this route's own domain.
    identity: TrustPolicyIdentity,
}

impl AuthorTrustInput {
    /// Returns the author input of one platform snapshot and one extension.
    ///
    /// # Errors
    ///
    /// Returns an [`IdentityFailure`] when a length cannot be framed.
    pub fn from_platform_and_extension(
        snapshot: &PlatformTrustSnapshot,
        extension: Option<&AdditionalAuthorCertificates>,
    ) -> Result<Self, IdentityFailure> {
        let additional: Vec<Vec<u8>> =
            extension.map(|extension| extension.certificates().to_vec()).unwrap_or_default();
        let mut roots = snapshot.roots().to_vec();
        roots.extend(additional.iter().cloned());
        roots.sort();
        roots.dedup();
        Ok(Self { roots, identity: TrustPolicyIdentity::author(snapshot.roots(), &additional)? })
    }

    /// Returns the roots the route verifies against.
    #[must_use]
    pub fn roots(&self) -> &[Vec<u8>] {
        &self.roots
    }

    /// Returns the identity of those roots.
    #[must_use]
    pub fn identity(&self) -> TrustPolicyIdentity {
        self.identity
    }
}

/// The one outbound connection policy both routes install.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DirectTransportPolicy;

impl DirectTransportPolicy {
    /// Returns the manifest literal this policy is written as.
    #[must_use]
    pub fn policy() -> &'static str {
        &ProfileAuthenticationContract::embedded().literals.proxy_policy
    }

    /// Returns the environment variables neither route consults.
    ///
    /// They are listed rather than merely unused, so a client that started
    /// honouring one would disagree with a value this policy publishes.
    #[must_use]
    pub fn ignored_proxy_variables() -> &'static [String] {
        &ProfileAuthenticationContract::embedded().literals.ignored_proxy_variables
    }

    /// Returns the transport-layer-security versions both routes negotiate.
    #[must_use]
    pub fn transport_layer_security_versions() -> &'static [String] {
        &ProfileAuthenticationContract::embedded().literals.transport_layer_security_versions
    }

    /// Returns the redirect handling both routes install.
    #[must_use]
    pub fn redirect_policy() -> &'static str {
        &ProfileAuthenticationContract::embedded().literals.redirect_policy
    }
}
