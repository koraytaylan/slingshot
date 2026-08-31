//! One live owner per namespace, and safe recovery of what a dead one left.
//!
//! The property that matters is negative: a numeric process identifier
//! establishes nothing. The operating system reuses them, so a record whose
//! identifier matches a running program may be naming something unrelated.
//! Every test here that could be tempted to consult one instead reaches the
//! endpoint and compares the nonce, which is the only evidence that whoever
//! wrote a record is still there.

use slingshot_daemon::ownership::{Acquisition, DaemonOwnership, Liveness, classify_liveness};
use slingshot_daemon::platform_runtime::readiness::{self, PublishedIdentity, ReadinessRecord};
use slingshot_daemon::runtime_namespace::RuntimeNamespace;
use slingshot_local_protocol::foundation_contract::FoundationContract;

/// The profile every fixture here owns.
const PROFILE: &str = "production";

/// The environment every fixture here owns.
const ENVIRONMENT: &str = "publish";

/// A second environment, for proving two namespaces do not contend.
const OTHER_ENVIRONMENT: &str = "staging";

/// Two-character pairs in a sixty-four-character hexadecimal value.
const DIGEST_PAIRS: usize = 32;

/// Characters a sixty-four-character hexadecimal value has.
const DIGEST_CHARACTERS: usize = 64;

/// A process identifier a fixture reuses on purpose.
const REUSED_PROCESS_IDENTIFIER: u32 = 4242;

/// Returns a root inside `directory` for the daemon to create itself.
fn root(directory: &tempfile::TempDir) -> std::path::PathBuf {
    directory.path().join("runtime")
}

/// Returns the namespace one environment names inside `root`.
///
/// The runtime directory is created here, because a lock file needs a
/// directory to live in and creating one is the daemon's own job rather than
/// something a temporary directory can be trusted to have done at the right
/// protection.
fn namespace(root: &std::path::Path, environment: &str) -> RuntimeNamespace {
    let named = RuntimeNamespace::name(&FoundationContract::embedded(), root, PROFILE, environment)
        .expect("a legal pair");
    named.create_runtime_directory().expect("a runtime directory");
    named
}

/// Returns the identity a fixture daemon publishes.
fn identity(revision: &str) -> PublishedIdentity {
    PublishedIdentity {
        author_target_identity_digest: "1d".repeat(DIGEST_PAIRS),
        daemon_runtime_contract_digest: "c".repeat(DIGEST_CHARACTERS),
        retained_control_version: 1,
        selected_environment_revision: revision.to_owned(),
        supported_operation_versions: vec![1],
    }
}

/// Takes ownership, or panics saying who already has it.
fn owned(root: &std::path::Path, environment: &str) -> Box<DaemonOwnership> {
    let contract = FoundationContract::embedded();
    match DaemonOwnership::acquire(&contract, namespace(root, environment)).expect("an attempt") {
        Acquisition::Owned(owner) => owner,
        Acquisition::AlreadyOwned(evidence) => {
            panic!("{} is already owned: {evidence:?}", evidence.namespace_display)
        }
    }
}

#[test]
fn exactly_one_process_owns_a_namespace_and_the_rest_are_told_who_does() {
    let directory = tempfile::tempdir().expect("a directory");
    let root = root(&directory);
    let contract = FoundationContract::embedded();
    let mut owner = owned(&root, ENVIRONMENT);
    owner.identify(identity("revision-1"));
    owner.publish_readiness(&contract, "endpoint").expect("readiness publishes");

    let contender = DaemonOwnership::acquire(&contract, namespace(&root, ENVIRONMENT))
        .expect("a contending attempt");
    let Acquisition::AlreadyOwned(evidence) = contender else {
        panic!("a second process cannot own a namespace that is owned");
    };
    assert_eq!(evidence.namespace_display, "production/publish");
    let published = evidence.readiness.expect("the live owner's record");
    assert_eq!(published.readiness_nonce, owner.readiness_nonce(), "which is this owner's");
    let served = published.identity.expect("an identified owner");
    assert_eq!(served, identity("revision-1"), "and says what it serves, without a principal");
}

#[test]
fn a_readiness_record_is_a_claim_and_a_matching_live_nonce_is_the_evidence() {
    let record = ReadinessRecord {
        endpoint_display: "endpoint".to_owned(),
        identity: Some(identity("revision-1")),
        process_identifier: REUSED_PROCESS_IDENTIFIER,
        readiness_nonce: "a".repeat(DIGEST_CHARACTERS),
    };

    assert_eq!(
        classify_liveness(&record, Some(&record.readiness_nonce)),
        Liveness::Live,
        "the endpoint answered with the nonce the record claims"
    );
    assert_eq!(
        classify_liveness(&record, Some(&"b".repeat(DIGEST_CHARACTERS))),
        Liveness::AnotherInstance,
        "someone answered, and it is not who this record says"
    );
    assert_eq!(
        classify_liveness(&record, None),
        Liveness::Departed,
        "nothing answered, so this is what a departed owner left"
    );
}

#[test]
fn a_reused_process_identifier_is_never_ownership_liveness_or_cleanup_proof() {
    let directory = tempfile::tempdir().expect("a directory");
    let root = root(&directory);
    let contract = FoundationContract::embedded();
    let mut owner = owned(&root, ENVIRONMENT);
    owner.identify(identity("revision-1"));
    owner.publish_readiness(&contract, "endpoint").expect("readiness publishes");
    let live_nonce = owner.readiness_nonce().to_owned();

    let impostor = ReadinessRecord {
        endpoint_display: "elsewhere".to_owned(),
        identity: Some(identity("revision-1")),
        process_identifier: std::process::id(),
        readiness_nonce: "f".repeat(live_nonce.len()),
    };
    assert_eq!(
        classify_liveness(&impostor, Some(&live_nonce)),
        Liveness::AnotherInstance,
        "a record wearing this very process identifier is still not this owner"
    );
    assert!(
        !owner.stop_is_authorized(&impostor.readiness_nonce),
        "and cannot stop the owner that is actually here"
    );
    assert!(owner.stop_is_authorized(&live_nonce), "which only its own nonce can");

    let removed = readiness::remove_matching(
        &root,
        namespace(&root, ENVIRONMENT).digest(),
        &impostor.readiness_nonce,
    )
    .expect("a removal attempt");
    assert!(!removed, "nor remove the live record");
    let still_there = readiness::read(&root, namespace(&root, ENVIRONMENT).digest())
        .expect("a read")
        .expect("the live record");
    assert_eq!(still_there.readiness_nonce, live_nonce, "which is still exactly what it was");
}

#[test]
fn a_successor_recovers_what_a_departed_owner_left() {
    let directory = tempfile::tempdir().expect("a directory");
    let root = root(&directory);
    let contract = FoundationContract::embedded();
    let digest = namespace(&root, ENVIRONMENT).digest().to_owned();

    // A crash leaves the record and releases the lock, so the record is
    // written here without one being held. Proving that across real processes
    // needs a spawned binary, which is the start harness's business rather than
    // this module's.
    let departed_nonce = "3".repeat(DIGEST_CHARACTERS);
    readiness::publish(
        &contract,
        &root,
        &digest,
        &ReadinessRecord {
            endpoint_display: "endpoint".to_owned(),
            identity: Some(identity("revision-1")),
            process_identifier: REUSED_PROCESS_IDENTIFIER,
            readiness_nonce: departed_nonce.clone(),
        },
    )
    .expect("what a departed owner left");

    let mut successor = owned(&root, ENVIRONMENT);
    assert_ne!(
        successor.readiness_nonce(),
        departed_nonce,
        "a successor draws its own nonce rather than inheriting one"
    );
    assert!(
        readiness::read(&root, &digest).expect("a read").is_none(),
        "and the stale record was recovered, which only the lock holder may do"
    );
    successor.identify(identity("revision-2"));
    successor.publish_readiness(&contract, "endpoint").expect("readiness publishes");
    assert!(
        !successor.stop_is_authorized(&departed_nonce),
        "and the departed owner's nonce cannot stop its replacement"
    );
}

#[test]
fn a_departing_owner_removes_only_its_own_record() {
    let directory = tempfile::tempdir().expect("a directory");
    let root = root(&directory);
    let contract = FoundationContract::embedded();
    let digest = namespace(&root, ENVIRONMENT).digest().to_owned();

    let mut owner = owned(&root, ENVIRONMENT);
    owner.identify(identity("revision-1"));
    owner.publish_readiness(&contract, "endpoint").expect("readiness publishes");
    let replacement_nonce = "9".repeat(owner.readiness_nonce().len());
    readiness::publish(
        &contract,
        &root,
        &digest,
        &ReadinessRecord {
            endpoint_display: "replacement".to_owned(),
            identity: Some(identity("revision-2")),
            process_identifier: REUSED_PROCESS_IDENTIFIER,
            readiness_nonce: replacement_nonce.clone(),
        },
    )
    .expect("a replacement publishes over it");

    let withdrawn = owner.withdraw_readiness().expect("a withdrawal attempt");
    assert!(!withdrawn, "the departing owner's nonce no longer names the record that is there");
    let held = readiness::read(&root, &digest).expect("a read").expect("the replacement's record");
    assert_eq!(
        held.readiness_nonce, replacement_nonce,
        "so the replacement's readiness survived its predecessor leaving"
    );
}

#[test]
fn two_namespaces_are_owned_at_once_without_contending() {
    let directory = tempfile::tempdir().expect("a directory");
    let root = root(&directory);
    let contract = FoundationContract::embedded();
    let mut one = owned(&root, ENVIRONMENT);
    let mut other = owned(&root, OTHER_ENVIRONMENT);

    one.identify(identity("revision-1"));
    other.identify(identity("revision-1"));
    one.publish_readiness(&contract, "one").expect("readiness publishes");
    other.publish_readiness(&contract, "other").expect("readiness publishes");

    assert_ne!(one.readiness_nonce(), other.readiness_nonce(), "each draws its own nonce");
    assert_ne!(one.lock_path(), other.lock_path(), "and holds its own lock");
    assert_eq!(
        readiness::read(&root, namespace(&root, ENVIRONMENT).digest())
            .expect("a read")
            .expect("a record")
            .endpoint_display,
        "one"
    );
    assert_eq!(
        readiness::read(&root, namespace(&root, OTHER_ENVIRONMENT).digest())
            .expect("a read")
            .expect("a record")
            .endpoint_display,
        "other"
    );
}

#[test]
fn an_owner_serving_no_target_yet_says_so_rather_than_publishing_empty_fields() {
    let directory = tempfile::tempdir().expect("a directory");
    let root = root(&directory);
    let contract = FoundationContract::embedded();
    let mut owner = owned(&root, ENVIRONMENT);
    assert!(owner.identity().is_none(), "an owner has taken a lock, not selected a target");
    owner.publish_readiness(&contract, "endpoint").expect("readiness publishes");

    let published = readiness::read(&root, namespace(&root, ENVIRONMENT).digest())
        .expect("a read")
        .expect("a record");
    assert_eq!(
        published.identity, None,
        "a daemon serving only retained control has genuinely not selected one"
    );
    assert_eq!(published.readiness_nonce, owner.readiness_nonce(), "and is still reachable");
}
