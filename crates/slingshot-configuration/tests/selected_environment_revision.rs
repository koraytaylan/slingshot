//! Assertions for the nonsecret identities a selection produces.
//!
//! Every digest here is compared with one computed by a separate implementation
//! of the framing rules, committed as a fixture. Two implementations that read
//! the same manifest and disagree is a failure; one that quietly agrees with
//! itself proves nothing.
//!
//! The properties these values exist for are the ones the assertions are
//! written around. Rotating a password or a key at the same principal and
//! address must leave the target identity and the revision untouched, because a
//! remote job partitioned by target has to survive a credential rotation.
//! Changing the principal, the address, the authorization scope, or either
//! route's effective trust must not, because each of those changes where
//! credentials go or what they can do.

use std::path::PathBuf;

use serde::Deserialize;
use slingshot_domain::selected_environment_revision::{
    AuthenticationPrincipalIdentity, AuthorTargetIdentityDigest, CanonicalMetascopeSet,
    IdentityDigest, RevisionFields, SelectedEnvironmentRevision, TrustPolicyIdentity,
    profile_authentication_contract_digest,
};

/// Fixture holding the independently calculated vectors.
const VECTOR_FIXTURE: &str = "tests/fixtures/selected-environment-revision/identity-vectors.toml";

/// Method a Basic principal declares.
const BASIC_METHOD: &str = "basic";

/// Method a service-credential principal declares.
const CLOUD_METHOD: &str = "adobe_experience_manager_developer_console_service_credentials";

/// Deployment the vectors are built for.
const DEPLOYMENT: &str = "adobe_experience_manager_cloud_service";

/// Author address the vectors are built for.
const AUTHOR: &str = "https://author.example.com";

/// One platform root the trust vectors are built from.
const PLATFORM_ROOT_A: &[u8] = b"\x30\x01platform-a";

/// A second platform root.
const PLATFORM_ROOT_B: &[u8] = b"\x30\x02platform-b";

/// A root that extends author trust and reaches nothing else.
const ADDITIONAL_AUTHOR_ROOT: &[u8] = b"\x30\x03author-extra";

/// The independently calculated vectors.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct IdentityVectors {
    /// Format identifier of the fixture.
    format: String,
    /// Digest of the contract manifest every identity is bound to.
    contract_digest: String,
    /// Principal identities.
    principal: PrincipalVectors,
    /// Route-specific trust identities.
    trust: TrustVectors,
    /// Author target identities.
    target: TargetVectors,
    /// Selected-environment revisions.
    revision: RevisionVectors,
}

/// Principal identity vectors.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct PrincipalVectors {
    /// Identity of the Basic principal.
    basic: String,
    /// Identity of the service-credential principal.
    cloud: String,
}

/// Route-specific trust identity vectors.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct TrustVectors {
    /// Identity-management trust over no root at all.
    identity_management_empty: String,
    /// Identity-management trust over both platform roots.
    identity_management_two_roots: String,
    /// Author trust over both platform roots.
    author_two_roots: String,
    /// Author trust over both platform roots and one additional root.
    author_two_roots_and_one_additional: String,
}

/// Author target identity vectors.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct TargetVectors {
    /// Identity of the service-credential target.
    cloud: String,
}

/// Revision vectors.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct RevisionVectors {
    /// The revision every other vector is a mutation of.
    base: String,
    /// The same revision with a metascope named twice.
    reordered_metascopes: String,
    /// The same revision with one additional author root.
    extended_author_trust: String,
    /// The same revision permitting a cleartext author.
    permitted_cleartext: String,
    /// The same revision with no selection document.
    without_selection: String,
}

/// Returns the committed vectors.
fn vectors() -> IdentityVectors {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(VECTOR_FIXTURE);
    let text = std::fs::read_to_string(&path)
        .unwrap_or_else(|failure| panic!("{} could not be read: {failure}", path.display()));
    toml::from_str(&text).expect("the vectors read")
}

/// Returns the principal every revision vector is built for.
fn cloud_principal() -> AuthenticationPrincipalIdentity {
    AuthenticationPrincipalIdentity::cloud(CLOUD_METHOD, "ORG@AdobeOrg", "a1b2c3", "integration-1")
        .expect("the principal builds")
}

/// Returns the fields every revision vector is built from.
fn base_fields() -> RevisionFields {
    let principal = cloud_principal();
    let platform = vec![PLATFORM_ROOT_A.to_vec(), PLATFORM_ROOT_B.to_vec()];
    RevisionFields {
        profile_name: "cloud-site".to_owned(),
        environment_name: "production".to_owned(),
        profile_source_reference: "profiles/cloud-site.toml".to_owned(),
        selection_source_reference: Some("selection.toml".to_owned()),
        author_target_identity: AuthorTargetIdentityDigest::build(DEPLOYMENT, AUTHOR, principal)
            .expect("the target builds"),
        publisher_base_address: "https://publish.example.com".to_owned(),
        authentication_method: CLOUD_METHOD.to_owned(),
        credential_source_reference: Some("credentials/production.json".to_owned()),
        certificate_source_reference: None,
        proxy_policy: "direct_without_ambient_discovery".to_owned(),
        allow_insecure_author_transport: false,
        canonical_metascope_set: CanonicalMetascopeSet::from_values(&[
            "ent_aem_cloud_api".to_owned()
        ]),
        identity_management_trust_policy_identity: TrustPolicyIdentity::identity_management(
            &platform,
        )
        .expect("the trust identity builds"),
        author_trust_policy_identity: TrustPolicyIdentity::author(&platform, &[])
            .expect("the trust identity builds"),
    }
}

/// Returns the revision of `fields`.
fn revision_of(fields: &RevisionFields) -> String {
    SelectedEnvironmentRevision::build(fields).expect("the revision builds").to_string()
}

#[test]
fn every_identity_matches_its_independently_calculated_vector() {
    let vectors = vectors();
    assert_eq!(vectors.format, "slingshot.identity-vectors/1");
    assert_eq!(profile_authentication_contract_digest().to_string(), vectors.contract_digest);

    let basic = AuthenticationPrincipalIdentity::basic(BASIC_METHOD, "admin")
        .expect("the principal builds");
    assert_eq!(basic.to_string(), vectors.principal.basic);
    assert_eq!(cloud_principal().to_string(), vectors.principal.cloud);

    let platform = vec![PLATFORM_ROOT_A.to_vec(), PLATFORM_ROOT_B.to_vec()];
    assert_eq!(
        TrustPolicyIdentity::identity_management(&[]).expect("it builds").to_string(),
        vectors.trust.identity_management_empty
    );
    assert_eq!(
        TrustPolicyIdentity::identity_management(&platform).expect("it builds").to_string(),
        vectors.trust.identity_management_two_roots
    );
    assert_eq!(
        TrustPolicyIdentity::author(&platform, &[]).expect("it builds").to_string(),
        vectors.trust.author_two_roots
    );
    assert_eq!(
        TrustPolicyIdentity::author(&platform, &[ADDITIONAL_AUTHOR_ROOT.to_vec()])
            .expect("it builds")
            .to_string(),
        vectors.trust.author_two_roots_and_one_additional
    );

    let target = AuthorTargetIdentityDigest::build(DEPLOYMENT, AUTHOR, cloud_principal())
        .expect("the target builds");
    assert_eq!(target.to_string(), vectors.target.cloud);
    assert_eq!(revision_of(&base_fields()), vectors.revision.base);
}

#[test]
fn the_target_identity_is_the_hash_output_and_not_a_hash_of_its_rendering() {
    let target = AuthorTargetIdentityDigest::build(DEPLOYMENT, AUTHOR, cloud_principal())
        .expect("the target builds");
    let rendered = target.to_string();
    let parsed = IdentityDigest::parse(&rendered).expect("the rendering parses back");
    assert_eq!(parsed, target.digest(), "the rendering is not the value");
    let rehashed = IdentityDigest::of(rendered.as_bytes());
    assert_ne!(rehashed, target.digest(), "the rendering was hashed a second time");
    assert!(IdentityDigest::parse(&rendered.to_uppercase()).is_err(), "two spellings");
}

#[test]
fn one_route_is_never_mistaken_for_the_other_over_the_same_roots() {
    let platform = vec![PLATFORM_ROOT_A.to_vec(), PLATFORM_ROOT_B.to_vec()];
    let identity_management =
        TrustPolicyIdentity::identity_management(&platform).expect("it builds");
    let author = TrustPolicyIdentity::author(&platform, &[]).expect("it builds");
    assert_ne!(identity_management, author, "one root set produced one identity for two routes");
}

#[test]
fn reordering_a_root_or_a_metascope_leaves_every_identity_alone() {
    let vectors = vectors();
    let forward = vec![PLATFORM_ROOT_A.to_vec(), PLATFORM_ROOT_B.to_vec()];
    let reversed = vec![PLATFORM_ROOT_B.to_vec(), PLATFORM_ROOT_A.to_vec()];
    assert_eq!(
        TrustPolicyIdentity::identity_management(&forward).expect("it builds"),
        TrustPolicyIdentity::identity_management(&reversed).expect("it builds")
    );
    let repeated =
        vec![PLATFORM_ROOT_A.to_vec(), PLATFORM_ROOT_B.to_vec(), PLATFORM_ROOT_A.to_vec()];
    assert_eq!(
        TrustPolicyIdentity::author(&forward, &[]).expect("it builds"),
        TrustPolicyIdentity::author(&repeated, &[]).expect("it builds")
    );

    let mut fields = base_fields();
    fields.canonical_metascope_set = CanonicalMetascopeSet::from_values(&[
        "ent_aem_cloud_api".to_owned(),
        "ent_aem_cloud_api".to_owned(),
    ]);
    assert_eq!(revision_of(&fields), vectors.revision.reordered_metascopes);
    assert_eq!(vectors.revision.reordered_metascopes, vectors.revision.base);
}

#[test]
fn changing_either_route_or_the_permission_changes_only_the_revision() {
    let vectors = vectors();
    let base = base_fields();
    let platform = vec![PLATFORM_ROOT_A.to_vec(), PLATFORM_ROOT_B.to_vec()];

    let mut extended = base_fields();
    extended.author_trust_policy_identity =
        TrustPolicyIdentity::author(&platform, &[ADDITIONAL_AUTHOR_ROOT.to_vec()])
            .expect("it builds");
    assert_eq!(revision_of(&extended), vectors.revision.extended_author_trust);
    assert_ne!(revision_of(&extended), vectors.revision.base);
    assert_eq!(
        extended.identity_management_trust_policy_identity,
        base.identity_management_trust_policy_identity,
        "an author root reached identity-management trust"
    );
    assert_eq!(extended.author_target_identity, base.author_target_identity);

    let mut permitted = base_fields();
    permitted.allow_insecure_author_transport = true;
    assert_eq!(revision_of(&permitted), vectors.revision.permitted_cleartext);
    assert_eq!(permitted.author_target_identity, base.author_target_identity);

    let mut without = base_fields();
    without.selection_source_reference = None;
    assert_eq!(revision_of(&without), vectors.revision.without_selection);
    assert_ne!(revision_of(&without), vectors.revision.base);
}

#[test]
fn changing_the_principal_or_the_address_changes_the_target_and_the_revision() {
    let base = base_fields();
    let other_principal = AuthenticationPrincipalIdentity::cloud(
        CLOUD_METHOD,
        "ORG@AdobeOrg",
        "a1b2c3",
        "integration-2",
    )
    .expect("the principal builds");
    let moved = AuthorTargetIdentityDigest::build(DEPLOYMENT, AUTHOR, other_principal)
        .expect("the target builds");
    assert_ne!(moved, base.author_target_identity);

    let elsewhere = AuthorTargetIdentityDigest::build(
        DEPLOYMENT,
        "https://author.example.com/context",
        cloud_principal(),
    )
    .expect("the target builds");
    assert_ne!(elsewhere, base.author_target_identity, "a context path did not move the target");

    let mut changed = base_fields();
    changed.author_target_identity = moved;
    assert_ne!(revision_of(&changed), revision_of(&base));
}

#[test]
fn a_credential_rotation_at_the_same_principal_leaves_everything_alone() {
    let before = base_fields();
    let after = base_fields();
    assert_eq!(after.author_target_identity, before.author_target_identity);
    assert_eq!(revision_of(&after), revision_of(&before));
    let rotated = AuthenticationPrincipalIdentity::cloud(
        CLOUD_METHOD,
        "ORG@AdobeOrg",
        "a1b2c3",
        "integration-1",
    )
    .expect("the principal builds");
    assert_eq!(rotated, cloud_principal(), "the same tuple produced two principals");
}
