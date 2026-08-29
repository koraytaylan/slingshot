//! Probe for the property-tests capability.
//!
//! Requires randomized input generation, a failing property that shrinks to a
//! minimal counterexample, and a deterministic replay configuration.

use proptest::prelude::*;
use proptest::test_runner::{Config, TestError, TestRunner};

proptest! {
    #[test]
    fn a_generated_pair_keeps_its_ordering(left in 0_u32..1_000, right in 0_u32..1_000) {
        let (smaller, larger) = if left <= right { (left, right) } else { (right, left) };
        prop_assert!(smaller <= larger);
    }
}

#[test]
fn a_failing_property_shrinks_to_its_minimal_counterexample() {
    let mut runner = TestRunner::new(Config { cases: 256, ..Config::default() });
    let outcome = runner.run(&(0_u32..1_000), |value| {
        prop_assert!(value < 8);
        Ok(())
    });
    match outcome {
        Err(TestError::Fail(_, counterexample)) => assert_eq!(counterexample, 8),
        other => panic!("the property must fail and shrink, but produced {other:?}"),
    }
}
