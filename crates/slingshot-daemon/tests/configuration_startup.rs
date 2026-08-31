//! What a daemon establishes before anything can reach it, and when it refuses.
//!
//! The audit vectors are the heart of this. A daemon serves one author target
//! at one environment revision, and the durable state it opens may hold work
//! some earlier daemon admitted under a different identity. Finished work is
//! history and never blocks anything; unfinished work belonging to somebody
//! else stops startup dead, because adopting it would mean executing against a
//! security context nobody chose.
//!
//! Every refusal is also checked for what it did not do. A daemon that refused
//! and left a changed byte behind would be worse than one that started.

use serde_json::Value;
use slingshot_daemon::startup::{
    self, EstablishedDaemon, SelectedTarget, StartupRefusal, StartupRequest,
};
use slingshot_domain::command_fingerprint::{CommandFingerprint, FingerprintInput};
use slingshot_domain::installation::InstallationIdentifier;
use slingshot_domain::operation::{OperationFact, OperationLifecycleState};
use slingshot_storage::database::{MIGRATIONS, OperationDatabase, RequiredSettings};
use slingshot_storage::operation_repository::{
    AdmissionOutcome, AdmissionRequest, OperationRepository,
};

/// Audit vectors this test reads.
const AUDIT: &str = include_str!("fixtures/configuration-startup/audit.jsonl");

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

/// The profile every fixture here starts under.
const PROFILE: &str = "production";

/// The environment every fixture here starts under.
const ENVIRONMENT: &str = "publish";

/// The revision this daemon selects.
const REVISION: &str = "revision-1";

/// Returns one row's string member.
fn text<'row>(row: &'row Value, member: &str) -> &'row str {
    row[member].as_str().unwrap_or_else(|| panic!("{member} is a string in {row}"))
}

/// Returns every row of the audit fixture.
fn rows() -> Vec<Value> {
    AUDIT
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

/// Returns what this daemon selects.
fn selected() -> SelectedTarget {
    SelectedTarget {
        author_target_identity_digest: "1d".repeat(DIGEST_PAIRS),
        daemon_runtime_contract_digest: "c".repeat(DIGEST_CHARACTERS),
        selected_environment_revision: REVISION.to_owned(),
    }
}

/// Returns one startup request rooted inside `directory`.
fn request(directory: &tempfile::TempDir) -> StartupRequest {
    StartupRequest {
        environment: ENVIRONMENT.to_owned(),
        profile: PROFILE.to_owned(),
        runtime_root: directory.path().join("runtime"),
        settings: settings(),
        state_root: directory.path().join("state"),
    }
}

/// Starts one daemon, or says which step refused.
fn establish(directory: &tempfile::TempDir) -> Result<EstablishedDaemon, StartupRefusal> {
    startup::establish(&request(directory), &selected())
}

/// Admits one operation and walks it to the state a vector names.
///
/// Seeded through the repository that owns admission rather than with a
/// hand-written row, so the audit is proved against rows a daemon actually
/// writes - including the lifecycle order a row has to pass through to reach
/// the state it is in.
fn seed(path: &std::path::Path, target: &str, revision: &str, state: &str, index: usize) {
    let repository =
        OperationRepository::new(OperationDatabase::open(path, settings()).expect("a database"));
    let identifier = format!("operation-{index}");
    let canonical = format!("{{\"paths\":[\"/{index}\"]}}");
    let asked = AdmissionRequest {
        author_target_identity: format!("opaque-identity-behind-{target}"),
        author_target_identity_digest: target.to_owned(),
        caller_identity: None,
        canonical_command: canonical.clone(),
        command_fingerprint: CommandFingerprint::derive(&FingerprintInput {
            author_target_identity_digest: target.to_owned(),
            canonical_command: canonical,
            command_wire_name: "query_paths".to_owned(),
            command_semantic_contract_version: "1".to_owned(),
            selected_environment_revision: revision.to_owned(),
        })
        .expect("a derivable fingerprint"),
        command_wire_name: "query_paths".to_owned(),
        daemon_runtime_contract_digest: "c".repeat(DIGEST_CHARACTERS),
        installation_identifier: InstallationIdentifier::parse(&"a1".repeat(DIGEST_PAIRS))
            .expect("a legal identifier"),
        operation_identifier: identifier.clone(),
        selected_environment_revision: revision.to_owned(),
        workflow_correlation_identifier: None,
    };
    let outcome = repository.admit(&asked, 0).expect("an admission");
    assert!(matches!(outcome, AdmissionOutcome::Admitted(_)), "each fixture row is admitted");
    let mut revision_now = outcome.summary().record.revision;
    for reached in walk_to(state) {
        let advanced = repository
            .apply(
                target,
                &identifier,
                revision_now,
                &OperationFact::Lifecycle { lifecycle_state: reached },
                0,
            )
            .expect("a legal advance");
        revision_now = advanced.record.revision;
    }
}

/// Returns the states an operation passes through to reach `state`.
fn walk_to(state: &str) -> Vec<OperationLifecycleState> {
    let path = [
        OperationLifecycleState::Submitting,
        OperationLifecycleState::Accepted,
        OperationLifecycleState::Running,
        OperationLifecycleState::Succeeded,
    ];
    let wanted = match state {
        "queued" => return Vec::new(),
        "submitting" => OperationLifecycleState::Submitting,
        "accepted" => OperationLifecycleState::Accepted,
        "running" => OperationLifecycleState::Running,
        "succeeded" => OperationLifecycleState::Succeeded,
        other => {
            assert_eq!(other, "failed", "a lifecycle state this daemon has");
            return vec![OperationLifecycleState::Failed];
        }
    };
    let reached = path.iter().position(|held| *held == wanted).expect("a reachable state");
    path[..=reached].to_vec()
}

/// Returns the schema version this binary migrates to.
///
/// Read from the migration list rather than written down again, so a startup
/// test cannot disagree with the schema it just applied.
fn current_schema_version() -> u32 {
    MIGRATIONS.last().map(|(version, _)| *version).unwrap_or_default()
}

#[test]
fn a_first_startup_establishes_everything_and_leaves_it_where_a_second_finds_it() {
    let directory = tempfile::tempdir().expect("a directory");
    let established = establish(&directory).expect("a first startup");
    assert_eq!(established.target, selected(), "serving what it selected");
    assert_eq!(established.namespace.display(), "production/publish");
    assert!(established.paths.database_path().exists(), "with a database that exists");
    assert_eq!(
        established.database.schema_version().expect("a version"),
        current_schema_version(),
        "and is at the current schema"
    );
    let path = established.paths.database_path();
    drop(established);

    let again = establish(&directory).expect("a second startup over the same state");
    assert_eq!(again.paths.database_path(), path, "which finds what the first left");
}

#[test]
fn every_audit_vector_starts_or_refuses_the_way_the_fixture_says() {
    let vectors = rows();
    assert!(vectors.len() >= 10, "both identities, both directions, and every unfinished state");
    for (index, row) in vectors.iter().enumerate() {
        let directory = tempfile::tempdir().expect("a directory");
        {
            let established = establish(&directory).expect("an empty first startup");
            seed(
                &established.paths.database_path(),
                text(row, "admitted_target"),
                text(row, "admitted_revision"),
                text(row, "lifecycle_state"),
                index,
            );
        }
        let started = establish(&directory);
        let expected = row["starts"].as_bool().expect("a vector states its verdict");
        match (expected, started) {
            (true, Ok(_)) | (false, Err(StartupRefusal::ForeignWorkOutstanding { .. })) => (),
            (true, Err(refusal)) => panic!("{}: refused as {refusal}", text(row, "note")),
            (false, Ok(_)) => panic!("{}: started", text(row, "note")),
            (false, Err(other)) => {
                panic!("{}: refused for the wrong reason: {other}", text(row, "note"))
            }
        }
    }
}

#[test]
fn a_refusal_names_whose_work_it_found_and_changes_nothing() {
    let directory = tempfile::tempdir().expect("a directory");
    let foreign = "2d".repeat(DIGEST_PAIRS);
    let path = {
        let established = establish(&directory).expect("an empty first startup");
        let path = established.paths.database_path();
        seed(&path, &selected().author_target_identity_digest, REVISION, "queued", 1);
        seed(&path, &foreign, REVISION, "queued", 0);
        path
    };
    let before = std::fs::read(&path).expect("the database reads");

    let refused = establish(&directory);
    let Err(StartupRefusal::ForeignWorkOutstanding { count, partitions }) = refused else {
        panic!("unfinished work under another target refuses startup: {refused:?}");
    };
    assert_eq!(count, 1, "one partition holds work this daemon may not adopt");
    assert_eq!(
        partitions[0].author_target_identity_digest, foreign,
        "and the refusal says whose, so a person can decide what to do"
    );
    assert_eq!(
        partitions[0].selected_environment_revision, REVISION,
        "under the revision it was admitted at"
    );
    assert_eq!(
        std::fs::read(&path).expect("the database reads"),
        before,
        "and the refusal left every byte of it exactly as it was"
    );
}

/// Returns where the fixture's database lives.
fn establish_path(directory: &tempfile::TempDir) -> std::path::PathBuf {
    let targets = directory.path().join("state").join("targets");
    let entry = std::fs::read_dir(&targets)
        .expect("the targets directory reads")
        .next()
        .expect("one target")
        .expect("an entry");
    entry.path().join("operations.sqlite3")
}

#[test]
fn finished_work_from_an_old_target_stays_queryable_and_blocks_nothing() {
    let directory = tempfile::tempdir().expect("a directory");
    let old = "2d".repeat(DIGEST_PAIRS);
    {
        let established = establish(&directory).expect("an empty first startup");
        let path = established.paths.database_path();
        seed(&path, &old, "revision-0", "succeeded", 0);
        seed(&path, &old, "revision-0", "failed", 1);
    }

    let established = establish(&directory).expect("history is not an obstacle");
    let held = OperationRepository::new(
        OperationDatabase::open(&establish_path(&directory), settings()).expect("a database"),
    );
    drop(established);
    assert_eq!(
        held.reconstruct(&old).expect("a reconstruction").len(),
        2,
        "and the old target's history is still there to answer questions"
    );
    let database = OperationDatabase::open(&establish_path(&directory), settings())
        .expect("a second look at the same database");
    assert!(
        startup::unfinished_partitions(&database).expect("an audit").is_empty(),
        "while the audit finds nothing unfinished at all"
    );
}

#[test]
fn a_namespace_that_cannot_be_named_binds_nothing() {
    let directory = tempfile::tempdir().expect("a directory");
    let refused = startup::establish(
        &StartupRequest { profile: "prod/uction".to_owned(), ..request(&directory) },
        &selected(),
    );
    assert!(
        matches!(refused, Err(StartupRefusal::Namespace(_))),
        "a name carrying a separator refuses before anything is created: {refused:?}"
    );
    assert!(
        !directory.path().join("state").join("targets").exists(),
        "and no durable state was created on the way to refusing"
    );
}
