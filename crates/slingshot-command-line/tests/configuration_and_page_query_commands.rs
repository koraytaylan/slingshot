//! One predicate grammar, one window rule, and three searches that share both.
//!
//! The predicates are the subject. A bespoke flag grammar for the same
//! questions would be a second encoding of one vocabulary, and the two would
//! drift on exactly the values where drifting matters: which operator takes a
//! value, which take several, and which types those may be. So every spelling
//! comes from the domain's own constants, and the suite walks the declared
//! operator list rather than a list written here - a new operator this build
//! publishes and this parser does not handle is a failure rather than a silent
//! omission.
//!
//! A phrase is passed exactly as typed. Trimming it would mean searching for
//! something the caller did not ask for and reporting the matches as though
//! they had, so the domain's refusal of leading and trailing whitespace comes
//! through instead.
//!
//! A window is an offset and a limit or a continuation token, never both. A
//! token already carries the window it was issued under, so accepting one
//! beside a fresh offset would let a caller widen the page it was bound to.

use slingshot_command_line::commands::configuration::{IDENTIFIER_OPTION, INSPECT_CONFIGURATION};
use slingshot_command_line::commands::content::RequestRefusal;
use slingshot_command_line::commands::{configuration, page_query, path_query};
use slingshot_command_line::invocation::{
    CONTINUATION_TOKEN_OPTION, Invocation, LIMIT_OPTION, MATCH_ALL_OPTION, NODE_TYPE_OPTION,
    OFFSET_OPTION, PATH_OPTION, PHRASE_OPTION, PROPERTY_PREDICATE_OPTION, RESOURCE_TYPES_OPTION,
    TEMPLATE_OPTION, parse,
};
use slingshot_command_line::predicate_arguments::{
    OPERATOR_MEMBER, PREDICATE_OPTION, PROPERTY_PATH_MEMBER, PredicateArgumentRefusal, TYPE_MEMBER,
    VALUE_MEMBER, VALUES_MEMBER, parse_one,
};
use slingshot_domain::command::catalog::{AccessClassification, Command, CommandCatalog};
use slingshot_domain::command::find_pages_using_components::ComponentMatchMode;
use slingshot_domain::command::property_value::{
    BOOLEAN_TYPE, DATE_TIME_TYPE, DECIMAL_TYPE, INTEGER_TYPE, REPOSITORY_PATH_TYPE, STRING_TYPE,
};
use slingshot_domain::command::search_predicate::{
    DECLARED_OPERATORS, EXISTS_OPERATOR, PropertyPredicate,
};

/// A repository path these fixtures search under.
const ROOT: &str = "/content/site";

/// A template a page records.
const TEMPLATE: &str = "/apps/site/templates/page";

/// A phrase with internal whitespace, which survives exactly.
const PHRASE: &str = "the quick  brown fox";

/// A phrase with leading whitespace, which the domain refuses.
const UNTRIMMED_PHRASE: &str = " the quick brown fox";

/// Component resource types a page may use.
const RESOURCE_TYPES: &str = "site/components/hero,site/components/teaser";

/// A configuration this build inspects.
const PERSISTENT_IDENTIFIER: &str = "com.example.Service";

/// The property one predicate asks about.
const PROPERTY: &str = "jcr:title";

/// Returns the invocation `words` parse into.
fn invocation(words: &[&str]) -> Invocation {
    parse(&words.iter().map(|word| (*word).to_owned()).collect::<Vec<String>>())
        .expect("the words parse")
}

/// Returns one predicate object with the given members.
fn predicate(operator: &str, extra: &str) -> String {
    format!(
        "{{\"{PROPERTY_PATH_MEMBER}\":\"{PROPERTY}\",\"{OPERATOR_MEMBER}\":\"{operator}\"{extra}}}"
    )
}

#[test]
fn every_operator_this_build_publishes_is_one_the_parser_handles() {
    for operator in DECLARED_OPERATORS {
        let extra = match *operator {
            "exists" => String::new(),
            "scalar_in" | "list_contains_any" | "list_contains_all" => {
                format!(",\"{TYPE_MEMBER}\":\"{STRING_TYPE}\",\"{VALUES_MEMBER}\":[\"a\",\"b\"]")
            }
            _ => format!(",\"{TYPE_MEMBER}\":\"{STRING_TYPE}\",\"{VALUE_MEMBER}\":\"a\""),
        };
        let parsed = parse_one(&predicate(operator, &extra))
            .unwrap_or_else(|refusal| panic!("{operator}: {refusal}"));
        assert_eq!(
            parsed.operator(),
            *operator,
            "a new operator this build publishes must not be a silent omission here"
        );
        assert_eq!(parsed.property_path().as_text(), PROPERTY);
    }
}

#[test]
fn presence_takes_no_value_and_equality_takes_exactly_one() {
    let bare = parse_one(&predicate(EXISTS_OPERATOR, "")).expect("presence needs nothing");
    assert!(matches!(bare, PropertyPredicate::Exists { .. }));
    let extra = format!(",\"{TYPE_MEMBER}\":\"{STRING_TYPE}\",\"{VALUE_MEMBER}\":\"a\"");
    assert_eq!(
        parse_one(&predicate(EXISTS_OPERATOR, &extra)),
        Err(PredicateArgumentRefusal::ValueNotTaken),
        "a value on a presence question is a question nobody asked"
    );
    let both = format!(
        ",\"{TYPE_MEMBER}\":\"{STRING_TYPE}\",\"{VALUE_MEMBER}\":\"a\",\"{VALUES_MEMBER}\":[\"b\"]"
    );
    assert!(matches!(
        parse_one(&predicate("equals", &both)),
        Err(PredicateArgumentRefusal::SurplusMember { .. })
    ));
}

#[test]
fn a_membership_list_is_non_empty_ordered_and_holds_each_value_once() {
    let empty = format!(",\"{TYPE_MEMBER}\":\"{STRING_TYPE}\",\"{VALUES_MEMBER}\":[]");
    assert_eq!(
        parse_one(&predicate("scalar_in", &empty)),
        Err(PredicateArgumentRefusal::MembershipUnusable)
    );
    let repeated =
        format!(",\"{TYPE_MEMBER}\":\"{STRING_TYPE}\",\"{VALUES_MEMBER}\":[\"a\",\"a\"]");
    assert_eq!(
        parse_one(&predicate("scalar_in", &repeated)),
        Err(PredicateArgumentRefusal::MembershipUnusable),
        "duplicates fail rather than collapse, because collapsing changes the question"
    );
}

#[test]
fn a_type_this_build_does_not_publish_names_what_was_written() {
    let unknown = format!(",\"{TYPE_MEMBER}\":\"colour\",\"{VALUE_MEMBER}\":\"a\"");
    assert_eq!(
        parse_one(&predicate("equals", &unknown)),
        Err(PredicateArgumentRefusal::UnknownType { named: "colour".to_owned() })
    );
    for named in [
        STRING_TYPE,
        BOOLEAN_TYPE,
        INTEGER_TYPE,
        DECIMAL_TYPE,
        DATE_TIME_TYPE,
        REPOSITORY_PATH_TYPE,
    ] {
        let stated = match named {
            BOOLEAN_TYPE => format!(",\"{TYPE_MEMBER}\":\"{named}\",\"{VALUE_MEMBER}\":true"),
            INTEGER_TYPE => format!(",\"{TYPE_MEMBER}\":\"{named}\",\"{VALUE_MEMBER}\":\"7\""),
            DECIMAL_TYPE => format!(",\"{TYPE_MEMBER}\":\"{named}\",\"{VALUE_MEMBER}\":\"7.50\""),
            DATE_TIME_TYPE => format!(
                ",\"{TYPE_MEMBER}\":\"{named}\",\"{VALUE_MEMBER}\":\"2026-08-30T12:00:00Z\""
            ),
            REPOSITORY_PATH_TYPE => {
                format!(",\"{TYPE_MEMBER}\":\"{named}\",\"{VALUE_MEMBER}\":\"{ROOT}\"")
            }
            _ => format!(",\"{TYPE_MEMBER}\":\"{named}\",\"{VALUE_MEMBER}\":\"a\""),
        };
        assert!(
            parse_one(&predicate("equals", &stated)).is_ok(),
            "{named} is a type this build publishes and this parser accepts"
        );
    }
}

#[test]
fn a_surplus_member_is_named_rather_than_ignored() {
    let surplus = format!(
        "{{\"{PROPERTY_PATH_MEMBER}\":\"{PROPERTY}\",\"{OPERATOR_MEMBER}\":\"{EXISTS_OPERATOR}\",\
         \"colour\":\"red\"}}"
    );
    assert_eq!(
        parse_one(&surplus),
        Err(PredicateArgumentRefusal::SurplusMember { named: "colour".to_owned() }),
        "ignoring it would accept a question the caller thinks they asked"
    );
    assert_eq!(parse_one("not json at all"), Err(PredicateArgumentRefusal::NotAnObject));
}

#[test]
fn a_path_query_carries_its_root_its_type_and_its_predicates() {
    let stated = predicate(EXISTS_OPERATOR, "");
    let built = path_query::build(&invocation(&[
        path_query::QUERY_PATHS,
        PATH_OPTION,
        ROOT,
        NODE_TYPE_OPTION,
        "cq:Page",
        PROPERTY_PREDICATE_OPTION,
        &stated,
    ]))
    .expect("every value is usable");
    let Command::QueryPaths(request) = built else { panic!("one variant") };
    assert_eq!(request.root_path.as_text(), ROOT);
    assert_eq!(request.primary_node_type.as_ref().map(|held| held.as_text()), Some("cq:Page"));
    assert_eq!(request.property_predicates.expect("one predicate").predicates().len(), 1);
    assert_eq!(request.result_window, None, "an unstated window is unstated");
}

#[test]
fn a_window_is_an_offset_and_a_limit_or_a_token_and_never_both() {
    let built = path_query::build(&invocation(&[
        path_query::QUERY_PATHS,
        PATH_OPTION,
        ROOT,
        OFFSET_OPTION,
        "0",
        LIMIT_OPTION,
        "10",
    ]))
    .expect("an offset and a limit are a window");
    let Command::QueryPaths(request) = built else { panic!("one variant") };
    assert!(request.result_window.is_some());

    let refusal = path_query::build(&invocation(&[
        path_query::QUERY_PATHS,
        PATH_OPTION,
        ROOT,
        OFFSET_OPTION,
        "0",
        LIMIT_OPTION,
        "10",
        CONTINUATION_TOKEN_OPTION,
        "opaque-token",
    ]))
    .expect_err("a token already carries the window it was issued under");
    assert_eq!(
        refusal,
        RequestRefusal::ValueUnusable { named: CONTINUATION_TOKEN_OPTION.to_owned() }
    );

    let half = path_query::build(&invocation(&[
        path_query::QUERY_PATHS,
        PATH_OPTION,
        ROOT,
        OFFSET_OPTION,
        "0",
    ]))
    .expect_err("half a window is not one");
    assert_eq!(half, RequestRefusal::ValueUnusable { named: OFFSET_OPTION.to_owned() });
}

#[test]
fn a_phrase_is_searched_for_exactly_as_it_was_typed() {
    let built = page_query::build(&invocation(&[
        page_query::FIND_CONTAINING_PHRASE,
        PATH_OPTION,
        ROOT,
        PHRASE_OPTION,
        PHRASE,
    ]))
    .expect("internal whitespace is part of the phrase");
    let Command::FindPagesContainingPhrase(request) = built else { panic!("one variant") };
    assert_eq!(
        request.phrase.as_text(),
        PHRASE,
        "two spaces in the middle are two spaces, because that is what was asked for"
    );
    let refusal = page_query::build(&invocation(&[
        page_query::FIND_CONTAINING_PHRASE,
        PATH_OPTION,
        ROOT,
        PHRASE_OPTION,
        UNTRIMMED_PHRASE,
    ]))
    .expect_err("the domain refuses it rather than trimming it");
    assert_eq!(refusal, RequestRefusal::ValueUnusable { named: PHRASE_OPTION.to_owned() });
}

#[test]
fn the_component_search_says_whether_every_named_type_is_required() {
    let any = page_query::build(&invocation(&[
        page_query::FIND_USING_COMPONENTS,
        PATH_OPTION,
        ROOT,
        RESOURCE_TYPES_OPTION,
        RESOURCE_TYPES,
    ]))
    .expect("two types are enough");
    let Command::FindPagesUsingComponents(request) = any else { panic!("one variant") };
    assert_eq!(request.match_mode, ComponentMatchMode::Any, "one of them, unless asked otherwise");

    let all = page_query::build(&invocation(&[
        page_query::FIND_USING_COMPONENTS,
        PATH_OPTION,
        ROOT,
        RESOURCE_TYPES_OPTION,
        RESOURCE_TYPES,
        MATCH_ALL_OPTION,
    ]))
    .expect("and every one of them when asked");
    let Command::FindPagesUsingComponents(request) = all else { panic!("one variant") };
    assert_eq!(request.match_mode, ComponentMatchMode::All);
}

#[test]
fn the_template_search_carries_the_template_it_was_given() {
    let built = page_query::build(&invocation(&[
        page_query::FIND_BY_TEMPLATE,
        PATH_OPTION,
        ROOT,
        TEMPLATE_OPTION,
        TEMPLATE,
    ]))
    .expect("a root and a template");
    let Command::FindPagesByTemplate(request) = built else { panic!("one variant") };
    assert_eq!(request.template_path.as_text(), TEMPLATE);
    assert_eq!(request.root_path.as_text(), ROOT);
}

#[test]
fn a_configuration_inspection_takes_one_identifier_and_offers_no_way_around_redaction() {
    let built = configuration::build(&invocation(&[
        INSPECT_CONFIGURATION,
        IDENTIFIER_OPTION,
        PERSISTENT_IDENTIFIER,
    ]))
    .expect("an identifier is enough");
    let Command::InspectOpenServiceGatewayInitiativeConfiguration(request) = built else {
        panic!("one variant")
    };
    assert_eq!(request.persistent_identifier.as_text(), PERSISTENT_IDENTIFIER);
    let source = std::fs::read_to_string("src/commands/configuration.rs").expect("it is readable");
    for around in ["--raw", "--unredacted", "--show-secret", "--include-values"] {
        assert!(
            !source.contains(around),
            "offering {around} would be offering a way to read a password"
        );
    }
}

#[test]
fn every_command_in_these_families_is_read_only_and_needs_no_caller_key() {
    let catalog = CommandCatalog::published();
    for leaf in [
        path_query::QUERY_PATHS,
        page_query::FIND_BY_TEMPLATE,
        page_query::FIND_CONTAINING_PHRASE,
        page_query::FIND_USING_COMPONENTS,
        INSPECT_CONFIGURATION,
    ] {
        let descriptor = catalog.find(leaf).expect("the registry publishes it");
        assert_eq!(descriptor.access, AccessClassification::Read, "{leaf} changes nothing");
        assert!(
            !descriptor.intrinsic_idempotency.requires_operation_key(),
            "{leaf} repeats harmlessly, so it needs no key"
        );
        assert_eq!(
            slingshot_domain::selected_command_contract_identity::SelectedCommandContractIdentity
                ::installed(leaf)
                .expect("it is installed")
                .command_semantic_contract_version,
            "1.0.0",
            "{leaf}"
        );
    }
    assert_eq!(PREDICATE_OPTION, PROPERTY_PREDICATE_OPTION, "one option, spelled once");
}
