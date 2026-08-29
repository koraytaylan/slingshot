//! Assertions for the downloaded service-credential document.
//!
//! The document holds a private key and a client secret, so the assertions come
//! in two halves. One half is that the right document is accepted and the wrong
//! ones are not. The other is that nothing a wrong document contains reaches a
//! diagnostic: every rejection is scanned for the fixtures' sentinels, and two
//! documents that fail for different secret-bearing reasons report the same
//! closed tuple.
//!
//! The distinction the fixtures exist to pin is between `technicalAccount.
//! clientId` and `integration.id`. The first is the client identity used for the
//! form field and the audience; the second is the technical account itself and
//! becomes the assertion subject. Swapping them would authenticate as somebody
//! else, so each has its own fixture that changes it alone.

use std::path::PathBuf;

use slingshot_agent_connection::authentication::cloud_service_credentials::CloudServiceCredentials;
use slingshot_domain::profile_authentication_contract::ConfigurationFailureCode;
use slingshot_domain::secret_value::SensitiveConfigurationDocument;

/// Directory holding the committed credential documents.
const CREDENTIAL_FIXTURES: &str = "../slingshot-test-support/fixtures/cloud-credentials";

/// Sentinels no diagnostic may carry.
const SENTINELS: &[&str] = &[
    "p8e-not-a-real-client-secret",
    "PRIVATE KEY",
    "not-a-real-account@techacct.adobe.com",
    "1A2B3C4D5E6F7A8B9C0D1E2F@AdobeOrg",
    "a1b2c3d4e5f6",
    "surprise",
    "not-a-real-value",
];

/// Returns one committed credential document.
fn document(name: &str) -> SensitiveConfigurationDocument {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(CREDENTIAL_FIXTURES).join(name);
    let bytes = std::fs::read(&path)
        .unwrap_or_else(|failure| panic!("{} could not be read: {failure}", path.display()));
    SensitiveConfigurationDocument::from_bytes(bytes)
}

/// Returns the credentials one committed document yields.
fn parsed(name: &str) -> CloudServiceCredentials {
    CloudServiceCredentials::parse(&document(name))
        .unwrap_or_else(|diagnostic| panic!("{name} was refused: {diagnostic:?}"))
}

/// Returns the code one committed document is refused with.
fn refusal(name: &str) -> ConfigurationFailureCode {
    let diagnostic = CloudServiceCredentials::parse(&document(name))
        .map_or_else(|diagnostic| diagnostic, |_| panic!("{name} was accepted"));
    let rendered = format!("{diagnostic:?}");
    for sentinel in SENTINELS {
        assert!(!rendered.contains(sentinel), "{name}: {rendered} carries {sentinel}");
    }
    diagnostic.code
}

#[test]
fn the_documented_shape_is_accepted_and_keeps_its_two_identities_apart() {
    let credentials = parsed("valid.json");
    assert_eq!(credentials.identity_management_authority(), "ims-na1.adobelogin.com");
    assert_eq!(credentials.technical_account_client_identifier(), "a1b2c3d4e5f6");
    assert_eq!(credentials.technical_account_identifier(), "6E1B0F2A5C3D4E5F@techacct.adobe.com");
    assert_ne!(
        credentials.technical_account_client_identifier(),
        credentials.technical_account_identifier(),
        "the client identity and the technical account are one value"
    );
    assert_eq!(credentials.organization_identifier(), "1A2B3C4D5E6F7A8B9C0D1E2F@AdobeOrg");
    assert!(!credentials.public_certificate().is_empty());
}

#[test]
fn rotating_the_key_pair_leaves_the_principal_where_it_was() {
    let before = parsed("valid.json");
    let after = parsed("rotated-key.json");
    assert_eq!(after.principal(), before.principal(), "a rotation moved the principal");
    assert_ne!(
        after.public_certificate(),
        before.public_certificate(),
        "the fixture did not rotate anything"
    );
}

#[test]
fn changing_any_one_member_of_the_tuple_moves_the_principal() {
    let before = parsed("valid.json").principal();
    for name in ["other-organization.json", "other-client.json", "other-technical-account.json"] {
        assert_ne!(parsed(name).principal(), before, "{name} left the principal where it was");
    }
}

#[test]
fn one_scope_has_one_canonical_form() {
    let forward = parsed("valid.json");
    let reordered = parsed("reordered-metascopes.json");
    assert_eq!(
        forward.metascopes().encoded().expect("it encodes"),
        reordered.metascopes().encoded().expect("it encodes"),
        "two orderings of one scope encoded differently"
    );
    assert_eq!(
        refusal("repeated-metascope.json"),
        ConfigurationFailureCode::ServiceCredentialsInvalid
    );
}

#[test]
fn the_deprecated_product_is_named_as_a_product_rather_than_a_malformed_file() {
    assert_eq!(
        refusal("deprecated-product.json"),
        ConfigurationFailureCode::ServiceCredentialsDeprecatedProduct
    );
}

#[test]
fn depth_is_charged_before_the_shape_is_interpreted_at_all() {
    assert_eq!(
        refusal("at-depth-limit.json"),
        ConfigurationFailureCode::ServiceCredentialsDeprecatedProduct,
        "a document at the ceiling was refused before its shape was read"
    );
    assert_eq!(
        refusal("beyond-depth-limit.json"),
        ConfigurationFailureCode::ServiceCredentialsInvalid,
        "a document beyond the ceiling was interpreted before it was charged"
    );
}

#[test]
fn every_other_unusable_document_reports_only_its_own_code() {
    assert_eq!(
        refusal("wrong-authority.json"),
        ConfigurationFailureCode::IdentityManagementAuthorityNotAllowed
    );
    assert_eq!(
        refusal("key-mismatch.json"),
        ConfigurationFailureCode::ServiceCredentialsKeyMismatch
    );
    for name in ["unknown-member.json", "unsuccessful-wrapper.json"] {
        assert_eq!(refusal(name), ConfigurationFailureCode::ServiceCredentialsInvalid, "{name}");
    }
}

#[test]
fn no_rendering_of_accepted_credentials_carries_a_secret() {
    let credentials = parsed("valid.json");
    let rendered = format!("{credentials:?}\n{credentials:#?}");
    for sentinel in ["p8e-not-a-real-client-secret", "PRIVATE KEY", "MII"] {
        assert!(!rendered.contains(sentinel), "{rendered} carries {sentinel}");
    }
    assert!(rendered.contains(&credentials.principal().to_string()), "{rendered}");
    let secret = credentials.client_secret();
    assert_eq!(format!("{secret}"), "[redacted]");
}
