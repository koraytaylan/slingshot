//! Probe for the random-values capability.
//!
//! Requires a generator the type system marks as cryptographically strong, an
//! exact-length buffer fill, and two independent draws that differ, because the
//! readiness nonce is the only stop authority a daemon has.

use rand::{CryptoRng, RngExt};

/// Length of the readiness nonce the runtime contract fixes.
const NONCE_LENGTH: usize = 32;

/// Accepts only a generator that is marked cryptographically strong.
fn draw_nonce(generator: &mut (impl RngExt + CryptoRng)) -> [u8; NONCE_LENGTH] {
    let mut nonce = [0_u8; NONCE_LENGTH];
    generator.fill(&mut nonce[..]);
    nonce
}

#[test]
fn a_cryptographically_strong_generator_fills_an_exact_length_buffer() {
    let mut generator = rand::rng();
    let first = draw_nonce(&mut generator);
    let second = draw_nonce(&mut generator);
    assert_eq!(first.len(), NONCE_LENGTH);
    assert_ne!(first, second, "two draws must differ");
    assert!(first.iter().any(|byte| *byte != 0), "the draw is not all zero");
}
