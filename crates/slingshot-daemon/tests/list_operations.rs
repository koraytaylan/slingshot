//! Walking one target's history without missing or repeating a row.
//!
//! Every fixture states the exact pages a walk produces, so the tests say what
//! the paging does rather than that it did something plausible. The property
//! underneath them all is that a boundary named by an arrival sequence cannot
//! move: rows admitted during a walk appear above where the walk already is,
//! and never shift a row the walker has yet to see.

use serde_json::Value;
use slingshot_daemon::operation_queries::{NEWEST_FIRST, OperationPage, PageCursor, list};
use slingshot_domain::command_fingerprint::{CommandFingerprint, FingerprintInput};
use slingshot_domain::installation::InstallationIdentifier;
use slingshot_domain::operation::{
    OperationFact, OperationLifecycleState, TerminalFailure, TerminalFailureDisposition,
    TerminalFailureKind,
};
use slingshot_storage::database::{OperationDatabase, RequiredSettings};
use slingshot_storage::operation_repository::{
    AdmissionOutcome, AdmissionRequest, OperationRepository,
};

/// Paging fixtures this test reads.
const PAGES: &str = include_str!("fixtures/list-operations/pages.jsonl");

/// Bytes one page occupies, from the runtime contract.
const PAGE_BYTES: u64 = 4096;

/// Pages the database may reach, from the runtime contract.
const DATABASE_PAGES: u64 = 262_144;

/// Milliseconds a busy connection waits, from the runtime contract.
const BUSY_TIMEOUT: u64 = 5000;

/// Two-character pairs in a sixty-four-character hexadecimal value.
const DIGEST_PAIRS: usize = 32;

/// Characters a sixty-four-character hexadecimal value has.
const DIGEST_CHARACTERS: usize = 64;

/// One instant, for a test that does not care which.
const NOW: u64 = 1_700_000_000_000;

/// The environment revision these fixtures are admitted under.
const REVISION: &str = "revision-1";

/// The first partition these fixtures use.
const FIRST_PRINCIPAL: &str = "1d";

/// A second partition, for proving a page cannot cross one.
const SECOND_PRINCIPAL: &str = "2d";

/// Rows one interleaving fixture admits before it starts walking.
const INTERLEAVED_ADMITTED: u64 = 5;

/// Rows one interleaving fixture takes per page.
const INTERLEAVED_PAGE_SIZE: u64 = 2;

/// Rows the concatenation fixture admits.
const CONCATENATED_ADMITTED: u64 = 7;

/// Rows the concatenation fixture takes per page.
const CONCATENATED_PAGE_SIZE: u64 = 3;

/// Values one comparison of neighbours looks at.
const ADJACENT_PAIR: usize = 2;

/// The arrival sequences one interleaving walk expects, newest first.
const INTERLEAVED_WALK: &[u64] = &[5, 4, 3, 2, 1];

/// The second row that arrives while an interleaving walk is under way.
const ARRIVED_MID_WALK: u64 = INTERLEAVED_ADMITTED + 2;

/// Returns one row's string member.
fn text<'row>(row: &'row Value, member: &str) -> &'row str {
    row[member].as_str().unwrap_or_else(|| panic!("{member} is a string in {row}"))
}

/// Returns every case of the fixture.
fn cases() -> Vec<Value> {
    PAGES
        .lines()
        .map(|line| serde_json::from_str(line).expect("every fixture line is one object"))
        .collect()
}

/// Returns the settings every connection is held to.
fn settings() -> RequiredSettings {
    RequiredSettings {
        page_bytes: PAGE_BYTES,
        database_pages: DATABASE_PAGES,
        busy_timeout_milliseconds: BUSY_TIMEOUT,
    }
}

/// Returns one repository over a database held in memory.
fn repository() -> OperationRepository {
    OperationRepository::new(OperationDatabase::open_in_memory(settings()).expect("a database"))
}

/// Returns the digest one principal's author target has.
fn partition(principal: &str) -> String {
    principal.repeat(DIGEST_PAIRS)
}

/// Admits one operation numbered `index`.
fn admit(repository: &OperationRepository, digest: &str, index: u64) {
    let identifier = format!("operation-{index}");
    let canonical = format!("{{\"paths\":[\"/{index}\"]}}");
    let asked = AdmissionRequest {
        author_target_identity: format!("opaque-identity-behind-{digest}"),
        author_target_identity_digest: digest.to_owned(),
        caller_identity: Some("caller-1".to_owned()),
        canonical_command: canonical.clone(),
        command_fingerprint: CommandFingerprint::derive(&FingerprintInput {
            author_target_identity_digest: digest.to_owned(),
            canonical_command: canonical,
            command_wire_name: "query_paths".to_owned(),
            command_semantic_contract_version: "1".to_owned(),
            selected_environment_revision: REVISION.to_owned(),
        })
        .expect("a derivable fingerprint"),
        command_wire_name: "query_paths".to_owned(),
        daemon_runtime_contract_digest: "c".repeat(DIGEST_CHARACTERS),
        installation_identifier: InstallationIdentifier::parse(&"a1".repeat(DIGEST_PAIRS))
            .expect("a legal identifier"),
        operation_identifier: identifier,
        selected_environment_revision: REVISION.to_owned(),
        workflow_correlation_identifier: Some("workflow-1".to_owned()),
    };
    let outcome = repository.admit(&asked, NOW).expect("an admission");
    assert!(matches!(outcome, AdmissionOutcome::Admitted(_)), "each fixture row admits");
}

/// Walks the whole partition, returning each page's arrival sequences.
fn walk(repository: &OperationRepository, digest: &str, page_size: u64) -> Vec<Vec<u64>> {
    let mut cursor = PageCursor { before_enqueue_sequence: NEWEST_FIRST };
    let mut pages = Vec::new();
    loop {
        let page: OperationPage = list(repository, digest, cursor, page_size).expect("a page");
        pages.push(page.rows.iter().map(|row| row.enqueue_sequence).collect());
        match page.next {
            Some(next) => cursor = next,
            None => return pages,
        }
    }
}

#[test]
fn every_paging_fixture_walks_exactly_the_pages_it_states() {
    for case in cases() {
        let repository = repository();
        let digest = partition(FIRST_PRINCIPAL);
        let admitted = case["admitted"].as_u64().expect("a count");
        for index in 1..=admitted {
            admit(&repository, &digest, index);
        }
        let page_size = case["page_size"].as_u64().expect("a page size");
        let expected: Vec<Vec<u64>> = case["pages"]
            .as_array()
            .expect("a page list")
            .iter()
            .map(|page| {
                page.as_array()
                    .expect("a row list")
                    .iter()
                    .map(|value| value.as_u64().expect("a sequence"))
                    .collect()
            })
            .collect();
        assert_eq!(walk(&repository, &digest, page_size), expected, "{}", text(&case, "note"));
    }
}

#[test]
fn concatenated_pages_are_the_whole_partition_with_every_row_once() {
    let repository = repository();
    let digest = partition(FIRST_PRINCIPAL);
    let admitted = CONCATENATED_ADMITTED;
    for index in 1..=admitted {
        admit(&repository, &digest, index);
    }

    let walked: Vec<u64> =
        walk(&repository, &digest, CONCATENATED_PAGE_SIZE).into_iter().flatten().collect();
    let mut sorted = walked.clone();
    sorted.sort_unstable();
    sorted.dedup();
    assert_eq!(
        sorted.len(),
        usize::try_from(admitted).expect("a countable count"),
        "every row appeared, and none appeared twice"
    );
    assert!(
        walked.windows(ADJACENT_PAIR).all(|pair| pair[0] > pair[1]),
        "and the whole walk is newest first: {walked:?}"
    );
}

#[test]
fn a_row_admitted_during_a_walk_never_moves_one_the_walker_has_yet_to_see() {
    let repository = repository();
    let digest = partition(FIRST_PRINCIPAL);
    for index in 1..=INTERLEAVED_ADMITTED {
        admit(&repository, &digest, index);
    }

    let mut cursor = PageCursor { before_enqueue_sequence: NEWEST_FIRST };
    let mut walked = Vec::new();
    let first = list(&repository, &digest, cursor, INTERLEAVED_PAGE_SIZE).expect("a page");
    walked.extend(first.rows.iter().map(|row| row.enqueue_sequence));
    cursor = first.next.expect("a cursor");

    for later in [INTERLEAVED_ADMITTED + 1, ARRIVED_MID_WALK] {
        admit(&repository, &digest, later);
    }

    loop {
        let page = list(&repository, &digest, cursor, INTERLEAVED_PAGE_SIZE).expect("a page");
        walked.extend(page.rows.iter().map(|row| row.enqueue_sequence));
        match page.next {
            Some(next) => cursor = next,
            None => break,
        }
    }
    assert_eq!(
        walked.as_slice(),
        INTERLEAVED_WALK,
        "the walk saw the rows that existed when it started, each once, in order"
    );
    assert!(
        !walked.contains(&(INTERLEAVED_ADMITTED + 1)),
        "and the rows admitted mid-walk sit above where the walk already was"
    );
}

#[test]
fn a_page_carries_what_a_client_needs_and_no_payload() {
    let repository = repository();
    let digest = partition(FIRST_PRINCIPAL);
    admit(&repository, &digest, 1);
    let held = repository.read(&digest, "operation-1").expect("a read").expect("a row");
    repository
        .apply(
            &digest,
            "operation-1",
            held.record.revision,
            &OperationFact::Terminal {
                failure: TerminalFailure {
                    disposition: TerminalFailureDisposition::AuthoritativeRemoteFailure,
                    kind: TerminalFailureKind::RemoteFailed,
                    metadata: Some("the remote said no".to_owned()),
                },
            },
            NOW,
        )
        .expect("a settlement");

    let page = list(
        &repository,
        &digest,
        PageCursor { before_enqueue_sequence: NEWEST_FIRST },
        INTERLEAVED_PAGE_SIZE,
    )
    .expect("a page");
    let row = page.rows.first().expect("one row");
    assert_eq!(row.operation_identifier, "operation-1");
    assert_eq!(row.lifecycle_state, OperationLifecycleState::Failed);
    assert_eq!(
        row.terminal_failure_kind,
        Some(TerminalFailureKind::RemoteFailed),
        "knowing an operation failed without knowing how is not useful"
    );
    assert_eq!(row.caller_identity.as_deref(), Some("caller-1"));
    assert_eq!(row.workflow_correlation_identifier.as_deref(), Some("workflow-1"));
    assert!(row.settled_at_unix_milliseconds.is_some(), "and it says when it ended");

    let rendered = format!("{row:?}");
    assert!(
        !rendered.contains("paths") && !rendered.contains("opaque-identity"),
        "while the command payload and the opaque identity stay out of a listing: {rendered}"
    );
}

#[test]
fn a_page_never_crosses_a_partition() {
    let repository = repository();
    let here = partition(FIRST_PRINCIPAL);
    let elsewhere = partition(SECOND_PRINCIPAL);
    for index in 1..=INTERLEAVED_ADMITTED {
        admit(&repository, &here, index);
    }
    admit(&repository, &elsewhere, 1);

    let page = list(
        &repository,
        &elsewhere,
        PageCursor { before_enqueue_sequence: NEWEST_FIRST },
        INTERLEAVED_PAGE_SIZE,
    )
    .expect("a page");
    assert_eq!(page.rows.len(), 1, "one partition's page holds one partition's rows");
    assert_eq!(page.rows[0].enqueue_sequence, 1, "counting from its own arrival order");
    assert_eq!(page.next, None, "and the walk ends there");
}
