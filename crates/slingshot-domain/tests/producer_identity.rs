//! Exact-byte labels yield opaque stable identities; malformed inputs stay private.

use slingshot_domain::daemon_runtime_contract::{DIGEST_CHARACTERS, DaemonRuntimeContract};
use slingshot_domain::producer_identity::ProducerIdentity;

#[test]
fn labels_are_stable_domain_separated_and_not_normalized() {
    let identity = ProducerIdentity::from_label("build").unwrap();
    assert_eq!(identity, ProducerIdentity::from_label("build").unwrap());
    assert_eq!(identity, ProducerIdentity::parse(identity.as_text()).unwrap());
    assert_ne!(identity, ProducerIdentity::from_label("Build").unwrap());
    assert_ne!(identity, ProducerIdentity::from_label("build ").unwrap());
    assert_eq!(identity.as_text().len(), DIGEST_CHARACTERS);
    assert!(!identity.as_text().contains("build"));
}

#[test]
fn label_bound_counts_utf8_bytes_and_rejects_controls_without_echo() {
    let bound = DaemonRuntimeContract::embedded().limit("maximum_producer_label_bytes") as usize;
    assert!(ProducerIdentity::from_label(&"a".repeat(bound)).is_ok());
    assert!(ProducerIdentity::from_label(&"a".repeat(bound + 1)).is_err());
    assert!(ProducerIdentity::from_label(&"é".repeat(bound / "é".len())).is_ok());
    assert!(ProducerIdentity::from_label(&"é".repeat(bound / "é".len() + 1)).is_err());
    for label in ["", "private\0", "private\n", "private\t", "private\u{85}"] {
        let failure = ProducerIdentity::from_label(label).unwrap_err();
        assert!(!failure.to_string().contains("private"));
    }
}

#[test]
fn wire_identity_is_only_canonical_lowercase_digest() {
    assert!(ProducerIdentity::parse(&"a".repeat(DIGEST_CHARACTERS)).is_ok());
    for identity in [
        String::new(),
        "a".repeat(DIGEST_CHARACTERS - 1),
        "a".repeat(DIGEST_CHARACTERS + 1),
        "A".repeat(DIGEST_CHARACTERS),
        "g".repeat(DIGEST_CHARACTERS),
        "é".repeat(DIGEST_CHARACTERS / "é".len()),
    ] {
        assert!(ProducerIdentity::parse(&identity).is_err());
    }
}
