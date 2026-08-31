//! Handing work over once, and being able to come back to it.
//!
//! What comes back from a submission is an identifier, and the identifier is
//! what makes coming back possible. So the subject here is where it comes from:
//! a command that repeats harmlessly gets one generated exactly once, and a
//! command that does not gets one from the caller. The difference decides what
//! a rerun after a lost response means, and the suite counts the generator's
//! calls rather than reasoning about them.
//!
//! Nothing is prepared for a daemon this client may not send work to. The
//! handshake is checked first, so a mismatched daemon costs a caller a message
//! rather than a request - and the suite proves the ordering by leaving the
//! operation key out as well and watching the handshake refusal arrive.
//!
//! The registry inventory is exhaustive rather than sampled: every published
//! command falls on one side of the key rule, and the counts are asserted so a
//! command added without a classification cannot slip through as either.

use std::cell::Cell;

use slingshot_command_line::daemon_process::{DaemonExpectation, HandshakeRefusal};
use slingshot_command_line::invocation::{Invocation, Selection, parse};
use slingshot_command_line::operation_submission::{
    Admission, AfterAdmission, OperationKeySource, SubmissionRefusal, after_admission, key_source,
    prepare, require_installed_binding,
};
use slingshot_domain::command::catalog::CommandCatalog;
use slingshot_domain::command::query_paths::QueryPathsCommand;
use slingshot_domain::command::repository_path::RepositoryPath;

/// The partition this client acts in.
const TARGET: &str = "target-identity-digest-one";

/// Another partition, which it does not act in.
const OTHER_TARGET: &str = "target-identity-digest-two";

/// The environment revision it acts under.
const REVISION: &str = "environment-revision-one";

/// The caller key a mutation supplies.
const KEY: &str = "operation-one";

/// What the generator produces when it is called.
const GENERATED: &str = "generated-identifier";

/// A repository path a fixture command acts on.
const ROOT: &str = "/content/site";

/// How many published commands repeat harmlessly.
const IDEMPOTENT_COMMANDS: usize = 26;

/// How many need a caller key.
const NON_IDEMPOTENT_COMMANDS: usize = 38;

/// Returns what this client requires of a daemon.
fn expectation() -> DaemonExpectation {
    DaemonExpectation {
        author_target_identity_digest: TARGET.to_owned(),
        runtime_contract_digest: DaemonExpectation::embedded_runtime_digest(),
        selected_environment_revision: REVISION.to_owned(),
    }
}

/// Returns the handshake a compatible daemon gives.
fn compatible() -> (String, String, String) {
    (DaemonExpectation::embedded_runtime_digest(), TARGET.to_owned(), REVISION.to_owned())
}

/// Returns one command to submit.
fn command() -> slingshot_domain::command::catalog::Command {
    slingshot_domain::command::catalog::Command::QueryPaths(QueryPathsCommand {
        primary_node_type: None,
        property_predicates: None,
        result_window: None,
        root_path: RepositoryPath::parse(ROOT).expect("a repository path"),
    })
}

/// Returns the invocation `words` parse into.
fn invocation(words: &[&str]) -> Invocation {
    parse(&words.iter().map(|word| (*word).to_owned()).collect::<Vec<String>>())
        .expect("the words parse")
}

#[test]
fn every_published_command_falls_on_one_side_of_the_key_rule() {
    let catalog = CommandCatalog::published();
    let mut generated = 0;
    let mut supplied = 0;
    for descriptor in catalog.descriptors() {
        let leaf = descriptor.wire_name.as_str();
        if descriptor.intrinsic_idempotency.requires_operation_key() {
            supplied += 1;
            assert_eq!(
                key_source(leaf, None, || GENERATED.to_owned()),
                Err(SubmissionRefusal::OperationKeyRequired { named: leaf.to_owned() }),
                "{leaf}: a generated key would make every rerun a new operation"
            );
            assert_eq!(
                key_source(leaf, Some(KEY), || GENERATED.to_owned()),
                Ok(OperationKeySource::CallerSupplied(KEY.to_owned()))
            );
        } else {
            generated += 1;
            assert_eq!(
                key_source(leaf, None, || GENERATED.to_owned()),
                Ok(OperationKeySource::GeneratedOnce(GENERATED.to_owned())),
                "{leaf}: it repeats harmlessly, so one is made for it"
            );
            assert_eq!(
                key_source(leaf, Some(KEY), || GENERATED.to_owned()),
                Err(SubmissionRefusal::OperationKeyNotTaken { named: leaf.to_owned() }),
                "{leaf}: a key on it would name work nobody tracks"
            );
        }
    }
    assert_eq!(generated, IDEMPOTENT_COMMANDS, "the inventory is exhaustive rather than sampled");
    assert_eq!(supplied, NON_IDEMPOTENT_COMMANDS);
    assert_eq!(generated + supplied, catalog.descriptors().len());
}

#[test]
fn a_generator_is_called_once_and_only_where_it_is_wanted() {
    let calls = Cell::new(0);
    let generate = || {
        calls.set(calls.get() + 1);
        GENERATED.to_owned()
    };
    key_source("query_paths", None, generate).expect("it repeats harmlessly");
    assert_eq!(calls.get(), 1);

    let calls = Cell::new(0);
    let generate = || {
        calls.set(calls.get() + 1);
        GENERATED.to_owned()
    };
    key_source("create_page", None, generate).expect_err("that one needs a caller key");
    assert_eq!(
        calls.get(),
        0,
        "nothing is generated for a command that must not have a generated identifier"
    );
}

#[test]
fn a_supplied_key_survives_a_rerun_and_a_generated_one_does_not() {
    let supplied = OperationKeySource::CallerSupplied(KEY.to_owned());
    assert!(supplied.survives_a_rerun(), "which is what lets a rerun address the same operation");
    assert_eq!(supplied.identifier(), KEY);
    let generated = OperationKeySource::GeneratedOnce(GENERATED.to_owned());
    assert!(!generated.survives_a_rerun());
    assert_eq!(generated.identifier(), GENERATED);
}

#[test]
fn the_installed_binding_is_the_registry_the_build_came_from() {
    for descriptor in CommandCatalog::published().descriptors() {
        let identity = require_installed_binding(&descriptor.wire_name)
            .unwrap_or_else(|refusal| panic!("{}: {refusal}", descriptor.wire_name));
        assert_eq!(identity.command_wire_name, descriptor.wire_name);
        assert_eq!(identity.command_semantic_contract_version, "1.0.0");
        assert_eq!(
            identity.command_contract_limits_digest,
            descriptor.command_contract_limits_sha256
        );
        assert_eq!(identity.argument_schema_digest, descriptor.arguments_schema_sha256);
        assert_eq!(identity.result_schema_digest, descriptor.result_schema_sha256);
    }
    assert_eq!(
        require_installed_binding("teleport"),
        Err(SubmissionRefusal::NotInstalled { named: "teleport".to_owned() })
    );
}

#[test]
fn a_daemon_this_client_may_not_use_is_refused_before_anything_is_prepared() {
    let calls = Cell::new(0);
    let generate = || {
        calls.set(calls.get() + 1);
        GENERATED.to_owned()
    };
    let keyless = Invocation {
        arguments: std::collections::BTreeMap::new(),
        detached: false,
        operation_key: None,
        output: None,
        selection: Selection::default(),
        verb: "create_page".to_owned(),
    };
    let refusal = prepare(
        &keyless,
        command(),
        &expectation(),
        (&DaemonExpectation::embedded_runtime_digest(), OTHER_TARGET, REVISION),
        generate,
    )
    .expect_err("that daemon serves another target");
    assert_eq!(
        refusal,
        SubmissionRefusal::Handshake(HandshakeRefusal::TargetMismatch),
        "the handshake comes first, so a mismatched daemon costs a message rather than a request"
    );
    assert_eq!(calls.get(), 0, "and nothing was prepared on the way to finding out");
}

#[test]
fn a_prepared_submission_carries_the_three_values_the_daemon_checks() {
    let asked = invocation(&["query_paths"]);
    let prepared = prepare(
        &asked,
        command(),
        &expectation(),
        (&compatible().0, &compatible().1, &compatible().2),
        || GENERATED.to_owned(),
    )
    .expect("everything agrees");
    assert_eq!(prepared.author_target_identity_digest, TARGET);
    assert_eq!(prepared.selected_environment_revision, REVISION);
    assert_eq!(
        prepared.daemon_runtime_contract_digest,
        DaemonExpectation::embedded_runtime_digest()
    );
    assert_eq!(prepared.key_source, OperationKeySource::GeneratedOnce(GENERATED.to_owned()));
    assert_eq!(prepared.command, command());
}

#[test]
fn a_replay_is_told_apart_from_new_work_by_what_the_daemon_answered() {
    let accepted = Admission::Accepted { revision: 1 };
    let replayed = Admission::Replayed { revision: 1 };
    assert!(accepted.created_work());
    assert!(
        !replayed.created_work(),
        "which is the whole reason a caller-supplied key exists: without it a rerun would have \
         to assume the worse of the two"
    );
    assert_eq!(accepted.revision(), replayed.revision());
}

#[test]
fn what_happens_after_admission_comes_from_the_invocation_and_not_the_command() {
    assert_eq!(after_admission(&invocation(&["query_paths"])), AfterAdmission::Observe);
    assert_eq!(after_admission(&invocation(&["query_paths", "--detach"])), AfterAdmission::Detach);
    assert_eq!(
        after_admission(&invocation(&["create_page", "--operation-key", KEY, "--detach"])),
        AfterAdmission::Detach,
        "one decision for every command, so nothing detaches from one and not another"
    );
}
