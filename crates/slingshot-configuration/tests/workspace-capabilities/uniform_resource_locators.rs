//! Probe for the locator capability.
//!
//! Requires absolute parsing, refusal of a relative reference, joining that
//! respects the base path, and a normalized rendering, because an environment
//! address must never be guessed from a fragment.

use url::{ParseError, Url};

#[test]
fn an_absolute_locator_joins_and_normalizes_without_guessing() {
    let base = Url::parse("https://author.example.invalid/bin/").expect("the base parses");
    assert_eq!(base.scheme(), "https");
    assert_eq!(base.host_str(), Some("author.example.invalid"));
    assert_eq!(base.path(), "/bin/");

    let joined = base.join("querybuilder.json?path=/content").expect("the reference joins");
    assert_eq!(
        joined.as_str(),
        "https://author.example.invalid/bin/querybuilder.json?path=/content"
    );
    assert_eq!(joined.query(), Some("path=/content"));

    let relative =
        Url::parse("/bin/querybuilder.json").expect_err("a relative reference is refused");
    assert_eq!(relative, ParseError::RelativeUrlWithoutBase);

    let normalized =
        Url::parse("https://author.example.invalid:443/a/../b").expect("the locator parses");
    assert_eq!(normalized.as_str(), "https://author.example.invalid/b");
}
