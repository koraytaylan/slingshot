//! Probe for the secure-hash-digest capability.
//!
//! Requires a fixed-length digest over incremental input that equals the digest
//! over the same bytes supplied at once, because namespaces and artifact
//! checksums are computed while streaming.

use base64::Engine as _;
use sha2::{Digest, Sha256};

/// Length in bytes of the digest the runtime contract fixes.
const DIGEST_LENGTH: usize = 32;

#[test]
fn an_incremental_digest_equals_the_digest_of_the_whole_input() {
    let whole = Sha256::digest(b"slingshot artifact bytes");
    assert_eq!(whole.len(), DIGEST_LENGTH);

    let mut incremental = Sha256::new();
    incremental.update(b"slingshot ");
    incremental.update(b"artifact ");
    incremental.update(b"bytes");
    assert_eq!(incremental.finalize().as_slice(), whole.as_slice());

    let rendered = base64::engine::general_purpose::STANDARD.encode(whole);
    assert_eq!(
        base64::Engine::decode(&base64::engine::general_purpose::STANDARD, &rendered)
            .expect("the rendered digest decodes"),
        whole.as_slice()
    );
    assert_ne!(Sha256::digest(b"slingshot artifact byte").as_slice(), whole.as_slice());
}
