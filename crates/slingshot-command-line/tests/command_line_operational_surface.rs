//! Assertions for the leaves and options the operational commands are typed as.
//!
//! The assembler asks every family in turn and stops at the first refusal that
//! is not "another command", so a family that reads an option before it answers
//! about the verb hides every family declared after it. That is not a
//! hypothetical: it happened, and one family reading `--path` first made ten
//! commands unreachable while every one of their own tests still passed. The
//! first assertion here asks every family about every command and counts the
//! claims, because asking the assembler can only ever show that somebody
//! answered - never that two families would have.

use slingshot_command_line::commands::content::RequestRefusal;
use slingshot_command_line::commands::{
    asset_lifecycle, asset_query, authorizable, configuration, content, content_fragment,
    experience_fragment, package, page_lifecycle, page_mutation, page_query, path_query,
    platform_configuration, replication, replication_queue, resource_mapping, sling_job, workflow,
};
use slingshot_command_line::daemon_request::build_command;
use slingshot_command_line::invocation::{
    EVERY_OPTION, Invocation, OutputForm, Selection, leaves_taking,
};
use slingshot_domain::command::catalog::{Command, CommandCatalog};

/// A caller-supplied key, for the leaves that require one.
const KEY: &str = "an-operation-key";

/// Returns one invocation of `leaf`, carrying a key and nothing else.
fn invocation(leaf: &str) -> Invocation {
    Invocation {
        arguments: std::collections::BTreeMap::new(),
        detached: false,
        operation_key: Some(KEY.to_owned()),
        output: Some(OutputForm::Machine),
        selection: Selection {
            environment: Some("author".to_owned()),
            profile: Some("local".to_owned()),
        },
        verb: leaf.to_owned(),
    }
}

/// One family's builder.
type CommandBuilder = fn(&Invocation) -> Result<Command, RequestRefusal>;

/// Every family that turns an invocation into a typed command.
///
/// Written here rather than read from the assembler on purpose: the assembler
/// stops at the first family that answers, so asking it can only ever show that
/// somebody answered. This list asks all of them and counts.
const EVERY_FAMILY: &[(&str, CommandBuilder)] = &[
    ("asset_lifecycle", asset_lifecycle::build),
    ("asset_query", asset_query::build),
    ("authorizable", authorizable::build),
    ("configuration", configuration::build),
    ("content", content::build),
    ("content_fragment", content_fragment::build),
    ("experience_fragment", experience_fragment::build),
    ("package", package::build),
    ("page_lifecycle", page_lifecycle::build),
    ("page_mutation", page_mutation::build),
    ("page_query", page_query::build),
    ("path_query", path_query::build),
    ("platform_configuration", platform_configuration::build),
    ("replication", replication::build),
    ("replication_queue", replication_queue::build),
    ("resource_mapping", resource_mapping::build),
    ("sling_job", sling_job::build),
    ("workflow", workflow::build),
];

#[test]
fn exactly_one_family_claims_each_command() {
    let mut disputed = Vec::new();
    for descriptor in CommandCatalog::published().descriptors() {
        let leaf = descriptor.wire_name.as_str();
        let claiming: Vec<&str> = EVERY_FAMILY
            .iter()
            .filter(|(_, build)| {
                !matches!(build(&invocation(leaf)), Err(RequestRefusal::AnotherCommand { .. }))
            })
            .map(|(named, _)| *named)
            .collect();
        if claiming.len() != 1 {
            disputed.push(format!("{leaf} is claimed by {claiming:?}"));
        }
    }
    assert_eq!(
        disputed,
        Vec::<String>::new(),
        "a family that answers about a command it does not build hides every family after it"
    );
}

#[test]
fn every_registry_command_is_reachable_as_a_leaf() {
    for descriptor in CommandCatalog::published().descriptors() {
        let leaf = descriptor.wire_name.as_str();
        let answered = build_command(&invocation(leaf));
        assert!(
            !matches!(answered, Err(RequestRefusal::AnotherCommand { .. })),
            "{leaf} reaches no builder"
        );
    }
}

#[test]
fn a_command_that_needs_nothing_is_built_from_nothing() {
    // Six listings take no required argument at all. A refusal here would mean
    // an option had been made required that the contract does not require.
    for leaf in [
        "find_open_service_gateway_initiative_configurations",
        "list_open_service_gateway_initiative_bundles",
        "list_open_service_gateway_initiative_components",
        "list_replication_agents",
        "list_resource_mappings",
        "list_sling_job_queues",
        "list_workflow_models",
    ] {
        let built = build_command(&invocation(leaf));
        assert!(built.is_ok(), "{leaf} was refused with nothing wrong: {built:?}");
        assert_eq!(built.expect("a command").wire_name(), leaf);
    }
}

#[test]
fn an_operation_key_is_required_exactly_where_the_registry_says() {
    for descriptor in CommandCatalog::published().descriptors() {
        let leaf = descriptor.wire_name.as_str();
        let mut asked = invocation(leaf);
        asked.operation_key = None;
        let answered = build_command(&asked);
        let refused_for_a_key =
            matches!(answered, Err(RequestRefusal::OperationKeyRequired { .. }));
        assert_eq!(
            refused_for_a_key,
            descriptor.intrinsic_idempotency.requires_operation_key(),
            "{leaf}: the key rule and the registry disagree"
        );
    }
}

#[test]
fn a_request_that_contradicts_itself_is_refused_before_it_leaves() {
    let mut asked = invocation("move_page");
    asked.arguments.insert("--path".to_owned(), "/content/site".to_owned());
    asked.arguments.insert("--destination-path".to_owned(), "/content/site/en".to_owned());
    assert!(
        matches!(build_command(&asked), Err(RequestRefusal::RequestUnusable { .. })),
        "a move into its own subtree was built"
    );

    let mut asked = invocation("add_group_member");
    asked.arguments.insert("--group".to_owned(), "authors".to_owned());
    asked.arguments.insert("--member".to_owned(), "authors".to_owned());
    assert!(
        matches!(build_command(&asked), Err(RequestRefusal::RequestUnusable { .. })),
        "a group was allowed to contain itself"
    );

    let mut asked = invocation("update_page");
    asked.arguments.insert("--path".to_owned(), "/content/site/en".to_owned());
    assert!(
        matches!(build_command(&asked), Err(RequestRefusal::RequestUnusable { .. })),
        "a page update that changes nothing was built"
    );
}

#[test]
fn a_valueless_option_is_absorbed_as_a_decision_rather_than_eating_the_next_word() {
    let mut asked = invocation("move_page");
    asked.arguments.insert("--path".to_owned(), "/content/site/en".to_owned());
    asked.arguments.insert("--destination-path".to_owned(), "/content/archive/en".to_owned());
    asked.arguments.insert("--adjust-references".to_owned(), String::new());
    let built = build_command(&asked).expect("a legal move");
    let written = serde_json::to_value(&built).expect("a command serializes");
    assert_eq!(
        written["adjust_references"],
        serde_json::Value::Bool(true),
        "a flag given on the command line did not reach the request"
    );
}

#[test]
fn a_sibling_beside_a_last_placement_is_refused_rather_than_dropped() {
    let mut asked = invocation("reorder_component");
    asked
        .arguments
        .insert("--path".to_owned(), "/content/site/en/jcr:content/root/text".to_owned());
    asked.arguments.insert("--placement".to_owned(), "last".to_owned());
    asked.arguments.insert("--sibling".to_owned(), "image".to_owned());
    assert!(
        matches!(build_command(&asked), Err(RequestRefusal::ValueUnusable { named }) if named == "--sibling"),
        "a sibling beside a last placement was dropped"
    );
}

#[test]
fn a_value_the_domain_refuses_is_refused_here_with_its_option_named() {
    for (leaf, option, value, companions) in [
        ("delete_page", "--reference-policy", "maybe", vec![("--path", "/content/site/en")]),
        ("start_workflow", "--model", " leading", vec![("--payload-path", "/content/site")]),
        ("find_sling_jobs", "--states", "confused", Vec::new()),
        ("create_user", "--authorizable", "with/slash", Vec::new()),
        ("resolve_resource_path", "--request-address", "relative", Vec::new()),
        ("inspect_replication_queue", "--agent", " spaced", Vec::new()),
    ] {
        let mut asked = invocation(leaf);
        asked.arguments.insert(option.to_owned(), value.to_owned());
        for (named, carried) in companions {
            asked.arguments.insert(named.to_owned(), carried.to_owned());
        }
        assert!(
            matches!(
                build_command(&asked),
                Err(RequestRefusal::ValueUnusable { ref named }) if named == option
            ),
            "{leaf}: {option}={value} was accepted or refused for something else"
        );
    }
}

#[test]
fn every_option_this_surface_knows_belongs_to_at_least_one_leaf() {
    for option in EVERY_OPTION {
        assert!(!leaves_taking(option).is_empty(), "{option} belongs to no leaf");
    }
}
