//! Probe for the property-tests capability.
//!
//! Requires randomized input generation, a failing property that shrinks to a
//! minimal counterexample, and a deterministic replay configuration.

use proptest::prelude::*;
use proptest::test_runner::{Config, TestError, TestRunner};

/// Cases the runner draws before it gives up on finding a counterexample.
const SHRINKING_CASES: u32 = 256;

/// Bound of the values the probe generates.
const GENERATED_BOUND: u32 = 1_000;

/// Value the failing property accepts up to.
const ACCEPTED_BOUND: u32 = 8;

proptest! {
    #[test]
    fn a_generated_pair_keeps_its_ordering(left in 0_u32..GENERATED_BOUND, right in 0_u32..GENERATED_BOUND) {
        let (smaller, larger) = if left <= right { (left, right) } else { (right, left) };
        prop_assert!(smaller <= larger);
    }
}

#[test]
fn a_failing_property_shrinks_to_its_minimal_counterexample() {
    let mut runner = TestRunner::new(Config { cases: SHRINKING_CASES, ..Config::default() });
    let outcome = runner.run(&(0_u32..GENERATED_BOUND), |value| {
        prop_assert!(value < ACCEPTED_BOUND);
        Ok(())
    });
    match outcome {
        Err(TestError::Fail(_, counterexample)) => assert_eq!(counterexample, ACCEPTED_BOUND),
        other => panic!("the property must fail and shrink, but produced {other:?}"),
    }
}
