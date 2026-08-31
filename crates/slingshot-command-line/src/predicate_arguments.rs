//! Turning command-line predicate arguments into typed questions.
//!
//! The module map assigns this leaf the conversion from repeated flag syntax
//! into the closed predicate vocabulary the catalog accepts. A predicate a
//! caller could express but the catalog could not is refused here, where the
//! caller can still see what they typed.
