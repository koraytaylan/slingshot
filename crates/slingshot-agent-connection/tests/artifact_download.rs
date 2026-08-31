//! Fetching one artifact from a route this daemon builds, and nothing else.
//!
//! The agent's result says an artifact exists; it does not say where. So the
//! route is constructed here, every segment encoded once, and no server-offered
//! location is accepted - not even a same-origin redirect. Following one with a
//! credential attached is how a credential reaches somewhere it was never
//! issued for, and the tests spend most of their attention on the ways a
//! response might try to choose the next request.
//!
//! The other subject is that nothing is written where a reader could see it
//! until everything is proved together. A body that ended cleanly at the wrong
//! length and a body of the right length that never ended are both failures,
//! and both leave the mapping intact so the retry asks for the same artifact
//! under the same names.

use slingshot_agent_connection::artifact_download::{
    ARTIFACT_ROUTE, ArtifactResponseHead, ArtifactTransfer, ArtifactUnavailable, DownloadRefusal,
    ExpectedArtifact, OPERATION_QUERY_MEMBER, PERMITTED_CONTENT_CODINGS, SLOT_QUERY_MEMBER,
    TransferEnd, UnavailableOutcome, UnavailableReason, artifact_route, encoded_segment,
    missing_grace_milliseconds, require_remote_slot, require_streamable, unavailable_outcome,
};
use slingshot_agent_connection::author_hypertext_transfer_protocol_policy::ResponseHead;
use slingshot_agent_connection::structured_job_result::STRUCTURED_RESULT_SLOT;
use slingshot_domain::command::artifact::{
    CONTENT_PACKAGE_MEDIA_TYPE, CONTENT_PACKAGE_SLOT, LOADED_CONTENT_MEDIA_TYPE,
    LOADED_CONTENT_SLOT,
};

/// Where the vectors this suite is driven from live.
const FIXTURES: &str = "tests/fixtures/artifact-download";

/// The author these artifacts are fetched from.
const AUTHOR_BASE: &str = "https://author.example";

/// Where a server would rather this daemon went.
const OFFERED_ROUTE: &str = "https://elsewhere.example/artifacts/one";

/// What the operation is called at the agent.
const AGENT_OPERATION: &str = "agent-operation-alpha";

/// Which incarnation of the store it belongs to.
const GENERATION: u64 = 7;

/// A later generation, after the agent's store was rebuilt.
const LATER_GENERATION: u64 = 8;

/// What the artifact will digest to.
const EXPECTED_DIGEST: &str = "expected-artifact-digest";

/// What a different body digests to.
const OTHER_DIGEST: &str = "another-artifact-digest";

/// How long the artifact will be.
const EXPECTED_BYTES: u64 = 4_096;

/// How large one artifact in this slot may be.
const SLOT_MAXIMUM: u64 = 1_048_576;

/// One instant, for the vectors that need one.
const ASKED_AT: u64 = 1_700_000_000_000;

/// The protocol version the author is spoken to over.
const SPOKEN_VERSION: &str = "HTTP/1.1";

/// A protocol version this daemon does not speak.
const UNSPOKEN_VERSION: &str = "HTTP/1.0";

/// A content coding whose decoded length nobody declared.
const UNEXPECTED_CODING: &str = "gzip";

/// Returns every vector one fixture holds.
fn vectors(name: &str) -> Vec<serde_json::Value> {
    let path = format!("{FIXTURES}/{name}");
    let text = std::fs::read_to_string(&path).unwrap_or_else(|_| panic!("{path} is readable"));
    text.lines().map(|line| serde_json::from_str(line).expect("each line is one vector")).collect()
}

/// Returns what the manifest says this artifact will be.
fn expected() -> ExpectedArtifact {
    ExpectedArtifact {
        artifact_digest: EXPECTED_DIGEST.to_owned(),
        artifact_slot: CONTENT_PACKAGE_SLOT.to_owned(),
        byte_length: EXPECTED_BYTES,
        media_type: CONTENT_PACKAGE_MEDIA_TYPE.to_owned(),
    }
}

/// Returns a head with nothing wrong with it.
fn clean_head() -> ArtifactResponseHead {
    ArtifactResponseHead {
        content_type: CONTENT_PACKAGE_MEDIA_TYPE.to_owned(),
        head: ResponseHead {
            alternative_service_offered: false,
            content_coding: None,
            informational: false,
            location: None,
            protocol_version: SPOKEN_VERSION.to_owned(),
            trailers_declared: false,
        },
    }
}

/// Returns the end `spelling` names.
fn end_named(spelling: &str) -> TransferEnd {
    match spelling {
        "framed" => TransferEnd::Framed,
        "interrupted" => TransferEnd::Interrupted,
        "undeclared-trailer" => TransferEnd::UndeclaredTrailer,
        "trailing-bytes" => TransferEnd::TrailingBytes,
        other => panic!("{other} is an end this suite does not stage"),
    }
}

/// Returns the answer one unavailability vector describes.
fn answered(vector: &serde_json::Value) -> ArtifactUnavailable {
    let reason = match vector["reason"].as_str().expect("a reason") {
        "missing" => UnavailableReason::Missing,
        "retention-expired" => UnavailableReason::RetentionExpired,
        other => panic!("{other} is a reason this suite does not name"),
    };
    let mut answer = ArtifactUnavailable {
        agent_event_store_generation: GENERATION,
        agent_operation_identifier: AGENT_OPERATION.to_owned(),
        artifact_digest: EXPECTED_DIGEST.to_owned(),
        artifact_slot: CONTENT_PACKAGE_SLOT.to_owned(),
        reason,
    };
    match vector["field"].as_str() {
        Some("generation") => answer.agent_event_store_generation = LATER_GENERATION,
        Some("operation") => answer.agent_operation_identifier = "agent-operation-beta".to_owned(),
        Some("artifact") => answer.artifact_digest = OTHER_DIGEST.to_owned(),
        Some("slot") => answer.artifact_slot = LOADED_CONTENT_SLOT.to_owned(),
        Some(other) => panic!("{other} is a field this suite does not substitute"),
        None => {}
    }
    answer
}

#[test]
fn every_route_is_built_here_and_every_segment_is_encoded_once() {
    for vector in vectors("routes.jsonl") {
        let name = vector["name"].as_str().expect("a name");
        let operation = vector["operation"].as_str().expect("an operation");
        let slot = vector["slot"].as_str().expect("a slot");
        assert_eq!(
            encoded_segment(operation),
            vector["encoded_operation"].as_str().expect("an encoding"),
            "{name}: a separator surviving into a segment would let it choose the route"
        );
        assert_eq!(
            artifact_route(AUTHOR_BASE, operation, slot),
            format!(
                "{AUTHOR_BASE}{ARTIFACT_ROUTE}?{OPERATION_QUERY_MEMBER}={}&{SLOT_QUERY_MEMBER}={}",
                vector["encoded_operation"].as_str().expect("an encoding"),
                vector["encoded_slot"].as_str().expect("an encoding")
            ),
            "{name}"
        );
    }
    assert_eq!(
        artifact_route(&format!("{AUTHOR_BASE}/"), AGENT_OPERATION, CONTENT_PACKAGE_SLOT),
        artifact_route(AUTHOR_BASE, AGENT_OPERATION, CONTENT_PACKAGE_SLOT),
        "one author base, however it is spelled"
    );
}

#[test]
fn a_server_offering_a_route_gets_no_second_request() {
    let mut redirected = clean_head();
    redirected.head.location = Some(OFFERED_ROUTE.to_owned());
    assert_eq!(
        require_streamable(&expected(), &redirected),
        Err(DownloadRefusal::ServerRouteOffered { offered: OFFERED_ROUTE.to_owned() }),
        "following one with a credential attached sends it somewhere it was not issued for"
    );
    let mut same_origin = clean_head();
    same_origin.head.location = Some(format!("{AUTHOR_BASE}/somewhere-else"));
    assert!(
        matches!(
            require_streamable(&expected(), &same_origin),
            Err(DownloadRefusal::ServerRouteOffered { .. })
        ),
        "including a same-origin one, because the route is this daemon's to choose"
    );
}

#[test]
fn a_head_the_shared_policy_refuses_never_becomes_a_body() {
    let mut informational = clean_head();
    informational.head.informational = true;
    let mut declared = clean_head();
    declared.head.trailers_declared = true;
    let mut unspoken = clean_head();
    unspoken.head.protocol_version = UNSPOKEN_VERSION.to_owned();
    let mut migrating = clean_head();
    migrating.head.alternative_service_offered = true;
    for announced in [informational, declared, unspoken, migrating] {
        assert!(matches!(
            require_streamable(&expected(), &announced),
            Err(DownloadRefusal::Head(_))
        ));
    }
    let mut compressed = clean_head();
    compressed.head.content_coding = Some(UNEXPECTED_CODING.to_owned());
    assert!(
        require_streamable(&expected(), &compressed).is_err(),
        "a compressed body has a decoded length nobody declared, so the length check checks \
         the wrong number"
    );
    let mut identity = clean_head();
    identity.head.content_coding = Some(PERMITTED_CONTENT_CODINGS[0].to_owned());
    require_streamable(&expected(), &identity).expect("identity is the one coding accepted");
}

#[test]
fn a_body_announcing_something_else_is_refused_before_it_is_read() {
    let mut wrong_type = clean_head();
    wrong_type.content_type = LOADED_CONTENT_MEDIA_TYPE.to_owned();
    assert_eq!(
        require_streamable(&expected(), &wrong_type),
        Err(DownloadRefusal::MediaTypeDrifted {
            expected: CONTENT_PACKAGE_MEDIA_TYPE.to_owned(),
            named: LOADED_CONTENT_MEDIA_TYPE.to_owned()
        })
    );
    let mut parameterized = clean_head();
    parameterized.content_type = format!("{CONTENT_PACKAGE_MEDIA_TYPE}; charset=utf-8");
    assert!(
        require_streamable(&expected(), &parameterized).is_err(),
        "one parameter-free media type, exactly equal to the manifest's"
    );
}

#[test]
fn only_a_slot_some_command_declares_may_be_fetched_remotely() {
    assert_eq!(
        require_remote_slot(CONTENT_PACKAGE_SLOT).expect("a package slot"),
        CONTENT_PACKAGE_MEDIA_TYPE
    );
    assert_eq!(
        require_remote_slot(LOADED_CONTENT_SLOT).expect("a loaded-content slot"),
        LOADED_CONTENT_MEDIA_TYPE
    );
    assert_eq!(
        require_remote_slot(STRUCTURED_RESULT_SLOT),
        Err(DownloadRefusal::LocalSlot { slot: STRUCTURED_RESULT_SLOT.to_owned() }),
        "the local externalization slot is this daemon's, and no agent fills it"
    );
    assert!(matches!(
        require_remote_slot("a_slot_nobody_declares"),
        Err(DownloadRefusal::UnknownSlot { .. })
    ));
}

#[test]
fn the_bounds_are_checked_as_the_bytes_arrive_rather_than_at_the_end() {
    let mut transfer = ArtifactTransfer::of(expected(), SLOT_MAXIMUM);
    transfer.absorb(EXPECTED_BYTES).expect("exactly what was declared");
    assert_eq!(transfer.received(), EXPECTED_BYTES);
    assert!(
        matches!(transfer.absorb(1), Err(DownloadRefusal::LengthDrifted { .. })),
        "a server that sends more than it declared has already cost the disk it wrote"
    );

    let mut narrow = ArtifactTransfer::of(
        ExpectedArtifact { byte_length: SLOT_MAXIMUM + 1, ..expected() },
        SLOT_MAXIMUM,
    );
    assert!(matches!(
        narrow.absorb(SLOT_MAXIMUM + 1),
        Err(DownloadRefusal::SlotMaximumExceeded { .. })
    ));
}

#[test]
fn every_transfer_publishes_exactly_when_its_vector_says() {
    for vector in vectors("transfers.jsonl") {
        let name = vector["name"].as_str().expect("a name");
        let mut transfer = ArtifactTransfer::of(expected(), SLOT_MAXIMUM);
        transfer.absorb(vector["received"].as_u64().expect("a count")).expect("within the bound");
        let digest = match vector["digest"].as_str().expect("a digest") {
            "expected" => EXPECTED_DIGEST,
            _ => OTHER_DIGEST,
        };
        let produced = transfer
            .require_publishable(end_named(vector["end"].as_str().expect("an end")), digest);
        assert_eq!(
            produced.is_ok(),
            vector["publishes"].as_bool().expect("an expectation"),
            "{name}: {produced:?}"
        );
    }
}

#[test]
fn an_unavailable_answer_concludes_only_when_it_identifies_this_artifact() {
    for vector in vectors("unavailability.jsonl") {
        let name = vector["name"].as_str().expect("a name");
        let outcome = unavailable_outcome(
            &expected(),
            AGENT_OPERATION,
            GENERATION,
            Some(&answered(&vector)),
            ASKED_AT,
            ASKED_AT + vector["elapsed"].as_u64().expect("an elapsed"),
        );
        let spelling = match outcome {
            UnavailableOutcome::Grace { .. } => "grace",
            UnavailableOutcome::ResultUnavailable => "result-unavailable",
            UnavailableOutcome::ProtocolInvalid => "protocol-invalid",
        };
        assert_eq!(spelling, vector["outcome"].as_str().expect("an outcome"), "{name}");
    }
    assert_eq!(
        unavailable_outcome(&expected(), AGENT_OPERATION, GENERATION, None, ASKED_AT, ASKED_AT),
        UnavailableOutcome::ProtocolInvalid,
        "an answer identifying nothing could end any operation any server can reach"
    );
    let grace = missing_grace_milliseconds();
    assert!(matches!(
        unavailable_outcome(
            &expected(),
            AGENT_OPERATION,
            GENERATION,
            Some(&ArtifactUnavailable {
                agent_event_store_generation: GENERATION,
                agent_operation_identifier: AGENT_OPERATION.to_owned(),
                artifact_digest: EXPECTED_DIGEST.to_owned(),
                artifact_slot: CONTENT_PACKAGE_SLOT.to_owned(),
                reason: UnavailableReason::Missing,
            }),
            ASKED_AT,
            ASKED_AT + grace - 1
        ),
        UnavailableOutcome::Grace { .. }
    ));
}
