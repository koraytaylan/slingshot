//! Probe for the unicode-normalization capability.
//!
//! Requires deciding whether text is already in normalization form C without
//! rewriting it, because a repository name that arrives decomposed must be
//! refused rather than silently turned into its composed twin.
//!
//! The quick check alone cannot carry that refusal. It decides a composed
//! spelling and rules out a singleton, but it answers `Maybe` for a combining
//! mark, which is exactly the decomposed spelling the refusal is aimed at. So
//! the capability must offer the deciding pass as well, and this probe fails a
//! candidate that offers the quick check alone.

use unicode_normalization::{IsNormalized, UnicodeNormalization};

/// A composed spelling of one accented name.
const COMPOSED: &str = "caf\u{e9}";

/// The decomposed spelling of the same name.
const DECOMPOSED: &str = "cafe\u{301}";

/// A singleton the quick check rules out without deciding anything else.
const SINGLETON: &str = "\u{212b}";

#[test]
fn a_decomposed_spelling_is_reported_rather_than_rewritten() {
    assert_ne!(COMPOSED, DECOMPOSED, "the two spellings are already the same bytes");
    assert_eq!(unicode_normalization::is_nfc_quick(COMPOSED.chars()), IsNormalized::Yes);
    assert_eq!(unicode_normalization::is_nfc_quick(SINGLETON.chars()), IsNormalized::No);
    assert_eq!(unicode_normalization::is_nfc_quick(DECOMPOSED.chars()), IsNormalized::Maybe);

    let decided: String = COMPOSED.nfc().collect();
    assert_eq!(decided, COMPOSED, "the deciding pass leaves a composed spelling alone");
    let rewritten: String = DECOMPOSED.nfc().collect();
    assert_eq!(rewritten, COMPOSED, "the two spellings name the same text");
    assert_eq!(DECOMPOSED.len(), "cafe".len() + 2, "the decomposed spelling is unchanged");
}
