//! Immutable revision of one selected environment.
//!
//! Four questions have to be answerable without looking at a secret. Is this
//! the same principal as before? Is this the same remote target? Is this the
//! same effective trust? And has anything about the selection changed? Each is
//! answered by a digest over an exactly framed preimage of nonsecret values.
//!
//! The framing exists because concatenating values is ambiguous: two different
//! field sets can produce the same bytes, and an attacker who controls one
//! field could then impersonate another. Every field is therefore framed as its
//! length-prefixed name, one presence byte, and - when present - its
//! length-prefixed value, and every length is added with checked arithmetic.
//!
//! What is left out matters as much as what is in. A password, a private key, a
//! client secret, an access token, a public certificate, a source digest, a
//! timestamp, a file identity, and a permission are all excluded, so rotating a
//! credential without changing who it belongs to leaves the target identity and
//! the revision exactly as they were. A remote job partitioned by target
//! survives a password change, and stops being addressable when the principal
//! or the address behind it changes.

use sha2::{Digest, Sha256};

use crate::profile_authentication_contract::{MemberSpecification, ProfileAuthenticationContract};

/// Bytes one identity digest occupies.
const DIGEST_BYTES: usize = 32;

/// Characters one rendered identity digest occupies.
const RENDERED_DIGEST_BYTES: usize = DIGEST_BYTES * 2;

/// Radix a rendered identity digest is written in.
const DIGEST_RADIX: u32 = 16;

/// Rendered characters one raw digest byte occupies.
const RENDERED_BYTES_PER_DIGEST_BYTE: usize = 2;

/// Bytes one framed length occupies.
const FRAMED_LENGTH_BYTES: usize = 8;

/// Presence byte of a field that carries a value.
const PRESENT: u8 = 1;

/// Presence byte of a field that carries none.
const ABSENT: u8 = 0;

/// Reason an identity could not be constructed.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum IdentityFailure {
    /// A field the preimage requires carried no value.
    #[error("the identity field {0} is required and carried no value")]
    MissingField(String),
    /// A value the preimage carries is empty, which no required field may be.
    #[error("the identity field {0} carried an empty value")]
    EmptyField(String),
    /// A value was supplied for a field the preimage does not frame.
    #[error("the identity preimage does not frame a field named {0}")]
    UnknownField(String),
    /// A length could not be represented, so the framing would be ambiguous.
    #[error("the identity preimage is longer than its framing can describe")]
    LengthOverflow,
    /// A rendered digest is not the exact lowercase hexadecimal form.
    #[error("the rendered identity is not a lowercase hexadecimal digest")]
    MalformedDigest,
}

/// One opaque nonsecret identity.
///
/// It renders as exactly sixty-four lowercase hexadecimal characters, and it is
/// the hash output itself: nothing hashes the rendering a second time, so two
/// consumers that agree on the preimage agree on the value.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct IdentityDigest {
    /// The raw digest bytes.
    digest: [u8; DIGEST_BYTES],
}

impl IdentityDigest {
    /// Returns the digest of `preimage`.
    #[must_use]
    pub fn of(preimage: &[u8]) -> Self {
        let mut hasher = Sha256::new();
        hasher.update(preimage);
        let mut digest = [0; DIGEST_BYTES];
        digest.copy_from_slice(&hasher.finalize());
        Self { digest }
    }

    /// Returns the raw bytes, for a preimage that frames this identity.
    #[must_use]
    pub fn raw(&self) -> &[u8; DIGEST_BYTES] {
        &self.digest
    }

    /// Parses one digest from its lowercase hexadecimal rendering.
    ///
    /// # Errors
    ///
    /// Returns [`IdentityFailure::MalformedDigest`] for any other spelling,
    /// including an uppercase one, so a rendered identity has one form.
    pub fn parse(rendered: &str) -> Result<Self, IdentityFailure> {
        if rendered.len() != RENDERED_DIGEST_BYTES {
            return Err(IdentityFailure::MalformedDigest);
        }
        let (pairs, remainder) = rendered.as_bytes().as_chunks::<RENDERED_BYTES_PER_DIGEST_BYTE>();
        if !remainder.is_empty() {
            return Err(IdentityFailure::MalformedDigest);
        }
        let mut digest = [0; DIGEST_BYTES];
        for (slot, pair) in digest.iter_mut().zip(pairs) {
            let text = core::str::from_utf8(pair).map_err(|_| IdentityFailure::MalformedDigest)?;
            if text.bytes().any(|byte| byte.is_ascii_uppercase()) {
                return Err(IdentityFailure::MalformedDigest);
            }
            *slot = u8::from_str_radix(text, DIGEST_RADIX)
                .map_err(|_| IdentityFailure::MalformedDigest)?;
        }
        Ok(Self { digest })
    }
}

impl ::core::fmt::Display for IdentityDigest {
    fn fmt(&self, formatter: &mut ::core::fmt::Formatter<'_>) -> ::core::fmt::Result {
        for byte in self.digest {
            write!(formatter, "{byte:02x}")?;
        }
        Ok(())
    }
}

/// One field of an identity preimage.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IdentityField {
    /// Name the manifest frames this field under.
    pub name: &'static str,
    /// Value the field carries, when it carries one.
    pub value: Option<Vec<u8>>,
}

impl IdentityField {
    /// Returns a field carrying `value`.
    #[must_use]
    pub fn present(name: &'static str, value: impl Into<Vec<u8>>) -> Self {
        Self { name, value: Some(value.into()) }
    }

    /// Returns a field carrying nothing.
    #[must_use]
    pub fn absent(name: &'static str) -> Self {
        Self { name, value: None }
    }
}

/// Builds one preimage from a domain tag and the manifest's field inventory.
///
/// The order and the presence rules come from the manifest, not from this code,
/// so a field cannot be reordered, dropped, or made optional by editing Rust.
///
/// # Errors
///
/// Returns [`IdentityFailure::MissingField`] when a required field carried no
/// value, [`IdentityFailure::EmptyField`] when a present value is empty,
/// [`IdentityFailure::UnknownField`] when a value was supplied for a field the
/// inventory does not frame, and [`IdentityFailure::LengthOverflow`] when a
/// length cannot be framed.
pub fn build_preimage(
    domain: &str,
    inventory: &[MemberSpecification],
    fields: &[IdentityField],
) -> Result<Vec<u8>, IdentityFailure> {
    for field in fields {
        if !inventory.iter().any(|member| member.name == field.name) {
            return Err(IdentityFailure::UnknownField(field.name.to_owned()));
        }
    }
    let mut preimage = domain.as_bytes().to_vec();
    for member in inventory {
        let supplied = fields.iter().find(|field| field.name == member.name);
        let value = supplied.and_then(|field| field.value.as_deref());
        if member.required && value.is_none() {
            return Err(IdentityFailure::MissingField(member.name.clone()));
        }
        if value.is_some_and(<[u8]>::is_empty) {
            return Err(IdentityFailure::EmptyField(member.name.clone()));
        }
        frame(&mut preimage, &member.name, value)?;
    }
    Ok(preimage)
}

/// Frames one named field into `preimage`.
fn frame(preimage: &mut Vec<u8>, name: &str, value: Option<&[u8]>) -> Result<(), IdentityFailure> {
    preimage.extend_from_slice(&framed_length(name.len())?);
    preimage.extend_from_slice(name.as_bytes());
    let Some(value) = value else {
        preimage.push(ABSENT);
        return Ok(());
    };
    preimage.push(PRESENT);
    preimage.extend_from_slice(&framed_length(value.len())?);
    preimage.extend_from_slice(value);
    Ok(())
}

/// Returns one length in the exact form every frame uses.
fn framed_length(length: usize) -> Result<[u8; FRAMED_LENGTH_BYTES], IdentityFailure> {
    u64::try_from(length).map(u64::to_be_bytes).map_err(|_| IdentityFailure::LengthOverflow)
}

/// Encodes an ordered set of values as a count followed by framed items.
///
/// The count is framed as well as the items, so a set of two values cannot be
/// mistaken for one value that happens to contain the same bytes.
fn encode_set(values: &[Vec<u8>]) -> Result<Vec<u8>, IdentityFailure> {
    let mut encoded = framed_length(values.len())?.to_vec();
    for value in values {
        encoded.extend_from_slice(&framed_length(value.len())?);
        encoded.extend_from_slice(value);
    }
    Ok(encoded)
}

/// Returns the unique ascending form of `values`.
fn canonical_set(values: &[Vec<u8>]) -> Vec<Vec<u8>> {
    let mut unique: Vec<Vec<u8>> = values.to_vec();
    unique.sort();
    unique.dedup();
    unique
}

/// The digest of the contract manifest every identity is bound to.
///
/// Binding it means a build reading a different contract produces different
/// identities, so two builds that disagree about a limit cannot silently share
/// a target partition.
#[must_use]
pub fn profile_authentication_contract_digest() -> IdentityDigest {
    IdentityDigest::of(ProfileAuthenticationContract::embedded_manifest().as_bytes())
}

/// The opaque identity of one authentication principal.
///
/// Only this digest travels: the user name, the organization, the client
/// identifier, and the technical-account identifier stay where authentication
/// is constructed and never reach a target, a log, or a wire value.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct AuthenticationPrincipalIdentity {
    /// The digest itself.
    digest: IdentityDigest,
}

impl AuthenticationPrincipalIdentity {
    /// Returns the identity of one Basic principal.
    ///
    /// # Errors
    ///
    /// Returns an [`IdentityFailure`] when a required field is empty or a
    /// length cannot be framed. The bytes receive no Unicode normalization.
    pub fn basic(method: &str, user_name: &str) -> Result<Self, IdentityFailure> {
        let literals = &ProfileAuthenticationContract::embedded().literals;
        let preimage = build_preimage(
            &literals.identity_domains.authentication_principal,
            &literals.identity_preimage_fields.authentication_principal_basic,
            &[
                IdentityField::present("authentication_method", method),
                IdentityField::present("user_name", user_name),
            ],
        )?;
        Ok(Self { digest: IdentityDigest::of(&preimage) })
    }

    /// Returns the identity of one service-credential principal.
    ///
    /// # Errors
    ///
    /// Returns an [`IdentityFailure`] when a required field is empty or a
    /// length cannot be framed. The bytes receive no Unicode normalization.
    pub fn cloud(
        method: &str,
        organization_identifier: &str,
        technical_account_client_identifier: &str,
        technical_account_identifier: &str,
    ) -> Result<Self, IdentityFailure> {
        let literals = &ProfileAuthenticationContract::embedded().literals;
        let preimage = build_preimage(
            &literals.identity_domains.authentication_principal,
            &literals.identity_preimage_fields.authentication_principal_cloud,
            &[
                IdentityField::present("authentication_method", method),
                IdentityField::present("organization_identifier", organization_identifier),
                IdentityField::present(
                    "technical_account_client_identifier",
                    technical_account_client_identifier,
                ),
                IdentityField::present(
                    "technical_account_identifier",
                    technical_account_identifier,
                ),
            ],
        )?;
        Ok(Self { digest: IdentityDigest::of(&preimage) })
    }

    /// Returns the digest itself.
    #[must_use]
    pub fn digest(&self) -> IdentityDigest {
        self.digest
    }
}

impl ::core::fmt::Display for AuthenticationPrincipalIdentity {
    fn fmt(&self, formatter: &mut ::core::fmt::Formatter<'_>) -> ::core::fmt::Result {
        self.digest.fmt(formatter)
    }
}

/// The durable identity of one author target.
///
/// The hash output is the value; nothing hashes its rendering again. It carries
/// no publisher, no metascope, no trust, and no credential material, so a
/// password or key rotation at the same principal and address leaves a remote
/// job exactly where it was.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct AuthorTargetIdentityDigest {
    /// The digest itself.
    digest: IdentityDigest,
}

impl AuthorTargetIdentityDigest {
    /// Returns the identity of one author target.
    ///
    /// # Errors
    ///
    /// Returns an [`IdentityFailure`] when a required field is empty or a
    /// length cannot be framed.
    pub fn build(
        deployment: &str,
        author_base_address: &str,
        principal: AuthenticationPrincipalIdentity,
    ) -> Result<Self, IdentityFailure> {
        let literals = &ProfileAuthenticationContract::embedded().literals;
        let contract = profile_authentication_contract_digest();
        let preimage = build_preimage(
            &literals.identity_domains.author_target,
            &literals.identity_preimage_fields.author_target,
            &[
                IdentityField::present(
                    "profile_authentication_contract_digest",
                    contract.raw().to_vec(),
                ),
                IdentityField::present("deployment", deployment),
                IdentityField::present("author_base_address", author_base_address),
                IdentityField::present(
                    "authentication_principal_identity",
                    principal.digest().raw().to_vec(),
                ),
            ],
        )?;
        Ok(Self { digest: IdentityDigest::of(&preimage) })
    }

    /// Returns the digest itself.
    #[must_use]
    pub fn digest(&self) -> IdentityDigest {
        self.digest
    }
}

impl ::core::fmt::Display for AuthorTargetIdentityDigest {
    fn fmt(&self, formatter: &mut ::core::fmt::Formatter<'_>) -> ::core::fmt::Result {
        self.digest.fmt(formatter)
    }
}

/// The authorization scope one credential carries, in one form.
///
/// Two credentials that name the same metascopes in different orders, or name
/// one twice, carry the same scope, so they encode identically.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CanonicalMetascopeSet {
    /// The unique ascending values.
    values: Vec<Vec<u8>>,
}

impl CanonicalMetascopeSet {
    /// Returns the canonical form of `values`.
    #[must_use]
    pub fn from_values(values: &[String]) -> Self {
        let raw: Vec<Vec<u8>> = values.iter().map(|value| value.as_bytes().to_vec()).collect();
        Self { values: canonical_set(&raw) }
    }

    /// Returns the empty scope, which Basic authentication carries.
    #[must_use]
    pub fn empty() -> Self {
        Self { values: Vec::new() }
    }

    /// Returns the encoded form the revision frames.
    ///
    /// # Errors
    ///
    /// Returns [`IdentityFailure::LengthOverflow`] when a length cannot be
    /// framed.
    pub fn encoded(&self) -> Result<Vec<u8>, IdentityFailure> {
        encode_set(&self.values)
    }
}

/// The identity of one route's verified trust roots.
///
/// Two routes carry different identities even over the same roots, because each
/// is built under its own domain tag. That is what makes it impossible for an
/// author trust extension to be mistaken for the identity-management one.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct TrustPolicyIdentity {
    /// The digest itself.
    digest: IdentityDigest,
}

impl TrustPolicyIdentity {
    /// Returns the identity of the identity-management route's platform roots.
    ///
    /// # Errors
    ///
    /// Returns [`IdentityFailure::LengthOverflow`] when a length cannot be
    /// framed.
    pub fn identity_management(platform_roots: &[Vec<u8>]) -> Result<Self, IdentityFailure> {
        let domains = &ProfileAuthenticationContract::embedded().literals.identity_domains;
        Self::over(
            &domains.verified_identity_management_trust_policy,
            &canonical_set(platform_roots),
        )
    }

    /// Returns the identity of the author route's roots.
    ///
    /// The author route trusts the platform roots and the selected additional
    /// ones together, which is why its set is a union rather than a second
    /// list: an additional root extends author trust and reaches nothing else.
    ///
    /// # Errors
    ///
    /// Returns [`IdentityFailure::LengthOverflow`] when a length cannot be
    /// framed.
    pub fn author(
        platform_roots: &[Vec<u8>],
        additional_roots: &[Vec<u8>],
    ) -> Result<Self, IdentityFailure> {
        let domains = &ProfileAuthenticationContract::embedded().literals.identity_domains;
        let mut union = platform_roots.to_vec();
        union.extend_from_slice(additional_roots);
        Self::over(&domains.verified_author_trust_policy, &canonical_set(&union))
    }

    /// Returns the identity of one canonical root set under `domain`.
    fn over(domain: &str, roots: &[Vec<u8>]) -> Result<Self, IdentityFailure> {
        let mut preimage = domain.as_bytes().to_vec();
        preimage.extend_from_slice(&encode_set(roots)?);
        Ok(Self { digest: IdentityDigest::of(&preimage) })
    }

    /// Returns the digest itself.
    #[must_use]
    pub fn digest(&self) -> IdentityDigest {
        self.digest
    }
}

impl ::core::fmt::Display for TrustPolicyIdentity {
    fn fmt(&self, formatter: &mut ::core::fmt::Formatter<'_>) -> ::core::fmt::Result {
        self.digest.fmt(formatter)
    }
}

/// Everything one revision is built from.
#[derive(Debug, Clone)]
pub struct RevisionFields {
    /// Name of the selected profile.
    pub profile_name: String,
    /// Name of the selected environment.
    pub environment_name: String,
    /// Reference the selected profile was read from.
    pub profile_source_reference: String,
    /// Reference the selection document was read from, when there was one.
    pub selection_source_reference: Option<String>,
    /// Identity of the author target.
    pub author_target_identity: AuthorTargetIdentityDigest,
    /// Canonical publisher address, which is metadata and never a target.
    pub publisher_base_address: String,
    /// Manifest literal of the authentication method.
    pub authentication_method: String,
    /// Reference the credential document was read from, when there was one.
    pub credential_source_reference: Option<String>,
    /// Reference the certificate document was read from, when there was one.
    pub certificate_source_reference: Option<String>,
    /// Manifest literal of the outbound proxy policy.
    pub proxy_policy: String,
    /// Whether the environment permits a cleartext author off loopback.
    pub allow_insecure_author_transport: bool,
    /// Canonical authorization scope.
    pub canonical_metascope_set: CanonicalMetascopeSet,
    /// Identity of the identity-management route's trust roots.
    pub identity_management_trust_policy_identity: TrustPolicyIdentity,
    /// Identity of the author route's trust roots.
    pub author_trust_policy_identity: TrustPolicyIdentity,
}

/// The revision of one selected environment.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct SelectedEnvironmentRevision {
    /// The digest itself.
    digest: IdentityDigest,
}

impl SelectedEnvironmentRevision {
    /// Returns the revision of one selected environment.
    ///
    /// # Errors
    ///
    /// Returns an [`IdentityFailure`] when a required field is empty or a
    /// length cannot be framed.
    pub fn build(fields: &RevisionFields) -> Result<Self, IdentityFailure> {
        let literals = &ProfileAuthenticationContract::embedded().literals;
        let contract = profile_authentication_contract_digest();
        let permitted = u8::from(fields.allow_insecure_author_transport);
        let framed = [
            IdentityField::present(
                "profile_authentication_contract_digest",
                contract.raw().to_vec(),
            ),
            IdentityField::present("profile_name", fields.profile_name.clone()),
            IdentityField::present("environment_name", fields.environment_name.clone()),
            IdentityField::present(
                "profile_source_reference",
                fields.profile_source_reference.clone(),
            ),
            optional("selection_source_reference", fields.selection_source_reference.as_ref()),
            IdentityField::present(
                "author_target_identity",
                fields.author_target_identity.digest().raw().to_vec(),
            ),
            IdentityField::present("publisher_base_address", fields.publisher_base_address.clone()),
            IdentityField::present("authentication_method", fields.authentication_method.clone()),
            optional("credential_source_reference", fields.credential_source_reference.as_ref()),
            optional("certificate_source_reference", fields.certificate_source_reference.as_ref()),
            IdentityField::present("proxy_policy", fields.proxy_policy.clone()),
            IdentityField::present("allow_insecure_author_transport", vec![permitted]),
            IdentityField::present(
                "canonical_metascope_set",
                fields.canonical_metascope_set.encoded()?,
            ),
            IdentityField::present(
                "identity_management_trust_policy_identity",
                fields.identity_management_trust_policy_identity.digest().raw().to_vec(),
            ),
            IdentityField::present(
                "author_trust_policy_identity",
                fields.author_trust_policy_identity.digest().raw().to_vec(),
            ),
        ];
        let preimage = build_preimage(
            &literals.identity_domains.selected_environment_revision,
            &literals.identity_preimage_fields.selected_environment_revision,
            &framed,
        )?;
        Ok(Self { digest: IdentityDigest::of(&preimage) })
    }

    /// Returns the digest itself.
    #[must_use]
    pub fn digest(&self) -> IdentityDigest {
        self.digest
    }
}

impl ::core::fmt::Display for SelectedEnvironmentRevision {
    fn fmt(&self, formatter: &mut ::core::fmt::Formatter<'_>) -> ::core::fmt::Result {
        self.digest.fmt(formatter)
    }
}

/// Returns one optional field, present exactly when the value is.
fn optional(name: &'static str, value: Option<&String>) -> IdentityField {
    value.map_or_else(
        || IdentityField::absent(name),
        |value| IdentityField::present(name, value.clone()),
    )
}
