//! Turning a content-load invocation into the request the registry describes.
//!
//! The interesting property is that a read needs a caller's operation key.
//! Loading a document changes nothing in the repository and is still not
//! intrinsically idempotent, because it produces an artifact whose retention is
//! charged against a target: running it twice is two pieces of work. The
//! registry says so, and this surface reads that classification rather than
//! inferring idempotency from the read label - which is exactly the inference
//! that would let a retry quietly charge twice.
//!
//! The key is required before anything external is touched, so a caller who
//! forgot it is told rather than discovering it after work has started. The
//! suite proves the ordering by leaving every other option out and watching the
//! key refusal come first.

use slingshot_command_line::commands::content::{
    DEPTH_OPTION, LOAD_CONTENT, PATH_OPTION, RequestRefusal, build,
};
use slingshot_command_line::invocation::{Invocation, ParseRefusal, Selection, parse};
use slingshot_domain::command::catalog::{AccessClassification, Command, CommandCatalog};
use slingshot_domain::command::load_content_as_javascript_object_notation::LoadDepth;

/// A repository path the fixture loads.
const PATH: &str = "/content/site/en";

/// A depth the fixture asks for.
const DEPTH: &str = "3";

/// A depth no load reaches.
const OVERDEEP: &str = "1000000";

/// A path the domain does not accept.
const MALFORMED_PATH: &str = "content/site";

/// The caller key the fixture supplies.
const KEY: &str = "operation-one";

/// Returns the invocation `words` parse into.
fn invocation(words: &[&str]) -> slingshot_command_line::invocation::Invocation {
    parse(&words.iter().map(|word| (*word).to_owned()).collect::<Vec<String>>())
        .expect("the words parse")
}

#[test]
fn a_load_produces_exactly_the_registered_request() {
    let built = build(&invocation(&[
        LOAD_CONTENT,
        "--operation-key",
        KEY,
        PATH_OPTION,
        PATH,
        DEPTH_OPTION,
        DEPTH,
    ]))
    .expect("every value is usable");
    let Command::LoadContentAsJson(request) = built else { panic!("one command, one variant") };
    assert_eq!(request.path.as_text(), PATH);
    assert_eq!(request.depth, Some(LoadDepth::new(3).expect("a depth the contract admits")));
    assert_eq!(request.resolved_depth(), LoadDepth::new(3).expect("the stated depth"));
}

#[test]
fn an_unstated_depth_is_left_unstated_rather_than_filled_in() {
    let built = build(&invocation(&[LOAD_CONTENT, "--operation-key", KEY, PATH_OPTION, PATH]))
        .expect("a path is enough");
    let Command::LoadContentAsJson(request) = built else { panic!("one command, one variant") };
    assert_eq!(
        request.depth, None,
        "the default belongs to the domain, and writing it here would be a second copy of it"
    );
    assert_eq!(request.resolved_depth(), LoadDepth::default_depth());
}

#[test]
fn the_caller_key_is_required_at_the_parser_and_again_at_the_builder() {
    let refusal = parse(&[LOAD_CONTENT.to_owned(), PATH_OPTION.to_owned(), PATH.to_owned()])
        .expect_err("no key was supplied");
    assert!(
        matches!(refusal, ParseRefusal::OperationKeyRequired { .. }),
        "the parser refuses it first, so nothing external is reached at all"
    );

    let keyless = Invocation {
        arguments: [(PATH_OPTION.to_owned(), MALFORMED_PATH.to_owned())].into_iter().collect(),
        detached: false,
        operation_key: None,
        output: None,
        selection: Selection::default(),
        verb: LOAD_CONTENT.to_owned(),
    };
    assert_eq!(
        build(&keyless),
        Err(RequestRefusal::OperationKeyRequired { named: LOAD_CONTENT.to_owned() }),
        "and the builder checks again before parsing a value, so the caller hears about the \
         key rather than the path"
    );
}

#[test]
fn a_value_the_domain_refuses_is_refused_here_with_the_option_that_carried_it() {
    let refusal =
        build(&invocation(&[LOAD_CONTENT, "--operation-key", KEY, PATH_OPTION, MALFORMED_PATH]))
            .expect_err("that is not a repository path");
    assert_eq!(refusal, RequestRefusal::ValueUnusable { named: PATH_OPTION.to_owned() });
    let refusal = build(&invocation(&[
        LOAD_CONTENT,
        "--operation-key",
        KEY,
        PATH_OPTION,
        PATH,
        DEPTH_OPTION,
        OVERDEEP,
    ]))
    .expect_err("that is deeper than a load reaches");
    assert_eq!(refusal, RequestRefusal::ValueUnusable { named: DEPTH_OPTION.to_owned() });
}

#[test]
fn a_required_option_that_is_absent_is_named() {
    let refusal = build(&invocation(&[LOAD_CONTENT, "--operation-key", KEY])).expect_err("no path");
    assert_eq!(refusal, RequestRefusal::OptionMissing { named: PATH_OPTION.to_owned() });
}

#[test]
fn the_family_builds_its_own_command_and_says_so_when_asked_for_another() {
    let refusal = build(&invocation(&["query_paths"])).expect_err("another family owns that");
    assert_eq!(refusal, RequestRefusal::AnotherCommand { named: "query_paths".to_owned() });
}

#[test]
fn the_registry_names_one_read_that_is_not_intrinsically_idempotent() {
    let catalog = CommandCatalog::published();
    let descriptor = catalog.find(LOAD_CONTENT).expect("the registry publishes this command");
    assert_eq!(
        descriptor.access,
        AccessClassification::Read,
        "loading a document changes nothing in the repository"
    );
    assert!(
        descriptor.intrinsic_idempotency.requires_operation_key(),
        "and it is still not intrinsically idempotent, because a repeat is a second artifact"
    );
    assert!(
        !descriptor.access.read_only_hint()
            || descriptor.intrinsic_idempotency.requires_operation_key(),
        "so idempotency is never inferred from the read label"
    );
}
