//! Every event-stream seed, replayed through the production decoder.
//!
//! A stream arrives in whatever pieces the transport chose, so the decoder's
//! answer must not depend on how the bytes were split. Each seed is therefore
//! pushed twice: once whole, and once one byte at a time. The two must agree,
//! because a decoder whose answer depends on chunking is a decoder that behaves
//! differently on a slow network.

use std::path::PathBuf;

use slingshot_agent_connection::author_hypertext_transfer_protocol_policy::ResponseHead;
use slingshot_agent_connection::server_sent_event_decoder::{
    DecoderBounds, ServerSentEventDecoder, StreamExpectation,
};
use slingshot_agent_protocol::wire_contract::ExpectedProvenance;
use slingshot_domain::author_agent_transport_contract::AuthorAgentTransportContract;
use slingshot_domain::command::schema::canonical_contract_digest;
use slingshot_domain::selected_command_contract_identity::SelectedCommandContractIdentity;

/// Where the seeds live.
const CORPUS: &str = "../../fuzz/corpus/agent_protocol_server_sent_event";

/// The command every stream in these seeds is about.
const COMMAND: &str = "load_content_as_json";

/// The protocol every seed arrives over.
const SPOKEN_VERSION: &str = "HTTP/1.1";

/// What an event stream is served as.
const EVENT_STREAM: &str = "text/event-stream";

/// The generation every seed names.
const GENERATION: u64 = 1;

/// The fewest seeds a corpus worth keeping holds.
const LEAST_SEEDS: usize = 12;

/// A value no refusal may carry back out.
const SECRET_SENTINEL: &str = "s3ntinel-bearer-token-71bd0e";

/// Returns every seed, by name and bytes.
fn seeds() -> Vec<(String, Vec<u8>)> {
    let directory = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(CORPUS);
    let mut held: Vec<(String, Vec<u8>)> = std::fs::read_dir(&directory)
        .unwrap_or_else(|failure| panic!("{} could not be read: {failure}", directory.display()))
        .filter_map(Result::ok)
        .map(|entry| {
            let name = entry.file_name().to_string_lossy().into_owned();
            (name, std::fs::read(entry.path()).expect("the seed reads"))
        })
        .collect();
    held.sort();
    held
}

/// Returns a decoder attached to an acceptable event-stream response.
fn attached() -> ServerSentEventDecoder {
    let head = ResponseHead {
        alternative_service_offered: false,
        content_coding: None,
        informational: false,
        location: None,
        protocol_version: SPOKEN_VERSION.to_owned(),
        trailers_declared: false,
    };
    let expectation = StreamExpectation {
        agent_event_store_generation: GENERATION,
        daemon_subscription_identifier: "subscription".to_owned(),
        expected_provenance: ExpectedProvenance {
            canonical_json_contract_digest: canonical_contract_digest(),
            command_contract: SelectedCommandContractIdentity::installed(COMMAND)
                .expect("the command is published"),
            transport_contract_digest: AuthorAgentTransportContract::embedded_digest(),
        },
        submitted_command_digest: "0".repeat(DIGEST_CHARACTERS),
    };
    ServerSentEventDecoder::attached(&head, EVENT_STREAM, DecoderBounds::embedded(), expectation)
        .expect("this response is one a stream may be decoded from")
}

/// How many characters a digest is written in.
const DIGEST_CHARACTERS: usize = 64;

#[test]
fn the_corpus_holds_enough_seeds_to_be_worth_starting_from() {
    assert!(seeds().len() >= LEAST_SEEDS);
}

#[test]
fn every_seed_is_decoded_or_refused_and_never_anything_else() {
    for (name, bytes) in seeds() {
        let mut decoder = attached();
        match decoder.push(&bytes) {
            Ok(items) => {
                for item in &items {
                    assert!(!format!("{item:?}").is_empty(), "{name} produced an unreadable item");
                }
            }
            Err(refusal) => {
                assert!(
                    !format!("{refusal}").is_empty(),
                    "{name} was refused for no stated reason"
                );
            }
        }
    }
}

#[test]
fn how_the_bytes_were_split_does_not_change_the_answer() {
    for (name, bytes) in seeds() {
        let mut whole = attached();
        let at_once = whole.push(&bytes).map(|items| items.len()).map_err(|held| held.to_string());

        let mut byte_at_a_time = attached();
        let mut counted = 0;
        let mut refused = None;
        for byte in &bytes {
            match byte_at_a_time.push(&[*byte]) {
                Ok(items) => counted += items.len(),
                Err(held) => {
                    refused = Some(held.to_string());
                    break;
                }
            }
        }
        let piecemeal = refused.map_or(Ok(counted), Err);
        assert_eq!(at_once, piecemeal, "{name} depends on how the transport split it");
    }
}

#[test]
fn no_refusal_carries_the_stream_it_refused_back_out() {
    for (name, bytes) in seeds() {
        let mut decoder = attached();
        let Err(refusal) = decoder.push(&bytes) else {
            continue;
        };
        assert!(
            !refusal.to_string().contains(SECRET_SENTINEL),
            "{name} carried a secret out of the stream it refused"
        );
    }
}

#[test]
fn the_target_that_consumes_this_corpus_exists_and_drives_the_decoder() {
    let target = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../fuzz/fuzz_targets/agent_protocol_server_sent_event.rs");
    let held = std::fs::read_to_string(&target).expect("the target is committed");
    assert!(held.contains("ServerSentEventDecoder::attached"), "the target drives the decoder");
    assert!(held.contains("decoder.push"), "and feeds it what the fuzzer produced");
}
