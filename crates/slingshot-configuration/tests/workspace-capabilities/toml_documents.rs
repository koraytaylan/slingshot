//! Probe for the TOML documents capability.
//!
//! Requires reading a profile document into a typed shape, refusing an unknown
//! key, and reporting the span of a malformed document, because a profile is
//! rejected with the line the author must fix.

use serde::Deserialize;

#[derive(Debug, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "kebab-case", deny_unknown_fields)]
struct ProfileDocument {
    name: String,
    environments: Vec<String>,
}

#[test]
fn a_profile_document_reads_into_a_typed_shape_and_refuses_a_widened_one() {
    let document = "name = \"one\"\nenvironments = [\"author\", \"publish\"]\n";
    let profile: ProfileDocument = toml::from_str(document).expect("the document reads");
    assert_eq!(profile.name, "one");
    assert_eq!(profile.environments, vec!["author".to_owned(), "publish".to_owned()]);

    let widened = format!("{document}surprise = true\n");
    let refused =
        toml::from_str::<ProfileDocument>(&widened).expect_err("an unknown key is refused");
    assert!(refused.to_string().contains("surprise"), "{refused}");

    let malformed = toml::from_str::<ProfileDocument>("name = \nenvironments = []\n")
        .expect_err("a malformed document is refused");
    assert!(malformed.span().is_some(), "the failure reports where it is");
}
