//! Every local-protocol seed, replayed through the production decoder.
//!
//! The decoder stands between a socket anybody on this machine can connect to
//! and a daemon that owns durable state. What it must never do is act on a
//! frame it did not fully understand, and what it must never do afterwards is
//! quote that frame back in a refusal - a daemon that echoed what it was sent
//! would be a way to write into somebody's log.

use std::path::PathBuf;

use slingshot_local_protocol::envelope;
use slingshot_local_protocol::foundation_contract::FoundationContract;

/// Where the seeds live.
const CORPUS: &str = "../../fuzz/corpus/local_protocol_frame";

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

#[test]
fn the_corpus_holds_enough_seeds_to_be_worth_starting_from() {
    assert!(seeds().len() >= LEAST_SEEDS);
}

#[test]
fn every_seed_is_decoded_or_refused_with_a_code_and_never_anything_else() {
    let contract = FoundationContract::embedded();
    for (name, bytes) in seeds() {
        match envelope::decode_request(&contract, &bytes) {
            Ok(request) => {
                assert!(!request.method.is_empty(), "{name} decoded to a request with no method");
            }
            Err(refused) => {
                assert!(!refused.error.code.is_empty(), "{name} was refused with no code");
                assert!(!refused.error.message.is_empty(), "{name} was refused with no message");
            }
        }
    }
}

#[test]
fn no_refusal_quotes_the_frame_it_refused() {
    let contract = FoundationContract::embedded();
    for (name, bytes) in seeds() {
        let Err(refused) = envelope::decode_request(&contract, &bytes) else {
            continue;
        };
        assert!(
            !refused.error.message.contains(SECRET_SENTINEL),
            "{name} carried a secret out of the frame it refused"
        );
        let held = String::from_utf8_lossy(&bytes);
        if held.len() > MOST_QUOTED_BYTES {
            assert!(
                !refused.error.message.contains(&held[..MOST_QUOTED_BYTES]),
                "{name} quoted the frame back"
            );
        }
    }
}

/// How much of a frame is enough to say it was quoted.
const MOST_QUOTED_BYTES: usize = 32;

#[test]
fn decoding_one_seed_twice_answers_the_same_way_twice() {
    let contract = FoundationContract::embedded();
    for (name, bytes) in seeds() {
        let first = envelope::decode_request(&contract, &bytes).is_ok();
        let again = envelope::decode_request(&contract, &bytes).is_ok();
        assert_eq!(first, again, "{name} answered differently the second time");
    }
}

#[test]
fn the_target_that_consumes_this_corpus_exists_and_drives_the_decoder() {
    let target = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../fuzz/fuzz_targets/local_protocol_frame.rs");
    let held = std::fs::read_to_string(&target).expect("the target is committed");
    assert!(held.contains("decode_request"), "the target drives the production decoder");
}
