//! Deciding whether to join a daemon, start one, or refuse to touch it.
//!
//! The refusal is the case worth caring about. A daemon owning the namespace a
//! client wants but serving a different target is somebody else's daemon
//! running work this client cannot see. Joining it would send commands to the
//! wrong remote system; stopping it would end that work. The only correct
//! action is to say which namespace to look at and let a person decide, and
//! these prove that is what happens.

use slingshot_command_line::daemon_connection::{
    ExpectedTarget, MismatchReason, ObservedOwner, OwnerDisposition, classify_owner,
};
use slingshot_local_protocol::foundation_contract::FoundationContract;

/// Two-character pairs in a sixty-four-character hexadecimal value.
const DIGEST_PAIRS: usize = 32;

/// Characters a sixty-four-character hexadecimal value has.
const DIGEST_CHARACTERS: usize = 64;

/// The operation protocol version this client speaks.
const OPERATION_VERSION: u64 = 1;

/// A version no daemon in these fixtures serves.
const UNSERVED_VERSION: u64 = 9;

/// Milliseconds a start may take in total, from the foundation contract.
const START_TOTAL_MILLISECONDS: u64 = 30_000;

/// Milliseconds the longest retry delay lasts, from the foundation contract.
const RETRY_MAXIMUM_MILLISECONDS: u64 = 100;

/// Returns what this client expects to be serving its target.
fn expected() -> ExpectedTarget {
    ExpectedTarget {
        author_target_identity_digest: "1d".repeat(DIGEST_PAIRS),
        operation_protocol_version: OPERATION_VERSION,
        selected_environment_revision: "revision-1".to_owned(),
    }
}

/// Returns an owner serving exactly what this client expects.
fn matching_owner() -> ObservedOwner {
    ObservedOwner {
        author_target_identity_digest: expected().author_target_identity_digest,
        namespace_display: "production/publish".to_owned(),
        readiness_nonce: "a".repeat(DIGEST_CHARACTERS),
        selected_environment_revision: expected().selected_environment_revision,
        supported_operation_versions: vec![OPERATION_VERSION],
    }
}

#[test]
fn an_owner_serving_this_target_is_joined_without_starting_anything() {
    let owner = matching_owner();
    assert_eq!(
        classify_owner(&expected(), Some(&owner)),
        OwnerDisposition::Matching { readiness_nonce: owner.readiness_nonce.clone() },
        "a compatible owner is reached rather than replaced"
    );
}

#[test]
fn nobody_owning_the_target_is_the_only_case_that_starts_one() {
    assert_eq!(
        classify_owner(&expected(), None),
        OwnerDisposition::Absent,
        "absence is what a client contends over; every other case already has an answer"
    );
}

#[test]
fn an_owner_serving_another_target_is_never_joined_stopped_or_signalled() {
    let elsewhere = ObservedOwner {
        author_target_identity_digest: "2d".repeat(DIGEST_PAIRS),
        ..matching_owner()
    };
    let disposition = classify_owner(&expected(), Some(&elsewhere));
    let OwnerDisposition::Mismatched { guidance, reason } = disposition else {
        panic!("another target is a refusal: {disposition:?}");
    };
    assert_eq!(reason, MismatchReason::Target);
    assert!(
        guidance.contains("production/publish"),
        "the guidance names the namespace to look at: {guidance}"
    );
    assert!(
        guidance.contains("stop it explicitly"),
        "and says a person decides, not this client: {guidance}"
    );
    assert!(
        guidance.contains("will not stop it for you"),
        "and says so in as many words: {guidance}"
    );
    assert!(
        !guidance.contains("kill") && !guidance.contains("signal"),
        "and never suggests reaching for a process identifier: {guidance}"
    );
}

#[test]
fn an_owner_at_another_revision_is_refused_for_that_reason_and_not_another() {
    let older = ObservedOwner {
        selected_environment_revision: "revision-0".to_owned(),
        ..matching_owner()
    };
    let disposition = classify_owner(&expected(), Some(&older));
    let OwnerDisposition::Mismatched { guidance, reason } = disposition else {
        panic!("another revision is a refusal: {disposition:?}");
    };
    assert_eq!(
        reason,
        MismatchReason::Revision,
        "the same target under another security context is still not this client's daemon"
    );
    assert!(
        guidance.contains("another environment revision"),
        "and the guidance says which of the two differs: {guidance}"
    );
}

#[test]
fn an_owner_this_client_cannot_speak_to_is_still_the_right_owner() {
    let older =
        ObservedOwner { supported_operation_versions: vec![UNSERVED_VERSION], ..matching_owner() };
    let disposition = classify_owner(&expected(), Some(&older));
    assert_eq!(
        disposition,
        OwnerDisposition::OperationIncompatible {
            supported_operation_versions: vec![UNSERVED_VERSION]
        },
        "an operation version it cannot serve is not a reason to start a second daemon"
    );
    assert!(
        !matches!(disposition, OwnerDisposition::Absent),
        "and certainly not a reason to treat the namespace as unowned"
    );
}

#[test]
fn the_start_deadlines_come_from_the_manifest_rather_than_from_the_connector() {
    let contract = FoundationContract::embedded();
    assert_eq!(
        contract.startup.explicit_start_total(),
        std::time::Duration::from_millis(START_TOTAL_MILLISECONDS),
        "how long a start may take in total"
    );
    assert_eq!(
        contract.startup.start_retry_maximum_delay(),
        std::time::Duration::from_millis(RETRY_MAXIMUM_MILLISECONDS),
        "and how long the longest wait between attempts lasts"
    );
    assert!(
        contract.startup.start_retry_maximum_delay() < contract.startup.explicit_start_total(),
        "so a client gets many attempts inside the total rather than one long one"
    );
}
