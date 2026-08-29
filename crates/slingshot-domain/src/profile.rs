//! Profile and environment value objects.
//!
//! A profile document names one site and its environments. Each environment
//! fixes a deployment, an author and a publisher address, and one
//! authentication method; the only legal combinations are Basic with Adobe
//! Experience Manager 6.5 and Developer Console service credentials with Cloud
//! Service. Every other pairing is refused here, not at the first request.
//!
//! Two properties exist for safety. A base address has exactly one spelling,
//! so it cannot be written two ways and produce two target identities, and
//! appending an endpoint only extends the context path. And the publisher
//! address is data: nothing turns it into a connection target.

use std::collections::BTreeMap;

use serde::Deserialize;

use crate::configuration_snapshot::ConfigurationReference;
use crate::profile_authentication_contract::{
    ConfigurationFailureCode, ProfileAuthenticationContract, narrow_limit,
};
use crate::secret_value::SecretValue;

use AdobeExperienceManagerDeployment as Deployment;

/// Separator between a scheme and the authority that follows it.
const SCHEME_SEPARATOR: &str = "://";

/// Separator between path segments.
const PATH_SEPARATOR: char = '/';

/// Separator between a host and an explicit port.
const PORT_SEPARATOR: char = ':';

/// Separator between the user name and the password of a Basic credential.
const BASIC_SEPARATOR: u8 = b':';

/// Bytes that stay literal in a canonical path segment.
const UNRESERVED_PUNCTUATION: &str = "-._~";

/// Bytes that are legal in a path segment without being unreserved.
const SEGMENT_PUNCTUATION: &str = "!$&'()*+,;=:@";

/// Radix a percent escape is written in.
const ESCAPE_RADIX: u32 = 16;

/// Characters a percent escape occupies after the percent sign.
const ESCAPE_DIGITS: usize = 2;

/// Largest port number a base address may name.
const MAXIMUM_PORT: u32 = 65_535;

/// Opening byte of a bracketed internet-protocol version six host.
const BRACKET_OPEN: char = '[';

/// Closing byte of a bracketed internet-protocol version six host.
const BRACKET_CLOSE: char = ']';

/// Reason a profile or selection document was refused.
///
/// It carries the contract's stable code and a structural location from the
/// manifest's vocabulary, never a name, address, reference, value, or parser
/// excerpt, because any of those can contain a secret.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("{code} at {structural_location}")]
pub struct ProfileDocumentFailure {
    /// Stable code from the contract registry.
    pub code: ConfigurationFailureCode,
    /// Manifest member vocabulary naming where the failure was found.
    pub structural_location: &'static str,
}

impl ProfileDocumentFailure {
    /// Returns one failure at a named structural location.
    #[must_use]
    pub fn at(code: ConfigurationFailureCode, structural_location: &'static str) -> Self {
        Self { code, structural_location }
    }

    /// Returns a value failure at a named structural location.
    fn value(structural_location: &'static str) -> Self {
        Self::at(ConfigurationFailureCode::ConfigurationValueInvalid, structural_location)
    }
}

/// The name one profile document declares.
///
/// The name is independent of the file it was read from, so renaming the file
/// does not rename the profile.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ProfileName {
    name: String,
}

impl ProfileName {
    /// Validates one profile name against the contract grammar and bound.
    ///
    /// # Errors
    ///
    /// Returns [`ConfigurationFailureCode::ConfigurationValueInvalid`] for an
    /// empty, overlong, or non-kebab name.
    pub fn parse(name: &str) -> Result<Self, ProfileDocumentFailure> {
        let limits = &ProfileAuthenticationContract::embedded().limits;
        if !is_kebab_identifier(name, narrow_limit(limits.maximum_profile_name_bytes)) {
            return Err(ProfileDocumentFailure::value("name"));
        }
        Ok(Self { name: name.to_owned() })
    }

    /// Returns the name exactly as it was written.
    #[must_use]
    pub fn as_text(&self) -> &str {
        &self.name
    }
}

impl ::core::fmt::Display for ProfileName {
    fn fmt(&self, formatter: &mut ::core::fmt::Formatter<'_>) -> ::core::fmt::Result {
        formatter.write_str(&self.name)
    }
}

/// The name one environment carries inside its profile.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct EnvironmentName {
    name: String,
}

impl EnvironmentName {
    /// Validates one environment name against the contract grammar and bound.
    ///
    /// # Errors
    ///
    /// Returns [`ConfigurationFailureCode::ConfigurationValueInvalid`] for an
    /// empty, overlong, or non-kebab name.
    pub fn parse(name: &str) -> Result<Self, ProfileDocumentFailure> {
        let limits = &ProfileAuthenticationContract::embedded().limits;
        if !is_kebab_identifier(name, narrow_limit(limits.maximum_environment_name_bytes)) {
            return Err(ProfileDocumentFailure::value("environments"));
        }
        Ok(Self { name: name.to_owned() })
    }

    /// Returns the name exactly as it was written.
    #[must_use]
    pub fn as_text(&self) -> &str {
        &self.name
    }
}

impl ::core::fmt::Display for EnvironmentName {
    fn fmt(&self, formatter: &mut ::core::fmt::Formatter<'_>) -> ::core::fmt::Result {
        formatter.write_str(&self.name)
    }
}

/// The product an environment runs.
///
/// It decides which authentication method is legal and whether a cleartext
/// address can be accepted, so it is closed rather than a string.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum AdobeExperienceManagerDeployment {
    /// An Adobe Experience Manager 6.5 installation.
    AdobeExperienceManager65,
    /// An Adobe Experience Manager Cloud Service environment.
    AdobeExperienceManagerCloudService,
}

impl AdobeExperienceManagerDeployment {
    /// Parses one deployment from its manifest literal.
    ///
    /// # Errors
    ///
    /// Returns [`ConfigurationFailureCode::ConfigurationValueInvalid`] for any
    /// value the manifest does not list.
    pub fn parse(value: &str) -> Result<Self, ProfileDocumentFailure> {
        let deployments = &ProfileAuthenticationContract::embedded().literals.deployments;
        match deployments.iter().position(|known| known == value) {
            Some(0) => Ok(Self::AdobeExperienceManager65),
            Some(_) => Ok(Self::AdobeExperienceManagerCloudService),
            None => Err(ProfileDocumentFailure::value("deployment")),
        }
    }

    /// Returns the manifest literal this deployment is written as.
    #[must_use]
    pub fn as_text(self) -> &'static str {
        let deployments = &ProfileAuthenticationContract::embedded().literals.deployments;
        let position = match self {
            Self::AdobeExperienceManager65 => 0,
            Self::AdobeExperienceManagerCloudService => 1,
        };
        deployments[position].as_str()
    }
}

/// A normalized author or publisher base address.
///
/// The address is an origin plus the root or one absolute context-path prefix,
/// with exactly one spelling: lowercase scheme and host, a port only when it
/// differs from the scheme's default, uppercase escapes that never spell an
/// unreserved byte, and a prefix with one leading and no trailing separator -
/// empty for the root. Anything ambiguous is refused rather than normalized
/// away: normalizing is how one address becomes two.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct TierBaseAddress {
    scheme: String,
    host: String,
    port: Option<u16>,
    context_path: String,
    rendered: String,
}

impl TierBaseAddress {
    /// Parses and canonicalizes one base address.
    ///
    /// # Errors
    ///
    /// Returns [`ConfigurationFailureCode::ConfigurationValueInvalid`] for a
    /// bound, scheme, user-information, query, fragment, host, port, or
    /// context-path segment the contract does not accept.
    pub fn parse(address: &str) -> Result<Self, ProfileDocumentFailure> {
        let contract = ProfileAuthenticationContract::embedded();
        let limits = &contract.limits;
        if address.is_empty()
            || address.len() > narrow_limit(limits.maximum_tier_base_address_bytes)
        {
            return Err(ProfileDocumentFailure::value("base_address"));
        }
        if address.contains(['?', '#']) {
            return Err(ProfileDocumentFailure::value("base_address"));
        }
        let (scheme, remainder) = address
            .split_once(SCHEME_SEPARATOR)
            .ok_or_else(|| ProfileDocumentFailure::value("base_address"))?;
        let scheme = scheme.to_ascii_lowercase();
        if !contract.literals.schemes.contains(&scheme) {
            return Err(ProfileDocumentFailure::value("base_address"));
        }
        let (authority, path) = match remainder.find(PATH_SEPARATOR) {
            Some(position) => remainder.split_at(position),
            None => (remainder, ""),
        };
        if authority.contains('@') {
            return Err(ProfileDocumentFailure::value("base_address"));
        }
        let (host, port) = split_authority(authority, &scheme, contract)?;
        let context_path = canonical_context_path(path, limits)?;
        let rendered = render_address(&scheme, &host, port, &context_path);
        if rendered.len() > narrow_limit(limits.maximum_tier_base_address_bytes) {
            return Err(ProfileDocumentFailure::value("base_address"));
        }
        Ok(Self { scheme, host, port, context_path, rendered })
    }

    /// Returns the complete canonical rendering.
    #[must_use]
    pub fn as_text(&self) -> &str {
        &self.rendered
    }

    /// Returns the lowercase host, bracketed for a version six literal.
    #[must_use]
    pub fn host(&self) -> &str {
        &self.host
    }

    /// Returns the port, when it differs from the scheme's default.
    #[must_use]
    pub fn port(&self) -> Option<u16> {
        self.port
    }

    /// Reports whether the transport protects the bytes it carries.
    #[must_use]
    pub fn is_protected(&self) -> bool {
        let schemes = &ProfileAuthenticationContract::embedded().literals.schemes;
        schemes.get(1).is_some_and(|protected| protected == &self.scheme)
    }

    /// Reports whether the host is one the contract accepts as loopback.
    #[must_use]
    pub fn is_loopback(&self) -> bool {
        ProfileAuthenticationContract::embedded()
            .literals
            .loopback_hosts
            .iter()
            .any(|known| known == &self.host)
    }

    /// Appends encoded endpoint segments to this address.
    ///
    /// Each segment is encoded on its own, so none can introduce a separator,
    /// replace the context path the way resolving a reference would, or climb
    /// out of it: a segment of only dots is escaped in full.
    #[must_use]
    pub fn endpoint(&self, segments: &[&str]) -> String {
        let mut endpoint = self.rendered.clone();
        for segment in segments {
            endpoint.push(PATH_SEPARATOR);
            endpoint.push_str(&percent_encode(segment));
        }
        endpoint
    }
}

impl ::core::fmt::Display for TierBaseAddress {
    fn fmt(&self, formatter: &mut ::core::fmt::Formatter<'_>) -> ::core::fmt::Result {
        formatter.write_str(&self.rendered)
    }
}

/// The user name of a Basic credential.
///
/// Canonical Basic input is the user name bytes, one colon, then the password
/// bytes. A colon in the user name would make that ambiguous and is refused; a
/// colon in the password is fine, because the first colon ends the user name.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct BasicUserName {
    user_name: String,
}

impl BasicUserName {
    /// Validates one Basic user name against the contract.
    ///
    /// The bytes receive no Unicode normalization, so a user name differing
    /// only in composition stays different, as the server comparing it sees it.
    ///
    /// # Errors
    ///
    /// Returns [`ConfigurationFailureCode::ConfigurationValueInvalid`] for an
    /// empty or overlong name, or one holding a colon or a control byte.
    pub fn parse(user_name: &str) -> Result<Self, ProfileDocumentFailure> {
        let limits = &ProfileAuthenticationContract::embedded().limits;
        let refused = user_name.is_empty()
            || user_name.len() > narrow_limit(limits.maximum_basic_username_bytes)
            || user_name.chars().any(|character| character == PORT_SEPARATOR)
            || user_name.chars().any(char::is_control);
        if refused {
            return Err(ProfileDocumentFailure::value("user_name"));
        }
        Ok(Self { user_name: user_name.to_owned() })
    }

    /// Returns the user name exactly as it was written.
    #[must_use]
    pub fn as_text(&self) -> &str {
        &self.user_name
    }
}

/// The typed opt-in that permits a cleartext author address off loopback.
///
/// A caller holding one already knows the environment is Adobe Experience
/// Manager 6.5, Basic, and cleartext off loopback; nothing constructs it from
/// a caller-supplied boolean.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct AllowInsecureAuthorTransport;

/// The stable status a cleartext author address carries.
///
/// Configuration checking and connection setup both report it, so an operator
/// sees the same nonsecret warning wherever the environment is used.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct InsecureAuthorTransportWarning;

/// How one environment authenticates to its author.
///
/// A Basic password is a [`SecretValue`] from the moment it is read.
#[derive(Debug)]
pub enum EnvironmentAuthentication {
    /// Adobe Experience Manager 6.5 Basic credentials.
    BasicCredentials {
        /// Validated user name.
        user_name: BasicUserName,
        /// Password bytes, exactly as written.
        password: SecretValue,
    },
    /// A Developer Console service-credential document below the root.
    DeveloperConsoleServiceCredentialsFile {
        /// Root-relative reference to the credential document.
        credentials_file: ConfigurationReference,
    },
}

impl EnvironmentAuthentication {
    /// Returns the manifest literal this method is written as.
    #[must_use]
    pub fn method(&self) -> &'static str {
        let methods = &ProfileAuthenticationContract::embedded().literals.authentication_methods;
        let position = match self {
            Self::BasicCredentials { .. } => 0,
            Self::DeveloperConsoleServiceCredentialsFile { .. } => 1,
        };
        methods[position].as_str()
    }

    /// Returns the deployment this method is legal with.
    #[must_use]
    pub fn required_deployment(&self) -> AdobeExperienceManagerDeployment {
        match self {
            Self::BasicCredentials { .. } => Deployment::AdobeExperienceManager65,
            Self::DeveloperConsoleServiceCredentialsFile { .. } => {
                Deployment::AdobeExperienceManagerCloudService
            }
        }
    }

    /// Lends the canonical Basic input bytes to `use_bytes`.
    ///
    /// The canonical input is the exact user name bytes, one colon, and the
    /// exact password bytes, neither normalized. It is lent rather than
    /// returned so the assembled buffer is scrubbed when this call ends instead
    /// of becoming a second long-lived copy of the password. Service-credential
    /// authentication has no canonical Basic input and answers `None`.
    pub fn lend_canonical_basic_input<Outcome>(
        &self,
        use_bytes: impl FnOnce(&[u8]) -> Outcome,
    ) -> Option<Outcome> {
        let Self::BasicCredentials { user_name, password } = self else {
            return None;
        };
        let mut canonical =
            Vec::with_capacity(user_name.as_text().len() + password.secret_byte_length() + 1);
        canonical.extend_from_slice(user_name.as_text().as_bytes());
        canonical.push(BASIC_SEPARATOR);
        canonical.extend_from_slice(password.expose_secret_bytes());
        let canonical = SecretValue::from_bytes(canonical);
        Some(use_bytes(canonical.expose_secret_bytes()))
    }
}

/// One named environment of a profile.
#[derive(Debug)]
pub struct Environment {
    deployment: AdobeExperienceManagerDeployment,
    author: TierBaseAddress,
    publisher: TierBaseAddress,
    authentication: EnvironmentAuthentication,
    additional_certificate_authority_file: Option<ConfigurationReference>,
    insecure_author_transport: Option<AllowInsecureAuthorTransport>,
}

impl Environment {
    /// Returns the product this environment runs.
    #[must_use]
    pub fn deployment(&self) -> AdobeExperienceManagerDeployment {
        self.deployment
    }

    /// Returns the address every Adobe Experience Manager request is sent to.
    ///
    /// This is the only method here that yields a connection target.
    #[must_use]
    pub fn author_connection_target(&self) -> &TierBaseAddress {
        &self.author
    }

    /// Returns the publisher address as metadata.
    ///
    /// Commands report and reason about it; Slingshot has no publisher client.
    #[must_use]
    pub fn publisher_metadata(&self) -> &TierBaseAddress {
        &self.publisher
    }

    /// Returns how this environment authenticates to its author.
    #[must_use]
    pub fn authentication(&self) -> &EnvironmentAuthentication {
        &self.authentication
    }

    /// Returns the optional author-only certificate-authority source.
    #[must_use]
    pub fn additional_certificate_authority_file(&self) -> Option<&ConfigurationReference> {
        self.additional_certificate_authority_file.as_ref()
    }

    /// Returns the opt-in, which is present exactly when the author address is
    /// cleartext and off loopback.
    #[must_use]
    pub fn insecure_author_transport(&self) -> Option<AllowInsecureAuthorTransport> {
        self.insecure_author_transport
    }

    /// Returns the warning a cleartext non-loopback author address carries.
    ///
    /// A protected or loopback address carries none, because neither exposes
    /// credentials to a network an operator did not choose.
    #[must_use]
    pub fn insecure_author_transport_warning(&self) -> Option<InsecureAuthorTransportWarning> {
        self.insecure_author_transport.map(|_| InsecureAuthorTransportWarning)
    }
}

/// One profile document.
#[derive(Debug)]
pub struct Profile {
    name: ProfileName,
    environments: BTreeMap<EnvironmentName, Environment>,
}

impl Profile {
    /// Returns the name the document declared.
    #[must_use]
    pub fn name(&self) -> &ProfileName {
        &self.name
    }

    /// Returns the environments the document defined, ordered by name.
    #[must_use]
    pub fn environments(&self) -> &BTreeMap<EnvironmentName, Environment> {
        &self.environments
    }

    /// Parses one profile document.
    ///
    /// # Errors
    ///
    /// Returns the contract's syntax, shape, format, or value code naming the
    /// first member that failed, in manifest order.
    pub fn parse(text: &str) -> Result<Self, ProfileDocumentFailure> {
        refuse_ambiguous_spelling(text, "profile")?;
        let document: ProfileDocument = parse_document(text, "profile")?;
        let limits = &ProfileAuthenticationContract::embedded().limits;
        if document.format_version != limits.supported_profile_format_version {
            return Err(ProfileDocumentFailure::at(
                ConfigurationFailureCode::ConfigurationFormatUnsupported,
                "format_version",
            ));
        }
        let name = ProfileName::parse(&document.name)?;
        if document.environments.is_empty()
            || document.environments.len() > narrow_limit(limits.maximum_environments_per_profile)
        {
            return Err(ProfileDocumentFailure::value("environments"));
        }
        let mut environments = BTreeMap::new();
        for (environment_name, environment) in document.environments {
            let environment_name = EnvironmentName::parse(&environment_name)?;
            environments.insert(environment_name, build_environment(environment)?);
        }
        Ok(Self { name, environments })
    }
}

/// The complete default selection.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SelectionDocument {
    profile: ProfileName,
    environment: EnvironmentName,
}

impl SelectionDocument {
    /// Returns the profile the document selects.
    #[must_use]
    pub fn profile(&self) -> &ProfileName {
        &self.profile
    }

    /// Returns the environment the document selects.
    #[must_use]
    pub fn environment(&self) -> &EnvironmentName {
        &self.environment
    }

    /// Parses one selection document.
    ///
    /// # Errors
    ///
    /// Returns the contract's syntax, shape, format, or value code. Naming only
    /// one of the two members is a shape failure: a partial default is exactly
    /// what the contract refuses.
    pub fn parse(text: &str) -> Result<Self, ProfileDocumentFailure> {
        refuse_ambiguous_spelling(text, "selection")?;
        let document: SelectionDocumentShape = parse_document(text, "selection")?;
        let limits = &ProfileAuthenticationContract::embedded().limits;
        if document.format_version != limits.supported_selection_format_version {
            return Err(ProfileDocumentFailure::at(
                ConfigurationFailureCode::ConfigurationFormatUnsupported,
                "format_version",
            ));
        }
        Ok(Self {
            profile: ProfileName::parse(&document.profile)?,
            environment: EnvironmentName::parse(&document.environment)?,
        })
    }
}

/// The profile document exactly as it is spelled.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct ProfileDocument {
    format_version: u64,
    name: String,
    environments: BTreeMap<String, EnvironmentDocument>,
}

/// One environment exactly as it is spelled.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct EnvironmentDocument {
    deployment: String,
    author: TierDocument,
    publisher: TierDocument,
    authentication: AuthenticationDocument,
    additional_ca_certificate_file: Option<String>,
    allow_insecure_author_transport: Option<bool>,
}

/// One address table exactly as it is spelled.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct TierDocument {
    base_address: String,
}

/// The authentication table exactly as it is spelled.
///
/// Both variants read into one shape so a crossed pair names the member that
/// does not belong, rather than reporting a missing field.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct AuthenticationDocument {
    method: String,
    user_name: Option<String>,
    password: Option<String>,
    credentials_file: Option<String>,
}

/// The selection document exactly as it is spelled.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct SelectionDocumentShape {
    format_version: u64,
    profile: String,
    environment: String,
}

/// Builds one environment and enforces the deployment and transport matrix.
fn build_environment(document: EnvironmentDocument) -> Result<Environment, ProfileDocumentFailure> {
    let deployment = AdobeExperienceManagerDeployment::parse(&document.deployment)?;
    let author = TierBaseAddress::parse(&document.author.base_address)?;
    let publisher = TierBaseAddress::parse(&document.publisher.base_address)?;
    let authentication = build_authentication(document.authentication)?;
    if authentication.required_deployment() != deployment {
        return Err(ProfileDocumentFailure::value("authentication"));
    }
    let additional_certificate_authority_file = document
        .additional_ca_certificate_file
        .map(|reference| ConfigurationReference::parse(&reference))
        .transpose()
        .map_err(|_| ProfileDocumentFailure::value("additional_ca_certificate_file"))?;
    let insecure_author_transport = resolve_cleartext_permission(
        document.allow_insecure_author_transport,
        deployment,
        &author,
        &publisher,
    )?;
    Ok(Environment {
        deployment,
        author,
        publisher,
        authentication,
        additional_certificate_authority_file,
        insecure_author_transport,
    })
}

/// Builds one authentication variant and refuses a crossed pair.
fn build_authentication(
    document: AuthenticationDocument,
) -> Result<EnvironmentAuthentication, ProfileDocumentFailure> {
    let methods = &ProfileAuthenticationContract::embedded().literals.authentication_methods;
    let AuthenticationDocument { method, user_name, password, credentials_file } = document;
    let basic = methods.first().is_some_and(|known| known == &method);
    let cloud = methods.get(1).is_some_and(|known| known == &method);
    if basic {
        if credentials_file.is_some() {
            return Err(ProfileDocumentFailure::value("credentials_file"));
        }
        let user_name = user_name.ok_or_else(|| ProfileDocumentFailure::value("user_name"))?;
        let password = password.ok_or_else(|| ProfileDocumentFailure::value("password"))?;
        let limits = &ProfileAuthenticationContract::embedded().limits;
        if password.len() > narrow_limit(limits.maximum_basic_password_bytes)
            || password.contains('\0')
        {
            return Err(ProfileDocumentFailure::value("password"));
        }
        return Ok(EnvironmentAuthentication::BasicCredentials {
            user_name: BasicUserName::parse(&user_name)?,
            password: SecretValue::from_text(password),
        });
    }
    if !cloud {
        return Err(ProfileDocumentFailure::value("method"));
    }
    if user_name.is_some() {
        return Err(ProfileDocumentFailure::value("user_name"));
    }
    if password.is_some() {
        return Err(ProfileDocumentFailure::value("password"));
    }
    let reference =
        credentials_file.ok_or_else(|| ProfileDocumentFailure::value("credentials_file"))?;
    let credentials_file = ConfigurationReference::parse(&reference)
        .map_err(|_| ProfileDocumentFailure::value("credentials_file"))?;
    Ok(EnvironmentAuthentication::DeveloperConsoleServiceCredentialsFile { credentials_file })
}

/// Decides whether the cleartext author permission is required, legal, or
/// refused.
///
/// It is refused wherever it changes nothing: an explicit `false` says nothing
/// the absence does not, and a permission beside a protected or loopback author
/// claims a risk that is not being taken.
fn resolve_cleartext_permission(
    declared: Option<bool>,
    deployment: AdobeExperienceManagerDeployment,
    author: &TierBaseAddress,
    publisher: &TierBaseAddress,
) -> Result<Option<AllowInsecureAuthorTransport>, ProfileDocumentFailure> {
    let location = "allow_insecure_author_transport";
    let protected_only = matches!(deployment, Deployment::AdobeExperienceManagerCloudService);
    if protected_only && !(author.is_protected() && publisher.is_protected()) {
        return Err(ProfileDocumentFailure::value("base_address"));
    }
    let needed = !author.is_protected() && !author.is_loopback();
    match declared {
        Some(false) => Err(ProfileDocumentFailure::value(location)),
        Some(true) if !needed => Err(ProfileDocumentFailure::value(location)),
        Some(true) => Ok(Some(AllowInsecureAuthorTransport)),
        None if needed => Err(ProfileDocumentFailure::at(
            ConfigurationFailureCode::InsecureAuthorTransportNotAllowed,
            location,
        )),
        None => Ok(None),
    }
}

/// Reports whether one name is a lowercase kebab identifier within its bound.
fn is_kebab_identifier(name: &str, maximum_bytes: usize) -> bool {
    if name.is_empty() || name.len() > maximum_bytes {
        return false;
    }
    let mut groups = name.split('-');
    let opens = groups.next().is_some_and(|first| {
        first.starts_with(|character: char| character.is_ascii_lowercase())
            && first
                .chars()
                .all(|character| character.is_ascii_lowercase() || character.is_ascii_digit())
    });
    opens
        && groups.all(|group| {
            !group.is_empty()
                && group
                    .chars()
                    .all(|character| character.is_ascii_lowercase() || character.is_ascii_digit())
        })
}

/// Splits one authority into its lowercase host and its non-default port.
fn split_authority(
    authority: &str,
    scheme: &str,
    contract: &ProfileAuthenticationContract,
) -> Result<(String, Option<u16>), ProfileDocumentFailure> {
    let refuse = || ProfileDocumentFailure::value("base_address");
    let (host, port_text) = if authority.starts_with(BRACKET_OPEN) {
        let close = authority.find(BRACKET_CLOSE).ok_or_else(refuse)?;
        let (bracketed, remainder) = authority.split_at(close + 1);
        (bracketed, remainder.strip_prefix(PORT_SEPARATOR))
    } else {
        match authority.split_once(PORT_SEPARATOR) {
            Some((host, port)) => (host, Some(port)),
            None => (authority, None),
        }
    };
    let host = host.to_ascii_lowercase();
    if !is_usable_host(&host, narrow_limit(contract.limits.maximum_tier_host_bytes)) {
        return Err(refuse());
    }
    let default = if scheme == contract.literals.schemes[0] {
        contract.literals.scheme_default_ports.http
    } else {
        contract.literals.scheme_default_ports.https
    };
    let port = match port_text {
        None => None,
        Some(text) => {
            let port = parse_port(text)?;
            if port == default {
                return Err(refuse());
            }
            Some(port)
        }
    };
    Ok((host, port))
}

/// Parses one explicit port, refusing a leading zero or an out-of-range value.
fn parse_port(text: &str) -> Result<u16, ProfileDocumentFailure> {
    let refuse = || ProfileDocumentFailure::value("base_address");
    if text.is_empty() || (text.len() > 1 && text.starts_with('0')) {
        return Err(refuse());
    }
    let port: u32 = text.parse().map_err(|_| refuse())?;
    if port == 0 || port > MAXIMUM_PORT {
        return Err(refuse());
    }
    u16::try_from(port).map_err(|_| refuse())
}

/// Reports whether one lowercase host is usable and unambiguous.
fn is_usable_host(host: &str, maximum_bytes: usize) -> bool {
    if host.is_empty() || host.len() > maximum_bytes {
        return false;
    }
    if let Some(literal) =
        host.strip_prefix(BRACKET_OPEN).and_then(|rest| rest.strip_suffix(BRACKET_CLOSE))
    {
        return !literal.is_empty()
            && literal
                .chars()
                .all(|character| character.is_ascii_hexdigit() || character == PORT_SEPARATOR);
    }
    if host.starts_with('.') || host.ends_with('.') {
        return false;
    }
    host.split('.').all(|label| {
        !label.is_empty()
            && label.chars().all(|character| character.is_ascii_alphanumeric() || character == '-')
    })
}

/// Canonicalizes one context-path prefix, or refuses it.
fn canonical_context_path(
    path: &str,
    limits: &crate::profile_authentication_contract::ContractLimits,
) -> Result<String, ProfileDocumentFailure> {
    let refuse = || ProfileDocumentFailure::value("base_address");
    if path.is_empty() || path == "/" {
        return Ok(String::new());
    }
    let Some(body) = path.strip_prefix(PATH_SEPARATOR) else {
        return Err(refuse());
    };
    let segments: Vec<&str> = body.split(PATH_SEPARATOR).collect();
    if segments.len() > narrow_limit(limits.maximum_tier_context_path_segments) {
        return Err(refuse());
    }
    let mut canonical = String::new();
    for segment in segments {
        if segment.len() > narrow_limit(limits.maximum_tier_context_path_segment_bytes) {
            return Err(refuse());
        }
        canonical.push(PATH_SEPARATOR);
        canonical.push_str(&canonical_segment(segment).ok_or_else(refuse)?);
    }
    if canonical.len() > narrow_limit(limits.maximum_tier_context_path_bytes) {
        return Err(refuse());
    }
    Ok(canonical)
}

/// Canonicalizes one path segment, or reports that it is unusable.
///
/// An empty or dot segment is refused outright; every byte inside the rest is
/// judged by the two helpers below, and accepted escapes become uppercase.
fn canonical_segment(segment: &str) -> Option<String> {
    if segment.is_empty() || segment == "." || segment == ".." {
        return None;
    }
    let bytes = segment.as_bytes();
    let mut canonical = String::with_capacity(segment.len());
    let mut position = 0;
    while position < bytes.len() {
        if bytes[position] == b'%' {
            let text = segment.get(position + 1..position + 1 + ESCAPE_DIGITS)?;
            canonical.push('%');
            canonical.push_str(&format!("{:02X}", decode_escape(text)?));
            position += 1 + ESCAPE_DIGITS;
            continue;
        }
        canonical.push(literal_segment_byte(bytes[position])?);
        position += 1;
    }
    Some(canonical)
}

/// Decodes one escape, refusing a byte that must not have been escaped at all
/// and a byte that would reintroduce a separator or a control.
fn decode_escape(text: &str) -> Option<u8> {
    let decoded = u8::from_str_radix(text, ESCAPE_RADIX).ok()?;
    let refused =
        is_unreserved(decoded) || decoded == b'/' || decoded == b'\\' || decoded.is_ascii_control();
    (!refused).then_some(decoded)
}

/// Accepts one literal segment byte, refusing a control, a backslash, an
/// unescaped non-ASCII byte, and anything outside the segment vocabulary.
fn literal_segment_byte(byte: u8) -> Option<char> {
    let usable = byte.is_ascii()
        && !byte.is_ascii_control()
        && byte != b'\\'
        && (is_unreserved(byte) || SEGMENT_PUNCTUATION.contains(char::from(byte)));
    usable.then(|| char::from(byte))
}

/// Percent-encodes one endpoint segment, keeping only unreserved bytes literal.
///
/// A dot is unreserved, so a segment of only dots would survive literally and a
/// server could resolve it as a traversal. Such a segment is escaped in full.
fn percent_encode(segment: &str) -> String {
    let dots = !segment.is_empty() && segment.chars().all(|character| character == '.');
    let mut encoded = String::with_capacity(segment.len());
    for byte in segment.bytes() {
        if is_unreserved(byte) && !dots {
            encoded.push(char::from(byte));
        } else {
            encoded.push('%');
            encoded.push_str(&format!("{byte:02X}"));
        }
    }
    encoded
}

/// Reports whether one byte is unreserved and therefore never escaped.
fn is_unreserved(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || UNRESERVED_PUNCTUATION.contains(char::from(byte))
}

/// Renders one canonical address from its validated parts.
fn render_address(scheme: &str, host: &str, port: Option<u16>, context_path: &str) -> String {
    let mut rendered = format!("{scheme}{SCHEME_SEPARATOR}{host}");
    if let Some(port) = port {
        rendered.push(PORT_SEPARATOR);
        rendered.push_str(&port.to_string());
    }
    rendered.push_str(context_path);
    rendered
}

/// Refuses a document whose spelling has more than one meaning.
///
/// A literal or multiline string, and a dotted key, each let one value be
/// written several ways, so both are refused before the parser resolves them.
fn refuse_ambiguous_spelling(
    text: &str,
    structural_location: &'static str,
) -> Result<(), ProfileDocumentFailure> {
    let ambiguous = text.starts_with('\u{feff}')
        || text.contains('\0')
        || text.lines().any(|line| line_is_ambiguous(line.trim()));
    if ambiguous {
        return Err(ProfileDocumentFailure::at(
            ConfigurationFailureCode::ConfigurationDocumentSyntaxInvalid,
            structural_location,
        ));
    }
    Ok(())
}

/// Reports whether one document line can mean more than one thing.
///
/// A blank line, a comment, and a table header spell no value, so only a
/// key-value line is judged; anything else without an equals sign is refused.
fn line_is_ambiguous(line: &str) -> bool {
    if line.is_empty() || line.starts_with('#') || line.starts_with('[') {
        return false;
    }
    let Some((key, value)) = line.split_once('=') else {
        return true;
    };
    let value = value.trim();
    key.contains(['.', '\'', '"']) || value.starts_with('\'') || value.starts_with("\"\"\"")
}

/// Parses one document in two phases so a failure names its real cause.
///
/// Tokenizing first separates invalid source from valid source with the wrong
/// members. The parser's message is discarded: it quotes source bytes.
fn parse_document<Shape: serde::de::DeserializeOwned>(
    text: &str,
    structural_location: &'static str,
) -> Result<Shape, ProfileDocumentFailure> {
    let value: toml::Value = toml::from_str(text).map_err(|_| {
        ProfileDocumentFailure::at(
            ConfigurationFailureCode::ConfigurationDocumentSyntaxInvalid,
            structural_location,
        )
    })?;
    value.try_into().map_err(|_| {
        ProfileDocumentFailure::at(
            ConfigurationFailureCode::ConfigurationDocumentShapeInvalid,
            structural_location,
        )
    })
}
