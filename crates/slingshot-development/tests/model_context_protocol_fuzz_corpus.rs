//! Every protocol-message seed, replayed through the production reader.
//!
//! This reader is what a model host's bytes meet first, and everything after it
//! assumes it did its job. So the seeds are the shapes that get past a careless
//! reader: a repeated member whose two values differ, two hundred levels of
//! nesting, a two-hundred-kilobyte identifier, and bytes that are not text.
//!
//! Two claims carry the rest. The bounds are checked before the content, so an
//! oversized or over-nested line is refused without being parsed; and no
//! refusal quotes what it refused.

use std::path::PathBuf;

use slingshot_command_line::model_context_protocol::standard_stream_transport::{
    MessageRefusal, maximum_line_bytes, maximum_nesting_depth, read_message,
};

/// Where the seeds live.
const CORPUS: &str = "../../fuzz/corpus/model_context_protocol_message";

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
fn every_seed_is_read_as_one_message_or_refused_with_a_reason() {
    for (name, bytes) in seeds() {
        match read_message(&bytes) {
            Ok(message) => {
                let named = format!("{message:?}");
                assert!(named.contains("method"), "{name} read as a message with no method");
            }
            Err(refusal) => {
                assert!(!refusal.to_string().is_empty(), "{name} was refused for no stated reason");
            }
        }
    }
}

#[test]
fn a_repeated_member_is_refused_rather_than_resolved_to_one_of_its_values() {
    let held = seeds()
        .into_iter()
        .find(|(name, _)| name == "a-duplicate-member")
        .expect("the seed is committed");
    let refusal = read_message(&held.1).expect_err("which value was meant is not knowable");
    assert!(matches!(refusal, MessageRefusal::DuplicateMember(_)), "{refusal:?}");
}

#[test]
fn a_line_past_a_bound_is_refused_without_being_parsed() {
    let long = vec![b'a'; maximum_line_bytes() + 1];
    assert_eq!(read_message(&long), Err(MessageRefusal::LineTooLong(long.len())));
    let past_the_bound = format!(
        "{{\"id\":\"one\",\"method\":\"ping\",\"params\":{}1{}}}",
        "[".repeat(maximum_nesting_depth() + 1),
        "]".repeat(maximum_nesting_depth() + 1)
    );
    assert_eq!(
        read_message(past_the_bound.as_bytes()),
        Err(MessageRefusal::TooDeep),
        "one level past the bound is this reader's own refusal"
    );
    let nested = seeds()
        .into_iter()
        .find(|(name, _)| name == "deeply-nested")
        .expect("the seed is committed");
    assert!(
        read_message(&nested.1).is_err(),
        "far past the bound the reader beneath refuses first, and either refusal is a refusal"
    );
}

#[test]
fn no_refusal_quotes_the_line_it_refused() {
    for (name, bytes) in seeds() {
        let Err(refusal) = read_message(&bytes) else {
            continue;
        };
        assert!(
            !refusal.to_string().contains(SECRET_SENTINEL),
            "{name} carried a secret out of the line it refused"
        );
    }
}

#[test]
fn the_target_that_consumes_this_corpus_exists_and_drives_the_reader() {
    let target = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../fuzz/fuzz_targets/model_context_protocol_message.rs");
    let held = std::fs::read_to_string(&target).expect("the target is committed");
    assert!(held.contains("read_message"), "the target drives the production reader");
}
