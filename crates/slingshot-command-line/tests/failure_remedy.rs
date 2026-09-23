//! What a caller is told to do next when a command names a failure category.
//!
//! A category alone says what went wrong and not what to try, and a caller
//! with nothing to try gives up: a missing anchor is the start of a search, not
//! the end of one. These cases hold each remedy to the category it answers.

use slingshot_command_line::failure_remedy::{REMEDIES, remedy};
use slingshot_domain::command::classification::{DISCOVERY_FAILURES, ROOT_ANCHOR_FAILURES};

#[test]
fn a_missing_anchor_sends_the_caller_to_list_its_parent() {
    let told = remedy("root_not_found").expect("a missing anchor has a remedy");
    assert!(told.contains("parent"), "{told}");
    assert!(told.contains("list_child_pages"), "{told}");
    assert!(told.contains("list_child_nodes"), "{told}");
}

#[test]
fn an_exhausted_budget_sends_the_caller_to_a_narrower_search() {
    let told = remedy("discovery_budget_exceeded").expect("an exhausted budget has a remedy");
    assert!(told.contains("root_path"), "{told}");
    assert!(told.contains("primary_node_type"), "{told}");
}

#[test]
fn a_category_without_a_remedy_answers_nothing() {
    assert_eq!(remedy("remote work succeeded; result acquisition remains pending"), None);
    assert_eq!(remedy(""), None);
}

#[test]
fn every_remedy_answers_a_category_the_catalog_declares() {
    for (category, _) in REMEDIES {
        assert!(
            ROOT_ANCHOR_FAILURES.contains(category) || DISCOVERY_FAILURES.contains(category),
            "{category} is not a category any command reports"
        );
    }
}
