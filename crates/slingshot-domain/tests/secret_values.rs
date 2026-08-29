//! Assertions for the extracted-secret and sensitive-document wrappers.
//!
//! The sentinels below stand in for the two shapes a real secret takes. A
//! low-entropy sentinel is a value an attacker could enumerate, so a stable
//! fingerprint of it would be as good as the value itself; a high-entropy
//! sentinel is a value only its exact bytes reveal. Every assertion is made
//! against both, and none of them claims to observe memory a wrapper does not
//! own: the zeroization assertions read only the buffer the wrapper still
//! holds, and the claim's own limits are read from the module documentation.

use serde::Serialize;
use slingshot_domain::secret_value::{
    REDACTED_RENDERING, SecretValue, SensitiveConfigurationDocument, SensitiveDocumentNotUnicode,
};

/// A secret an attacker could enumerate from a small candidate set.
const LOW_ENTROPY_SENTINEL: &str = "admin";

/// A second enumerable secret, to prove nothing keys on the first one's shape.
const SHORT_LOW_ENTROPY_SENTINEL: &str = "0000";

/// A secret only its exact bytes reveal.
const HIGH_ENTROPY_SENTINEL: &str = "q7Fh2mVx-not-a-real-secret-8sKd3Lp0aZ9wRt5nYu1B";

/// A replacement secret, used to prove a rotation clears what it supersedes.
const ROTATED_SENTINEL: &str = "Xj4Tb-not-a-real-secret-9mQ2vLc7ePw0sHn6dRk3";

/// Source file the zeroization claim is read from.
const MODULE_SOURCE: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/src/secret_value.rs");

/// A byte that no Unicode sequence can begin or continue with.
const NOT_UNICODE_BYTE: u8 = 0xff;

/// Memory the zeroization claim must state it does not cover.
const EXCLUDED_MEMORY: &[&str] = &["operating-system", "parser", "allocator", "caller"];

/// The complete public function inventory of the module.
///
/// Pinning the inventory is what keeps the byte access narrow: a new
/// general-purpose accessor cannot appear without this list admitting it, and
/// the two constructors are the only names that repeat.
const PUBLIC_FUNCTIONS: &[&str] = &[
    "dispose",
    "document_byte_length",
    "expose_secret_bytes",
    "from_bytes",
    "from_bytes",
    "from_text",
    "lend_bytes_for_digest",
    "lend_bytes_for_inspection",
    "lend_text_for_parsing",
    "replace",
    "scrub",
    "secret_byte_length",
];

/// Every sentinel, in the order the assertions report them.
const SENTINELS: &[&str] =
    &[LOW_ENTROPY_SENTINEL, SHORT_LOW_ENTROPY_SENTINEL, HIGH_ENTROPY_SENTINEL];

/// Borrowed value the interface questions below are asked about.
///
/// An inherent method whose bound the value satisfies wins over the trait
/// default, so each question answers `true` only when the interface really
/// exists. Asking the question this way keeps the test compiling whether or
/// not the interface is present, which a plain trait bound could not do.
struct Question<'subject, Subject>(&'subject Subject);

/// The answer given when the interface is absent.
trait AbsentInterface {
    /// Whether the value can be copied implicitly.
    fn copying(&self) -> bool {
        false
    }
    /// Whether the value can be compared for equality.
    fn comparison(&self) -> bool {
        false
    }
    /// Whether the value can be ordered.
    fn ordering(&self) -> bool {
        false
    }
    /// Whether the value can be reduced to a hash.
    fn hashing(&self) -> bool {
        false
    }
    /// Whether the value can be written to a document.
    fn serialization(&self) -> bool {
        false
    }
}

impl<Subject> AbsentInterface for Question<'_, Subject> {}

impl<Subject> Question<'_, Subject> {
    /// Returns the value the questions are asked about.
    fn subject(&self) -> &Subject {
        self.0
    }
}

impl<Subject: Clone> Question<'_, Subject> {
    /// Answers that the value can be copied implicitly.
    fn copying(&self) -> bool {
        true
    }
}

impl<Subject: PartialEq> Question<'_, Subject> {
    /// Answers that the value can be compared for equality.
    fn comparison(&self) -> bool {
        true
    }
}

impl<Subject: PartialOrd> Question<'_, Subject> {
    /// Answers that the value can be ordered.
    fn ordering(&self) -> bool {
        true
    }
}

impl<Subject: core::hash::Hash> Question<'_, Subject> {
    /// Answers that the value can be reduced to a hash.
    fn hashing(&self) -> bool {
        true
    }
}

impl<Subject: Serialize> Question<'_, Subject> {
    /// Answers that the value can be written to a document.
    fn serialization(&self) -> bool {
        true
    }
}

/// Returns every rendering a diagnostic or a trace could produce for a value.
fn renderings<Rendered: core::fmt::Display + core::fmt::Debug>(value: &Rendered) -> Vec<String> {
    vec![
        format!("{value}"),
        format!("{value:?}"),
        format!("{value:#?}"),
        format!("{value:>40}"),
        format!("{value:.1}"),
        value.to_string(),
    ]
}

/// Returns the module source the zeroization claim is written in.
fn module_source() -> String {
    std::fs::read_to_string(MODULE_SOURCE).expect("the secret-value module is readable")
}

#[test]
fn every_rendering_of_a_secret_is_the_fixed_redaction() {
    for sentinel in SENTINELS {
        let secret = SecretValue::from_text((*sentinel).to_owned());
        let document = SensitiveConfigurationDocument::from_bytes(sentinel.as_bytes().to_vec());
        for rendered in renderings(&secret).into_iter().chain(renderings(&document)) {
            assert!(!rendered.contains(sentinel), "{rendered} reveals {sentinel}");
            assert_eq!(rendered.trim(), REDACTED_RENDERING, "the redaction varies");
        }
    }
}

#[test]
fn a_secret_exposes_its_bytes_only_through_the_named_call() {
    let secret = SecretValue::from_text(HIGH_ENTROPY_SENTINEL.to_owned());
    assert_eq!(secret.expose_secret_bytes(), HIGH_ENTROPY_SENTINEL.as_bytes());
    assert_eq!(secret.secret_byte_length(), HIGH_ENTROPY_SENTINEL.len());

    let source = module_source();
    let mut declared: Vec<&str> = source
        .lines()
        .map(str::trim)
        .filter_map(|line| line.strip_prefix("pub fn "))
        .filter_map(|rest| rest.split(['(', '<']).next())
        .collect();
    declared.sort_unstable();
    assert_eq!(declared, PUBLIC_FUNCTIONS, "the public function inventory changed");
}

#[test]
fn a_secret_preserves_the_exact_bytes_it_was_given() {
    let composed = "e\u{0301}xample-not-a-real-secret";
    let secret = SecretValue::from_text(composed.to_owned());
    assert_eq!(secret.expose_secret_bytes(), composed.as_bytes(), "the secret was normalized");

    let spare = String::with_capacity(HIGH_ENTROPY_SENTINEL.len() * SENTINELS.len());
    let mut spare = spare;
    spare.push_str(HIGH_ENTROPY_SENTINEL);
    let copied = SecretValue::from_text(spare);
    assert_eq!(copied.expose_secret_bytes(), HIGH_ENTROPY_SENTINEL.as_bytes());
}

#[test]
fn scrubbing_zeroes_the_buffer_the_wrapper_owns() {
    for sentinel in SENTINELS {
        let mut secret = SecretValue::from_text((*sentinel).to_owned());
        let scrubbed = secret.scrub();
        assert_eq!(scrubbed, sentinel.len(), "the report and the buffer disagree");
        assert_eq!(secret.secret_byte_length(), sentinel.len(), "the buffer was released early");
        assert!(
            secret.expose_secret_bytes().iter().all(|byte| *byte == 0),
            "the buffer still holds bytes"
        );
    }
}

#[test]
fn replacing_a_secret_scrubs_the_buffer_it_superseded() {
    let mut secret = SecretValue::from_text(LOW_ENTROPY_SENTINEL.to_owned());
    let scrubbed = secret.replace(ROTATED_SENTINEL.as_bytes().to_vec());
    assert_eq!(scrubbed, LOW_ENTROPY_SENTINEL.len());
    assert_eq!(secret.expose_secret_bytes(), ROTATED_SENTINEL.as_bytes());
    assert_eq!(secret.secret_byte_length(), ROTATED_SENTINEL.len());
}

#[test]
fn a_sensitive_document_lends_its_bytes_only_for_a_named_purpose() {
    let document = SensitiveConfigurationDocument::from_bytes(HIGH_ENTROPY_SENTINEL.into());
    assert_eq!(document.document_byte_length(), HIGH_ENTROPY_SENTINEL.len());
    assert_eq!(document.lend_bytes_for_digest(<[u8]>::len), HIGH_ENTROPY_SENTINEL.len());
    assert!(document.lend_bytes_for_inspection(|bytes| bytes.starts_with(b"q7Fh")));
    assert_eq!(document.lend_text_for_parsing(str::to_owned), Ok(HIGH_ENTROPY_SENTINEL.to_owned()));
    assert_eq!(document.dispose(), HIGH_ENTROPY_SENTINEL.len());
}

#[test]
fn a_document_that_is_not_unicode_is_refused_without_a_position() {
    let invalid = vec![b'a', NOT_UNICODE_BYTE, b'b'];
    let length = invalid.len();
    let document = SensitiveConfigurationDocument::from_bytes(invalid);
    let refusal = document.lend_text_for_parsing(str::to_owned).expect_err("the bytes are refused");
    assert_eq!(refusal, SensitiveDocumentNotUnicode);
    let rendered = format!("{refusal}{refusal:?}");
    assert!(!rendered.contains(&length.to_string()), "{rendered} reveals a position");
    assert_eq!(document.dispose(), length);
}

#[test]
fn neither_wrapper_carries_a_comparison_serialization_or_copying_interface() {
    let secret = SecretValue::from_text(LOW_ENTROPY_SENTINEL.to_owned());
    let document = SensitiveConfigurationDocument::from_bytes(LOW_ENTROPY_SENTINEL.into());
    let control = LOW_ENTROPY_SENTINEL.to_owned();
    let secret = Question(&secret);
    let document = Question(&document);
    let control = Question(&control);
    assert_eq!(control.subject(), LOW_ENTROPY_SENTINEL, "the control is not the sentinel");
    for (name, wrappers, answered) in [
        ("copying", secret.copying() || document.copying(), control.copying()),
        ("comparison", secret.comparison() || document.comparison(), control.comparison()),
        ("ordering", secret.ordering() || document.ordering(), control.ordering()),
        ("hashing", secret.hashing() || document.hashing(), control.hashing()),
        (
            "serialization",
            secret.serialization() || document.serialization(),
            control.serialization(),
        ),
    ] {
        assert!(answered, "the {name} question cannot answer yes, so its no proves nothing");
        assert!(!wrappers, "a wrapper implements {name}");
    }
}

#[test]
fn the_zeroization_claim_names_the_memory_it_does_not_cover() {
    let source = module_source();
    let claim = source.split("# Zeroization claim").nth(1).expect("the module states the claim");
    let claim = claim.split("use ").next().expect("the claim ends before the imports");
    for excluded in EXCLUDED_MEMORY {
        assert!(claim.contains(excluded), "the claim does not exclude {excluded} memory");
    }
}
