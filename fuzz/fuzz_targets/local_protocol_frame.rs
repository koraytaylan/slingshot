//! Feeding arbitrary bytes to the local control decoder.
//!
//! The decoder stands between a socket anybody on this machine can connect to
//! and a daemon that owns durable state, so what it must never do is act on a
//! frame it did not fully understand. Every input here produces a decoded
//! request or a refusal, and the refusal carries a code rather than anything
//! from the bytes it refused.

#![no_main]

use libfuzzer_sys::fuzz_target;
use slingshot_local_protocol::envelope;
use slingshot_local_protocol::foundation_contract::FoundationContract;

fuzz_target!(|bytes: &[u8]| {
    let contract = FoundationContract::embedded();
    if let Err(refused) = envelope::decode_request(&contract, bytes) {
        assert!(!refused.error.code.is_empty());
    }
});
