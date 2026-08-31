//! What crosses the boundary when one process starts a daemon, and what it
//! refuses to start.
//!
//! A daemon outlives the client that started it, so its identity cannot be a
//! function of who happened to start it. The whole of what a starter may pass
//! is two names, and a configuration root supplied by a test says out loud that
//! it is one rather than looking like an ordinary path.
//!
//! The rest is about what a client concludes from what it finds. A daemon built
//! against another runtime contract, serving another target, or serving another
//! revision is not this client's daemon and gets no work - but it is exactly
//! the daemon a caller needs to be able to ask about and stop, so retained
//! control survives every mismatch. Refusing those too would leave somebody
//! with a running daemon and no way to replace it.
//!
//! Spawning is counted rather than reasoned about. A claim that status never
//! starts anything is checked by asking the scripted daemon how many times it
//! was asked to spawn.

use slingshot_command_line::daemon_process::{
    ConfigurationRootSource, DaemonExpectation, DaemonProcessArguments, DaemonProcessFailure,
    HandshakeRefusal, StartFailure, require_compatible,
};
use slingshot_domain::daemon_runtime_contract::DaemonRuntimeContract;
use slingshot_test_support::daemon_process::{
    Handshake, Lifecycle, ProbeAnswer, ScriptedDaemon, StopAnswer,
};

/// The profile these fixtures start under.
const PROFILE: &str = "production";

/// The environment these fixtures start under.
const ENVIRONMENT: &str = "publish";

/// The partition this client acts in.
const TARGET: &str = "target-identity-digest-one";

/// Another partition, which this client does not act in.
const OTHER_TARGET: &str = "target-identity-digest-two";

/// The environment revision this client acts under.
const REVISION: &str = "environment-revision-one";

/// Another revision, which this client does not act under.
const OTHER_REVISION: &str = "environment-revision-two";

/// A runtime contract digest that is not this build's.
const OTHER_DIGEST: &str = "0000000000000000000000000000000000000000000000000000000000000000";

/// The nonce a live daemon answers with.
const NONCE: &str = "nonce-one";

/// A nonce that named an owner which has since been replaced.
const STALE_NONCE: &str = "nonce-zero";

/// How many clients arrive at once in the convergence case.
const CONCURRENT_CLIENTS: u32 = 8;

/// How many times readiness is polled before a start is given up on.
const READINESS_ATTEMPTS: u32 = 4;

#[test]
fn a_daemon_is_started_with_two_names_and_nothing_else() {
    let arguments =
        DaemonProcessArguments::production(PROFILE, ENVIRONMENT).expect("both names supplied");
    assert_eq!(
        arguments.configuration_root,
        ConfigurationRootSource::AccountProfile,
        "a production start reads the account's own root"
    );
    assert!(!arguments.configuration_root.is_overridden(), "and nothing redirects it");
    assert_eq!(
        arguments.words(),
        vec!["--profile", PROFILE, "--environment", ENVIRONMENT],
        "the whole of what crosses the boundary is two names"
    );

    for word in arguments.words() {
        assert!(
            !word.contains("secret") && !word.contains("token") && !word.contains('/'),
            "no credential and no path reaches a daemon through its arguments: {word}"
        );
    }
    for (profile, environment) in [("", ENVIRONMENT), (PROFILE, "")] {
        assert!(
            matches!(
                DaemonProcessArguments::production(profile, environment),
                Err(DaemonProcessFailure::NameMissing { .. })
            ),
            "a daemon cannot be started without both names"
        );
    }
}

#[test]
fn a_supplied_configuration_root_says_that_it_is_one() {
    let directory = tempfile::tempdir().expect("a directory");
    let arguments = DaemonProcessArguments::with_root(
        PROFILE,
        ENVIRONMENT,
        ConfigurationRootSource::for_test(directory.path()),
    )
    .expect("both names supplied");

    assert!(
        arguments.configuration_root.is_overridden(),
        "an overridden root is visible as one rather than looking like an ordinary path"
    );
    assert_eq!(
        arguments.configuration_root.supplied_root(),
        Some(directory.path()),
        "and it is the root the test chose"
    );
    assert_eq!(
        arguments.words(),
        DaemonProcessArguments::production(PROFILE, ENVIRONMENT)
            .expect("a production start")
            .words(),
        "while the words a child receives are the same either way, because a root override \
         is a thing the test harness arranges rather than a thing a daemon is told"
    );
}

/// The runtime contract digest this build carries.
fn expected() -> DaemonExpectation {
    DaemonExpectation {
        author_target_identity_digest: TARGET.to_owned(),
        runtime_contract_digest: DaemonExpectation::embedded_runtime_digest(),
        selected_environment_revision: REVISION.to_owned(),
    }
}

/// Returns the handshake a compatible daemon gives.
fn compatible() -> Handshake {
    Handshake {
        author_target_identity_digest: TARGET.to_owned(),
        current_nonce: NONCE.to_owned(),
        runtime_contract_digest: DaemonExpectation::embedded_runtime_digest(),
        selected_environment_revision: REVISION.to_owned(),
    }
}

/// Requires one handshake against this build's expectation.
fn checked(handshake: &Handshake) -> Result<(), HandshakeRefusal> {
    require_compatible(
        &expected(),
        &handshake.runtime_contract_digest,
        &handshake.author_target_identity_digest,
        &handshake.selected_environment_revision,
    )
}

#[test]
fn a_compatible_daemon_is_one_this_client_may_send_work_to() {
    checked(&compatible()).expect("everything agrees");
    assert_eq!(
        DaemonExpectation::embedded_runtime_digest(),
        DaemonRuntimeContract::embedded_digest().as_text(),
        "the digest is recomputed from the embedded bytes rather than remembered"
    );
}

#[test]
fn every_mismatch_is_named_and_none_of_them_takes_away_retained_control() {
    let cases = [
        (
            Handshake { runtime_contract_digest: OTHER_DIGEST.to_owned(), ..compatible() },
            HandshakeRefusal::RuntimeContractMismatch,
        ),
        (
            Handshake { author_target_identity_digest: OTHER_TARGET.to_owned(), ..compatible() },
            HandshakeRefusal::TargetMismatch,
        ),
        (
            Handshake { selected_environment_revision: OTHER_REVISION.to_owned(), ..compatible() },
            HandshakeRefusal::RevisionMismatch,
        ),
    ];
    for (handshake, expected_refusal) in cases {
        let refusal = checked(&handshake).expect_err("this daemon is not this client's");
        assert_eq!(refusal, expected_refusal);
        assert!(
            refusal.permits_retained_control(),
            "refusing control too would leave a running daemon nobody can replace"
        );
        let rendered = format!("{refusal}");
        assert!(
            rendered.contains("stop it"),
            "and the refusal says what to do about it: {rendered}"
        );
    }
}

#[test]
fn a_contract_mismatch_is_refused_before_the_target_is_even_considered() {
    let both_wrong = Handshake {
        author_target_identity_digest: OTHER_TARGET.to_owned(),
        runtime_contract_digest: OTHER_DIGEST.to_owned(),
        ..compatible()
    };
    assert_eq!(
        checked(&both_wrong),
        Err(HandshakeRefusal::RuntimeContractMismatch),
        "nothing a daemon on another contract says about a target means the same thing"
    );
}

#[test]
fn asking_after_a_daemon_never_starts_one() {
    for lifecycle in
        [Lifecycle::Absent, Lifecycle::Unhealthy, Lifecycle::AlreadyServing(Box::new(compatible()))]
    {
        let daemon = ScriptedDaemon::following(lifecycle);
        daemon.probe();
        daemon.probe();
        assert_eq!(daemon.spawns(), 0, "status and ping look, and looking is not starting");
        assert_eq!(daemon.probes(), 2);
    }
}

#[test]
fn one_daemon_serves_however_many_clients_arrive_at_once() {
    let daemon = ScriptedDaemon::following(Lifecycle::StartsOnDemand(Box::new(compatible())));
    let mut started = 0;
    for _ in 0..CONCURRENT_CLIENTS {
        if matches!(daemon.probe_before_start(), ProbeAnswer::Absent) && started == 0 {
            daemon.spawn().expect("the child starts");
            started += 1;
        }
    }
    assert_eq!(daemon.spawns(), 1, "the clients converge on one process rather than each starting");
    assert!(matches!(daemon.probe(), ProbeAnswer::Serving(_)), "and it is serving afterwards");
}

#[test]
fn a_child_that_exits_is_reported_with_what_it_said() {
    let daemon = ScriptedDaemon::following(Lifecycle::ExitsEarly {
        detail: "the state root is not writable".to_owned(),
    });
    let detail = daemon.spawn().expect_err("the child exits");
    let failure = StartFailure::ChildExited { detail: detail.clone() };
    assert!(
        format!("{failure}").contains(&detail),
        "a caller needs what the child said, not that something went wrong"
    );
    assert!(matches!(daemon.probe(), ProbeAnswer::Absent), "and nothing is serving afterwards");
}

#[test]
fn a_daemon_that_never_becomes_ready_is_reported_with_how_long_it_was_given() {
    let daemon = ScriptedDaemon::following(Lifecycle::NeverReady);
    daemon.spawn().expect("the child starts");
    let mut attempts = 0;
    while attempts < READINESS_ATTEMPTS && matches!(daemon.probe(), ProbeAnswer::Absent) {
        attempts += 1;
    }
    assert_eq!(attempts, READINESS_ATTEMPTS);
    let failure = StartFailure::NeverReady { attempts };
    assert!(format!("{failure}").contains(&attempts.to_string()));
}

#[test]
fn a_stale_nonce_cannot_stop_the_daemon_that_replaced_the_one_it_names() {
    let daemon = ScriptedDaemon::following(Lifecycle::AlreadyServing(Box::new(compatible())));
    assert_eq!(daemon.stop(STALE_NONCE), StopAnswer::NonceStale);
    assert!(
        matches!(daemon.probe(), ProbeAnswer::Serving(_)),
        "the replacement is untouched by a nonce that named its predecessor"
    );
    assert_eq!(daemon.stop(NONCE), StopAnswer::Released);
    assert!(matches!(daemon.probe(), ProbeAnswer::Absent), "and the current nonce releases it");
    assert_eq!(daemon.stop(NONCE), StopAnswer::Absent, "and stopping again finds nothing");
    assert_eq!(daemon.spawns(), 0, "stopping never starts anything");
    assert_eq!(daemon.stops(), vec![STALE_NONCE, NONCE, NONCE]);
}
