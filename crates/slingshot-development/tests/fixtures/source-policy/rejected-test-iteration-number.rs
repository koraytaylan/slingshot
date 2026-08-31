//! An iteration count nobody named.

/// Runs the probe until it settles.
pub fn probe_until_settled() {
    for _ in 0..64 {
        std::hint::black_box(());
    }
}
