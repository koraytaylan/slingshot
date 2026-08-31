//! Feeding arbitrary bytes to the protocol server's message reader.
//!
//! The reader is what a model host's bytes meet first, and everything after it
//! assumes it did its job: bounds checked before parsing, one message or one
//! refusal, and nothing of the input carried into the answer.

#![no_main]

use libfuzzer_sys::fuzz_target;
use slingshot_command_line::model_context_protocol::standard_stream_transport::read_message;

fuzz_target!(|bytes: &[u8]| {
    if let Err(refusal) = read_message(bytes) {
        assert!(!refusal.to_string().is_empty());
    }
});
