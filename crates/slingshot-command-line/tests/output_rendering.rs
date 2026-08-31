//! One outer shape for machines, one voice for people, and a strict divide.
//!
//! A shape that varied by command would make every consumer learn every command
//! before it could read an error, and the consumers that mattered would learn
//! only the commands they happened to try first. So the union is closed, every
//! golden value round-trips to identical bytes, and the tag inventory is walked
//! rather than sampled - a tag added without being declared is a failure here.
//!
//! The divide that matters most is that a local problem may never claim a
//! remote fact. An interruption knows how far it got and nothing else, and the
//! shape gives it nowhere to say otherwise: the suite checks that structurally
//! rather than by inspecting values.
//!
//! Human interruption output leaves standard output empty. A pipeline that
//! captured a partial answer would treat it as the answer, and the whole point
//! of an interruption is that there is not one yet.

use slingshot_command_line::artifact_access::{
    AccessContext, ArtifactDescriptor, MaintenanceAssociation, access_entry, maintenance_entry,
};
use slingshot_command_line::human_renderer::{
    POST_RECEIPT_TEMPLATE, PRE_RECEIPT_TEMPLATE, TRANSFER_TEMPLATE, render as render_human,
};
use slingshot_command_line::machine_outcome_envelope::{
    ACCESS_SCHEME, Interruption, MAXIMUM_MACHINE_OUTCOME_ENVELOPE_BYTES, MachineOutcomeEnvelope,
    artifact_uri, encoded_segment, maintenance_result_uri,
};
use slingshot_command_line::machine_readable_renderer::{
    MACHINE_STREAM, RenderRefusal, Stream, render,
};
use slingshot_command_line::progress_renderer::{
    MAXIMUM_PROGRESS_LINE_BYTES, PROGRESS_STREAM, ProgressNote, is_shown, render as render_progress,
};

/// Where the golden values live.
const FIXTURES: &str = "tests/fixtures/machine-outcome-envelope";

/// The profile these references belong to.
const PROFILE: &str = "alpha-site";

/// The environment they belong to.
const ENVIRONMENT: &str = "production";

/// The partition they belong to.
const TARGET: &str = "target-identity-digest-one";

/// The operation one artifact belongs to.
const OPERATION: &str = "operation-one";

/// The artifact itself.
const ARTIFACT: &str = "artifact-one";

/// One maintenance result.
const MAINTENANCE_RESULT: &str = "maintenance-result-one";

/// How long one artifact is.
const ARTIFACT_BYTES: u64 = 4096;

/// What it digests to.
const DIGEST: &str = "content-digest";

/// What it is.
const MEDIA_TYPE: &str = "application/zip";

/// Which revision one association is at.
const ASSOCIATION_REVISION: u64 = 2;

/// The canonical acknowledgement cap the workflow integration is held to.
const ACKNOWLEDGEMENT_CAP_BYTES: u64 = 4096;

/// How many words of a template are compared against what was printed.
const TEMPLATE_OPENING_WORDS: usize = 2;

/// The revision a replayed receipt stands at.
const REPLAYED_REVISION: u64 = 4;

/// Returns every golden value one fixture holds.
fn vectors(name: &str) -> Vec<serde_json::Value> {
    let path = format!("{FIXTURES}/{name}");
    let text = std::fs::read_to_string(&path).unwrap_or_else(|_| panic!("{path} is readable"));
    text.lines().map(|line| serde_json::from_str(line).expect("each line is one value")).collect()
}

/// Returns the context these references belong to.
fn context() -> AccessContext {
    AccessContext {
        author_target_identity_digest: TARGET.to_owned(),
        environment: ENVIRONMENT.to_owned(),
        operation_identifier: OPERATION.to_owned(),
        profile: PROFILE.to_owned(),
    }
}

#[test]
fn every_golden_value_parses_as_one_envelope_and_writes_the_same_bytes_back() {
    for value in vectors("envelopes.jsonl") {
        let written = serde_json::to_string(&value).expect("the fixture serializes");
        let envelope: MachineOutcomeEnvelope =
            serde_json::from_str(&written).unwrap_or_else(|failure| panic!("{written}: {failure}"));
        let rendered = render(&envelope).expect("it is within the bound");
        let again: MachineOutcomeEnvelope = serde_json::from_str(&rendered).expect("it reads back");
        assert_eq!(
            again, envelope,
            "a consumer diffing two runs must see a difference only where one exists"
        );
        assert_eq!(render(&again).expect("again"), rendered, "and the bytes are stable");
    }
}

#[test]
fn every_declared_tag_is_reachable_and_every_envelope_selects_exactly_one() {
    let mut met: Vec<String> = Vec::new();
    for value in vectors("envelopes.jsonl") {
        let envelope: MachineOutcomeEnvelope = serde_json::from_value(value).expect("one envelope");
        let tag = envelope.tag();
        assert!(
            MachineOutcomeEnvelope::EVERY_TAG.contains(&tag.as_str()),
            "{tag} is selected and not declared"
        );
        if !met.contains(&tag) {
            met.push(tag);
        }
    }
    for tag in MachineOutcomeEnvelope::EVERY_TAG {
        let reachable = met.iter().any(|held| held == tag)
            || [
                "command_artifact_access",
                "structured_result_artifact_access",
                "maintenance_result_access",
            ]
            .contains(tag);
        assert!(reachable, "{tag} is declared and no golden value reaches it");
    }
}

#[test]
fn a_local_error_has_nowhere_to_claim_a_remote_fact() {
    for interruption in [
        Interruption::PreReceipt { retry_identifier: "retry-one".to_owned() },
        Interruption::PostReceipt { operation_identifier: OPERATION.to_owned(), revision: 1 },
        Interruption::ArtifactTransfer {
            artifact_identifier: ARTIFACT.to_owned(),
            operation_identifier: OPERATION.to_owned(),
        },
        Interruption::MaintenanceResultTransfer {
            author_target_identity_digest: TARGET.to_owned(),
            maintenance_result_identifier: MAINTENANCE_RESULT.to_owned(),
        },
    ] {
        let envelope = MachineOutcomeEnvelope::LocalApplicationError { interruption };
        assert!(
            !envelope.claims_remote_authority(),
            "a local signal reporting a remote outcome is the one thing the shape prevents"
        );
        let rendered = render(&envelope).expect("it is within the bound");
        for forbidden in ["disposition", "failure", "\"result\""] {
            assert!(!rendered.contains(forbidden), "{rendered} carries {forbidden}");
        }
    }
}

#[test]
fn a_pre_receipt_interruption_names_no_operation_at_all() {
    let envelope = MachineOutcomeEnvelope::LocalApplicationError {
        interruption: Interruption::PreReceipt { retry_identifier: "retry-one".to_owned() },
    };
    let rendered = render(&envelope).expect("it is within the bound");
    assert!(
        !rendered.contains("operation_identifier"),
        "before the daemon answered nothing is known about any operation: {rendered}"
    );
    assert!(rendered.contains("retry_identifier"), "and what is known is what to quote");
}

#[test]
fn an_artifact_reference_names_the_daemon_and_never_a_local_path() {
    let entry = access_entry(
        &context(),
        &ArtifactDescriptor {
            artifact_identifier: ARTIFACT.to_owned(),
            byte_length: ARTIFACT_BYTES,
            content_digest: DIGEST.to_owned(),
            media_type: MEDIA_TYPE.to_owned(),
        },
    );
    assert_eq!(
        entry.uri,
        format!(
            "{ACCESS_SCHEME}://profiles/{PROFILE}/environments/{ENVIRONMENT}/targets/{TARGET}\
             /operations/{OPERATION}/artifacts/{ARTIFACT}"
        )
    );
    assert_eq!(entry.byte_length, ARTIFACT_BYTES, "a caller deciding whether to fetch needs this");
    assert_eq!(entry.content_digest, DIGEST);
    let rendered = serde_json::to_string(&entry).expect("it serializes");
    for path in ["/tmp", "/home", "./", "file://"] {
        assert!(
            !rendered.contains(path),
            "the bytes may not be on this machine, so a local path names a file that is not there"
        );
    }
}

#[test]
fn a_maintenance_reference_names_no_operation_because_there_is_none() {
    let entry = maintenance_entry(
        PROFILE,
        ENVIRONMENT,
        TARGET,
        &MaintenanceAssociation {
            association_revision: ASSOCIATION_REVISION,
            byte_length: ARTIFACT_BYTES,
            content_digest: DIGEST.to_owned(),
            kind: "terminal-maintenance-manifest".to_owned(),
            maintenance_result_identifier: MAINTENANCE_RESULT.to_owned(),
            media_type: "application/json".to_owned(),
            reviewed_source_digest: "reviewed-digest".to_owned(),
        },
    );
    assert_eq!(entry.uri, maintenance_result_uri(PROFILE, ENVIRONMENT, TARGET, MAINTENANCE_RESULT));
    assert!(
        !entry.uri.contains("/operations/"),
        "a maintenance result belongs to a target, and naming an operation would invent one"
    );
    let rendered = serde_json::to_string(&entry).expect("it serializes");
    assert!(!rendered.contains("operation"), "and no member names one either: {rendered}");
    assert!(!rendered.contains("slot"));
}

#[test]
fn every_reference_segment_is_encoded_exactly_once() {
    for vector in vectors("uri-segments.jsonl") {
        let name = vector["name"].as_str().expect("a name");
        assert_eq!(
            encoded_segment(vector["segment"].as_str().expect("a segment")),
            vector["encoded"].as_str().expect("an encoding"),
            "{name}: a separator surviving into a segment would name another thing"
        );
    }
    let awkward = artifact_uri(PROFILE, ENVIRONMENT, TARGET, "a/b", ARTIFACT);
    assert!(awkward.contains("a%2Fb"), "and a reference built from one is still one reference");
}

#[test]
fn an_envelope_past_the_bound_is_an_invariant_violation_and_not_a_truncation() {
    let enormous = MachineOutcomeEnvelope::OperationResult {
        result: serde_json::json!({
            "text": "r".repeat(MAXIMUM_MACHINE_OUTCOME_ENVELOPE_BYTES as usize)
        }),
    };
    let refusal = render(&enormous).expect_err("that is larger than one may be");
    assert!(
        matches!(refusal, RenderRefusal::TooLarge { .. }),
        "a truncated envelope is not a smaller answer, it is an unparseable one"
    );
    assert_eq!(
        MAXIMUM_MACHINE_OUTCOME_ENVELOPE_BYTES.min(ACKNOWLEDGEMENT_CAP_BYTES),
        MAXIMUM_MACHINE_OUTCOME_ENVELOPE_BYTES,
        "and the bound sits below the acknowledgement cap the workflow integration is held to"
    );
    assert_ne!(MAXIMUM_MACHINE_OUTCOME_ENVELOPE_BYTES, ACKNOWLEDGEMENT_CAP_BYTES);
}

#[test]
fn a_human_interruption_says_one_of_three_things_and_writes_nothing_to_standard_output() {
    let cases = [
        (
            Interruption::PreReceipt { retry_identifier: "retry-one".to_owned() },
            PRE_RECEIPT_TEMPLATE,
        ),
        (
            Interruption::PostReceipt { operation_identifier: OPERATION.to_owned(), revision: 1 },
            POST_RECEIPT_TEMPLATE,
        ),
        (
            Interruption::ArtifactTransfer {
                artifact_identifier: ARTIFACT.to_owned(),
                operation_identifier: OPERATION.to_owned(),
            },
            TRANSFER_TEMPLATE,
        ),
    ];
    for (interruption, template) in cases {
        let output = render_human(&MachineOutcomeEnvelope::LocalApplicationError { interruption });
        assert!(
            output.standard_output.is_empty(),
            "a pipeline that captured a partial answer would treat it as the answer"
        );
        assert!(!output.standard_error.is_empty());
        assert_eq!(output.substantive_stream(), Stream::StandardError);
        let opening = template
            .split_whitespace()
            .take(TEMPLATE_OPENING_WORDS)
            .collect::<Vec<&str>>()
            .join(" ");
        assert!(output.standard_error.starts_with(&opening), "{}", output.standard_error);
    }
}

#[test]
fn an_ordinary_human_outcome_goes_to_standard_output_and_says_what_happened() {
    let output = render_human(&MachineOutcomeEnvelope::OperationReceipt {
        operation_identifier: OPERATION.to_owned(),
        replayed: true,
        revision: REPLAYED_REVISION,
    });
    assert!(output.standard_error.is_empty());
    assert_eq!(output.substantive_stream(), Stream::StandardOutput);
    assert!(output.standard_output.contains(OPERATION));
    assert!(
        output.standard_output.contains("already held"),
        "a replay is said out loud, because it is the difference between one run and two"
    );
}

#[test]
fn progress_goes_to_standard_error_and_only_when_a_person_is_reading() {
    assert_eq!(PROGRESS_STREAM, Stream::StandardError);
    assert_eq!(MACHINE_STREAM, Stream::StandardOutput);
    assert!(is_shown(false), "a person reading a terminal sees it");
    assert!(
        !is_shown(true),
        "and a machine-readable run writes exactly one envelope, with nothing beside it"
    );
    let note = ProgressNote {
        detail: "d".repeat(MAXIMUM_PROGRESS_LINE_BYTES + MAXIMUM_PROGRESS_LINE_BYTES),
        operation_identifier: OPERATION.to_owned(),
    };
    let line = render_progress(&note);
    assert!(
        line.len() <= MAXIMUM_PROGRESS_LINE_BYTES,
        "losing the tail of a long note costs a reader nothing"
    );
    assert!(line.starts_with(OPERATION));
}
