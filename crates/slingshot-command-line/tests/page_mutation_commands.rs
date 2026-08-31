//! The two commands that change content, and what they refuse to be told.
//!
//! Both set one property themselves - a page's title, a component's resource
//! type - and both refuse a document that redefines it. Two parts of one
//! request disagreeing about the same value has no correct winner, so the
//! document is refused rather than one of them silently preferred.
//!
//! The caller key is checked before the property file is opened. A caller who
//! forgot it should not have their file read, and a read that happened anyway
//! would be a side effect of an invocation that was never going to be sent -
//! which the suite proves by pointing the option at a file that does not exist
//! and watching the key refusal arrive instead.
//!
//! Nothing here templates, interpolates, or expands. What the document holds is
//! what the request carries, because a surface that rewrote a value would be
//! writing content the caller never approved.

use std::io::Write;

use slingshot_command_line::commands::content::RequestRefusal;
use slingshot_command_line::commands::page_mutation::{
    ADD_COMPONENT, CONTENT_ROOT, CREATE_PAGE, build,
};
use slingshot_command_line::invocation::{
    COMPONENT_PARENT_OPTION, Invocation, NAME_OPTION, PATH_OPTION, PROPERTIES_OPTION,
    RESOURCE_TYPE_OPTION, Selection, TEMPLATE_OPTION, TITLE_OPTION, parse,
};
use slingshot_command_line::property_document::{PropertyDocumentRefusal, parse as read_document};
use slingshot_domain::command::add_component::{
    COMPONENT_RESOURCE_TYPE_PROPERTY, ContentRootMarker, PageContentParent,
};
use slingshot_domain::command::catalog::{AccessClassification, Command, CommandCatalog};
use slingshot_domain::command::create_page::PAGE_TITLE_PROPERTY;

/// Where a created page goes.
const PARENT: &str = "/content/site/en";

/// What it is called.
const PAGE_NAME: &str = "about-us";

/// What it is created from.
const TEMPLATE: &str = "/apps/site/templates/page";

/// What it records as its title.
const TITLE: &str = "About us";

/// The page a component is added to.
const PAGE: &str = "/content/site/en/home";

/// What the component is called.
const COMPONENT_NAME: &str = "hero";

/// What it records as its type.
const RESOURCE_TYPE: &str = "site/components/hero";

/// A descendant of the content resource.
const DESCENDANT: &str = "container/column";

/// The caller key these fixtures supply.
const KEY: &str = "operation-one";

/// A property document that sets nothing reserved.
const ORDINARY_DOCUMENT: &str = "{\"cq:tags\":{\"type\":\"string\",\"values\":[\"site:brand\"]},\
     \"published\":{\"type\":\"boolean\",\"value\":true}}";

/// A file that does not exist and will not be created.
const ABSENT_FILE: &str = "/nonexistent/slingshot-properties.json";

/// Returns the invocation `words` parse into.
fn invocation(words: &[&str]) -> Invocation {
    parse(&words.iter().map(|word| (*word).to_owned()).collect::<Vec<String>>())
        .expect("the words parse")
}

/// Returns a file holding `text`, kept alive by the returned handle.
fn document(text: &str) -> tempfile::NamedTempFile {
    let mut file = tempfile::NamedTempFile::new().expect("a temporary file");
    file.write_all(text.as_bytes()).expect("the document writes");
    file.flush().expect("the document lands");
    file
}

#[test]
fn a_page_creation_carries_every_value_it_was_given() {
    let built = build(&invocation(&[
        CREATE_PAGE,
        "--operation-key",
        KEY,
        PATH_OPTION,
        PARENT,
        NAME_OPTION,
        PAGE_NAME,
        TEMPLATE_OPTION,
        TEMPLATE,
        TITLE_OPTION,
        TITLE,
    ]))
    .expect("every value is usable");
    let Command::CreatePage(request) = built else { panic!("one variant") };
    assert_eq!(request.parent_path.as_text(), PARENT);
    assert_eq!(request.page_name.as_text(), PAGE_NAME);
    assert_eq!(request.template_path.as_text(), TEMPLATE);
    assert_eq!(request.title, TITLE, "the title is carried exactly, not normalized");
    assert_eq!(request.initial_properties, None, "and no document means no properties");
}

#[test]
fn a_component_addition_defaults_to_the_content_resource_itself() {
    let built = build(&invocation(&[
        ADD_COMPONENT,
        "--operation-key",
        KEY,
        PATH_OPTION,
        PAGE,
        NAME_OPTION,
        COMPONENT_NAME,
        RESOURCE_TYPE_OPTION,
        RESOURCE_TYPE,
    ]))
    .expect("every value is usable");
    let Command::AddComponent(request) = built else { panic!("one variant") };
    assert_eq!(request.page_path.as_text(), PAGE);
    assert_eq!(request.component_name.as_text(), COMPONENT_NAME);
    assert_eq!(request.resource_type.as_text(), RESOURCE_TYPE);
    assert_eq!(
        request.content_parent,
        PageContentParent::ContentRoot(ContentRootMarker::ContentRoot),
        "most components go at the top, and asking for that every time would be ceremony"
    );

    let nested = build(&invocation(&[
        ADD_COMPONENT,
        "--operation-key",
        KEY,
        PATH_OPTION,
        PAGE,
        NAME_OPTION,
        COMPONENT_NAME,
        RESOURCE_TYPE_OPTION,
        RESOURCE_TYPE,
        COMPONENT_PARENT_OPTION,
        DESCENDANT,
    ]))
    .expect("a descendant is a place too");
    let Command::AddComponent(request) = nested else { panic!("one variant") };
    assert_eq!(request.content_parent.descendant().map(|path| path.as_text()), Some(DESCENDANT));
    let root = build(&invocation(&[
        ADD_COMPONENT,
        "--operation-key",
        KEY,
        PATH_OPTION,
        PAGE,
        NAME_OPTION,
        COMPONENT_NAME,
        RESOURCE_TYPE_OPTION,
        RESOURCE_TYPE,
        COMPONENT_PARENT_OPTION,
        CONTENT_ROOT,
    ]))
    .expect("and the root may be named explicitly");
    let Command::AddComponent(request) = root else { panic!("one variant") };
    assert_eq!(request.content_parent.descendant(), None);
}

#[test]
fn a_property_document_is_carried_exactly_and_nothing_expands_it() {
    let file = document(ORDINARY_DOCUMENT);
    let built = build(&invocation(&[
        CREATE_PAGE,
        "--operation-key",
        KEY,
        PATH_OPTION,
        PARENT,
        NAME_OPTION,
        PAGE_NAME,
        TEMPLATE_OPTION,
        TEMPLATE,
        TITLE_OPTION,
        TITLE,
        PROPERTIES_OPTION,
        file.path().to_str().expect("a path"),
    ]))
    .expect("the document is usable");
    let Command::CreatePage(request) = built else { panic!("one variant") };
    let properties = request.initial_properties.expect("two properties");
    assert_eq!(properties.values().len(), 2);
    assert!(properties.values().contains_key("cq:tags"));
    assert!(properties.values().contains_key("published"));
}

#[test]
fn a_document_that_redefines_what_the_command_sets_is_refused() {
    let reserved =
        format!("{{\"{PAGE_TITLE_PROPERTY}\":{{\"type\":\"string\",\"value\":\"Another\"}}}}");
    let file = document(&reserved);
    let refusal = build(&invocation(&[
        CREATE_PAGE,
        "--operation-key",
        KEY,
        PATH_OPTION,
        PARENT,
        NAME_OPTION,
        PAGE_NAME,
        TEMPLATE_OPTION,
        TEMPLATE,
        TITLE_OPTION,
        TITLE,
        PROPERTIES_OPTION,
        file.path().to_str().expect("a path"),
    ]))
    .expect_err("two parts of one request disagreeing has no correct winner");
    assert_eq!(refusal, RequestRefusal::ValueUnusable { named: PROPERTIES_OPTION.to_owned() });

    let reserved = format!(
        "{{\"{COMPONENT_RESOURCE_TYPE_PROPERTY}\":{{\"type\":\"string\",\"value\":\"other\"}}}}"
    );
    let file = document(&reserved);
    let refusal = build(&invocation(&[
        ADD_COMPONENT,
        "--operation-key",
        KEY,
        PATH_OPTION,
        PAGE,
        NAME_OPTION,
        COMPONENT_NAME,
        RESOURCE_TYPE_OPTION,
        RESOURCE_TYPE,
        PROPERTIES_OPTION,
        file.path().to_str().expect("a path"),
    ]))
    .expect_err("the same rule for the component's own type");
    assert_eq!(refusal, RequestRefusal::ValueUnusable { named: PROPERTIES_OPTION.to_owned() });
}

#[test]
fn the_caller_key_is_checked_before_the_property_file_is_opened() {
    let keyless = Invocation {
        arguments: [
            (PATH_OPTION.to_owned(), PARENT.to_owned()),
            (PROPERTIES_OPTION.to_owned(), ABSENT_FILE.to_owned()),
        ]
        .into_iter()
        .collect(),
        detached: false,
        operation_key: None,
        output: None,
        selection: Selection::default(),
        verb: CREATE_PAGE.to_owned(),
    };
    assert_eq!(
        build(&keyless),
        Err(RequestRefusal::OperationKeyRequired { named: CREATE_PAGE.to_owned() }),
        "a caller who forgot the key should not have a file read on the way to being told"
    );
}

#[test]
fn a_document_that_is_not_a_set_of_typed_properties_is_refused_as_a_document() {
    assert_eq!(read_document("[]"), Err(PropertyDocumentRefusal::NotAnObject));
    assert_eq!(
        read_document("{\"a\":{\"type\":\"colour\",\"value\":\"red\"}}"),
        Err(PropertyDocumentRefusal::UnknownType { named: "colour".to_owned() })
    );
    assert_eq!(
        read_document("{\"a\":{\"type\":\"string\"}}"),
        Err(PropertyDocumentRefusal::MemberMissing { named: "value".to_owned() })
    );
    assert_eq!(
        read_document("{\"a\":{\"type\":\"string\",\"value\":\"x\",\"colour\":\"red\"}}"),
        Err(PropertyDocumentRefusal::SurplusMember { named: "colour".to_owned() })
    );
    assert_eq!(
        read_document("{\"a\":{\"type\":\"string\",\"values\":[]}}"),
        Err(PropertyDocumentRefusal::EmptyMultiple)
    );
    assert_eq!(
        read_document("{\"a\":{\"type\":\"string\",\"values\":[{\"b\":{\"c\":1}}]}}"),
        Err(PropertyDocumentRefusal::TooDeep),
        "a structure this vocabulary has no meaning for would have to be given one"
    );
}

#[test]
fn both_mutations_are_write_classified_and_need_a_caller_key() {
    let catalog = CommandCatalog::published();
    for leaf in [CREATE_PAGE, ADD_COMPONENT] {
        let descriptor = catalog.find(leaf).expect("the registry publishes it");
        assert_eq!(descriptor.access, AccessClassification::Write, "{leaf} changes content");
        assert!(descriptor.intrinsic_idempotency.requires_operation_key(), "{leaf} needs a key");
        assert!(!descriptor.failure_categories.is_empty(), "{leaf} has a closed failure set");
    }
    let ordering = catalog
        .find(ADD_COMPONENT)
        .expect("it is published")
        .failure_categories
        .contains(&"parent_not_orderable".to_owned());
    assert!(
        ordering,
        "add-component's ordering refusal is one of its registered categories, and renaming or \
         losing it would change what a caller is told when a parent cannot be ordered"
    );
    let source = std::fs::read_to_string("src/commands/page_mutation.rs").expect("it is readable");
    for category in &catalog.find(ADD_COMPONENT).expect("it is published").failure_categories {
        assert!(
            !source.contains(category.as_str()),
            "{category} is the registry's word, and naming it here would be a second spelling"
        );
    }
}
