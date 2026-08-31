//! The values and helpers every part of this suite is built from.
//!
//! One place for them so a fixture's meaning cannot drift between the files
//! that use it.

use std::sync::Arc;
use std::sync::mpsc;

use slingshot_domain::command_fingerprint::{CommandFingerprint, FingerprintInput};
use slingshot_domain::installation::InstallationIdentifier;
use slingshot_domain::operation::{
    OperationExecutionCertainty, OperationFact, OperationLifecycleState, RecoveryCategory,
    RecoveryExecutionEvidence, RecoveryFact, TerminalFailure, TerminalFailureDisposition,
    TerminalFailureKind,
};
use slingshot_storage::database::{OperationDatabase, RequiredSettings};
use slingshot_storage::operation_repository::{
    AdmissionOutcome, AdmissionRequest, OperationRepository, OperationSummary, ResultDisposition,
    ResumeOutcome,
};

/// Bytes one page occupies, from the runtime contract.
pub const PAGE_BYTES: u64 = 4096;

/// Pages the database may reach, from the runtime contract.
pub const DATABASE_PAGES: u64 = 262_144;

/// Milliseconds a busy connection waits, from the runtime contract.
pub const BUSY_TIMEOUT: u64 = 5000;

/// Receipts one operation may hold, from the runtime contract.
pub const RECEIPTS_PER_OPERATION: u64 = 64;

/// Bytes a progress note may occupy, from the runtime contract.
pub const PROGRESS_DETAIL_BYTES: usize = 1024;

/// Two-character pairs in a sixty-four-character hexadecimal value.
pub const DIGEST_PAIRS: usize = 32;

/// Characters a sixty-four-character hexadecimal value has.
pub const DIGEST_CHARACTERS: usize = 64;

/// Four-character groups in a sixty-four-character source fingerprint.
pub const SOURCE_GROUPS: usize = 16;

/// Milliseconds one recovery fixture waits before retrying.
pub const RETRY_DELAY_MILLISECONDS: u64 = 2500;

/// Attempts one recovery fixture has already made.
pub const ATTEMPTS_ALREADY_MADE: u32 = 3;

/// Operations one reconstruction fixture admits.
pub const ADMITTED_OPERATIONS: usize = 4;

/// Milliseconds between admission and settlement in one fixture.
pub const SETTLING_DELAY_MILLISECONDS: u64 = 60_000;

/// A revision no operation in these fixtures is at.
///
/// A replay must hand back the receipt that was committed rather than build one
/// from what it was just asked for, so the repeat asks with a revision that
/// would be obvious in the answer if it ever leaked into it.
pub const REVISION_NOBODY_IS_AT: u64 = 99;

/// One instant, for a test that does not care which.
pub const NOW: u64 = 1_700_000_000_000;

/// The instant one fixture step after [`NOW`].
pub const SECOND_INSTANT: u64 = NOW + 1;

/// The instant one fixture step after [`SECOND_INSTANT`].
pub const THIRD_INSTANT: u64 = SECOND_INSTANT + 1;

/// The instant one fixture step after [`THIRD_INSTANT`].
pub const FOURTH_INSTANT: u64 = THIRD_INSTANT + 1;

/// Contenders one concurrency test runs.
pub const CONTENDERS: usize = 8;

/// Returns the settings every connection is held to.
pub fn settings() -> RequiredSettings {
    RequiredSettings {
        page_bytes: PAGE_BYTES,
        database_pages: DATABASE_PAGES,
        busy_timeout_milliseconds: BUSY_TIMEOUT,
    }
}

/// Returns one repository over the database at `path`.
pub fn repository(path: &std::path::Path) -> OperationRepository {
    OperationRepository::new(OperationDatabase::open(path, settings()).expect("a database"))
}

/// Returns one repository over a database held in memory.
pub fn in_memory() -> OperationRepository {
    OperationRepository::new(OperationDatabase::open_in_memory(settings()).expect("a database"))
}

/// Returns the installation identifier every fixture is admitted under.
pub fn installation() -> InstallationIdentifier {
    InstallationIdentifier::parse(&"a1".repeat(DIGEST_PAIRS)).expect("a legal identifier")
}

/// Returns a second installation identifier, for proving the snapshot is one.
pub fn other_installation() -> InstallationIdentifier {
    InstallationIdentifier::parse(&"b2".repeat(DIGEST_PAIRS)).expect("a legal identifier")
}

/// Returns the fingerprint of one command against one revision.
pub fn fingerprint(digest: &str, canonical_command: &str, revision: &str) -> CommandFingerprint {
    CommandFingerprint::derive(&FingerprintInput {
        author_target_identity_digest: digest.to_owned(),
        canonical_command: canonical_command.to_owned(),
        command_wire_name: "query_paths".to_owned(),
        command_semantic_contract_version: "1".to_owned(),
        selected_environment_revision: revision.to_owned(),
    })
    .expect("a derivable fingerprint")
}

/// Returns one admission request.
pub fn request(
    digest: &str,
    identifier: &str,
    canonical_command: &str,
    revision: &str,
) -> AdmissionRequest {
    AdmissionRequest {
        author_target_identity: format!("opaque-identity-behind-{digest}"),
        author_target_identity_digest: digest.to_owned(),
        caller_identity: Some("caller-1".to_owned()),
        canonical_command: canonical_command.to_owned(),
        command_fingerprint: fingerprint(digest, canonical_command, revision),
        command_wire_name: "query_paths".to_owned(),
        daemon_runtime_contract_digest: "c".repeat(DIGEST_CHARACTERS),
        installation_identifier: installation(),
        operation_identifier: identifier.to_owned(),
        selected_environment_revision: revision.to_owned(),
        workflow_correlation_identifier: None,
    }
}

/// The two partitions a same-deployment fixture uses.
///
/// Nothing about the work differs between them. Only the opaque principal the
/// author target was reached through does, which is exactly enough to make them
/// two operations rather than one.
pub const FIRST_PRINCIPAL: &str = "1d";

/// The second of those two partitions.
pub const SECOND_PRINCIPAL: &str = "2d";

/// Returns the digest one principal's author target has.
pub fn partition(principal: &str) -> String {
    principal.repeat(DIGEST_PAIRS)
}

/// Applies one fact to the fixture operation and returns the row it makes.
pub fn applied(
    store: &OperationRepository,
    digest: &str,
    revision: u64,
    fact: &OperationFact,
    now_unix_milliseconds: u64,
) -> OperationSummary {
    store.apply(digest, OPERATION, revision, fact, now_unix_milliseconds).expect("a legal fact")
}

/// Runs `work` on its own repository in every contender at once.
///
/// Each contender opens its own connection to the same file, which is the shape
/// that matters: two clients of one daemon are two connections, and a race
/// proved on one shared handle would prove nothing about them.
pub fn race<Work>(path: &std::path::Path, work: Work) -> Vec<bool>
where
    Work: Fn(&OperationRepository, usize) -> bool + Send + Sync + 'static,
{
    let path = Arc::new(path.to_path_buf());
    let work = Arc::new(work);
    let (sender, receiver) = mpsc::channel();
    let contenders: Vec<std::thread::JoinHandle<()>> = (0..CONTENDERS)
        .map(|index| {
            let path = Arc::clone(&path);
            let work = Arc::clone(&work);
            let sender = sender.clone();
            std::thread::spawn(move || {
                let store = repository(&path);
                sender.send(work(&store, index)).expect("a delivered result");
            })
        })
        .collect();
    drop(sender);
    let answers: Vec<bool> = receiver.iter().collect();
    for contender in contenders {
        contender.join().expect("a contender finishes");
    }
    assert_eq!(answers.len(), CONTENDERS, "every contender answered");
    answers
}

/// Returns the operation identifier one admitted fixture uses.
pub const OPERATION: &str = "operation-1";

/// Admits one operation into `store` and returns it.
pub fn admitted(store: &OperationRepository, digest: &str) -> u64 {
    let asked = request(digest, OPERATION, "{\"paths\":[\"/content\"]}", "revision-1");
    let outcome = store.admit(&asked, NOW).expect("an admission");
    assert!(matches!(outcome, AdmissionOutcome::Admitted(_)), "a first admission admits");
    outcome.summary().record.revision
}

/// Returns one lifecycle fact.
pub fn reaching(lifecycle_state: OperationLifecycleState) -> OperationFact {
    OperationFact::Lifecycle { lifecycle_state }
}

/// Returns one recovery fact of the category and evidence named.
pub fn recovery(
    category: RecoveryCategory,
    evidence: RecoveryExecutionEvidence,
    attempt_count: u32,
) -> RecoveryFact {
    RecoveryFact {
        attempt_count,
        category,
        detail: "the remote closed the stream".to_owned(),
        evidence,
        manual_resume_eligible: true,
        retry_delay_milliseconds: RETRY_DELAY_MILLISECONDS,
        retry_observed_at_unix_milliseconds: NOW,
    }
}

/// Returns one terminal fact of the kind and disposition named.
pub fn settling(
    kind: TerminalFailureKind,
    disposition: TerminalFailureDisposition,
) -> OperationFact {
    OperationFact::Terminal { failure: TerminalFailure { disposition, kind, metadata: None } }
}

/// Returns one recovery fact of the category and evidence named.
pub fn recovering(
    category: RecoveryCategory,
    evidence: RecoveryExecutionEvidence,
    attempt_count: u32,
) -> OperationFact {
    OperationFact::Recovery { recovery: recovery(category, evidence, attempt_count) }
}

/// Records where the fixture operation's result went.
pub fn disposed(
    store: &OperationRepository,
    digest: &str,
    revision: u64,
    disposition: ResultDisposition,
) -> OperationSummary {
    store
        .record_result_disposition(digest, OPERATION, revision, disposition)
        .expect("a disposition")
}

/// Returns the source fingerprint one resume fixture is keyed by.
pub fn source(index: usize) -> String {
    format!("{index:04x}").repeat(SOURCE_GROUPS)
}

/// The certainty an unresolved-submission fixture carries.
pub const SUBMISSION_UNKNOWN: RecoveryExecutionEvidence =
    RecoveryExecutionEvidence::ExecutionCertainty {
        certainty: OperationExecutionCertainty::SubmissionUnknown,
    };

/// Records or replays one resume receipt for the fixture operation.
pub fn receipted(
    store: &OperationRepository,
    digest: &str,
    index: usize,
    applied_operation_revision: u64,
    now_unix_milliseconds: u64,
) -> ResumeOutcome {
    store
        .record_resume_receipt(
            digest,
            OPERATION,
            &source(index),
            "revision-1",
            applied_operation_revision,
            now_unix_milliseconds,
        )
        .expect("a classification")
}
