//! What crosses the boundary when one process starts a daemon.
//!
//! A daemon outlives the client that started it, so its identity cannot be a
//! function of who happened to start it. These prove the whole of what a
//! starter may pass is two names - and that a configuration root supplied by a
//! test says out loud that it is one rather than looking like an ordinary path.

use slingshot_command_line::daemon_process::{
    ConfigurationRootSource, DaemonProcessArguments, DaemonProcessFailure,
};

/// The profile these fixtures start under.
const PROFILE: &str = "production";

/// The environment these fixtures start under.
const ENVIRONMENT: &str = "publish";

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
