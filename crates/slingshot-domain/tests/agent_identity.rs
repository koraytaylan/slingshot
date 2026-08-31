//! What a daemon and an operation are called at the agent, and why.
//!
//! Both identifiers are derived rather than allocated, so the properties worth
//! testing are the ones a derivation can get wrong: two things that should be
//! the same coming out different, and two that should differ coming out alike.
//! The partition fields are the ones most likely to be dropped by mistake, so
//! each of them is varied on its own.

use std::collections::BTreeSet;

use slingshot_domain::agent_identity::{
    AgentEventStoreGeneration, AgentIdentityFailure, AgentOperationIdentifier,
    DaemonSubscriptionIdentifier, IDENTIFIER_CHARACTERS, MAXIMUM_GENERATION,
};
use slingshot_domain::selected_command_contract_identity::{
    ContractIdentityFailure, SelectedCommandContractIdentity, SubmittedCommandDigest,
    installed_limits_digest, limits_are_installed,
};

/// Two-character pairs in a sixty-four-character hexadecimal value.
const DIGEST_PAIRS: usize = 32;

/// The installation these fixtures subscribe from.
const INSTALLATION: &str = "a1";

/// The target these fixtures serve.
const TARGET: &str = "1d";

/// Another target, differing only by the principal behind it.
const OTHER_TARGET: &str = "2d";

/// The environment revision these fixtures run at.
const REVISION: &str = "revision-1";

/// Another environment revision.
const OTHER_REVISION: &str = "revision-2";

/// The operation these fixtures name.
const OPERATION: &str = "operation-1";

/// The command these fixtures submit.
const COMMAND: &str = "query_paths";

/// Returns a sixty-four-character value made of one repeated pair.
fn digest(pair: &str) -> String {
    pair.repeat(DIGEST_PAIRS)
}

/// Returns the subscription one set of facts derives.
fn subscription(
    installation: &str,
    target: &str,
    revision: &str,
    generation: u64,
) -> DaemonSubscriptionIdentifier {
    DaemonSubscriptionIdentifier::derive(
        &digest(installation),
        &digest(target),
        revision,
        AgentEventStoreGeneration::of(generation),
    )
}

/// Returns the operation identifier one set of facts derives.
fn operation(
    target: &str,
    revision: &str,
    identifier: &str,
    generation: u64,
) -> AgentOperationIdentifier {
    AgentOperationIdentifier::derive(
        &digest(target),
        revision,
        identifier,
        AgentEventStoreGeneration::of(generation),
    )
}

#[test]
fn the_same_facts_derive_the_same_names_every_time() {
    assert_eq!(
        subscription(INSTALLATION, TARGET, REVISION, 1),
        subscription(INSTALLATION, TARGET, REVISION, 1),
        "two daemons resolving one target agree without coordinating"
    );
    assert_eq!(
        operation(TARGET, REVISION, OPERATION, 1),
        operation(TARGET, REVISION, OPERATION, 1),
        "and a daemon that restarts arrives at the names it had"
    );
    assert_eq!(
        subscription(INSTALLATION, TARGET, REVISION, 1).as_text().len(),
        IDENTIFIER_CHARACTERS
    );
}

#[test]
fn every_field_that_should_partition_does() {
    let held = [
        subscription(INSTALLATION, TARGET, REVISION, 1),
        subscription("b2", TARGET, REVISION, 1),
        subscription(INSTALLATION, OTHER_TARGET, REVISION, 1),
        subscription(INSTALLATION, TARGET, OTHER_REVISION, 1),
        subscription(INSTALLATION, TARGET, REVISION, 2),
    ];
    let distinct: BTreeSet<&DaemonSubscriptionIdentifier> = held.iter().collect();
    assert_eq!(
        distinct.len(),
        held.len(),
        "installation, target, revision, and generation each name a different subscription"
    );

    let operations = [
        operation(TARGET, REVISION, OPERATION, 1),
        operation(OTHER_TARGET, REVISION, OPERATION, 1),
        operation(TARGET, OTHER_REVISION, OPERATION, 1),
        operation(TARGET, REVISION, "operation-2", 1),
        operation(TARGET, REVISION, OPERATION, 2),
    ];
    let distinct: BTreeSet<&AgentOperationIdentifier> = operations.iter().collect();
    assert_eq!(
        distinct.len(),
        operations.len(),
        "and the same identifier under another security context is another operation"
    );
}

#[test]
fn a_target_digest_is_used_exactly_as_it_arrived() {
    let upper = digest(TARGET).to_uppercase();
    let lower = digest(TARGET);
    assert_ne!(
        AgentOperationIdentifier::derive(
            &upper,
            REVISION,
            OPERATION,
            AgentEventStoreGeneration::first()
        ),
        AgentOperationIdentifier::derive(
            &lower,
            REVISION,
            OPERATION,
            AgentEventStoreGeneration::first()
        ),
        "a digest hashed from its rendering rather than used as it arrived would collapse these, \
         and two identities for one thing eventually disagree"
    );
}

#[test]
fn a_name_that_is_not_a_derived_one_is_refused() {
    let held = subscription(INSTALLATION, TARGET, REVISION, 1);
    assert_eq!(
        DaemonSubscriptionIdentifier::parse(held.as_text()).expect("a legal identifier"),
        held
    );
    for wrong in [
        "",
        "not-a-digest",
        &"a".repeat(IDENTIFIER_CHARACTERS - 1),
        &"A".repeat(IDENTIFIER_CHARACTERS),
    ] {
        assert_eq!(
            DaemonSubscriptionIdentifier::parse(wrong),
            Err(AgentIdentityFailure::NotCanonical),
            "{wrong} is not a derived identifier"
        );
        assert_eq!(AgentOperationIdentifier::parse(wrong), Err(AgentIdentityFailure::NotCanonical));
    }
}

#[test]
fn generations_advance_and_then_run_out_rather_than_wrapping() {
    let first = AgentEventStoreGeneration::first();
    assert_eq!(first.value(), 1);
    assert_eq!(first.next().expect("a second generation").value(), 2);

    let last = AgentEventStoreGeneration::of(MAXIMUM_GENERATION);
    assert_eq!(
        last.next(),
        Err(AgentIdentityFailure::GenerationsExhausted),
        "wrapping would reuse identifiers from a store that no longer exists"
    );
}

#[test]
fn the_installed_identity_is_five_fields_and_all_of_them_count() {
    let identity =
        SelectedCommandContractIdentity::installed(COMMAND).expect("an installed command");
    assert_eq!(identity.command_wire_name, COMMAND);
    assert_eq!(identity.command_semantic_contract_version, "1.0.0");
    assert_eq!(identity.command_contract_limits_digest, installed_limits_digest());
    assert!(!identity.argument_schema_digest.is_empty(), "both role digests are present");
    assert!(!identity.result_schema_digest.is_empty());
    assert_ne!(
        identity.argument_schema_digest, identity.result_schema_digest,
        "and they describe different sides of the command"
    );
    assert!(identity.is_the_same_contract_as(&identity.clone()));

    for changed in [
        SelectedCommandContractIdentity {
            argument_schema_digest: digest("ff"),
            ..identity.clone()
        },
        SelectedCommandContractIdentity { result_schema_digest: digest("ff"), ..identity.clone() },
        SelectedCommandContractIdentity {
            command_contract_limits_digest: digest("ff"),
            ..identity.clone()
        },
        SelectedCommandContractIdentity {
            command_semantic_contract_version: "2.0.0".to_owned(),
            ..identity.clone()
        },
        SelectedCommandContractIdentity {
            command_wire_name: "create_page".to_owned(),
            ..identity.clone()
        },
    ] {
        assert!(
            !identity.is_the_same_contract_as(&changed),
            "each of the five fields can change what running the command does"
        );
    }
    assert!(limits_are_installed(&identity.command_contract_limits_digest));
}

#[test]
fn a_command_nobody_registered_has_no_identity() {
    assert_eq!(
        SelectedCommandContractIdentity::installed("a_command_nobody_added"),
        Err(ContractIdentityFailure::UnknownCommand {
            wire_name: "a_command_nobody_added".to_owned()
        }),
        "an agent that accepted a name it did not know would run the wrong contract"
    );
}

#[test]
fn a_submitted_digest_changes_with_everything_that_decides_the_submission() {
    let identity =
        SelectedCommandContractIdentity::installed(COMMAND).expect("an installed command");
    let contract = digest("c0");
    let transport = digest("70");
    let arguments = "{\"paths\":[\"/content\"]}";
    let base = SubmittedCommandDigest::derive(&identity, &contract, &transport, arguments);

    assert_eq!(
        base,
        SubmittedCommandDigest::derive(&identity, &contract, &transport, arguments),
        "the same submission derives the same digest"
    );
    let varied = [
        SubmittedCommandDigest::derive(&identity, &digest("c1"), &transport, arguments),
        SubmittedCommandDigest::derive(&identity, &contract, &digest("71"), arguments),
        SubmittedCommandDigest::derive(
            &identity,
            &contract,
            &transport,
            "{\"paths\":[\"/other\"]}",
        ),
        SubmittedCommandDigest::derive(
            &SelectedCommandContractIdentity {
                argument_schema_digest: digest("ff"),
                ..identity.clone()
            },
            &contract,
            &transport,
            arguments,
        ),
    ];
    let mut distinct: BTreeSet<&str> = varied.iter().map(SubmittedCommandDigest::as_text).collect();
    distinct.insert(base.as_text());
    assert_eq!(
        distinct.len(),
        varied.len() + 1,
        "the canonical contract binds as a peer of the shapes, so changing it alone is noticed"
    );
}
