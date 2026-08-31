//! Assertions that no secret reaches a rendered diagnostic or a trace.
//!
//! A configuration failure is reported through two channels: an error value a
//! caller renders and walks, and a tracing event a subscriber writes. This test
//! drives both with real machinery - a `thiserror` chain and an installed
//! formatting subscriber - rather than asserting over a formatting call the
//! product would not make, then scans every captured byte.
//!
//! The scan looks for three things. It looks for each sentinel verbatim, and
//! for its hexadecimal and base64 spellings, because an encoded secret is still
//! the secret. It also looks for any long run of hexadecimal digits, because
//! this repository renders every digest as lowercase hexadecimal, so such a run
//! is how a stable fingerprint would appear. A fingerprint of a low-entropy
//! secret is as good as the secret: an attacker enumerates the small candidate
//! set and compares.

use std::error::Error;
use std::io::Write;
use std::sync::{Arc, Mutex};

use base64::Engine;
use slingshot_domain::secret_value::{
    SecretValue, SensitiveConfigurationDocument, SensitiveDocumentNotUnicode,
};
use tracing_subscriber::fmt::MakeWriter;

/// A secret an attacker could enumerate from a small candidate set.
const LOW_ENTROPY_SENTINEL: &str = "admin";

/// A second enumerable secret, to prove nothing keys on the first one's shape.
const SHORT_LOW_ENTROPY_SENTINEL: &str = "0000";

/// A secret only its exact bytes reveal.
const HIGH_ENTROPY_SENTINEL: &str = "q7Fh2mVx-not-a-real-secret-8sKd3Lp0aZ9wRt5nYu1B";

/// Every sentinel the scan looks for.
const SENTINELS: &[&str] =
    &[LOW_ENTROPY_SENTINEL, SHORT_LOW_ENTROPY_SENTINEL, HIGH_ENTROPY_SENTINEL];

/// Shortest run of hexadecimal digits the scan treats as a fingerprint.
///
/// An eight-byte truncation is the shortest digest that would still single out
/// one candidate from an enumerable set, and no incidental value a formatting
/// subscriber writes - a timestamp, a level, a target, a line number - reaches
/// that length in hexadecimal digits alone.
const SHORTEST_FINGERPRINT_HEXADECIMAL_DIGITS: usize = 16;

/// Length of the truncated digest the scan is proved against.
const TRUNCATED_DIGEST_BYTES: usize = SHORTEST_FINGERPRINT_HEXADECIMAL_DIGITS / 2;

/// A profile document that carries a secret in the middle of ordinary text.
const DOCUMENT_TEMPLATE: &str = "format_version = 1\nname = \"local-site\"\npassword = \"{}\"\n";

/// A configuration failure that carries every secret-bearing value it saw.
#[derive(Debug, thiserror::Error)]
#[error("the selected environment could not be authenticated")]
struct EnvironmentFailure {
    /// The extracted secret the failing path held.
    secret: SecretValue,
    /// The source document the failing path was reading.
    document: SensitiveConfigurationDocument,
    /// The failure that caused this one.
    #[source]
    cause: SensitiveDocumentNotUnicode,
}

/// A writer that captures everything a subscriber emits.
#[derive(Clone)]
struct CapturedOutput(Arc<Mutex<Vec<u8>>>);

impl Write for CapturedOutput {
    fn write(&mut self, buffer: &[u8]) -> std::io::Result<usize> {
        self.0.lock().expect("the capture buffer is not poisoned").extend_from_slice(buffer);
        Ok(buffer.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

impl MakeWriter<'_> for CapturedOutput {
    type Writer = Self;

    fn make_writer(&self) -> Self::Writer {
        self.clone()
    }
}

/// Returns the lowercase hexadecimal spelling of `bytes`.
fn hexadecimal(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

/// Returns every spelling of `sentinel` the scan refuses to find.
fn forbidden_spellings(sentinel: &str) -> Vec<String> {
    let bytes = sentinel.as_bytes();
    let lowercase = hexadecimal(bytes);
    vec![
        sentinel.to_owned(),
        lowercase.to_uppercase(),
        lowercase,
        base64::engine::general_purpose::STANDARD.encode(bytes),
        base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(bytes),
    ]
}

/// Returns the longest run of hexadecimal digits in `rendered`.
fn longest_hexadecimal_run(rendered: &str) -> usize {
    let mut longest = 0;
    let mut current = 0;
    for character in rendered.chars() {
        if character.is_ascii_hexdigit() {
            current += 1;
            longest = longest.max(current);
        } else {
            current = 0;
        }
    }
    longest
}

/// Builds one failure that holds every secret-bearing value.
fn failure(sentinel: &str) -> EnvironmentFailure {
    EnvironmentFailure {
        secret: SecretValue::from_text(sentinel.to_owned()),
        document: SensitiveConfigurationDocument::from_bytes(
            DOCUMENT_TEMPLATE.replace("{}", sentinel).into_bytes(),
        ),
        cause: SensitiveDocumentNotUnicode,
    }
}

/// Returns every rendering a caller can obtain from one failure.
fn rendered_error(failure: &EnvironmentFailure) -> String {
    let mut rendered = format!("{failure}\n{failure:?}\n{failure:#?}\n");
    let mut cause: Option<&dyn Error> = failure.source();
    while let Some(current) = cause {
        rendered.push_str(&format!("{current}\n{current:?}\n"));
        cause = current.source();
    }
    rendered.push_str(&format!("{}\n{:?}\n", failure.secret, failure.document));
    rendered
}

/// Returns everything an installed subscriber writes for one failure.
///
/// Without the timestamp, and that omission is the point rather than tidiness.
/// The scan looks for short enumerable sentinels, and a wall-clock timestamp
/// carries four consecutive zeros several times an hour - so a subscriber that
/// wrote one would fail this test at those moments and pass at every other,
/// which is a test that reports the time of day rather than the behaviour.
/// What is under test is what the daemon writes about a secret, and a
/// timestamp is not that.
fn traced_error(failure: &EnvironmentFailure) -> String {
    let buffer = Arc::new(Mutex::new(Vec::new()));
    let subscriber = tracing_subscriber::fmt()
        .with_writer(CapturedOutput(Arc::clone(&buffer)))
        .without_time()
        .with_ansi(false)
        .with_level(true)
        .with_target(true)
        .finish();
    tracing::subscriber::with_default(subscriber, || {
        let span = tracing::error_span!(
            "authenticating",
            secret = %failure.secret,
            document = ?failure.document
        );
        let _entered = span.enter();
        tracing::error!(secret = ?failure.secret, "the secret was rejected");
        tracing::error!(secret = %failure.secret, "the secret was rejected");
        tracing::error!(document = %failure.document, "the document was rejected");
        tracing::error!(error = failure as &dyn Error, "the environment failed");
    });
    let captured = buffer.lock().expect("the capture buffer is not poisoned").clone();
    String::from_utf8(captured).expect("the subscriber writes text")
}

#[test]
fn no_sentinel_reaches_a_rendered_error_or_a_trace() {
    for sentinel in SENTINELS {
        let failure = failure(sentinel);
        let rendered = format!("{}{}", rendered_error(&failure), traced_error(&failure));
        assert!(!rendered.is_empty(), "nothing was rendered, so the scan proves nothing");
        for spelling in forbidden_spellings(sentinel) {
            assert!(!rendered.contains(&spelling), "a rendering carries {spelling}");
        }
    }
}

#[test]
fn no_rendering_carries_a_fingerprint_of_a_low_entropy_secret() {
    for sentinel in SENTINELS {
        let failure = failure(sentinel);
        for rendered in [rendered_error(&failure), traced_error(&failure)] {
            let longest = longest_hexadecimal_run(&rendered);
            assert!(
                longest < SHORTEST_FINGERPRINT_HEXADECIMAL_DIGITS,
                "a rendering carries a {longest}-digit hexadecimal run: {rendered}"
            );
        }
    }
}

#[test]
fn the_scan_would_find_a_secret_that_did_reach_a_trace() {
    let failure = failure(HIGH_ENTROPY_SENTINEL);
    let leaked = format!("{}{}", rendered_error(&failure), HIGH_ENTROPY_SENTINEL);
    let found = forbidden_spellings(HIGH_ENTROPY_SENTINEL)
        .into_iter()
        .filter(|spelling| leaked.contains(spelling))
        .count();
    assert_eq!(found, 1, "the scan cannot see a secret it is given");
    let fingerprinted =
        format!("{}{}", rendered_error(&failure), hexadecimal(&[0; TRUNCATED_DIGEST_BYTES]));
    assert!(
        longest_hexadecimal_run(&fingerprinted) >= SHORTEST_FINGERPRINT_HEXADECIMAL_DIGITS,
        "the scan cannot see a fingerprint it is given"
    );
}
