//! Assertions for the values every authorizable command is addressed by.
//!
//! The identifier is the interesting one. It is not a repository name and not a
//! path: it may carry a colon, a full stop, and a hyphen, because real
//! deployments mint identifiers that do, and it may not carry a separator,
//! because an identifier that looked like a path would let a caller believe it
//! had said where something lives when it had not.

use slingshot_domain::command::authorizable_identity::{
    AUTHORIZABLE_KIND_COUNT, AuthorizableIdentifier, AuthorizableIntermediatePath, AuthorizableKind,
};
use slingshot_domain::command::command_identity::CommandContract;

/// Returns one limit by name.
fn limit(name: &str) -> usize {
    usize::try_from(CommandContract::embedded().limit(name)).expect("the bound fits")
}

#[test]
fn an_identifier_accepts_the_spellings_a_deployment_actually_mints() {
    for accepted in [
        "author",
        "content-authors",
        "slingshot.service",
        "first.last@example.test",
        "user_1",
        "sling:reader",
        "Ünicode",
    ] {
        let identifier = AuthorizableIdentifier::parse(accepted)
            .unwrap_or_else(|failure| panic!("{accepted} was refused: {failure:?}"));
        assert_eq!(identifier.as_text(), accepted, "the spelling changed");
        assert_eq!(identifier.to_string(), accepted);
    }
}

#[test]
fn an_identifier_refuses_every_form_that_would_mean_something_else() {
    for refused in ["", "/author", "author/reader", ".", "..", " author", "author ", "aut\u{0}hor"]
    {
        assert!(
            AuthorizableIdentifier::parse(refused).is_err(),
            "{refused:?} was accepted as an identifier"
        );
    }
}

#[test]
fn an_identifier_is_accepted_at_its_bound_and_refused_one_byte_past_it() {
    let bound = limit("maximum_authorizable_identifier_bytes");
    let exact = "a".repeat(bound);
    assert!(AuthorizableIdentifier::parse(&exact).is_ok(), "the bound itself was refused");
    let beyond = "a".repeat(bound + 1);
    assert!(
        AuthorizableIdentifier::parse(&beyond).is_err(),
        "one byte past the bound was accepted"
    );
}

#[test]
fn an_intermediate_path_is_relative_and_made_of_repository_names() {
    let path = AuthorizableIntermediatePath::parse("slingshot/editors").expect("a legal path");
    assert_eq!(path.segments(), vec!["slingshot", "editors"]);
    assert_eq!(path.as_text(), "slingshot/editors");
    for refused in
        ["", "/slingshot", "slingshot/", "slingshot//editors", "slingshot/../editors", "."]
    {
        assert!(
            AuthorizableIntermediatePath::parse(refused).is_err(),
            "{refused:?} was accepted as an intermediate path"
        );
    }
}

#[test]
fn an_intermediate_path_is_accepted_at_its_bound_and_refused_one_byte_past_it() {
    let bound = limit("maximum_authorizable_intermediate_path_bytes");
    let name_bound = limit("maximum_repository_name_bytes");
    let segments = bound.div_ceil(name_bound);
    let separators = segments - 1;
    let each = (bound - separators) / segments;
    let mut exact: String =
        (0..segments).map(|_| "a".repeat(each)).collect::<Vec<String>>().join("/");
    while exact.len() < bound {
        exact.push('a');
    }
    assert_eq!(exact.len(), bound, "the fixture is not the bound itself");
    assert!(
        AuthorizableIntermediatePath::parse(&exact).is_ok(),
        "the bound itself was refused: {exact:?}"
    );
    assert!(
        AuthorizableIntermediatePath::parse(&format!("{exact}a")).is_err(),
        "one byte past the bound was accepted"
    );
}

#[test]
fn an_intermediate_path_segment_is_held_to_the_repository_name_bound() {
    let name_bound = limit("maximum_repository_name_bytes");
    let exact = "a".repeat(name_bound);
    assert!(
        AuthorizableIntermediatePath::parse(&exact).is_ok(),
        "a name-sized segment was refused"
    );
    assert!(
        AuthorizableIntermediatePath::parse(&format!("{exact}a")).is_err(),
        "a segment one byte past the repository name bound was accepted"
    );
}

#[test]
fn a_kind_is_two_spellings_and_no_third() {
    for (kind, spelling) in
        [(AuthorizableKind::Group, "\"group\""), (AuthorizableKind::User, "\"user\"")]
    {
        let written = serde_json::to_string(&kind).expect("a kind serializes");
        assert_eq!(written, spelling);
        let read: AuthorizableKind = serde_json::from_str(spelling).expect("a kind parses");
        assert_eq!(read, kind);
        assert_eq!(kind.to_string(), spelling.trim_matches('"'));
    }
    assert!(serde_json::from_str::<AuthorizableKind>("\"administrator\"").is_err());
    assert_eq!(AuthorizableKind::both().len(), AUTHORIZABLE_KIND_COUNT, "two kinds, and no third");
}
