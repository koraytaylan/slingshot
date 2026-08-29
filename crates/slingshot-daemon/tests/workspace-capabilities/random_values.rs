//! Probe for the random-values capability.
//!
//! Requires a generator the type system marks as cryptographically strong, an
//! exact-length buffer fill, and two independent draws that differ, because the
//! readiness sample is the only stop authority a daemon has.

use rand::{CryptoRng, RngExt};

/// Length of the buffer this probe draws, chosen so it names no runtime limit.
const SAMPLE_LENGTH: usize = 48;

/// Accepts only a generator that is marked cryptographically strong.
fn draw_sample(generator: &mut (impl RngExt + CryptoRng)) -> [u8; SAMPLE_LENGTH] {
    let mut sample = [0_u8; SAMPLE_LENGTH];
    generator.fill(&mut sample[..]);
    sample
}

#[test]
fn a_cryptographically_strong_generator_fills_an_exact_length_buffer() {
    let mut generator = rand::rng();
    let first = draw_sample(&mut generator);
    let second = draw_sample(&mut generator);
    assert_eq!(first.len(), SAMPLE_LENGTH);
    assert_ne!(first, second, "two draws must differ");
    assert!(first.iter().any(|byte| *byte != 0), "the draw is not all zero");
}
