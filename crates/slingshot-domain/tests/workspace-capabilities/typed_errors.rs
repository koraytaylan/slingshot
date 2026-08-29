//! Probe for the typed-errors capability.
//!
//! Requires derived error enumerations that render their message, carry a
//! source, and implement the standard error trait.

use std::error::Error;

#[derive(Debug, thiserror::Error)]
enum InnerFailure {
    #[error("the value {value} is outside the bound {bound}")]
    OutsideBound { value: u32, bound: u32 },
}

#[derive(Debug, thiserror::Error)]
enum OuterFailure {
    #[error("the operation could not start")]
    CouldNotStart(#[source] InnerFailure),
}

#[test]
fn a_derived_error_renders_its_message_and_exposes_its_source() {
    let inner = InnerFailure::OutsideBound { value: 11, bound: 10 };
    assert_eq!(inner.to_string(), "the value 11 is outside the bound 10");
    let outer = OuterFailure::CouldNotStart(inner);
    assert_eq!(outer.to_string(), "the operation could not start");
    let source = outer.source().expect("the outer failure exposes its source");
    assert_eq!(source.to_string(), "the value 11 is outside the bound 10");
}
