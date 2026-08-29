//! Typed access to the normative profile and authentication contract.
//!
//! `policy/profile-authentication-contract-1.json` is embedded into this crate
//! and parsed through one interface, so every Plan 0002 limit, literal, failure
//! code, and precedence order exists exactly once in the repository. A value
//! that is not in the manifest is not a value this plan may use: parsing denies
//! an unknown member and requires every declared one, so a manifest that grows,
//! loses, or renames a value fails the build's own reader rather than drifting
//! past it.
//!
//! The manifest is written in one canonical form - object keys in
//! byte-lexicographic order, no insignificant whitespace, minimal unsigned
//! integers, and one final line feed - and the typed reader mirrors that order
//! field for field. Rendering the parsed contract therefore reproduces the
//! committed bytes exactly, so a reordered key, a widened number, a stray
//! space, and a value this reader silently ignored are all one byte difference
//! rather than a judgement call.

use serde::{Deserialize, Serialize};

/// Bytes of the committed manifest, embedded at compile time.
const EMBEDDED_MANIFEST: &str =
    include_str!("../../../policy/profile-authentication-contract-1.json");

/// Format identifier the manifest must declare.
pub const CONTRACT_FORMAT: &str = "slingshot.profile-authentication-contract/1";

/// Reason the profile-authentication contract could not be read.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ContractFailure {
    /// The manifest bytes are not a valid contract document.
    #[error("the profile-authentication contract could not be read: {0}")]
    Unreadable(String),
    /// The manifest declares a format this build does not implement.
    #[error("the profile-authentication contract declares the format {0}")]
    UnsupportedFormat(String),
    /// The manifest is not in the canonical form its readers regenerate.
    #[error("the profile-authentication contract is not in canonical form")]
    NotCanonical,
    /// A closed inventory repeats an entry.
    #[error("the profile-authentication contract repeats {entry} in {inventory}")]
    RepeatedEntry {
        /// Inventory holding the repetition.
        inventory: &'static str,
        /// Entry that appears more than once.
        entry: String,
    },
    /// A closed inventory is empty where at least one entry is required.
    #[error("the profile-authentication contract leaves {0} empty")]
    EmptyInventory(&'static str),
    /// A value names an entry that its own inventory does not hold.
    #[error("the profile-authentication contract field {field} names the unknown {entry}")]
    UnknownEntry {
        /// Field naming the entry.
        field: &'static str,
        /// Entry no inventory holds.
        entry: String,
    },
    /// The retained diagnostic prefix and the marker do not add up to the limit.
    #[error("the profile-authentication contract diagnostic limit excludes its marker")]
    TruncationLimitExcludesMarker,
}

/// One member of a closed document inventory.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct MemberSpecification {
    /// Exact member name, spelled as the document spells it.
    pub name: String,
    /// Whether a document without the member is invalid.
    pub required: bool,
}

/// Rules the exchange form body is encoded under.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct FormEncoding {
    /// Spelling every byte outside the literal set takes.
    pub escaped_byte_form: String,
    /// Grammar of the bytes that stay literal.
    pub literal_byte_grammar: String,
    /// Spelling a space takes.
    pub space_replacement: String,
}

/// Rules the compact assertion is serialized under.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct JsonWebSignatureEncoding {
    /// Alphabet each segment is encoded with.
    pub alphabet: String,
    /// Padding each encoded segment carries.
    pub padding: String,
    /// Byte that separates two segments.
    pub segment_separator: String,
    /// Form the signature takes before it is encoded.
    pub signature_form: String,
    /// Segments the signing input is built from, in order.
    pub signing_input_segments: Vec<String>,
}

/// The one diagnostic that replaces everything past the retained prefix.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct DiagnosticTruncationMarker {
    /// Failure code the marker carries.
    pub code: String,
    /// Source class the marker names.
    pub source_class: String,
    /// Stage the marker names.
    pub stage: String,
    /// Structural location the marker names.
    pub structural_location: String,
}

/// Domain tag every identity preimage starts with.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct IdentityDomains {
    /// Tag of the authentication-principal preimage.
    pub authentication_principal: String,
    /// Tag of the author-target preimage.
    pub author_target: String,
    /// Tag of the selected-environment-revision preimage.
    pub selected_environment_revision: String,
    /// Tag of the author trust-policy preimage.
    pub verified_author_trust_policy: String,
    /// Tag of the identity-management trust-policy preimage.
    pub verified_identity_management_trust_policy: String,
}

/// Field name and presence of every identity preimage, in framing order.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct IdentityPreimageFields {
    /// Fields a Basic authentication principal frames.
    pub authentication_principal_basic: Vec<MemberSpecification>,
    /// Fields a service-credential authentication principal frames.
    pub authentication_principal_cloud: Vec<MemberSpecification>,
    /// Fields the author target frames.
    pub author_target: Vec<MemberSpecification>,
    /// Fields the selected-environment revision frames.
    pub selected_environment_revision: Vec<MemberSpecification>,
}

/// Port each scheme implies, which a canonical address never spells out.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct SchemeDefaultPorts {
    /// Port the cleartext scheme implies.
    pub http: u16,
    /// Port the protected scheme implies.
    pub https: u16,
}

/// Order in which competing failures are reported.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ContractPrecedence {
    /// Checkpoints an assertion construction fails at, first match reported.
    pub assertion_failure: Vec<String>,
    /// Checkpoints a configuration load fails at, first match reported.
    pub configuration_failure: Vec<String>,
    /// Checkpoints an exchange fails at, first match reported.
    pub exchange_failure: Vec<String>,
}

/// Every numeric bound this plan enforces.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ContractLimits {
    /// Milliseconds before expiry at which a refresh becomes due.
    pub access_token_refresh_skew_milliseconds: u64,
    /// Deadline for name resolution and transport connection.
    pub identity_management_connect_timeout_milliseconds: u64,
    /// Deadline for the whole exchange.
    pub identity_management_overall_timeout_milliseconds: u64,
    /// Port the identity-management endpoint is reached on.
    pub identity_management_port: u64,
    /// Bytes the three field names, equals signs, and separators occupy.
    pub identity_management_request_field_overhead_bytes: u64,
    /// Deadline for writing the complete form body.
    pub identity_management_request_write_timeout_milliseconds: u64,
    /// Deadline between consecutive response body bytes.
    pub identity_management_response_body_idle_timeout_milliseconds: u64,
    /// Deadline for the complete response stream.
    pub identity_management_response_body_total_timeout_milliseconds: u64,
    /// Bytes each decoded field charges beyond its name and value.
    pub identity_management_response_field_charge_bytes: u64,
    /// Bytes a head charges before its fields: the status digits and two line feeds.
    pub identity_management_response_head_status_charge_bytes: u64,
    /// Deadline for one complete response head.
    pub identity_management_response_header_timeout_milliseconds: u64,
    /// Only final status that continues to a token.
    pub identity_management_response_success_status: u64,
    /// Bytes a trailer section charges before its fields, having no status line.
    pub identity_management_response_trailer_charge_bytes: u64,
    /// Deadline for the transport-layer-security handshake.
    pub identity_management_tls_handshake_timeout_milliseconds: u64,
    /// Bytes an access token may occupy.
    pub maximum_access_token_bytes: u64,
    /// Largest lifetime a response may advertise.
    pub maximum_access_token_lifetime_milliseconds: u64,
    /// Certificates that source may carry.
    pub maximum_additional_certificate_authorities: u64,
    /// Bytes one certificate from that source may occupy once decoded.
    pub maximum_additional_certificate_authority_der_bytes: u64,
    /// Bytes the optional author certificate-authority source may occupy.
    pub maximum_additional_certificate_authority_document_bytes: u64,
    /// Bytes the author trust preimage may occupy.
    pub maximum_author_trust_canonical_bytes: u64,
    /// Bytes a Basic password may occupy.
    pub maximum_basic_password_bytes: u64,
    /// Bytes a Basic user name may occupy.
    pub maximum_basic_username_bytes: u64,
    /// Public diagnostics one failure may carry, the truncation marker included.
    pub maximum_configuration_diagnostics: u64,
    /// Whole-generation attempts startup makes before it refuses.
    pub maximum_configuration_generation_attempts: u64,
    /// Bytes every retained source of one generation may occupy together.
    pub maximum_configuration_generation_source_bytes: u64,
    /// Bytes a root-relative credential or certificate reference may occupy.
    pub maximum_configuration_reference_bytes: u64,
    /// Bytes one component of such a reference may occupy.
    pub maximum_configuration_reference_component_bytes: u64,
    /// Path components such a reference may name.
    pub maximum_configuration_reference_components: u64,
    /// Bytes the commit inventory may occupy.
    pub maximum_configuration_snapshot_document_bytes: u64,
    /// Sources the commit inventory may list.
    pub maximum_configuration_snapshot_sources: u64,
    /// Bytes a source whose role is not yet known may occupy.
    pub maximum_configuration_source_document_bytes: u64,
    /// Bytes a commit-inventory source reference may occupy.
    pub maximum_configuration_source_reference_bytes: u64,
    /// Stable-read attempts one source file receives before it is refused.
    pub maximum_configuration_stable_read_attempts: u64,
    /// Bytes a diagnostic structural location may occupy.
    pub maximum_diagnostic_structural_location_bytes: u64,
    /// Bytes an environment name may occupy.
    pub maximum_environment_name_bytes: u64,
    /// Named environments one profile may define.
    pub maximum_environments_per_profile: u64,
    /// Bytes the credential's identity-management authority may occupy.
    pub maximum_identity_management_authority_bytes: u64,
    /// Bytes the exchange form body may occupy, which its field bounds make reachable.
    pub maximum_identity_management_request_body_bytes: u64,
    /// Bytes the response body may charge.
    pub maximum_identity_management_response_body_bytes: u64,
    /// Bytes one decoded response section may charge in total.
    pub maximum_identity_management_response_head_bytes: u64,
    /// Bytes one decoded response field may charge.
    pub maximum_identity_management_response_header_bytes: u64,
    /// Decoded fields one response section may charge.
    pub maximum_identity_management_response_header_count: u64,
    /// Bytes the identity-management trust preimage may occupy.
    pub maximum_identity_management_trust_canonical_bytes: u64,
    /// Bytes one metascope may occupy.
    pub maximum_metascope_bytes: u64,
    /// Metascopes one credential may name.
    pub maximum_metascopes: u64,
    /// Bytes the comma-separated metascope input may occupy.
    pub maximum_metascopes_bytes: u64,
    /// Bytes `integration.org` may occupy.
    pub maximum_organization_identifier_bytes: u64,
    /// Certificates the platform trust snapshot may hold.
    pub maximum_platform_trust_authorities: u64,
    /// Bytes one platform certificate may occupy.
    pub maximum_platform_trust_authority_der_bytes: u64,
    /// Bytes the private-key privacy-enhanced-mail block may occupy.
    pub maximum_private_key_pem_bytes: u64,
    /// Entries the `profiles` directory may hold, counted before extension filtering.
    pub maximum_profile_directory_entries: u64,
    /// Bytes one profile document may occupy.
    pub maximum_profile_document_bytes: u64,
    /// Profile documents one configuration root may define.
    pub maximum_profile_documents: u64,
    /// Bytes a profile file's complete name may occupy.
    pub maximum_profile_file_name_bytes: u64,
    /// Bytes a profile name may occupy.
    pub maximum_profile_name_bytes: u64,
    /// Bytes the public-certificate privacy-enhanced-mail block may occupy.
    pub maximum_public_certificate_pem_bytes: u64,
    /// Bytes the optional selection document may occupy.
    pub maximum_selection_document_bytes: u64,
    /// Bytes the compact assertion may occupy, which its field bounds make reachable.
    pub maximum_service_credential_assertion_bytes: u64,
    /// Bytes a downloaded service-credential document may occupy.
    pub maximum_service_credential_document_bytes: u64,
    /// Root-relative depth a service-credential document may reach.
    pub maximum_service_credential_json_depth: u64,
    /// Largest modulus width the credential private key may have.
    pub maximum_service_credential_rsa_modulus_bits: u64,
    /// Largest coordinated-universal-time second the clock may report.
    pub maximum_service_credential_utc_unix_seconds: u64,
    /// Bytes `technicalAccount.clientId` may occupy.
    pub maximum_technical_account_client_identifier_bytes: u64,
    /// Bytes `technicalAccount.clientSecret` may occupy.
    pub maximum_technical_account_client_secret_bytes: u64,
    /// Bytes the technical-account email may occupy.
    pub maximum_technical_account_email_bytes: u64,
    /// Bytes `integration.id` may occupy.
    pub maximum_technical_account_identifier_bytes: u64,
    /// Bytes a normalized author or publisher base address may occupy.
    pub maximum_tier_base_address_bytes: u64,
    /// Bytes the context-path prefix of a base address may occupy.
    pub maximum_tier_context_path_bytes: u64,
    /// Bytes one context-path segment may occupy.
    pub maximum_tier_context_path_segment_bytes: u64,
    /// Segments that context-path prefix may hold.
    pub maximum_tier_context_path_segments: u64,
    /// Bytes the host of a base address may occupy.
    pub maximum_tier_host_bytes: u64,
    /// Milliseconds of usable lease a token must have beyond that skew.
    pub minimum_access_token_usable_lease_milliseconds: u64,
    /// Smallest modulus width the credential private key may have.
    pub minimum_service_credential_rsa_modulus_bits: u64,
    /// Distinct diagnostics kept ahead of the marker when truncation happens.
    pub retained_configuration_diagnostics: u64,
    /// Seconds added to the sampled second to produce the assertion expiry.
    pub service_credential_assertion_lifetime_seconds: u64,
    /// Depth a nonempty container adds above its deepest direct child.
    pub service_credential_container_child_increment: u64,
    /// Depth an empty array or object contributes to that recurrence.
    pub service_credential_empty_container_depth: u64,
    /// Depth an object member name adds, which is none.
    pub service_credential_member_name_increment: u64,
    /// Only public exponent the credential private key may have.
    pub service_credential_rsa_public_exponent: u64,
    /// Depth a scalar contributes to that recurrence.
    pub service_credential_scalar_depth: u64,
    /// Only `statusCode` the downloaded success wrapper may carry.
    pub service_credential_status_code: u64,
    /// Only `format_version` a commit inventory may declare.
    pub supported_configuration_snapshot_format_version: u64,
    /// Only `format_version` a profile document may declare.
    pub supported_profile_format_version: u64,
    /// Only `format_version` a selection document may declare.
    pub supported_selection_format_version: u64,
}

/// Every exact literal, grammar, and closed inventory this plan accepts.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ContractLiterals {
    /// Content codings the exchange response may declare, beyond declaring none.
    pub accepted_content_codings: Vec<String>,
    /// Grammar an access token matches.
    pub access_token_grammar: String,
    /// Signature algorithm the assertion declares.
    pub assertion_algorithm: String,
    /// Prefix the assertion audience is built from.
    pub assertion_audience_prefix: String,
    /// Claim names the assertion payload carries besides its metascope claims.
    pub assertion_claim_names: Vec<String>,
    /// Prefix each metascope claim name is built from.
    pub assertion_metascope_claim_prefix: String,
    /// Exact protected-header bytes the assertion carries.
    pub assertion_protected_header: String,
    /// Type the assertion declares.
    pub assertion_type: String,
    /// Authentication methods an environment may declare.
    pub authentication_methods: Vec<String>,
    /// Closed member inventory of Basic authentication.
    pub basic_authentication_members: Vec<MemberSpecification>,
    /// Closed member inventory of service-credential authentication.
    pub cloud_authentication_members: Vec<MemberSpecification>,
    /// Components appended to the account home directory to reach the root.
    pub configuration_root_components: Vec<String>,
    /// File below the root that publishes the commit inventory.
    pub configuration_snapshot_file_name: String,
    /// Closed member inventory of the commit-inventory root.
    pub configuration_snapshot_members: Vec<MemberSpecification>,
    /// Closed member inventory of one commit-inventory source.
    pub configuration_snapshot_source_members: Vec<MemberSpecification>,
    /// Deployment values an environment may declare.
    pub deployments: Vec<String>,
    /// Source classes a public diagnostic may name, in sort order.
    pub diagnostic_source_classes: Vec<String>,
    /// Stages a public diagnostic may name, in sort order.
    pub diagnostic_stages: Vec<String>,
    /// The one diagnostic that replaces everything past the retained prefix.
    pub diagnostic_truncation_marker: DiagnosticTruncationMarker,
    /// Closed member inventory of one environment, in validation order.
    pub environment_members: Vec<MemberSpecification>,
    /// Grammar an environment name matches.
    pub environment_name_grammar: String,
    /// Rules the exchange form body is encoded under.
    pub form_encoding: FormEncoding,
    /// Domain tag every identity preimage starts with.
    pub identity_domains: IdentityDomains,
    /// Authorities a credential's identity-management endpoint may name.
    pub identity_management_authorities: Vec<String>,
    /// Path the identity-management endpoint is reached at.
    pub identity_management_endpoint_path: String,
    /// Protocol versions the exchange client negotiates.
    pub identity_management_protocol_versions: Vec<String>,
    /// Form fields the exchange request carries, in order.
    pub identity_management_request_fields: Vec<String>,
    /// Method the exchange request uses.
    pub identity_management_request_method: String,
    /// Closed member inventory of the exchange response document.
    pub identity_management_response_members: Vec<MemberSpecification>,
    /// Scheme the identity-management endpoint is reached with.
    pub identity_management_scheme: String,
    /// Field name and presence of every identity preimage, in framing order.
    pub identity_preimage_fields: IdentityPreimageFields,
    /// Environment variables that never influence root resolution.
    pub ignored_home_variables: Vec<String>,
    /// Environment variables that never influence either client.
    pub ignored_proxy_variables: Vec<String>,
    /// Informational-response handling the exchange installs.
    pub informational_response_policy: String,
    /// Rules the compact assertion is serialized under.
    pub json_web_signature_encoding: JsonWebSignatureEncoding,
    /// Hosts a cleartext author address may use without an opt-in.
    pub loopback_hosts: Vec<String>,
    /// Grammar a rendered source digest matches.
    pub lowercase_secure_hash_grammar: String,
    /// Grammar one component of a root-relative reference matches.
    pub portable_reference_component_grammar: String,
    /// Directory below the root that holds profile documents.
    pub profile_directory_name: String,
    /// Suffix every profile file name ends with.
    pub profile_file_name_suffix: String,
    /// Closed member inventory of a profile document root, in validation order.
    pub profile_members: Vec<MemberSpecification>,
    /// Grammar a profile name matches.
    pub profile_name_grammar: String,
    /// Protocol-migration handling both clients install.
    pub protocol_migration_policy: String,
    /// Outbound proxy handling both clients install.
    pub proxy_policy: String,
    /// Redirect handling both clients install.
    pub redirect_policy: String,
    /// Presence of a connection-upgrade request in the exchange request.
    pub request_connection_upgrade_policy: String,
    /// Presence of an `Expect` field in the exchange request.
    pub request_expect_policy: String,
    /// Media type the exchange request body declares.
    pub request_media_type: String,
    /// Only media-type parameter the exchange response may carry.
    pub response_charset_parameter: String,
    /// Media type the exchange response must declare.
    pub response_media_type: String,
    /// Only token type the exchange response may declare.
    pub response_token_type: String,
    /// Port each scheme implies, which a canonical address never spells out.
    pub scheme_default_ports: SchemeDefaultPorts,
    /// Schemes a base address may use.
    pub schemes: Vec<String>,
    /// File below the root that supplies the default selection.
    pub selection_file_name: String,
    /// Closed member inventory of the selection document root.
    pub selection_members: Vec<MemberSpecification>,
    /// Closed member inventory of its `integration` object, in validation order.
    pub service_credential_integration_members: Vec<MemberSpecification>,
    /// Closed member inventory of the downloaded credential document root.
    pub service_credential_members: Vec<MemberSpecification>,
    /// Closed member inventory of its `technicalAccount` object.
    pub service_credential_technical_account_members: Vec<MemberSpecification>,
    /// Roles the inventory assigns a canonical source reference.
    pub source_roles: Vec<String>,
    /// Closed member inventory of an author or publisher table.
    pub tier_members: Vec<MemberSpecification>,
    /// Specification version every configuration document conforms to.
    pub toml_specification: String,
    /// Trailer handling the exchange installs.
    pub trailer_policy: String,
    /// Transport-layer-security versions either route negotiates.
    pub transport_layer_security_versions: Vec<String>,
}

/// The complete profile-authentication contract.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ProfileAuthenticationContract {
    /// Ordered registry of every failure code this plan may report.
    pub failure_codes: Vec<String>,
    /// Format identifier of the manifest document.
    pub format: String,
    /// Every numeric bound this plan enforces.
    pub limits: ContractLimits,
    /// Every exact literal, grammar, and closed inventory this plan accepts.
    pub literals: ContractLiterals,
    /// Order in which competing failures are reported.
    pub precedence: ContractPrecedence,
}

impl ProfileAuthenticationContract {
    /// Returns the contract embedded in this build.
    ///
    /// The manifest is parsed once and shared, because every bound this plan
    /// checks reads it and re-parsing a fourteen-kilobyte document per check
    /// would make the contract expensive to consult rather than free.
    ///
    /// # Panics
    ///
    /// Panics when the embedded manifest is not a valid contract, which is a
    /// repository defect rather than a runtime condition: the bytes are fixed
    /// at compile time and the same check runs in this crate's tests.
    #[must_use]
    pub fn embedded() -> &'static Self {
        static PARSED: std::sync::OnceLock<ProfileAuthenticationContract> =
            std::sync::OnceLock::new();
        PARSED.get_or_init(|| {
            Self::parse(EMBEDDED_MANIFEST)
                .expect("the embedded profile-authentication contract is valid")
        })
    }

    /// Returns the exact manifest bytes embedded in this build.
    #[must_use]
    pub fn embedded_manifest() -> &'static str {
        EMBEDDED_MANIFEST
    }

    /// Renders this contract back into the manifest's canonical form.
    ///
    /// The reader declares its fields in the manifest's own key order, so the
    /// rendering of a contract that was parsed from a canonical document is
    /// that document. A field this reader does not carry could not survive the
    /// round trip, which is what makes the comparison a completeness proof
    /// rather than a formatting check.
    ///
    /// # Errors
    ///
    /// Returns [`ContractFailure::Unreadable`] when the contract cannot be
    /// rendered, which no parsed contract can fail to be.
    pub fn render(&self) -> Result<String, ContractFailure> {
        let mut rendered = serde_json::to_string(self)
            .map_err(|failure| ContractFailure::Unreadable(failure.to_string()))?;
        rendered.push('\n');
        Ok(rendered)
    }

    /// Parses one manifest document.
    ///
    /// # Errors
    ///
    /// Returns [`ContractFailure::Unreadable`] when the bytes are not a
    /// contract document, [`ContractFailure::UnsupportedFormat`] when the
    /// document declares another format, [`ContractFailure::NotCanonical`] when
    /// the bytes are not the canonical rendering of what they parse to, and one
    /// of the inventory failures when a closed inventory is empty, repeats an
    /// entry, or names an entry no inventory holds.
    pub fn parse(text: &str) -> Result<Self, ContractFailure> {
        let contract: Self = serde_json::from_str(text)
            .map_err(|failure| ContractFailure::Unreadable(failure.to_string()))?;
        if contract.format != CONTRACT_FORMAT {
            return Err(ContractFailure::UnsupportedFormat(contract.format));
        }
        if render_canonical(text)? != text {
            return Err(ContractFailure::NotCanonical);
        }
        contract.verify_inventories()?;
        Ok(contract)
    }

    /// Reports every way the manifest's closed inventories disagree.
    fn verify_inventories(&self) -> Result<(), ContractFailure> {
        unique("failure_codes", &self.failure_codes)?;
        unique("literals.deployments", &self.literals.deployments)?;
        unique("literals.authentication_methods", &self.literals.authentication_methods)?;
        unique("literals.diagnostic_source_classes", &self.literals.diagnostic_source_classes)?;
        unique("literals.diagnostic_stages", &self.literals.diagnostic_stages)?;
        unique("literals.source_roles", &self.literals.source_roles)?;
        unique(
            "literals.identity_management_authorities",
            &self.literals.identity_management_authorities,
        )?;
        if self.literals.identity_management_authorities.is_empty() {
            return Err(ContractFailure::EmptyInventory(
                "literals.identity_management_authorities",
            ));
        }
        let marker = &self.literals.diagnostic_truncation_marker;
        holds("diagnostic_truncation_marker.code", &self.failure_codes, &marker.code)?;
        holds(
            "diagnostic_truncation_marker.source_class",
            &self.literals.diagnostic_source_classes,
            &marker.source_class,
        )?;
        holds(
            "diagnostic_truncation_marker.stage",
            &self.literals.diagnostic_stages,
            &marker.stage,
        )?;
        if self.limits.retained_configuration_diagnostics + 1
            != self.limits.maximum_configuration_diagnostics
        {
            return Err(ContractFailure::TruncationLimitExcludesMarker);
        }
        Ok(())
    }
}

/// Returns the canonical rendering of one manifest document.
///
/// The rendering is what a reader regenerates from what it parsed: object keys
/// in byte-lexicographic order, no insignificant whitespace, and one final line
/// feed. Comparing it with the committed bytes is what makes the canonical form
/// a checked property rather than a convention.
///
/// # Errors
///
/// Returns [`ContractFailure::Unreadable`] when the bytes are not a JavaScript
/// Object Notation document.
pub fn render_canonical(text: &str) -> Result<String, ContractFailure> {
    let value: serde_json::Value = serde_json::from_str(text)
        .map_err(|failure| ContractFailure::Unreadable(failure.to_string()))?;
    let mut rendered = serde_json::to_string(&value)
        .map_err(|failure| ContractFailure::Unreadable(failure.to_string()))?;
    rendered.push('\n');
    Ok(rendered)
}

/// Narrows one contract limit to a length this platform can index with.
///
/// A limit is stored as an unsigned sixty-four-bit value because the manifest
/// is platform-independent; a target whose pointer is narrower saturates rather
/// than wrapping, which keeps the bound conservative instead of accidentally
/// permissive.
#[must_use]
pub fn narrow_limit(limit: u64) -> usize {
    usize::try_from(limit).unwrap_or(usize::MAX)
}

/// Reports the first repeated entry of one closed inventory.
fn unique(inventory: &'static str, entries: &[String]) -> Result<(), ContractFailure> {
    let mut seen = std::collections::BTreeSet::new();
    for entry in entries {
        if !seen.insert(entry) {
            return Err(ContractFailure::RepeatedEntry { inventory, entry: entry.clone() });
        }
    }
    Ok(())
}

/// Reports an entry one closed inventory does not hold.
fn holds(field: &'static str, inventory: &[String], entry: &str) -> Result<(), ContractFailure> {
    if inventory.iter().any(|held| held == entry) {
        return Ok(());
    }
    Err(ContractFailure::UnknownEntry { field, entry: entry.to_owned() })
}

/// Declares the ordered failure-code registry as a closed enumeration.
///
/// The manifest is the authority for the codes themselves; this declaration
/// exists so a caller matches on a value the compiler checks instead of on a
/// string. A test compares the sequence below with the manifest registry, so
/// the two cannot diverge in spelling or in order.
macro_rules! failure_code_registry {
    ($($(#[$attribute:meta])* $variant:ident => $code:literal,)*) => {
        /// A failure this plan may report.
        #[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
        #[non_exhaustive]
        pub enum ConfigurationFailureCode {
            $($(#[$attribute])* $variant,)*
        }

        impl ConfigurationFailureCode {
            /// Every code, in the order the manifest registry declares them.
            pub const REGISTRY: &'static [Self] = &[$(Self::$variant,)*];

            /// Returns the stable lowercase code a caller and a log observe.
            #[must_use]
            pub fn code(self) -> &'static str {
                match self {
                    $(Self::$variant => $code,)*
                }
            }
        }

        impl ::core::fmt::Display for ConfigurationFailureCode {
            fn fmt(&self, formatter: &mut ::core::fmt::Formatter<'_>) -> ::core::fmt::Result {
                formatter.write_str(self.code())
            }
        }
    };
}

failure_code_registry! {
    /// The current platform is not one this build supports.
    UnsupportedPlatform => "unsupported_platform",
    /// The operating-system account database could not be consulted.
    ConfigurationAccountUnavailable => "configuration_account_unavailable",
    /// The account database named no home directory.
    ConfigurationHomeUnavailable => "configuration_home_unavailable",
    /// The account database named more than one home directory.
    ConfigurationHomeAmbiguous => "configuration_home_ambiguous",
    /// The home directory the account database named is not Unicode text.
    ConfigurationHomeNotUnicode => "configuration_home_not_unicode",
    /// The home directory the account database named is not absolute.
    ConfigurationHomeNotAbsolute => "configuration_home_not_absolute",
    /// The configuration root fails the ownership or access-control policy.
    ConfigurationRootUnsafe => "configuration_root_unsafe",
    /// A source reference is not a root-contained portable reference.
    ConfigurationReferenceInvalid => "configuration_reference_invalid",
    /// The profile directory holds more entries than the contract permits.
    ConfigurationDirectoryLimitExceeded => "configuration_directory_limit_exceeded",
    /// A source file fails the ownership, link, or access-control policy.
    ConfigurationFileUnsafe => "configuration_file_unsafe",
    /// A source file changed while both stable-read attempts were reading it.
    ConfigurationFileChangedDuringRead => "configuration_file_changed_during_read",
    /// The commit inventory and the sources on disk are not one generation.
    ConfigurationSnapshotInconsistent => "configuration_snapshot_inconsistent",
    /// A document is larger than the bound its role allows.
    ConfigurationDocumentTooLarge => "configuration_document_too_large",
    /// A document is not Unicode text.
    ConfigurationDocumentNotUtf8 => "configuration_document_not_utf8",
    /// A document does not parse under its document specification.
    ConfigurationDocumentSyntaxInvalid => "configuration_document_syntax_invalid",
    /// A document parses but does not match its closed member inventory.
    ConfigurationDocumentShapeInvalid => "configuration_document_shape_invalid",
    /// A document declares a format version this build does not implement.
    ConfigurationFormatUnsupported => "configuration_format_unsupported",
    /// A document member holds a value outside its grammar or bound.
    ConfigurationValueInvalid => "configuration_value_invalid",
    /// Distinct diagnostics past the retained prefix were replaced by a marker.
    ConfigurationDiagnosticsTruncated => "configuration_diagnostics_truncated",
    /// Two profile documents declare the same profile name.
    ProfileNameDuplicate => "profile_name_duplicate",
    /// A selection names a profile without an environment, or the reverse.
    SelectionIncomplete => "selection_incomplete",
    /// The selected profile name matches no loaded profile.
    ProfileNotFound => "profile_not_found",
    /// The selected environment name matches no environment of that profile.
    EnvironmentNotFound => "environment_not_found",
    /// The authentication principal could not be constructed from its fields.
    AuthenticationPrincipalInvalid => "authentication_principal_invalid",
    /// A cleartext author address is not loopback and carries no exact opt-in.
    InsecureAuthorTransportNotAllowed => "insecure_author_transport_not_allowed",
    /// The platform trust store could not be reduced to an unconditional set.
    PlatformTrustSnapshotInvalid => "platform_trust_snapshot_invalid",
    /// The additional author certificate source is not a valid authority set.
    AdditionalCertificateAuthorityInvalid => "additional_certificate_authority_invalid",
    /// The additional author certificate source carries private-key material.
    AdditionalCertificateAuthorityPrivateKey => "additional_certificate_authority_private_key",
    /// The additional author certificate source carries too many certificates.
    AdditionalCertificateAuthorityLimitExceeded => "additional_certificate_authority_limit_exceeded",
    /// The service-credential document is not the documented success shape.
    ServiceCredentialsInvalid => "service_credentials_invalid",
    /// The document is a deprecated Adobe Developer Console credential instead.
    ServiceCredentialsDeprecatedProduct => "service_credentials_deprecated_product",
    /// The credential private key does not match its public certificate.
    ServiceCredentialsKeyMismatch => "service_credentials_key_mismatch",
    /// The credential names an identity-management authority that is not allowed.
    IdentityManagementAuthorityNotAllowed => "identity_management_authority_not_allowed",
    /// The coordinated-universal-time clock reported no observation.
    AssertionClockUnavailable => "assertion_clock_unavailable",
    /// The clock observation is outside the second range the contract accepts.
    AssertionClockOutOfRange => "assertion_clock_out_of_range",
    /// The sampled second precedes the public certificate's validity interval.
    AssertionCertificateNotYetValid => "assertion_certificate_not_yet_valid",
    /// The sampled second follows the public certificate's validity interval.
    AssertionCertificateExpired => "assertion_certificate_expired",
    /// The assertion could not be signed with the credential private key.
    AssertionSigningFailed => "assertion_signing_failed",
    /// The exchange was cancelled before it completed.
    IdentityManagementCancelled => "identity_management_cancelled",
    /// Name resolution and connection did not finish inside their deadline.
    IdentityManagementConnectTimeout => "identity_management_connect_timeout",
    /// The transport-layer-security handshake did not finish inside its deadline.
    IdentityManagementTlsHandshakeTimeout => "identity_management_tls_handshake_timeout",
    /// The transport-layer-security handshake failed.
    IdentityManagementTlsFailed => "identity_management_tls_failed",
    /// The request body was not written inside its deadline.
    IdentityManagementRequestWriteTimeout => "identity_management_request_write_timeout",
    /// A response head did not arrive complete inside its deadline.
    IdentityManagementResponseHeaderTimeout => "identity_management_response_header_timeout",
    /// The response stream did not complete inside its deadline.
    IdentityManagementResponseBodyTotalTimeout => "identity_management_response_body_total_timeout",
    /// Consecutive response body bytes did not arrive inside their deadline.
    IdentityManagementResponseBodyIdleTimeout => "identity_management_response_body_idle_timeout",
    /// The whole exchange did not complete inside its deadline.
    IdentityManagementOverallTimeout => "identity_management_overall_timeout",
    /// A decoded response section charged more than its bounds allow.
    IdentityManagementResponseHeadLimitExceeded => "identity_management_response_head_limit_exceeded",
    /// The response body charged more than its bound allows.
    IdentityManagementResponseBodyLimitExceeded => "identity_management_response_body_limit_exceeded",
    /// The response is a redirection, which this exchange never follows.
    IdentityManagementRedirectRefused => "identity_management_redirect_refused",
    /// The transport could not produce an unambiguous decoded message.
    IdentityManagementTransportFailed => "identity_management_transport_failed",
    /// The final response status is not the one status that continues.
    IdentityManagementResponseStatusRejected => "identity_management_response_status_rejected",
    /// The response declares or carries a trailer section.
    IdentityManagementResponseTrailerRejected => "identity_management_response_trailer_rejected",
    /// The response content coding or media type is not one that is accepted.
    IdentityManagementResponseMediaInvalid => "identity_management_response_media_invalid",
    /// The response document does not match its closed member inventory.
    IdentityManagementResponseDocumentInvalid => "identity_management_response_document_invalid",
    /// The response declares a token type other than the one that is accepted.
    IdentityManagementTokenTypeInvalid => "identity_management_token_type_invalid",
    /// The response advertises a lifetime outside the range that is accepted.
    IdentityManagementTokenLifetimeInvalid => "identity_management_token_lifetime_invalid",
    /// The token's remaining lease is too short to install.
    AccessTokenLifetimeTooShort => "access_token_lifetime_too_short",
    /// The cache's installation generation counter reached its end.
    AccessTokenInstallationGenerationExhausted => "access_token_installation_generation_exhausted",
    /// A request names a target other than the selected author base address.
    AuthenticationTargetMismatch => "authentication_target_mismatch",
}
