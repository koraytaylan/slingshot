//! What another process on this machine can do to a daemon, and what it cannot.
//!
//! Being able to connect proves only that the socket exists. Everything that
//! changes anything quotes the nonce the live daemon published, and an attacker
//! who could read that could already read the runtime state - at which point
//! the endpoint is not what was protecting anything.
//!
//! The property worth having is therefore narrow and strong: an attacker
//! without the live nonce can waste a daemon's time and nothing else.

use slingshot_local_protocol::foundation_contract::FoundationContract;
use slingshot_local_protocol::ping::{nonce_is_well_formed, stop_is_authorized};
use slingshot_test_support::local_endpoint_attacker::{
    AttackOutcome, EVERY_LOCAL_ATTACK, LocalAttack,
};

/// The nonce a live daemon published.
const LIVE_NONCE: &str = "3c3c3c3c3c3c3c3c3c3c3c3c3c3c3c3c3c3c3c3c3c3c3c3c3c3c3c3c3c3c3c3c";

/// A nonce a previous instance published.
const STALE_NONCE: &str = "5a5a5a5a5a5a5a5a5a5a5a5a5a5a5a5a5a5a5a5a5a5a5a5a5a5a5a5a5a5a5a5a";

#[test]
fn only_the_attack_that_quotes_the_live_nonce_changes_anything() {
    for attack in EVERY_LOCAL_ATTACK {
        assert_eq!(
            attack.changes_durable_state(),
            *attack == LocalAttack::StopWithLiveNonce,
            "{attack:?} disagrees about whether it changes anything"
        );
        assert_eq!(attack.leaves_the_daemon_serving(), !attack.changes_durable_state());
    }
    let authorized = EVERY_LOCAL_ATTACK.iter().filter(|held| held.changes_durable_state()).count();
    assert_eq!(authorized, 1, "one way in, and it is the one the mechanism is for");
}

#[test]
fn probing_is_answered_because_answering_it_gives_nothing_away() {
    assert_eq!(LocalAttack::Probe.outcome(), AttackOutcome::Answered);
    assert!(LocalAttack::Probe.leaves_the_daemon_serving());
}

#[test]
fn a_stop_without_a_nonce_or_with_a_stale_one_is_refused() {
    for attack in [LocalAttack::StopWithoutNonce, LocalAttack::StopWithStaleNonce] {
        assert_eq!(attack.outcome(), AttackOutcome::Refused, "{attack:?}");
        assert!(
            attack.leaves_the_daemon_serving(),
            "{attack:?} stopped a daemon it could not name"
        );
    }
    assert!(!stop_is_authorized(LIVE_NONCE, STALE_NONCE), "a stale nonce authorizes nothing");
    assert!(!stop_is_authorized(LIVE_NONCE, ""), "and neither does none");
    assert!(stop_is_authorized(LIVE_NONCE, LIVE_NONCE), "and the live one authorizes exactly one");
}

#[test]
fn a_nonce_that_is_not_shaped_like_one_is_not_one() {
    let contract = FoundationContract::embedded();
    assert!(nonce_is_well_formed(&contract, LIVE_NONCE), "the scenario nonce is a nonce");
    for held in ["", "nope", &LIVE_NONCE.to_uppercase(), &format!("{LIVE_NONCE}0")] {
        assert!(!nonce_is_well_formed(&contract, held), "{held:?} was accepted as a nonce");
    }
}

#[test]
fn a_connection_from_off_this_machine_reaches_nothing() {
    assert_eq!(
        LocalAttack::ConnectRemotely.outcome(),
        AttackOutcome::Unreachable,
        "the endpoint is local, and a remote client is refused by the endpoint's own kind"
    );
}

#[test]
fn holding_every_connection_wastes_time_and_changes_nothing() {
    let attack = LocalAttack::ExhaustConnections;
    assert_eq!(attack.outcome(), AttackOutcome::Refused);
    assert!(
        attack.leaves_the_daemon_serving(),
        "an attacker who cannot produce the live nonce can waste a daemon's time and no more"
    );
    let contract = FoundationContract::embedded();
    assert_ne!(
        contract.server.connection_capacity, 0,
        "a daemon serving no connections serves nobody"
    );
    assert!(
        contract.server.initial_control_frame_milliseconds > 0,
        "a connection that could be held open silently forever is a connection nobody gets back"
    );
}

#[test]
fn every_attack_is_named_once_and_reaches_one_of_the_four_outcomes() {
    let named: std::collections::BTreeSet<String> =
        EVERY_LOCAL_ATTACK.iter().map(|attack| format!("{attack:?}")).collect();
    assert_eq!(named.len(), EVERY_LOCAL_ATTACK.len());
    let outcomes: std::collections::BTreeSet<String> =
        EVERY_LOCAL_ATTACK.iter().map(|attack| format!("{:?}", attack.outcome())).collect();
    assert_eq!(outcomes.len(), EVERY_OUTCOME_COUNT, "every outcome is reachable");
}

/// How many outcomes an attack can reach.
const EVERY_OUTCOME_COUNT: usize = 4;
