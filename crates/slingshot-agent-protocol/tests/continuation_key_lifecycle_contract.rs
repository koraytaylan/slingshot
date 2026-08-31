//! Tokens, the keys that sign them, and what a rotation must not strand.
//!
//! Two properties carry the suite. A token cannot be steered by anything a
//! client controls: integrity is checked before the token's own claims are
//! compared against anything, so a forged token never reaches a comparison
//! against the data it named. And a rotation retains the previous key long
//! enough that a token issued a moment before it is still honoured, because
//! somebody is holding that token right now.

use serde_json::Value;
use slingshot_agent_protocol::continuation_key_authority::{
    KeyRing, KeyRingFailure, ValidatingKey,
};
use slingshot_agent_protocol::continuation_token::{
    ContinuationRefusal, ContinuationState, ContinuationToken,
};
use slingshot_domain::author_agent_transport_contract::AuthorAgentTransportContract;

/// The generated schema manifest this test reads.
const MANIFEST: &str = include_str!("fixtures/continuation-key-lifecycle/manifest.json");

/// Two-character pairs in a sixty-four-character hexadecimal value.
const DIGEST_PAIRS: usize = 32;

/// Directories between this crate's manifest and the workspace root.
const WORKSPACE_ROOT_ANCESTORS: usize = 2;

/// One instant, for a test that does not care which.
const NOW: u64 = 1_700_000_000_000;

/// How long after now the fixture tokens expire.
const TOKEN_LIFETIME: u64 = 900_000;

/// The generation these fixtures run against.
const GENERATION: u64 = 1;

/// Where in a query's results the fixture tokens resume.
const POSITION: u64 = 40;

/// Returns a sixty-four-character value made of one repeated pair.
fn digest(pair: &str) -> String {
    pair.repeat(DIGEST_PAIRS)
}

/// Returns the state the fixture tokens carry.
fn state() -> ContinuationState {
    ContinuationState {
        author_target_identity_digest: digest("1d"),
        agent_event_store_generation: GENERATION,
        query_digest: digest("40"),
        position: POSITION,
        expires_at_unix_milliseconds: NOW + TOKEN_LIFETIME,
    }
}

/// Returns a ring holding one key.
fn ring() -> KeyRing {
    KeyRing::initial(&digest("ce"))
}

/// Honours one token against `ring`, at `now`.
fn validate(
    token: &ContinuationToken,
    ring: &KeyRing,
    now: u64,
) -> Result<ValidatingKey, ContinuationRefusal> {
    token.validate(ring, &digest("1d"), &digest("40"), GENERATION, now)
}

#[test]
fn a_token_this_agent_issued_is_honoured_under_the_key_that_signed_it() {
    let ring = ring();
    let token = ContinuationToken::issue(state(), &ring.current);
    assert_eq!(
        validate(&token, &ring, NOW).expect("a token this agent issued"),
        ValidatingKey::Current
    );
    assert!(token.is_bounded(), "and it fits the bound the transport contract names");
    assert_eq!(token.unvalidated_state().position, POSITION, "saying where to resume");
}

#[test]
fn integrity_is_checked_before_anything_the_token_claims() {
    let ring = ring();
    let forged = ContinuationToken::issue(
        ContinuationState { author_target_identity_digest: digest("2d"), ..state() },
        &digest("ff"),
    );
    assert_eq!(
        validate(&forged, &ring, NOW),
        Err(ContinuationRefusal::IntegrityInvalid),
        "a token nobody's key signs is refused before its claims are compared against anything"
    );
    assert!(
        ContinuationRefusal::IntegrityInvalid < ContinuationRefusal::WrongTarget,
        "and the precedence is in the type, so it cannot drift with the order of the checks"
    );
    assert!(ContinuationRefusal::IntegrityInvalid < ContinuationRefusal::Expired);
}

#[test]
fn a_token_signed_correctly_but_naming_something_else_is_refused_for_that_reason() {
    let ring = ring();
    let cases = [
        (
            ContinuationState { author_target_identity_digest: digest("2d"), ..state() },
            ContinuationRefusal::WrongTarget,
        ),
        (
            ContinuationState { query_digest: digest("41"), ..state() },
            ContinuationRefusal::WrongQuery,
        ),
        (
            ContinuationState { agent_event_store_generation: GENERATION + 1, ..state() },
            ContinuationRefusal::WrongGeneration,
        ),
    ];
    for (held, expected) in cases {
        let token = ContinuationToken::issue(held, &ring.current);
        assert_eq!(
            validate(&token, &ring, NOW),
            Err(expected),
            "resuming somewhere other than where a token names is worse than starting over"
        );
    }
}

#[test]
fn a_token_that_has_run_out_is_refused_even_though_it_is_genuine() {
    let ring = ring();
    let token = ContinuationToken::issue(state(), &ring.current);
    validate(&token, &ring, NOW + TOKEN_LIFETIME - 1).expect("one millisecond before it expires");
    assert_eq!(
        validate(&token, &ring, NOW + TOKEN_LIFETIME),
        Err(ContinuationRefusal::Expired),
        "and exactly at its expiry, because a bound that included its own instant would not be one"
    );
}

#[test]
fn a_rotation_keeps_honouring_the_token_somebody_is_already_holding() {
    let ring = ring();
    let issued = ContinuationToken::issue(state(), &ring.current);
    let rotated = ring.rotated(&digest("cf"), NOW).expect("a rotation");

    assert_eq!(
        validate(&issued, &rotated, NOW + 1).expect("a token issued a moment before"),
        ValidatingKey::Prior,
        "which is a token to honour, and a signal that a rotation is under way"
    );
    let fresh = ContinuationToken::issue(state(), &rotated.current);
    assert_eq!(
        validate(&fresh, &rotated, NOW + 1).expect("a token issued after"),
        ValidatingKey::Current
    );

    let retention = AuthorAgentTransportContract::embedded()
        .limit("continuation_key_prior_retention_milliseconds");
    assert_eq!(
        validate(&issued, &rotated, NOW + retention),
        Err(ContinuationRefusal::IntegrityInvalid),
        "once the retention has passed, the old key is gone rather than merely disfavoured"
    );
}

#[test]
fn a_second_rotation_inside_one_retention_window_is_refused() {
    let ring = ring();
    let rotated = ring.rotated(&digest("cf"), NOW).expect("a first rotation");
    let refused = rotated.rotated(&digest("d0"), NOW + 1);
    assert!(
        matches!(refused, Err(KeyRingFailure::PriorStillRetained { .. })),
        "two rotations inside one window would strand every token issued under the key that \
         fell off the end: {refused:?}"
    );

    let retention = AuthorAgentTransportContract::embedded()
        .limit("continuation_key_prior_retention_milliseconds");
    rotated
        .rotated(&digest("d0"), NOW + retention)
        .expect("while a rotation after the window is ordinary");
}

#[test]
fn both_key_bounds_come_from_the_manifest_and_refuse_at_them() {
    let contract = AuthorAgentTransportContract::embedded();
    let key = usize::try_from(contract.limit("maximum_agent_continuation_key_state_bytes"))
        .expect("a countable bound");
    let record = usize::try_from(contract.limit("maximum_continuation_key_authority_record_bytes"))
        .expect("a countable bound");

    KeyRing::initial(&"k".repeat(key)).require_bounded().expect("the largest key");
    let over = KeyRing::initial(&"k".repeat(key + 1)).require_bounded();
    assert!(
        matches!(over, Err(KeyRingFailure::KeyTooLong { .. })),
        "one byte further in a key: {over:?}"
    );

    let ring = KeyRing {
        current: "k".repeat(key),
        prior: Some("k".repeat(key)),
        prior_expires_at_unix_milliseconds: NOW,
    };
    let refused = ring.require_bounded();
    assert!(
        matches!(refused, Err(KeyRingFailure::RecordTooLong { .. })),
        "and two largest keys cross the record bound, which is smaller than twice one key: \
         {record} against {}",
        key * 2
    );
}

#[test]
fn an_absent_ring_is_a_deployment_nobody_prepared() {
    assert_eq!(
        KeyRingFailure::Absent.to_string(),
        "this deployment holds no continuation key ring, and one is not created implicitly",
        "a caller finding an empty ring would issue keys into it; one finding nothing looks at why"
    );
}

#[test]
fn every_generated_schema_regenerates_to_the_bytes_the_manifest_records() {
    let manifest: Value = serde_json::from_str(MANIFEST).expect("the manifest is one value");
    let schemas = manifest["schemas"].as_object().expect("a schema list");
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(WORKSPACE_ROOT_ANCESTORS)
        .expect("the workspace root")
        .join("schemas/agent-protocol");

    for (relative, recorded) in schemas {
        let bytes = std::fs::read(root.join(relative)).expect("a generated schema reads");
        let digest: String = <sha2::Sha256 as sha2::Digest>::digest(&bytes)
            .iter()
            .map(|octet| format!("{octet:02x}"))
            .collect();
        assert_eq!(Some(digest.as_str()), recorded.as_str(), "{relative} is not what was recorded");
    }
}
