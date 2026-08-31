//! Reaching readiness in one order, and serving a bounded number of clients.
//!
//! The ordering is the guarantee. A client that can see readiness is entitled
//! to assume ownership, configuration, installation, migration, and the
//! cross-partition audit all held, and that the endpoint can actually answer.
//! An implementation that bound one stage too early would still pass every test
//! that only checked the stages individually, which is why the order is a value
//! here rather than a comment.

use slingshot_daemon::local_server::{
    ConnectionCapacity, READINESS_STAGES, ReadinessProgress, ServerFailure,
};
use slingshot_local_protocol::foundation_contract::FoundationContract;

/// Connections this daemon serves at once, from the foundation contract.
const CONNECTION_CAPACITY: u32 = 64;

/// Returns a daemon that has completed every stage up to `stage`.
fn progressed_to(stage: &str) -> ReadinessProgress {
    let mut progress = ReadinessProgress::started();
    for reached in READINESS_STAGES {
        if *reached == stage {
            return progress;
        }
        progress = progress.complete(reached).expect("each stage in its own order");
    }
    panic!("{stage} is a stage this daemon has");
}

#[test]
fn the_order_runs_from_ownership_to_readiness_and_binds_in_the_middle() {
    assert_eq!(
        READINESS_STAGES.first(),
        Some(&"ownership"),
        "nothing precedes taking the lock, because everything else is done as the owner"
    );
    assert_eq!(
        READINESS_STAGES.last(),
        Some(&"readiness published"),
        "and publishing is last, because it is the claim that the rest already happened"
    );
    let audit = READINESS_STAGES
        .iter()
        .position(|stage| *stage == "cross-partition audit")
        .expect("an audit stage");
    let bind =
        READINESS_STAGES.iter().position(|stage| *stage == "listener bound").expect("a bind stage");
    assert!(audit < bind, "no client can reach an endpoint the audit has not cleared");
}

#[test]
fn a_stage_reached_out_of_order_is_refused_rather_than_tolerated() {
    let progress = ReadinessProgress::started();
    let skipped = progress.complete("listener bound");
    assert!(
        matches!(
            skipped,
            Err(ServerFailure::StageOutOfOrder { ref expected, .. }) if expected == "ownership"
        ),
        "binding before taking the lock names what was outstanding: {skipped:?}"
    );

    let mut ordered = ReadinessProgress::started();
    for stage in READINESS_STAGES {
        ordered = ordered.complete(stage).expect("each stage in its own order");
    }
    assert!(ordered.is_ready(), "and the whole order in order reaches readiness");
    assert!(
        ordered.complete("readiness published").is_err(),
        "while a ready daemon has nothing further to complete"
    );
}

#[test]
fn nothing_binds_or_publishes_before_the_stage_that_permits_it() {
    for stage in ["ownership", "selected environment snapshot", "installation comparison"] {
        let progress = progressed_to(stage);
        assert!(!progress.may_bind(), "at {stage} there is nothing to bind yet");
        assert!(!progress.may_publish(), "and nothing to publish");
    }
    let audited = progressed_to("listener bound");
    assert!(audited.may_bind(), "the audit has cleared, so the listener may bind");
    assert!(!audited.may_publish(), "but a bound listener has not answered anything yet");

    let answerable = progressed_to("readiness published");
    assert!(
        answerable.may_publish(),
        "only once hello can be answered, because a record naming an endpoint that \
         cannot answer lies for as long as the gap lasts"
    );
    assert!(!answerable.is_ready(), "and it is ready only once the record exists");
}

#[test]
fn the_capacity_is_the_contract_s_and_the_client_past_it_is_refused() {
    let contract = FoundationContract::embedded();
    let capacity = ConnectionCapacity::declared(&contract);
    assert_eq!(
        contract.server.connection_capacity, CONNECTION_CAPACITY,
        "the bound comes from the manifest rather than from this daemon"
    );

    let mut held = Vec::new();
    for _ in 0..CONNECTION_CAPACITY {
        held.push(capacity.claim().expect("a connection below the bound"));
    }
    assert_eq!(capacity.in_use(), CONNECTION_CAPACITY, "every one of them is being served");

    let refused = capacity.claim();
    assert!(
        matches!(refused, Err(ServerFailure::CapacityInUse { capacity: bound })
            if bound == CONNECTION_CAPACITY),
        "one further client is refused rather than queued: {refused:?}"
    );
}

#[test]
fn a_connection_that_ends_gives_its_capacity_back_however_it_ends() {
    let capacity = ConnectionCapacity::declared(&FoundationContract::embedded());
    let mut held = Vec::new();
    for _ in 0..CONNECTION_CAPACITY {
        held.push(capacity.claim().expect("a connection below the bound"));
    }
    assert!(capacity.claim().is_err(), "the daemon is full");

    drop(held.pop());
    assert_eq!(capacity.in_use(), CONNECTION_CAPACITY - 1, "one client left");
    let later = capacity.claim().expect("so a later client fits");
    assert_eq!(capacity.in_use(), CONNECTION_CAPACITY, "and the daemon is full again");

    drop(later);
    drop(held);
    assert_eq!(
        capacity.in_use(),
        0,
        "capacity that leaked on a close would be capacity nobody could get back"
    );
    capacity.claim().expect("and a client after everyone left still fits");
}
