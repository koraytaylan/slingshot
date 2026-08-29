//! Assertions for runtime namespace naming and exclusive daemon ownership.
//!
//! Every assertion works inside an injected temporary runtime root, so the real
//! user runtime and configuration directories are never touched.

use std::path::PathBuf;

use slingshot_daemon::ownership::{Acquisition, DaemonOwnership};
use slingshot_daemon::platform_runtime::current_user;
use slingshot_daemon::platform_runtime::locks::OwnerLock;
use slingshot_daemon::platform_runtime::readiness::{self, ReadinessRecord};
use slingshot_daemon::runtime_namespace::{NamespaceFailure, RuntimeNamespace};
use slingshot_local_protocol::foundation_contract::FoundationContract;

/// Profile the assertions name their first target with.
const FIRST_PROFILE: &str = "local";

/// Environment the assertions name their first target with.
const FIRST_ENVIRONMENT: &str = "author";

/// Creates an injected temporary runtime root that no other assertion shares.
fn temporary_runtime_root(name: &str) -> PathBuf {
    let root = std::env::temp_dir().join(format!("sls-own-{}-{name}", std::process::id()));
    std::fs::remove_dir_all(&root).ok();
    current_user::create_owner_only_directory(&root).expect("the runtime root is created");
    root
}

/// Names one runtime namespace inside an injected root.
fn namespace(root: &std::path::Path, profile: &str, environment: &str) -> RuntimeNamespace {
    RuntimeNamespace::name(&FoundationContract::embedded(), root, profile, environment)
        .expect("the target names a namespace")
}

/// Takes ownership of one namespace, expecting it to be free.
fn own(namespace: RuntimeNamespace) -> Box<DaemonOwnership> {
    match DaemonOwnership::acquire(&FoundationContract::embedded(), namespace) {
        Ok(Acquisition::Owned(owner)) => owner,
        other => panic!("the namespace must be free, but reported {other:?}"),
    }
}

#[test]
fn equal_targets_name_one_namespace_and_distinct_targets_never_collide() {
    let root = temporary_runtime_root("naming");
    let first = namespace(&root, FIRST_PROFILE, FIRST_ENVIRONMENT);
    let repeated = namespace(&root, FIRST_PROFILE, FIRST_ENVIRONMENT);
    assert_eq!(first.digest(), repeated.digest(), "one target names one namespace");
    assert_eq!(
        first.digest().len(),
        FoundationContract::embedded().namespace.digest_rendered_bytes as usize
    );
    assert!(
        first.digest().bytes().all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    );
    assert_eq!(first.display(), format!("{FIRST_PROFILE}/{FIRST_ENVIRONMENT}"));

    let other_environment = namespace(&root, FIRST_PROFILE, "publish");
    assert_ne!(first.digest(), other_environment.digest());

    let ambiguous_left = namespace(&root, "local-author", "one");
    let ambiguous_right = namespace(&root, "local", "author-one");
    assert_ne!(
        ambiguous_left.digest(),
        ambiguous_right.digest(),
        "a delimiter cannot make two targets name one namespace"
    );
    let joined_left = namespace(&root, "ab", "c");
    let joined_right = namespace(&root, "a", "bc");
    assert_ne!(joined_left.digest(), joined_right.digest());
    std::fs::remove_dir_all(&root).ok();
}

#[test]
fn a_target_name_is_refused_when_it_is_empty_too_long_or_unusable() {
    let contract = FoundationContract::embedded();
    let root = temporary_runtime_root("names");
    assert_eq!(
        RuntimeNamespace::name(&contract, &root, "", FIRST_ENVIRONMENT),
        Err(NamespaceFailure::Empty { name: "profile" })
    );
    assert_eq!(
        RuntimeNamespace::name(&contract, &root, FIRST_PROFILE, ""),
        Err(NamespaceFailure::Empty { name: "environment" })
    );
    let at_limit = "p".repeat(contract.names.profile_bytes as usize);
    assert!(RuntimeNamespace::name(&contract, &root, &at_limit, FIRST_ENVIRONMENT).is_ok());
    let beyond = format!("{at_limit}p");
    assert!(matches!(
        RuntimeNamespace::name(&contract, &root, &beyond, FIRST_ENVIRONMENT),
        Err(NamespaceFailure::TooLong { name: "profile", .. })
    ));
    assert!(matches!(
        RuntimeNamespace::name(&contract, &root, FIRST_PROFILE, "author/publish"),
        Err(NamespaceFailure::Unusable { name: "environment", .. })
    ));
    std::fs::remove_dir_all(&root).ok();
}

#[test]
fn one_owner_succeeds_and_every_contender_learns_who_owns_the_namespace() {
    let contract = FoundationContract::embedded();
    let root = temporary_runtime_root("owner");
    let mut owner = own(namespace(&root, FIRST_PROFILE, FIRST_ENVIRONMENT));
    owner.publish_readiness(&contract, "endpoint").expect("readiness publishes");

    let contender =
        DaemonOwnership::acquire(&contract, namespace(&root, FIRST_PROFILE, FIRST_ENVIRONMENT))
            .expect("the runtime state is readable");
    match contender {
        Acquisition::AlreadyOwned(evidence) => {
            assert_eq!(evidence.namespace_display, format!("{FIRST_PROFILE}/{FIRST_ENVIRONMENT}"));
            let published = evidence.readiness.expect("the live owner published readiness");
            assert_eq!(published.readiness_nonce, owner.readiness_nonce());
        }
        other => panic!("a contender must be refused, but reported {other:?}"),
    }

    let separate = own(namespace(&root, FIRST_PROFILE, "publish"));
    assert_ne!(separate.lock_path(), owner.lock_path(), "another target is another namespace");
    std::fs::remove_dir_all(&root).ok();
}

#[test]
fn only_the_exact_live_nonce_authorizes_a_stop() {
    let contract = FoundationContract::embedded();
    let root = temporary_runtime_root("nonce");
    let owner = own(namespace(&root, FIRST_PROFILE, FIRST_ENVIRONMENT));
    let live = owner.readiness_nonce().to_owned();
    assert_eq!(live.len(), contract.namespace.readiness_nonce_rendered_bytes as usize);
    assert!(owner.stop_is_authorized(&live));
    let first = if live.starts_with('a') { 'b' } else { 'a' };
    assert!(!owner.stop_is_authorized(&format!("{first}{}", &live[1..])));
    assert!(!owner.stop_is_authorized(""));
    assert!(!owner.stop_is_authorized(&live[..live.len() - 1]));

    drop(owner);
    let replacement = own(namespace(&root, FIRST_PROFILE, FIRST_ENVIRONMENT));
    assert_ne!(replacement.readiness_nonce(), live, "a replacement draws its own nonce");
    assert!(!replacement.stop_is_authorized(&live), "a stale nonce cannot stop a replacement");
    std::fs::remove_dir_all(&root).ok();
}

#[test]
fn a_departing_owner_removes_only_the_record_carrying_its_own_nonce() {
    let contract = FoundationContract::embedded();
    let root = temporary_runtime_root("records");
    let target = namespace(&root, FIRST_PROFILE, FIRST_ENVIRONMENT);
    let mut owner = own(target.clone());
    owner.publish_readiness(&contract, "first").expect("readiness publishes");
    let first_nonce = owner.readiness_nonce().to_owned();
    drop(owner);
    assert_eq!(
        readiness::read(&root, target.digest()).expect("the record is readable"),
        None,
        "a departing owner removes its own record"
    );

    let mut replacement = own(target.clone());
    replacement.publish_readiness(&contract, "second").expect("readiness publishes");
    let replacement_nonce = replacement.readiness_nonce().to_owned();
    let forged = ReadinessRecord {
        process_identifier: std::process::id(),
        readiness_nonce: first_nonce.clone(),
        endpoint_display: "forged".to_owned(),
    };
    assert!(
        !readiness::remove_matching(&root, target.digest(), &forged.readiness_nonce)
            .expect("the record is readable"),
        "a stale nonce cannot remove a live record even with the same process identifier"
    );
    assert_eq!(
        readiness::read(&root, target.digest())
            .expect("the record is readable")
            .map(|record| record.readiness_nonce),
        Some(replacement_nonce)
    );
    std::fs::remove_dir_all(&root).ok();
}

#[test]
fn a_record_without_a_held_lock_is_recovered_and_the_lock_file_persists() {
    let contract = FoundationContract::embedded();
    let root = temporary_runtime_root("recovery");
    let target = namespace(&root, FIRST_PROFILE, FIRST_ENVIRONMENT);
    let abandoned = ReadinessRecord {
        process_identifier: std::process::id(),
        readiness_nonce: "d".repeat(contract.namespace.readiness_nonce_rendered_bytes as usize),
        endpoint_display: "abandoned".to_owned(),
    };
    readiness::publish(&contract, &root, target.digest(), &abandoned)
        .expect("the record is written");
    let recovered = own(target.clone());
    assert_eq!(
        readiness::read(&root, target.digest()).expect("the record is readable"),
        None,
        "recovery removes the abandoned record under the owner lock"
    );
    let lock_path = OwnerLock::path_for(&root, target.digest());
    assert_eq!(recovered.lock_path(), lock_path);
    drop(recovered);
    assert!(lock_path.is_file(), "the lock file is persistent and is never unlinked");

    let forged = ReadinessRecord { endpoint_display: "forged".to_owned(), ..abandoned };
    let live = own(target.clone());
    readiness::publish(&contract, &root, target.digest(), &forged).expect("the record is written");
    match DaemonOwnership::acquire(&contract, target) {
        Ok(Acquisition::AlreadyOwned(_)) => {}
        other => panic!("a forged record cannot displace a live owner, but reported {other:?}"),
    }
    drop(live);
    std::fs::remove_dir_all(&root).ok();
}

#[test]
fn every_assertion_leaves_the_real_user_directories_untouched() {
    let root = temporary_runtime_root("isolation");
    assert!(current_user::is_owner_only(&root).expect("the root is inspectable"));
    assert!(root.starts_with(std::env::temp_dir()), "{}", root.display());
    let target = namespace(&root, FIRST_PROFILE, FIRST_ENVIRONMENT);
    assert!(target.runtime_root().starts_with(std::env::temp_dir()));
    assert!(OwnerLock::path_for(&root, target.digest()).starts_with(&root));
    assert!(readiness::record_path(&root, target.digest()).starts_with(&root));
    std::fs::remove_dir_all(&root).ok();
}
