//! The pagination contract every discovery command shares.
//!
//! Two things are proved here and they are different in kind. The window itself
//! is Rust: it is built, refused, serialized, and read back. The continuation
//! token is not: this crate holds no key and mints nothing, so what is proved
//! about it is that the contract an agent must implement is written down
//! exactly - which members, in which order, under which bounds, and which
//! failure wins when two things are wrong at once.
//!
//! The second kind is deliberately recomputed rather than read. Every envelope
//! size, every lifetime verdict, and every precedence answer in the fixture is
//! derived again here from the manifest constants and compared, so the fixture
//! cannot drift into agreeing with itself.

use serde_json::Value;
use slingshot_domain::command::result_window::{
    CONTINUATION_FAILURE_MEMBER, CONTINUATION_FAILURE_PRECEDENCE, CONTINUATION_HEADER_MEMBERS,
    CONTINUATION_MODE, CONTINUATION_PAYLOAD_MEMBERS, CONTINUATION_TAG_BYTES,
    CONTINUATION_TOKEN_ALGORITHM, CONTINUATION_TOKEN_FORMAT, CONTINUATION_TOKEN_SEGMENTS,
    ContinuationToken, DIGEST_HEXADECIMAL_CHARACTERS, INITIAL_MODE, RESUME_SORT_KEY_MEMBER,
    ResultLimit, ResultOffset, ResultWindow, WindowFailure,
    continuation_token_lifetime_milliseconds, default_result_limit,
    maximum_continuation_key_identifier_bytes, maximum_continuation_resume_key_canonical_bytes,
    maximum_continuation_token_bytes, maximum_continuation_token_clock_skew_milliseconds,
    maximum_result_limit, maximum_result_offset,
};

/// Vectors this test reads.
const FIXTURE: &str = include_str!("fixtures/commands/result-window.jsonl");

/// Bytes base64url spends on three input bytes.
const BASE64URL_GROUP_OUTPUT: usize = 4;

/// Bytes base64url consumes per group.
const BASE64URL_GROUP_INPUT: usize = 3;

/// Reads one row's string member.
fn text<'row>(row: &'row Value, member: &str) -> &'row str {
    row[member].as_str().unwrap_or_else(|| panic!("{member} is a string in {row}"))
}

/// Reads one row's unsigned member.
fn number(row: &Value, member: &str) -> u64 {
    row[member].as_u64().unwrap_or_else(|| panic!("{member} is an unsigned integer in {row}"))
}

/// Returns every fixture row of one kind.
fn rows(kind: &str) -> Vec<Value> {
    FIXTURE
        .lines()
        .map(|line| serde_json::from_str::<Value>(line).expect("every fixture line is one object"))
        .filter(|row| text(row, "kind") == kind)
        .collect()
}

/// Returns how many bytes unpadded base64url spends on `byte_length`.
fn unpadded_base64url_length(byte_length: usize) -> usize {
    (byte_length * BASE64URL_GROUP_OUTPUT).div_ceil(BASE64URL_GROUP_INPUT)
}

/// Every refusal the fixture can name, beside the variant that produces it.
///
/// The fixture names a variant rather than a sentence so the sentence stays
/// free to improve without rewriting thirty vectors.
const DECLARED_REFUSALS: &[(&str, WindowFailure)] = &[
    ("UnknownMode", WindowFailure::UnknownMode),
    ("LimitZero", WindowFailure::LimitZero),
    ("LimitAboveMaximum", WindowFailure::LimitAboveMaximum),
    ("OffsetAboveMaximum", WindowFailure::OffsetAboveMaximum),
    ("InitialIncomplete", WindowFailure::InitialIncomplete),
    ("ContinuationNotAlone", WindowFailure::ContinuationNotAlone),
    ("ContinuationIncomplete", WindowFailure::ContinuationIncomplete),
    ("TokenEmpty", WindowFailure::TokenEmpty),
    ("TokenControlCharacter", WindowFailure::TokenControlCharacter),
    ("TokenTooLong", WindowFailure::TokenTooLong),
];

/// Name the fixture gives to the refusals the closed object makes on its own.
const CLOSED_OBJECT: &str = "ClosedObject";

/// Returns the rendering the named refusal produces.
fn refusal_rendering(reason: &str) -> Option<String> {
    if reason == CLOSED_OBJECT {
        return None;
    }
    let failure = DECLARED_REFUSALS
        .iter()
        .find(|(name, _)| *name == reason)
        .map(|(_, failure)| *failure)
        .unwrap_or_else(|| panic!("the fixture names a refusal this test does not know: {reason}"));
    Some(failure.to_string())
}

/// Returns every sentence a window refusal can render as.
fn every_refusal_rendering() -> Vec<String> {
    DECLARED_REFUSALS.iter().map(|(_, failure)| failure.to_string()).collect()
}

#[test]
fn every_window_vector_lands_where_the_fixture_says_it_does() {
    let vectors = rows("window");
    assert!(vectors.len() >= 30, "the window corpus stays broad");
    for row in &vectors {
        let document = text(row, "document");
        let note = text(row, "note");
        let outcome = serde_json::from_str::<ResultWindow>(document);
        match (row["accepted"].as_bool(), outcome) {
            (Some(true), Ok(_)) => (),
            (Some(false), Err(failure)) => {
                let rendered = failure.to_string();
                match refusal_rendering(text(row, "reason")) {
                    Some(expected) => assert!(
                        rendered.contains(&expected),
                        "{note}: refused as {rendered}, not as {expected}"
                    ),
                    None => assert!(
                        !every_refusal_rendering().contains(&rendered),
                        "{note}: the closed object itself refuses this, not the window: \
                         {rendered}"
                    ),
                }
            }
            (Some(true), Err(failure)) => panic!("{note}: refused as {failure}"),
            (Some(false), Ok(window)) => panic!("{note}: accepted as {window:?}"),
            (None, _) => panic!("{note}: the fixture states whether it is accepted"),
        }
    }
}

#[test]
fn every_accepted_window_writes_itself_back_exactly() {
    for row in rows("window").iter().filter(|row| row["accepted"] == Value::Bool(true)) {
        let document = text(row, "document");
        let window: ResultWindow =
            serde_json::from_str(document).expect("the fixture says this is accepted");
        let written = serde_json::to_string(&window).expect("a valid window serializes");
        assert_eq!(written, document, "{}: rewritten differently", text(row, "note"));
        let again: ResultWindow = serde_json::from_str(&written).expect("its own bytes parse");
        assert_eq!(again, window, "{}: did not survive a round trip", text(row, "note"));
    }
}

#[test]
fn an_omitted_window_begins_the_enumeration_at_the_named_default() {
    let omitted = ResultWindow::omitted();
    assert_eq!(omitted, ResultWindow::default());
    assert_eq!(omitted.offset().map(ResultOffset::count), Some(0));
    assert_eq!(omitted.limit().map(ResultLimit::count), Some(default_result_limit()));
    assert_eq!(omitted.mode(), INITIAL_MODE);
    assert_eq!(omitted.continuation_token(), None);
    assert_eq!(
        serde_json::to_string(&omitted).expect("the default window serializes"),
        format!("{{\"mode\":\"initial\",\"offset\":0,\"limit\":{}}}", default_result_limit())
    );
}

#[test]
fn a_continuation_states_no_offset_and_no_limit_even_through_the_typed_interface() {
    let window = ResultWindow::continuation("aGVhZGVy.cGF5bG9hZA.dGFn").expect("a shaped token");
    assert_eq!(window.mode(), CONTINUATION_MODE);
    assert_eq!(window.offset(), None, "a continuation resumes after a key, not by counting");
    assert_eq!(window.limit(), None, "the limit it resumes under travels inside the token");
    assert!(window.continuation_token().is_some());
}

#[test]
fn every_named_bound_comes_from_the_manifest_rather_than_from_here() {
    let contract = slingshot_domain::command::command_identity::CommandContract::embedded();
    assert_eq!(default_result_limit(), contract.limit("default_result_limit"));
    assert_eq!(maximum_result_limit(), contract.limit("maximum_result_limit"));
    assert_eq!(maximum_result_offset(), contract.limit("maximum_result_offset"));
    assert_eq!(
        maximum_continuation_token_bytes(),
        contract.limit("maximum_continuation_token_bytes")
    );
    assert!(default_result_limit() <= maximum_result_limit(), "the default is askable");
    assert!(ResultLimit::new(default_result_limit()).is_ok());
    assert!(ResultOffset::new(maximum_result_offset()).is_ok());
    assert_eq!(
        ResultOffset::new(maximum_result_offset() + 1),
        Err(WindowFailure::OffsetAboveMaximum)
    );
    assert_eq!(ResultLimit::new(maximum_result_limit() + 1), Err(WindowFailure::LimitAboveMaximum));
    assert_eq!(ResultLimit::new(0), Err(WindowFailure::LimitZero));
}

#[test]
fn a_token_reaches_this_crate_as_bytes_and_leaves_as_the_same_bytes() {
    let spelling = "aGVhZGVy.cGF5bG9hZA.dGFn";
    let token = ContinuationToken::new(spelling).expect("a shaped token");
    assert_eq!(token.as_text(), spelling, "nothing here rewrites a token");
    assert_eq!(token.to_string(), spelling);
    let boundary = "t".repeat(
        usize::try_from(maximum_continuation_token_bytes()).expect("the bound is addressable"),
    );
    assert!(ContinuationToken::new(boundary.clone()).is_ok(), "the largest token is a token");
    assert_eq!(
        ContinuationToken::new(format!("{boundary}t")),
        Err(WindowFailure::TokenTooLong),
        "one byte further is not"
    );
}

#[test]
fn the_protection_contract_names_exactly_the_members_it_promises() {
    let headers = rows("continuation_header");
    let payloads = rows("continuation_payload");
    assert_eq!(headers.len(), 1, "one header shape is pinned");
    assert_eq!(payloads.len(), 1, "one payload shape is pinned");

    let header: Value =
        serde_json::from_str(text(&headers[0], "canonical")).expect("the header is one object");
    let members: Vec<&str> =
        header.as_object().expect("an object").keys().map(String::as_str).collect();
    assert_eq!(members, CONTINUATION_HEADER_MEMBERS, "the header carries exactly these, in order");
    assert_eq!(header["algorithm"], Value::from(CONTINUATION_TOKEN_ALGORITHM));
    assert_eq!(header["format"], Value::from(CONTINUATION_TOKEN_FORMAT));
    let key_identifier = header["key_identifier"].as_str().expect("a printable identifier");
    assert!(!key_identifier.is_empty(), "a key identifier names a key");
    assert!(key_identifier.is_ascii(), "a key identifier is printable ASCII");
    assert!(
        u64::try_from(key_identifier.len()).expect("an addressable length")
            <= maximum_continuation_key_identifier_bytes()
    );

    let payload: Value =
        serde_json::from_str(text(&payloads[0], "canonical")).expect("the payload is one object");
    let members: Vec<&str> =
        payload.as_object().expect("an object").keys().map(String::as_str).collect();
    assert_eq!(
        members, CONTINUATION_PAYLOAD_MEMBERS,
        "the payload carries exactly these, in order"
    );
    assert_eq!(payload["format"], Value::from(CONTINUATION_TOKEN_FORMAT));
    assert_eq!(payload["key_identifier"], header["key_identifier"], "one identifier, twice");
    assert_eq!(
        payload["expires_at_unix_milliseconds"].as_u64(),
        payload["issued_at_unix_milliseconds"]
            .as_u64()
            .map(|issued| issued + continuation_token_lifetime_milliseconds()),
        "expiry is issuance plus the exact lifetime"
    );
    for digest in ["arguments_digest", "author_target_identity_digest"] {
        let value = payload[digest].as_str().expect("a hexadecimal digest");
        assert_eq!(value.len(), DIGEST_HEXADECIMAL_CHARACTERS, "{digest} is a SHA-256 digest");
        assert!(
            value
                .chars()
                .all(|character| character.is_ascii_hexdigit() && !character.is_ascii_uppercase()),
            "{digest} is lowercase hexadecimal"
        );
    }
    let resume: Vec<&str> = payload["resume_sort_key"]
        .as_object()
        .expect("the resume key is one closed object")
        .keys()
        .map(String::as_str)
        .collect();
    assert_eq!(resume, vec![RESUME_SORT_KEY_MEMBER], "every discovery command resumes by path");
}

#[test]
fn the_largest_legal_token_is_built_here_and_still_fits() {
    let envelopes = rows("continuation_envelope");
    assert_eq!(envelopes.len(), 1, "one envelope proof");
    let envelope = &envelopes[0];

    let header = number(envelope, "protected_header_bytes");
    let payload = number(envelope, "payload_bytes");
    let resume = number(envelope, "resume_sort_key_bytes");
    assert!(
        resume <= maximum_continuation_resume_key_canonical_bytes(),
        "the resume key fits its own bound before the token is built"
    );

    let compact = unpadded_base64url_length(usize::try_from(header).expect("addressable"))
        + 1
        + unpadded_base64url_length(usize::try_from(payload).expect("addressable"))
        + 1
        + unpadded_base64url_length(CONTINUATION_TAG_BYTES);
    assert_eq!(
        u64::try_from(compact).expect("addressable"),
        number(envelope, "compact_bytes"),
        "the fixture's arithmetic and this test's agree"
    );
    assert_eq!(number(envelope, "maximum_compact_bytes"), maximum_continuation_token_bytes());
    assert!(
        u64::try_from(compact).expect("addressable") <= maximum_continuation_token_bytes(),
        "the largest legal token fits: {compact} bytes"
    );
    assert_eq!(envelope["fits"], Value::Bool(true));
    assert_eq!(CONTINUATION_TOKEN_SEGMENTS, 3, "header, payload, tag");
}

#[test]
fn every_lifetime_verdict_is_recomputed_rather_than_believed() {
    let lifetime = continuation_token_lifetime_milliseconds();
    let skew = maximum_continuation_token_clock_skew_milliseconds();
    let vectors = rows("continuation_lifetime");
    assert!(vectors.len() >= 7, "both edges of both relations are covered");
    for row in &vectors {
        let issued = number(row, "issued_at_unix_milliseconds");
        let expires = number(row, "expires_at_unix_milliseconds");
        let validated = number(row, "validated_at_unix_milliseconds");
        let unusable = expires != issued + lifetime || issued.saturating_sub(validated) > skew;
        let expected = if unusable {
            "continuation_token_malformed"
        } else if validated >= expires {
            "continuation_token_expired"
        } else {
            "accepted"
        };
        assert_eq!(text(row, "verdict"), expected, "{}", text(row, "note"));
    }
}

#[test]
fn every_declared_failure_is_one_of_the_five_and_the_earliest_one_wins() {
    let vectors = rows("continuation_failure");
    assert!(vectors.len() >= 25, "the precedence corpus stays broad");
    let rank = |failure: &str| {
        CONTINUATION_FAILURE_PRECEDENCE
            .iter()
            .position(|declared| *declared == failure)
            .unwrap_or_else(|| panic!("{failure} is not one of the declared five"))
    };

    /// Which failure one defect alone provokes.
    fn provoked(defect: &str) -> &'static str {
        match defect {
            "unknown_key_identifier" | "tag_mismatch" => "continuation_token_integrity_invalid",
            "wrong_target" => "continuation_token_wrong_target",
            "wrong_command_wire_name" | "wrong_version" | "wrong_arguments_digest" => {
                "continuation_token_wrong_query"
            }
            "expired" => "continuation_token_expired",
            _ => "continuation_token_malformed",
        }
    }

    let mut seen = std::collections::BTreeSet::new();
    for row in &vectors {
        let declared = text(row, "failure");
        seen.insert(declared.to_owned());
        let defects = row["defects"].as_array().expect("every vector names its defects");
        assert!(!defects.is_empty(), "a failure vector has at least one defect");
        let earliest = defects
            .iter()
            .map(|defect| provoked(defect.as_str().expect("a defect is named")))
            .min_by_key(|failure| rank(failure))
            .expect("at least one defect");
        assert_eq!(declared, earliest, "{}", text(row, "note"));
    }
    let declared: std::collections::BTreeSet<String> =
        CONTINUATION_FAILURE_PRECEDENCE.iter().map(|failure| (*failure).to_owned()).collect();
    assert_eq!(seen, declared, "every declared failure has at least one vector");
    assert_eq!(CONTINUATION_FAILURE_MEMBER, "failure", "a closed failure carries only this");
}
