//! Which service answers what, and what a daemon refuses on its own.
//!
//! The claim worth testing is the one about incompatibility. A client that
//! cannot speak this daemon's operation protocol still has to be able to find
//! out what it is talking to and ask it to stop - otherwise it is stuck with a
//! daemon it can neither use nor replace.

use slingshot_daemon::request_dispatch::{Dispatch, DispatchPolicy, DispatchRefusal, RequestKind};

/// The operation protocol version this daemon serves.
const SERVED_VERSION: u64 = 1;

/// A version no daemon in these fixtures serves.
const UNSERVED_VERSION: u64 = 9;

/// Every kind of request a client can send.
const EVERY_KIND: &[RequestKind] = &[
    RequestKind::RetainedControl,
    RequestKind::Execute,
    RequestKind::Query,
    RequestKind::Wait,
    RequestKind::ArtifactRead,
    RequestKind::Maintenance,
    RequestKind::ResumeRecovery,
];

/// Returns a policy serving this daemon's version.
fn serving() -> DispatchPolicy {
    DispatchPolicy { stopping: false, supported_operation_versions: vec![SERVED_VERSION] }
}

#[test]
fn every_kind_of_request_reaches_its_service_when_everything_matches() {
    let policy = serving();
    for kind in EVERY_KIND {
        assert_eq!(
            policy.dispatch(*kind, SERVED_VERSION),
            Dispatch::Serve(*kind),
            "{kind:?} is answered by the service that answers it"
        );
    }
}

#[test]
fn an_incompatible_client_keeps_control_and_loses_everything_else() {
    let policy = serving();
    assert_eq!(
        policy.dispatch(RequestKind::RetainedControl, UNSERVED_VERSION),
        Dispatch::Serve(RequestKind::RetainedControl),
        "a client that cannot run operations can still find out what it is talking to"
    );

    for kind in EVERY_KIND.iter().filter(|kind| !kind.survives_incompatibility()) {
        let refused = policy.dispatch(*kind, UNSERVED_VERSION);
        assert!(
            matches!(
                refused,
                Dispatch::Refuse(DispatchRefusal::IncompatibleOperationVersion { asked, .. })
                    if asked == UNSERVED_VERSION
            ),
            "{kind:?} speaks the operation protocol, so an incompatible client cannot: {refused:?}"
        );
    }
}

#[test]
fn a_stopping_daemon_takes_no_new_work_and_keeps_answering_about_the_old() {
    let policy = DispatchPolicy { stopping: true, ..serving() };
    assert_eq!(
        policy.dispatch(RequestKind::Execute, SERVED_VERSION),
        Dispatch::Refuse(DispatchRefusal::Stopping),
        "nothing arrives that will not be finished"
    );
    for kind in EVERY_KIND.iter().filter(|kind| !kind.creates_work()) {
        assert_eq!(
            policy.dispatch(*kind, SERVED_VERSION),
            Dispatch::Serve(*kind),
            "{kind:?} is about work that already exists, so it carries on"
        );
    }
}

#[test]
fn incompatibility_is_decided_before_stopping() {
    let policy = DispatchPolicy { stopping: true, ..serving() };
    let refused = policy.dispatch(RequestKind::Execute, UNSERVED_VERSION);
    assert!(
        matches!(refused, Dispatch::Refuse(DispatchRefusal::IncompatibleOperationVersion { .. })),
        "an incompatible client is told something true about this daemon rather than sent away \
         to reconnect to a successor it also could not talk to: {refused:?}"
    );
}
