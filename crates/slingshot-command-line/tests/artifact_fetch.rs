// SPDX-License-Identifier: MIT OR Apache-2.0
// Copyright 2026 Koray Taylan Davgana

//! Fetching one operation's artifact to a destination.
//!
//! The interesting cases are the ones where bytes and facts disagree. A
//! transfer whose length or digest does not match what the daemon declared must
//! leave nothing at the destination and remove what it staged, because a
//! partial file beside a caller's name is worse than no file: the next command
//! that reads it has no way to tell it from a whole one. A destination that is
//! already there belongs to somebody else and is never replaced.
//!
//! Every case here drives the shipped application: the same `observe` leaf, the
//! same staging and publication machinery, and the same refusal types a run
//! reaches. What is faked is the daemon on the other end, which is the one
//! boundary a suite cannot have.

#![allow(missing_docs)]

use std::path::{Path, PathBuf};

use sha2::Digest as _;
use slingshot_command_line::application::{
    Answer, CommandLineApplication, Completion, ConfigurationBoundary, DaemonBoundary,
    FilesystemBoundary, NetworkBoundary, ProcessBoundary, Provenance, RequestIdentityBoundary,
    SignalBoundary,
};
use slingshot_command_line::configuration_check::{CheckReport, ResolvedFacts};
use slingshot_command_line::daemon_connection::{
    ArtifactEvent, ArtifactStreamRefusal, ExchangeFailure,
};
use slingshot_command_line::invocation::{
    DESTINATION_OPTION, EXPECTED_DIGEST_OPTION, Invocation, OPERATION_IDENTIFIER_OPTION,
    Selection,
};
use slingshot_command_line::machine_outcome_envelope::MachineOutcomeEnvelope;
use slingshot_command_line::target_selection::NamespacePair;
use slingshot_domain::profile::AdobeExperienceManagerDeployment;
use slingshot_local_protocol::control::HelloResult;
use slingshot_local_protocol::message::{OperationEnvelope, OperationResponse};

/// Profile every case names.
const PROFILE: &str = "local";

/// Environment every case names.
const ENVIRONMENT: &str = "author";

/// The target digest the scenario daemon serves.
const DIGEST: &str = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";

/// The environment revision the scenario daemon resolved.
const REVISION: &str = "abcdefabcdefabcdefabcdefabcdefabcdefabcdefabcdefabcdefabcdefabcd";

/// The namespace the scenario daemon owns.
const NAMESPACE: &str = "local/author";

/// The version the scenario daemon reports.
const PRODUCT_VERSION: &str = "0.0.0";

/// The operation-protocol version every side of a case speaks.
const SPOKEN_VERSION: u32 = 1;

/// The operation a case observes.
const OPERATION_IDENTIFIER: &str = "scenario-operation";

/// The artifact a case fetches.
const ARTIFACT_IDENTIFIER: &str = "scenario-artifact";

/// The media type the artifact declares.
const MEDIA_TYPE: &str = "application/json";

/// Returns the bytes one case transfers.
fn payload() -> Vec<u8> {
    let mut bytes = Vec::new();
    for index in 0..64 {
        bytes.extend_from_slice(format!("chunk-{index:04}-").as_bytes());
    }
    bytes
}

/// Returns what one set of bytes digests to.
fn digest_of(bytes: &[u8]) -> String {
    let mut hasher = sha2::Sha256::new();
    hasher.update(bytes);
    hasher.finalize().iter().map(|byte| format!("{byte:02x}")).collect()
}

/// How a case daemon answers an artifact read.
enum Answering {
    /// The transfer the daemon described, in chunks of `chunk_bytes`.
    Whole {
        /// The bytes.
        bytes: Vec<u8>,
        /// How large each chunk is.
        chunk_bytes: usize,
    },
    /// Every byte the daemon described, except the digest it declared.
    AnotherDigest {
        /// The bytes.
        bytes: Vec<u8>,
        /// What the daemon says they digest to.
        declared: String,
    },
    /// Fewer bytes than the daemon declared.
    Short {
        /// The bytes actually sent.
        bytes: Vec<u8>,
        /// How long the daemon said the artifact is.
        declared_length: u64,
    },
    /// Every byte, delivered twice, which is a length that cannot be right.
    Repeated {
        /// The bytes.
        bytes: Vec<u8>,
    },
    /// An answer that is not an artifact transfer at all.
    NotATransfer,
    /// A transfer whose connection ends before it says it did.
    EndedEarly {
        /// The bytes.
        bytes: Vec<u8>,
    },
}

/// One case's daemon: the boundary the application reaches through.
struct Fakes {
    answering: Answering,
    reached_daemon: std::cell::Cell<u32>,
}

impl Fakes {
    fn new(answering: Answering) -> Self {
        Self { answering, reached_daemon: std::cell::Cell::new(0) }
    }
}

impl RequestIdentityBoundary for Fakes {
    fn invent_request_identifier(&self) -> String {
        "case-request".to_owned()
    }
}

impl SignalBoundary for Fakes {
    fn stop_requested(&self) -> bool {
        false
    }
}

impl ConfigurationBoundary for Fakes {
    fn check(&self, _selection: &Selection) -> CheckReport {
        CheckReport::Resolved(Box::new(ResolvedFacts {
            author_target: "https://author.example/".to_owned(),
            deployment: AdobeExperienceManagerDeployment::AdobeExperienceManager65,
            environment: ENVIRONMENT.to_owned(),
            profile: PROFILE.to_owned(),
            warned_cleartext_transport: true,
        }))
    }
}

impl NetworkBoundary for Fakes {
    fn authority_answers(&self, _authority: &str) -> bool {
        false
    }
}

impl ProcessBoundary for Fakes {
    fn start_daemon(&self, _namespace: &NamespacePair) -> Result<(), String> {
        Ok(())
    }
}

impl FilesystemBoundary for Fakes {
    fn place(&self, _destination: &Path, _bytes: &[u8]) -> Result<(), String> {
        Ok(())
    }
}

impl DaemonBoundary for Fakes {
    fn owner_nonce(&self, _namespace: &NamespacePair) -> Result<Option<String>, ExchangeFailure> {
        Ok(Some("nonce".to_owned()))
    }

    fn hello(&self, _namespace: &NamespacePair) -> Result<HelloResult, ExchangeFailure> {
        Ok(HelloResult {
            author_target_identity_digest: DIGEST.to_owned(),
            daemon_runtime_contract_digest: Provenance::embedded().daemon_runtime_contract_digest,
            product_version: PRODUCT_VERSION.to_owned(),
            readiness_nonce: "nonce".to_owned(),
            runtime_namespace: NAMESPACE.to_owned(),
            selected_environment_revision: REVISION.to_owned(),
            supported_operation_protocol_versions: vec![SPOKEN_VERSION],
        })
    }

    fn stop(&self, _namespace: &NamespacePair, _readiness_nonce: &str) -> Result<(), ExchangeFailure> {
        Ok(())
    }

    fn operate(
        &self,
        _namespace: &NamespacePair,
        _envelope: &OperationEnvelope,
    ) -> Result<OperationResponse, ExchangeFailure> {
        Ok(OperationResponse::InternalFailure { detail: "not used here".to_owned() })
    }

    fn stream_artifact(
        &self,
        _namespace: &NamespacePair,
        _envelope: &OperationEnvelope,
        take: &mut dyn FnMut(ArtifactEvent) -> Result<(), String>,
    ) -> Result<OperationResponse, ArtifactStreamRefusal> {
        self.reached_daemon.set(self.reached_daemon.get() + 1);
        match &self.answering {
            Answering::NotATransfer => {
                Ok(OperationResponse::InternalFailure { detail: "not a transfer".to_owned() })
            }
            Answering::Whole { bytes, chunk_bytes } => {
                declared(take, bytes.len() as u64, &digest_of(bytes))?;
                for chunk in bytes.chunks(*chunk_bytes) {
                    take(ArtifactEvent::Chunk(chunk.to_vec())).map_err(ArtifactStreamRefusal::AbsorbRefused)?;
                }
                Ok(start_frame())
            }
            Answering::AnotherDigest { bytes, declared: named } => {
                declared(take, bytes.len() as u64, named)?;
                for chunk in bytes.chunks(16) {
                    take(ArtifactEvent::Chunk(chunk.to_vec())).map_err(ArtifactStreamRefusal::AbsorbRefused)?;
                }
                Ok(start_frame())
            }
            Answering::Short { bytes, declared_length } => {
                declared(take, *declared_length, &digest_of(bytes))?;
                for chunk in bytes.chunks(16) {
                    take(ArtifactEvent::Chunk(chunk.to_vec())).map_err(ArtifactStreamRefusal::AbsorbRefused)?;
                }
                Ok(start_frame())
            }
            Answering::Repeated { bytes } => {
                declared(take, bytes.len() as u64, &digest_of(bytes))?;
                for chunk in bytes.chunks(16) {
                    take(ArtifactEvent::Chunk(chunk.to_vec())).map_err(ArtifactStreamRefusal::AbsorbRefused)?;
                }
                for chunk in bytes.chunks(16) {
                    take(ArtifactEvent::Chunk(chunk.to_vec())).map_err(ArtifactStreamRefusal::AbsorbRefused)?;
                }
                Ok(start_frame())
            }
            Answering::EndedEarly { bytes } => {
                declared(take, bytes.len() as u64, &digest_of(bytes))?;
                take(ArtifactEvent::Chunk(bytes.clone())).map_err(ArtifactStreamRefusal::AbsorbRefused)?;
                Err(ArtifactStreamRefusal::EndedEarly)
            }
        }
    }
}

/// Declares the transfer to the caller.
fn declared(
    take: &mut dyn FnMut(ArtifactEvent) -> Result<(), String>,
    byte_length: u64,
    content_digest: &str,
) -> Result<(), ArtifactStreamRefusal> {
    take(ArtifactEvent::Start {
        artifact_identifier: ARTIFACT_IDENTIFIER.to_owned(),
        byte_length,
        content_digest: content_digest.to_owned(),
        media_type: MEDIA_TYPE.to_owned(),
    })
    .map_err(ArtifactStreamRefusal::AbsorbRefused)
}

/// Returns the frame one completed transfer answers with.
fn start_frame() -> OperationResponse {
    OperationResponse::ArtifactStart {
        artifact_identifier: ARTIFACT_IDENTIFIER.to_owned(),
        byte_length: 0,
        content_digest: String::new(),
        media_type: MEDIA_TYPE.to_owned(),
    }
}

/// Returns the invocation one fetch makes.
fn fetching(destination: &Path) -> Invocation {
    Invocation {
        arguments: [
            (OPERATION_IDENTIFIER_OPTION.to_owned(), OPERATION_IDENTIFIER.to_owned()),
            ("--artifact".to_owned(), ARTIFACT_IDENTIFIER.to_owned()),
            (EXPECTED_DIGEST_OPTION.to_owned(), digest_of(&payload())),
            (DESTINATION_OPTION.to_owned(), destination.display().to_string()),
        ]
        .into_iter()
        .collect(),
        detached: false,
        operation_key: None,
        output: None,
        selection: Selection {
            environment: Some(ENVIRONMENT.to_owned()),
            profile: Some(PROFILE.to_owned()),
        },
        verb: "operation-artifact".to_owned(),
    }
}

/// Runs one fetch into a fresh root against a daemon answering `answering`.
///
/// Returns what the run produced and the root it ran in, kept rather than
/// removed so a case can look at what was and was not published there.
fn fetching_with(destination: &str, answering: Answering) -> (Completion, PathBuf) {
    let root = tempfile::tempdir().expect("a temporary root").keep();
    let destination = root.join(destination);
    let completion = fetch_into(&destination, answering);
    (completion, root)
}

/// Runs one fetch of `destination` against a daemon answering `answering`.
fn fetch_into(destination: &Path, answering: Answering) -> Completion {
    let fakes = Fakes::new(answering);
    let application = CommandLineApplication {
        request_identity: &fakes,
        configuration: &fakes,
        daemon: &fakes,
        filesystem: &fakes,
        network: &fakes,
        process: &fakes,
        provenance: Provenance::embedded(),
        signals: &fakes,
    };
    application.run(&fetching(destination))
}

/// Returns whether one completion is the refusal a case expects.
///
/// A refusal is a local diagnostic rather than an envelope: the envelope
/// vocabulary describes what happened to an operation, and a fetch that never
/// published has no outcome to describe.
fn refused_with(completion: &Completion, expected: &str) -> bool {
    matches!(&completion.answer, Answer::Refusal(message) if message.to_lowercase().contains(&expected.to_lowercase()))
}

/// Returns every file staged beside one destination.
fn staged_beside(root: &Path) -> Vec<PathBuf> {
    std::fs::read_dir(root)
        .expect("the root is readable")
        .filter_map(|entry| entry.ok().map(|held| held.path()))
        .filter(|path| {
            path.file_name()
                .and_then(|name| name.to_str())
                .is_some_and(|name| name.contains("slingshot-partial") || name.contains("slingshot-record") || name.contains("slingshot-lock"))
        })
        .collect()
}

#[test]
fn a_whole_transfer_publishes_exactly_the_bytes_the_daemon_declared() {
    let bytes = payload();
    let (completion, root) = fetching_with("fetched.bin", Answering::Whole { bytes: bytes.clone(), chunk_bytes: 16 });
    assert_eq!(completion.exit, 0, "a whole transfer succeeds: {:?}", completion.answer);
    let published = root.join("fetched.bin");
    assert_eq!(std::fs::read(&published).expect("the destination exists"), bytes);
    assert_eq!(staged_beside(&root), Vec::<PathBuf>::new(), "the staging files are removed");
    let Answer::Envelope(envelope) = completion.answer else {
        panic!("a fetch answers an envelope");
    };
    let MachineOutcomeEnvelope::StructuredResultArtifactAccess { artifact } = *envelope else {
        panic!("a fetch answers the artifact access entry");
    };
    assert_eq!(artifact.byte_length, bytes.len() as u64);
    assert_eq!(artifact.content_digest, digest_of(&bytes));
}

#[test]
fn a_transfer_that_digests_to_something_else_leaves_nothing() {
    let bytes = payload();
    let (completion, root) = fetching_with(
        "fetched.bin",
        Answering::AnotherDigest { bytes, declared: "00".repeat(32) },
    );
    assert!(refused_with(&completion, "digest"), "a digest that disagrees is refused: {:?}", completion.answer);
    assert!(!root.join("fetched.bin").exists(), "nothing is published");
    assert_eq!(staged_beside(&root), Vec::<PathBuf>::new(), "the staging files are removed");
}

#[test]
fn a_transfer_that_stops_short_leaves_nothing() {
    let bytes = payload();
    let length = bytes.len() as u64;
    let (completion, root) = fetching_with(
        "fetched.bin",
        Answering::Short { bytes, declared_length: length + 16 },
    );
    assert!(refused_with(&completion, "holds"), "a short transfer is refused: {:?}", completion.answer);
    assert!(!root.join("fetched.bin").exists(), "nothing is published");
    assert_eq!(staged_beside(&root), Vec::<PathBuf>::new(), "the staging files are removed");
}

#[test]
fn more_bytes_than_the_daemon_declared_are_refused_before_they_are_published() {
    let bytes = payload();
    let (completion, root) = fetching_with("fetched.bin", Answering::Repeated { bytes });
    assert!(refused_with(&completion, "arrived"), "a transfer longer than it declared is refused: {:?}", completion.answer);
    assert!(!root.join("fetched.bin").exists(), "nothing is published");
    assert_eq!(staged_beside(&root), Vec::<PathBuf>::new(), "the staging files are removed");
}

#[test]
fn a_connection_that_ends_before_the_transfer_does_leaves_nothing() {
    let bytes = payload();
    let (completion, root) = fetching_with("fetched.bin", Answering::EndedEarly { bytes });
    assert!(refused_with(&completion, "before it said"), "a transfer that ended early is refused: {:?}", completion.answer);
    assert!(!root.join("fetched.bin").exists(), "nothing is published");
    assert_eq!(staged_beside(&root), Vec::<PathBuf>::new(), "the staging files are removed");
}

#[test]
fn an_answer_that_is_not_a_transfer_is_rendered_as_that_answer() {
    let (completion, root) = fetching_with("fetched.bin", Answering::NotATransfer);
    // The daemon's own answer is the answer: a read that was not a transfer
    // still answered the question it was asked.
    assert!(matches!(completion.answer, Answer::Refusal(_)), "an internal failure is a refusal: {:?}", completion.answer);
    assert!(!root.join("fetched.bin").exists(), "nothing is published");
}

#[test]
fn a_destination_that_already_exists_is_never_replaced() {
    let root = tempfile::tempdir().expect("a temporary root").keep();
    let destination = root.join("fetched.bin");
    std::fs::write(&destination, b"somebody else's bytes").unwrap();
    let completion = fetch_into(&destination, Answering::Whole { bytes: payload(), chunk_bytes: 16 });
    assert!(refused_with(&completion, "already exists"), "an occupied destination is refused: {:?}", completion.answer);
    assert_eq!(std::fs::read(&destination).unwrap(), b"somebody else's bytes");
}
