//! The one authentication provider an author request may use.
//!
//! The snapshot behind this provider is assembled once, at startup, from a
//! generation that has already been proved whole. Nothing here reloads: editing
//! a profile, rotating a credential, or changing the platform trust store
//! affects a running daemon not at all, and takes effect on an explicit
//! restart, which takes a new snapshot and produces a revision that says so.
//! A provider that reloaded would let a request halfway through a command use
//! different credentials than the one before it.
//!
//! The provider answers for one target: the selected author base address, and
//! endpoints below it. A publisher address, an unrelated origin, and an address
//! that merely starts with the same characters are all refused before any
//! client sees them, because the whole point of holding credentials for one
//! server is not sending them to another.
//!
//! There is no publisher method here, and no way to construct one. Slingshot
//! never dials a publisher, and the way to guarantee that is to have nothing
//! that can.

use slingshot_configuration::profile_selection::ProfileSelection;
use slingshot_domain::profile::{
    AdobeExperienceManagerDeployment, BasicUserName, InsecureAuthorTransportWarning,
    TierBaseAddress,
};
use slingshot_domain::profile_authentication_contract::ConfigurationFailureCode;
use slingshot_domain::secret_value::SecretValue;
use slingshot_domain::selected_environment_revision::{
    AuthenticationPrincipalIdentity, AuthorTargetIdentityDigest, SelectedEnvironmentRevision,
};

use crate::authentication::access_token_cache::{
    AccessTokenLease, AccessTokenSource, CloudAccessTokenCache,
};
use crate::authentication::cloud_service_credentials::CloudServiceCredentials;
use crate::authentication::identity_management_exchange::ExchangeFailure;
use crate::transport_policy::{AuthorTrustInput, IdentityManagementTrustInput};

/// Scheme every authorization value this provider builds opens with.
const BASIC_SCHEME: &str = "Basic ";

/// Scheme every bearer authorization value opens with.
const BEARER_SCHEME: &str = "Bearer ";

/// Separator between an address and an endpoint below it.
const PATH_SEPARATOR: char = '/';

/// Reason a request could not be authenticated.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[error("{code}")]
pub struct ProviderFailure {
    /// Stable code from the contract registry.
    pub code: ConfigurationFailureCode,
}

impl ProviderFailure {
    /// Returns one failure carrying `code`.
    #[must_use]
    pub fn new(code: ConfigurationFailureCode) -> Self {
        Self { code }
    }
}

impl From<ExchangeFailure> for ProviderFailure {
    fn from(failure: ExchangeFailure) -> Self {
        Self { code: failure.code }
    }
}

/// How one snapshot authenticates to its author.
#[derive(Debug)]
pub enum SnapshotAuthentication {
    /// Adobe Experience Manager 6.5 Basic credentials.
    BasicCredentials {
        /// Validated user name.
        user_name: BasicUserName,
        /// Password bytes, exactly as the profile spelled them.
        password: SecretValue,
    },
    /// Developer Console service credentials, exchanged for a token.
    ServiceCredentials {
        /// Parsed credentials the exchange is built from.
        credentials: Box<CloudServiceCredentials>,
    },
}

/// The immutable material one startup accepted.
#[derive(Debug)]
pub struct SelectedEnvironmentSnapshot {
    /// The only address this snapshot authenticates to.
    author: TierBaseAddress,
    /// The publisher address, retained as metadata.
    publisher: TierBaseAddress,
    /// Product the environment runs.
    deployment: AdobeExperienceManagerDeployment,
    /// How the snapshot authenticates.
    authentication: SnapshotAuthentication,
    /// Warning a cleartext author address carries.
    warning: Option<InsecureAuthorTransportWarning>,
    /// Opaque identity of the principal.
    principal: AuthenticationPrincipalIdentity,
    /// Durable identity of the author target.
    target: AuthorTargetIdentityDigest,
    /// Revision of this selection.
    revision: SelectedEnvironmentRevision,
    /// Roots the identity-management route may use.
    identity_management_trust: IdentityManagementTrustInput,
    /// Roots the author route may use.
    author_trust: AuthorTrustInput,
}

/// Everything one snapshot is assembled from.
///
/// The values are supplied rather than read here, because assembling a snapshot
/// is the moment a startup stops looking at the filesystem: nothing below
/// reopens a source, and the provider above cannot.
#[derive(Debug)]
pub struct SnapshotMaterial {
    /// The only address the snapshot authenticates to.
    pub author: TierBaseAddress,
    /// The publisher address, retained as metadata.
    pub publisher: TierBaseAddress,
    /// Product the environment runs.
    pub deployment: AdobeExperienceManagerDeployment,
    /// How the snapshot authenticates.
    pub authentication: SnapshotAuthentication,
    /// Opaque identity of the principal.
    pub principal: AuthenticationPrincipalIdentity,
    /// Durable identity of the author target.
    pub target: AuthorTargetIdentityDigest,
    /// Revision of this selection.
    pub revision: SelectedEnvironmentRevision,
    /// Roots the identity-management route may use.
    pub identity_management_trust: IdentityManagementTrustInput,
    /// Roots the author route may use.
    pub author_trust: AuthorTrustInput,
}

impl SelectedEnvironmentSnapshot {
    /// Returns the snapshot one selection produced.
    #[must_use]
    pub fn assemble(selection: &ProfileSelection, material: SnapshotMaterial) -> Self {
        Self {
            author: material.author,
            publisher: material.publisher,
            deployment: material.deployment,
            authentication: material.authentication,
            warning: selection.insecure_author_transport_warning(),
            principal: material.principal,
            target: material.target,
            revision: material.revision,
            identity_management_trust: material.identity_management_trust,
            author_trust: material.author_trust,
        }
    }

    /// Returns the only address this snapshot authenticates to.
    #[must_use]
    pub fn author(&self) -> &TierBaseAddress {
        &self.author
    }

    /// Returns the publisher address, as metadata.
    #[must_use]
    pub fn publisher_metadata(&self) -> &TierBaseAddress {
        &self.publisher
    }

    /// Returns the product the environment runs.
    #[must_use]
    pub fn deployment(&self) -> AdobeExperienceManagerDeployment {
        self.deployment
    }

    /// Returns the opaque identity of the principal.
    #[must_use]
    pub fn principal(&self) -> AuthenticationPrincipalIdentity {
        self.principal
    }

    /// Returns the durable identity of the author target.
    #[must_use]
    pub fn target(&self) -> AuthorTargetIdentityDigest {
        self.target
    }

    /// Returns the revision of this selection.
    #[must_use]
    pub fn revision(&self) -> SelectedEnvironmentRevision {
        self.revision
    }

    /// Returns the roots only the identity-management route may use.
    #[must_use]
    pub fn identity_management_trust(&self) -> &IdentityManagementTrustInput {
        &self.identity_management_trust
    }

    /// Returns the roots only the author route may use.
    #[must_use]
    pub fn author_trust(&self) -> &AuthorTrustInput {
        &self.author_trust
    }

    /// Returns the warning a cleartext author address carries.
    #[must_use]
    pub fn insecure_author_transport_warning(&self) -> Option<InsecureAuthorTransportWarning> {
        self.warning
    }
}

/// The material one request carries to authenticate itself.
///
/// It renders redacted, because the value it holds is the credential in the
/// form a server accepts it.
pub struct RequestAuthentication {
    /// The complete authorization value.
    value: SecretValue,
}

impl RequestAuthentication {
    /// Lends the complete authorization value to `use_bytes`.
    pub fn lend_value_bytes<Outcome>(&self, use_bytes: impl FnOnce(&[u8]) -> Outcome) -> Outcome {
        use_bytes(self.value.expose_secret_bytes())
    }
}

impl ::core::fmt::Debug for RequestAuthentication {
    fn fmt(&self, formatter: &mut ::core::fmt::Formatter<'_>) -> ::core::fmt::Result {
        formatter.write_str("RequestAuthentication([redacted])")
    }
}

/// Authenticates requests to one author, and to nothing else.
#[derive(Debug)]
pub struct EnvironmentAuthenticationProvider {
    /// Material this provider was built from, which it never reloads.
    snapshot: SelectedEnvironmentSnapshot,
    /// Cache the cloud variant leases its token from.
    cache: CloudAccessTokenCache,
}

impl EnvironmentAuthenticationProvider {
    /// Returns a provider over `snapshot`, leasing through a cache identified
    /// by `cache_identity`.
    #[must_use]
    pub fn new(snapshot: SelectedEnvironmentSnapshot, cache_identity: u64) -> Self {
        Self { snapshot, cache: CloudAccessTokenCache::with_identity(cache_identity) }
    }

    /// Returns the snapshot this provider was built from.
    #[must_use]
    pub fn snapshot(&self) -> &SelectedEnvironmentSnapshot {
        &self.snapshot
    }

    /// Returns the endpoint `segments` names below the author address.
    #[must_use]
    pub fn author_endpoint(&self, segments: &[&str]) -> String {
        self.snapshot.author.endpoint(segments)
    }

    /// Returns the material a request to `endpoint` carries.
    ///
    /// # Errors
    ///
    /// Returns [`ConfigurationFailureCode::AuthenticationTargetMismatch`] when
    /// `endpoint` is not the author address or an endpoint below it,
    /// [`ConfigurationFailureCode::InsecureAuthorTransportNotAllowed`] when the
    /// address is cleartext off loopback without the typed permission, and
    /// whatever an exchange refused with for the cloud variant.
    pub fn authenticate(
        &self,
        endpoint: &str,
        reading: u64,
        source: &dyn AccessTokenSource,
    ) -> Result<(RequestAuthentication, Option<AccessTokenLease>), ProviderFailure> {
        self.require_author_target(endpoint)?;
        self.require_permitted_transport()?;
        match &self.snapshot.authentication {
            SnapshotAuthentication::BasicCredentials { user_name, password } => {
                Ok((basic_authentication(user_name, password), None))
            }
            SnapshotAuthentication::ServiceCredentials { .. } => {
                let (value, lease) = self.cache.token(reading, source, bearer_authentication)?;
                Ok((value, Some(lease)))
            }
        }
    }

    /// Replaces the generation `lease` names after a rejected request.
    ///
    /// # Errors
    ///
    /// Returns [`ConfigurationFailureCode::AuthenticationTargetMismatch`] when
    /// the snapshot authenticates with Basic credentials, which have no token
    /// to replace, and whatever the exchange refused with.
    pub fn refresh_after_unauthorized(
        &self,
        lease: AccessTokenLease,
        source: &dyn AccessTokenSource,
    ) -> Result<(RequestAuthentication, AccessTokenLease), ProviderFailure> {
        let SnapshotAuthentication::ServiceCredentials { .. } = &self.snapshot.authentication
        else {
            return Err(ProviderFailure::new(
                ConfigurationFailureCode::AuthenticationTargetMismatch,
            ));
        };
        Ok(self.cache.refresh_after_unauthorized(lease, source, bearer_authentication)?)
    }

    /// Requires `endpoint` to be the author address or an endpoint below it.
    ///
    /// The comparison is on the complete address followed by a separator or
    /// nothing, so an unrelated origin that merely begins with the same
    /// characters is refused rather than accepted as a prefix match.
    fn require_author_target(&self, endpoint: &str) -> Result<(), ProviderFailure> {
        let author = self.snapshot.author.as_text();
        let matched = endpoint == author
            || endpoint.strip_prefix(author).is_some_and(|below| below.starts_with(PATH_SEPARATOR));
        if matched {
            return Ok(());
        }
        Err(ProviderFailure::new(ConfigurationFailureCode::AuthenticationTargetMismatch))
    }

    /// Requires a cleartext author address to carry its typed permission.
    fn require_permitted_transport(&self) -> Result<(), ProviderFailure> {
        let author = &self.snapshot.author;
        if author.is_protected() || author.is_loopback() || self.snapshot.warning.is_some() {
            return Ok(());
        }
        Err(ProviderFailure::new(ConfigurationFailureCode::InsecureAuthorTransportNotAllowed))
    }
}

/// Builds the exact Basic authorization value.
///
/// The canonical input is assembled and encoded inside one lend, so the
/// assembled bytes are scrubbed when this returns and only the encoded value
/// outlives it.
fn basic_authentication(
    user_name: &BasicUserName,
    password: &SecretValue,
) -> RequestAuthentication {
    use base64::Engine;
    use base64::engine::general_purpose::STANDARD;

    let mut canonical =
        Vec::with_capacity(user_name.as_text().len() + password.secret_byte_length() + 1);
    canonical.extend_from_slice(user_name.as_text().as_bytes());
    canonical.push(b':');
    canonical.extend_from_slice(password.expose_secret_bytes());
    let canonical = SecretValue::from_bytes(canonical);
    let encoded = STANDARD.encode(canonical.expose_secret_bytes());
    RequestAuthentication { value: SecretValue::from_text(format!("{BASIC_SCHEME}{encoded}")) }
}

/// Builds the exact bearer authorization value.
fn bearer_authentication(
    token: &crate::authentication::identity_management_exchange::AccessToken,
) -> RequestAuthentication {
    let value = token.lend_token_bytes(|bytes| {
        let mut value = BEARER_SCHEME.as_bytes().to_vec();
        value.extend_from_slice(bytes);
        value
    });
    RequestAuthentication { value: SecretValue::from_bytes(value) }
}
