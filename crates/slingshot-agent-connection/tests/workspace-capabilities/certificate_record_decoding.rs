//! Probe for the certificate-record-decoding capability.
//!
//! Requires reaching each decision a trust store must not flatten: the encoded
//! bytes, whether a record may act as an authority, which purposes it permits,
//! the name constraints it carries, and the policy identifiers it declares.

use x509_parser::prelude::*;

use crate::material;

#[test]
fn a_record_exposes_its_authority_purpose_constraints_and_policies() {
    let author_root = material::certificate("author-root");
    let (remainder, decoded) =
        X509Certificate::from_der(author_root.as_ref()).expect("the root decodes");
    assert!(remainder.is_empty(), "the record has no trailing bytes");
    assert!(decoded.subject().to_string().contains("Slingshot Author Test Root"));
    let authority =
        decoded.basic_constraints().expect("the extension reads").expect("the root declares it");
    assert!(authority.value.ca, "the root may act as an authority");

    let leaf = material::certificate("author-leaf");
    let (_, decoded_leaf) = X509Certificate::from_der(leaf.as_ref()).expect("the record decodes");
    let purposes = decoded_leaf
        .extended_key_usage()
        .expect("the extension reads")
        .expect("the record declares it");
    assert!(purposes.value.server_auth, "the record permits server authentication");
    assert!(!purposes.value.client_auth, "the record permits nothing else");

    let restricted = material::certificate("client-only-leaf");
    let (_, decoded_restricted) =
        X509Certificate::from_der(restricted.as_ref()).expect("the record decodes");
    let restricted_purposes = decoded_restricted
        .extended_key_usage()
        .expect("the extension reads")
        .expect("the record declares it");
    assert!(
        !restricted_purposes.value.server_auth,
        "a restricted purpose is visible, not flattened"
    );

    let constrained = material::certificate("name-constrained-root");
    let (_, decoded_constrained) =
        X509Certificate::from_der(constrained.as_ref()).expect("the record decodes");
    let constraints = decoded_constrained
        .name_constraints()
        .expect("the extension reads")
        .expect("the root declares constraints");
    assert!(constraints.value.permitted_subtrees.is_some(), "permitted subtrees are visible");
    assert!(constraints.value.excluded_subtrees.is_some(), "excluded subtrees are visible");
    let declared_policies = decoded_constrained
        .extensions()
        .iter()
        .filter_map(|extension| match extension.parsed_extension() {
            ParsedExtension::CertificatePolicies(policies) => Some(policies),
            _ => None,
        })
        .next()
        .expect("the root declares certificate policies");
    assert!(!declared_policies.is_empty(), "at least one policy identifier is visible");

    assert!(X509Certificate::from_der(b"not a record").is_err(), "malformed input is refused");
}
