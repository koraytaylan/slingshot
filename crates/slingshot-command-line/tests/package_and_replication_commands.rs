//! Two commands that change something, and what their words map to.
//!
//! Both are write-classified and neither is intrinsically idempotent, so both
//! demand a caller key before anything external is reached. That is read from
//! the registry rather than decided here, and the suite checks every
//! registry-owned fact the commands lean on: the semantic version, the limits
//! digest, and the schema digests. A stale digest bound into this surface would
//! mean a request built against a contract the agent no longer has.
//!
//! Filter order is the property worth spending effort on. A package filter is
//! read in order, so reordering the words would change which subtrees survive -
//! quietly, and only for the inputs where it mattered. The suite pins the order
//! through the request rather than trusting that nothing sorts it.

use slingshot_command_line::commands::content::RequestRefusal;
use slingshot_command_line::commands::package::{
    DOWNLOAD_PACKAGE, EXCLUDE_OPTION, INCLUDE_OPTION, PACKAGE_NAME_OPTION, ROOTS_OPTION,
};
use slingshot_command_line::commands::{package, replication};
use slingshot_command_line::invocation::{Invocation, PATH_OPTION, parse};
use slingshot_domain::command::catalog::{
    AccessClassification, Command, CommandCatalog, IntrinsicIdempotencyClassification,
};
use slingshot_domain::selected_command_contract_identity::SelectedCommandContractIdentity;

/// A repository path these fixtures act on.
const PATH: &str = "/content/site/en";

/// Another path, so a list has more than one member.
///
/// After the first in ascending order, because the domain requires roots to be
/// strictly ascending and this surface passes what it is given rather than
/// sorting it: a caller who lists roots out of order gets the domain's refusal
/// rather than a package they did not describe.
const OTHER_PATH: &str = "/content/site/fr";

/// The stem a produced package is named from.
const PACKAGE_NAME: &str = "site-export";

/// The caller key these fixtures supply.
const KEY: &str = "operation-one";

/// Two filters whose order decides which subtrees survive.
const FIRST_FILTER: &str = "/content/site/en";

/// The second of them.
const SECOND_FILTER: &str = "/content/site/en/private";

/// Returns the invocation `words` parse into.
fn invocation(words: &[&str]) -> Invocation {
    parse(&words.iter().map(|word| (*word).to_owned()).collect::<Vec<String>>())
        .expect("the words parse")
}

#[test]
fn a_replication_carries_its_path_and_says_whether_it_reaches_below_it() {
    let shallow = replication::build(&invocation(&[
        replication::REPLICATE_CONTENT,
        "--operation-key",
        KEY,
        PATH_OPTION,
        PATH,
    ]))
    .expect("a path is enough");
    let Command::ReplicateContent(request) = shallow else { panic!("one command, one variant") };
    assert_eq!(request.path.as_text(), PATH);
    assert!(
        !request.recursive,
        "the difference between one node and a subtree is asked for rather than assumed"
    );

    let deep = replication::build(&invocation(&[
        replication::REPLICATE_CONTENT,
        "--operation-key",
        KEY,
        PATH_OPTION,
        PATH,
        replication::RECURSIVE_OPTION,
    ]))
    .expect("a path and a flag");
    let Command::ReplicateContent(request) = deep else { panic!("one command, one variant") };
    assert!(request.recursive);
}

#[test]
fn a_package_carries_its_roots_its_name_and_both_filter_lists_in_order() {
    let built = package::build(&invocation(&[
        DOWNLOAD_PACKAGE,
        "--operation-key",
        KEY,
        PACKAGE_NAME_OPTION,
        PACKAGE_NAME,
        ROOTS_OPTION,
        &format!("{PATH},{OTHER_PATH}"),
        INCLUDE_OPTION,
        &format!("{FIRST_FILTER},{SECOND_FILTER}"),
        EXCLUDE_OPTION,
        SECOND_FILTER,
    ]))
    .expect("every value is usable");
    let Command::DownloadContentPackage(request) = built else { panic!("one variant") };
    assert_eq!(request.package_name.as_text(), PACKAGE_NAME);
    let roots: Vec<&str> = request.roots.paths().iter().map(|path| path.as_text()).collect();
    assert_eq!(roots.len(), 2, "both roots survive");
    let inclusion = request.inclusion_filters.expect("two were given");
    let stated: Vec<&str> =
        inclusion.expressions().iter().map(|expression| expression.as_text()).collect();
    assert_eq!(
        stated,
        vec![FIRST_FILTER, SECOND_FILTER],
        "a package filter is read in order, so reordering the words would change the package"
    );
    assert!(request.exclusion_filters.is_some(), "and the exclusions are their own list");
}

#[test]
fn a_package_without_filters_states_neither_rather_than_stating_empty_ones() {
    let built = package::build(&invocation(&[
        DOWNLOAD_PACKAGE,
        "--operation-key",
        KEY,
        PACKAGE_NAME_OPTION,
        PACKAGE_NAME,
        ROOTS_OPTION,
        PATH,
    ]))
    .expect("roots and a name are enough");
    let Command::DownloadContentPackage(request) = built else { panic!("one variant") };
    assert_eq!(request.inclusion_filters, None);
    assert_eq!(
        request.exclusion_filters, None,
        "an empty list and no list are different requests, and only one of them was made"
    );
}

#[test]
fn both_commands_need_a_caller_key_before_anything_external_is_reached() {
    for leaf in [replication::REPLICATE_CONTENT, DOWNLOAD_PACKAGE] {
        let refusal = parse(&[leaf.to_owned(), PATH_OPTION.to_owned(), PATH.to_owned()])
            .expect_err("no key was supplied");
        assert!(
            format!("{refusal}").contains("repeat"),
            "{leaf}: the refusal says why a key is what makes a retry the same request"
        );
    }
}

#[test]
fn a_value_neither_command_accepts_is_refused_with_the_option_that_carried_it() {
    let refusal = package::build(&invocation(&[
        DOWNLOAD_PACKAGE,
        "--operation-key",
        KEY,
        PACKAGE_NAME_OPTION,
        PACKAGE_NAME,
        ROOTS_OPTION,
        "content/no-leading-slash",
    ]))
    .expect_err("that is not a repository path");
    assert_eq!(refusal, RequestRefusal::ValueUnusable { named: ROOTS_OPTION.to_owned() });
    let refusal =
        replication::build(&invocation(&[replication::REPLICATE_CONTENT, "--operation-key", KEY]))
            .expect_err("no path");
    assert_eq!(refusal, RequestRefusal::OptionMissing { named: PATH_OPTION.to_owned() });
}

#[test]
fn each_family_builds_its_own_command_and_refuses_the_others() {
    let asked =
        invocation(&[replication::REPLICATE_CONTENT, "--operation-key", KEY, PATH_OPTION, PATH]);
    assert_eq!(
        package::build(&asked),
        Err(RequestRefusal::AnotherCommand { named: replication::REPLICATE_CONTENT.to_owned() })
    );
}

#[test]
fn both_commands_bind_the_exact_registry_identity_this_build_installed() {
    let catalog = CommandCatalog::published();
    for leaf in [replication::REPLICATE_CONTENT, DOWNLOAD_PACKAGE] {
        let descriptor = catalog.find(leaf).expect("the registry publishes it");
        assert_eq!(
            descriptor.intrinsic_idempotency,
            IntrinsicIdempotencyClassification::NotIntrinsicallyIdempotent,
            "{leaf}: so a repeat is a second effect unless a key says otherwise"
        );
        assert!(
            matches!(descriptor.access, AccessClassification::Read | AccessClassification::Write),
            "{leaf}: whichever it is, the key requirement does not come from the access label"
        );
        let identity = SelectedCommandContractIdentity::installed(leaf).expect("it is installed");
        assert_eq!(identity.command_semantic_contract_version, "1.0.0", "{leaf}");
        assert_eq!(
            identity.command_contract_limits_digest, descriptor.command_contract_limits_sha256,
            "{leaf}: a stale limits digest would build against a contract the agent has not got"
        );
        assert_eq!(identity.argument_schema_digest, descriptor.arguments_schema_sha256, "{leaf}");
        assert_eq!(identity.result_schema_digest, descriptor.result_schema_sha256, "{leaf}");
    }
}

#[test]
fn the_package_command_declares_the_artifact_it_produces() {
    let catalog = CommandCatalog::published();
    let descriptor = catalog.find(DOWNLOAD_PACKAGE).expect("the registry publishes it");
    assert_eq!(descriptor.remote_artifact_slots.len(), 1, "one package, one slot");
    let slot = &descriptor.remote_artifact_slots[0];
    assert_eq!(slot.media_type.as_text(), "application/zip");
    assert!(
        !descriptor.failure_categories.is_empty(),
        "and its closed failure set is the registry's, with no alias defined here"
    );
    let source = std::fs::read_to_string("src/commands/package.rs").expect("it is readable");
    for category in &descriptor.failure_categories {
        assert!(
            !source.contains(category.as_str()),
            "{category} is the registry's word, and naming it here would be a second spelling"
        );
    }
}
