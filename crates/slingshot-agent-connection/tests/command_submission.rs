//! Submitting one command, and being honest about what happened afterwards.
//!
//! Two things are being proved here, and they pull in opposite directions.
//!
//! The first is that a submission is completely determined by what this build
//! has: every published command derives its own submission, and changing any
//! one thing that decides it - either schema, the limits, the version, the wire
//! name, the byte contract, the transport contract, the arguments, or the
//! artifact manifest - changes the digest that binds it, independently, without
//! the five-field contract identity growing a sixth field.
//!
//! The second is that after the first request byte is written, this daemon
//! claims nothing it cannot prove. Every malformed, misframed, mistyped,
//! mistimed, or mis-echoed answer resolves to one unknown submission awaiting a
//! lookup - never to a refusal, because a refusal is a licence to send the
//! command again.

use slingshot_agent_connection::author_cross_site_request_forgery_protection::{
    CrossSiteRequestForgeryToken, TOKEN_HEADER,
};
use slingshot_agent_connection::author_hypertext_transfer_protocol_policy::ResponseHead;
use slingshot_agent_connection::command_submission::{
    ANSWERED_STATUSES, CONFLICT_STATUS, CapacityDiscriminator, Checkpoint, DiscoveryShape,
    Exchange, ExpectedArtifactManifest, IDEMPOTENCY_KEY_HEADER, ManifestKind, NonExecution,
    REFERER_HEADER, RESERVED_HEADERS, RecoveryPreconditions, StatusClass, Submission,
    SubmissionAcknowledgement, SubmissionOutcome, SubmissionRefusal, UnknownCause, classify_status,
    remaining_retention_milliseconds,
};
use slingshot_agent_protocol::identity::WireOperationIdentity;
use slingshot_agent_protocol::wire_contract::ExpectedProvenance;
use slingshot_domain::agent_identity::AgentEventStoreGeneration;
use slingshot_domain::author_agent_transport_contract::AuthorAgentTransportContract;
use slingshot_domain::command::catalog::{CommandCatalog, CommandDescriptor};
use slingshot_domain::command::schema::canonical_contract_digest;
use slingshot_domain::selected_command_contract_identity::SelectedCommandContractIdentity;

/// Where the vectors this suite is driven from live.
const FIXTURES: &str = "tests/fixtures/command-submission";

/// The author these submissions are sent to.
const AUTHOR: &str = "https://author.example";

/// One instant, for the tests that do not care which.
const NOW: u64 = 1_700_000_000_000;

/// How long after now the fixture token expires.
const TOKEN_LIFETIME: u64 = 300_000;

/// The value the fixture token carries, which nothing may print.
const TOKEN_VALUE: &str = "a-forgery-protection-token-value";

/// Which generation these submissions are derived under.
const GENERATION: u64 = 7;

/// A later generation, for the recovery that must not happen.
const LATER_GENERATION: u64 = 8;

/// The target partition these submissions belong to.
const TARGET: &str = "target-identity-digest-one";

/// Another target, to prove nothing is replayed across partitions.
const ANOTHER_TARGET: &str = "target-identity-digest-two";

/// The environment revision these submissions are derived under.
const REVISION: &str = "environment-revision-one";

/// The local operation key every derivation starts from.
const LOCAL_OPERATION: &str = "local-operation-one";

/// The subscription one submission registers.
const SUBSCRIPTION: &str = "daemon-subscription-one";

/// Canonical arguments the derivation tests submit.
const ARGUMENTS: &str = "{\"path\":\"/content/one\"}";

/// Other canonical arguments, validated the same and digesting differently.
const OTHER_ARGUMENTS: &str = "{\"path\":\"/content/two\"}";

/// A digest substituted where a real one belongs.
const SUBSTITUTED_DIGEST: &str = "0000000000000000000000000000000000000000000000000000000000000000";

/// A response body a leak test would print if anything printed one.
const RESPONSE_BODY: &str = "an unbounded remote explanation nobody asked for";

/// How long an agent promises to keep one submission's results.
const GRANTED_RETENTION: u64 = 120_000;

/// How long a prompt exchange takes.
const PROMPT_ELAPSED: u64 = 400;

/// How long a delayed exchange takes.
const DELAYED_ELAPSED: u64 = 90_000;

/// How long a server asks to be left alone, in milliseconds.
const REQUESTED_RETRY_DELAY: u64 = 5_000;

/// A retry delay longer than any server may ask for.
const EXCESSIVE_RETRY_DELAY: u64 = 3_600_000;

/// One retryable status, for the tests that need only one.
const RETRYABLE_STATUS: u16 = 503;

/// One authoritative rejection status.
const REJECTION_STATUS: u16 = 422;

/// A status this build never validated.
const UNVALIDATED_STATUS: u16 = 418;

/// A protocol version this daemon does not speak.
const UNSPOKEN_VERSION: &str = "HTTP/1.0";

/// The protocol versions the author is spoken to over.
const SPOKEN_VERSIONS: &[&str] = &["HTTP/1.1", "HTTP/2"];

/// A media type that is not the one a submission is answered in.
const WRONG_MEDIA_TYPE: &str = "text/html";

/// A content coding that hides a body's decoded length.
const UNEXPECTED_CODING: &str = "gzip";

/// Where a redirect would send a submission that follows one.
const REDIRECT_TARGET: &str = "https://elsewhere.example/submit";

/// Another semantic contract version, which no installed command carries.
const OTHER_CONTRACT_VERSION: &str = "second";

/// Another published command, to substitute one wire name for another.
const OTHER_COMMAND: &str = "create_page";

/// Rows one artifact manifest declares when it declares any.
const DECLARED_ARTIFACT_ROWS: u64 = 3;

/// Bytes one artifact manifest declares when it declares any.
const DECLARED_ARTIFACT_BYTES: u64 = 4_096;

/// Returns the vectors `name` holds.
fn fixture(name: &str) -> serde_json::Value {
    let path = format!("{FIXTURES}/{name}");
    let text = std::fs::read_to_string(&path).unwrap_or_else(|_| panic!("{path} is readable"));
    serde_json::from_str(&text).unwrap_or_else(|_| panic!("{path} is one value"))
}

/// Returns what this build expects of `wire_name`'s documents.
fn expected_for(wire_name: &str) -> ExpectedProvenance {
    ExpectedProvenance {
        canonical_json_contract_digest: canonical_contract_digest(),
        command_contract: SelectedCommandContractIdentity::installed(wire_name)
            .unwrap_or_else(|_| panic!("{wire_name} is published")),
        transport_contract_digest: AuthorAgentTransportContract::embedded_digest(),
    }
}

/// Returns the manifest `descriptor` declares.
fn manifest_for(descriptor: &CommandDescriptor) -> ExpectedArtifactManifest {
    let slots = &descriptor.remote_artifact_slots;
    if slots.is_empty() {
        return ExpectedArtifactManifest::empty();
    }
    let kind = if slots.len() > 1 { ManifestKind::Package } else { ManifestKind::Load };
    let bytes = slots.iter().map(|slot| slot.maximum_byte_length).sum();
    ExpectedArtifactManifest::declaring(kind, slots.len() as u64, bytes)
        .expect("a published command declares artifacts one generation can hold")
}

/// Returns the identity one operation has, in `target`'s partition.
fn operation_in(target: &str, generation: u64) -> WireOperationIdentity {
    WireOperationIdentity::of(
        target,
        REVISION,
        LOCAL_OPERATION,
        AgentEventStoreGeneration::of(generation),
    )
}

/// Returns the submission one command with `manifest` produces.
fn submission_of(expected: &ExpectedProvenance, manifest: ExpectedArtifactManifest) -> Submission {
    Submission::build(expected, operation_in(TARGET, GENERATION), SUBSCRIPTION, ARGUMENTS, manifest)
        .expect("these arguments fit one submission")
}

/// Returns the submission the rest of this suite exchanges answers about.
fn submission() -> Submission {
    submission_of(&expected_for("query_paths"), ExpectedArtifactManifest::empty())
}

/// Returns a token this author issued.
fn token() -> CrossSiteRequestForgeryToken {
    CrossSiteRequestForgeryToken {
        expires_at_unix_milliseconds: NOW + TOKEN_LIFETIME,
        origin: AUTHOR.to_owned(),
        value: TOKEN_VALUE.to_owned(),
    }
}

/// Returns a head with nothing wrong with it.
fn clean_head() -> ResponseHead {
    ResponseHead {
        alternative_service_offered: false,
        content_coding: None,
        informational: false,
        location: None,
        protocol_version: SPOKEN_VERSIONS[0].to_owned(),
        trailers_declared: false,
    }
}

/// Returns what an agent that accepted `submission` says about it.
fn acknowledgement_of(submission: &Submission, jobs: &[&str]) -> SubmissionAcknowledgement {
    SubmissionAcknowledgement {
        agent_event_store_generation: submission.operation.agent_event_store_generation,
        agent_operation_identifier: submission.operation.agent_operation_identifier.clone(),
        author_target_identity_digest: submission.operation.author_target_identity_digest.clone(),
        already_accepted: false,
        daemon_subscription_identifier: submission.daemon_subscription_identifier.clone(),
        granted_retention_milliseconds: GRANTED_RETENTION,
        non_execution: None,
        physical_sling_job_identifiers: jobs.iter().map(|job| (*job).to_owned()).collect(),
        retired: false,
        submitted_command_digest: submission.submitted_command_digest.clone(),
    }
}

/// Returns a clean exchange carrying `acknowledgement`.
fn exchange_of(acknowledgement: SubmissionAcknowledgement) -> Exchange {
    Exchange {
        acknowledgement: Some(acknowledgement),
        body_bytes: 0,
        elapsed_milliseconds: PROMPT_ELAPSED,
        framing_ambiguous: false,
        head: clean_head(),
        media_type: "application/json".to_owned(),
        retry_after_milliseconds: None,
        status: ANSWERED_STATUSES[0],
        trailer_section_present: false,
        trailing_bytes: false,
        unknown_fields: false,
    }
}

/// Returns `exchange` with one named defect introduced.
fn with_defect(mut exchange: Exchange, defect: &str) -> Exchange {
    if !stage_head_defect(&mut exchange, defect) {
        stage_message_defect(&mut exchange, defect);
    }
    exchange
}

/// Stages one defect in the response head, and says whether it did.
fn stage_head_defect(exchange: &mut Exchange, defect: &str) -> bool {
    match defect {
        "informational-head" => exchange.head.informational = true,
        "alternative-service-offered" => exchange.head.alternative_service_offered = true,
        "unsupported-protocol-version" => {
            exchange.head.protocol_version = UNSPOKEN_VERSION.to_owned();
        }
        "redirect-offered" => exchange.head.location = Some(REDIRECT_TARGET.to_owned()),
        "trailers-declared" => exchange.head.trailers_declared = true,
        "unexpected-content-coding" => {
            exchange.head.content_coding = Some(UNEXPECTED_CODING.to_owned());
        }
        _ => return false,
    }
    true
}

/// Stages one defect in the message the head arrived in.
fn stage_message_defect(exchange: &mut Exchange, defect: &str) {
    match defect {
        "trailer-section-present" => exchange.trailer_section_present = true,
        "ambiguous-framing" => exchange.framing_ambiguous = true,
        "trailing-bytes" => exchange.trailing_bytes = true,
        "wrong-media-type" => exchange.media_type = WRONG_MEDIA_TYPE.to_owned(),
        "oversized-body" => {
            exchange.body_bytes = AuthorAgentTransportContract::embedded()
                .limit("maximum_finite_response_body_bytes")
                + 1;
        }
        "unknown-field" => exchange.unknown_fields = true,
        "unvalidated-status" => exchange.status = UNVALIDATED_STATUS,
        "absent-body" => exchange.acknowledgement = None,
        other => panic!("{other} is a defect this suite does not stage"),
    }
}

/// How the vectors spell each closed cause an unbelievable answer resolves to.
const CAUSE_SPELLINGS: &[(&str, UnknownCause)] = &[
    ("informational-head", UnknownCause::InformationalHead),
    ("protocol-version", UnknownCause::ProtocolVersion),
    ("trailers-declared", UnknownCause::TrailersDeclared),
    ("trailer-section", UnknownCause::TrailerSection),
    ("framing", UnknownCause::Framing),
    ("trailing-bytes", UnknownCause::TrailingBytes),
    ("media", UnknownCause::Media),
    ("body", UnknownCause::Body),
    ("unknown-field", UnknownCause::UnknownField),
    ("unvalidated-status", UnknownCause::UnvalidatedStatus),
];

/// Returns the cause `spelling` names.
fn cause_named(spelling: &str) -> UnknownCause {
    CAUSE_SPELLINGS
        .iter()
        .find(|(named, _)| *named == spelling)
        .map(|(_, cause)| *cause)
        .unwrap_or_else(|| panic!("{spelling} is a cause this suite does not name"))
}

/// Returns the class `spelling` names.
fn class_named(spelling: &str) -> StatusClass {
    match spelling {
        "answered" => StatusClass::Answered,
        "rejected" => StatusClass::Rejected,
        "conflict" => StatusClass::Conflict,
        "retryable" => StatusClass::Retryable,
        "unvalidated" => StatusClass::Unvalidated,
        other => panic!("{other} is a class this suite does not name"),
    }
}

#[test]
fn every_published_command_derives_its_own_submission_from_what_this_build_has() {
    let catalog = CommandCatalog::published();
    let mut digests: Vec<String> = Vec::new();
    for descriptor in catalog.descriptors() {
        let expected = expected_for(&descriptor.wire_name);
        let manifest = manifest_for(descriptor);
        let submission = submission_of(&expected, manifest);
        let identity = &expected.command_contract;
        assert_eq!(submission.provenance.command_contract.command_wire_name, descriptor.wire_name);
        assert_eq!(
            submission.provenance.command_contract.argument_schema_digest,
            identity.argument_schema_digest,
            "{}: the arguments schema a submission names is the installed one",
            descriptor.wire_name
        );
        assert_eq!(
            submission.provenance.command_contract.result_schema_digest,
            identity.result_schema_digest
        );
        assert_eq!(
            submission.provenance.canonical_json_contract_digest,
            canonical_contract_digest()
        );
        assert_eq!(submission.manifest, manifest);
        digests.push(submission.submitted_command_digest);
    }
    let mut distinct = digests.clone();
    distinct.sort();
    distinct.dedup();
    assert_eq!(
        distinct.len(),
        digests.len(),
        "two commands sharing a submitted digest would let an agent answer about one and be \
         believed about the other"
    );
}

#[test]
fn changing_any_one_thing_that_decides_a_submission_changes_its_digest() {
    let base = expected_for("query_paths");
    let manifest = ExpectedArtifactManifest::empty();
    let declared = ExpectedArtifactManifest::declaring(
        ManifestKind::Load,
        DECLARED_ARTIFACT_ROWS,
        DECLARED_ARTIFACT_BYTES,
    )
    .expect("a small manifest fits");
    let mut substitutions: Vec<(&str, Submission)> = Vec::new();
    substitutions.push(("nothing", submission_of(&base, manifest)));

    let mut contract_substituted = base.clone();
    contract_substituted.canonical_json_contract_digest = SUBSTITUTED_DIGEST.to_owned();
    substitutions.push(("canonical byte contract", submission_of(&contract_substituted, manifest)));

    let mut transport_substituted = base.clone();
    transport_substituted.transport_contract_digest = SUBSTITUTED_DIGEST.to_owned();
    substitutions.push(("transport contract", submission_of(&transport_substituted, manifest)));

    for (label, substituted) in contract_substitutions(&base) {
        substitutions.push((label, submission_of(&substituted, manifest)));
    }

    substitutions.push((
        "arguments",
        Submission::build(
            &base,
            operation_in(TARGET, GENERATION),
            SUBSCRIPTION,
            OTHER_ARGUMENTS,
            manifest,
        )
        .expect("other arguments also fit"),
    ));
    substitutions.push(("manifest", submission_of(&base, declared)));
    substitutions.push((
        "manifest rows",
        submission_of(
            &base,
            ExpectedArtifactManifest::declaring(
                ManifestKind::Load,
                DECLARED_ARTIFACT_ROWS + 1,
                DECLARED_ARTIFACT_BYTES,
            )
            .expect("a small manifest fits"),
        ),
    ));
    substitutions.push((
        "manifest bytes",
        submission_of(
            &base,
            ExpectedArtifactManifest::declaring(
                ManifestKind::Load,
                DECLARED_ARTIFACT_ROWS,
                DECLARED_ARTIFACT_BYTES + 1,
            )
            .expect("a small manifest fits"),
        ),
    ));

    for (position, (label, submission)) in substitutions.iter().enumerate() {
        let named = &submission.provenance.command_contract;
        assert!(
            !named.command_wire_name.is_empty()
                && !named.command_semantic_contract_version.is_empty()
                && !named.command_contract_limits_digest.is_empty()
                && !named.argument_schema_digest.is_empty()
                && !named.result_schema_digest.is_empty(),
            "{label}: the identity keeps all five fields and grows no sixth"
        );
        for (other_label, other) in substitutions.iter().take(position) {
            assert_ne!(
                submission.submitted_command_digest, other.submitted_command_digest,
                "substituting {label} must not digest the same as substituting {other_label}"
            );
        }
    }
}

/// Returns one substitution per field of the five-field contract identity.
fn contract_substitutions(base: &ExpectedProvenance) -> Vec<(&'static str, ExpectedProvenance)> {
    let mut argument_schema = base.clone();
    argument_schema.command_contract.argument_schema_digest = SUBSTITUTED_DIGEST.to_owned();
    let mut result_schema = base.clone();
    result_schema.command_contract.result_schema_digest = SUBSTITUTED_DIGEST.to_owned();
    let mut contract_limits = base.clone();
    contract_limits.command_contract.command_contract_limits_digest = SUBSTITUTED_DIGEST.to_owned();
    let mut semantic_version = base.clone();
    semantic_version.command_contract.command_semantic_contract_version =
        OTHER_CONTRACT_VERSION.to_owned();
    let mut wire_name = base.clone();
    wire_name.command_contract.command_wire_name = OTHER_COMMAND.to_owned();
    vec![
        ("argument schema", argument_schema),
        ("result schema", result_schema),
        ("contract limits", contract_limits),
        ("semantic version", semantic_version),
        ("wire name", wire_name),
    ]
}

#[test]
fn the_idempotency_key_is_derived_so_a_restart_arrives_at_the_same_submission() {
    let first = submission();
    let second = submission();
    assert_eq!(first, second, "nothing about a submission is allocated");
    let headers = first
        .request_headers(Some(&token()), AUTHOR, NOW, &[])
        .expect("a fresh token and no caller headers");
    let key = headers
        .iter()
        .find(|(name, _)| name == IDEMPOTENCY_KEY_HEADER)
        .expect("a submission carries an idempotency key");
    assert_eq!(
        key.1, first.submitted_command_digest,
        "a key that were not the digest would let one submission be resent as another"
    );
    assert!(headers.iter().any(|(name, value)| name == TOKEN_HEADER && value == TOKEN_VALUE));
    assert!(
        headers.iter().any(|(name, value)| name == REFERER_HEADER && value.starts_with(AUTHOR)),
        "the origin a request names is the one it was formed against"
    );
}

#[test]
fn a_caller_cannot_set_a_header_this_submission_derives() {
    let submission = submission();
    for reserved in RESERVED_HEADERS {
        let supplied = vec![(reserved.to_lowercase(), "anything".to_owned())];
        assert!(
            matches!(
                submission.request_headers(Some(&token()), AUTHOR, NOW, &supplied),
                Err(SubmissionRefusal::ReservedHeader(_))
            ),
            "{reserved} carries a decision made from derived values, however it is spelled"
        );
    }
    let duplicated =
        vec![("X-Trace".to_owned(), "one".to_owned()), ("x-trace".to_owned(), "two".to_owned())];
    assert!(matches!(
        submission.request_headers(Some(&token()), AUTHOR, NOW, &duplicated),
        Err(SubmissionRefusal::DuplicateHeader(_))
    ));
}

#[test]
fn a_submission_without_a_usable_token_is_refused_before_it_is_sent() {
    let submission = submission();
    assert!(matches!(
        submission.request_headers(None, AUTHOR, NOW, &[]),
        Err(SubmissionRefusal::Token(_))
    ));
    assert!(
        matches!(
            submission.request_headers(Some(&token()), AUTHOR, NOW + TOKEN_LIFETIME, &[]),
            Err(SubmissionRefusal::Token(_))
        ),
        "a token expires at its expiry, not after it"
    );
    assert!(matches!(
        submission.request_headers(Some(&token()), "https://elsewhere.example", NOW, &[]),
        Err(SubmissionRefusal::Token(_))
    ));
}

#[test]
fn a_submission_reserves_its_whole_retention_branch_before_a_remote_job_exists() {
    let declared = ExpectedArtifactManifest::declaring(
        ManifestKind::Package,
        DECLARED_ARTIFACT_ROWS,
        DECLARED_ARTIFACT_BYTES,
    )
    .expect("a small manifest fits");
    let reservation = submission_of(&expected_for("query_paths"), declared).reservation();
    assert!(reservation.is_whole());
    assert_eq!(reservation.artifact_rows, DECLARED_ARTIFACT_ROWS);
    assert_eq!(
        reservation.event_rows,
        AuthorAgentTransportContract::embedded().limit("maximum_operation_event_rows"),
        "the worst case is the whole event bound, because a reservation sized to the expected \
         case runs out when the work is unusual"
    );
    for part in [
        reservation.execution_detail_rows,
        reservation.result_rows,
        reservation.snapshot_rows,
        reservation.subscription_rows,
    ] {
        assert_eq!(part, 1, "one operation occupies exactly one of each");
    }
}

#[test]
fn a_manifest_beyond_one_generation_is_refused_and_an_empty_one_declares_nothing() {
    assert!(matches!(
        ExpectedArtifactManifest::declaring(ManifestKind::Empty, 1, 0),
        Err(SubmissionRefusal::ManifestBeyondCapacity { .. })
    ));
    let beyond = AuthorAgentTransportContract::embedded()
        .formula("maximum_current_generation_artifact_rows")
        + 1;
    assert!(matches!(
        ExpectedArtifactManifest::declaring(ManifestKind::Package, beyond, DECLARED_ARTIFACT_BYTES),
        Err(SubmissionRefusal::ManifestBeyondCapacity { .. })
    ));
    assert_eq!(ExpectedArtifactManifest::empty().artifact_bytes, 0);
}

#[test]
fn arguments_larger_than_one_submission_are_refused_before_any_network_work() {
    let allowed =
        AuthorAgentTransportContract::embedded().limit("maximum_canonical_submission_bytes");
    let oversized = "a".repeat(allowed as usize + 1);
    assert!(matches!(
        Submission::build(
            &expected_for("query_paths"),
            operation_in(TARGET, GENERATION),
            SUBSCRIPTION,
            &oversized,
            ExpectedArtifactManifest::empty(),
        ),
        Err(SubmissionRefusal::TooLarge { .. })
    ));
}

#[test]
fn every_status_this_build_acts_on_falls_in_exactly_one_class() {
    let vectors = fixture("status-policy.json");
    for expectation in vectors["expectations"].as_array().expect("expectations are a list") {
        let status =
            u16::try_from(expectation["status"].as_u64().expect("a status")).expect("small");
        let expected = class_named(expectation["class"].as_str().expect("a class"));
        assert_eq!(
            classify_status(status),
            expected,
            "{status} must be read one way, because a status read two ways is a submission \
             settled two ways"
        );
    }
    assert_eq!(classify_status(CONFLICT_STATUS), StatusClass::Conflict);
}

#[test]
fn every_defect_after_the_request_bytes_leaves_the_submission_unknown() {
    let submission = submission();
    let vectors = fixture("transport-vectors.json");
    for vector in vectors["vectors"].as_array().expect("vectors are a list") {
        let defect = vector["defect"].as_str().expect("a defect");
        let cause = cause_named(vector["cause"].as_str().expect("a cause"));
        let exchange =
            with_defect(exchange_of(acknowledgement_of(&submission, &["sling-job-alpha"])), defect);
        let outcome = submission.interpret(&exchange);
        assert_eq!(
            outcome,
            SubmissionOutcome::SubmissionUnknown { cause },
            "{defect}: a message this daemon cannot frame carries no answer"
        );
        assert!(outcome.requires_reconciliation());
        assert!(!outcome.provably_not_recorded(), "{defect}: unknown is not a licence to resend");
        assert!(!outcome.provably_recorded());
    }
}

#[test]
fn an_answer_that_echoes_something_else_is_not_an_answer_about_this_submission() {
    let submission = submission();
    let base = acknowledgement_of(&submission, &["sling-job-alpha"]);
    let cases: Vec<(UnknownCause, SubmissionAcknowledgement)> = vec![
        (
            UnknownCause::Identity,
            SubmissionAcknowledgement {
                agent_operation_identifier: "another-operation".to_owned(),
                ..base.clone()
            },
        ),
        (
            UnknownCause::Generation,
            SubmissionAcknowledgement {
                agent_event_store_generation: LATER_GENERATION,
                ..base.clone()
            },
        ),
        (
            UnknownCause::Partition,
            SubmissionAcknowledgement {
                author_target_identity_digest: ANOTHER_TARGET.to_owned(),
                ..base.clone()
            },
        ),
        (
            UnknownCause::Digest,
            SubmissionAcknowledgement {
                submitted_command_digest: SUBSTITUTED_DIGEST.to_owned(),
                ..base.clone()
            },
        ),
        (
            UnknownCause::Registration,
            SubmissionAcknowledgement {
                daemon_subscription_identifier: "another-subscription".to_owned(),
                ..base.clone()
            },
        ),
    ];
    for (cause, acknowledgement) in cases {
        assert_eq!(
            submission.interpret(&exchange_of(acknowledgement)),
            SubmissionOutcome::SubmissionUnknown { cause },
            "believing this would record a local row about remote work that does not exist"
        );
    }
}

#[test]
fn accepted_and_duplicate_carry_the_same_provenance_and_a_bounded_sorted_job_set() {
    let submission = submission();
    let vectors = fixture("job-sets.json");
    for set in vectors["sets"].as_array().expect("sets are a list") {
        let name = set["name"].as_str().expect("a name");
        let identifiers: Vec<String> = set["identifiers"]
            .as_array()
            .expect("identifiers are a list")
            .iter()
            .map(|value| value.as_str().expect("an identifier").to_owned())
            .collect();
        let acceptable = set["acceptable"].as_bool().expect("an expectation");
        let mut acknowledgement = acknowledgement_of(&submission, &[]);
        acknowledgement.physical_sling_job_identifiers = identifiers.clone();
        let accepted = submission.interpret(&exchange_of(acknowledgement.clone()));
        acknowledgement.already_accepted = true;
        let duplicate = submission.interpret(&exchange_of(acknowledgement));
        if acceptable {
            assert_eq!(
                accepted,
                SubmissionOutcome::Accepted {
                    physical_sling_job_identifiers: identifiers.clone(),
                    remaining_retention_milliseconds: GRANTED_RETENTION - PROMPT_ELAPSED,
                }
            );
            assert_eq!(
                duplicate,
                SubmissionOutcome::Duplicate {
                    physical_sling_job_identifiers: identifiers,
                    remaining_retention_milliseconds: GRANTED_RETENTION - PROMPT_ELAPSED,
                }
            );
            assert!(accepted.provably_recorded() && duplicate.provably_recorded());
        } else {
            assert!(
                accepted.requires_reconciliation() && duplicate.requires_reconciliation(),
                "{name}: a set this daemon cannot act on settles nothing"
            );
        }
    }
}

#[test]
fn a_job_set_beyond_its_bound_is_not_an_answer() {
    let submission = submission();
    let contract = AuthorAgentTransportContract::embedded();
    let allowed = contract.limit("maximum_physical_sling_job_matches");
    let overlong: Vec<String> =
        (0..=allowed).map(|position| format!("sling-job-{position:04}")).collect();
    let mut acknowledgement = acknowledgement_of(&submission, &[]);
    acknowledgement.physical_sling_job_identifiers = overlong;
    assert!(submission.interpret(&exchange_of(acknowledgement.clone())).requires_reconciliation());
    let name_bound = contract.limit("maximum_sling_job_identifier_bytes") as usize;
    acknowledgement.physical_sling_job_identifiers = vec!["j".repeat(name_bound + 1)];
    assert!(submission.interpret(&exchange_of(acknowledgement)).requires_reconciliation());
}

#[test]
fn only_a_matching_retired_answer_maps_to_an_expired_recovery_window() {
    let submission = submission();
    let mut acknowledgement = acknowledgement_of(&submission, &[]);
    acknowledgement.retired = true;
    assert_eq!(
        submission.interpret(&exchange_of(acknowledgement.clone())),
        SubmissionOutcome::RecoveryWindowExpired
    );
    acknowledgement.physical_sling_job_identifiers = vec!["sling-job-alpha".to_owned()];
    assert_eq!(
        submission.interpret(&exchange_of(acknowledgement)),
        SubmissionOutcome::SubmissionUnknown { cause: UnknownCause::Body },
        "an agent that retired the work and still names jobs doing it is not answering"
    );
}

#[test]
fn a_capacity_refusal_names_its_closed_discriminator_and_settles_as_nonexecution() {
    let submission = submission();
    let discriminators = [
        CapacityDiscriminator::Artifact,
        CapacityDiscriminator::Event,
        CapacityDiscriminator::ExecutionDetail,
        CapacityDiscriminator::Operation,
        CapacityDiscriminator::Result,
        CapacityDiscriminator::Snapshot,
        CapacityDiscriminator::Subscription,
    ];
    for discriminator in discriminators {
        let mut acknowledgement = acknowledgement_of(&submission, &[]);
        acknowledgement.non_execution = Some(NonExecution::Capacity(discriminator));
        let mut exchange = exchange_of(acknowledgement);
        exchange.status = REJECTION_STATUS;
        let outcome = submission.interpret(&exchange);
        assert_eq!(
            outcome,
            SubmissionOutcome::AuthoritativeNonExecution {
                non_execution: NonExecution::Capacity(discriminator),
            }
        );
        assert!(outcome.provably_not_recorded());
        assert!(!outcome.provably_recorded() && !outcome.requires_reconciliation());
    }
}

#[test]
fn a_refusal_the_status_does_not_agree_with_settles_nothing() {
    let submission = submission();
    let mut acknowledgement = acknowledgement_of(&submission, &[]);
    acknowledgement.non_execution = Some(NonExecution::Semantic);
    assert_eq!(
        submission.interpret(&exchange_of(acknowledgement)),
        SubmissionOutcome::SubmissionUnknown { cause: UnknownCause::UnvalidatedStatus },
        "an agent refusing under an accepting status is telling this daemon two things"
    );
    let mut rejected = exchange_of(acknowledgement_of(&submission, &["sling-job-alpha"]));
    rejected.status = REJECTION_STATUS;
    assert_eq!(
        submission.interpret(&rejected),
        SubmissionOutcome::SubmissionUnknown { cause: UnknownCause::Body },
        "a rejection naming no closed refusal proves nothing about what was reserved"
    );
}

#[test]
fn a_conflict_is_not_a_retry_and_a_retry_waits_a_bounded_time() {
    let submission = submission();
    let mut conflicting = exchange_of(acknowledgement_of(&submission, &[]));
    conflicting.status = CONFLICT_STATUS;
    assert_eq!(submission.interpret(&conflicting), SubmissionOutcome::Conflict);
    assert!(!submission.interpret(&conflicting).provably_not_recorded());

    let mut retrying = exchange_of(acknowledgement_of(&submission, &[]));
    retrying.status = RETRYABLE_STATUS;
    let base = AuthorAgentTransportContract::embedded().limit("retry_base_milliseconds");
    assert_eq!(
        submission.interpret(&retrying),
        SubmissionOutcome::RetryAfter { milliseconds: base }
    );
    retrying.retry_after_milliseconds = Some(REQUESTED_RETRY_DELAY);
    assert_eq!(
        submission.interpret(&retrying),
        SubmissionOutcome::RetryAfter { milliseconds: REQUESTED_RETRY_DELAY }
    );
    retrying.retry_after_milliseconds = Some(EXCESSIVE_RETRY_DELAY);
    let cap = AuthorAgentTransportContract::embedded().limit("retry_after_cap_milliseconds");
    assert_eq!(
        submission.interpret(&retrying),
        SubmissionOutcome::RetryAfter { milliseconds: cap },
        "without a cap a server parks a client for as long as it likes by asking"
    );
}

#[test]
fn retention_is_reduced_from_request_start_and_an_exhausted_window_is_expired() {
    assert_eq!(
        remaining_retention_milliseconds(GRANTED_RETENTION, PROMPT_ELAPSED),
        Some(GRANTED_RETENTION - PROMPT_ELAPSED)
    );
    assert_eq!(
        remaining_retention_milliseconds(GRANTED_RETENTION, DELAYED_ELAPSED),
        Some(GRANTED_RETENTION - DELAYED_ELAPSED),
        "a slow answer spends the retention it describes"
    );
    assert_eq!(
        remaining_retention_milliseconds(GRANTED_RETENTION, GRANTED_RETENTION),
        None,
        "zero remaining time promises nothing, so equality is expired rather than a window"
    );
    let cap = AuthorAgentTransportContract::embedded()
        .limit("maximum_persisted_remaining_retention_milliseconds");
    assert_eq!(remaining_retention_milliseconds(cap + GRANTED_RETENTION, 0), Some(cap));

    let submission = submission();
    let mut acknowledgement = acknowledgement_of(&submission, &["sling-job-alpha"]);
    acknowledgement.granted_retention_milliseconds = PROMPT_ELAPSED;
    assert_eq!(
        submission.interpret(&exchange_of(acknowledgement)),
        SubmissionOutcome::SubmissionUnknown { cause: UnknownCause::Retention }
    );
}

#[test]
fn only_a_failure_before_the_first_request_byte_confirms_nothing_executed() {
    let before = [
        Checkpoint::NameResolution,
        Checkpoint::TransportConnect,
        Checkpoint::TransportLayerSecurity,
    ];
    let after = [
        Checkpoint::RequestHead,
        Checkpoint::RequestBody,
        Checkpoint::ResponseHead,
        Checkpoint::ResponseBody,
    ];
    for checkpoint in before {
        assert!(!checkpoint.bytes_may_have_reached_author());
        assert_eq!(
            Submission::transport_failure(checkpoint),
            SubmissionOutcome::ConfirmedNotExecuted { checkpoint }
        );
    }
    for checkpoint in after {
        assert!(checkpoint.bytes_may_have_reached_author());
        assert_eq!(
            Submission::transport_failure(checkpoint),
            SubmissionOutcome::SubmissionUnknown { cause: UnknownCause::Deadline(checkpoint) },
            "after a byte may have arrived, a timeout says nothing about what was recorded"
        );
    }
    for checkpoint in before.iter().chain(after.iter()) {
        assert!(
            checkpoint.deadline_milliseconds() > 0,
            "every phase is bounded on its own, because a stalled handshake is not a slow server"
        );
    }
}

#[test]
fn a_generation_change_blocks_recovery_rather_than_pairing_an_old_identifier_with_a_new_one() {
    let submission = submission();
    assert!(
        submission
            .require_recoverable(&RecoveryPreconditions {
                continuation_authority_ready: true,
                current_generation: GENERATION,
            })
            .is_ok()
    );
    assert!(matches!(
        submission.require_recoverable(&RecoveryPreconditions {
            continuation_authority_ready: true,
            current_generation: LATER_GENERATION,
        }),
        Err(SubmissionRefusal::GenerationChanged { .. })
    ));
    assert!(
        matches!(
            submission.require_recoverable(&RecoveryPreconditions {
                continuation_authority_ready: false,
                current_generation: GENERATION,
            }),
            Err(SubmissionRefusal::ContinuationAuthorityAbsent)
        ),
        "without an authority to issue continuations no lookup can settle an unknown submission"
    );
}

#[test]
fn the_same_local_identifiers_under_another_target_derive_another_partition() {
    let expected = expected_for("query_paths");
    let here = submission();
    let elsewhere = Submission::build(
        &expected,
        operation_in(ANOTHER_TARGET, GENERATION),
        SUBSCRIPTION,
        ARGUMENTS,
        ExpectedArtifactManifest::empty(),
    )
    .expect("these arguments fit one submission");
    assert_ne!(
        here.operation.agent_operation_identifier, elsewhere.operation.agent_operation_identifier,
        "reusing a profile against another author must not name the prior author's work"
    );
    assert_eq!(
        here.interpret(&exchange_of(acknowledgement_of(&elsewhere, &["sling-job-alpha"]))),
        SubmissionOutcome::SubmissionUnknown { cause: UnknownCause::Identity }
    );
}

#[test]
fn a_continuation_carries_its_token_alone_and_an_initial_page_carries_its_window() {
    let initial = DiscoveryShape::Initial { limit: DECLARED_ARTIFACT_ROWS, offset: 0 };
    initial.require_wellformed().expect("an initial page names its window");
    let named: Vec<&str> = initial.query_pairs().iter().map(|(name, _)| *name).collect();
    assert_eq!(named, vec!["offset", "limit"]);

    let continuation = DiscoveryShape::continuing("opaque-token-bytes", DECLARED_ARTIFACT_ROWS);
    continuation.require_wellformed().expect("a continuation names only its token");
    assert_eq!(continuation.query_pairs(), vec![("continuation", "opaque-token-bytes".to_owned())]);
    assert_eq!(
        continuation,
        DiscoveryShape::Continuation {
            originating_limit: DECLARED_ARTIFACT_ROWS,
            token: "opaque-token-bytes".to_owned(),
        },
        "a continuation preserves its opaque bytes and inherits the limit it was issued under"
    );
}

#[test]
fn nothing_this_module_reports_carries_a_credential_or_a_response_body() {
    let submission = submission();
    let mut acknowledgement = acknowledgement_of(&submission, &["sling-job-alpha"]);
    acknowledgement.submitted_command_digest = SUBSTITUTED_DIGEST.to_owned();
    let mut exchange = exchange_of(acknowledgement);
    exchange.media_type = format!("{WRONG_MEDIA_TYPE}; note={RESPONSE_BODY}");
    let reported = vec![
        format!("{:?}", submission.interpret(&exchange)),
        format!("{:?}", Submission::transport_failure(Checkpoint::ResponseHead)),
        format!(
            "{}",
            submission.request_headers(None, AUTHOR, NOW, &[]).expect_err("no token is a refusal")
        ),
        format!(
            "{}",
            submission
                .request_headers(
                    Some(&token()),
                    AUTHOR,
                    NOW,
                    &[(TOKEN_HEADER.to_owned(), TOKEN_VALUE.to_owned())]
                )
                .expect_err("a caller may not set a derived header")
        ),
    ];
    for line in reported {
        assert!(!line.contains(TOKEN_VALUE), "a credential must never reach a log: {line}");
        assert!(!line.contains(RESPONSE_BODY), "a remote string must never reach a log: {line}");
    }
}
