//! An author the tests can drive, and the properties it exists to make testable.
//!
//! Three of them matter more than the rest. Behaviour comes from a validated
//! script, so a case that is hard to reach is reached on demand rather than by
//! timing. Credentials are recorded as presence and acceptance, never as
//! values, so the harness proving secrets do not leak is not itself leaking
//! them. And one logical command has one effect however many physical records
//! Sling makes of it.

use slingshot_test_support::fake_author::authority::{
    AuthorityRefusal, ContinuationKeyAuthority, DeploymentProfile, EVERY_PROFILE,
    MAXIMUM_KEY_BYTES, MAXIMUM_KEY_RING_BYTES,
};
use slingshot_test_support::fake_author::outbox::{
    ExecutionState, LogicalOperation, OutboxRefusal,
};
use slingshot_test_support::fake_author::recording::CredentialKind;
use slingshot_test_support::fake_author::script::{
    AUTHOR_ROUTES, PUBLISHER_PREFIXES, Script, ScriptFailure, ScriptedExchange, ScriptedResponse,
};
use slingshot_test_support::fake_author::server::{
    ALREADY_ACCEPTED_STATUS, Answer, CredentialPolicy, FakeAuthor, IncomingRequest, OK_STATUS,
};

/// The route these fixtures submit to.
const SUBMIT: &str = "/bin/slingshot/agent/submit";

/// A route a publisher would serve and this author never does.
const PUBLISHER: &str = "/content/dam/something";

/// Two-character pairs in a sixty-four-character hexadecimal value.
const DIGEST_PAIRS: usize = 32;

/// The lease a first caller holds.
const FIRST_LEASE: u64 = 2;

/// Physical records one duplicated delivery makes.
const DUPLICATED_RECORDS: usize = 3;

/// Characters a sixty-four-character hexadecimal value has.
const DIGEST_CHARACTERS: usize = 64;

/// Records the disagreement fixture actually has.
const RECORDED_JOBS: usize = 2;

/// Returns the digest these fixtures serve.
fn target() -> String {
    "1d".repeat(DIGEST_PAIRS)
}

/// Returns one script answering `route` once.
fn script(route: &str, response: ScriptedResponse) -> Script {
    Script::of(vec![ScriptedExchange { response, route: route.to_owned() }])
        .expect("a legal script")
}

/// Returns one request presenting `authorization`.
fn request(route: &str, authorization: Option<&str>) -> IncomingRequest {
    IncomingRequest {
        authorization: authorization.map(str::to_owned),
        author_target_identity_digest: Some(target()),
        route: route.to_owned(),
        selected_environment_revision: Some("revision-1".to_owned()),
    }
}

#[test]
fn a_script_naming_something_the_contract_cannot_do_fails_while_it_is_written() {
    assert_eq!(Script::of(Vec::new()), Err(ScriptFailure::Empty));
    for prefix in PUBLISHER_PREFIXES {
        let refused = Script::of(vec![ScriptedExchange {
            response: ScriptedResponse::CloseWithoutAnswering,
            route: format!("{prefix}/something"),
        }]);
        assert!(
            matches!(refused, Err(ScriptFailure::PublisherRoute { .. })),
            "a suite that could script a publisher route would eventually write one by accident"
        );
    }
    let unknown = Script::of(vec![ScriptedExchange {
        response: ScriptedResponse::CloseWithoutAnswering,
        route: "/bin/something/else".to_owned(),
    }]);
    assert!(matches!(unknown, Err(ScriptFailure::UnknownRoute { .. })));

    for route in AUTHOR_ROUTES {
        Script::of(vec![ScriptedExchange {
            response: ScriptedResponse::CloseWithoutAnswering,
            route: (*route).to_owned(),
        }])
        .expect("every route this author serves is scriptable");
    }
}

#[test]
fn a_credential_is_recorded_as_presence_and_acceptance_and_never_as_a_value() {
    let author = FakeAuthor::following(
        script(SUBMIT, ScriptedResponse::Respond { body: b"{}".to_vec(), status: OK_STATUS }),
        CredentialPolicy::Bearer,
    );
    let recording = author.recording();

    assert_eq!(
        author.answer(&request(SUBMIT, Some("Basic dXNlcjpwYXNzd29yZA=="))),
        Answer::Unauthenticated,
        "a Basic header where Bearer is required does not serve"
    );
    assert_eq!(author.answer(&request(SUBMIT, None)), Answer::Unauthenticated);
    let served = author.answer(&request(SUBMIT, Some("Bearer ya29.a-real-looking-token")));
    assert!(matches!(served, Answer::Responded { status, .. } if status == OK_STATUS));

    let requests = recording.requests();
    assert_eq!(requests.len(), 3, "every attempt was recorded, accepted or not");
    assert_eq!(requests[0].credential_kind, CredentialKind::Basic);
    assert!(!requests[0].credential_accepted);
    assert_eq!(requests[1].credential_kind, CredentialKind::Absent);
    assert_eq!(requests[2].credential_kind, CredentialKind::Bearer);
    assert!(requests[2].credential_accepted);
    assert!(
        recording.holds_no_credential_values(),
        "the harness proving secrets do not leak must not be the thing leaking them"
    );
}

#[test]
fn a_publisher_route_is_refused_and_recorded_before_anything_else_is_considered() {
    let author = FakeAuthor::following(
        script(SUBMIT, ScriptedResponse::Respond { body: b"{}".to_vec(), status: OK_STATUS }),
        CredentialPolicy::Bearer,
    );
    let recording = author.recording();

    assert_eq!(
        author.answer(&request(PUBLISHER, Some("Bearer good"))),
        Answer::RouteRefused,
        "a credential does not make a publisher route into an author route"
    );
    assert_eq!(recording.refused_routes(), vec![PUBLISHER.to_owned()]);
    assert!(
        recording.requests().is_empty(),
        "and it is a finding about the client rather than a thing the author served"
    );
}

#[test]
fn an_exhausted_script_says_so_rather_than_inventing_an_answer() {
    let author = FakeAuthor::following(
        script(
            SUBMIT,
            ScriptedResponse::AlreadyAccepted {
                agent_operation_identifier: "a".repeat(DIGEST_CHARACTERS),
            },
        ),
        CredentialPolicy::Basic,
    );
    let first = author.answer(&request(SUBMIT, Some("Basic dXNlcjpwYXNz")));
    assert!(
        matches!(first, Answer::Responded { status, .. } if status == ALREADY_ACCEPTED_STATUS),
        "the scripted answer"
    );
    assert!(author.script_is_exhausted());
    assert_eq!(
        author.answer(&request(SUBMIT, Some("Basic dXNlcjpwYXNz"))),
        Answer::ScriptExhausted,
        "a simulator that improvised would let a test pass for a reason nobody wrote down"
    );
}

#[test]
fn every_deployment_profile_provides_the_same_authority() {
    for profile in EVERY_PROFILE {
        let authority = ContinuationKeyAuthority::created(*profile);
        assert_eq!(authority.profile(), *profile);
        let lease = authority.take_lease();
        authority.compare_and_set(lease, "current", None, "a-key").expect("a first write");
        assert_eq!(authority.read("current").expect("a read"), Some("a-key".to_owned()));

        assert_eq!(
            authority.compare_and_set(lease, "current", None, "another"),
            Err(AuthorityRefusal::CompareFailed),
            "{profile:?}: a write against a value that moved on does not apply"
        );

        let taken = authority.take_lease();
        assert_eq!(
            authority.compare_and_set(lease, "current", Some("a-key"), "another"),
            Err(AuthorityRefusal::Fenced),
            "{profile:?}: a node replaced under a deployment cannot write late"
        );
        authority
            .compare_and_set(taken, "current", Some("a-key"), "another")
            .expect("while the holder can");
    }
    assert!(
        EVERY_PROFILE.contains(&DeploymentProfile::SingleNode),
        "a single node implements the cluster contract, so adding one changes no guarantee"
    );
}

#[test]
fn an_absent_key_ring_is_not_an_empty_one() {
    let absent = ContinuationKeyAuthority::absent(DeploymentProfile::SingleNode);
    assert_eq!(absent.read("current"), Err(AuthorityRefusal::Absent));
    assert_eq!(
        absent.compare_and_set(absent.current_lease(), "current", None, "a-key"),
        Err(AuthorityRefusal::Absent),
        "a caller finding nothing is told to look at why, rather than issuing new keys"
    );

    let created = ContinuationKeyAuthority::created(DeploymentProfile::SingleNode);
    assert_eq!(created.read("current").expect("a read"), None, "while an empty ring reads empty");
}

#[test]
fn both_bounds_refuse_before_the_ring_grows_past_them() {
    let authority = ContinuationKeyAuthority::created(DeploymentProfile::Cluster);
    let lease = authority.current_lease();
    let largest = "k".repeat(MAXIMUM_KEY_BYTES);
    authority.compare_and_set(lease, "a", None, &largest).expect("the largest key");
    assert!(
        matches!(
            authority.compare_and_set(lease, "b", None, &"k".repeat(MAXIMUM_KEY_BYTES + 1)),
            Err(AuthorityRefusal::KeyTooLong { .. })
        ),
        "one byte further in a key"
    );

    let refused = authority.compare_and_set(lease, "b", None, &largest);
    assert!(
        matches!(refused, Err(AuthorityRefusal::RingTooLong { .. })),
        "and a second largest key crosses the ring bound: {refused:?}"
    );
    assert!(
        authority.ring_bytes().expect("a size") <= MAXIMUM_KEY_RING_BYTES,
        "which is refused before the ring grows past it"
    );
}

#[test]
fn one_logical_command_has_one_effect_however_many_records_sling_makes() {
    let operation = LogicalOperation::recorded();
    for index in 0..DUPLICATED_RECORDS {
        operation.physical_record(&format!("sling-job-{index}"));
    }
    operation.physical_record("sling-job-0");
    assert_eq!(
        operation.physical_records(),
        DUPLICATED_RECORDS,
        "a duplicate delivery is the normal case, not an error"
    );
    assert_eq!(operation.state(), ExecutionState::NotStarted);

    operation.start(FIRST_LEASE).expect("one caller crosses the transition");
    assert_eq!(operation.state(), ExecutionState::Started);
    assert_eq!(
        operation.start(FIRST_LEASE + 1),
        Err(OutboxRefusal::AlreadyStarted),
        "starting is not a thing that happens twice"
    );

    assert_eq!(
        operation.effect(FIRST_LEASE + 1),
        Err(OutboxRefusal::Fenced),
        "and a lease lost after the transition never permits a second effect"
    );
    operation.effect(FIRST_LEASE).expect("the caller that started may act");
    assert_eq!(operation.state(), ExecutionState::Effected);
}

#[test]
fn a_postcheck_that_disagrees_with_the_records_fails_closed() {
    let operation = LogicalOperation::recorded();
    operation.physical_record("sling-job-0");
    operation.physical_record("sling-job-1");

    operation.require_records(RECORDED_JOBS).expect("what the postcheck expected");
    let too_few = operation.require_records(1);
    assert!(
        matches!(
            too_few,
            Err(OutboxRefusal::RecordsDisagree { expected: 1, found: RECORDED_JOBS })
        ),
        "more records than expected may mean a delivery this daemon does not know about"
    );
    let too_many = operation.require_records(DUPLICATED_RECORDS);
    assert!(
        matches!(too_many, Err(OutboxRefusal::RecordsDisagree { found: RECORDED_JOBS, .. })),
        "and fewer may mean one it recorded and lost; neither is a state to act on: {too_many:?}"
    );
}
